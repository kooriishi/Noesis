use serde::{Deserialize, Serialize};
use std::env;
use std::time::Instant;
use thiserror::Error;
use tokio::fs;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

#[derive(Error, Debug)]
pub enum RunnerError {
    #[error("Execution timed out ({0}s)")]
    Timeout(u64),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Unsupported runner language: {0}")]
    Unsupported(String),
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
}

pub struct CodeRunner;

impl CodeRunner {
    pub async fn execute(req: RunRequest) -> Result<RunResponse, RunnerError> {
        let timeout_secs = req.timeout_seconds.unwrap_or(10);
        let start = Instant::now();

        let temp_dir = env::temp_dir().join("noesis_runner");
        fs::create_dir_all(&temp_dir).await?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();

        let (ext, cmd_name, base_args): (&str, &str, Vec<String>) = match req.language.to_lowercase().as_str() {
            "javascript" | "js" => ("js", "node", vec![]),
            "python" | "py" => ("py", "python", vec![]),
            "typescript" | "ts" => ("ts", "npx", vec!["ts-node".to_string()]),
            _ => {
                return Err(RunnerError::Unsupported(req.language));
            }
        };

        let script_file = temp_dir.join(format!("scratch_{}.{}", timestamp, ext));
        fs::write(&script_file, req.code).await?;

        let mut cmd = Command::new(cmd_name);
        for a in base_args {
            cmd.arg(a);
        }
        cmd.arg(&script_file);

        let exec_future = cmd.output();
        let res = match timeout(Duration::from_secs(timeout_secs), exec_future).await {
            Ok(output_res) => output_res?,
            Err(_) => {
                let _ = fs::remove_file(&script_file).await;
                return Err(RunnerError::Timeout(timeout_secs));
            }
        };

        let _ = fs::remove_file(&script_file).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(RunResponse {
            stdout: String::from_utf8_lossy(&res.stdout).to_string(),
            stderr: String::from_utf8_lossy(&res.stderr).to_string(),
            exit_code: res.status.code().unwrap_or(-1),
            success: res.status.success(),
            duration_ms,
        })
    }
}
