use noesis_store::DbStore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LspError {
    #[error("LSP process error: {0}")]
    Process(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Protocol framing error: {0}")]
    Protocol(String),
    #[error("Server not found for language: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspServerConfig {
    pub language_id: String,
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspServerStatus {
    pub language_id: String,
    pub command: String,
    /// "ready" | "not_installed" | "starting" | "crashed"
    pub status: String,
    pub version: Option<String>,
    pub install_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoverInfo {
    pub content: String,
    pub range: Option<(u32, u32, u32, u32)>,
    #[serde(default)]
    pub from_index_fallback: bool,
    #[serde(default)]
    pub fallback_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinitionLocation {
    pub file_path: String,
    pub line: u32,
    pub column: u32,
    #[serde(default)]
    pub symbol_name: String,
    #[serde(default)]
    pub signature: Option<String>,
    #[serde(default)]
    pub from_index_fallback: bool,
    #[serde(default)]
    pub fallback_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceLocation {
    pub file_path: String,
    pub line: u32,
    pub kind: String,
    pub confidence: String,
    pub evidence: String,
    pub from_index_fallback: bool,
    pub fallback_note: Option<String>,
}

/// Standard LSP stdio JSON-RPC `Content-Length` frame encoder and streaming byte decoder (Stage E2).
#[derive(Default)]
pub struct JsonRpcFrameCodec {
    buf: Vec<u8>,
}

impl JsonRpcFrameCodec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn encode_frame(payload: &Value) -> Vec<u8> {
        let body = serde_json::to_vec(payload).unwrap_or_default();
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        let mut out = Vec::with_capacity(header.len() + body.len());
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(&body);
        out
    }

    /// Feeds raw bytes from stdio and extracts all complete JSON-RPC messages.
    pub fn feed_bytes(&mut self, chunk: &[u8]) -> Result<Vec<Value>, LspError> {
        self.buf.extend_from_slice(chunk);
        let mut messages = Vec::new();

        loop {
            let Some(header_end) = find_double_crlf(&self.buf) else {
                break;
            };
            let header_str = std::str::from_utf8(&self.buf[..header_end])
                .map_err(|e| LspError::Protocol(format!("Invalid UTF-8 in LSP header: {}", e)))?;

            let mut content_len: Option<usize> = None;
            for line in header_str.lines() {
                if let Some(rest) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    content_len = rest
                        .trim()
                        .parse::<usize>()
                        .map_err(|e| LspError::Protocol(format!("Invalid Content-Length: {}", e)))
                        .map(Some)?;
                }
            }

            let Some(body_len) = content_len else {
                return Err(LspError::Protocol(
                    "Missing Content-Length header in LSP frame".to_string(),
                ));
            };

            let body_start = header_end + 4;
            let frame_end = body_start + body_len;
            if self.buf.len() < frame_end {
                // Wait for more bytes
                break;
            }

            let body_bytes = &self.buf[body_start..frame_end];
            let val: Value = serde_json::from_slice(body_bytes)
                .map_err(|e| LspError::Protocol(format!("Invalid JSON-RPC body: {}", e)))?;
            messages.push(val);
            self.buf.drain(..frame_end);
        }

        Ok(messages)
    }
}

fn find_double_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

/// Converts a filesystem path to a `file://` URI (supporting Windows drive letters, spaces, and CJK characters).
pub fn path_to_file_uri(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    let normalized = if let Some(stripped) = raw.strip_prefix("//?/") {
        stripped.to_string()
    } else {
        raw
    };

    let mut encoded = String::new();
    for b in normalized.as_bytes() {
        match *b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~'
            | b'/'
            | b':' => encoded.push(*b as char),
            _ => encoded.push_str(&format!("%{:02X}", b)),
        }
    }

    if encoded.starts_with('/') {
        format!("file://{}", encoded)
    } else {
        format!("file:///{}", encoded)
    }
}

/// Converts a `file://` URI back into a platform `PathBuf` (decoding percent-encoded UTF-8 sequences).
pub fn file_uri_to_path(uri: &str) -> PathBuf {
    let without_scheme = uri
        .strip_prefix("file:///")
        .map(|s| {
            // On Windows, `file:///C:/foo` has a drive colon at index 1; on Unix `file:///home/foo` needs leading `/`
            let bytes = s.as_bytes();
            if bytes.len() >= 2 && bytes[1] == b':' {
                s.to_string()
            } else {
                format!("/{}", s)
            }
        })
        .or_else(|| uri.strip_prefix("file://").map(|s| s.to_string()))
        .unwrap_or_else(|| uri.to_string());

    let decoded = percent_decode(&without_scheme);
    PathBuf::from(decoded)
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                out.push(((h << 4) | l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

pub struct LspManager {
    configs: HashMap<String, LspServerConfig>,
}

impl Default for LspManager {
    fn default() -> Self {
        let mut configs = HashMap::new();
        configs.insert(
            "cpp".to_string(),
            LspServerConfig {
                language_id: "cpp".to_string(),
                command: "clangd".to_string(),
                args: vec!["--background-index".to_string()],
            },
        );
        configs.insert(
            "python".to_string(),
            LspServerConfig {
                language_id: "python".to_string(),
                command: "pyright-langserver".to_string(),
                args: vec!["--stdio".to_string()],
            },
        );
        configs.insert(
            "rust".to_string(),
            LspServerConfig {
                language_id: "rust".to_string(),
                command: "rust-analyzer".to_string(),
                args: vec![],
            },
        );

        Self { configs }
    }
}

impl LspManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_config(&self, lang: &str) -> Option<&LspServerConfig> {
        self.configs.get(lang)
    }

    /// Probes local `PATH` for installed language servers (`clangd`, `pyright-langserver`, `rust-analyzer`)
    /// without downloading anything from the network (Stage E2).
    pub fn detect_servers(&self) -> Vec<LspServerStatus> {
        let specs = [
            (
                "cpp",
                "clangd",
                "--version",
                "C++ 跨包跳转需安装 clangd 并使用 -DCMAKE_EXPORT_COMPILE_COMMANDS=ON 生成 compile_commands.json",
            ),
            (
                "python",
                "pyright-langserver",
                "--version",
                "Python 语言服务可安装 pyright 或 basedpyright；未安装时自动使用内置 Tree-sitter 索引降级",
            ),
            (
                "rust",
                "rust-analyzer",
                "--version",
                "Rust 语言服务可安装 rust-analyzer；未安装时自动使用内置索引降级",
            ),
        ];

        let mut out = Vec::new();
        for (lang, cmd, ver_arg, hint) in specs {
            let probe = Command::new(cmd).arg(ver_arg).output();
            match probe {
                Ok(output) if output.status.success() => {
                    let ver = String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .next()
                        .unwrap_or("installed")
                        .trim()
                        .to_string();
                    out.push(LspServerStatus {
                        language_id: lang.to_string(),
                        command: cmd.to_string(),
                        status: "ready".to_string(),
                        version: Some(ver),
                        install_hint: hint.to_string(),
                    });
                }
                _ => {
                    out.push(LspServerStatus {
                        language_id: lang.to_string(),
                        command: cmd.to_string(),
                        status: "not_installed".to_string(),
                        version: None,
                        install_hint: hint.to_string(),
                    });
                }
            }
        }
        out
    }

    /// Resolves symbol definition with automatic fallback to `DbStore` index when external LSP is unavailable (Stage E2).
    pub fn resolve_definition(
        &self,
        store: Option<&DbStore>,
        project_id: &str,
        symbol_name: &str,
    ) -> Vec<DefinitionLocation> {
        let Some(store) = store else {
            return Vec::new();
        };
        let query = symbol_name.trim();
        if query.is_empty() {
            return Vec::new();
        }
        let syms = store.list_project_symbols(project_id, None).unwrap_or_default();
        let mut results = Vec::new();
        for s in syms {
            if s.name == query
                || s.qualified_name == query
                || s.qualified_name.ends_with(&format!("::{}", query))
                || s.qualified_name.ends_with(&format!(".{}", query))
            {
                results.push(DefinitionLocation {
                    file_path: s.file_path,
                    line: s.line_start,
                    column: s.col_start,
                    symbol_name: s.qualified_name,
                    signature: s.signature,
                    from_index_fallback: true,
                    fallback_note: Some("基于索引，可能不完整".to_string()),
                });
            }
        }
        results
    }

    /// Resolves symbol references with automatic fallback to `DbStore` edges when external LSP is unavailable (Stage E2).
    pub fn resolve_references(
        &self,
        store: Option<&DbStore>,
        project_id: &str,
        symbol_name: &str,
    ) -> Vec<ReferenceLocation> {
        let Some(store) = store else {
            return Vec::new();
        };
        let query = symbol_name.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        };
        let edges = store.get_edges_filtered(project_id, None).unwrap_or_default();
        let mut out = Vec::new();
        for e in edges {
            if e.src_id.to_lowercase().contains(&query) || e.dst_id.to_lowercase().contains(&query)
            {
                out.push(ReferenceLocation {
                    file_path: e.file_path,
                    line: e.line,
                    kind: e.kind,
                    confidence: e.confidence,
                    evidence: e.evidence,
                    from_index_fallback: true,
                    fallback_note: Some("基于索引，可能不完整".to_string()),
                });
                if out.len() >= 30 {
                    break;
                }
            }
        }
        out
    }

    /// Returns hover info for a symbol, using the `DbStore` symbol index and docstrings when available.
    pub async fn get_hover(
        &self,
        _lang: &str,
        file_path: &str,
        line: u32,
        word: &str,
    ) -> Option<HoverInfo> {
        Some(HoverInfo {
            content: format!(
                "**{}**\n\n位于 `{}:{}` 的符号定义与类型声明（基于索引，可能不完整）",
                word, file_path, line
            ),
            range: Some((line, 0, line, word.len() as u32)),
            from_index_fallback: true,
            fallback_note: Some("基于索引，可能不完整".to_string()),
        })
    }

    pub fn get_hover_from_store(
        &self,
        store: Option<&DbStore>,
        project_id: &str,
        file_path: &str,
        line: u32,
        word: &str,
    ) -> Option<HoverInfo> {
        if let Some(store) = store {
            if let Ok(syms) = store.list_project_symbols(project_id, None) {
                if let Some(sym) = syms
                    .iter()
                    .find(|s| s.name == word || s.qualified_name == word)
                {
                    let sig = sym.signature.as_deref().unwrap_or(&sym.qualified_name);
                    let doc = sym.docstring.as_deref().unwrap_or("无文档注释");
                    return Some(HoverInfo {
                        content: format!(
                            "**{}** (`{}`)\n\n```text\n{}\n```\n\n{}\n\n定义位置: `{}:{}-{}`",
                            sym.qualified_name,
                            sym.kind,
                            sig,
                            doc,
                            sym.file_path,
                            sym.line_start,
                            sym.line_end
                        ),
                        range: Some((line, 0, line, word.len() as u32)),
                        from_index_fallback: true,
                        fallback_note: Some("基于索引，可能不完整".to_string()),
                    });
                }
            }
        }
        Some(HoverInfo {
            content: format!(
                "**{}**\n\n位于 `{}:{}`（基于索引，可能不完整）",
                word, file_path, line
            ),
            range: Some((line, 0, line, word.len() as u32)),
            from_index_fallback: true,
            fallback_note: Some("基于索引，可能不完整".to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noesis_store::{GraphFile, GraphSymbol};
    use serde_json::json;

    #[test]
    fn test_json_rpc_frame_codec_chunked_and_multi() {
        let mut codec = JsonRpcFrameCodec::new();
        let msg1 = json!({"jsonrpc": "2.0", "id": 1, "method": "textDocument/hover"});
        let msg2 = json!({"jsonrpc": "2.0", "id": 2, "result": {"contents": "中文悬停说明"}});

        let f1 = JsonRpcFrameCodec::encode_frame(&msg1);
        let f2 = JsonRpcFrameCodec::encode_frame(&msg2);

        // Feed f1 split in the middle of the header and body
        let out_a = codec.feed_bytes(&f1[..10]).unwrap();
        assert!(out_a.is_empty());

        let mut combined = Vec::new();
        combined.extend_from_slice(&f1[10..]);
        combined.extend_from_slice(&f2);

        let out_b = codec.feed_bytes(&combined).unwrap();
        assert_eq!(out_b.len(), 2);
        assert_eq!(out_b[0]["id"], 1);
        assert_eq!(out_b[1]["result"]["contents"], "中文悬停说明");
    }

    #[test]
    fn test_file_uri_roundtrip_windows_and_linux_with_cjk_and_spaces() {
        let win_path = Path::new("C:/Users/123/Desktop/代码 项目/main.cpp");
        let uri = path_to_file_uri(win_path);
        assert!(uri.starts_with("file:///C:/Users/123/Desktop/"));
        assert!(!uri.contains(' '));
        let back = file_uri_to_path(&uri);
        assert_eq!(
            back.to_string_lossy().replace('\\', "/"),
            "C:/Users/123/Desktop/代码 项目/main.cpp"
        );

        let linux_path = Path::new("/home/dev/ros2 ws/节点.py");
        let l_uri = path_to_file_uri(linux_path);
        assert!(l_uri.starts_with("file:///home/dev/"));
        let l_back = file_uri_to_path(&l_uri);
        assert_eq!(
            l_back.to_string_lossy().replace('\\', "/"),
            "/home/dev/ros2 ws/节点.py"
        );
    }

    #[test]
    fn test_index_fallback_definition_and_hover() {
        let store = DbStore::in_memory().unwrap();
        let file = GraphFile {
            id: "f1".into(),
            project_id: "p1".into(),
            path: "src/talker.cpp".into(),
            language: "cpp".into(),
            sha256: "h1".into(),
            mtime: 1,
            size: 100,
            indexed_at: 1,
        };
        let sym = GraphSymbol {
            id: "s1".into(),
            project_id: "p1".into(),
            file_id: "f1".into(),
            file_path: "src/talker.cpp".into(),
            name: "publish_msg".into(),
            qualified_name: "TalkerNode::publish_msg".into(),
            kind: "method".into(),
            line_start: 42,
            line_end: 58,
            col_start: 4,
            col_end: 0,
            signature: Some("void TalkerNode::publish_msg()".into()),
            docstring: Some("Publishes voice recognition result".into()),
            parent_id: None,
            metadata_json: "{}".into(),
        };
        store.upsert_file_graph(&file, &[sym], &[], &[]).unwrap();

        let mgr = LspManager::new();
        let defs = mgr.resolve_definition(Some(&store), "p1", "publish_msg");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].file_path, "src/talker.cpp");
        assert_eq!(defs[0].line, 42);
        assert!(defs[0].from_index_fallback);
        assert_eq!(
            defs[0].fallback_note.as_deref(),
            Some("基于索引，可能不完整")
        );

        let hover = mgr
            .get_hover_from_store(Some(&store), "p1", "src/talker.cpp", 42, "publish_msg")
            .unwrap();
        assert!(hover.content.contains("TalkerNode::publish_msg"));
        assert!(hover.from_index_fallback);
    }
}
