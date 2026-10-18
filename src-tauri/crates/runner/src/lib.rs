use serde::{Deserialize, Serialize};
use std::env;
use std::process::Stdio;
use std::time::Instant;
use thiserror::Error;
use tokio::fs;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

const MAX_OUTPUT_BYTES: usize = 1024 * 1024; // 1 MB

#[derive(Error, Debug)]
pub enum RunnerError {
    #[error("Execution timed out ({0}s)")]
    Timeout(u64),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Unsupported runner language: {0}")]
    Unsupported(String),
    #[error("Interpreter not found: {0}")]
    InterpreterNotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRequest {
    pub language: String, // "javascript" | "typescript" | "python" | "rust" | "shell"
    pub code: String,
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResponse {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub success: bool,
    #[serde(default)]
    pub truncated: bool,
}

pub struct CodeRunner;

impl CodeRunner {
    /// Probes local Python interpreter in order: `python3` -> `python` -> `py -3`
    pub async fn probe_python_command() -> Result<(String, Vec<String>), RunnerError> {
        let candidates: Vec<(&str, Vec<&str>)> = vec![
            ("python3", vec![]),
            ("python", vec![]),
            ("py", vec!["-3"]),
        ];

        for (bin, args) in candidates {
            let mut cmd = Command::new(bin);
            for a in &args {
                cmd.arg(a);
            }
            cmd.arg("--version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true);

            if let Ok(Ok(status)) = timeout(Duration::from_secs(3), cmd.status()).await {
                if status.success() {
                    return Ok((
                        bin.to_string(),
                        args.into_iter().map(|s| s.to_string()).collect(),
                    ));
                }
            }
        }

        Err(RunnerError::InterpreterNotFound(
            "未检测到本地 Python 解释器 (已尝试 python3, python, py -3)".to_string(),
        ))
    }

    pub fn truncate_output(bytes: &[u8], max_bytes: usize) -> (String, bool) {
        if bytes.len() <= max_bytes {
            return (String::from_utf8_lossy(bytes).to_string(), false);
        }
        let slice = &bytes[..max_bytes];
        let mut s = String::from_utf8_lossy(slice).to_string();
        // Remove replacement character at tail if we sliced mid-codepoint
        if s.ends_with('\u{FFFD}') {
            s.pop();
        }
        s.push_str("\n...[输出超过 1MB，已截断]");
        (s, true)
    }

    pub async fn execute(req: RunRequest) -> Result<RunResponse, RunnerError> {
        let timeout_secs = req.timeout_seconds.unwrap_or(30);
        let start = Instant::now();

        let temp_dir = env::temp_dir().join("noesis_runner");
        fs::create_dir_all(&temp_dir).await?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let (ext, cmd_name, base_args): (&str, String, Vec<String>) =
            match req.language.to_lowercase().as_str() {
                "javascript" | "js" => ("js", "node".to_string(), vec![]),
                "python" | "py" => {
                    let (py_bin, py_args) = Self::probe_python_command().await?;
                    ("py", py_bin, py_args)
                }
                "typescript" | "ts" => ("ts", "npx".to_string(), vec!["ts-node".to_string()]),
                _ => {
                    return Err(RunnerError::Unsupported(req.language));
                }
            };

        let script_file = temp_dir.join(format!("scratch_{}.{}", timestamp, ext));
        fs::write(&script_file, req.code).await?;

        let mut cmd = Command::new(&cmd_name);
        for a in base_args {
            cmd.arg(a);
        }
        cmd.arg(&script_file)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let exec_future = cmd.output();
        let res = match timeout(Duration::from_secs(timeout_secs), exec_future).await {
            Ok(output_res) => output_res?,
            Err(_) => {
                // `exec_future` is dropped here, triggering `kill_on_drop(true)` on the child process
                let _ = fs::remove_file(&script_file).await;
                return Err(RunnerError::Timeout(timeout_secs));
            }
        };

        let _ = fs::remove_file(&script_file).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        let (stdout, out_trunc) = Self::truncate_output(&res.stdout, MAX_OUTPUT_BYTES);
        let (stderr, err_trunc) = Self::truncate_output(&res.stderr, MAX_OUTPUT_BYTES);

        Ok(RunResponse {
            stdout,
            stderr,
            exit_code: res.status.code().unwrap_or(-1),
            success: res.status.success(),
            duration_ms,
            truncated: out_trunc || err_trunc,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_output_over_1mb() {
        let large = vec![b'A'; MAX_OUTPUT_BYTES + 5000];
        let (text, truncated) = CodeRunner::truncate_output(&large, MAX_OUTPUT_BYTES);
        assert!(truncated);
        assert!(text.ends_with("...[输出超过 1MB，已截断]"));
        assert!(text.len() < MAX_OUTPUT_BYTES + 100);
    }

    #[tokio::test]
    async fn test_runner_timeout_kills_process() {
        if CodeRunner::probe_python_command().await.is_ok() {
            let req = RunRequest {
                language: "python".to_string(),
                code: "import time\ntime.sleep(10)\nprint('done')".to_string(),
                timeout_seconds: Some(1),
            };
            let res = CodeRunner::execute(req).await;
            assert!(matches!(res, Err(RunnerError::Timeout(1))));
        }
    }
}
