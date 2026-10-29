use serde::{Deserialize, Serialize};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Instant;
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

const MAX_OUTPUT_BYTES: usize = 1024 * 1024; // 1 MB
const COMPILE_TIMEOUT_SECONDS: u64 = 60;

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
    #[error("Compiler not found: {0}")]
    CompilerNotFound(String),
    #[error("Linux code execution requires bubblewrap (bwrap); refusing to run without filesystem isolation")]
    SandboxUnavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRequest {
    pub language: String, // "javascript" | "typescript" | "python" | "cpp" | "c"
    pub code: String,
    pub timeout_seconds: Option<u64>,
    pub stdin: Option<String>,
    pub cpp_standard: Option<String>, // "c++14" | "c++17" | "c++20" | "c11"
    pub extra_args: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RunResponse {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub success: bool,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub phase: Option<String>, // "compile" | "run"
    #[serde(default)]
    pub compile_stdout: Option<String>,
    #[serde(default)]
    pub compile_stderr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilerInfo {
    pub name: String,
    pub path: String,
    pub version: String,
}

#[derive(Debug, Clone)]
pub enum CompilerKind {
    GccLike { bin: String },
    ClDirect { bin: String },
    MsvcVcvars { vcvars_bat: PathBuf },
}

#[derive(Debug, Clone)]
pub struct CompilerConfig {
    pub info: CompilerInfo,
    pub kind: CompilerKind,
}

static DETECTED_CPP_COMPILER: OnceLock<Option<CompilerConfig>> = OnceLock::new();
static DETECTED_C_COMPILER: OnceLock<Option<CompilerConfig>> = OnceLock::new();

pub struct CodeRunner;

#[cfg(target_os = "linux")]
const SANDBOX_SESSION_DIR: &str = "/run/noesis_runner/session";

fn runtime_session_dir(host_session_dir: &Path) -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        let _ = host_session_dir;
        PathBuf::from(SANDBOX_SESSION_DIR)
    }
    #[cfg(not(target_os = "linux"))]
    {
        host_session_dir.to_path_buf()
    }
}

fn command_for_session(program: &str, host_session_dir: &Path) -> Result<Command, RunnerError> {
    #[cfg(target_os = "linux")]
    {
        let has_bwrap = env::var_os("PATH")
            .into_iter()
            .flat_map(|path| env::split_paths(&path).collect::<Vec<_>>())
            .any(|dir| dir.join("bwrap").is_file());
        if !has_bwrap {
            return Err(RunnerError::SandboxUnavailable);
        }

        let mut cmd = Command::new("bwrap");
        cmd.args(["--unshare-all", "--die-with-parent", "--new-session", "--tmpfs", "/"]);
        for runtime_dir in ["/usr", "/bin", "/sbin", "/lib", "/lib64"] {
            if Path::new(runtime_dir).exists() {
                cmd.arg("--dir")
                    .arg(runtime_dir)
                    .arg("--ro-bind")
                    .arg(runtime_dir)
                    .arg(runtime_dir);
            }
        }
        cmd.args(["--tmpfs", "/tmp", "--tmpfs", "/run", "--dir", "/run/noesis_runner"])
            .arg("--bind")
            .arg(host_session_dir)
            .arg(SANDBOX_SESSION_DIR);
        cmd.args(["--proc", "/proc", "--dev", "/dev"])
            .args(["--chdir", SANDBOX_SESSION_DIR, "--clearenv", "--setenv", "HOME", "/tmp"])
            .args(["--setenv", "TMPDIR", "/tmp", "--setenv", "PATH", "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin", "--", program])
            .kill_on_drop(true);
        Ok(cmd)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let mut cmd = Command::new(program);
        cmd.current_dir(host_session_dir).kill_on_drop(true);
        Ok(cmd)
    }
}

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

    /// Probes local C/C++ compiler and caches result in process
    pub async fn probe_compiler(is_cpp: bool) -> Result<CompilerConfig, RunnerError> {
        let cache = if is_cpp {
            &DETECTED_CPP_COMPILER
        } else {
            &DETECTED_C_COMPILER
        };

        if let Some(cached) = cache.get() {
            return cached.clone().ok_or_else(|| {
                RunnerError::CompilerNotFound(
                    "未检测到本地编译器。建议安装 MSYS2 (gcc/g++) 或 Visual Studio Build Tools。".to_string()
                )
            });
        }

        let detected = Self::do_probe_compiler(is_cpp).await;
        let _ = cache.set(detected.as_ref().ok().cloned());
        detected
    }

    async fn do_probe_compiler(is_cpp: bool) -> Result<CompilerConfig, RunnerError> {
        // Priority list of compiler commands
        let candidates: Vec<&str> = if cfg!(windows) {
            if is_cpp {
                vec!["g++", "clang++", "cl.exe", "cl"]
            } else {
                vec!["gcc", "clang", "cl.exe", "cl"]
            }
        } else {
            if is_cpp {
                vec!["g++", "clang++"]
            } else {
                vec!["gcc", "clang"]
            }
        };

        for bin in &candidates {
            if *bin == "cl.exe" || *bin == "cl" {
                let mut cmd = Command::new(bin);
                cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
                if let Ok(Ok(out)) = timeout(Duration::from_secs(3), cmd.output()).await {
                    let combined = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
                    if combined.contains("Microsoft (R) C/C++") || combined.contains("Optimizing Compiler") {
                        let version = combined.lines().next().unwrap_or("MSVC cl").trim().to_string();
                        return Ok(CompilerConfig {
                            info: CompilerInfo {
                                name: "MSVC cl".to_string(),
                                path: bin.to_string(),
                                version,
                            },
                            kind: CompilerKind::ClDirect { bin: bin.to_string() },
                        });
                    }
                }
            } else {
                let mut cmd = Command::new(bin);
                cmd.arg("--version")
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .kill_on_drop(true);

                if let Ok(Ok(out)) = timeout(Duration::from_secs(3), cmd.output()).await {
                    if out.status.success() {
                        let text = String::from_utf8_lossy(&out.stdout);
                        let first_line = text.lines().next().unwrap_or(bin).trim().to_string();
                        return Ok(CompilerConfig {
                            info: CompilerInfo {
                                name: bin.to_string(),
                                path: bin.to_string(),
                                version: first_line,
                            },
                            kind: CompilerKind::GccLike { bin: bin.to_string() },
                        });
                    }
                }
            }
        }

        // On Windows, if cl is not directly in PATH, check vswhere / vcvars64.bat
        if cfg!(windows) {
            if let Some(vcvars) = Self::find_vcvars64_bat().await {
                // Test if vcvars64.bat can run cl
                let test_cmd = format!("call \"{}\" >nul 2>&1 && cl", vcvars.to_string_lossy());
                let mut cmd = Command::new("cmd.exe");
                cmd.args(["/c", &test_cmd])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .kill_on_drop(true);

                if let Ok(Ok(out)) = timeout(Duration::from_secs(10), cmd.output()).await {
                    let combined = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
                    if combined.contains("Microsoft (R) C/C++") || combined.contains("Optimizing Compiler") {
                        let version = combined.lines().next().unwrap_or("MSVC cl (via vcvars64)").trim().to_string();
                        return Ok(CompilerConfig {
                            info: CompilerInfo {
                                name: "MSVC cl (vcvars64)".to_string(),
                                path: vcvars.to_string_lossy().to_string(),
                                version,
                            },
                            kind: CompilerKind::MsvcVcvars { vcvars_bat: vcvars },
                        });
                    }
                }
            }
        }

        Err(RunnerError::CompilerNotFound(
            "未检测到 C/C++ 编译器 (已尝试 g++, clang++, MSVC cl.exe)。建议安装 MSYS2 (gcc/g++) 或 Visual Studio Build Tools。".to_string()
        ))
    }

    async fn find_vcvars64_bat() -> Option<PathBuf> {
        let prog_files_x86 = env::var("ProgramFiles(x86)").unwrap_or_else(|_| r"C:\Program Files (x86)".to_string());
        let vswhere_path = PathBuf::from(&prog_files_x86)
            .join("Microsoft Visual Studio")
            .join("Installer")
            .join("vswhere.exe");

        let vswhere_bin = if vswhere_path.exists() {
            vswhere_path.to_string_lossy().to_string()
        } else {
            "vswhere.exe".to_string()
        };

        let mut cmd = Command::new(&vswhere_bin);
        cmd.args(["-latest", "-property", "installationPath"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        if let Ok(Ok(out)) = timeout(Duration::from_secs(5), cmd.output()).await {
            if out.status.success() {
                let install_dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !install_dir.is_empty() {
                    let bat = PathBuf::from(install_dir)
                        .join("VC")
                        .join("Auxiliary")
                        .join("Build")
                        .join("vcvars64.bat");
                    if bat.exists() {
                        return Some(bat);
                    }
                }
            }
        }

        // Common hardcoded fallback paths
        let hardcoded = [
            r"C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat",
            r"C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat",
            r"C:\Program Files\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvars64.bat",
            r"C:\Program Files\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat",
            r"C:\Program Files (x86)\Microsoft Visual Studio\2019\Community\VC\Auxiliary\Build\vcvars64.bat",
            r"C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools\VC\Auxiliary\Build\vcvars64.bat",
        ];
        for p in &hardcoded {
            let pb = PathBuf::from(p);
            if pb.exists() {
                return Some(pb);
            }
        }

        None
    }

    pub fn get_detected_compiler_info(is_cpp: bool) -> Option<CompilerInfo> {
        let cache = if is_cpp {
            &DETECTED_CPP_COMPILER
        } else {
            &DETECTED_C_COMPILER
        };
        cache.get().and_then(|opt| opt.as_ref().map(|c| c.info.clone()))
    }

    pub fn truncate_output(bytes: &[u8], max_bytes: usize) -> (String, bool) {
        if bytes.len() <= max_bytes {
            return (String::from_utf8_lossy(bytes).to_string(), false);
        }
        let slice = &bytes[..max_bytes];
        let mut s = String::from_utf8_lossy(slice).to_string();
        if s.ends_with('\u{FFFD}') {
            s.pop();
        }
        s.push_str("\n...[输出超过 1MB，已截断]");
        (s, true)
    }

    pub async fn execute(req: RunRequest) -> Result<RunResponse, RunnerError> {
        let lang = req.language.to_lowercase();
        match lang.as_str() {
            "cpp" | "c++" => Self::execute_compiled(req, true).await,
            "c" => Self::execute_compiled(req, false).await,
            _ => Self::execute_interpreted(req).await,
        }
    }

    async fn execute_compiled(req: RunRequest, is_cpp: bool) -> Result<RunResponse, RunnerError> {
        let total_start = Instant::now();
        let compiler_cfg = Self::probe_compiler(is_cpp).await?;
        let timeout_secs = req.timeout_seconds.unwrap_or(30);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let session_dir = env::temp_dir()
            .join("noesis_runner")
            .join(format!("session_{}_{}", std::process::id(), timestamp));
        fs::create_dir_all(&session_dir).await?;

        // RAII-like guard to ensure session_dir is cleaned up
        struct DirGuard(PathBuf);
        impl Drop for DirGuard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _guard = DirGuard(session_dir.clone());

        let src_filename = if is_cpp { "main.cpp" } else { "main.c" };
        let src_path = session_dir.join(src_filename);
        let out_bin_name = if cfg!(windows) { "main.exe" } else { "main" };
        let runtime_dir = runtime_session_dir(&session_dir);
        let runtime_src_path = runtime_dir.join(src_filename);
        let runtime_out_bin_path = runtime_dir.join(out_bin_name);

        fs::write(&src_path, &req.code).await?;

        // Determine standard flag
        let std_flag = match req.cpp_standard.as_deref().unwrap_or(if is_cpp { "c++17" } else { "c11" }).to_lowercase().as_str() {
            "c++14" => if is_cpp { "-std=c++14" } else { "-std=c11" },
            "c++20" => if is_cpp { "-std=c++20" } else { "-std=c11" },
            "c11" => "-std=c11",
            _ => if is_cpp { "-std=c++17" } else { "-std=c11" },
        };

        // 1. Compile Phase
        let compile_start = Instant::now();
        let compile_res = match &compiler_cfg.kind {
            CompilerKind::GccLike { bin } => {
                let mut cmd = command_for_session(bin, &session_dir)?;
                cmd.arg(std_flag);
                cmd.args(["-O0", "-g", "-Wall", "-Wextra"]);
                if let Some(extra) = &req.extra_args {
                    for arg in extra.split_whitespace() {
                        cmd.arg(arg);
                    }
                }
                cmd.arg(&runtime_src_path);
                cmd.arg("-o");
                cmd.arg(&runtime_out_bin_path);
                cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);

                timeout(Duration::from_secs(COMPILE_TIMEOUT_SECONDS), cmd.output()).await
            }
            CompilerKind::ClDirect { bin } => {
                let mut cmd = command_for_session(bin, &session_dir)?;
                let cl_std = match std_flag {
                    "-std=c++14" => "/std:c++14",
                    "-std=c++20" => "/std:c++20",
                    "-std=c11" => "/std:c11",
                    _ => "/std:c++17",
                };
                cmd.arg(cl_std);
                cmd.args(["/EHsc", "/W4", "/O0", "/nologo"]);
                if let Some(extra) = &req.extra_args {
                    for arg in extra.split_whitespace() {
                        cmd.arg(arg);
                    }
                }
                cmd.arg(&runtime_src_path);
                cmd.arg(format!("/Fe:{}", out_bin_name));
                cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);

                timeout(Duration::from_secs(COMPILE_TIMEOUT_SECONDS), cmd.output()).await
            }
            CompilerKind::MsvcVcvars { vcvars_bat } => {
                let cl_std = match std_flag {
                    "-std=c++14" => "/std:c++14",
                    "-std=c++20" => "/std:c++20",
                    "-std=c11" => "/std:c11",
                    _ => "/std:c++17",
                };
                let extra_args_str = req.extra_args.as_deref().unwrap_or("");
                let cmd_line = format!(
                    "call \"{}\" >nul 2>&1 && cl {} /EHsc /W4 /O0 /nologo {} \"{}\" /Fe:{}",
                    vcvars_bat.to_string_lossy(),
                    cl_std,
                    extra_args_str,
                    runtime_src_path.to_string_lossy(),
                    out_bin_name
                );
                let mut cmd = command_for_session("cmd.exe", &session_dir)?;
                cmd.args(["/c", &cmd_line]);
                cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);

                timeout(Duration::from_secs(COMPILE_TIMEOUT_SECONDS), cmd.output()).await
            }
        };

        let compile_output = match compile_res {
            Ok(Ok(out)) => out,
            Ok(Err(io_err)) => return Err(RunnerError::Io(io_err)),
            Err(_) => return Err(RunnerError::Timeout(COMPILE_TIMEOUT_SECONDS)),
        };

        let (c_stdout, c_out_trunc) = Self::truncate_output(&compile_output.stdout, MAX_OUTPUT_BYTES);
        let (c_stderr, c_err_trunc) = Self::truncate_output(&compile_output.stderr, MAX_OUTPUT_BYTES);

        if !compile_output.status.success() {
            let duration_ms = compile_start.elapsed().as_millis() as u64;
            return Ok(RunResponse {
                stdout: String::new(),
                stderr: format!("编译失败:\n{}", c_stderr),
                exit_code: compile_output.status.code().unwrap_or(-1),
                duration_ms,
                success: false,
                truncated: c_out_trunc || c_err_trunc,
                phase: Some("compile".to_string()),
                compile_stdout: Some(c_stdout),
                compile_stderr: Some(c_stderr),
            });
        }

        // 2. Run Phase
        let _run_start = Instant::now();
        let runtime_binary = runtime_out_bin_path.to_string_lossy().to_string();
        let mut child = command_for_session(&runtime_binary, &session_dir)?;
        child.stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);

        if req.stdin.is_some() {
            child.stdin(Stdio::piped());
        } else {
            child.stdin(Stdio::null());
        }

        let mut spawned = child.spawn()?;

        // Pipe stdin if provided
        if let Some(stdin_content) = req.stdin {
            if let Some(mut stdin_pipe) = spawned.stdin.take() {
                let _ = stdin_pipe.write_all(stdin_content.as_bytes()).await;
                let _ = stdin_pipe.flush().await;
                drop(stdin_pipe);
            }
        }

        let run_res = match timeout(Duration::from_secs(timeout_secs), spawned.wait_with_output()).await {
            Ok(Ok(out)) => out,
            Ok(Err(io_err)) => return Err(RunnerError::Io(io_err)),
            Err(_) => {
                return Err(RunnerError::Timeout(timeout_secs));
            }
        };

        let duration_ms = total_start.elapsed().as_millis() as u64;
        let (stdout, out_trunc) = Self::truncate_output(&run_res.stdout, MAX_OUTPUT_BYTES);
        let (stderr, err_trunc) = Self::truncate_output(&run_res.stderr, MAX_OUTPUT_BYTES);

        Ok(RunResponse {
            stdout,
            stderr,
            exit_code: run_res.status.code().unwrap_or(-1),
            success: run_res.status.success(),
            duration_ms,
            truncated: out_trunc || err_trunc || c_out_trunc || c_err_trunc,
            phase: Some("run".to_string()),
            compile_stdout: Some(c_stdout),
            compile_stderr: Some(c_stderr),
        })
    }

    async fn execute_interpreted(req: RunRequest) -> Result<RunResponse, RunnerError> {
        let timeout_secs = req.timeout_seconds.unwrap_or(30);
        let start = Instant::now();

        let temp_root = env::temp_dir().join("noesis_runner");
        fs::create_dir_all(&temp_root).await?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let session_dir = temp_root.join(format!("session_{}_{}", std::process::id(), timestamp));
        fs::create_dir_all(&session_dir).await?;
        struct DirGuard(PathBuf);
        impl Drop for DirGuard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _guard = DirGuard(session_dir.clone());

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

        let script_file = session_dir.join(format!("scratch_{}.{}", timestamp, ext));
        fs::write(&script_file, req.code).await?;
        let runtime_script_file = runtime_session_dir(&session_dir).join(
            script_file.file_name().expect("script filename"),
        );

        let mut cmd = command_for_session(&cmd_name, &session_dir)?;
        for a in base_args {
            cmd.arg(a);
        }
        cmd.arg(&runtime_script_file)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        if req.stdin.is_some() {
            cmd.stdin(Stdio::piped());
        } else {
            cmd.stdin(Stdio::null());
        }

        let mut spawned = cmd.spawn()?;

        if let Some(stdin_content) = req.stdin {
            if let Some(mut stdin_pipe) = spawned.stdin.take() {
                let _ = stdin_pipe.write_all(stdin_content.as_bytes()).await;
                let _ = stdin_pipe.flush().await;
                drop(stdin_pipe);
            }
        }

        let res = match timeout(Duration::from_secs(timeout_secs), spawned.wait_with_output()).await {
            Ok(output_res) => output_res?,
            Err(_) => {
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
            phase: Some("run".to_string()),
            compile_stdout: None,
            compile_stderr: None,
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
                stdin: None,
                cpp_standard: None,
                extra_args: None,
            };
            let res = CodeRunner::execute(req).await;
            assert!(matches!(res, Err(RunnerError::Timeout(1))));
        }
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn linux_runner_cannot_read_files_outside_session() {
        let bwrap_available = env::var_os("PATH")
            .into_iter()
            .flat_map(|path| env::split_paths(&path).collect::<Vec<_>>())
            .any(|dir| dir.join("bwrap").is_file());
        if !bwrap_available || std::process::Command::new("node").arg("--version").output().is_err() {
            println!("Skipping Linux isolation test: bwrap or Node.js is unavailable");
            return;
        }

        let outside_file = env::temp_dir().join(format!(
            "noesis-runner-outside-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::write(&outside_file, "OUTSIDE_RUNTIME_SECRET").await.expect("write host-only fixture");
        let path_literal = serde_json::to_string(&outside_file.to_string_lossy().to_string()).unwrap();
        let code = format!(
            "const fs = require('fs'); let visible = false; for (const p of [{}, '/etc/passwd']) {{ try {{ fs.readFileSync(p); visible = true; }} catch (e) {{ if (e.code !== 'ENOENT') throw e; }} }} console.log(visible ? 'visible' : 'hidden');",
            path_literal
        );
        let res = CodeRunner::execute(RunRequest {
            language: "javascript".to_string(),
            code,
            timeout_seconds: Some(10),
            stdin: None,
            cpp_standard: None,
            extra_args: None,
        }).await.expect("execute isolated JavaScript");
        let _ = fs::remove_file(outside_file).await;
        assert!(res.success, "sandboxed runner failed: {:?}", res);
        assert!(res.stdout.contains("hidden"), "host fixture was visible: {:?}", res.stdout);
    }

    #[tokio::test]
    async fn test_cpp_runner_hello_world() {
        let compiler = match CodeRunner::probe_compiler(true).await {
            Ok(c) => c,
            Err(e) => {
                println!("Skipping test_cpp_runner_hello_world: {}", e);
                return;
            }
        };
        println!("Testing with C++ compiler: {:?}", compiler.info);

        let code = r#"
#include <iostream>
int main() {
    std::cout << "Hello from Noesis C++ Sandbox!" << std::endl;
    return 0;
}
"#;
        let req = RunRequest {
            language: "cpp".to_string(),
            code: code.to_string(),
            timeout_seconds: Some(10),
            stdin: None,
            cpp_standard: Some("c++17".to_string()),
            extra_args: None,
        };
        let res = CodeRunner::execute(req).await.expect("execute C++ code");
        assert!(res.success, "Compilation or execution failed: {:?}", res);
        assert_eq!(res.phase.as_deref(), Some("run"));
        assert!(res.stdout.contains("Hello from Noesis C++ Sandbox!"));
    }

    #[tokio::test]
    async fn test_cpp_runner_stdin_sum() {
        if let Err(e) = CodeRunner::probe_compiler(true).await {
            println!("Skipping test_cpp_runner_stdin_sum: {}", e);
            return;
        }

        let code = r#"
#include <iostream>
int main() {
    int a, b;
    if (std::cin >> a >> b) {
        std::cout << "SUM=" << (a + b) << std::endl;
    }
    return 0;
}
"#;
        let req = RunRequest {
            language: "cpp".to_string(),
            code: code.to_string(),
            timeout_seconds: Some(10),
            stdin: Some("15 27\n".to_string()),
            cpp_standard: Some("c++17".to_string()),
            extra_args: None,
        };
        let res = CodeRunner::execute(req).await.expect("execute C++ stdin");
        assert!(res.success, "Failed: {:?}", res);
        assert!(res.stdout.contains("SUM=42"));
    }

    #[tokio::test]
    async fn test_cpp_runner_compile_error() {
        if let Err(e) = CodeRunner::probe_compiler(true).await {
            println!("Skipping test_cpp_runner_compile_error: {}", e);
            return;
        }

        let code = r#"
#include <iostream>
int main() {
    std::cout << "Missing semicolon"
    return 0;
}
"#;
        let req = RunRequest {
            language: "cpp".to_string(),
            code: code.to_string(),
            timeout_seconds: Some(10),
            stdin: None,
            cpp_standard: Some("c++17".to_string()),
            extra_args: None,
        };
        let res = CodeRunner::execute(req).await.expect("execute C++ compile error");
        assert!(!res.success);
        assert_eq!(res.phase.as_deref(), Some("compile"));
        assert!(res.stderr.contains("编译失败"));
    }

    #[tokio::test]
    async fn test_cpp_runner_infinite_loop_timeout() {
        if let Err(e) = CodeRunner::probe_compiler(true).await {
            println!("Skipping test_cpp_runner_infinite_loop_timeout: {}", e);
            return;
        }

        let code = r#"
int main() {
    while (true) {}
    return 0;
}
"#;
        let req = RunRequest {
            language: "cpp".to_string(),
            code: code.to_string(),
            timeout_seconds: Some(2),
            stdin: None,
            cpp_standard: Some("c++17".to_string()),
            extra_args: None,
        };
        let res = CodeRunner::execute(req).await;
        assert!(matches!(res, Err(RunnerError::Timeout(2))));
    }
}
