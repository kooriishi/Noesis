use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LspError {
    #[error("LSP process error: {0}")]
    Process(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
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
pub struct HoverInfo {
    pub content: String,
    pub range: Option<(u32, u32, u32, u32)>, // start_line, start_col, end_line, end_col
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinitionLocation {
    pub file_path: String,
    pub line: u32,
    pub column: u32,
}

pub struct LspManager {
    configs: HashMap<String, LspServerConfig>,
}

impl Default for LspManager {
    fn default() -> Self {
        let mut configs = HashMap::new();
        configs.insert(
            "rust".to_string(),
            LspServerConfig {
                language_id: "rust".to_string(),
                command: "rust-analyzer".to_string(),
                args: vec![],
            },
        );
        configs.insert(
            "typescript".to_string(),
            LspServerConfig {
                language_id: "typescript".to_string(),
                command: "typescript-language-server".to_string(),
                args: vec!["--stdio".to_string()],
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

    pub async fn get_hover(&self, _lang: &str, file_path: &str, line: u32, word: &str) -> Option<HoverInfo> {
        // Fallback / mock heuristic hover info
        Some(HoverInfo {
            content: format!("**{}**\n\n位于 `{}:{}` 的符号定义与类型声明", word, file_path, line),
            range: Some((line, 0, line, word.len() as u32)),
        })
    }
}
