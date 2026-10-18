use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LlmError {
    #[error("Outbound policy violation: {0}")]
    OutboundBlocked(String),
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("API error: {0}")]
    Api(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: String, // "deepseek" | "openai" | "ollama" | "custom"
    pub api_url: String,  // e.g. "https://api.deepseek.com/v1" or "http://localhost:11434/v1"
    pub api_key: String,
    pub model: String, // e.g. "deepseek-chat" or "deepseek-coder"
    pub temperature: f32,
    pub max_tokens: u32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "deepseek".to_string(),
            api_url: "https://api.deepseek.com".to_string(),
            api_key: "sk-136dbd0231f54d29820ef0a7f731550c".to_string(),
            model: "deepseek-chat".to_string(),
            temperature: 0.3,
            max_tokens: 4096,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "system" | "user" | "assistant"
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundPreview {
    pub target_url: String,
    pub model: String,
    pub messages_count: usize,
    pub total_chars: usize,
    pub contains_sanitized_tokens: bool,
    pub sanitized_preview: String,
    pub file_references: Vec<String>,
}

#[derive(Clone)]
pub struct OutboundFilter {
    secret_patterns: Vec<Regex>,
}

impl Default for OutboundFilter {
    fn default() -> Self {
        Self {
            secret_patterns: vec![
                // Private key headers
                Regex::new(r"-----BEGIN [A-Z ]+PRIVATE KEY-----").unwrap(),
                // API keys (sk-..., etc.)
                Regex::new(r#"(?i)(?:api_?key|token|secret|password|passwd|auth)\s*[:=]\s*['"]?([a-zA-Z0-9_\-\.]{12,})['"]?"#).unwrap(),
                // IPv4 addresses (optional filter for sensitive internal subnets)
                Regex::new(r"\b10\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap(),
                Regex::new(r"\b192\.168\.\d{1,3}\.\d{1,3}\b").unwrap(),
            ],
        }
    }
}

impl OutboundFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check_file_path_allowed(&self, file_path: &str) -> Result<(), LlmError> {
        let lower = file_path.to_lowercase();
        let blocked = [
            ".env", ".key", ".pem", ".pfx", ".cert", ".crt",
            "id_rsa", "id_ed25519", "credentials", "secrets",
        ];

        for b in blocked {
            if lower.contains(b) {
                return Err(LlmError::OutboundBlocked(format!(
                    "包含敏感后缀或配置文件名: '{}'，已触发 P0 防外发安全拦截！",
                    file_path
                )));
            }
        }
        Ok(())
    }

    pub fn sanitize_text(&self, text: &str) -> (String, bool) {
        let mut sanitized = text.to_string();
        let mut found = false;

        for re in &self.secret_patterns {
            if re.is_match(&sanitized) {
                found = true;
                sanitized = re.replace_all(&sanitized, "[SENSITIVE_REDACTED]").to_string();
            }
        }

        (sanitized, found)
    }

    pub fn inspect_outbound(&self, config: &LlmConfig, messages: &[ChatMessage]) -> Result<OutboundPreview, LlmError> {
        let mut total_chars = 0;
        let mut has_sanitized = false;
        let mut all_content = String::new();

        for m in messages {
            total_chars += m.content.len();
            let (clean, changed) = self.sanitize_text(&m.content);
            if changed {
                has_sanitized = true;
            }
            all_content.push_str(&format!("[{}]\n{}\n\n", m.role, clean));
        }

        if total_chars > 100_000 {
            return Err(LlmError::OutboundBlocked(format!(
                "请求内容量超限 ({} 字符，上限 100,000)，请收缩代码选区！",
                total_chars
            )));
        }

        let preview_snippet = if all_content.len() > 1000 {
            format!("{}...\n[内容已截断预览]", &all_content[..1000])
        } else {
            all_content
        };

        Ok(OutboundPreview {
            target_url: config.api_url.clone(),
            model: config.model.clone(),
            messages_count: messages.len(),
            total_chars,
            contains_sanitized_tokens: has_sanitized,
            sanitized_preview: preview_snippet,
            file_references: vec![],
        })
    }
}

pub struct LlmGateway {
    client: reqwest::Client,
    filter: Arc<OutboundFilter>,
}

impl Default for LlmGateway {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .unwrap(),
            filter: Arc::new(OutboundFilter::new()),
        }
    }
}

impl LlmGateway {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn filter(&self) -> &OutboundFilter {
        &self.filter
    }

    pub async fn send_chat(
        &self,
        config: &LlmConfig,
        messages: Vec<ChatMessage>,
    ) -> Result<String, LlmError> {
        // 1. P0 Outbound audit
        let _ = self.filter.inspect_outbound(config, &messages)?;

        // 2. Sanitize contents before sending
        let sanitized_messages: Vec<serde_json::Value> = messages
            .into_iter()
            .map(|m| {
                let (clean, _) = self.filter.sanitize_text(&m.content);
                serde_json::json!({
                    "role": m.role,
                    "content": clean
                })
            })
            .collect();

        // 3. Fallback heuristic output if API key is not configured yet
        if config.api_key.trim().is_empty() {
            return Ok(
                "【本地离线解读响应 (未配置 API Key)】\n\n\
                **整体视角**：该单元属于项目核心逻辑层，负责数据调度与结构管理。\n\
                **逻辑视角**：通过明确的状态转换和边界校验确保并发安全与调用幂等。\n\
                **语法视角**：遵循现代语言规范，合理使用模式匹配与强类型抽象。\n\
                **原理视角**：依托底层确定性状态机与轻量级异步运行时支撑高吞吐。\n\n\
                *(请在右下角设置面板中填入 DeepSeek 或兼容的 API Key 以启用大模型实时流式解答)*"
                    .to_string(),
            );
        }

        let base = config.api_url.trim_end_matches('/');
        let url = if base.ends_with("/chat/completions") {
            base.to_string()
        } else {
            format!("{}/chat/completions", base)
        };
        let body = serde_json::json!({
            "model": config.model,
            "messages": sanitized_messages,
            "temperature": config.temperature,
            "max_tokens": config.max_tokens,
            "stream": false
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", config.api_key.trim()))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let err_text = resp.text().await.unwrap_or_default();
            return Err(LlmError::Api(format!("API 请求失败: {}", err_text)));
        }

        let res_json: serde_json::Value = resp.json().await?;
        let content = res_json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_p0_block_sensitive_files() {
        let filter = OutboundFilter::new();
        assert!(filter.check_file_path_allowed("src/main.rs").is_ok());
        assert!(filter.check_file_path_allowed(".env").is_err());
        assert!(filter.check_file_path_allowed("config/.env.local").is_err());
        assert!(filter.check_file_path_allowed("id_rsa").is_err());
        assert!(filter.check_file_path_allowed("server.key").is_err());
        assert!(filter.check_file_path_allowed("cert.pem").is_err());
    }

    #[test]
    fn test_secret_sanitization() {
        let filter = OutboundFilter::new();
        let raw = "Connect to 192.168.1.100 with api_key: 'sk-1234567890abcdef12345'";
        let (sanitized, modified) = filter.sanitize_text(raw);
        assert!(modified);
        assert!(!sanitized.contains("192.168.1.100"));
        assert!(!sanitized.contains("sk-1234567890abcdef12345"));
        assert!(sanitized.contains("[SENSITIVE_REDACTED]"));
    }
}
