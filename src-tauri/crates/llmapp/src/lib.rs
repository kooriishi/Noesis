use noesis_indexer::{DomainAdapter, GraphBatch, IndexerError};
use noesis_store::{DbStore, GraphEdge, GraphEntity};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

const PROVIDERS_TOML: &str = include_str!("../rules/providers.toml");
const FRAMEWORKS_TOML: &str = include_str!("../rules/frameworks.toml");
const ENTRYPOINTS_TOML: &str = include_str!("../rules/entrypoints.toml");

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderRule {
    pub id: String,
    pub name: String,
    pub python_imports: Vec<String>,
    pub client_constructors: Vec<String>,
    pub call_methods: Vec<String>,
    pub env_keys: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProvidersConfig {
    providers: Vec<ProviderRule>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FrameworkRule {
    pub id: String,
    pub name: String,
    pub imports: Vec<String>,
    pub client_classes: Vec<String>,
    pub call_methods: Vec<String>,
    pub prompt_classes: Vec<String>,
    pub workflow_classes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct FrameworksConfig {
    frameworks: Vec<FrameworkRule>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EntrypointRule {
    pub id: String,
    pub kind: String,
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct EntrypointsConfig {
    entrypoints: Vec<EntrypointRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmAppWorkspaceSummary {
    pub entrypoints_count: usize,
    pub clients_count: usize,
    pub calls_count: usize,
    pub prompts_count: usize,
    pub tools_count: usize,
    pub workflows_count: usize,
    pub rag_components_count: usize,
    pub config_keys_count: usize,
}

pub struct LlmAppAdapter {
    pub providers: Vec<ProviderRule>,
    pub frameworks: Vec<FrameworkRule>,
    pub entrypoints: Vec<EntrypointRule>,
}

impl Default for LlmAppAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmAppAdapter {
    pub fn new() -> Self {
        let p_cfg: ProvidersConfig =
            toml::from_str(PROVIDERS_TOML).unwrap_or(ProvidersConfig { providers: vec![] });
        let f_cfg: FrameworksConfig =
            toml::from_str(FRAMEWORKS_TOML).unwrap_or(FrameworksConfig { frameworks: vec![] });
        let e_cfg: EntrypointsConfig =
            toml::from_str(ENTRYPOINTS_TOML).unwrap_or(EntrypointsConfig {
                entrypoints: vec![],
            });
        Self {
            providers: p_cfg.providers,
            frameworks: f_cfg.frameworks,
            entrypoints: e_cfg.entrypoints,
        }
    }

    pub fn analyze_workspace(
        &self,
        project_id: &str,
        root: &Path,
    ) -> Result<(GraphBatch, LlmAppWorkspaceSummary), IndexerError> {
        let mut batch = GraphBatch::default();
        let mut summary = LlmAppWorkspaceSummary::default();

        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                if e.depth() == 0 {
                    return true;
                }
                let name = e.file_name().to_string_lossy();
                !matches!(
                    name.as_ref(),
                    ".git"
                        | "node_modules"
                        | "target"
                        | "build"
                        | "install"
                        | "log"
                        | "dist"
                        | "__pycache__"
                        | ".venv"
                        | "venv"
                )
            })
            .flatten()
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            if !matches!(
                ext.as_str(),
                "py" | "cpp" | "cc" | "cxx" | "hpp" | "h" | "yaml" | "yml" | "prompt" | "jinja2"
            ) {
                continue;
            }

            let rel_path = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");

            let Ok(content) = fs::read_to_string(path) else {
                continue;
            };

            self.analyze_file_content(
                project_id,
                &rel_path,
                &ext,
                &content,
                &mut batch.entities,
                &mut batch.edges,
                &mut summary,
            );
        }

        Ok((batch, summary))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn analyze_file_content(
        &self,
        project_id: &str,
        rel_path: &str,
        ext: &str,
        content: &str,
        entities: &mut Vec<GraphEntity>,
        edges: &mut Vec<GraphEdge>,
        summary: &mut LlmAppWorkspaceSummary,
    ) {
        let file_id = format!("{}:{}", project_id, rel_path);

        let mut file_entrypoint_ids: Vec<(String, u32)> = Vec::new();
        let mut file_client_ids: Vec<(String, u32)> = Vec::new();
        let mut file_call_ids: Vec<(String, u32)> = Vec::new();
        let mut file_prompt_ids: Vec<(String, String, u32)> = Vec::new();
        let mut file_tool_ids: Vec<(String, String, u32)> = Vec::new();
        let mut file_config_ids: Vec<(String, String, u32)> = Vec::new();

        // 1. Dedicated prompt files (.prompt, .jinja2, or files inside prompts/ directory)
        if ext == "prompt" || ext == "jinja2" || rel_path.contains("prompts/") {
            let line_count = content.lines().count().max(1) as u32;
            let name = Path::new(rel_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("prompt_file")
                .to_string();
            let pid = format!("{}:llm_prompt:{}:1", project_id, rel_path);
            let vars = extract_template_variables(content);
            entities.push(GraphEntity {
                id: pid.clone(),
                project_id: project_id.to_string(),
                domain: "llm_app".to_string(),
                kind: "prompt_template".to_string(),
                name: name.clone(),
                file_id: file_id.clone(),
                file_path: rel_path.to_string(),
                symbol_id: None,
                line_start: 1,
                line_end: line_count,
                metadata_json: serde_json::json!({
                    "source": "prompt_file",
                    "variables": vars,
                    "char_len": content.len(),
                    "confidence": "exact",
                })
                .to_string(),
            });
            summary.prompts_count += 1;
            file_prompt_ids.push((pid, name, 1));
        }

        // 2. YAML config files (e.g., cascade.yaml / minicpm.yaml with system_prompt, api_key_env, endpoint, model)
        if ext == "yaml" || ext == "yml" {
            self.extract_from_yaml(
                project_id,
                rel_path,
                &file_id,
                content,
                entities,
                summary,
                &mut file_prompt_ids,
                &mut file_config_ids,
            );
            return;
        }

        // 3. Entrypoints (FastAPI routes, Gradio/Streamlit, CLI, ROS2 Voice LLM nodes)
        let fastapi_route_re = Regex::new(
            r#"@(?:app|router)\.(get|post|put|delete)\s*\(\s*["']([^"']+)["'][^\)]*\)\s*(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)"#,
        )
        .unwrap();
        for cap in fastapi_route_re.captures_iter(content) {
            let idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
            let line = content[..idx].lines().count().max(1) as u32;
            let method = cap[1].to_uppercase();
            let route_path = &cap[2];
            let handler_fn = &cap[3];
            let eid = format!("{}:llm_entry:{}:{}", project_id, rel_path, line);
            let name = format!("{} {} ({})", method, route_path, handler_fn);
            entities.push(GraphEntity {
                id: eid.clone(),
                project_id: project_id.to_string(),
                domain: "llm_app".to_string(),
                kind: "llm_entrypoint".to_string(),
                name,
                file_id: file_id.clone(),
                file_path: rel_path.to_string(),
                symbol_id: Some(handler_fn.to_string()),
                line_start: line,
                line_end: line + 10,
                metadata_json: serde_json::json!({
                    "entry_type": "http_route",
                    "http_method": method,
                    "route": route_path,
                    "handler": handler_fn,
                    "confidence": "exact",
                })
                .to_string(),
            });
            summary.entrypoints_count += 1;
            file_entrypoint_ids.push((eid, line));
        }

        for rule in &self.entrypoints {
            for pat in &rule.patterns {
                if pat.starts_with("@app.") || pat.starts_with("@router.") {
                    continue;
                }
                for (idx, line_str) in content.lines().enumerate() {
                    if line_str.contains(pat) {
                        let line = (idx + 1) as u32;
                        let eid = format!("{}:llm_entry:{}:{}", project_id, rel_path, line);
                        entities.push(GraphEntity {
                            id: eid.clone(),
                            project_id: project_id.to_string(),
                            domain: "llm_app".to_string(),
                            kind: "llm_entrypoint".to_string(),
                            name: format!("{}:{}", rule.id, pat.trim_end_matches('(')),
                            file_id: file_id.clone(),
                            file_path: rel_path.to_string(),
                            symbol_id: None,
                            line_start: line,
                            line_end: line,
                            metadata_json: serde_json::json!({
                                "entry_type": rule.kind,
                                "pattern": pat,
                                "confidence": "exact",
                            })
                            .to_string(),
                        });
                        summary.entrypoints_count += 1;
                        file_entrypoint_ids.push((eid, line));
                    }
                }
            }
        }

        // 4. Config keys (os.getenv / os.environ.get / std::getenv) — NEVER record secret values!
        let env_re = Regex::new(
            r#"(?:os\.getenv|os\.environ\.get|os\.environ\[|std::getenv)\s*\(?\s*["']([A-Za-z0-9_]+)["']"#,
        )
        .unwrap();
        for cap in env_re.captures_iter(content) {
            let idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
            let line = content[..idx].lines().count().max(1) as u32;
            let key_name = cap[1].to_string();
            let cid = format!("{}:config_key:{}:{}", project_id, key_name, line);
            entities.push(GraphEntity {
                id: cid.clone(),
                project_id: project_id.to_string(),
                domain: "llm_app".to_string(),
                kind: "config_key".to_string(),
                name: key_name.clone(),
                file_id: file_id.clone(),
                file_path: rel_path.to_string(),
                symbol_id: None,
                line_start: line,
                line_end: line,
                metadata_json: serde_json::json!({
                    "key": key_name,
                    "source": "env_lookup",
                    "confidence": "exact",
                })
                .to_string(),
            });
            summary.config_keys_count += 1;
            file_config_ids.push((cid, key_name, line));
        }

        // 5. LLM Clients (OpenAI, AsyncOpenAI, Anthropic, ChatOpenAI, QwenSseLlmProvider, MiniCpmRealtimeNode, etc.)
        let client_re = Regex::new(
            r"(?m)(?:([A-Za-z_][A-Za-z0-9_]*)\s*=\s*)?(OpenAI|AsyncOpenAI|AzureOpenAI|Anthropic|AsyncAnthropic|ChatOpenAI|ChatAnthropic|ChatOllama|ChatDeepSeek|QwenSseLlmProvider|DashScopeAsrProvider|CosyVoiceTtsProvider|MiniCpmRealtimeNode)\s*\(([^\)]*)\)",
        )
        .unwrap();
        for cap in client_re.captures_iter(content) {
            let idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
            let line = content[..idx].lines().count().max(1) as u32;
            let var_name = cap.get(1).map(|m| m.as_str()).unwrap_or("client");
            let ctor = &cap[2];
            let args = &cap[3];
            let base_url_expr = extract_kwarg(args, "base_url").unwrap_or_default();
            let model_expr = extract_kwarg(args, "model").unwrap_or_default();
            let api_key_source = if args.contains("getenv") || args.contains("environ") {
                "env"
            } else if args.contains("api_key") {
                "argument"
            } else {
                "default_env"
            };
            let cl_id = format!("{}:llm_client:{}:{}", project_id, rel_path, line);
            entities.push(GraphEntity {
                id: cl_id.clone(),
                project_id: project_id.to_string(),
                domain: "llm_app".to_string(),
                kind: "llm_client".to_string(),
                name: format!("{} ({})", var_name, ctor),
                file_id: file_id.clone(),
                file_path: rel_path.to_string(),
                symbol_id: Some(var_name.to_string()),
                line_start: line,
                line_end: line,
                metadata_json: serde_json::json!({
                    "constructor": ctor,
                    "variable": var_name,
                    "base_url_expr": base_url_expr,
                    "default_model": model_expr,
                    "api_key_source": api_key_source,
                    "confidence": "exact",
                })
                .to_string(),
            });
            summary.clients_count += 1;
            file_client_ids.push((cl_id, line));
        }

        // Also detect C++ class definitions for LLM providers (e.g. class QwenSseLlmProvider : public ILlmProvider)
        let cpp_provider_re = Regex::new(
            r"class\s+(QwenSseLlmProvider|DashScopeAsrProvider|CosyVoiceTtsProvider|MiniCpmRealtimeNode)\b",
        )
        .unwrap();
        for cap in cpp_provider_re.captures_iter(content) {
            let idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
            let line = content[..idx].lines().count().max(1) as u32;
            let cls = &cap[1];
            let cl_id = format!("{}:llm_client:{}:{}:{}", project_id, rel_path, cls, line);
            entities.push(GraphEntity {
                id: cl_id.clone(),
                project_id: project_id.to_string(),
                domain: "llm_app".to_string(),
                kind: "llm_client".to_string(),
                name: cls.to_string(),
                file_id: file_id.clone(),
                file_path: rel_path.to_string(),
                symbol_id: Some(cls.to_string()),
                line_start: line,
                line_end: line + 25,
                metadata_json: serde_json::json!({
                    "constructor": cls,
                    "language": "cpp",
                    "confidence": "exact",
                })
                .to_string(),
            });
            summary.clients_count += 1;
            file_client_ids.push((cl_id, line));
        }

        // 6. LLM Calls (chat.completions.create, messages.create, .invoke, .ainvoke, .stream, CompleteStream, SendAudioChunk)
        let call_re = Regex::new(
            r"(?s)(chat\.completions\.create|responses\.create|messages\.create|embeddings\.create|CompleteStream|SynthesizeStream|SendAudioChunk|\.(?:ainvoke|invoke|stream|astream))\s*\(([^\)]{0,600})\)",
        )
        .unwrap();
        for cap in call_re.captures_iter(content) {
            let idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
            let line = content[..idx].lines().count().max(1) as u32;
            let method = cap[1].trim_start_matches('.');
            let args = &cap[2];
            let model_expr = extract_kwarg(args, "model").unwrap_or_else(|| "dynamic".to_string());
            let stream_expr = extract_kwarg(args, "stream").unwrap_or_else(|| {
                if method.contains("Stream") || method.contains("stream") {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            });
            let temp_expr = extract_kwarg(args, "temperature");
            let tools_expr = extract_kwarg(args, "tools");

            let call_id = format!("{}:llm_call:{}:{}", project_id, rel_path, line);
            entities.push(GraphEntity {
                id: call_id.clone(),
                project_id: project_id.to_string(),
                domain: "llm_app".to_string(),
                kind: "llm_call".to_string(),
                name: format!("{} (L{})", method, line),
                file_id: file_id.clone(),
                file_path: rel_path.to_string(),
                symbol_id: None,
                line_start: line,
                line_end: line + args.lines().count() as u32,
                metadata_json: serde_json::json!({
                    "method": method,
                    "model": model_expr,
                    "stream": stream_expr,
                    "temperature": temp_expr,
                    "tools_expr": tools_expr,
                    "confidence": "exact",
                })
                .to_string(),
            });
            summary.calls_count += 1;
            file_call_ids.push((call_id, line));
        }

        // 7. Prompt Templates in code
        let prompt_var_re = Regex::new(
            r#"(?s)([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(?:f?"""([^"]{40,})"""|f?'''([^']{40,})'''|(?:ChatPromptTemplate|PromptTemplate)\.[A-Za-z_]+\(([^\)]{20,})\))"#,
        )
        .unwrap();
        for cap in prompt_var_re.captures_iter(content) {
            let idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
            let line = content[..idx].lines().count().max(1) as u32;
            let var_name = cap[1].to_string();
            let body = cap
                .get(2)
                .or_else(|| cap.get(3))
                .or_else(|| cap.get(4))
                .map(|m| m.as_str())
                .unwrap_or("");

            let lower_var = var_name.to_lowercase();
            let lower_body = body.to_lowercase();
            let is_prompt = lower_var.contains("prompt")
                || lower_var.contains("system")
                || lower_var.contains("instruction")
                || lower_body.contains("you are")
                || lower_body.contains("assistant")
                || lower_body.contains("只输出")
                || lower_body.contains("机器人")
                || cap[0].contains("PromptTemplate");

            if is_prompt {
                let pid = format!("{}:llm_prompt:{}:{}", project_id, rel_path, line);
                let vars = extract_template_variables(body);
                entities.push(GraphEntity {
                    id: pid.clone(),
                    project_id: project_id.to_string(),
                    domain: "llm_app".to_string(),
                    kind: "prompt_template".to_string(),
                    name: var_name.clone(),
                    file_id: file_id.clone(),
                    file_path: rel_path.to_string(),
                    symbol_id: Some(var_name.clone()),
                    line_start: line,
                    line_end: line + body.lines().count() as u32,
                    metadata_json: serde_json::json!({
                        "variable": var_name,
                        "variables": vars,
                        "preview": body.chars().take(120).collect::<String>(),
                        "confidence": if cap[0].contains("PromptTemplate") || lower_var.contains("prompt") { "exact" } else { "heuristic" },
                    })
                    .to_string(),
                });
                summary.prompts_count += 1;
                file_prompt_ids.push((pid, var_name, line));
            }
        }

        // 8. Tool Definitions
        let tool_dec_re = Regex::new(
            r#"@tool(?:\([^\)]*\))?\s*(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(([^\)]*)\)"#,
        )
        .unwrap();
        for cap in tool_dec_re.captures_iter(content) {
            let idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
            let line = content[..idx].lines().count().max(1) as u32;
            let fn_name = cap[1].to_string();
            let params = cap[2].trim().to_string();
            let tid = format!("{}:llm_tool:{}:{}", project_id, rel_path, line);
            entities.push(GraphEntity {
                id: tid.clone(),
                project_id: project_id.to_string(),
                domain: "llm_app".to_string(),
                kind: "tool_def".to_string(),
                name: fn_name.clone(),
                file_id: file_id.clone(),
                file_path: rel_path.to_string(),
                symbol_id: Some(fn_name.clone()),
                line_start: line,
                line_end: line + 8,
                metadata_json: serde_json::json!({
                    "style": "langchain_decorator",
                    "parameters": params,
                    "confidence": "exact",
                })
                .to_string(),
            });
            summary.tools_count += 1;
            file_tool_ids.push((tid, fn_name, line));
        }

        let openai_tool_re = Regex::new(
            r#"(?s)["']type["']\s*:\s*["']function["']\s*,\s*["']function["']\s*:\s*\{\s*["']name["']\s*:\s*["']([^"']+)["']"#,
        )
        .unwrap();
        for cap in openai_tool_re.captures_iter(content) {
            let idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
            let line = content[..idx].lines().count().max(1) as u32;
            let tool_name = cap[1].to_string();
            let tid = format!("{}:llm_tool:{}:{}", project_id, rel_path, line);
            entities.push(GraphEntity {
                id: tid.clone(),
                project_id: project_id.to_string(),
                domain: "llm_app".to_string(),
                kind: "tool_def".to_string(),
                name: tool_name.clone(),
                file_id: file_id.clone(),
                file_path: rel_path.to_string(),
                symbol_id: Some(tool_name.clone()),
                line_start: line,
                line_end: line + 6,
                metadata_json: serde_json::json!({
                    "style": "openai_json_schema",
                    "confidence": "exact",
                })
                .to_string(),
            });
            summary.tools_count += 1;
            file_tool_ids.push((tid, tool_name, line));
        }

        // 9. Agent Workflows
        let workflow_re = Regex::new(
            r"(?:class\s+(VoiceOrchestratorNode)\b|(StateGraph|MessageGraph|AgentExecutor|create_react_agent|create_tool_calling_agent)\s*\()",
        )
        .unwrap();
        for cap in workflow_re.captures_iter(content) {
            let idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
            let line = content[..idx].lines().count().max(1) as u32;
            let wf_kind = cap
                .get(1)
                .or_else(|| cap.get(2))
                .map(|m| m.as_str())
                .unwrap_or("Workflow");
            let wid = format!("{}:llm_workflow:{}:{}", project_id, rel_path, line);
            entities.push(GraphEntity {
                id: wid,
                project_id: project_id.to_string(),
                domain: "llm_app".to_string(),
                kind: "agent_workflow".to_string(),
                name: format!("{} ({})", wf_kind, rel_path),
                file_id: file_id.clone(),
                file_path: rel_path.to_string(),
                symbol_id: Some(wf_kind.to_string()),
                line_start: line,
                line_end: line + 15,
                metadata_json: serde_json::json!({
                    "workflow_type": wf_kind,
                    "confidence": "exact",
                })
                .to_string(),
            });
            summary.workflows_count += 1;
        }

        // 10. RAG Pipeline components
        let rag_re = Regex::new(
            r"\b(Chroma|FAISS|Qdrant|Milvus|RecursiveCharacterTextSplitter|OpenAIEmbeddings|HuggingFaceEmbeddings|DirectoryLoader|PyPDFLoader|TextLoader|as_retriever|similarity_search)\b",
        )
        .unwrap();
        for cap in rag_re.captures_iter(content) {
            let idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
            let line = content[..idx].lines().count().max(1) as u32;
            let comp = &cap[1];
            let rid = format!("{}:rag_comp:{}:{}:{}", project_id, rel_path, comp, line);
            entities.push(GraphEntity {
                id: rid,
                project_id: project_id.to_string(),
                domain: "llm_app".to_string(),
                kind: "rag_pipeline".to_string(),
                name: comp.to_string(),
                file_id: file_id.clone(),
                file_path: rel_path.to_string(),
                symbol_id: None,
                line_start: line,
                line_end: line,
                metadata_json: serde_json::json!({
                    "component": comp,
                    "confidence": "exact",
                })
                .to_string(),
            });
            summary.rag_components_count += 1;
        }

        // 11. Build semantic edges within the file:
        for (entry_id, entry_line) in &file_entrypoint_ids {
            for (call_id, call_line) in &file_call_ids {
                if call_line >= entry_line {
                    edges.push(GraphEdge {
                        id: format!("{}:routes_to:{}:{}", project_id, entry_id, call_id),
                        project_id: project_id.to_string(),
                        src_type: "entity".to_string(),
                        src_id: entry_id.clone(),
                        dst_type: "entity".to_string(),
                        dst_id: call_id.clone(),
                        kind: "routes_to".to_string(),
                        file_id: file_id.clone(),
                        file_path: rel_path.to_string(),
                        line: *call_line,
                        confidence: "heuristic".to_string(),
                        evidence: "entrypoint dispatches to LLM call".to_string(),
                    });
                }
            }
        }

        for (call_id, call_line) in &file_call_ids {
            for (prompt_id, prompt_name, _) in &file_prompt_ids {
                edges.push(GraphEdge {
                    id: format!("{}:uses_prompt:{}:{}", project_id, call_id, prompt_id),
                    project_id: project_id.to_string(),
                    src_type: "entity".to_string(),
                    src_id: call_id.clone(),
                    dst_type: "entity".to_string(),
                    dst_id: prompt_id.clone(),
                    kind: "uses_prompt".to_string(),
                    file_id: file_id.clone(),
                    file_path: rel_path.to_string(),
                    line: *call_line,
                    confidence: "heuristic".to_string(),
                    evidence: format!("uses prompt {}", prompt_name),
                });
            }
        }

        for (call_id, call_line) in &file_call_ids {
            for (tool_id, tool_name, _) in &file_tool_ids {
                edges.push(GraphEdge {
                    id: format!("{}:binds_tool:{}:{}", project_id, call_id, tool_id),
                    project_id: project_id.to_string(),
                    src_type: "entity".to_string(),
                    src_id: call_id.clone(),
                    dst_type: "entity".to_string(),
                    dst_id: tool_id.clone(),
                    kind: "binds_tool".to_string(),
                    file_id: file_id.clone(),
                    file_path: rel_path.to_string(),
                    line: *call_line,
                    confidence: "heuristic".to_string(),
                    evidence: format!("binds tool {}", tool_name),
                });
            }
        }

        for (client_id, client_line) in &file_client_ids {
            for (cfg_id, cfg_name, _) in &file_config_ids {
                edges.push(GraphEdge {
                    id: format!("{}:reads_config:{}:{}", project_id, client_id, cfg_id),
                    project_id: project_id.to_string(),
                    src_type: "entity".to_string(),
                    src_id: client_id.clone(),
                    dst_type: "entity".to_string(),
                    dst_id: cfg_id.clone(),
                    kind: "reads_config".to_string(),
                    file_id: file_id.clone(),
                    file_path: rel_path.to_string(),
                    line: *client_line,
                    confidence: "exact".to_string(),
                    evidence: format!("reads config {}", cfg_name),
                });
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn extract_from_yaml(
        &self,
        project_id: &str,
        rel_path: &str,
        file_id: &str,
        content: &str,
        entities: &mut Vec<GraphEntity>,
        summary: &mut LlmAppWorkspaceSummary,
        file_prompt_ids: &mut Vec<(String, String, u32)>,
        file_config_ids: &mut Vec<(String, String, u32)>,
    ) {
        for (idx, line_str) in content.lines().enumerate() {
            let line = (idx + 1) as u32;
            let trimmed = line_str.trim();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            if let Some((k, v)) = trimmed.split_once(':') {
                let key = k.trim();
                let val = v.trim().trim_matches('"').trim_matches('\'');

                if key == "system_prompt" {
                    let pid = format!("{}:llm_prompt:{}:{}", project_id, rel_path, line);
                    entities.push(GraphEntity {
                        id: pid.clone(),
                        project_id: project_id.to_string(),
                        domain: "llm_app".to_string(),
                        kind: "prompt_template".to_string(),
                        name: format!("system_prompt ({})", rel_path),
                        file_id: file_id.to_string(),
                        file_path: rel_path.to_string(),
                        symbol_id: None,
                        line_start: line,
                        line_end: line + 6,
                        metadata_json: serde_json::json!({
                            "source": "yaml_config",
                            "key": key,
                            "confidence": "exact",
                        })
                        .to_string(),
                    });
                    summary.prompts_count += 1;
                    file_prompt_ids.push((pid, "system_prompt".to_string(), line));
                } else if (key == "api_key_env"
                    || key == "ws_url"
                    || key == "endpoint"
                    || key == "model"
                    || (key == "url"
                        && (val.contains("/v1/")
                            || val.starts_with("ws://")
                            || val.starts_with("wss://"))))
                    && !val.is_empty()
                {
                    let cfg_name = if key == "api_key_env" {
                        val.to_string()
                    } else {
                        format!("{}:{}", key, val)
                    };
                    let cid =
                        format!("{}:config_key:{}:{}:{}", project_id, rel_path, key, line);
                    entities.push(GraphEntity {
                        id: cid.clone(),
                        project_id: project_id.to_string(),
                        domain: "llm_app".to_string(),
                        kind: "config_key".to_string(),
                        name: cfg_name.clone(),
                        file_id: file_id.to_string(),
                        file_path: rel_path.to_string(),
                        symbol_id: None,
                        line_start: line,
                        line_end: line,
                        metadata_json: serde_json::json!({
                            "yaml_key": key,
                            "env_var": if key == "api_key_env" { Some(val) } else { None },
                            "confidence": "exact",
                        })
                        .to_string(),
                    });
                    summary.config_keys_count += 1;
                    file_config_ids.push((cid, cfg_name, line));
                }
            }
        }
    }
}

impl DomainAdapter for LlmAppAdapter {
    fn domain_id(&self) -> &'static str {
        "llm_app"
    }

    fn detect(&self, root: &Path) -> bool {
        for entry in WalkDir::new(root)
            .max_depth(4)
            .follow_links(false)
            .into_iter()
            .flatten()
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let ext = entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            if matches!(ext, "py" | "cpp" | "hpp" | "yaml" | "yml" | "toml" | "txt") {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    if content.contains("openai")
                        || content.contains("anthropic")
                        || content.contains("langchain")
                        || content.contains("dashscope")
                        || content.contains("QwenSseLlmProvider")
                        || content.contains("system_prompt")
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn analyze_project(
        &self,
        project_id: &str,
        project_root: &Path,
        _store: &DbStore,
    ) -> Result<GraphBatch, IndexerError> {
        let (batch, _) = self.analyze_workspace(project_id, project_root)?;
        Ok(batch)
    }
}

fn extract_kwarg(args: &str, key: &str) -> Option<String> {
    let pattern = format!(r#"\b{}\s*=\s*([^,\)\n]+)"#, regex::escape(key));
    let re = Regex::new(&pattern).ok()?;
    let cap = re.captures(args)?;
    Some(cap[1].trim().to_string())
}

fn extract_template_variables(text: &str) -> Vec<String> {
    let re = Regex::new(r"\{\{\s*([A-Za-z_][A-Za-z0-9_]*)\s*\}\}|\{([A-Za-z_][A-Za-z0-9_]*)\}")
        .unwrap();
    let mut vars = Vec::new();
    for cap in re.captures_iter(text) {
        if let Some(m) = cap.get(1).or_else(|| cap.get(2)) {
            let v = m.as_str().to_string();
            if !vars.contains(&v) {
                vars.push(v);
            }
        }
    }
    vars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_python_fastapi_openai_rag_agent() {
        let py_src = r#"
import os
from fastapi import FastAPI
from openai import OpenAI
from langchain_core.tools import tool
from langgraph.graph import StateGraph
from langchain_community.vectorstores import FAISS

app = FastAPI()
api_key = os.getenv("OPENAI_API_KEY")
client = OpenAI(api_key=api_key, base_url="https://api.deepseek.com/v1")

SYSTEM_PROMPT = """
You are an expert code learning assistant for ROS2 and LLM applications.
Please explain {{symbol_name}} in detail for a {{user_level}} engineer.
"""

@tool
def search_docs(query: str) -> str:
    """Search documentation."""
    return "result"

@app.post("/api/chat")
async def chat_endpoint(user_msg: str):
    vectorstore = FAISS.from_documents(docs, embeddings)
    workflow = StateGraph(AgentState)
    resp = client.chat.completions.create(
        model="deepseek-chat",
        stream=True,
        temperature=0.2,
        tools=[search_docs],
        messages=[{"role": "system", "content": SYSTEM_PROMPT}]
    )
    return resp
"#;

        let adapter = LlmAppAdapter::new();
        let mut entities = Vec::new();
        let mut edges = Vec::new();
        let mut summary = LlmAppWorkspaceSummary::default();

        adapter.analyze_file_content(
            "test_proj",
            "app/main.py",
            "py",
            py_src,
            &mut entities,
            &mut edges,
            &mut summary,
        );

        assert!(summary.entrypoints_count >= 1, "Should detect FastAPI route");
        assert_eq!(summary.clients_count, 1, "Should detect OpenAI client");
        assert_eq!(summary.calls_count, 1, "Should detect chat.completions.create");
        assert_eq!(summary.prompts_count, 1, "Should detect SYSTEM_PROMPT");
        assert_eq!(summary.tools_count, 1, "Should detect @tool search_docs");
        assert_eq!(summary.workflows_count, 1, "Should detect StateGraph");
        assert!(summary.rag_components_count >= 1, "Should detect FAISS");
        assert_eq!(summary.config_keys_count, 1, "Should detect OPENAI_API_KEY");

        assert!(edges.iter().any(|e| e.kind == "routes_to"));
        assert!(edges.iter().any(|e| e.kind == "uses_prompt"));
        assert!(edges.iter().any(|e| e.kind == "binds_tool"));
        assert!(edges.iter().any(|e| e.kind == "reads_config"));
    }

    #[test]
    fn test_analyze_robot_voice_ws_llm_components() {
        let sample_path = Path::new(r"C:\Users\123\Desktop\RobotVoiceWS-main");
        if !sample_path.exists() {
            return;
        }
        let adapter = LlmAppAdapter::new();
        assert!(adapter.detect(sample_path));

        let (batch, summary) = adapter
            .analyze_workspace("robot_voice_ws", sample_path)
            .unwrap();
        assert!(summary.clients_count >= 2, "Should detect QwenSseLlmProvider and MiniCpm");
        assert!(summary.prompts_count >= 1, "Should detect system_prompt in cascade.yaml");
        assert!(summary.config_keys_count >= 1, "Should detect DASHSCOPE_API_KEY in cascade.yaml");
        assert!(!batch.entities.is_empty());
    }
}
