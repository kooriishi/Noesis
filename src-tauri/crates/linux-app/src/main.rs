use noesis_indexer::{CodeIndexer, DomainAdapter, ProjectMap};
use noesis_learn::context::{
    ContextAssembler, ContextManifest, ContextRequest, DroppedItem, HeuristicTokenEstimator,
    PROMPT_VERSION,
};
use noesis_learn::exercises::{
    compute_sm2_review, generate_cloze_exercise, generate_fact_exercises,
    grade_deterministic_answer, map_answer_to_quality, refresh_project_exercises_freshness,
    Sm2ReviewState,
};
use noesis_learn::knowledge::KnowledgeBase;
use noesis_learn::verifier::{
    extract_and_parse_explanation_json, CitationVerifier, StructuredExplanation,
    VerificationSummary,
};
use noesis_learn::LearnEngine;
use noesis_llm::stream::CancellationRegistry;
use noesis_llm::{ChatMessage, CredentialStore, LlmConfig, LlmGateway};
use noesis_llmapp::LlmAppAdapter;
use noesis_lsp::LspManager;
use noesis_project::ProjectManager;
use noesis_ros2::Ros2Adapter;
use noesis_runner::{CodeRunner, RunRequest};
use noesis_store::{CachedExplanation, ConfusionMark, DbStore, StoredMessageRecord};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const EXPERT_SYSTEM_PROMPT: &str = include_str!("../../../../prompts/code_guide_expert.system.md");
const DIST_INDEX_HTML: &str = include_str!("../../../../dist/index.html");
const DIST_CSS: &[u8] = include_bytes!("../../../../dist/assets/index-CbmUPwi2.css");
const DIST_JS: &[u8] = include_bytes!("../../../../dist/assets/index-iEFAoPzU.js");

const IPC_SHIM_SCRIPT: &str = r#"<script>
window.__TAURI_INTERNALS__ = {
  invoke: async function(cmd, args) {
    const res = await fetch('/__noesis_ipc__', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ cmd: cmd, args: args || {} })
    });
    const payload = await res.json();
    if (!res.ok || payload.error) {
      throw new Error(payload.error || 'Noesis IPC Error');
    }
    return payload.result;
  },
  transformCallback: function() { return 0; },
  metadata: {
    currentWindow: { label: 'main' },
    currentWebview: { label: 'main', windowLabel: 'main' }
  }
};
</script>"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub db_degraded: bool,
    pub db_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredExplainResponse {
    pub explanation: Option<StructuredExplanation>,
    pub verification: VerificationSummary,
    pub manifest: ContextManifest,
    pub dropped: Vec<DroppedItem>,
    pub cached: bool,
    pub raw_unverified_output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitExerciseResult {
    pub is_correct: bool,
    pub quality: u8,
    pub review: Sm2ReviewState,
    pub reference_answer: String,
    pub verified_by_program: bool,
}

pub struct AppState {
    pub project_manager: Mutex<Option<ProjectManager>>,
    pub project_map: Mutex<Option<ProjectMap>>,
    pub store: DbStore,
    pub db_degraded: bool,
    pub db_error: Option<String>,
    pub credentials: CredentialStore,
    pub llm: LlmGateway,
    pub indexer: CodeIndexer,
    pub knowledge_base: KnowledgeBase,
    pub lsp: LspManager,
    pub cancellations: CancellationRegistry,
}

#[derive(Deserialize)]
struct IpcEnvelope {
    cmd: String,
    #[serde(default)]
    args: Value,
}

fn select_linux_directory() -> Option<String> {
    if let Ok(out) = Command::new("zenity")
        .args(["--file-selection", "--directory", "--title=选择要分析的代码仓库根目录"])
        .output()
    {
        if out.status.success() {
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !p.is_empty() {
                return Some(p);
            }
        }
    }
    if let Ok(out) = Command::new("kdialog")
        .args(["--getexistingdirectory", "."])
        .output()
    {
        if out.status.success() {
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !p.is_empty() {
                return Some(p);
            }
        }
    }
    None
}

async fn dispatch_command(state: &AppState, cmd: &str, args: Value) -> Result<Value, String> {
    match cmd {
        "plugin:dialog|open" => {
            if let Some(dir) = select_linux_directory() {
                Ok(Value::String(dir))
            } else {
                Err("No native dialog utility (zenity/kdialog) selected a directory".to_string())
            }
        }
        "get_system_status" => Ok(serde_json::to_value(SystemStatus {
            db_degraded: state.db_degraded,
            db_error: state.db_error.clone(),
        })
        .unwrap()),
        "get_expert_system_prompt" => Ok(Value::String(EXPERT_SYSTEM_PROMPT.to_string())),
        "get_llm_config" => Ok(serde_json::to_value(state.credentials.get_config_view()).unwrap()),
        "set_llm_config" => {
            let cfg: LlmConfig = serde_json::from_value(args["config"].clone())
                .map_err(|e| e.to_string())?;
            let res = state.credentials.update_config(cfg).map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "set_api_key" => {
            let key = args["apiKey"]
                .as_str()
                .or_else(|| args["api_key"].as_str())
                .unwrap_or_default();
            let res = state.credentials.set_api_key(key).map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "clear_api_key" => {
            let res = state.credentials.clear_api_key().map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "test_llm_connection" => {
            let cfg: LlmConfig = serde_json::from_value(args["config"].clone())
                .map_err(|e| e.to_string())?;
            let effective = state.credentials.resolve_effective_config(&cfg);
            let res = state.llm.test_connection(&effective).await;
            Ok(serde_json::to_value(res).unwrap())
        }
        "cancel_llm_request" => {
            let req_id = args["requestId"]
                .as_str()
                .or_else(|| args["request_id"].as_str())
                .unwrap_or_default();
            Ok(Value::Bool(state.cancellations.cancel(req_id)))
        }
        "open_project" => {
            let path = args["path"].as_str().unwrap_or("./");
            let pm = ProjectManager::new(path).map_err(|e| e.to_string())?;
            let tree = pm.build_tree(2).map_err(|e| e.to_string())?;
            let mut guard = state.project_manager.lock().map_err(|e| e.to_string())?;
            *guard = Some(pm);
            Ok(serde_json::to_value(tree).unwrap())
        }
        "list_project_dir" => {
            let rel_dir = args["relDir"]
                .as_str()
                .or_else(|| args["rel_dir"].as_str())
                .unwrap_or_default();
            let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
            let pm = pm_guard.as_ref().ok_or("尚未打开任何工程")?;
            let list = pm.list_dir(rel_dir).map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(list).unwrap())
        }
        "get_project_tree" => {
            let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
            if let Some(pm) = pm_guard.as_ref() {
                let tree = pm.build_tree(6).map_err(|e| e.to_string())?;
                Ok(serde_json::to_value(Some(tree)).unwrap())
            } else {
                Ok(Value::Null)
            }
        }
        "read_file" => {
            let path = args["path"].as_str().unwrap_or_default();
            let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
            let pm = pm_guard
                .as_ref()
                .ok_or("尚未打开任何工程，请先在左侧选择工程目录。")?;
            let payload = pm.read_file(path).map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(payload).unwrap())
        }
        "save_file" => {
            let path = args["path"].as_str().unwrap_or_default();
            let content = args["content"].as_str().unwrap_or_default();
            let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
            let pm = pm_guard
                .as_ref()
                .ok_or("尚未打开任何工程，无法保存文件。")?;
            let hash = pm.save_file(path, content).map_err(|e| e.to_string())?;
            Ok(Value::String(hash))
        }
        "index_project" => {
            let (root_path, project_id) = {
                let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
                let pm = pm_guard.as_ref().ok_or("No project opened yet")?;
                (pm.root_path().to_path_buf(), pm.normalized_root())
            };
            let _ = state.store.ensure_schema();
            let ros2_adapter = Ros2Adapter::new();
            let llm_adapter = LlmAppAdapter::new();
            let adapters: [&dyn DomainAdapter; 2] = [&ros2_adapter, &llm_adapter];
            let (map, _report) = state
                .indexer
                .index_workspace(&root_path, &state.store, &adapters)
                .map_err(|e| e.to_string())?;
            refresh_project_exercises_freshness(&root_path, &state.store, &project_id);
            let mut map_guard = state.project_map.lock().map_err(|e| e.to_string())?;
            *map_guard = Some(map.clone());
            Ok(serde_json::to_value(map).unwrap())
        }
        "get_project_map" => {
            let map = state.project_map.lock().map_err(|e| e.to_string())?.clone();
            Ok(serde_json::to_value(map).unwrap())
        }
        "set_reading_step_status" => {
            let step_id = args["stepId"]
                .as_str()
                .or_else(|| args["step_id"].as_str())
                .unwrap_or_default();
            let status = args["status"].as_str().unwrap_or("unread");
            let project_id = {
                let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
                let pm = pm_guard.as_ref().ok_or("No project opened yet")?;
                pm.normalized_root()
            };
            state
                .store
                .set_reading_progress(&project_id, step_id, status)
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "get_reading_progress" => {
            let project_id = {
                let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
                let pm = pm_guard.as_ref().ok_or("No project opened yet")?;
                pm.normalized_root()
            };
            let res = state
                .store
                .get_reading_progress(&project_id)
                .map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "get_symbol_neighborhood" => {
            let symbol_id = args["symbolId"]
                .as_str()
                .or_else(|| args["symbol_id"].as_str())
                .unwrap_or_default();
            let project_id = {
                let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
                let pm = pm_guard.as_ref().ok_or("No project opened yet")?;
                pm.normalized_root()
            };
            let res = state
                .store
                .get_symbol_with_neighbors(&project_id, symbol_id)
                .map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "list_project_symbols" => {
            let file_path = args["filePath"]
                .as_str()
                .or_else(|| args["file_path"].as_str());
            let project_id = {
                let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
                let pm = pm_guard.as_ref().ok_or("No project opened yet")?;
                pm.normalized_root()
            };
            let res = state
                .store
                .list_project_symbols(&project_id, file_path)
                .map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "explain_code" => {
            let file_path = args["filePath"]
                .as_str()
                .or_else(|| args["file_path"].as_str())
                .unwrap_or_default();
            let snippet = args["snippet"].as_str().unwrap_or_default();
            let start_line = args["startLine"]
                .as_u64()
                .or_else(|| args["start_line"].as_u64())
                .unwrap_or(1) as u32;
            let end_line = args["endLine"]
                .as_u64()
                .or_else(|| args["end_line"].as_u64())
                .unwrap_or(1) as u32;
            let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
            let source_matches = if let Some(pm) = pm_guard.as_ref() {
                if let Ok(payload) = pm.read_file(file_path) {
                    LearnEngine::verify_source_reference(
                        &payload.content,
                        start_line,
                        end_line,
                        snippet,
                    )
                } else {
                    false
                }
            } else {
                false
            };
            let cards = LearnEngine::generate_structured_cards(
                file_path,
                snippet,
                start_line,
                end_line,
                source_matches,
            );
            Ok(serde_json::to_value(cards).unwrap())
        }
        "explain_selection_structured" => {
            let config: LlmConfig = serde_json::from_value(args["config"].clone())
                .map_err(|e| e.to_string())?;
            let file_path = args["filePath"]
                .as_str()
                .or_else(|| args["file_path"].as_str())
                .unwrap_or_default()
                .to_string();
            let snippet = args["snippet"].as_str().unwrap_or_default().to_string();
            let start_line = args["startLine"]
                .as_u64()
                .or_else(|| args["start_line"].as_u64())
                .unwrap_or(1) as u32;
            let end_line = args["endLine"]
                .as_u64()
                .or_else(|| args["end_line"].as_u64())
                .unwrap_or(1) as u32;
            let force_refresh = args["forceRefresh"]
                .as_bool()
                .or_else(|| args["force_refresh"].as_bool())
                .unwrap_or(false);

            let (root_path, project_id) = {
                let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
                if let Some(pm) = pm_guard.as_ref() {
                    (pm.root_path().to_path_buf(), pm.normalized_root())
                } else {
                    (PathBuf::from("."), "default".to_string())
                }
            };
            let effective = state.credentials.resolve_effective_config(&config);
            let estimator = HeuristicTokenEstimator;
            let ctx_req = ContextRequest {
                kind: "explain_selection".to_string(),
                project_id: project_id.clone(),
                file: Some(file_path.clone()),
                range: Some((start_line, end_line)),
                selected_code: Some(snippet),
                question: None,
                entity_ref: None,
                conversation_id: None,
                budget_tokens: 16000,
                include_learning_profile: true,
                re_explain_type: None,
                re_explain_level: None,
                previous_explanation_summary: None,
                user_note: None,
            };
            let pack = ContextAssembler::assemble(
                &root_path,
                Some(&state.store),
                Some(&state.knowledge_base),
                &ctx_req,
                &estimator,
                &effective.model,
            );
            if !force_refresh {
                if let Ok(Some(cached)) = state.store.get_cached_explanation(
                    &pack.basis_hash,
                    &effective.model,
                    PROMPT_VERSION,
                ) {
                    if let Ok(mut exp) = serde_json::from_str::<StructuredExplanation>(&cached.json)
                    {
                        let verification = CitationVerifier::verify(
                            &root_path,
                            &pack.manifest.loaded_knowledge_packs,
                            &mut exp,
                        );
                        return Ok(serde_json::to_value(StructuredExplainResponse {
                            explanation: Some(exp),
                            verification,
                            manifest: pack.manifest,
                            dropped: pack.dropped,
                            cached: true,
                            raw_unverified_output: None,
                        })
                        .unwrap());
                    }
                }
            }
            let combined_prompt = pack
                .items
                .iter()
                .map(|i| i.text.clone())
                .collect::<Vec<_>>()
                .join("\n\n");
            let messages = vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: EXPERT_SYSTEM_PROMPT.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: combined_prompt,
                },
            ];
            let raw_reply = state
                .llm
                .send_chat(&effective, messages)
                .await
                .map_err(|e| e.to_string())?;
            match extract_and_parse_explanation_json(&raw_reply) {
                Ok(mut exp) => {
                    let verification = CitationVerifier::verify(
                        &root_path,
                        &pack.manifest.loaded_knowledge_packs,
                        &mut exp,
                    );
                    if let Ok(json_str) = serde_json::to_string(&exp) {
                        let stats_str = serde_json::to_string(&verification).unwrap_or_default();
                        let _ = state.store.save_explanation_cache(&CachedExplanation {
                            id: format!("exp-{}", pack.basis_hash),
                            project_id,
                            file_id: file_path,
                            range: format!("{}-{}", start_line, end_line),
                            basis_hash: pack.basis_hash,
                            prompt_version: PROMPT_VERSION.to_string(),
                            model: effective.model.clone(),
                            request_kind: "explain_selection".to_string(),
                            json: json_str,
                            stats: stats_str,
                            created_at: chrono::Utc::now().timestamp(),
                        });
                    }
                    Ok(serde_json::to_value(StructuredExplainResponse {
                        explanation: Some(exp),
                        verification,
                        manifest: pack.manifest,
                        dropped: pack.dropped,
                        cached: false,
                        raw_unverified_output: None,
                    })
                    .unwrap())
                }
                Err(_) => Ok(serde_json::to_value(StructuredExplainResponse {
                    explanation: None,
                    verification: VerificationSummary::default(),
                    manifest: pack.manifest,
                    dropped: pack.dropped,
                    cached: false,
                    raw_unverified_output: Some(raw_reply),
                })
                .unwrap()),
            }
        }
        "mark_confusion" => {
            let file_path = args["filePath"]
                .as_str()
                .or_else(|| args["file_path"].as_str())
                .unwrap_or_default()
                .to_string();
            let start_line = args["startLine"]
                .as_u64()
                .or_else(|| args["start_line"].as_u64())
                .unwrap_or(1) as u32;
            let end_line = args["endLine"]
                .as_u64()
                .or_else(|| args["end_line"].as_u64())
                .unwrap_or(1) as u32;
            let snippet = args["snippet"].as_str().unwrap_or_default().to_string();
            let category = args["category"].as_str().unwrap_or("logic").to_string();
            let user_note = args["userNote"]
                .as_str()
                .or_else(|| args["user_note"].as_str())
                .map(|s| s.to_string());
            let mut rewording = LearnEngine::handle_confusion_mark(&category, &snippet);
            let project_id = {
                let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
                pm_guard.as_ref().map(|pm| pm.normalized_root())
            };
            let mark_id = format!(
                "mark-{}-{}",
                chrono::Utc::now().timestamp_millis(),
                start_line
            );
            rewording.mark_id = Some(mark_id.clone());
            let mark = ConfusionMark {
                id: mark_id,
                project_id,
                file_path,
                line_start: start_line,
                line_end: end_line,
                code_snippet: snippet,
                category: category.clone(),
                confusion_type: Some(category),
                explanation: Some(rewording.analogy.clone()),
                ai_explanation: None,
                model: None,
                level: 1,
                resolved_at: None,
                resolved_level: None,
                user_note,
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            let _ = state.store.save_mark(&mark);
            Ok(serde_json::to_value(rewording).unwrap())
        }
        "update_confusion_mark_explanation" => {
            let id = args["id"].as_str().unwrap_or_default();
            let ai_exp = args["aiExplanation"]
                .as_str()
                .or_else(|| args["ai_explanation"].as_str())
                .unwrap_or_default();
            let model = args["model"].as_str().unwrap_or_default();
            state
                .store
                .update_mark_ai_explanation(id, ai_exp, model)
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "resolve_confusion_mark" => {
            let id = args["id"].as_str().unwrap_or_default();
            let level = args["level"].as_i64().unwrap_or(1) as i32;
            let resolved = args["resolved"].as_bool().unwrap_or(true);
            state
                .store
                .update_mark_level_and_resolution(id, level, resolved)
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "get_confusion_marks" => {
            let file_path = args["filePath"]
                .as_str()
                .or_else(|| args["file_path"].as_str());
            let project_id = {
                let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
                pm_guard.as_ref().map(|pm| pm.normalized_root())
            };
            let marks = state
                .store
                .list_marks(file_path, project_id.as_deref())
                .map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(marks).unwrap())
        }
        "get_learning_profile" => {
            let project_id = {
                let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
                pm_guard.as_ref().map(|pm| pm.normalized_root())
            };
            let items = state
                .store
                .list_concept_profile(project_id.as_deref())
                .map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(items).unwrap())
        }
        "list_conversations" => {
            let project_id = {
                let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
                pm_guard.as_ref().map(|pm| pm.normalized_root())
            };
            let list = state
                .store
                .list_conversations(project_id.as_deref())
                .map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(list).unwrap())
        }
        "create_conversation" => {
            let title = args["title"].as_str().unwrap_or("新对话");
            let project_id = {
                let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
                pm_guard
                    .as_ref()
                    .map(|pm| pm.normalized_root())
                    .unwrap_or_else(|| "default".to_string())
            };
            let id = format!("conv-{}", chrono::Utc::now().timestamp_millis());
            let rec = state
                .store
                .create_conversation(&id, &project_id, title)
                .map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(rec).unwrap())
        }
        "load_conversation_messages" => {
            let conv_id = args["conversationId"]
                .as_str()
                .or_else(|| args["conversation_id"].as_str())
                .unwrap_or_default();
            let limit = args["limit"].as_u64().unwrap_or(50) as usize;
            let msgs = state
                .store
                .list_messages(conv_id, limit)
                .map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(msgs).unwrap())
        }
        "append_conversation_message" => {
            let conv_id = args["conversationId"]
                .as_str()
                .or_else(|| args["conversation_id"].as_str())
                .unwrap_or_default()
                .to_string();
            let role = args["role"].as_str().unwrap_or("user").to_string();
            let content = args["content"].as_str().unwrap_or_default().to_string();
            let meta_json = args["metaJson"]
                .as_str()
                .or_else(|| args["meta_json"].as_str())
                .unwrap_or("{}")
                .to_string();
            let est = HeuristicTokenEstimator;
            let tokens = noesis_learn::context::TokenEstimator::estimate(&est, &content);
            let msg = StoredMessageRecord {
                id: format!("msg-{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)),
                conversation_id: conv_id,
                role,
                content,
                meta_json,
                tokens,
                created_at: chrono::Utc::now().timestamp(),
            };
            state
                .store
                .append_message(&msg)
                .map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(msg).unwrap())
        }
        "delete_conversation" => {
            let id = args["id"].as_str().unwrap_or_default();
            state
                .store
                .delete_conversation(id)
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "preview_outbound" => {
            let cfg: LlmConfig = serde_json::from_value(args["config"].clone())
                .map_err(|e| e.to_string())?;
            let messages: Vec<ChatMessage> = serde_json::from_value(args["messages"].clone())
                .map_err(|e| e.to_string())?;
            let effective = state.credentials.resolve_effective_config(&cfg);
            let prev = state
                .llm
                .filter()
                .inspect_outbound(&effective, &messages)
                .map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(prev).unwrap())
        }
        "send_llm_chat" => {
            let cfg: LlmConfig = serde_json::from_value(args["config"].clone())
                .map_err(|e| e.to_string())?;
            let messages: Vec<ChatMessage> = serde_json::from_value(args["messages"].clone())
                .map_err(|e| e.to_string())?;
            let effective = state.credentials.resolve_effective_config(&cfg);
            let reply = state
                .llm
                .send_chat(&effective, messages)
                .await
                .map_err(|e| e.to_string())?;
            Ok(Value::String(reply))
        }
        "run_scratch_code" => {
            let req: RunRequest = serde_json::from_value(args["req"].clone())
                .map_err(|e| e.to_string())?;
            let res = CodeRunner::execute(req).await.map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "generate_project_exercises" => {
            let count = args["count"].as_u64().unwrap_or(5) as usize;
            let file_path = args["filePath"]
                .as_str()
                .or_else(|| args["file_path"].as_str());
            let snippet = args["snippet"].as_str();
            let project_id = {
                let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
                pm_guard
                    .as_ref()
                    .map(|pm| pm.normalized_root())
                    .unwrap_or_else(|| "default".to_string())
            };
            let entities = state
                .store
                .get_entities_by_domain(&project_id, None, None)
                .unwrap_or_default();
            let edges = state
                .store
                .get_edges_filtered(&project_id, None)
                .unwrap_or_default();
            let symbols = state
                .store
                .list_project_symbols(&project_id, None)
                .unwrap_or_default();
            let mut generated =
                generate_fact_exercises(&project_id, &entities, &edges, &symbols, count);
            if let (Some(fp), Some(code)) = (file_path, snippet) {
                if let Some(cloze) = generate_cloze_exercise(&project_id, fp, code, None) {
                    generated.push(cloze);
                }
            }
            for ex in &generated {
                let _ = state.store.insert_exercise(ex);
            }
            Ok(serde_json::to_value(generated).unwrap())
        }
        "submit_exercise_answer" => {
            let exercise_id = args["exerciseId"]
                .as_str()
                .or_else(|| args["exercise_id"].as_str())
                .unwrap_or_default();
            let answer = args["answer"].as_str().unwrap_or_default();
            let hesitation = args["hesitation"].as_str();
            let due = state.store.list_due_exercises().map_err(|e| e.to_string())?;
            let Some(mut ex) = due.into_iter().find(|e| e.id == exercise_id) else {
                return Err(format!("未找到练习题 '{}'", exercise_id));
            };
            let is_correct = grade_deterministic_answer(answer, &ex.reference_answer);
            let quality = map_answer_to_quality(&ex.source_kind, is_correct, hesitation, None);
            let review =
                compute_sm2_review(quality, ex.ease, ex.interval_days, ex.repetition, ex.lapses);
            ex.ease = review.ease;
            ex.interval_days = review.interval_days;
            ex.repetition = review.repetitions;
            ex.lapses = review.lapses;
            ex.next_review_at = review.next_review_at.clone();
            state.store.insert_exercise(&ex).map_err(|e| e.to_string())?;
            let _ = state.store.record_exercise_attempt(
                &ex.id,
                answer,
                if is_correct { "correct" } else { "wrong" },
                if is_correct { 1.0 } else { 0.0 },
            );
            Ok(serde_json::to_value(SubmitExerciseResult {
                is_correct,
                quality,
                review,
                reference_answer: ex.reference_answer,
                verified_by_program: ex.verified_by_program,
            })
            .unwrap())
        }
        "list_due_reviews" => {
            let res = state.store.list_due_exercises().map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "detect_lsp_servers" => Ok(serde_json::to_value(state.lsp.detect_servers()).unwrap()),
        "lsp_find_definition" => {
            let sym = args["symbolName"]
                .as_str()
                .or_else(|| args["symbol_name"].as_str())
                .unwrap_or_default();
            let project_id = {
                let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
                pm_guard
                    .as_ref()
                    .map(|pm| pm.normalized_root())
                    .unwrap_or_default()
            };
            Ok(serde_json::to_value(state.lsp.resolve_definition(
                Some(&state.store),
                &project_id,
                sym,
            ))
            .unwrap())
        }
        "lsp_find_references" => {
            let sym = args["symbolName"]
                .as_str()
                .or_else(|| args["symbol_name"].as_str())
                .unwrap_or_default();
            let project_id = {
                let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
                pm_guard
                    .as_ref()
                    .map(|pm| pm.normalized_root())
                    .unwrap_or_default()
            };
            Ok(serde_json::to_value(state.lsp.resolve_references(
                Some(&state.store),
                &project_id,
                sym,
            ))
            .unwrap())
        }
        "lsp_get_hover" => {
            let file_path = args["filePath"]
                .as_str()
                .or_else(|| args["file_path"].as_str())
                .unwrap_or_default();
            let line = args["line"].as_u64().unwrap_or(1) as u32;
            let word = args["word"].as_str().unwrap_or_default();
            let project_id = {
                let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
                pm_guard
                    .as_ref()
                    .map(|pm| pm.normalized_root())
                    .unwrap_or_default()
            };
            Ok(serde_json::to_value(state.lsp.get_hover_from_store(
                Some(&state.store),
                &project_id,
                file_path,
                line,
                word,
            ))
            .unwrap())
        }
        other => Err(format!("Unsupported IPC command: {}", other)),
    }
}

async fn handle_http_connection(mut stream: TcpStream, state: Arc<AppState>) {
    let mut buf = vec![0u8; 65536];
    let mut total_read = 0usize;
    loop {
        match stream.read(&mut buf[total_read..]).await {
            Ok(0) => break,
            Ok(n) => {
                total_read += n;
                if let Some(header_end) = buf[..total_read]
                    .windows(4)
                    .position(|w| w == b"\r\n\r\n")
                {
                    let headers_str = String::from_utf8_lossy(&buf[..header_end]);
                    let content_len = headers_str
                        .lines()
                        .find_map(|l| {
                            let lower = l.to_ascii_lowercase();
                            if lower.starts_with("content-length:") {
                                l.split(':').nth(1)?.trim().parse::<usize>().ok()
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);
                    let body_start = header_end + 4;
                    let expected_total = body_start + content_len;
                    if total_read < expected_total {
                        if buf.len() < expected_total {
                            buf.resize(expected_total, 0);
                        }
                        if stream
                            .read_exact(&mut buf[total_read..expected_total])
                            .await
                            .is_ok()
                        {
                            total_read = expected_total;
                        }
                    }
                    break;
                }
                if total_read == buf.len() {
                    buf.resize(buf.len() * 2, 0);
                }
            }
            Err(_) => return,
        }
    }

    if total_read == 0 {
        return;
    }

    let Some(header_end) = buf[..total_read]
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
    else {
        return;
    };
    let req_line = String::from_utf8_lossy(&buf[..header_end])
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    let parts: Vec<&str> = req_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }
    let method = parts[0];
    let path = parts[1];
    let body = &buf[header_end + 4..total_read];

    if method == "POST" && path == "/__noesis_ipc__" {
        let resp_json = match serde_json::from_slice::<IpcEnvelope>(body) {
            Ok(env) => match dispatch_command(&state, &env.cmd, env.args).await {
                Ok(val) => json!({ "result": val }),
                Err(err) => json!({ "error": err }),
            },
            Err(e) => json!({ "error": format!("Invalid IPC JSON: {}", e) }),
        };
        let body_bytes = serde_json::to_vec(&resp_json).unwrap_or_default();
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body_bytes.len()
        );
        let _ = stream.write_all(header.as_bytes()).await;
        let _ = stream.write_all(&body_bytes).await;
        return;
    }

    let (content_type, body_bytes): (&str, Vec<u8>) = if path.ends_with(".css") {
        ("text/css; charset=utf-8", DIST_CSS.to_vec())
    } else if path.ends_with(".js") {
        ("application/javascript; charset=utf-8", DIST_JS.to_vec())
    } else {
        let injected = DIST_INDEX_HTML.replace("<head>", &format!("<head>\n{}", IPC_SHIM_SCRIPT));
        ("text/html; charset=utf-8", injected.into_bytes())
    };

    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        content_type,
        body_bytes.len()
    );
    let _ = stream.write_all(header.as_bytes()).await;
    let _ = stream.write_all(&body_bytes).await;
}

fn launch_linux_window(url: &str) {
    let app_arg = format!("--app={}", url);
    for browser in [
        "google-chrome",
        "chromium",
        "chromium-browser",
        "microsoft-edge",
        "brave-browser",
    ] {
        if Command::new(browser)
            .args([&app_arg, "--window-size=1280,840"])
            .spawn()
            .is_ok()
        {
            return;
        }
    }
    let _ = Command::new("xdg-open").arg(url).spawn();
}

#[tokio::main]
async fn main() {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let app_data = PathBuf::from(home).join(".local/share/com.noesis.ide");
    let _ = std::fs::create_dir_all(&app_data);
    let db_path = app_data.join("noesis_local.db");

    let (store, db_degraded, db_error) = match DbStore::open(&db_path) {
        Ok(s) => (s, false, None),
        Err(err) => {
            let msg = format!("数据库打开失败 ({:?}): {}", db_path, err);
            let mem = DbStore::in_memory().expect("In-memory database initialization failed");
            (mem, true, Some(msg))
        }
    };

    let state = Arc::new(AppState {
        project_manager: Mutex::new(None),
        project_map: Mutex::new(None),
        store,
        db_degraded,
        db_error,
        credentials: CredentialStore::new(Some(app_data)),
        llm: LlmGateway::new(),
        indexer: CodeIndexer::new(),
        knowledge_base: KnowledgeBase::load_builtin(),
        lsp: LspManager::new(),
        cancellations: CancellationRegistry::new(),
    });

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind local Noesis IPC socket");
    let addr = listener.local_addr().unwrap();
    let url = format!("http://127.0.0.1:{}", addr.port());
    println!("NOESIS v0.1.0 (Linux x86_64) running at {}", url);

    if std::env::var("NOESIS_HEADLESS").is_err() {
        launch_linux_window(&url);
    }

    loop {
        if let Ok((stream, _)) = listener.accept().await {
            let st = Arc::clone(&state);
            tokio::spawn(async move {
                handle_http_connection(stream, st).await;
            });
        }
    }
}
