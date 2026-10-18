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
use noesis_learn::{ExplanationCard, LearnEngine, RewordingResult};
use noesis_llm::stream::CancellationRegistry;
use noesis_llm::{
    ChatMessage, ConnectionTestResult, CredentialStore, LlmConfig, LlmGateway, OutboundPreview,
};
use noesis_llmapp::LlmAppAdapter;
use noesis_lsp::{DefinitionLocation, HoverInfo, LspManager, LspServerStatus, ReferenceLocation};
use noesis_project::{FileContentPayload, FileNode, ProjectManager};
use noesis_ros2::Ros2Adapter;
use noesis_runner::{CodeRunner, RunRequest, RunResponse};
use noesis_store::{
    CachedExplanation, ConceptProfileItem, ConfusionMark, ConversationRecord, DbStore,
    ExerciseRecord, GraphSymbol, ReadingProgressRecord, StoredMessageRecord, SymbolNeighborhood,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Manager, State};

const EXPERT_SYSTEM_PROMPT: &str = include_str!("../../prompts/code_guide_expert.system.md");

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
            if let Ok(mut exp) = serde_json::from_str::<StructuredExplanation>(&cached.json) {
                let verification = CitationVerifier::verify(
                    &root_path,
                    &pack.manifest.loaded_knowledge_packs,
                    &mut exp,
                );
                return Ok(StructuredExplainResponse {
                    explanation: Some(exp),
                    verification,
                    manifest: pack.manifest,
                    dropped: pack.dropped,
                    cached: true,
                    raw_unverified_output: None,
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
            content: EXPERT_SYSTEM_PROMPT.to_string(),
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
            let mut retry_msgs = messages;
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
            let verification = CitationVerifier::verify(
                &root_path,
                &pack.manifest.loaded_knowledge_packs,
                &mut exp,
            );
            if let Ok(json_str) = serde_json::to_string(&exp) {
                let stats_str = serde_json::to_string(&verification).unwrap_or_default();
                let _ = state.store.save_explanation_cache(&CachedExplanation {
                    id: format!("exp-{}", pack.basis_hash),
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
                });
            }
            Ok(StructuredExplainResponse {
                explanation: Some(exp),
                verification,
                manifest: pack.manifest,
                dropped: pack.dropped,
                cached: false,
                raw_unverified_output: None,
            })
        }
        Err(_) => Ok(StructuredExplainResponse {
            explanation: None,
            verification: VerificationSummary::default(),
            manifest: pack.manifest,
            dropped: pack.dropped,
            cached: false,
            raw_unverified_output: Some(raw_reply),
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
            get_expert_system_prompt
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

