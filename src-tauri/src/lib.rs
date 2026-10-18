use noesis_indexer::{CodeIndexer, DomainAdapter, ProjectMap};
use noesis_learn::context::{
    ContextAssembler, ContextManifest, ContextRequest, DroppedItem, HeuristicTokenEstimator,
    TokenEstimator, PROMPT_VERSION,
};
use noesis_learn::exercises::{
    compute_sm2_review, generate_cloze_exercise, generate_fact_exercises,
    grade_deterministic_answer, map_answer_to_quality, refresh_project_exercises_freshness,
    Sm2ReviewState,
};
use noesis_learn::knowledge::KnowledgeBase;
use noesis_learn::quality::{QualityGate, QualityReport};
use noesis_learn::verifier::{
    extract_and_parse_explanation_json, CitationItem, CitationVerifier, StructuredExplanation,
    VerificationSummary,
};
use noesis_learn::{ExplanationCard, LearnEngine, RewordingResult};
use noesis_llm::stream::{
    budget_conversation_history, split_chat_citations, CancellationRegistry,
};
use noesis_llm::{
    ChatMessage, ConnectionTestResult, CredentialStore, LlmConfig, LlmGateway, OutboundPreview,
};
use noesis_llmapp::LlmAppAdapter;
use noesis_lsp::{DefinitionLocation, HoverInfo, LspManager, LspServerStatus, ReferenceLocation};
use noesis_project::{FileContentPayload, FileNode, ProjectManager};
use noesis_ros2::Ros2Adapter;
use noesis_runner::{CodeRunner, RunRequest, RunResponse};
use noesis_store::{
    CachedExplanation, ConceptProfileItem, ConfusionMark, ConfusionRevision, ConversationRecord,
    DbStore, ExerciseRecord, ExplanationFeedback, ExplanationSummary, FeedbackStats, GraphSymbol,
    PromptPreset, ReadingProgressRecord, StoredMessageRecord, SymbolNeighborhood,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Manager, State};

const EXPERT_SYSTEM_PROMPT: &str = include_str!("../../prompts/code_guide_expert.system.md");
const LOCKED_RULES_PROMPT: &str = include_str!("../../prompts/rules.locked.md");
const DEFAULT_PERSONA_PROMPT: &str = include_str!("../../prompts/persona.default.md");
const ANNOTATE_LINES_PROMPT: &str = include_str!("../../prompts/annotate_lines.md");

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
    #[serde(default)]
    pub quality: Option<QualityReport>,
    #[serde(default)]
    pub explanation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AskStreamEvent {
    #[serde(rename = "type")]
    pub event_type: String, // "context_ready" | "token" | "tool_call" | "tool_result" | "citation_check" | "done" | "error"
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_args: Option<String>,
    #[serde(default)]
    pub tool_result: Option<String>,
    #[serde(default)]
    pub citations: Option<Vec<CitationItem>>,
    #[serde(default)]
    pub manifest: Option<ContextManifest>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub conversation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskPreviewResponse {
    pub messages: Vec<ChatMessage>,
    pub token_estimate: usize,
    pub manifest: ContextManifest,
    pub outbound_preview: OutboundPreview,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectivePromptPreview {
    pub scope: String,
    pub preset_name: String,
    pub l1_persona: String,
    pub l0_locked: String,
    pub full_prompt: String,
    pub estimated_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotatedLine {
    pub line_no: u32,
    pub code: String,
    pub comment: String,
    pub kind: String, // "trivial" | "key" | "hard"
    #[serde(default)]
    pub status: Option<String>, // "verified" | "corrected" | "missing"
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnnotatedSnippet {
    pub lines: Vec<AnnotatedLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfusionDetailResponse {
    pub mark: ConfusionMark,
    pub revisions: Vec<ConfusionRevision>,
    pub active_annotated: Option<AnnotatedSnippet>,
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

#[tauri::command]
fn get_system_status(state: State<'_, AppState>) -> Result<SystemStatus, String> {
    Ok(SystemStatus {
        db_degraded: state.db_degraded,
        db_error: state.db_error.clone(),
    })
}

#[tauri::command]
fn get_expert_system_prompt() -> Result<String, String> {
    Ok(EXPERT_SYSTEM_PROMPT.to_string())
}

#[tauri::command]
fn get_llm_config(state: State<'_, AppState>) -> Result<LlmConfig, String> {
    Ok(state.credentials.get_config_view())
}

#[tauri::command]
fn set_llm_config(config: LlmConfig, state: State<'_, AppState>) -> Result<LlmConfig, String> {
    state
        .credentials
        .update_config(config)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_api_key(api_key: String, state: State<'_, AppState>) -> Result<LlmConfig, String> {
    state
        .credentials
        .set_api_key(&api_key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_api_key(state: State<'_, AppState>) -> Result<LlmConfig, String> {
    state
        .credentials
        .clear_api_key()
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn test_llm_connection(
    config: LlmConfig,
    state: State<'_, AppState>,
) -> Result<ConnectionTestResult, String> {
    let effective = state.credentials.resolve_effective_config(&config);
    Ok(state.llm.test_connection(&effective).await)
}

#[tauri::command]
fn cancel_llm_request(request_id: String, state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.cancellations.cancel(&request_id))
}

#[tauri::command]
fn open_project(path: String, state: State<'_, AppState>) -> Result<FileNode, String> {
    let pm = ProjectManager::new(&path).map_err(|e| e.to_string())?;
    let tree = pm.build_tree(2).map_err(|e| e.to_string())?;
    let mut guard = state.project_manager.lock().map_err(|e| e.to_string())?;
    *guard = Some(pm);
    Ok(tree)
}

#[tauri::command]
fn list_project_dir(rel_dir: String, state: State<'_, AppState>) -> Result<Vec<FileNode>, String> {
    let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
    if let Some(pm) = pm_guard.as_ref() {
        pm.list_dir(&rel_dir).map_err(|e| e.to_string())
    } else {
        Err("尚未打开任何工程".to_string())
    }
}

#[tauri::command]
fn get_project_tree(state: State<'_, AppState>) -> Result<Option<FileNode>, String> {
    let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
    if let Some(pm) = pm_guard.as_ref() {
        let tree = pm.build_tree(6).map_err(|e| e.to_string())?;
        Ok(Some(tree))
    } else {
        Ok(None)
    }
}

#[tauri::command]
fn read_file(path: String, state: State<'_, AppState>) -> Result<FileContentPayload, String> {
    let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
    if let Some(pm) = pm_guard.as_ref() {
        pm.read_file(&path).map_err(|e| e.to_string())
    } else {
        Err("尚未打开任何工程，请先在左侧选择工程目录。".to_string())
    }
}

#[tauri::command]
fn save_file(path: String, content: String, state: State<'_, AppState>) -> Result<String, String> {
    let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
    if let Some(pm) = pm_guard.as_ref() {
        pm.save_file(&path, &content).map_err(|e| e.to_string())
    } else {
        Err("尚未打开任何工程，无法保存文件。".to_string())
    }
}

#[tauri::command]
fn index_project(state: State<'_, AppState>) -> Result<ProjectMap, String> {
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

    // Stage D: refresh exercise evidence freshness after re-indexing
    refresh_project_exercises_freshness(&root_path, &state.store, &project_id);

    let mut map_guard = state.project_map.lock().map_err(|e| e.to_string())?;
    *map_guard = Some(map.clone());

    Ok(map)
}

#[tauri::command]
fn get_project_map(state: State<'_, AppState>) -> Result<Option<ProjectMap>, String> {
    let map = state
        .project_map
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    Ok(map)
}

#[tauri::command]
fn set_reading_step_status(
    step_id: String,
    status: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let project_id = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        let pm = pm_guard.as_ref().ok_or("No project opened yet")?;
        pm.normalized_root()
    };
    state
        .store
        .set_reading_progress(&project_id, &step_id, &status)
        .map_err(|e| e.to_string())?;

    if let Ok(mut map_guard) = state.project_map.lock() {
        if let Some(ref mut map) = *map_guard {
            for step in &mut map.reading_route {
                if step.id == step_id || step.file_path == step_id {
                    step.status = status.clone();
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
fn get_reading_progress(state: State<'_, AppState>) -> Result<Vec<ReadingProgressRecord>, String> {
    let project_id = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        let pm = pm_guard.as_ref().ok_or("No project opened yet")?;
        pm.normalized_root()
    };
    state
        .store
        .get_reading_progress(&project_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_symbol_neighborhood(
    symbol_id: String,
    state: State<'_, AppState>,
) -> Result<Option<SymbolNeighborhood>, String> {
    let project_id = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        let pm = pm_guard.as_ref().ok_or("No project opened yet")?;
        pm.normalized_root()
    };
    state
        .store
        .get_symbol_with_neighbors(&project_id, &symbol_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_project_symbols(
    file_path: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<GraphSymbol>, String> {
    let project_id = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        let pm = pm_guard.as_ref().ok_or("No project opened yet")?;
        pm.normalized_root()
    };
    state
        .store
        .list_project_symbols(&project_id, file_path.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn explain_code(
    file_path: String,
    snippet: String,
    start_line: u32,
    end_line: u32,
    state: State<'_, AppState>,
) -> Result<Vec<ExplanationCard>, String> {
    let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
    let source_matches = if let Some(pm) = pm_guard.as_ref() {
        if let Ok(payload) = pm.read_file(&file_path) {
            LearnEngine::verify_source_reference(&payload.content, start_line, end_line, &snippet)
        } else {
            false
        }
    } else {
        false
    };
    Ok(LearnEngine::generate_structured_cards(
        &file_path,
        &snippet,
        start_line,
        end_line,
        source_matches,
    ))
}

/// Stage C1 + C4 + C5: Assembles grounded project context, calls LLM, verifies citations, and caches result.
#[tauri::command]
async fn explain_selection_structured(
    config: LlmConfig,
    file_path: String,
    snippet: String,
    start_line: u32,
    end_line: u32,
    force_refresh: bool,
    depth_mode: Option<String>,
    state: State<'_, AppState>,
) -> Result<StructuredExplainResponse, String> {
    let (root_path, project_id) = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        if let Some(pm) = pm_guard.as_ref() {
            (pm.root_path().to_path_buf(), pm.normalized_root())
        } else {
            (PathBuf::from("."), "default".to_string())
        }
    };

    let mut effective = state.credentials.resolve_effective_config(&config);
    if depth_mode.as_deref() == Some("deep") {
        effective.max_tokens = effective.max_tokens.max(8192);
    }

    let active_preset = state.store.get_active_prompt_preset("explain").ok().flatten();
    let custom_persona = active_preset.map(|p| p.persona_text);

    let estimator = HeuristicTokenEstimator;
    let budget_tokens = if depth_mode.as_deref() == Some("deep") { 24000 } else { 16000 };

    let ctx_req = ContextRequest {
        kind: "explain_selection".to_string(),
        project_id: project_id.clone(),
        file: Some(file_path.clone()),
        range: Some((start_line, end_line)),
        selected_code: Some(snippet.clone()),
        question: None,
        entity_ref: None,
        conversation_id: None,
        budget_tokens,
        include_learning_profile: true,
        re_explain_type: None,
        re_explain_level: None,
        previous_explanation_summary: None,
        user_note: None,
        depth_mode: depth_mode.clone(),
        custom_persona,
    };

    let pack = ContextAssembler::assemble(
        &root_path,
        Some(&state.store),
        Some(&state.knowledge_base),
        &ctx_req,
        &estimator,
        &effective.model,
    );

    let generic_phrases = QualityGate::load_generic_phrases(&root_path);

    if !force_refresh {
        if let Ok(Some(cached)) = state.store.get_cached_explanation(
            &pack.basis_hash,
            &effective.model,
            PROMPT_VERSION,
        ) {
            if let Ok(mut exp) = serde_json::from_str::<StructuredExplanation>(&cached.json) {
                let verification = CitationVerifier::verify(
                    &root_path,
                    &pack.manifest.loaded_knowledge_packs,
                    &mut exp,
                );
                let quality = QualityGate::evaluate(
                    &exp,
                    &snippet,
                    depth_mode.as_deref(),
                    &generic_phrases,
                );
                return Ok(StructuredExplainResponse {
                    explanation: Some(exp),
                    verification,
                    manifest: pack.manifest,
                    dropped: pack.dropped,
                    cached: true,
                    raw_unverified_output: None,
                    quality: Some(quality),
                    explanation_id: Some(cached.id),
                });
            }
        }
    }

    let mut prompt_sections = Vec::new();
    for item in &pack.items {
        prompt_sections.push(item.text.clone());
    }
    let combined_prompt = prompt_sections.join("\n\n");
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: pack.items.iter().find(|i| i.kind == "system_prompt").map(|i| i.text.clone()).unwrap_or_else(|| EXPERT_SYSTEM_PROMPT.to_string()),
        },
        ChatMessage {
            role: "user".to_string(),
            content: combined_prompt,
        },
    ];

    let raw_reply = state
        .llm
        .send_chat(&effective, messages.clone())
        .await
        .map_err(|e| e.to_string())?;

    let parsed = match extract_and_parse_explanation_json(&raw_reply) {
        Ok(exp) => Ok(exp),
        Err(first_err) => {
            // C1 retry once with parse error
            let mut retry_msgs = messages.clone();
            retry_msgs.push(ChatMessage {
                role: "assistant".to_string(),
                content: raw_reply.clone(),
            });
            retry_msgs.push(ChatMessage {
                role: "user".to_string(),
                content: format!(
                    "你上一次返回的 JSON 无法解析（错误：{}）。请只输出符合 schema 的合法 JSON 对象，不要包含额外解释。",
                    first_err
                ),
            });
            match state.llm.send_chat(&effective, retry_msgs).await {
                Ok(second_reply) => extract_and_parse_explanation_json(&second_reply),
                Err(_) => Err(first_err),
            }
        }
    };

    match parsed {
        Ok(mut exp) => {
            let mut verification = CitationVerifier::verify(
                &root_path,
                &pack.manifest.loaded_knowledge_packs,
                &mut exp,
            );
            let mut quality = QualityGate::evaluate(
                &exp,
                &snippet,
                depth_mode.as_deref(),
                &generic_phrases,
            );

            // F2 Quality Gate: If content is too shallow or generic, retry once with issues prompt
            if quality.is_shallow {
                let mut quality_retry_msgs = messages.clone();
                quality_retry_msgs.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: serde_json::to_string(&exp).unwrap_or_default(),
                });
                quality_retry_msgs.push(ChatMessage {
                    role: "user".to_string(),
                    content: format!(
                        "你上一次给出的解读深度不足、过于泛化（具体问题：{}）。请结合代码中的具体标识符，补充逐行追踪逻辑和底层机制详述，必须输出完整合法的 JSON 对象。",
                        quality.issues.join("; ")
                    ),
                });
                if let Ok(retry_reply) = state.llm.send_chat(&effective, quality_retry_msgs).await {
                    if let Ok(mut retry_exp) = extract_and_parse_explanation_json(&retry_reply) {
                        verification = CitationVerifier::verify(
                            &root_path,
                            &pack.manifest.loaded_knowledge_packs,
                            &mut retry_exp,
                        );
                        quality = QualityGate::evaluate(
                            &retry_exp,
                            &snippet,
                            depth_mode.as_deref(),
                            &generic_phrases,
                        );
                        exp = retry_exp;
                    }
                }
            }

            let exp_id = format!(
                "exp-{}-{}",
                chrono::Utc::now().timestamp_millis(),
                &pack.basis_hash[..8.min(pack.basis_hash.len())]
            );

            let title_text = exp.overall.one_sentence.chars().take(40).collect::<String>();
            let title = if title_text.trim().is_empty() {
                let fn_name = file_path.split('/').next_back().unwrap_or(&file_path);
                format!("{}:{}-{}", fn_name, start_line, end_line)
            } else {
                title_text
            };
            let preview = snippet.chars().take(200).collect::<String>();

            if let Ok(json_str) = serde_json::to_string(&exp) {
                let stats_str = serde_json::to_string(&verification).unwrap_or_default();
                let _ = state.store.save_explanation_cache(&CachedExplanation {
                    id: exp_id.clone(),
                    project_id: project_id.clone(),
                    file_id: file_path.clone(),
                    range: format!("{}-{}", start_line, end_line),
                    basis_hash: pack.basis_hash,
                    prompt_version: PROMPT_VERSION.to_string(),
                    model: effective.model.clone(),
                    request_kind: "explain_selection".to_string(),
                    json: json_str,
                    stats: stats_str,
                    created_at: chrono::Utc::now().timestamp(),
                    title: Some(title),
                    file_path: Some(file_path.clone()),
                    selected_preview: Some(preview),
                    pinned: false,
                    deleted: false,
                    depth_mode: depth_mode.clone(),
                    last_opened_at: Some(chrono::Utc::now().timestamp()),
                });
            }

            Ok(StructuredExplainResponse {
                explanation: Some(exp),
                verification,
                manifest: pack.manifest,
                dropped: pack.dropped,
                cached: false,
                raw_unverified_output: None,
                quality: Some(quality),
                explanation_id: Some(exp_id),
            })
        }
        Err(_) => Ok(StructuredExplainResponse {
            explanation: None,
            verification: VerificationSummary::default(),
            manifest: pack.manifest,
            dropped: pack.dropped,
            cached: false,
            raw_unverified_output: Some(raw_reply),
            quality: None,
            explanation_id: None,
        }),
    }
}

#[tauri::command]
fn mark_confusion(
    file_path: String,
    start_line: u32,
    end_line: u32,
    snippet: String,
    category: String,
    user_note: Option<String>,
    state: State<'_, AppState>,
) -> Result<RewordingResult, String> {
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
        id: mark_id.clone(),
        project_id: project_id.clone(),
        file_path,
        line_start: start_line,
        line_end: end_line,
        code_snippet: snippet,
        category: category.clone(),
        confusion_type: Some(category.clone()),
        explanation: Some(rewording.analogy.clone()),
        ai_explanation: None,
        model: None,
        level: 1,
        resolved_at: None,
        resolved_level: None,
        user_note,
        created_at: chrono::Utc::now().to_rfc3339(),
        review_exercise_id: None,
        concepts_json: None,
    };

    let _ = state.store.save_mark(&mark);
    if let Some(ref pid) = project_id {
        let _ = state.store.record_concept_event(
            &category,
            None,
            "general",
            pid,
            Some(&mark_id),
            "confused",
        );
    }
    Ok(rewording)
}

#[tauri::command]
fn update_confusion_mark_explanation(
    id: String,
    ai_explanation: String,
    model: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .store
        .update_mark_ai_explanation(&id, &ai_explanation, &model)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn resolve_confusion_mark(
    id: String,
    level: i32,
    resolved: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .store
        .update_mark_level_and_resolution(&id, level, resolved)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_confusion_marks(
    file_path: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<ConfusionMark>, String> {
    let project_id = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        pm_guard.as_ref().map(|pm| pm.normalized_root())
    };
    state
        .store
        .list_marks(file_path.as_deref(), project_id.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_learning_profile(state: State<'_, AppState>) -> Result<Vec<ConceptProfileItem>, String> {
    let project_id = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        pm_guard.as_ref().map(|pm| pm.normalized_root())
    };
    state
        .store
        .list_concept_profile(project_id.as_deref())
        .map_err(|e| e.to_string())
}

/// Stage C3: Multi-session conversation persistence commands
#[tauri::command]
fn list_conversations(state: State<'_, AppState>) -> Result<Vec<ConversationRecord>, String> {
    let project_id = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        pm_guard.as_ref().map(|pm| pm.normalized_root())
    };
    state
        .store
        .list_conversations(project_id.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn create_conversation(
    title: String,
    state: State<'_, AppState>,
) -> Result<ConversationRecord, String> {
    let project_id = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        pm_guard
            .as_ref()
            .map(|pm| pm.normalized_root())
            .unwrap_or_else(|| "default".to_string())
    };
    let id = format!("conv-{}", chrono::Utc::now().timestamp_millis());
    let clean_title: String = title.trim().chars().take(30).collect();
    let final_title = if clean_title.is_empty() {
        "新对话".to_string()
    } else {
        clean_title
    };
    state
        .store
        .create_conversation(&id, &project_id, &final_title)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn load_conversation_messages(
    conversation_id: String,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<StoredMessageRecord>, String> {
    state
        .store
        .list_messages(&conversation_id, limit.unwrap_or(50))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn append_conversation_message(
    conversation_id: String,
    role: String,
    content: String,
    meta_json: Option<String>,
    state: State<'_, AppState>,
) -> Result<StoredMessageRecord, String> {
    let est = HeuristicTokenEstimator;
    let tokens = noesis_learn::context::TokenEstimator::estimate(&est, &content);
    let msg = StoredMessageRecord {
        id: format!("msg-{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)),
        conversation_id,
        role,
        content,
        meta_json: meta_json.unwrap_or_else(|| "{}".to_string()),
        tokens,
        created_at: chrono::Utc::now().timestamp(),
    };
    state
        .store
        .append_message(&msg)
        .map_err(|e| e.to_string())?;
    Ok(msg)
}

#[tauri::command]
fn delete_conversation(id: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .store
        .delete_conversation(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn preview_outbound(
    config: LlmConfig,
    messages: Vec<ChatMessage>,
    state: State<'_, AppState>,
) -> Result<OutboundPreview, String> {
    let effective = state.credentials.resolve_effective_config(&config);
    state
        .llm
        .filter()
        .inspect_outbound(&effective, &messages)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn send_llm_chat(
    config: LlmConfig,
    messages: Vec<ChatMessage>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let effective = state.credentials.resolve_effective_config(&config);
    state
        .llm
        .send_chat(&effective, messages)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn run_scratch_code(req: RunRequest) -> Result<RunResponse, String> {
    CodeRunner::execute(req).await.map_err(|e| e.to_string())
}

/// Stage D: Generate deterministic `fact` and `cloze` exercises from the project graph and persist to SQLite.
#[tauri::command]
fn generate_project_exercises(
    count: Option<usize>,
    file_path: Option<String>,
    snippet: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<ExerciseRecord>, String> {
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
        generate_fact_exercises(&project_id, &entities, &edges, &symbols, count.unwrap_or(5));

    if let (Some(ref fp), Some(ref code)) = (file_path, snippet) {
        if let Some(cloze) = generate_cloze_exercise(&project_id, fp, code, None) {
            generated.push(cloze);
        }
    }

    for ex in &generated {
        let _ = state.store.insert_exercise(ex);
    }

    Ok(generated)
}

#[tauri::command]
fn submit_exercise_answer(
    exercise_id: String,
    answer: String,
    hesitation: Option<String>,
    state: State<'_, AppState>,
) -> Result<SubmitExerciseResult, String> {
    let due = state.store.list_due_exercises().map_err(|e| e.to_string())?;
    let Some(mut ex) = due.into_iter().find(|e| e.id == exercise_id) else {
        return Err(format!("未找到练习题 '{}'", exercise_id));
    };

    let is_correct = grade_deterministic_answer(&answer, &ex.reference_answer);
    let quality = map_answer_to_quality(&ex.source_kind, is_correct, hesitation.as_deref(), None);
    let review = compute_sm2_review(quality, ex.ease, ex.interval_days, ex.repetition, ex.lapses);

    ex.ease = review.ease;
    ex.interval_days = review.interval_days;
    ex.repetition = review.repetitions;
    ex.lapses = review.lapses;
    ex.next_review_at = review.next_review_at.clone();

    state.store.insert_exercise(&ex).map_err(|e| e.to_string())?;
    let _ = state.store.record_exercise_attempt(
        &ex.id,
        &answer,
        if is_correct { "correct" } else { "wrong" },
        if is_correct { 1.0 } else { 0.0 },
    );

    Ok(SubmitExerciseResult {
        is_correct,
        quality,
        review,
        reference_answer: ex.reference_answer,
        verified_by_program: ex.verified_by_program,
    })
}

#[tauri::command]
fn list_due_reviews(state: State<'_, AppState>) -> Result<Vec<ExerciseRecord>, String> {
    state.store.list_due_exercises().map_err(|e| e.to_string())
}

/// Stage E2: LSP server detection and definition/reference/hover resolution with index fallback
#[tauri::command]
fn detect_lsp_servers(state: State<'_, AppState>) -> Result<Vec<LspServerStatus>, String> {
    Ok(state.lsp.detect_servers())
}

#[tauri::command]
fn lsp_find_definition(
    symbol_name: String,
    state: State<'_, AppState>,
) -> Result<Vec<DefinitionLocation>, String> {
    let project_id = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        pm_guard
            .as_ref()
            .map(|pm| pm.normalized_root())
            .unwrap_or_default()
    };
    Ok(state
        .lsp
        .resolve_definition(Some(&state.store), &project_id, &symbol_name))
}

#[tauri::command]
fn lsp_find_references(
    symbol_name: String,
    state: State<'_, AppState>,
) -> Result<Vec<ReferenceLocation>, String> {
    let project_id = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        pm_guard
            .as_ref()
            .map(|pm| pm.normalized_root())
            .unwrap_or_default()
    };
    Ok(state
        .lsp
        .resolve_references(Some(&state.store), &project_id, &symbol_name))
}

#[tauri::command]
fn lsp_get_hover(
    file_path: String,
    line: u32,
    word: String,
    state: State<'_, AppState>,
) -> Result<Option<HoverInfo>, String> {
    let project_id = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        pm_guard
            .as_ref()
            .map(|pm| pm.normalized_root())
            .unwrap_or_default()
    };
    Ok(state
        .lsp
        .get_hover_from_store(Some(&state.store), &project_id, &file_path, line, &word))
}

// ----------------- F3: Explanation History Commands -----------------

#[tauri::command]
fn list_explanations(
    file_path: Option<String>,
    query: Option<String>,
    pinned_only: Option<bool>,
    limit: Option<usize>,
    offset: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<ExplanationSummary>, String> {
    let project_id = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        if let Some(pm) = pm_guard.as_ref() {
            pm.normalized_root()
        } else {
            "default".to_string()
        }
    };
    state
        .store
        .list_explanations(
            &project_id,
            file_path.as_deref(),
            query.as_deref(),
            pinned_only.unwrap_or(false),
            limit.unwrap_or(20),
            offset.unwrap_or(0),
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_explanation(
    id: String,
    state: State<'_, AppState>,
) -> Result<Option<StructuredExplainResponse>, String> {
    let root_path = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        if let Some(pm) = pm_guard.as_ref() {
            pm.root_path().to_path_buf()
        } else {
            PathBuf::from(".")
        }
    };

    let item = state.store.get_explanation(&id).map_err(|e| e.to_string())?;
    let Some(cached) = item else {
        return Ok(None);
    };

    let mut exp: StructuredExplanation =
        serde_json::from_str(&cached.json).map_err(|e| e.to_string())?;
    let verification = CitationVerifier::verify(&root_path, &[], &mut exp);
    let generic_phrases = QualityGate::load_generic_phrases(&root_path);
    let quality = QualityGate::evaluate(
        &exp,
        &cached.selected_preview.clone().unwrap_or_default(),
        cached.depth_mode.as_deref(),
        &generic_phrases,
    );

    Ok(Some(StructuredExplainResponse {
        explanation: Some(exp),
        verification,
        manifest: ContextManifest::default(),
        dropped: vec![],
        cached: true,
        raw_unverified_output: None,
        quality: Some(quality),
        explanation_id: Some(cached.id),
    }))
}

#[tauri::command]
fn rename_explanation(
    id: String,
    new_title: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .store
        .rename_explanation(&id, &new_title)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn pin_explanation(id: String, pinned: bool, state: State<'_, AppState>) -> Result<(), String> {
    state
        .store
        .pin_explanation(&id, pinned)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_explanation(id: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .store
        .delete_explanation(&id)
        .map_err(|e| e.to_string())
}

// ----------------- F4: Project Ask & Outbound Preview -----------------

#[tauri::command]
fn preview_ask(
    question: String,
    file: Option<String>,
    range: Option<(u32, u32)>,
    selected_code: Option<String>,
    state: State<'_, AppState>,
) -> Result<AskPreviewResponse, String> {
    let (root_path, project_id) = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        if let Some(pm) = pm_guard.as_ref() {
            (pm.root_path().to_path_buf(), pm.normalized_root())
        } else {
            (PathBuf::from("."), "default".to_string())
        }
    };

    let effective = state
        .credentials
        .resolve_effective_config(&state.credentials.get_config_view());
    let estimator = HeuristicTokenEstimator;

    let active_preset = state.store.get_active_prompt_preset("ask").ok().flatten();
    let custom_persona = active_preset.map(|p| p.persona_text);

    let ctx_req = ContextRequest {
        kind: "ask".to_string(),
        project_id,
        file,
        range,
        selected_code,
        question: Some(question.clone()),
        entity_ref: None,
        conversation_id: None,
        budget_tokens: 16000,
        include_learning_profile: true,
        re_explain_type: None,
        re_explain_level: None,
        previous_explanation_summary: None,
        user_note: None,
        depth_mode: None,
        custom_persona,
    };

    let pack = ContextAssembler::assemble(
        &root_path,
        Some(&state.store),
        Some(&state.knowledge_base),
        &ctx_req,
        &estimator,
        &effective.model,
    );

    let sys_prompt = pack
        .items
        .iter()
        .find(|i| i.kind == "system_prompt")
        .map(|i| i.text.clone())
        .unwrap_or_else(|| EXPERT_SYSTEM_PROMPT.to_string());
    let mut context_chunks = Vec::new();
    for item in &pack.items {
        if item.kind != "system_prompt" {
            context_chunks.push(item.text.clone());
        }
    }
    let user_prompt = format!(
        "项目上下文：\n\n{}\n\n用户问题：{}",
        context_chunks.join("\n\n"),
        question
    );

    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: sys_prompt,
        },
        ChatMessage {
            role: "user".to_string(),
            content: user_prompt,
        },
    ];

    let outbound_preview = state
        .llm
        .filter()
        .inspect_outbound(&effective, &messages)
        .map_err(|e| e.to_string())?;

    Ok(AskPreviewResponse {
        messages,
        token_estimate: pack.token_estimate,
        manifest: pack.manifest,
        outbound_preview,
    })
}

#[tauri::command]
async fn ask_project_question(
    conversation_id: Option<String>,
    question: String,
    file: Option<String>,
    range: Option<(u32, u32)>,
    selected_code: Option<String>,
    _use_tools: Option<bool>,
    on_event: tauri::ipc::Channel<AskStreamEvent>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (root_path, project_id) = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        if let Some(pm) = pm_guard.as_ref() {
            (pm.root_path().to_path_buf(), pm.normalized_root())
        } else {
            (PathBuf::from("."), "default".to_string())
        }
    };

    let effective = state
        .credentials
        .resolve_effective_config(&state.credentials.get_config_view());
    let estimator = HeuristicTokenEstimator;

    let active_preset = state.store.get_active_prompt_preset("ask").ok().flatten();
    let custom_persona = active_preset.map(|p| p.persona_text);

    let ctx_req = ContextRequest {
        kind: "ask".to_string(),
        project_id: project_id.clone(),
        file: file.clone(),
        range,
        selected_code: selected_code.clone(),
        question: Some(question.clone()),
        entity_ref: None,
        conversation_id: conversation_id.clone(),
        budget_tokens: 16000,
        include_learning_profile: true,
        re_explain_type: None,
        re_explain_level: None,
        previous_explanation_summary: None,
        user_note: None,
        depth_mode: None,
        custom_persona,
    };

    let pack = ContextAssembler::assemble(
        &root_path,
        Some(&state.store),
        Some(&state.knowledge_base),
        &ctx_req,
        &estimator,
        &effective.model,
    );

    let conv_id = match conversation_id {
        Some(cid) if !cid.is_empty() => cid,
        _ => {
            let fn_prefix = file
                .as_deref()
                .and_then(|f| f.split('/').next_back())
                .unwrap_or("问答");
            let q_sub: String = question.chars().take(20).collect();
            let title = format!("{}: {}", fn_prefix, q_sub);
            let new_id = format!("conv-{}", chrono::Utc::now().timestamp_millis());
            state
                .store
                .create_conversation(&new_id, &project_id, &title)
                .map_err(|e| e.to_string())?
                .id
        }
    };

    let user_msg_id = format!("msg-{}", chrono::Utc::now().timestamp_millis());
    let user_msg = StoredMessageRecord {
        id: user_msg_id,
        conversation_id: conv_id.clone(),
        role: "user".to_string(),
        content: question.clone(),
        meta_json: "{}".to_string(),
        tokens: estimator.estimate(&question),
        created_at: chrono::Utc::now().timestamp(),
    };
    let _ = state.store.append_message(&user_msg);

    let _ = on_event.send(AskStreamEvent {
        event_type: "context_ready".to_string(),
        manifest: Some(pack.manifest.clone()),
        conversation_id: Some(conv_id.clone()),
        ..Default::default()
    });

    let sys_prompt = pack
        .items
        .iter()
        .find(|i| i.kind == "system_prompt")
        .map(|i| i.text.clone())
        .unwrap_or_else(|| EXPERT_SYSTEM_PROMPT.to_string());
    let mut context_chunks = Vec::new();
    for item in &pack.items {
        if item.kind != "system_prompt" {
            context_chunks.push(item.text.clone());
        }
    }

    let history_records = state.store.list_messages(&conv_id, 100).unwrap_or_default();
    let history_plan = budget_conversation_history(&history_records, "", None, 16000, &estimator);

    let mut send_messages = vec![ChatMessage {
        role: "system".to_string(),
        content: format!(
            "{}\n\n以下为当前项目的真实代码上下文与领域事实（严格根据此依据回答，严禁编造代码）：\n\n{}",
            sys_prompt,
            context_chunks.join("\n\n")
        ),
    }];

    for m in history_plan.retained_messages {
        send_messages.push(m);
    }
    send_messages.push(ChatMessage {
        role: "user".to_string(),
        content: question.clone(),
    });

    let on_event_clone = on_event.clone();
    let cid = conv_id.clone();
    let final_answer = state
        .llm
        .send_chat_stream(&effective, send_messages, move |delta| {
            let _ = on_event_clone.send(AskStreamEvent {
                event_type: "token".to_string(),
                token: Some(delta.to_string()),
                conversation_id: Some(cid.clone()),
                ..Default::default()
            });
        })
        .await
        .map_err(|e| e.to_string())?;

    let (clean_body, tail_opt) = split_chat_citations(&final_answer);
    let mut citations = tail_opt.map(|t| t.citations).unwrap_or_default();
    if !citations.is_empty() {
        let mut dummy_exp = StructuredExplanation {
            citations: citations.clone(),
            ..Default::default()
        };
        let _ = CitationVerifier::verify(
            &root_path,
            &pack.manifest.loaded_knowledge_packs,
            &mut dummy_exp,
        );
        citations = dummy_exp.citations;
    }

    let _ = on_event.send(AskStreamEvent {
        event_type: "citation_check".to_string(),
        citations: Some(citations.clone()),
        conversation_id: Some(conv_id.clone()),
        ..Default::default()
    });

    let _ = on_event.send(AskStreamEvent {
        event_type: "done".to_string(),
        conversation_id: Some(conv_id.clone()),
        ..Default::default()
    });

    let asst_msg_id = format!("msg-{}", chrono::Utc::now().timestamp_millis());
    let asst_msg = StoredMessageRecord {
        id: asst_msg_id,
        conversation_id: conv_id.clone(),
        role: "assistant".to_string(),
        content: clean_body,
        meta_json: serde_json::json!({
            "citations": citations
        }).to_string(),
        tokens: estimator.estimate(&final_answer),
        created_at: chrono::Utc::now().timestamp(),
    };
    let _ = state.store.append_message(&asst_msg);

    Ok(conv_id)
}

// ----------------- F5: Prompt Presets Commands -----------------

#[tauri::command]
fn list_prompt_presets(
    scope: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<PromptPreset>, String> {
    state
        .store
        .list_prompt_presets(scope.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn save_prompt_preset(preset: PromptPreset, state: State<'_, AppState>) -> Result<(), String> {
    state
        .store
        .save_prompt_preset(&preset)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_active_prompt_preset(
    id: String,
    scope: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .store
        .set_active_prompt_preset(&id, &scope)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_prompt_preset(id: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .store
        .delete_prompt_preset(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn reset_prompt_to_default(scope: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .store
        .reset_prompt_to_default(&scope)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn preview_effective_prompt(
    scope: String,
    state: State<'_, AppState>,
) -> Result<EffectivePromptPreview, String> {
    let (active_preset, preset_name) = match state.store.get_active_prompt_preset(&scope) {
        Ok(Some(p)) => (p.persona_text, p.name),
        _ => (
            DEFAULT_PERSONA_PROMPT.to_string(),
            "内置默认预设".to_string(),
        ),
    };
    let root = {
        let pm_guard = state.project_manager.lock().unwrap();
        pm_guard
            .as_ref()
            .map(|pm| pm.root_path().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    };
    let full = noesis_learn::context::load_layered_system_prompt(&root, Some(&active_preset));
    let est = HeuristicTokenEstimator;
    let tokens = est.estimate(&full);

    Ok(EffectivePromptPreview {
        scope,
        preset_name,
        l1_persona: active_preset,
        l0_locked: LOCKED_RULES_PROMPT.to_string(),
        full_prompt: full,
        estimated_tokens: tokens,
    })
}

// ----------------- F6: Confusion Learning Archive -----------------

#[tauri::command]
async fn explain_confusion(
    mark_id: String,
    level: Option<u8>,
    user_note: Option<String>,
    state: State<'_, AppState>,
) -> Result<ConfusionDetailResponse, String> {
    let (root_path, project_id) = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        if let Some(pm) = pm_guard.as_ref() {
            (pm.root_path().to_path_buf(), pm.normalized_root())
        } else {
            (PathBuf::from("."), "default".to_string())
        }
    };

    let detail = state
        .store
        .get_confusion_detail(&mark_id)
        .map_err(|e| e.to_string())?;
    let (mut mark, mut revisions) =
        detail.ok_or_else(|| format!("Mark '{}' not found", mark_id))?;

    if let Some(ref note) = user_note {
        mark.user_note = Some(note.clone());
    }

    let current_level = level.unwrap_or(mark.level as u8);
    mark.level = current_level as i32;

    let active_preset = state
        .store
        .get_active_prompt_preset("reexplain")
        .ok()
        .flatten();
    let custom_persona = active_preset.map(|p| p.persona_text);

    let effective = state
        .credentials
        .resolve_effective_config(&state.credentials.get_config_view());
    let estimator = HeuristicTokenEstimator;

    let prev_summary = revisions.last().map(|r| r.explanation_json.clone());

    let ctx_req = ContextRequest {
        kind: "re_explain".to_string(),
        project_id: project_id.clone(),
        file: Some(mark.file_path.clone()),
        range: Some((mark.line_start, mark.line_end)),
        selected_code: Some(mark.code_snippet.clone()),
        question: None,
        entity_ref: None,
        conversation_id: None,
        budget_tokens: 16000,
        include_learning_profile: true,
        re_explain_type: Some(mark.category.clone()),
        re_explain_level: Some(current_level),
        previous_explanation_summary: prev_summary,
        user_note: mark.user_note.clone(),
        depth_mode: None,
        custom_persona,
    };

    let pack = ContextAssembler::assemble(
        &root_path,
        Some(&state.store),
        Some(&state.knowledge_base),
        &ctx_req,
        &estimator,
        &effective.model,
    );

    let mut prompt_sections = Vec::new();
    for item in &pack.items {
        prompt_sections.push(item.text.clone());
    }
    let combined_prompt = prompt_sections.join("\n\n");

    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: "你是一位代码导读专家。请结合上下文给出通俗类比、最小示例、前置概念、启发式引导。必须输出合法的 JSON 对象，格式：{\"analogy\": \"...\", \"minimal_example\": \"...\", \"prerequisite_concept\": \"...\", \"guidance\": \"...\", \"concepts\": [\"...\"]}".to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: combined_prompt,
        },
    ];

    let reply = state
        .llm
        .send_chat(&effective, messages)
        .await
        .map_err(|e| e.to_string())?;

    let exp_json_val: serde_json::Value = serde_json::from_str(&reply)
        .or_else(|_| {
            let s = reply.find('{').unwrap_or(0);
            let e = reply.rfind('}').unwrap_or(reply.len());
            serde_json::from_str(&reply[s..=e])
        })
        .unwrap_or_else(|_| {
            serde_json::json!({
                "analogy": "生活类比：将该逻辑理解为工厂流水线的缓冲校验区",
                "minimal_example": mark.code_snippet,
                "prerequisite_concept": mark.category,
                "guidance": "请关注变量在条件分支下的状态流变",
                "concepts": [mark.category.clone()]
            })
        });

    let annotate_messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: ANNOTATE_LINES_PROMPT.to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!(
                "请为以下行范围 {}-{} 的代码生成逐行精准注释（必须严格输出 JSON）：\n```\n{}\n```",
                mark.line_start, mark.line_end, mark.code_snippet
            ),
        },
    ];

    let mut annotated_snippet = AnnotatedSnippet::default();
    if let Ok(annot_reply) = state.llm.send_chat(&effective, annotate_messages).await {
        if let Ok(parsed_annot) = serde_json::from_str::<AnnotatedSnippet>(&annot_reply) {
            annotated_snippet = parsed_annot;
        } else if let Some(s) = annot_reply.find('{') {
            if let Some(e) = annot_reply.rfind('}') {
                if let Ok(parsed_annot) =
                    serde_json::from_str::<AnnotatedSnippet>(&annot_reply[s..=e])
                {
                    annotated_snippet = parsed_annot;
                }
            }
        }
    }

    let real_lines: Vec<String> = mark.code_snippet.lines().map(|s| s.to_string()).collect();
    let mut verified_lines = Vec::new();
    for (idx, line_code) in real_lines.iter().enumerate() {
        let line_num = mark.line_start + idx as u32;
        if let Some(existing) = annotated_snippet.lines.iter().find(|l| l.line_no == line_num) {
            let status = if existing.code.trim() == line_code.trim() {
                "verified"
            } else {
                "corrected"
            };
            verified_lines.push(AnnotatedLine {
                line_no: line_num,
                code: line_code.clone(),
                comment: existing.comment.clone(),
                kind: existing.kind.clone(),
                status: Some(status.to_string()),
            });
        } else {
            verified_lines.push(AnnotatedLine {
                line_no: line_num,
                code: line_code.clone(),
                comment: "（常规逻辑语句）".to_string(),
                kind: "trivial".to_string(),
                status: Some("missing".to_string()),
            });
        }
    }
    annotated_snippet.lines = verified_lines;

    let concepts_vec: Vec<String> = exp_json_val
        .get("concepts")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_else(|| vec![mark.category.clone()]);
    let concepts_json = serde_json::to_string(&concepts_vec).ok();
    mark.concepts_json = concepts_json;

    let rev_id = format!("rev-{}-{}", mark.id, chrono::Utc::now().timestamp_millis());
    let revision = ConfusionRevision {
        id: rev_id,
        mark_id: mark.id.clone(),
        level: current_level as i32,
        created_at: chrono::Utc::now().timestamp(),
        model: effective.model.clone(),
        explanation_json: serde_json::to_string(&exp_json_val).unwrap_or_default(),
        annotated_json: serde_json::to_string(&annotated_snippet).ok(),
        similarity_score: 0.0,
    };

    let _ = state.store.save_confusion_revision(&revision);
    let _ = state.store.save_mark(&mark);
    revisions.push(revision);

    Ok(ConfusionDetailResponse {
        mark,
        revisions,
        active_annotated: Some(annotated_snippet),
    })
}

#[tauri::command]
fn get_confusion_detail(
    mark_id: String,
    state: State<'_, AppState>,
) -> Result<Option<ConfusionDetailResponse>, String> {
    let detail = state
        .store
        .get_confusion_detail(&mark_id)
        .map_err(|e| e.to_string())?;
    let Some((mark, revisions)) = detail else {
        return Ok(None);
    };
    let active_annotated = revisions
        .last()
        .and_then(|r| r.annotated_json.as_deref())
        .and_then(|json_str| serde_json::from_str::<AnnotatedSnippet>(json_str).ok());

    Ok(Some(ConfusionDetailResponse {
        mark,
        revisions,
        active_annotated,
    }))
}

#[tauri::command]
fn reopen_confusion_mark(mark_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .store
        .reopen_confusion_mark(&mark_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_confusion_mark(mark_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .store
        .delete_confusion_mark(&mark_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn export_confusion_markdown(
    mark_ids: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let marks = state.store.list_marks(None, None).map_err(|e| e.to_string())?;
    let filtered: Vec<_> = if let Some(ref ids) = mark_ids {
        marks.into_iter().filter(|m| ids.contains(&m.id)).collect()
    } else {
        marks
    };

    let mut md = String::from("# Noesis 学习档案：卡点与逐行精读汇编\n\n");
    for m in &filtered {
        md.push_str(&format!(
            "## 【{}】{} ({}:{}-{})\n\n",
            m.category,
            m.user_note.as_deref().unwrap_or("代码卡点"),
            m.file_path,
            m.line_start,
            m.line_end
        ));
        if let Ok(Some((_, revs))) = state.store.get_confusion_detail(&m.id) {
            if let Some(last_rev) = revs.last() {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&last_rev.explanation_json) {
                    if let Some(analogy) = val.get("analogy").and_then(|a| a.as_str()) {
                        md.push_str(&format!("> **通俗类比**：{}\n\n", analogy));
                    }
                    if let Some(guide) = val.get("guidance").and_then(|g| g.as_str()) {
                        md.push_str(&format!("- **引导思考**：{}\n", guide));
                    }
                }
                if let Some(ref annot_str) = last_rev.annotated_json {
                    if let Ok(annot) = serde_json::from_str::<AnnotatedSnippet>(annot_str) {
                        md.push_str("\n### 逐行精读注释\n```text\n");
                        for l in &annot.lines {
                            md.push_str(&format!("{:4} | {:<50} // {}\n", l.line_no, l.code, l.comment));
                        }
                        md.push_str("```\n\n");
                    }
                }
            }
        }
        md.push_str("---\n\n");
    }
    Ok(md)
}

// ----------------- F7-a: Explanation Feedback -----------------

#[tauri::command]
fn submit_explanation_feedback(
    explanation_id: String,
    card_perspective: String,
    feedback: String,
    note: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let fb = ExplanationFeedback {
        id: format!("fb-{}", chrono::Utc::now().timestamp_millis()),
        explanation_id,
        card_perspective,
        feedback,
        note,
        prompt_version: Some(PROMPT_VERSION.to_string()),
        model: None,
        created_at: chrono::Utc::now().timestamp(),
    };
    state
        .store
        .record_explanation_feedback(&fb)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_feedback_stats(
    prompt_version: Option<String>,
    model: Option<String>,
    state: State<'_, AppState>,
) -> Result<FeedbackStats, String> {
    state
        .store
        .get_feedback_stats(prompt_version.as_deref(), model.as_deref())
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            let _ = std::fs::create_dir_all(&app_data);
            let db_path = app_data.join("noesis_local.db");

            let (store, db_degraded, db_error) = match DbStore::open(&db_path) {
                Ok(s) => (s, false, None),
                Err(err) => {
                    let msg = format!(
                        "数据库打开失败 ({:?}): {}，当前学习记录仅保存在内存中",
                        db_path, err
                    );
                    eprintln!("Warning: {}", msg);
                    let mem = DbStore::in_memory()
                        .expect("In-memory database initialization failed");
                    (mem, true, Some(msg))
                }
            };

            let credentials = CredentialStore::new(Some(app_data));

            let state = AppState {
                project_manager: Mutex::new(None),
                project_map: Mutex::new(None),
                store,
                db_degraded,
                db_error,
                credentials,
                llm: LlmGateway::new(),
                indexer: CodeIndexer::new(),
                knowledge_base: KnowledgeBase::load_builtin(),
                lsp: LspManager::new(),
                cancellations: CancellationRegistry::new(),
            };

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_system_status,
            get_llm_config,
            set_llm_config,
            set_api_key,
            clear_api_key,
            test_llm_connection,
            cancel_llm_request,
            open_project,
            list_project_dir,
            get_project_tree,
            read_file,
            save_file,
            index_project,
            get_project_map,
            set_reading_step_status,
            get_reading_progress,
            get_symbol_neighborhood,
            list_project_symbols,
            explain_code,
            explain_selection_structured,
            mark_confusion,
            update_confusion_mark_explanation,
            resolve_confusion_mark,
            get_confusion_marks,
            get_learning_profile,
            list_conversations,
            create_conversation,
            load_conversation_messages,
            append_conversation_message,
            delete_conversation,
            preview_outbound,
            send_llm_chat,
            run_scratch_code,
            generate_project_exercises,
            submit_exercise_answer,
            list_due_reviews,
            detect_lsp_servers,
            lsp_find_definition,
            lsp_find_references,
            lsp_get_hover,
            get_expert_system_prompt,
            list_explanations,
            get_explanation,
            rename_explanation,
            pin_explanation,
            delete_explanation,
            preview_ask,
            ask_project_question,
            list_prompt_presets,
            save_prompt_preset,
            set_active_prompt_preset,
            delete_prompt_preset,
            reset_prompt_to_default,
            preview_effective_prompt,
            explain_confusion,
            get_confusion_detail,
            reopen_confusion_mark,
            delete_confusion_mark,
            export_confusion_markdown,
            submit_explanation_feedback,
            get_feedback_stats
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use noesis_learn::verifier::{
        CitationItem, ExplanationBullet, ExplanationSection, StructuredExplanation,
    };

    #[test]
    fn test_robot_voice_ws_end_to_end_pipeline() {
        let sample_path = PathBuf::from(r"C:\Users\123\Desktop\RobotVoiceWS-main");
        if !sample_path.exists() {
            return;
        }

        let store = DbStore::in_memory().expect("in-memory store");
        let indexer = CodeIndexer::new();
        let ros2_adapter = Ros2Adapter::new();
        let llm_adapter = LlmAppAdapter::new();
        let adapters: [&dyn DomainAdapter; 2] = [&ros2_adapter, &llm_adapter];

        let (map, _report) = indexer
            .index_workspace(&sample_path, &store, &adapters)
            .expect("index RobotVoiceWS-main");

        // Stage B verification: domain detection, symbols, entities, reading route
        assert!(map.total_symbols > 30, "expected >30 symbols in RobotVoiceWS-main");
        assert!(
            map.detected_domains.iter().any(|d| d == "ros2" || d == "llm_app"),
            "expected ros2 or llm_app domain detected"
        );
        assert!(!map.reading_route.is_empty(), "expected non-empty reading route");

        let pm = ProjectManager::new(&sample_path).expect("open project");
        let project_id = pm.normalized_root();

        // Stage C verification: ContextAssembler P0-P7 + KnowledgeBase + CitationVerifier
        let kb = KnowledgeBase::load_builtin();
        assert!(kb.entries().len() >= 31);

        let first_step = &map.reading_route[0];
        let file_payload = pm.read_file(&first_step.file_path).expect("read first step file");
        let lines: Vec<&str> = file_payload.content.lines().collect();
        let snippet_end = lines.len().min(25);
        let snippet = lines[..snippet_end].join("\n");

        let ctx_req = ContextRequest {
            kind: "explain_selection".to_string(),
            project_id: project_id.clone(),
            file: Some(first_step.file_path.clone()),
            range: Some((1, snippet_end as u32)),
            selected_code: Some(snippet.clone()),
            question: None,
            entity_ref: None,
            conversation_id: None,
            budget_tokens: 12000,
            include_learning_profile: true,
            re_explain_type: None,
            re_explain_level: None,
            previous_explanation_summary: None,
            user_note: None,
            depth_mode: None,
            custom_persona: None,
        };
        let pack = ContextAssembler::assemble(
            &sample_path,
            Some(&store),
            Some(&kb),
            &ctx_req,
            &HeuristicTokenEstimator,
            "deepseek-chat",
        );
        assert!(!pack.basis_hash.is_empty());
        assert!(pack.manifest.total_tokens <= pack.manifest.budget_tokens);

        let first_non_empty_line = lines
            .iter()
            .enumerate()
            .find(|(_, l)| !l.trim().is_empty())
            .map(|(idx, l)| ((idx + 1) as u32, l.trim().to_string()))
            .unwrap_or((1, String::new()));

        let mut dummy_exp = StructuredExplanation {
            overall: ExplanationSection {
                title: "整体".to_string(),
                one_sentence: "入口模块".to_string(),
                bullets: vec![ExplanationBullet {
                    text: "要点1".to_string(),
                    kind: "code".to_string(),
                    cites: vec!["c1".to_string()],
                    pack_ref: None,
                    status: String::new(),
                }],
                detail: "详情".to_string(),
            },
            logic: ExplanationSection::default(),
            syntax: ExplanationSection::default(),
            principle: ExplanationSection::default(),
            citations: vec![CitationItem {
                id: "c1".to_string(),
                file: first_step.file_path.clone(),
                line_start: first_non_empty_line.0,
                line_end: first_non_empty_line.0,
                quote: first_non_empty_line.1,
                status: String::new(),
                corrected_line_start: None,
                corrected_line_end: None,
                reason: None,
            }],
            unknowns: vec![],
            concepts: vec![],
        };
        let ver_summary = CitationVerifier::verify(
            &sample_path,
            &pack.manifest.loaded_knowledge_packs,
            &mut dummy_exp,
        );
        assert_eq!(ver_summary.verified_citations, 1);

        // Stage D verification: deterministic fact & cloze exercise generation
        let entities = store.get_entities_by_domain(&project_id, None, None).unwrap_or_default();
        let edges = store.get_edges_filtered(&project_id, None).unwrap_or_default();
        let symbols = store.list_project_symbols(&project_id, None).unwrap_or_default();
        let facts = generate_fact_exercises(&project_id, &entities, &edges, &symbols, 5);
        for f in &facts {
            assert!(f.verified_by_program);
        }
        if let Some(cloze) = generate_cloze_exercise(&project_id, &first_step.file_path, &snippet, None) {
            assert!(cloze.verified_by_program);
            assert!(!cloze.question.contains(&cloze.reference_answer));
        }

        // Stage E2 verification: LSP definition resolution fallback via DbStore
        if let Some(first_sym) = symbols.first() {
            let lsp = LspManager::new();
            let defs = lsp.resolve_definition(Some(&store), &project_id, &first_sym.name);
            assert!(!defs.is_empty());
        }
    }
}

