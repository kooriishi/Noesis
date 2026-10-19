pub mod prompts;
pub mod sink;
pub mod state;
pub mod types;

use chrono::Utc;
use noesis_indexer::{DomainAdapter, ProjectMap};
use noesis_learn::context::{
    ContextAssembler, ContextManifest, ContextRequest, HeuristicTokenEstimator,
    PROMPT_VERSION,
};
use noesis_learn::exercises::{
    compute_sm2_review, generate_cloze_exercise, generate_fact_exercises,
    grade_deterministic_answer, map_answer_to_quality, refresh_project_exercises_freshness,
};
use noesis_learn::quality::{LayerQualityReport, QualityGate};
use noesis_learn::verifier::{
    extract_and_parse_explanation_json, CitationItem, CitationVerifier,
    StructuredExplanation,
};
use noesis_learn::{ExplanationCard, LearnEngine, RewordingResult};
use noesis_llm::stream::{budget_conversation_history, split_chat_citations};
use noesis_llm::{ChatMessage, ConnectionTestResult, LlmConfig, OutboundPreview};
use noesis_llmapp::LlmAppAdapter;
use noesis_lsp::{DefinitionLocation, HoverInfo, LspServerStatus, ReferenceLocation};
use noesis_project::{FileContentPayload, FileNode, ProjectManager};
use noesis_ros2::Ros2Adapter;
use noesis_runner::{CodeRunner, RunRequest, RunResponse};
use noesis_store::{
    ConceptProfileItem, ConfusionMark,
    ConfusionRevision, ConversationRecord, ExerciseRecord, ExplanationFeedback,
    ExplanationSummary, FeedbackStats, GraphSymbol, PromptPreset, ReadingProgressRecord,
    StoredMessageRecord, SymbolNeighborhood,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub use sink::{EventSink, NullEventSink};
pub use state::{ActiveTaskGuard, AppState, ACTIVE_TASKS, IS_INDEXING};
pub use types::*;

// Re-export prompt constants
pub use prompts::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerExplainResponse {
    pub layer: String,
    pub markdown: String,
    pub quality: LayerQualityReport,
    pub citations: Vec<CitationItem>,
    pub concepts: Vec<String>,
    pub cached: bool,
    pub explanation_id: Option<String>,
}

pub fn supported_commands() -> Vec<&'static str> {
    vec![
        "get_system_status",
        "get_llm_config",
        "set_llm_config",
        "set_api_key",
        "clear_api_key",
        "test_llm_connection",
        "cancel_llm_request",
        "open_project",
        "list_project_dir",
        "get_project_tree",
        "read_file",
        "save_file",
        "index_project",
        "get_project_map",
        "set_reading_step_status",
        "get_reading_progress",
        "get_symbol_neighborhood",
        "list_project_symbols",
        "explain_code",
        "explain_selection_structured",
        "explain_layer",
        "mark_confusion",
        "update_confusion_mark_explanation",
        "resolve_confusion_mark",
        "get_confusion_marks",
        "get_learning_profile",
        "list_conversations",
        "create_conversation",
        "load_conversation_messages",
        "append_conversation_message",
        "delete_conversation",
        "preview_outbound",
        "send_llm_chat",
        "run_scratch_code",
        "generate_project_exercises",
        "submit_exercise_answer",
        "list_due_reviews",
        "detect_lsp_servers",
        "lsp_find_definition",
        "lsp_find_references",
        "lsp_get_hover",
        "get_expert_system_prompt",
        "list_explanations",
        "get_explanation",
        "rename_explanation",
        "pin_explanation",
        "delete_explanation",
        "preview_ask",
        "ask_project_question",
        "list_prompt_presets",
        "save_prompt_preset",
        "set_active_prompt_preset",
        "delete_prompt_preset",
        "reset_prompt_to_default",
        "preview_effective_prompt",
        "explain_confusion",
        "get_confusion_detail",
        "reopen_confusion_mark",
        "delete_confusion_mark",
        "export_confusion_markdown",
        "submit_explanation_feedback",
        "get_feedback_stats",
    ]
}

pub fn is_command_supported(cmd: &str) -> bool {
    supported_commands().contains(&cmd)
}

// -------------------------------------------------------------------------------------------------
// System Commands
// -------------------------------------------------------------------------------------------------

pub async fn get_system_status(state: &AppState) -> Result<SystemStatus, String> {
    let comp = CodeRunner::get_detected_compiler_info(true);
    Ok(SystemStatus {
        db_degraded: state.db_degraded,
        db_error: state.db_error.clone(),
        compiler_info: comp,
    })
}

pub async fn get_expert_system_prompt() -> Result<String, String> {
    Ok(EXPERT_SYSTEM_PROMPT.to_string())
}

// -------------------------------------------------------------------------------------------------
// LLM Config & Outbound
// -------------------------------------------------------------------------------------------------

pub async fn get_llm_config(state: &AppState) -> Result<LlmConfig, String> {
    Ok(state.credentials.get_config_view())
}

pub async fn set_llm_config(state: &AppState, config: LlmConfig) -> Result<LlmConfig, String> {
    state.credentials.update_config(config).map_err(|e| e.to_string())
}

pub async fn set_api_key(state: &AppState, api_key: String) -> Result<LlmConfig, String> {
    state.credentials.set_api_key(&api_key).map_err(|e| e.to_string())
}

pub async fn clear_api_key(state: &AppState) -> Result<LlmConfig, String> {
    state.credentials.clear_api_key().map_err(|e| e.to_string())
}

pub async fn test_llm_connection(state: &AppState, config: LlmConfig) -> Result<ConnectionTestResult, String> {
    let _task = ActiveTaskGuard::new();
    let effective = state.credentials.resolve_effective_config(&config);
    Ok(state.llm.test_connection(&effective).await)
}

pub async fn cancel_llm_request(state: &AppState, request_id: String) -> Result<bool, String> {
    Ok(state.cancellations.cancel(&request_id))
}

pub async fn preview_outbound(
    state: &AppState,
    config: LlmConfig,
    messages: Vec<ChatMessage>,
) -> Result<OutboundPreview, String> {
    let effective = state.credentials.resolve_effective_config(&config);
    state.llm.filter().inspect_outbound(&effective, &messages).map_err(|e| e.to_string())
}

pub async fn send_llm_chat(
    state: &AppState,
    config: LlmConfig,
    messages: Vec<ChatMessage>,
) -> Result<String, String> {
    let _task = ActiveTaskGuard::new();
    let effective = state.credentials.resolve_effective_config(&config);
    state.llm.send_chat(&effective, messages).await.map_err(|e| e.to_string())
}

// -------------------------------------------------------------------------------------------------
// Project & File Operations
// -------------------------------------------------------------------------------------------------

pub async fn open_project(state: &AppState, path: String) -> Result<FileNode, String> {
    let pm = ProjectManager::new(&path).map_err(|e| e.to_string())?;
    let tree = pm.build_tree(2).map_err(|e| e.to_string())?;
    let mut guard = state.project_manager.lock().map_err(|e| e.to_string())?;
    *guard = Some(pm);
    Ok(tree)
}

pub async fn list_project_dir(state: &AppState, rel_dir: Option<String>) -> Result<Vec<FileNode>, String> {
    let mgr_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
    if let Some(pm) = mgr_guard.as_ref() {
        let rel = rel_dir.unwrap_or_default();
        pm.list_dir(&rel).map_err(|e| e.to_string())
    } else {
        Err("尚未打开任何工程".to_string())
    }
}

pub async fn get_project_tree(state: &AppState) -> Result<Option<FileNode>, String> {
    let mgr_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
    if let Some(pm) = mgr_guard.as_ref() {
        let tree = pm.build_tree(6).map_err(|e| e.to_string())?;
        Ok(Some(tree))
    } else {
        Ok(None)
    }
}

pub async fn read_file(state: &AppState, path: String) -> Result<FileContentPayload, String> {
    let mgr_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
    if let Some(pm) = mgr_guard.as_ref() {
        pm.read_file(&path).map_err(|e| e.to_string())
    } else {
        Err("尚未打开任何工程，请先在左侧选择工程目录。".to_string())
    }
}

pub async fn save_file(state: &AppState, path: String, content: String) -> Result<String, String> {
    let mgr_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
    if let Some(pm) = mgr_guard.as_ref() {
        pm.save_file(&path, &content).map_err(|e| e.to_string())
    } else {
        Err("尚未打开任何工程，无法保存文件。".to_string())
    }
}

// -------------------------------------------------------------------------------------------------
// Indexer Commands (G1 non-blocking + AtomicBool)
// -------------------------------------------------------------------------------------------------

pub async fn index_project(state: Arc<AppState>) -> Result<ProjectMap, String> {
    if IS_INDEXING.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return Err("索引正在进行中，请稍候...".to_string());
    }

    struct IndexingGuard;
    impl Drop for IndexingGuard {
        fn drop(&mut self) {
            IS_INDEXING.store(false, Ordering::SeqCst);
        }
    }
    let _guard = IndexingGuard;
    let _task = ActiveTaskGuard::new();

    let (root_path, project_id) = {
        let mgr_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        let pm = mgr_guard.as_ref().ok_or("No project opened yet")?;
        (pm.root_path().to_path_buf(), pm.normalized_root())
    };

    let st = Arc::clone(&state);
    let res = tokio::task::spawn_blocking(move || {
        let _ = st.store.ensure_schema();
        let ros2_adapter = Ros2Adapter::new();
        let llmapp_adapter = LlmAppAdapter::new();
        let adapters: [&dyn DomainAdapter; 2] = [&ros2_adapter, &llmapp_adapter];

        let (map, _report) = st
            .indexer
            .index_workspace(&root_path, &st.store, &adapters)
            .map_err(|e| {
                let err_str = e.to_string();
                if err_str.contains("SQLITE_BUSY") || err_str.contains("database is locked") || err_str.to_lowercase().contains("busy") {
                    "数据库被另一个 Noesis 窗口占用，请关闭其他窗口后重试".to_string()
                } else {
                    err_str
                }
            })?;

        refresh_project_exercises_freshness(&root_path, &st.store, &project_id);
        *st.project_map.lock().unwrap() = Some(map.clone());
        Ok::<ProjectMap, String>(map)
    })
    .await
    .map_err(|e| format!("Indexing task join error: {}", e))??;

    Ok(res)
}

pub async fn get_project_map(state: &AppState) -> Result<Option<ProjectMap>, String> {
    let map = state
        .project_map
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    Ok(map)
}

pub async fn set_reading_step_status(
    state: &AppState,
    step_id: String,
    status: String,
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

pub async fn get_reading_progress(state: &AppState) -> Result<Vec<ReadingProgressRecord>, String> {
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

pub async fn get_symbol_neighborhood(
    state: &AppState,
    symbol_id: String,
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

pub async fn list_project_symbols(
    state: &AppState,
    file_path: Option<String>,
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

// -------------------------------------------------------------------------------------------------
// Four-Layer & Explanation Commands (G3 & G5)
// -------------------------------------------------------------------------------------------------

pub async fn explain_code(
    state: &AppState,
    snippet: String,
    file_path: String,
    range: (u32, u32),
) -> Result<Vec<ExplanationCard>, String> {
    let _task = ActiveTaskGuard::new();
    let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
    let source_matches = if let Some(pm) = pm_guard.as_ref() {
        if let Ok(payload) = pm.read_file(&file_path) {
            LearnEngine::verify_source_reference(&payload.content, range.0, range.1, &snippet)
        } else {
            false
        }
    } else {
        false
    };
    Ok(LearnEngine::generate_structured_cards(
        &file_path,
        &snippet,
        range.0,
        range.1,
        source_matches,
    ))
}

pub async fn explain_selection_structured(
    state: &AppState,
    file_path: String,
    snippet: String,
    start_line: u32,
    end_line: u32,
    force_refresh: bool,
    depth_mode: Option<String>,
) -> Result<StructuredExplainResponse, String> {
    let _task = ActiveTaskGuard::new();
    let (root_path, project_id) = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        if let Some(pm) = pm_guard.as_ref() {
            (pm.root_path().to_path_buf(), pm.normalized_root())
        } else {
            (PathBuf::from("."), "default".to_string())
        }
    };

    let effective = state.credentials.resolve_effective_config(&state.credentials.get_config_view());
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
        "项目上下文：\n\n{}\n\n选区代码：\n```\n{}\n```",
        context_chunks.join("\n\n"),
        snippet
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

    let mut effective_with_tokens = effective.clone();
    effective_with_tokens.max_tokens = if depth_mode.as_deref() == Some("deep") { 8192 } else { 4096 };
    let raw_resp = state
        .llm
        .send_chat(&effective_with_tokens, messages)
        .await
        .map_err(|e| e.to_string())?;

    let parsed_opt = extract_and_parse_explanation_json(&raw_resp);
    let mut final_exp = parsed_opt.unwrap_or_default();
    let ver_summary = CitationVerifier::verify(&root_path, &[], &mut final_exp);
    let generic_phrases = QualityGate::load_generic_phrases(&root_path);
    let quality = QualityGate::evaluate(&final_exp, &snippet, depth_mode.as_deref(), &generic_phrases);

    let exp_id = format!("exp-{}", Utc::now().timestamp_millis());
    Ok(StructuredExplainResponse {
        explanation: Some(final_exp),
        verification: ver_summary,
        manifest: pack.manifest,
        dropped: vec![],
        cached: false,
        raw_unverified_output: Some(raw_resp),
        quality: Some(quality),
        explanation_id: Some(exp_id),
    })
}

/// G3: explain_layer independently handles streaming of one specific layer
pub async fn explain_layer(
    state: &AppState,
    layer: String,
    file: String,
    range: (u32, u32),
    snippet: String,
    depth_mode: Option<String>,
    force_refresh: Option<bool>,
    sink: Option<&dyn EventSink>,
) -> Result<LayerExplainResponse, String> {
    let _task = ActiveTaskGuard::new();
    let is_force = force_refresh.unwrap_or(false);
    let depth_str = depth_mode.unwrap_or_else(|| "standard".to_string());
    let is_deep = depth_str == "deep";

    let (root_path, project_id) = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        if let Some(pm) = pm_guard.as_ref() {
            (pm.root_path().to_path_buf(), pm.normalized_root())
        } else {
            (PathBuf::from("."), "default".to_string())
        }
    };

    let effective = state.credentials.resolve_effective_config(&state.credentials.get_config_view());
    let prompt_version = PROMPT_VERSION.to_string();
    let generic_phrases = QualityGate::load_generic_phrases(&root_path);
    let estimator = HeuristicTokenEstimator;

    let ctx_req = ContextRequest {
        kind: "explain_selection".to_string(),
        project_id: project_id.clone(),
        file: Some(file.clone()),
        range: Some(range),
        selected_code: Some(snippet.clone()),
        question: None,
        entity_ref: None,
        conversation_id: None,
        budget_tokens: if is_deep { 24000 } else { 16000 },
        include_learning_profile: true,
        re_explain_type: None,
        re_explain_level: None,
        previous_explanation_summary: None,
        user_note: None,
        depth_mode: Some(depth_str.clone()),
        custom_persona: None,
    };

    let pack = ContextAssembler::assemble(
        &root_path,
        Some(&state.store),
        Some(&state.knowledge_base),
        &ctx_req,
        &estimator,
        &effective.model,
    );

    let layer_prompt_template = match layer.to_lowercase().as_str() {
        "overall" => PROMPT_OVERALL,
        "logic" => PROMPT_LOGIC,
        "syntax" => PROMPT_SYNTAX,
        "principle" => PROMPT_PRINCIPLE,
        _ => return Err(format!("Unsupported layer: {}", layer)),
    };

    let max_tokens = match (layer.to_lowercase().as_str(), is_deep) {
        ("overall", false) => 2048,
        ("overall", true) => 4096,
        ("logic", false) => 4096,
        ("logic", true) => 8192,
        ("syntax", false) => 6144,
        ("syntax", true) => 8192,
        ("principle", false) => 3072,
        ("principle", true) => 6144,
        _ => 4096,
    };

    let system_prompt = format!(
        "{}\n\n---\n## 锁定核心规则\n{}\n\n---\n## 教学风格与规范\n{}\n\n---\n{}",
        EXPERT_SYSTEM_PROMPT,
        LOCKED_RULES_PROMPT,
        DEFAULT_PERSONA_PROMPT,
        layer_prompt_template
    );

    let mut context_chunks = Vec::new();
    for item in &pack.items {
        if item.kind != "system_prompt" {
            context_chunks.push(item.text.clone());
        }
    }

    let user_prompt = format!(
        "目标文件: {}\n行号范围: L{}-L{}\n模式: {}\n\n项目上下文信息:\n{}\n\n选区代码:\n```\n{}\n```",
        file,
        range.0,
        range.1,
        if is_deep { "深度解读 (Deep Mode)" } else { "标准解读 (Standard Mode)" },
        context_chunks.join("\n\n"),
        snippet
    );

    let mut effective = state.credentials.resolve_effective_config(&state.credentials.get_config_view());
    effective.max_tokens = max_tokens;

    let system_prompt = format!(
        "{}\n\n---\n## 锁定核心规则\n{}\n\n---\n## 教学风格与规范\n{}\n\n---\n{}",
        EXPERT_SYSTEM_PROMPT,
        LOCKED_RULES_PROMPT,
        DEFAULT_PERSONA_PROMPT,
        layer_prompt_template
    );

    let mut context_chunks = Vec::new();
    for item in &pack.items {
        if item.kind != "system_prompt" {
            context_chunks.push(item.text.clone());
        }
    }

    let user_prompt = format!(
        "目标文件: {}\n行号范围: L{}-L{}\n模式: {}\n\n项目上下文信息:\n{}\n\n选区代码:\n```\n{}\n```",
        file,
        range.0,
        range.1,
        if is_deep { "深度解读 (Deep Mode)" } else { "标准解读 (Standard Mode)" },
        context_chunks.join("\n\n"),
        snippet
    );

    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system_prompt.clone(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: user_prompt.clone(),
        },
    ];

    let mut generated_markdown = if let Some(s) = sink {
        state
            .llm
            .send_chat_stream(&effective, messages.clone(), |delta| {
                let _ = s.send_event(json!({
                    "type": "token",
                    "layer": layer,
                    "token": delta
                }));
            })
            .await
            .map_err(|e| e.to_string())?
    } else {
        state
            .llm
            .send_chat(&effective, messages.clone())
            .await
            .map_err(|e| e.to_string())?
    };

    let mut quality = QualityGate::evaluate_layer(
        &layer,
        &generated_markdown,
        &snippet,
        is_deep,
        &generic_phrases,
    );

    if !quality.passed {
        let retry_issues = quality.issues.join("；");
        let retry_user_prompt = format!(
            "{}\n\n【质检重试指令】上一次输出未通过程序质检，具体问题如下：\n{}\n请严格针对上述问题重新生成完整的 Markdown 解读！",
            user_prompt, retry_issues
        );
        let retry_messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt,
            },
            ChatMessage {
                role: "user".to_string(),
                content: retry_user_prompt,
            },
        ];

        if let Ok(retry_resp) = state.llm.send_chat(&effective, retry_messages).await {
            generated_markdown = retry_resp;
            quality = QualityGate::evaluate_layer(
                &layer,
                &generated_markdown,
                &snippet,
                is_deep,
                &generic_phrases,
            );
        }
    }

    let mut concepts = Vec::new();
    if let Some(c_start) = generated_markdown.find("<!-- concepts:") {
        if let Some(c_end) = generated_markdown[c_start..].find("-->") {
            let tag_content = &generated_markdown[c_start + 14..c_start + c_end];
            for c in tag_content.split(',') {
                let trimmed = c.trim();
                if !trimmed.is_empty() {
                    concepts.push(trimmed.to_string());
                }
            }
        }
    }

    if let Some(s) = sink {
        let _ = s.send_event(json!({
            "type": "done",
            "layer": layer,
            "markdown": generated_markdown,
            "quality": quality,
            "concepts": concepts
        }));
    }

    Ok(LayerExplainResponse {
        layer,
        markdown: generated_markdown,
        quality,
        citations: vec![],
        concepts,
        cached: false,
        explanation_id: None,
    })
}

// -------------------------------------------------------------------------------------------------
// Confusion Marks & Re-Explain (G3/G5)
// -------------------------------------------------------------------------------------------------

pub async fn mark_confusion(
    state: &AppState,
    file_path: String,
    start_line: u32,
    end_line: u32,
    snippet: String,
    category: String,
    user_note: Option<String>,
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

pub async fn explain_confusion(
    state: &AppState,
    mark_id: Option<String>,
    file: Option<String>,
    range: Option<(u32, u32)>,
    snippet: Option<String>,
    confused_layer: Option<String>,
    user_note: Option<String>,
    level: Option<u8>,
    sink: Option<&dyn EventSink>,
) -> Result<ConfusionDetailResponse, String> {
    let _task = ActiveTaskGuard::new();
    let (root_path, project_id) = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        if let Some(pm) = pm_guard.as_ref() {
            (pm.root_path().to_path_buf(), pm.normalized_root())
        } else {
            (PathBuf::from("."), "default".to_string())
        }
    };

    let (mut mark, mut revisions) = if let Some(ref mid) = mark_id {
        let detail = state.store.get_confusion_detail(mid).map_err(|e| e.to_string())?;
        detail.ok_or_else(|| format!("Mark '{}' not found", mid))?
    } else {
        let fp = file.unwrap_or_default();
        let (s_line, e_line) = range.unwrap_or((1, 1));
        let snip = snippet.unwrap_or_default();
        let cat = confused_layer.clone().unwrap_or_else(|| "overall".to_string());
        let mid = format!("mark-{}-{}", chrono::Utc::now().timestamp_millis(), s_line);
        let mark = ConfusionMark {
            id: mid,
            project_id: Some(project_id.clone()),
            file_path: fp,
            line_start: s_line,
            line_end: e_line,
            code_snippet: snip,
            category: cat.clone(),
            confusion_type: Some(cat),
            explanation: None,
            ai_explanation: None,
            model: None,
            level: level.unwrap_or(1) as i32,
            resolved_at: None,
            resolved_level: None,
            user_note: user_note.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            review_exercise_id: None,
            concepts_json: None,
        };
        let _ = state.store.save_mark(&mark);
        (mark, Vec::new())
    };

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
            content: PROMPT_CONFUSION.to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!(
                "目标文件: {}\n行号范围: L{}-L{}\n答疑难度等级: Level {}\n卡点视角: {}\n用户具体疑问: {}\n\n上下文与代码事实:\n{}\n\n选区代码:\n```\n{}\n```",
                mark.file_path,
                mark.line_start,
                mark.line_end,
                current_level,
                mark.category,
                mark.user_note.as_deref().unwrap_or("未能理解运行流程与核心逻辑"),
                combined_prompt,
                mark.code_snippet
            ),
        },
    ];

    let generated_markdown = if let Some(s) = sink {
        state
            .llm
            .send_chat_stream(&effective, messages.clone(), |delta| {
                let _ = s.send_event(json!({
                    "type": "token",
                    "token": delta
                }));
            })
            .await
            .map_err(|e| e.to_string())?
    } else {
        state
            .llm
            .send_chat(&effective, messages.clone())
            .await
            .map_err(|e| e.to_string())?
    };

    let mut concepts = Vec::new();
    if let Some(c_start) = generated_markdown.find("<!-- concepts:") {
        if let Some(c_end) = generated_markdown[c_start..].find("-->") {
            let tag_content = &generated_markdown[c_start + 14..c_start + c_end];
            for c in tag_content.split(',') {
                let trimmed = c.trim();
                if !trimmed.is_empty() {
                    concepts.push(trimmed.to_string());
                }
            }
        }
    }
    if concepts.is_empty() {
        concepts.push(mark.category.clone());
    }

    let concepts_json = serde_json::to_string(&concepts).ok();
    mark.concepts_json = concepts_json;
    mark.ai_explanation = Some(generated_markdown.clone());
    mark.model = Some(effective.model.clone());

    let rev_id = format!("rev-{}-{}", mark.id, chrono::Utc::now().timestamp_millis());
    let revision = ConfusionRevision {
        id: rev_id,
        mark_id: mark.id.clone(),
        level: current_level as i32,
        created_at: chrono::Utc::now().timestamp(),
        model: effective.model.clone(),
        explanation_json: generated_markdown.clone(),
        annotated_json: None,
        similarity_score: 0.0,
    };

    let _ = state.store.save_confusion_revision(&revision);
    let _ = state.store.save_mark(&mark);
    revisions.push(revision);

    let res = ConfusionDetailResponse {
        mark,
        revisions,
        active_annotated: None,
    };

    if let Some(s) = sink {
        let _ = s.send_event(json!({
            "type": "done",
            "detail": res
        }));
    }

    Ok(res)
}


pub async fn update_confusion_mark_explanation(
    state: &AppState,
    id: String,
    ai_explanation: String,
    model: String,
) -> Result<(), String> {
    state
        .store
        .update_mark_ai_explanation(&id, &ai_explanation, &model)
        .map_err(|e| e.to_string())
}

pub async fn resolve_confusion_mark(
    state: &AppState,
    id: String,
    level: Option<u8>,
    resolved: bool,
) -> Result<(), String> {
    state
        .store
        .update_mark_level_and_resolution(&id, level.unwrap_or(1) as i32, resolved)
        .map_err(|e| e.to_string())
}

pub async fn get_confusion_marks(
    state: &AppState,
    file_path: Option<String>,
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

pub async fn get_learning_profile(state: &AppState) -> Result<Vec<ConceptProfileItem>, String> {
    let project_id = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        pm_guard.as_ref().map(|pm| pm.normalized_root())
    };
    state
        .store
        .list_concept_profile(project_id.as_deref())
        .map_err(|e| e.to_string())
}

pub async fn get_confusion_detail(
    state: &AppState,
    id: String,
) -> Result<Option<ConfusionDetailResponse>, String> {
    let detail = state.store.get_confusion_detail(&id).map_err(|e| e.to_string())?;
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

pub async fn reopen_confusion_mark(state: &AppState, id: String) -> Result<bool, String> {
    state.store.reopen_confusion_mark(&id).map_err(|e| e.to_string())?;
    Ok(true)
}

pub async fn delete_confusion_mark(state: &AppState, id: String) -> Result<bool, String> {
    state.store.delete_confusion_mark(&id).map_err(|e| e.to_string())?;
    Ok(true)
}

pub async fn export_confusion_markdown(state: &AppState) -> Result<String, String> {
    let marks = state.store.list_marks(None, None).map_err(|e| e.to_string())?;
    let mut md = String::from("# Noesis 学习档案：卡点与逐行精读汇编\n\n");
    for m in &marks {
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
            }
        }
        md.push_str("---\n\n");
    }
    Ok(md)
}

pub async fn submit_explanation_feedback(
    state: &AppState,
    fb: ExplanationFeedback,
) -> Result<bool, String> {
    state.store.record_explanation_feedback(&fb).map_err(|e| e.to_string())?;
    Ok(true)
}

pub async fn get_feedback_stats(
    state: &AppState,
    prompt_version: Option<String>,
    model: Option<String>,
) -> Result<FeedbackStats, String> {
    state
        .store
        .get_feedback_stats(prompt_version.as_deref(), model.as_deref())
        .map_err(|e| e.to_string())
}

// -------------------------------------------------------------------------------------------------
// Chat & Intelligent Agent (G5)
// -------------------------------------------------------------------------------------------------

pub async fn list_conversations(state: &AppState) -> Result<Vec<ConversationRecord>, String> {
    let project_id = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        pm_guard.as_ref().map(|pm| pm.normalized_root())
    };
    state.store.list_conversations(project_id.as_deref()).map_err(|e| e.to_string())
}

pub async fn create_conversation(state: &AppState, title: String) -> Result<ConversationRecord, String> {
    let project_id = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        pm_guard
            .as_ref()
            .map(|pm| pm.normalized_root())
            .unwrap_or_else(|| "default".to_string())
    };
    let id = format!("conv-{}", Utc::now().timestamp_millis());
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

pub async fn load_conversation_messages(
    state: &AppState,
    conversation_id: String,
) -> Result<Vec<StoredMessageRecord>, String> {
    state.store.list_messages(&conversation_id, 50).map_err(|e| e.to_string())
}

pub async fn append_conversation_message(
    state: &AppState,
    conversation_id: String,
    role: String,
    content: String,
) -> Result<StoredMessageRecord, String> {
    let est = HeuristicTokenEstimator;
    let tokens = noesis_learn::context::TokenEstimator::estimate(&est, &content);
    let msg = StoredMessageRecord {
        id: format!("msg-{}", Utc::now().timestamp_nanos_opt().unwrap_or(0)),
        conversation_id,
        role,
        content,
        meta_json: "{}".to_string(),
        tokens,
        created_at: Utc::now().timestamp(),
    };
    state.store.append_message(&msg).map_err(|e| e.to_string())?;
    Ok(msg)
}

pub async fn delete_conversation(state: &AppState, id: String) -> Result<bool, String> {
    state.store.delete_conversation(&id).map_err(|e| e.to_string())?;
    Ok(true)
}

pub async fn preview_ask(
    state: &AppState,
    question: String,
    file: Option<String>,
    range: Option<(u32, u32)>,
    selected_code: Option<String>,
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

pub async fn ask_project_question(
    state: &AppState,
    conversation_id: Option<String>,
    question: String,
    file: Option<String>,
    range: Option<(u32, u32)>,
    selected_code: Option<String>,
    use_tools: Option<bool>,
    sink: Option<&dyn EventSink>,
) -> Result<String, String> {
    let _task = ActiveTaskGuard::new();
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

    if let Some(s) = sink {
        let _ = s.send_event(json!({
            "type": "context_ready",
            "manifest": pack.manifest
        }));
    }

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

    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: sys_prompt,
    }];

    let conv_id = match conversation_id {
        Some(cid) => cid,
        None => {
            let c = create_conversation(state, question.chars().take(20).collect()).await?;
            c.id
        }
    };

    let history_records = state.store.list_messages(&conv_id, 100).unwrap_or_default();
    let history_plan = budget_conversation_history(&history_records, "", None, 16000, &estimator);
    messages.extend(history_plan.retained_messages);

    messages.push(ChatMessage {
        role: "user".to_string(),
        content: user_prompt,
    });

    let _ = append_conversation_message(state, conv_id.clone(), "user".to_string(), question).await;

    let mut effective_with_tokens = effective.clone();
    effective_with_tokens.max_tokens = 8192;
    let final_answer = if let Some(s) = sink {
        state
            .llm
            .send_chat_stream(&effective_with_tokens, messages, |delta| {
                let _ = s.send_event(json!({
                    "type": "token",
                    "token": delta
                }));
            })
            .await
            .map_err(|e| e.to_string())?
    } else {
        state
            .llm
            .send_chat(&effective_with_tokens, messages)
            .await
            .map_err(|e| e.to_string())?
    };

    let (_clean_text, cites) = split_chat_citations(&final_answer);

    if let Some(s) = sink {
        let _ = s.send_event(json!({
            "type": "citation_check",
            "citations": cites
        }));
        let _ = s.send_event(json!({
            "type": "done",
            "conversation_id": conv_id
        }));
    }

    let _ = append_conversation_message(state, conv_id.clone(), "assistant".to_string(), final_answer).await;

    Ok(conv_id)
}

// -------------------------------------------------------------------------------------------------
// Exercises & SM2 Reviews
// -------------------------------------------------------------------------------------------------

pub async fn generate_project_exercises(
    state: &AppState,
    count: Option<usize>,
    file_path: Option<String>,
    snippet: Option<String>,
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

pub async fn submit_exercise_answer(
    state: &AppState,
    exercise_id: String,
    answer: String,
    hesitation: Option<String>,
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

    let feedback = if is_correct {
        "回答正确！".to_string()
    } else {
        format!("参考答案：{}", ex.reference_answer)
    };

    Ok(SubmitExerciseResult {
        is_correct,
        quality: quality as u8,
        exercise_id: Some(ex.id),
        feedback: Some(feedback),
        next_interval_days: Some(review.interval_days),
        next_ease_factor: Some(review.ease),
        next_review_at: Some(review.next_review_at.clone()),
        review,
        reference_answer: ex.reference_answer,
        verified_by_program: ex.verified_by_program,
    })
}

pub async fn list_due_reviews(state: &AppState) -> Result<Vec<ExerciseRecord>, String> {
    state.store.list_due_exercises().map_err(|e| e.to_string())
}

// -------------------------------------------------------------------------------------------------
// LSP Integration
// -------------------------------------------------------------------------------------------------

pub async fn detect_lsp_servers(state: &AppState) -> Result<Vec<LspServerStatus>, String> {
    Ok(state.lsp.detect_servers())
}

pub async fn lsp_find_definition(
    state: &AppState,
    symbol_name: String,
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

pub async fn lsp_find_references(
    state: &AppState,
    symbol_name: String,
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

pub async fn lsp_get_hover(
    state: &AppState,
    file_path: String,
    line: u32,
    word: String,
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

// -------------------------------------------------------------------------------------------------
// Saved Explanations
// -------------------------------------------------------------------------------------------------

pub async fn list_explanations(
    state: &AppState,
    file_path: Option<String>,
    query: Option<String>,
    pinned_only: Option<bool>,
    limit: Option<usize>,
    offset: Option<usize>,
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

pub async fn get_explanation(
    state: &AppState,
    id: String,
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

pub async fn rename_explanation(state: &AppState, id: String, new_title: String) -> Result<(), String> {
    state.store.rename_explanation(&id, &new_title).map_err(|e| e.to_string())
}

pub async fn pin_explanation(state: &AppState, id: String, pinned: bool) -> Result<(), String> {
    state.store.pin_explanation(&id, pinned).map_err(|e| e.to_string())
}

pub async fn delete_explanation(state: &AppState, id: String) -> Result<(), String> {
    state.store.delete_explanation(&id).map_err(|e| e.to_string())
}

// -------------------------------------------------------------------------------------------------
// Prompt Presets
// -------------------------------------------------------------------------------------------------

pub async fn list_prompt_presets(state: &AppState, scope: String) -> Result<Vec<PromptPreset>, String> {
    state.store.list_prompt_presets(Some(&scope)).map_err(|e| e.to_string())
}

pub async fn save_prompt_preset(state: &AppState, preset: PromptPreset) -> Result<(), String> {
    state.store.save_prompt_preset(&preset).map_err(|e| e.to_string())
}

pub async fn set_active_prompt_preset(state: &AppState, id: String) -> Result<(), String> {
    state.store.set_active_prompt_preset(&id, "explanation").map_err(|e| e.to_string())
}

pub async fn delete_prompt_preset(state: &AppState, id: String) -> Result<(), String> {
    state.store.delete_prompt_preset(&id).map_err(|e| e.to_string())
}

pub async fn reset_prompt_to_default(state: &AppState, scope: String) -> Result<(), String> {
    state.store.reset_prompt_to_default(&scope).map_err(|e| e.to_string())
}

pub async fn preview_effective_prompt(state: &AppState, scope: String) -> Result<EffectivePromptPreview, String> {
    let preset = state
        .store
        .get_active_prompt_preset(&scope)
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| PromptPreset {
            id: "default".to_string(),
            name: "默认人设".to_string(),
            scope: scope.clone(),
            persona_text: DEFAULT_PERSONA_PROMPT.to_string(),
            created_at: 0,
            updated_at: 0,
            is_active: true,
            is_builtin: true,
        });

    let full_prompt = format!(
        "{}\n\n---\n## 锁定规则 (不可编辑)\n{}\n\n---\n## 教学风格与人设 (已激活预设: {})\n{}",
        EXPERT_SYSTEM_PROMPT,
        LOCKED_RULES_PROMPT,
        preset.name,
        preset.persona_text
    );

    Ok(EffectivePromptPreview {
        scope,
        preset_name: preset.name,
        l1_persona: preset.persona_text,
        l0_locked: LOCKED_RULES_PROMPT.to_string(),
        full_prompt,
    })
}

// -------------------------------------------------------------------------------------------------
// Code Runner Sandbox (G7)
// -------------------------------------------------------------------------------------------------

pub async fn run_scratch_code(req: RunRequest) -> Result<RunResponse, String> {
    CodeRunner::execute(req).await.map_err(|e| e.to_string())
}

// -------------------------------------------------------------------------------------------------
// Master Dispatcher (Used by Tauri commands and Linux App IPC shim)
// -------------------------------------------------------------------------------------------------

pub async fn dispatch(
    cmd: &str,
    state: &Arc<AppState>,
    args: Value,
    sink: Option<&dyn EventSink>,
) -> Result<Value, String> {
    match cmd {
        "get_system_status" => {
            let res = get_system_status(state).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "get_expert_system_prompt" => {
            let res = get_expert_system_prompt().await?;
            Ok(Value::String(res))
        }
        "get_llm_config" => {
            let res = get_llm_config(state).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "set_llm_config" => {
            let config: LlmConfig = serde_json::from_value(args.get("config").cloned().unwrap_or(args))
                .map_err(|e| format!("Invalid LlmConfig: {}", e))?;
            let res = set_llm_config(state, config).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "set_api_key" => {
            let key = args["apiKey"].as_str().or_else(|| args["api_key"].as_str()).unwrap_or_default().to_string();
            let res = set_api_key(state, key).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "clear_api_key" => {
            let res = clear_api_key(state).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "test_llm_connection" => {
            let config: LlmConfig = serde_json::from_value(args.get("config").cloned().unwrap_or(args))
                .map_err(|e| format!("Invalid LlmConfig: {}", e))?;
            let res = test_llm_connection(state, config).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "cancel_llm_request" => {
            let req_id = args["requestId"].as_str().or_else(|| args["request_id"].as_str()).unwrap_or_default().to_string();
            let res = cancel_llm_request(state, req_id).await?;
            Ok(Value::Bool(res))
        }
        "preview_outbound" => {
            let config: LlmConfig = serde_json::from_value(args["config"].clone())
                .map_err(|e| format!("Invalid config: {}", e))?;
            let messages: Vec<ChatMessage> = serde_json::from_value(args["messages"].clone())
                .map_err(|e| format!("Invalid messages: {}", e))?;
            let res = preview_outbound(state, config, messages).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "send_llm_chat" => {
            let config: LlmConfig = serde_json::from_value(args["config"].clone())
                .map_err(|e| format!("Invalid config: {}", e))?;
            let messages: Vec<ChatMessage> = serde_json::from_value(args["messages"].clone())
                .map_err(|e| format!("Invalid messages: {}", e))?;
            let res = send_llm_chat(state, config, messages).await?;
            Ok(Value::String(res))
        }
        "open_project" => {
            let path = args["path"].as_str().unwrap_or_default().to_string();
            let res = open_project(state, path).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "list_project_dir" => {
            let rel = args["relDir"].as_str().or_else(|| args["rel_dir"].as_str()).map(|s| s.to_string());
            let res = list_project_dir(state, rel).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "get_project_tree" => {
            let res = get_project_tree(state).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "read_file" => {
            let path = args["path"].as_str().unwrap_or_default().to_string();
            let res = read_file(state, path).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "save_file" => {
            let path = args["path"].as_str().unwrap_or_default().to_string();
            let content = args["content"].as_str().unwrap_or_default().to_string();
            let res = save_file(state, path, content).await?;
            Ok(Value::String(res))
        }
        "index_project" => {
            let res = index_project(Arc::clone(state)).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "get_project_map" => {
            let res = get_project_map(state).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "set_reading_step_status" => {
            let step_id = args["stepId"].as_str().or_else(|| args["step_id"].as_str()).unwrap_or_default().to_string();
            let status = args["status"].as_str().unwrap_or_default().to_string();
            set_reading_step_status(state, step_id, status).await?;
            Ok(Value::Null)
        }
        "get_reading_progress" => {
            let res = get_reading_progress(state).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "get_symbol_neighborhood" => {
            let sym_id = args["symbolId"].as_str().or_else(|| args["symbol_id"].as_str()).unwrap_or_default().to_string();
            let res = get_symbol_neighborhood(state, sym_id).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "list_project_symbols" => {
            let fp = args["filePath"].as_str().or_else(|| args["file_path"].as_str()).map(|s| s.to_string());
            let res = list_project_symbols(state, fp).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "explain_code" => {
            let snippet = args["snippet"].as_str().unwrap_or_default().to_string();
            let fp = args["filePath"].as_str().or_else(|| args["file_path"].as_str()).unwrap_or_default().to_string();
            let (start_line, end_line) = if let Some(arr) = args["range"].as_array() {
                (arr.get(0).and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                 arr.get(1).and_then(|v| v.as_u64()).unwrap_or(0) as u32)
            } else {
                (args["startLine"].as_u64().or_else(|| args["start_line"].as_u64()).unwrap_or(0) as u32,
                 args["endLine"].as_u64().or_else(|| args["end_line"].as_u64()).unwrap_or(0) as u32)
            };
            let res = explain_code(state, snippet, fp, (start_line, end_line)).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "explain_selection_structured" => {
            let file = args["filePath"].as_str().or_else(|| args["file_path"].as_str()).or_else(|| args["file"].as_str()).unwrap_or_default().to_string();
            let snippet = args["snippet"].as_str().unwrap_or_default().to_string();
            let (start_line, end_line) = if let Some(arr) = args["range"].as_array() {
                (arr.get(0).and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                 arr.get(1).and_then(|v| v.as_u64()).unwrap_or(0) as u32)
            } else {
                (args["startLine"].as_u64().or_else(|| args["start_line"].as_u64()).unwrap_or(0) as u32,
                 args["endLine"].as_u64().or_else(|| args["end_line"].as_u64()).unwrap_or(0) as u32)
            };
            let force_refresh = args["forceRefresh"].as_bool().or_else(|| args["force_refresh"].as_bool()).unwrap_or(false);
            let depth_mode = args["depthMode"].as_str().or_else(|| args["depth_mode"].as_str()).map(|s| s.to_string());
            let res = explain_selection_structured(state, file, snippet, start_line, end_line, force_refresh, depth_mode).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "explain_layer" => {
            let layer = args["layer"].as_str().unwrap_or_default().to_string();
            let file = args["file"].as_str().unwrap_or_default().to_string();
            let snippet = args["snippet"].as_str().unwrap_or_default().to_string();
            let range: (u32, u32) = if let Some(arr) = args["range"].as_array() {
                (arr.get(0).and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                 arr.get(1).and_then(|v| v.as_u64()).unwrap_or(0) as u32)
            } else {
                (0, 0)
            };
            let depth_mode = args["depthMode"].as_str().or_else(|| args["depth_mode"].as_str()).map(|s| s.to_string());
            let force_refresh = args["forceRefresh"].as_bool().or_else(|| args["force_refresh"].as_bool());
            let res = explain_layer(state, layer, file, range, snippet, depth_mode, force_refresh, sink).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "mark_confusion" => {
            let fp = args["filePath"].as_str().or_else(|| args["file_path"].as_str()).unwrap_or_default().to_string();
            let snippet = args["snippet"].as_str().unwrap_or_default().to_string();
            let perspective = args["cardPerspective"].as_str().or_else(|| args["card_perspective"].as_str()).or_else(|| args["perspective"].as_str()).unwrap_or("整体").to_string();
            let note = args["note"].as_str().map(|s| s.to_string());
            let range: (u32, u32) = if let Some(arr) = args["range"].as_array() {
                (arr.get(0).and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                 arr.get(1).and_then(|v| v.as_u64()).unwrap_or(0) as u32)
            } else {
                (args["lineStart"].as_u64().or_else(|| args["line_start"].as_u64()).unwrap_or(0) as u32,
                 args["lineEnd"].as_u64().or_else(|| args["line_end"].as_u64()).unwrap_or(0) as u32)
            };
            let res = mark_confusion(state, fp, range.0, range.1, snippet, perspective, note).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "explain_confusion" => {
            let mark_id = args["markId"].as_str().or_else(|| args["mark_id"].as_str()).or_else(|| args["id"].as_str()).map(|s| s.to_string());
            let file = args["file"].as_str().or_else(|| args["filePath"].as_str()).map(|s| s.to_string());
            let snippet = args["snippet"].as_str().map(|s| s.to_string());
            let range: Option<(u32, u32)> = if let Some(arr) = args["range"].as_array() {
                Some((arr.get(0).and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                      arr.get(1).and_then(|v| v.as_u64()).unwrap_or(0) as u32))
            } else if args["lineStart"].is_number() || args["line_start"].is_number() {
                Some((args["lineStart"].as_u64().or_else(|| args["line_start"].as_u64()).unwrap_or(0) as u32,
                      args["lineEnd"].as_u64().or_else(|| args["line_end"].as_u64()).unwrap_or(0) as u32))
            } else {
                None
            };
            let confused_layer = args["confusedLayer"].as_str().or_else(|| args["confused_layer"].as_str()).or_else(|| args["category"].as_str()).map(|s| s.to_string());
            let user_query = args["userQuery"].as_str().or_else(|| args["user_query"].as_str()).or_else(|| args["userNote"].as_str()).map(|s| s.to_string());
            let level = args["level"].as_u64().map(|v| v as u8);
            let res = explain_confusion(state, mark_id, file, range, snippet, confused_layer, user_query, level, sink).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "update_confusion_mark_explanation" => {
            let id = args["id"].as_str().unwrap_or_default().to_string();
            let ai_exp = args["aiExplanation"].as_str().or_else(|| args["ai_explanation"].as_str()).or_else(|| args["simplifiedText"].as_str()).unwrap_or_default().to_string();
            let model = args["model"].as_str().unwrap_or("deepseek-chat").to_string();
            update_confusion_mark_explanation(state, id, ai_exp, model).await?;
            Ok(Value::Null)
        }
        "resolve_confusion_mark" => {
            let id = args["id"].as_str().unwrap_or_default().to_string();
            let level = args["level"].as_u64().map(|v| v as u8);
            let resolved = args["resolved"].as_bool().unwrap_or(true);
            resolve_confusion_mark(state, id, level, resolved).await?;
            Ok(Value::Null)
        }
        "get_confusion_marks" => {
            let fp = args["filePath"].as_str().or_else(|| args["file_path"].as_str()).map(|s| s.to_string());
            let res = get_confusion_marks(state, fp).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "get_learning_profile" => {
            let res = get_learning_profile(state).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "get_confusion_detail" => {
            let id = args["id"].as_str().unwrap_or_default().to_string();
            let res = get_confusion_detail(state, id).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "reopen_confusion_mark" => {
            let id = args["id"].as_str().or_else(|| args["markId"].as_str()).unwrap_or_default().to_string();
            let res = reopen_confusion_mark(state, id).await?;
            Ok(Value::Bool(res))
        }
        "delete_confusion_mark" => {
            let id = args["id"].as_str().or_else(|| args["markId"].as_str()).unwrap_or_default().to_string();
            let res = delete_confusion_mark(state, id).await?;
            Ok(Value::Bool(res))
        }
        "export_confusion_markdown" => {
            let res = export_confusion_markdown(state).await?;
            Ok(Value::String(res))
        }
        "submit_explanation_feedback" => {
            let fb: ExplanationFeedback = serde_json::from_value(args.clone())
                .or_else(|_| {
                    Ok::<ExplanationFeedback, serde_json::Error>(ExplanationFeedback {
                        id: format!("fb-{}", Utc::now().timestamp_millis()),
                        explanation_id: args["explanationId"].as_str().or_else(|| args["explanation_id"].as_str()).unwrap_or_default().to_string(),
                        card_perspective: args["cardPerspective"].as_str().or_else(|| args["card_perspective"].as_str()).unwrap_or_default().to_string(),
                        feedback: args["feedback"].as_str().unwrap_or_default().to_string(),
                        note: args["note"].as_str().map(|s| s.to_string()),
                        prompt_version: Some(PROMPT_VERSION.to_string()),
                        model: None,
                        created_at: Utc::now().timestamp(),
                    })
                })
                .map_err(|e| format!("Invalid ExplanationFeedback: {}", e))?;
            let res = submit_explanation_feedback(state, fb).await?;
            Ok(Value::Bool(res))
        }
        "get_feedback_stats" => {
            let pv = args["promptVersion"].as_str().or_else(|| args["prompt_version"].as_str()).map(|s| s.to_string());
            let m = args["model"].as_str().map(|s| s.to_string());
            let res = get_feedback_stats(state, pv, m).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "list_conversations" => {
            let res = list_conversations(state).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "create_conversation" => {
            let title = args["title"].as_str().unwrap_or("新对话").to_string();
            let res = create_conversation(state, title).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "load_conversation_messages" => {
            let cid = args["conversationId"].as_str().or_else(|| args["conversation_id"].as_str()).unwrap_or_default().to_string();
            let res = load_conversation_messages(state, cid).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "append_conversation_message" => {
            let cid = args["conversationId"].as_str().or_else(|| args["conversation_id"].as_str()).unwrap_or_default().to_string();
            let role = args["role"].as_str().unwrap_or("user").to_string();
            let content = args["content"].as_str().unwrap_or_default().to_string();
            let res = append_conversation_message(state, cid, role, content).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "delete_conversation" => {
            let id = args["id"].as_str().unwrap_or_default().to_string();
            let res = delete_conversation(state, id).await?;
            Ok(Value::Bool(res))
        }
        "preview_ask" => {
            let question = args["question"].as_str().unwrap_or_default().to_string();
            let file = args["file"].as_str().map(|s| s.to_string());
            let range: Option<(u32, u32)> = args["range"].as_array().map(|arr| {
                (arr.get(0).and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                 arr.get(1).and_then(|v| v.as_u64()).unwrap_or(0) as u32)
            });
            let snippet = args["selectedCode"].as_str().or_else(|| args["selected_code"].as_str()).map(|s| s.to_string());
            let res = preview_ask(state, question, file, range, snippet).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "ask_project_question" => {
            let question = args["question"].as_str().unwrap_or_default().to_string();
            let cid = args["conversationId"].as_str().or_else(|| args["conversation_id"].as_str()).map(|s| s.to_string());
            let file = args["file"].as_str().map(|s| s.to_string());
            let range: Option<(u32, u32)> = args["range"].as_array().map(|arr| {
                (arr.get(0).and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                 arr.get(1).and_then(|v| v.as_u64()).unwrap_or(0) as u32)
            });
            let snippet = args["selectedCode"].as_str().or_else(|| args["selected_code"].as_str()).map(|s| s.to_string());
            let use_tools = args["useTools"].as_bool().or_else(|| args["use_tools"].as_bool());
            let res = ask_project_question(state, cid, question, file, range, snippet, use_tools, sink).await?;
            Ok(Value::String(res))
        }
        "generate_project_exercises" => {
            let count = args["count"].as_u64().map(|v| v as usize);
            let fp = args["filePath"].as_str().or_else(|| args["file_path"].as_str()).map(|s| s.to_string());
            let snippet = args["snippet"].as_str().map(|s| s.to_string());
            let res = generate_project_exercises(state, count, fp, snippet).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "submit_exercise_answer" => {
            let eid = args["exerciseId"].as_str().or_else(|| args["exercise_id"].as_str()).unwrap_or_default().to_string();
            let ans = args["answer"].as_str().unwrap_or_default().to_string();
            let hesitation = args["hesitation"].as_str().map(|s| s.to_string());
            let res = submit_exercise_answer(state, eid, ans, hesitation).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "list_due_reviews" => {
            let res = list_due_reviews(state).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "detect_lsp_servers" => {
            let res = detect_lsp_servers(state).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "lsp_find_definition" => {
            let sym_name = args["symbolName"].as_str().or_else(|| args["symbol_name"].as_str()).or_else(|| args["symbolId"].as_str()).unwrap_or_default().to_string();
            let res = lsp_find_definition(state, sym_name).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "lsp_find_references" => {
            let sym_name = args["symbolName"].as_str().or_else(|| args["symbol_name"].as_str()).or_else(|| args["symbolId"].as_str()).unwrap_or_default().to_string();
            let res = lsp_find_references(state, sym_name).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "lsp_get_hover" => {
            let fp = args["filePath"].as_str().or_else(|| args["file_path"].as_str()).unwrap_or_default().to_string();
            let line = args["line"].as_u64().unwrap_or(0) as u32;
            let word = args["word"].as_str().or_else(|| args["symbolId"].as_str()).unwrap_or_default().to_string();
            let res = lsp_get_hover(state, fp, line, word).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "list_explanations" => {
            let fp = args["filePath"].as_str().or_else(|| args["file_path"].as_str()).map(|s| s.to_string());
            let query = args["query"].as_str().map(|s| s.to_string());
            let pinned = args["onlyPinned"].as_bool().or_else(|| args["only_pinned"].as_bool()).or_else(|| args["pinnedOnly"].as_bool());
            let limit = args["limit"].as_u64().map(|v| v as usize);
            let offset = args["offset"].as_u64().map(|v| v as usize);
            let res = list_explanations(state, fp, query, pinned, limit, offset).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "get_explanation" => {
            let id = args["id"].as_str().unwrap_or_default().to_string();
            let res = get_explanation(state, id).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "rename_explanation" => {
            let id = args["id"].as_str().unwrap_or_default().to_string();
            let title = args["newTitle"].as_str().or_else(|| args["new_title"].as_str()).unwrap_or_default().to_string();
            rename_explanation(state, id, title).await?;
            Ok(Value::Bool(true))
        }
        "pin_explanation" => {
            let id = args["id"].as_str().unwrap_or_default().to_string();
            let pinned = args["pinned"].as_bool().unwrap_or(false);
            pin_explanation(state, id, pinned).await?;
            Ok(Value::Bool(true))
        }
        "delete_explanation" => {
            let id = args["id"].as_str().unwrap_or_default().to_string();
            delete_explanation(state, id).await?;
            Ok(Value::Bool(true))
        }
        "list_prompt_presets" => {
            let scope = args["scope"].as_str().unwrap_or("explanation").to_string();
            let res = list_prompt_presets(state, scope).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "save_prompt_preset" => {
            let preset: PromptPreset = serde_json::from_value(args["preset"].clone())
                .map_err(|e| format!("Invalid preset: {}", e))?;
            save_prompt_preset(state, preset).await?;
            Ok(Value::Bool(true))
        }
        "set_active_prompt_preset" => {
            let id = args["id"].as_str().unwrap_or_default().to_string();
            set_active_prompt_preset(state, id).await?;
            Ok(Value::Bool(true))
        }
        "delete_prompt_preset" => {
            let id = args["id"].as_str().unwrap_or_default().to_string();
            delete_prompt_preset(state, id).await?;
            Ok(Value::Bool(true))
        }
        "reset_prompt_to_default" => {
            let scope = args["scope"].as_str().unwrap_or("explanation").to_string();
            reset_prompt_to_default(state, scope).await?;
            Ok(Value::Bool(true))
        }
        "preview_effective_prompt" => {
            let scope = args["scope"].as_str().unwrap_or("explanation").to_string();
            let res = preview_effective_prompt(state, scope).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "run_scratch_code" => {
            let req: RunRequest = serde_json::from_value(args.get("req").cloned().unwrap_or(args))
                .map_err(|e| format!("Invalid RunRequest: {}", e))?;
            let res = run_scratch_code(req).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        other => Err(format!("Unsupported command: {}", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_all_api_commands_covered() {
        let api_ts = include_str!("../../../../src/services/api.ts");
        let re = regex::Regex::new(r#"invoke(?:<[^>]+>)?\(['"]([a-zA-Z0-9_]+)['"]"#).unwrap();
        let mut invoked_commands = HashSet::new();
        for cap in re.captures_iter(api_ts) {
            invoked_commands.insert(cap[1].to_string());
        }

        let supported = supported_commands();
        println!("Invoked commands in api.ts ({}): {:?}", invoked_commands.len(), invoked_commands);

        for cmd in &invoked_commands {
            assert!(
                supported.contains(&cmd.as_str()),
                "Command '{}' found in api.ts is NOT in supported_commands()!",
                cmd
            );
        }
    }
}
