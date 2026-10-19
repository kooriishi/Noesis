use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use thiserror::Error;

pub mod stream;
pub mod tools;

#[derive(Error, Debug)]
pub enum LlmError {
    #[error("Outbound policy violation: {0}")]
    OutboundBlocked(String),
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("API error: {0}")]
    Api(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: String, // "deepseek" | "openai" | "ollama" | "custom"
    pub api_url: String,  // e.g. "https://api.deepseek.com" or "http://localhost:11434/v1"
    #[serde(default)]
    pub api_key: String,
    pub model: String, // e.g. "deepseek-chat"
    pub temperature: f32,
    pub max_tokens: u32,
    #[serde(default)]
    pub has_api_key: bool,
    #[serde(default)]
    pub api_key_masked: String,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub custom_ca_cert_path: Option<String>,
    #[serde(default)]
    pub offline_mode: bool,
    #[serde(default = "default_true")]
    pub json_mode: bool,
}

fn default_true() -> bool {
    true
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "deepseek".to_string(),
            api_url: "https://api.deepseek.com".to_string(),
            api_key: String::new(),
            model: "deepseek-chat".to_string(),
            temperature: 0.3,
            max_tokens: 6144,
            has_api_key: false,
            api_key_masked: String::new(),
            proxy_url: None,
            custom_ca_cert_path: None,
            offline_mode: false,
            json_mode: true,
        }
    }
}

pub fn mask_api_key(key: &str) -> String {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() <= 4 {
        "****".to_string()
    } else {
        let suffix: String = chars[chars.len() - 4..].iter().collect();
        format!("sk-****{}", suffix)
    }
}

pub fn is_masked_key(key: &str) -> bool {
    let trimmed = key.trim();
    trimmed.contains("****")
}

/// Manages LLM configuration and API credentials in `app_data_dir` with restricted permissions,
/// ensuring raw API keys are never persisted in browser `localStorage`.
#[derive(Clone)]
pub struct CredentialStore {
    config_path: Option<PathBuf>,
    secret_path: Option<PathBuf>,
    inner: Arc<Mutex<(LlmConfig, String)>>,
}

impl CredentialStore {
    pub fn new(app_data_dir: Option<PathBuf>) -> Self {
        let (config_path, secret_path) = match app_data_dir {
            Some(dir) => {
                let _ = fs::create_dir_all(&dir);
                (
                    Some(dir.join("llm_config.json")),
                    Some(dir.join("llm_credentials.secret")),
                )
            }
            None => (None, None),
        };

        let mut cfg = LlmConfig::default();
        let mut raw_key = String::new();

        if let Some(ref cp) = config_path {
            if let Ok(content) = fs::read_to_string(cp) {
                if let Ok(loaded) = serde_json::from_str::<LlmConfig>(&content) {
                    cfg = loaded;
                }
            }
        }

        if let Some(ref sp) = secret_path {
            if let Ok(secret) = fs::read_to_string(sp) {
                raw_key = secret.trim().to_string();
            }
        }

        // Never keep raw key in the public view struct
        cfg.api_key = String::new();
        cfg.has_api_key = !raw_key.is_empty();
        cfg.api_key_masked = mask_api_key(&raw_key);

        Self {
            config_path,
            secret_path,
            inner: Arc::new(Mutex::new((cfg, raw_key))),
        }
    }

    pub fn get_config_view(&self) -> LlmConfig {
        let guard = self.inner.lock().unwrap();
        let mut view = guard.0.clone();
        let raw_key = &guard.1;
        view.api_key = String::new();
        view.has_api_key = !raw_key.is_empty();
        view.api_key_masked = mask_api_key(raw_key);
        view
    }

    pub fn update_config(&self, incoming: LlmConfig) -> Result<LlmConfig, LlmError> {
        let mut guard = self.inner.lock().unwrap();
        let incoming_key = incoming.api_key.trim().to_string();
        if !incoming_key.is_empty() && !is_masked_key(&incoming_key) {
            guard.1 = incoming_key;
            self.persist_secret(&guard.1)?;
        }

        guard.0.provider = incoming.provider;
        guard.0.api_url = incoming.api_url;
        guard.0.model = incoming.model;
        guard.0.temperature = incoming.temperature;
        guard.0.max_tokens = incoming.max_tokens;
        guard.0.proxy_url = incoming.proxy_url;
        guard.0.custom_ca_cert_path = incoming.custom_ca_cert_path;
        guard.0.offline_mode = incoming.offline_mode;
        guard.0.api_key = String::new();
        guard.0.has_api_key = !guard.1.is_empty();
        guard.0.api_key_masked = mask_api_key(&guard.1);

        self.persist_config(&guard.0)?;

        Ok(guard.0.clone())
    }

    pub fn set_api_key(&self, api_key: &str) -> Result<LlmConfig, LlmError> {
        let mut guard = self.inner.lock().unwrap();
        guard.1 = api_key.trim().to_string();
        self.persist_secret(&guard.1)?;
        guard.0.has_api_key = !guard.1.is_empty();
        guard.0.api_key_masked = mask_api_key(&guard.1);
        guard.0.api_key = String::new();
        Ok(guard.0.clone())
    }

    pub fn clear_api_key(&self) -> Result<LlmConfig, LlmError> {
        let mut guard = self.inner.lock().unwrap();
        guard.1.clear();
        if let Some(ref sp) = self.secret_path {
            let _ = fs::remove_file(sp);
        }
        guard.0.has_api_key = false;
        guard.0.api_key_masked.clear();
        guard.0.api_key.clear();
        Ok(guard.0.clone())
    }

    /// Resolves an incoming `LlmConfig` by injecting the stored raw API key if `incoming.api_key`
    /// is empty or masked.
    pub fn resolve_effective_config(&self, incoming: &LlmConfig) -> LlmConfig {
        let guard = self.inner.lock().unwrap();
        let mut resolved = incoming.clone();
        let candidate = incoming.api_key.trim();
        if candidate.is_empty() || is_masked_key(candidate) {
            resolved.api_key = guard.1.clone();
        }
        resolved.has_api_key = !resolved.api_key.trim().is_empty();
        resolved.api_key_masked = mask_api_key(&resolved.api_key);
        resolved
    }

    fn persist_config(&self, cfg: &LlmConfig) -> Result<(), LlmError> {
        if let Some(ref cp) = self.config_path {
            let mut clean = cfg.clone();
            clean.api_key.clear();
            let json = serde_json::to_string_pretty(&clean)?;
            fs::write(cp, json)?;
        }
        Ok(())
    }

    fn persist_secret(&self, secret: &str) -> Result<(), LlmError> {
        if let Some(ref sp) = self.secret_path {
            fs::write(sp, secret)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = fs::Permissions::from_mode(0o600);
                let _ = fs::set_permissions(sp, perms);
            }
        }
        Ok(())
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionTestResult {
    pub ok: bool,
    pub status_code: Option<u16>,
    pub latency_ms: u64,
    pub category: String, // "ok" | "offline" | "auth" | "tls" | "proxy" | "timeout" | "http_error" | "network"
    pub message: String,
}

#[derive(Clone)]
pub struct OutboundFilter {
    secret_patterns: Vec<Regex>,
}

impl Default for OutboundFilter {
    fn default() -> Self {
        Self {
            secret_patterns: vec![
                // Private key and certificate PEM blocks
                Regex::new(r"-----BEGIN [A-Z ]+-----[A-Za-z0-9+/=\s\r\n]+-----END [A-Z ]+-----")
                    .unwrap(),
                Regex::new(r"-----BEGIN [A-Z ]+-----").unwrap(),
                // Standalone OpenAI/DeepSeek style sk- API keys (20+ chars)
                Regex::new(r"\bsk-[a-zA-Z0-9_\-]{20,}\b").unwrap(),
                // Bearer auth tokens
                Regex::new(r"(?i)Bearer\s+[a-zA-Z0-9_\-\.]{20,}").unwrap(),
                // Key-value credentials (api_key: '...', etc.)
                Regex::new(
                    r#"(?i)(?:api_?key|token|secret|password|passwd|auth)\s*[:=]\s*['"]([a-zA-Z0-9_\-\.]{12,})['"]"#,
                )
                .unwrap(),
                // IPv4 addresses (sensitive internal subnets)
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

    /// Checks whether a relative file path is allowed to be read or attached to an outbound context.
    /// Uses exact filename, extension, and directory segment rules (matching `noesis-project`).
    pub fn check_file_path_allowed(&self, file_path: &str) -> Result<(), LlmError> {
        let normalized_str = file_path.replace('\\', "/");
        let norm_path = Path::new(&normalized_str);

        for comp in norm_path.components() {
            if let Component::Normal(seg) = comp {
                let seg_lower = seg.to_string_lossy().to_lowercase();
                if matches!(seg_lower.as_str(), ".ssh" | ".aws" | ".gnupg") {
                    return Err(LlmError::OutboundBlocked(format!(
                        "包含敏感目录 '{}'，已触发 P0 防外发安全拦截！",
                        file_path
                    )));
                }
            }
        }

        if let Some(name) = norm_path.file_name() {
            let file_name = name.to_string_lossy().to_lowercase();
            if matches!(
                file_name.as_str(),
                ".env"
                    | "id_rsa"
                    | "id_ed25519"
                    | "id_ecdsa"
                    | "id_dsa"
                    | "id_rsa.pub"
                    | ".netrc"
                    | ".npmrc"
                    | ".pypirc"
            ) {
                return Err(LlmError::OutboundBlocked(format!(
                    "包含敏感配置文件 '{}'，已触发 P0 防外发安全拦截！",
                    file_path
                )));
            }

            if file_name.starts_with(".env.")
                && !matches!(
                    file_name.as_str(),
                    ".env.example" | ".env.sample" | ".env.template" | ".env.schema"
                )
            {
                return Err(LlmError::OutboundBlocked(format!(
                    "包含敏感环境配置文件 '{}'，已触发 P0 防外发安全拦截！",
                    file_path
                )));
            }
        }

        if let Some(ext) = norm_path.extension() {
            let ext_lower = ext.to_string_lossy().to_lowercase();
            if matches!(
                ext_lower.as_str(),
                "pem" | "key" | "pfx" | "p12" | "crt" | "cer" | "der" | "jks" | "keystore"
            ) {
                return Err(LlmError::OutboundBlocked(format!(
                    "包含敏感密钥或证书后缀 '{}'，已触发 P0 防外发安全拦截！",
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
                sanitized = re
                    .replace_all(&sanitized, "[SENSITIVE_REDACTED]")
                    .to_string();
            }
        }

        (sanitized, found)
    }

    pub fn inspect_outbound(
        &self,
        config: &LlmConfig,
        messages: &[ChatMessage],
    ) -> Result<OutboundPreview, LlmError> {
        let mut total_chars = 0;
        let mut has_sanitized = false;
        let mut all_content = String::new();

        for m in messages {
            total_chars += m.content.chars().count();
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

        let preview_snippet = if all_content.chars().count() > 500 {
            let truncated: String = all_content.chars().take(500).collect();
            format!("{}...\n[内容已截断预览]", truncated)
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

#[derive(Clone)]
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

    /// Builds a `reqwest::Client` honoring `config.proxy_url` (or system env `HTTPS_PROXY`/`HTTP_PROXY`)
    /// and `config.custom_ca_cert_path` (PEM root CA), without ever disabling TLS verification (Stage E3).
    pub fn build_http_client(&self, config: &LlmConfig) -> Result<reqwest::Client, LlmError> {
        if config.proxy_url.is_none() && config.custom_ca_cert_path.is_none() {
            return Ok(self.client.clone());
        }

        let mut builder = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(60));

        if let Some(ref proxy_str) = config.proxy_url {
            let trimmed = proxy_str.trim();
            if !trimmed.is_empty() {
                let proxy = reqwest::Proxy::all(trimmed).map_err(|e| {
                    LlmError::Api(format!("代理地址 '{}' 格式无效: {}", trimmed, e))
                })?;
                builder = builder.proxy(proxy);
            }
        }

        if let Some(ref ca_path) = config.custom_ca_cert_path {
            let trimmed = ca_path.trim();
            if !trimmed.is_empty() {
                let pem_bytes = fs::read(trimmed).map_err(|e| {
                    LlmError::Api(format!("无法读取企业根证书 PEM 文件 '{}': {}", trimmed, e))
                })?;
                let cert = reqwest::Certificate::from_pem(&pem_bytes).map_err(|e| {
                    LlmError::Api(format!("解析企业根证书 PEM 文件 '{}' 失败: {}", trimmed, e))
                })?;
                builder = builder.add_root_certificate(cert);
            }
        }

        builder.build().map_err(LlmError::Network)
    }

    pub async fn send_chat(
        &self,
        config: &LlmConfig,
        messages: Vec<ChatMessage>,
    ) -> Result<String, LlmError> {
        if config.offline_mode {
            return Err(LlmError::OutboundBlocked(
                "当前处于完全离线模式 (Offline Mode)，已禁止任何外部网络请求。".to_string(),
            ));
        }

        // 1. Unified P0 Outbound audit (single gate)
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

        // 3. Explicit error when API key is not configured (except local ollama)
        if config.api_key.trim().is_empty() && config.provider != "ollama" {
            return Err(LlmError::Api(
                "未配置模型 API Key。请点击右上角「设置」或在右侧「模型/安全」面板中配置有效的 API 密钥。"
                    .to_string(),
            ));
        }

        let client = self.build_http_client(config)?;
        let base = config.api_url.trim_end_matches('/');
        let url = if base.ends_with("/chat/completions") {
            base.to_string()
        } else {
            format!("{}/chat/completions", base)
        };
        let wants_json = config.json_mode
            && sanitized_messages.iter().any(|m| {
                m.get("role").and_then(|r| r.as_str()) == Some("system")
                    && m.get("content")
                        .and_then(|c| c.as_str())
                        .map(|c| c.contains("JSON") || c.contains("json"))
                        .unwrap_or(false)
            });

        let mut body_val = serde_json::json!({
            "model": config.model,
            "messages": sanitized_messages,
            "temperature": config.temperature,
            "max_tokens": config.max_tokens,
            "stream": false
        });

        if wants_json {
            body_val["response_format"] = serde_json::json!({ "type": "json_object" });
        }

        let mut req_builder = client.post(&url).header("Content-Type", "application/json");

        if !config.api_key.trim().is_empty() {
            req_builder =
                req_builder.header("Authorization", format!("Bearer {}", config.api_key.trim()));
        }

        let resp = req_builder.json(&body_val).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            // Fallback retry if gateway returns 400 regarding response_format
            if status.as_u16() == 400 && wants_json && err_text.contains("response_format") {
                body_val.as_object_mut().map(|obj| obj.remove("response_format"));
                let mut retry_builder = client.post(&url).header("Content-Type", "application/json");
                if !config.api_key.trim().is_empty() {
                    retry_builder = retry_builder.header("Authorization", format!("Bearer {}", config.api_key.trim()));
                }
                let retry_resp = retry_builder.json(&body_val).send().await?;
                if retry_resp.status().is_success() {
                    let res_json: serde_json::Value = retry_resp.json().await?;
                    let content = res_json["choices"][0]["message"]["content"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    return Ok(content);
                }
            }
            return Err(LlmError::Api(format!(
                "API 请求失败 (HTTP {}): {}",
                status.as_u16(),
                err_text
            )));
        }

        let res_json: serde_json::Value = resp.json().await?;
        let content = res_json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(content)
    }

    pub async fn send_chat_raw(
        &self,
        config: &LlmConfig,
        body_val: serde_json::Value,
    ) -> Result<serde_json::Value, LlmError> {
        if config.offline_mode {
            return Err(LlmError::OutboundBlocked(
                "当前处于完全离线模式 (Offline Mode)，已禁止任何外部网络请求。".to_string(),
            ));
        }
        if config.api_key.trim().is_empty() && config.provider != "ollama" {
            return Err(LlmError::Api(
                "未配置模型 API Key。请在右侧「模型/安全」面板中配置有效的 API 密钥。".to_string(),
            ));
        }
        let client = self.build_http_client(config)?;
        let base = config.api_url.trim_end_matches('/');
        let url = if base.ends_with("/chat/completions") {
            base.to_string()
        } else {
            format!("{}/chat/completions", base)
        };
        let mut req_builder = client.post(&url).header("Content-Type", "application/json");
        if !config.api_key.trim().is_empty() {
            req_builder =
                req_builder.header("Authorization", format!("Bearer {}", config.api_key.trim()));
        }
        let resp = req_builder.json(&body_val).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            return Err(LlmError::Api(format!(
                "API 请求失败 (HTTP {}): {}",
                status.as_u16(),
                err_text
            )));
        }
        let res_json: serde_json::Value = resp.json().await?;
        Ok(res_json)
    }

    pub async fn send_chat_stream<F>(
        &self,
        config: &LlmConfig,
        messages: Vec<ChatMessage>,
        mut on_delta: F,
    ) -> Result<String, LlmError>
    where
        F: FnMut(&str) + Send,
    {
        if config.offline_mode {
            return Err(LlmError::OutboundBlocked(
                "当前处于完全离线模式 (Offline Mode)，已禁止任何外部网络请求。".to_string(),
            ));
        }

        let _ = self.filter.inspect_outbound(config, &messages)?;

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

        if config.api_key.trim().is_empty() && config.provider != "ollama" {
            return Err(LlmError::Api(
                "未配置模型 API Key。请点击右上角「设置」或在右侧「模型/安全」面板中配置有效的 API 密钥。"
                    .to_string(),
            ));
        }

        let client = self.build_http_client(config)?;
        let base = config.api_url.trim_end_matches('/');
        let url = if base.ends_with("/chat/completions") {
            base.to_string()
        } else {
            format!("{}/chat/completions", base)
        };
        let body_val = serde_json::json!({
            "model": config.model,
            "messages": sanitized_messages,
            "temperature": config.temperature,
            "max_tokens": config.max_tokens,
            "stream": true
        });

        let mut req_builder = client.post(&url).header("Content-Type", "application/json");
        if !config.api_key.trim().is_empty() {
            req_builder =
                req_builder.header("Authorization", format!("Bearer {}", config.api_key.trim()));
        }

        let resp = req_builder.json(&body_val).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            return Err(LlmError::Api(format!(
                "API 请求失败 (HTTP {}): {}",
                status.as_u16(),
                err_text
            )));
        }

        let mut parser = crate::stream::SseByteParser::new();
        let mut full_text = String::new();

        use futures_util::StreamExt;
        let mut byte_stream = resp.bytes_stream();
        while let Some(chunk_res) = byte_stream.next().await {
            let chunk = chunk_res.map_err(LlmError::Network)?;
            let batch = parser.feed_bytes(&chunk).map_err(LlmError::Api)?;
            if !batch.content_delta.is_empty() {
                full_text.push_str(&batch.content_delta);
                on_delta(&batch.content_delta);
            }
        }
        let final_batch = parser.finish().map_err(LlmError::Api)?;
        if !final_batch.content_delta.is_empty() {
            full_text.push_str(&final_batch.content_delta);
            on_delta(&final_batch.content_delta);
        }

        Ok(full_text)
    }

    pub async fn test_connection(&self, config: &LlmConfig) -> ConnectionTestResult {
        if config.offline_mode {
            return ConnectionTestResult {
                ok: false,
                status_code: None,
                latency_ms: 0,
                category: "offline".to_string(),
                message: "当前处于离线模式，未发起外部请求。".to_string(),
            };
        }

        let client = match self.build_http_client(config) {
            Ok(c) => c,
            Err(e) => {
                return ConnectionTestResult {
                    ok: false,
                    status_code: None,
                    latency_ms: 0,
                    category: "config".to_string(),
                    message: format!("网络客户端初始化失败: {}", e),
                };
            }
        };

        let start = Instant::now();
        let base = config.api_url.trim_end_matches('/');
        let url = if base.ends_with("/chat/completions") {
            base.to_string()
        } else {
            format!("{}/chat/completions", base)
        };

        let body = serde_json::json!({
            "model": config.model,
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 4,
            "stream": false
        });

        let mut req_builder = client.post(&url).header("Content-Type", "application/json");
        if !config.api_key.trim().is_empty() {
            req_builder =
                req_builder.header("Authorization", format!("Bearer {}", config.api_key.trim()));
        }

        match req_builder.json(&body).send().await {
            Ok(resp) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                let code = resp.status().as_u16();
                if resp.status().is_success() {
                    ConnectionTestResult {
                        ok: true,
                        status_code: Some(code),
                        latency_ms,
                        category: "ok".to_string(),
                        message: format!(
                            "DNS 解析 ✓ | TCP/TLS 握手 ✓ | 鉴权 ✓ | 模型 `{}` 补全响应 ✓ (HTTP {}, {} ms)",
                            config.model, code, latency_ms
                        ),
                    }
                } else if code == 401 || code == 403 {
                    ConnectionTestResult {
                        ok: false,
                        status_code: Some(code),
                        latency_ms,
                        category: "auth".to_string(),
                        message: format!(
                            "TCP/TLS 握手 ✓ | 鉴权失败 (HTTP {})：请检查 API Key 是否有效",
                            code
                        ),
                    }
                } else if code == 404 {
                    ConnectionTestResult {
                        ok: false,
                        status_code: Some(code),
                        latency_ms,
                        category: "model_not_found".to_string(),
                        message: format!(
                            "网关连接 ✓ | 模型或接口路径不存在 (HTTP 404)：请核对 Base URL 与模型名 `{}`",
                            config.model
                        ),
                    }
                } else if code == 429 {
                    ConnectionTestResult {
                        ok: false,
                        status_code: Some(code),
                        latency_ms,
                        category: "rate_limit".to_string(),
                        message: "网关连接 ✓ | 触发速率限制 (HTTP 429)：请稍后重试".to_string(),
                    }
                } else {
                    let text = resp.text().await.unwrap_or_default();
                    ConnectionTestResult {
                        ok: false,
                        status_code: Some(code),
                        latency_ms,
                        category: "http_error".to_string(),
                        message: format!("服务端返回错误 (HTTP {}): {}", code, text),
                    }
                }
            }
            Err(err) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                let msg = err.to_string();
                let category = if err.is_timeout() {
                    "timeout"
                } else if msg.to_lowercase().contains("certificate")
                    || msg.to_lowercase().contains("tls")
                    || msg.to_lowercase().contains("ssl")
                {
                    "tls"
                } else if msg.to_lowercase().contains("proxy") {
                    "proxy"
                } else {
                    "network"
                };
                ConnectionTestResult {
                    ok: false,
                    status_code: None,
                    latency_ms,
                    category: category.to_string(),
                    message: format!("网络请求异常 ({}): {}", category, msg),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_p0_block_sensitive_files() {
        let filter = OutboundFilter::new();
        assert!(filter.check_file_path_allowed("src/main.rs").is_ok());
        assert!(filter.check_file_path_allowed("src/environment.py").is_ok());
        assert!(filter.check_file_path_allowed("src/monkey.py").is_ok());
        assert!(filter
            .check_file_path_allowed("src/auth/credentials_form.tsx")
            .is_ok());
        assert!(filter.check_file_path_allowed(".env.example").is_ok());
        assert!(filter.check_file_path_allowed(".env").is_err());
        assert!(filter.check_file_path_allowed("config/.env.local").is_err());
        assert!(filter.check_file_path_allowed("id_rsa").is_err());
        assert!(filter.check_file_path_allowed("server.key").is_err());
        assert!(filter.check_file_path_allowed("cert.pem").is_err());
        assert!(filter.check_file_path_allowed(".ssh/config").is_err());
    }

    #[test]
    fn test_code_referencing_process_env_is_not_blocked() {
        let filter = OutboundFilter::new();
        let config = LlmConfig::default();
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: r#"
                const apiUrl = process.env.API_URL || import.meta.env.VITE_API;
                // Check if id_rsa or .pem is mentioned in comments
                const envFile = ".env.example";
            "#
            .to_string(),
        }];
        let preview = filter.inspect_outbound(&config, &messages);
        assert!(
            preview.is_ok(),
            "Code discussing process.env or import.meta.env must not be blocked"
        );
    }

    #[test]
    fn test_secret_sanitization() {
        let filter = OutboundFilter::new();
        let fake_key = format!("sk-{}", "a1b2c3d4e5f6g7h8i9j0k1l2");
        let raw = format!(
            "Connect to 192.168.1.100 with api_key: '{}' and Bearer {}",
            fake_key, fake_key
        );
        let (sanitized, modified) = filter.sanitize_text(&raw);
        assert!(modified);
        assert!(!sanitized.contains("192.168.1.100"));
        assert!(!sanitized.contains(&fake_key));
        assert!(sanitized.contains("[SENSITIVE_REDACTED]"));
    }

    #[test]
    fn test_chinese_content_no_panic() {
        let filter = OutboundFilter::new();
        let config = LlmConfig::default();
        let chinese_text =
            "这是一段非常冗长的中文代码深度解读文本，包含各种复杂的术语、原理解析与架构剖析。"
                .repeat(50);
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: chinese_text,
        }];
        let res = filter.inspect_outbound(&config, &messages);
        assert!(res.is_ok());
        let preview = res.unwrap();
        assert!(preview.sanitized_preview.contains("[内容已截断预览]"));
    }

    #[test]
    fn test_credential_store_masks_api_key() {
        let temp_dir = std::env::temp_dir().join(format!(
            "noesis_cred_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = CredentialStore::new(Some(temp_dir.clone()));
        assert!(!store.get_config_view().has_api_key);

        let fake_secret = format!("sk-{}", "test_dummy_key_9876");
        let view = store.set_api_key(&fake_secret).unwrap();
        assert!(view.has_api_key);
        assert_eq!(view.api_key, "");
        assert_eq!(view.api_key_masked, "sk-****9876");

        let effective = store.resolve_effective_config(&view);
        assert_eq!(effective.api_key, fake_secret);

        let cleared = store.clear_api_key().unwrap();
        assert!(!cleared.has_api_key);
        assert_eq!(cleared.api_key_masked, "");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
