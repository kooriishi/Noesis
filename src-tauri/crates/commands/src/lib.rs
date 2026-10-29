pub mod agent_ask;
pub mod prompts;
pub mod settings;
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
pub use settings::{get_recent_projects, get_ui_settings, remember_project, set_ui_settings, UiSettings};
pub use state::{ActiveTaskGuard, AppState, IndexJob, IndexStatus, ACTIVE_TASKS, IS_INDEXING};
pub use types::*;

// Re-export prompt constants
pub use prompts::*;

/// H1: 追加在每一层系统提示词末尾的硬性输出约束，确保模型只产出一层的纯 Markdown。
pub const SINGLE_LAYER_OUTPUT_CONTRACT: &str = "本次只输出一个层的解读正文，使用纯 Markdown。\n- 不要输出 JSON，不要包 ```json 代码块（除提示词明确要求的逐行注释数组外）。\n- 不要输出其他三层的任何章节。\n- 不要写「好的，下面是……」这类开场白，也不要在结尾写「如需继续」之类的收尾。\n- 不要重复选区代码全文，只在需要对照时引用关键行。";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerExplainResponse {
    pub layer: String,
    pub markdown: String,
    pub quality: LayerQualityReport,
    pub citations: Vec<CitationItem>,
    pub concepts: Vec<String>,
    pub cached: bool,
    pub explanation_id: Option<String>,
    /// H1: 同一选区的四层共享的分组 id，历史卡片档案可整组打开。
    #[serde(default)]
    pub group_id: Option<String>,
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
        "get_index_status",
        "cancel_index",
        "get_project_map",
        "set_reading_step_status",
        "get_reading_progress",
        "get_symbol_neighborhood",
        "list_project_symbols",
        "explain_code",
        "explain_selection_structured",
        "explain_layer",
        "get_explanation_layers",
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
        "get_ui_settings",
        "set_ui_settings",
        "get_recent_projects",
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

/// H2: 明显过大的根目录（家目录 / 文件系统根 / 系统目录）先做拦截，
/// 避免用户误把整个 `$HOME` 当成项目索引。
fn is_oversized_root(path: &std::path::Path) -> bool {
    let Ok(canonical) = std::fs::canonicalize(path) else {
        return false;
    };
    if canonical.parent().is_none() {
        // 文件系统根 `/` 或 `C:\`
        return true;
    }
    if let Some(home) = dirs_home() {
        if canonical == home {
            return true;
        }
    }
    matches!(
        canonical.to_string_lossy().replace('\\', "/").as_str(),
        "/usr" | "/etc" | "/var" | "/opt" | "/lib" | "/bin" | "/sbin" | "/proc" | "/sys"
    )
}

fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .and_then(|p| std::fs::canonicalize(p).ok())
}

pub async fn open_project(state: &AppState, path: String) -> Result<FileNode, String> {
    let target = std::path::Path::new(&path);

    // H2-6: 拒绝把家目录 / 系统根目录当成项目
    if is_oversized_root(target) {
        return Err(format!(
            "「{}」是系统级大目录（家目录或根目录），不适合作为项目打开。请选择具体的代码仓库目录。",
            path
        ));
    }

    let pm = ProjectManager::new(target).map_err(|e| e.to_string())?;
    let tree = pm.build_tree(2).map_err(|e| e.to_string())?;
    let new_project_id = pm.normalized_root();

    // H2-2: 切换到新项目时，取消上一个项目的索引任务
    {
        let job_guard = state.index_job.lock().map_err(|e| e.to_string())?;
        if let Some(job) = job_guard.as_ref() {
            if job.project_id != new_project_id {
                job.request_cancel();
            }
        }
    }

    {
        let mut guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        *guard = Some(pm);
    }

    // 切换项目后，旧项目的地图不再代表当前工作区
    if let Ok(mut map_guard) = state.project_map.lock() {
        *map_guard = None;
    }

    // H2-1: 记住最近打开的项目，下次启动直接恢复（不再默认打开 `./`）
    if let Ok(canonical) = std::fs::canonicalize(target) {
        let _ = settings::remember_project(state, &canonical.to_string_lossy()).await;
    }

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

fn ensure_active_project(pm: &ProjectManager, requested_project_id: &str) -> Result<(), String> {
    let active_project_id = pm.normalized_root();
    if active_project_id != requested_project_id {
        return Err(format!(
            "项目已切换：请求属于 {}，当前项目是 {}。请重新打开文件。",
            requested_project_id, active_project_id
        ));
    }
    Ok(())
}

pub async fn read_file(state: &AppState, path: String, project_id: String) -> Result<FileContentPayload, String> {
    let mgr_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
    if let Some(pm) = mgr_guard.as_ref() {
        ensure_active_project(pm, &project_id)?;
        pm.read_file(&path).map_err(|e| e.to_string())
    } else {
        Err("尚未打开任何工程，请先在左侧选择工程目录。".to_string())
    }
}

pub async fn save_file(state: &AppState, path: String, content: String, project_id: String) -> Result<String, String> {
    let mgr_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
    if let Some(pm) = mgr_guard.as_ref() {
        ensure_active_project(pm, &project_id)?;
        pm.save_file(&path, &content).map_err(|e| e.to_string())
    } else {
        Err("尚未打开任何工程，无法保存文件。".to_string())
    }
}

// -------------------------------------------------------------------------------------------------
// Indexer Commands (H2: 按项目管理、可取消、带进度)
// -------------------------------------------------------------------------------------------------

/// H2-4: 查询当前索引进度，前端每秒轮询一次即可显示进度条。
pub async fn get_index_status(state: &AppState) -> Result<IndexStatus, String> {
    let job_guard = state.index_job.lock().map_err(|e| e.to_string())?;
    Ok(match job_guard.as_ref() {
        Some(job) => job.snapshot(),
        None => IndexStatus {
            running: false,
            project_id: None,
            processed: 0,
            total: 0,
            phase: "idle".to_string(),
        },
    })
}

/// H2-2: 主动取消正在进行的索引。
pub async fn cancel_index(state: &AppState) -> Result<bool, String> {
    let job_guard = state.index_job.lock().map_err(|e| e.to_string())?;
    if let Some(job) = job_guard.as_ref() {
        job.request_cancel();
        Ok(true)
    } else {
        Ok(false)
    }
}

pub async fn index_project(state: Arc<AppState>) -> Result<ProjectMap, String> {
    let (root_path, project_id) = {
        let mgr_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        let pm = mgr_guard.as_ref().ok_or("尚未打开任何工程")?;
        (pm.root_path().to_path_buf(), pm.normalized_root())
    };

    // H2-2: 同一项目重复点击不报错，直接返回「已在索引中」；不同项目则取消旧任务。
    let job = {
        let mut job_guard = state.index_job.lock().map_err(|e| e.to_string())?;
        if let Some(existing) = job_guard.as_ref() {
            if existing.project_id == project_id && !existing.is_cancelled() {
                return Err(format!(
                    "INDEX_ALREADY_RUNNING: 项目 {} 的索引正在进行中（已处理 {}/{} 个文件），请等待完成。",
                    existing.project_id,
                    existing.processed.load(Ordering::SeqCst),
                    existing.total.load(Ordering::SeqCst)
                ));
            }
            existing.request_cancel();
        }
        let new_job = IndexJob::new(project_id.clone());
        *job_guard = Some(new_job.clone());
        new_job
    };
    IS_INDEXING.store(true, Ordering::SeqCst);

    struct IndexingGuard(Arc<AppState>);
    impl Drop for IndexingGuard {
        fn drop(&mut self) {
            IS_INDEXING.store(false, Ordering::SeqCst);
            if let Ok(mut guard) = self.0.index_job.lock() {
                *guard = None;
            }
        }
    }
    let _guard = IndexingGuard(Arc::clone(&state));
    let _task = ActiveTaskGuard::new();

    let st = Arc::clone(&state);
    let job_for_task = job.clone();
    let project_id_for_task = project_id.clone();

    let res = tokio::task::spawn_blocking(move || {
        let _ = st.store.ensure_schema();
        let ros2_adapter = Ros2Adapter::new();
        let llmapp_adapter = LlmAppAdapter::new();
        let adapters: [&dyn DomainAdapter; 2] = [&ros2_adapter, &llmapp_adapter];

        job_for_task.set_phase("scanning");
        let processed = Arc::clone(&job_for_task.processed);
        let total = Arc::clone(&job_for_task.total);
        let progress_cb = move |done: usize, all: usize, _rel: &str| {
            processed.store(done, Ordering::SeqCst);
            total.store(all, Ordering::SeqCst);
        };

        let indexed = st.indexer.index_workspace_with_progress(
            &root_path,
            &st.store,
            &adapters,
            Some(job_for_task.cancel.as_ref()),
            Some(&progress_cb),
        );

        let (map, _report) = match indexed {
            Ok(v) => v,
            Err(noesis_indexer::IndexerError::Cancelled) => {
                return Err("INDEX_CANCELLED: 索引已取消".to_string());
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("SQLITE_BUSY")
                    || err_str.contains("database is locked")
                    || err_str.to_lowercase().contains("busy")
                {
                    return Err("数据库被另一个 Noesis 窗口占用，请关闭其他窗口后重试".to_string());
                }
                return Err(err_str);
            }
        };

        // H2-3: 只写自己项目的地图；项目已被切走或任务被取消时丢弃结果
        if job_for_task.is_cancelled() {
            return Err("INDEX_CANCELLED: 索引已取消".to_string());
        }
        let still_current = st
            .project_manager
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|pm| pm.normalized_root()))
            .map(|current| current == project_id_for_task)
            .unwrap_or(false);
        if !still_current {
            return Err("INDEX_CANCELLED: 项目已切换，本次索引结果已丢弃".to_string());
        }

        job_for_task.set_phase("finalizing");
        refresh_project_exercises_freshness(&root_path, &st.store, &project_id_for_task);
        *st.project_map.lock().unwrap() = Some(map.clone());
        Ok::<ProjectMap, String>(map)
    })
    .await
    .map_err(|e| format!("Indexing task join error: {}", e))??;

    Ok(res)
}

pub async fn get_project_map(state: &AppState) -> Result<Option<ProjectMap>, String> {
    let (project_id, project_root) = {
        let manager = state.project_manager.lock().map_err(|e| e.to_string())?;
        let Some(pm) = manager.as_ref() else {
            return Ok(None);
        };
        let project_id = pm.normalized_root();
        let mut map = state.project_map.lock().map_err(|e| e.to_string())?;
        if let Some(cached) = map.as_ref() {
            if cached.root == project_id {
                return Ok(Some(cached.clone()));
            }
        }
        *map = None;
        (project_id, pm.root_path().to_path_buf())
    };

    let files = state
        .store
        .list_project_files(&project_id)
        .map_err(|e| e.to_string())?;
    if files.is_empty() {
        return Ok(None);
    }

    // Restore detected domains from lightweight adapter detection and persisted entities.
    let entities = state
        .store
        .get_entities_by_domain(&project_id, None, None)
        .map_err(|e| e.to_string())?;
    let mut detected_domains = Vec::new();
    let ros2_adapter = Ros2Adapter::new();
    let llmapp_adapter = LlmAppAdapter::new();
    let adapters: [&dyn DomainAdapter; 2] = [&ros2_adapter, &llmapp_adapter];
    for adapter in adapters {
        if adapter.detect(&project_root) {
            let domain = adapter.domain_id().to_string();
            if !detected_domains.contains(&domain) {
                detected_domains.push(domain);
            }
        }
    }
    for entity in entities {
        if entity.domain != "generic" && !detected_domains.contains(&entity.domain) {
            detected_domains.push(entity.domain);
        }
    }

    let restored = state
        .indexer
        .build_project_map_from_store(&project_id, &detected_domains, files.len(), 0, &state.store)
        .map_err(|e| e.to_string())?;

    // Only cache a rebuilt map if this project is still active; otherwise a late
    // restore could replace the map for a project opened concurrently.
    let manager = state.project_manager.lock().map_err(|e| e.to_string())?;
    if !manager
        .as_ref()
        .map(|pm| pm.normalized_root() == project_id)
        .unwrap_or(false)
    {
        return Ok(None);
    }
    let mut map = state.project_map.lock().map_err(|e| e.to_string())?;
    if let Some(cached) = map.as_ref() {
        if cached.root == project_id {
            return Ok(Some(cached.clone()));
        }
    }
    *map = Some(restored.clone());
    Ok(Some(restored))
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
    let prompt_version = PROMPT_VERSION.to_string();

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

    // 旧版流程：显式拼接「四视角 JSON」格式规则（新流程 explain_layer 不再使用它）
    let sys_prompt = format!("{}

---
## 锁定格式规则（旧版 JSON 四视角）
{}", sys_prompt, JSON_V2_RULES_PROMPT);

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

    // 旧流程缓存：同一 basis_hash + model + prompt_version 直接复用，force_refresh 为真时跳过
    if !force_refresh {
        if let Ok(Some(hit)) = state.store.get_cached_explanation(
            &pack.basis_hash,
            &effective.model,
            &prompt_version,
        ) {
            if let Ok(mut exp) = serde_json::from_str::<StructuredExplanation>(&hit.json) {
                let verification = CitationVerifier::verify(&root_path, &[], &mut exp);
                let generic_phrases = QualityGate::load_generic_phrases(&root_path);
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
                    dropped: vec![],
                    cached: true,
                    raw_unverified_output: None,
                    quality: Some(quality),
                    explanation_id: Some(hit.id),
                });
            }
        }
    }

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

    // 旧流程落缓存，历史卡片档案仍可打开
    let cached_entry = noesis_store::CachedExplanation {
        id: exp_id.clone(),
        project_id: project_id.clone(),
        file_id: file_path.clone(),
        range: format!("{}-{}", start_line, end_line),
        basis_hash: pack.basis_hash.clone(),
        prompt_version: prompt_version.clone(),
        model: effective.model.clone(),
        request_kind: "explain_selection_structured".to_string(),
        json: serde_json::to_string(&final_exp).unwrap_or_default(),
        stats: serde_json::to_string(&quality).unwrap_or_default(),
        created_at: Utc::now().timestamp(),
        title: Some(format!(
            "四层解读 · {}:L{}-L{}",
            file_path.rsplit('/').next().unwrap_or(&file_path),
            start_line,
            end_line
        )),
        file_path: Some(file_path.clone()),
        selected_preview: Some(snippet.chars().take(240).collect()),
        pinned: false,
        deleted: false,
        depth_mode: depth_mode.clone(),
        last_opened_at: None,
        layer: None,
        markdown: None,
        citations_json: None,
        concepts_json: serde_json::to_string(&final_exp.concepts).ok(),
        quality_json: serde_json::to_string(&quality).ok(),
        group_id: None,
    };
    if let Err(err) = state.store.save_explanation_cache(&cached_entry) {
        tracing::warn!("explain_selection_structured 缓存写入失败: {}", err);
    }

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

/// 把 Markdown 正文里的 `[L12-L18]` / `[path/to/file.cpp:L40-L52]` 引用标记解析成待核验的
/// `CitationItem`，交给 `CitationVerifier` 统一核对（真实行号 / 路径逃逸 / 敏感文件）。
fn extract_markdown_citations(current_file: &str, markdown: &str) -> Vec<CitationItem> {
    let mut out: Vec<CitationItem> = Vec::new();
    let bytes: Vec<char> = markdown.chars().collect();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != '[' {
            i += 1;
            continue;
        }
        // 找到同一行内的闭合 ']'，避免跨段落误配
        let mut j = i + 1;
        let mut closed = None;
        while j < bytes.len() {
            let ch = bytes[j];
            if ch == '\n' || ch == ']' {
                if ch == ']' {
                    closed = Some(j);
                }
                break;
            }
            j += 1;
        }
        let Some(close) = closed else {
            i += 1;
            continue;
        };
        let inner: String = bytes[i + 1..close].iter().collect();
        let inner = inner.trim();

        // 形状一：L12-L18（当前文件）
        // 形状二：path/to/file.cpp:L40-L52（其他文件）
        let (file_part, line_part) = match inner.rfind(':') {
            Some(pos) if inner[pos + 1..].trim_start().starts_with('L') => {
                (inner[..pos].trim().to_string(), inner[pos + 1..].trim().to_string())
            }
            _ => (String::new(), inner.to_string()),
        };

        let line_part = line_part.trim_start_matches('L').trim_start_matches('l');
        let mut nums = line_part.splitn(2, '-');
        let first = nums.next().and_then(|s| s.trim().parse::<u32>().ok());
        let second = nums
            .next()
            .and_then(|s| s.trim().trim_start_matches('L').parse::<u32>().ok());

        if let Some(start) = first {
            let end = second.unwrap_or(start);
            let file = if file_part.is_empty() {
                current_file.to_string()
            } else {
                file_part
            };
            if !file.is_empty() && end >= start {
                // 同一文件同一行段只保留一次
                if !out
                    .iter()
                    .any(|c| c.file == file && c.line_start == start && c.line_end == end)
                {
                    out.push(CitationItem {
                        id: format!("c{}", out.len() + 1),
                        file,
                        line_start: start,
                        line_end: end,
                        quote: String::new(),
                        status: String::new(),
                        corrected_line_start: None,
                        corrected_line_end: None,
                        reason: None,
                    });
                }
            }
        }
        i = close + 1;
    }

    // 限制数量，避免超大输出把核验拖慢
    out.truncate(24);
    out
}

/// 引用核验：为每个引用从磁盘真实文件中回填原文片段，再交给 `CitationVerifier`。
fn verify_markdown_citations(
    root_path: &std::path::Path,
    current_file: &str,
    markdown: &str,
) -> Vec<CitationItem> {
    let mut cites = extract_markdown_citations(current_file, markdown);
    if cites.is_empty() {
        return cites;
    }

    // 回填 quote（Verifier 依赖 quote 判断行号是否被偏移过）
    let canonical_root =
        std::fs::canonicalize(root_path).unwrap_or_else(|_| root_path.to_path_buf());
    for cite in cites.iter_mut() {
        let rel = cite.file.trim_start_matches('/').to_string();
        let candidate = canonical_root.join(&rel);
        if let Ok(content) = std::fs::read_to_string(&candidate) {
            let lines: Vec<&str> = content.lines().collect();
            let start = cite.line_start.max(1) as usize;
            let end = (cite.line_end as usize).min(lines.len());
            if start <= lines.len() && end >= start {
                cite.quote = lines[start - 1..end].join("\n");
            }
        }
    }

    let mut exp = StructuredExplanation {
        citations: cites,
        ..Default::default()
    };
    CitationVerifier::verify(&canonical_root, &[], &mut exp);
    exp.citations
}

/// H1: 按层生成一份 Markdown 解读（流式）。
///
/// 与旧版 `explain_selection_structured` 的区别：
/// - 一次只负责一层，系统提示词不再拼接旧版「四视角 JSON」规则；
/// - 流式把 token 推给前端，结束后推送质检结果；
/// - 结果按 `basis_hash + layer + model + PROMPT_VERSION` 写入缓存，同一选区的四层共用 `group_id`。
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
    let layer_key = layer.to_lowercase();

    let layer_prompt_template = match layer_key.as_str() {
        "overall" => PROMPT_OVERALL,
        "logic" => PROMPT_LOGIC,
        "syntax" => PROMPT_SYNTAX,
        "principle" => PROMPT_PRINCIPLE,
        _ => return Err(format!("Unsupported layer: {}", layer)),
    };
    let layer_label = match layer_key.as_str() {
        "overall" => "整体",
        "logic" => "逻辑",
        "syntax" => "语法",
        _ => "原理",
    };

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

    // 注意：kind 里**不能**带层名。basis_hash 由 kind 参与计算，而同一选区的四层必须共享
    // 同一个 group_id（历史记录才能整组打开）。层名只参与缓存查询键，不参与分组键。
    let ctx_req = ContextRequest {
        kind: "explain_layer".to_string(),
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

    let max_tokens = match (layer_key.as_str(), is_deep) {
        ("overall", false) => 4096,
        ("overall", true) => 6144,
        ("logic", false) => 6144,
        ("logic", true) => 8192,
        ("syntax", false) => 8192,
        ("syntax", true) => 8192,
        ("principle", false) => 4096,
        ("principle", true) => 6144,
        _ => 6144,
    };

    // ---------------------------------------------------------------------------------------------
    // H1-3: 系统提示词只保留「安全与引用规则 + 人设 + 本层提示词 + 单层硬性要求」。
    // 不再拼接 code_guide_expert.system.md 与旧版 JSON 四视角规则，避免把模型拉回短输出 / JSON 格式。
    // ---------------------------------------------------------------------------------------------
    let system_prompt = format!(
        "{}\n\n---\n## 教学风格与规范\n{}\n\n---\n## 本次输出要求\n{}\n\n---\n{}",
        SAFETY_RULES_PROMPT, DEFAULT_PERSONA_PROMPT, SINGLE_LAYER_OUTPUT_CONTRACT, layer_prompt_template
    );

    let mut context_chunks = Vec::new();
    for item in &pack.items {
        if item.kind != "system_prompt" {
            context_chunks.push(item.text.clone());
        }
    }

    let user_prompt = format!(
        "目标文件: {}\n行号范围: L{}-L{}\n模式: {}\n本次只写【{}】层\n\n项目上下文信息:\n{}\n\n选区代码:\n```\n{}\n```",
        file,
        range.0,
        range.1,
        if is_deep { "深度解读 (Deep Mode)" } else { "标准解读 (Standard Mode)" },
        layer_label,
        context_chunks.join("\n\n"),
        snippet
    );

    let mut effective = effective.clone();
    effective.max_tokens = max_tokens;

    // ---------------------------------------------------------------------------------------------
    // H1-4: 缓存命中直接回放（force_refresh 为真时跳过）
    // ---------------------------------------------------------------------------------------------
    if !is_force {
        if let Ok(Some(hit)) = state.store.get_cached_layer(
            &pack.basis_hash,
            &layer_key,
            &effective.model,
            &prompt_version,
        ) {
            if let Some(markdown) = hit.markdown.clone() {
                let quality = hit
                    .quality_json
                    .as_deref()
                    .and_then(|j| serde_json::from_str::<LayerQualityReport>(j).ok())
                    .unwrap_or_else(|| {
                        QualityGate::evaluate_layer(
                            &layer_key,
                            &markdown,
                            &snippet,
                            is_deep,
                            &generic_phrases,
                        )
                    });
                let citations = hit
                    .citations_json
                    .as_deref()
                    .and_then(|j| serde_json::from_str::<Vec<CitationItem>>(j).ok())
                    .unwrap_or_default();
                let concepts = hit
                    .concepts_json
                    .as_deref()
                    .and_then(|j| serde_json::from_str::<Vec<String>>(j).ok())
                    .unwrap_or_default();

                if let Some(s) = sink {
                    let _ = s.send_event(json!({ "type": "token", "layer": layer_key, "token": markdown }));
                    let _ = s.send_event(json!({
                        "type": "done",
                        "layer": layer_key,
                        "markdown": markdown,
                        "quality": quality,
                        "citations": citations,
                        "concepts": concepts,
                        "cached": true,
                        "explanation_id": hit.id,
                        "group_id": hit.group_id,
                    }));
                }
                return Ok(LayerExplainResponse {
                    layer: layer_key,
                    markdown,
                    quality,
                    citations,
                    concepts,
                    cached: true,
                    explanation_id: Some(hit.id),
                    group_id: hit.group_id.clone(),
                });
            }
        }
    }

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
                    "layer": layer_key,
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
        &layer_key,
        &generated_markdown,
        &snippet,
        is_deep,
        &generic_phrases,
    );

    // 质检未通过：带问题清单重试一次（只重试一次，避免无限放大成本）
    if !quality.passed {
        let retry_issues = quality.issues.join("；");
        let retry_user_prompt = format!(
            "{}\n\n【质检重试指令】上一次输出未通过程序质检，具体问题如下：\n{}\n请严格针对上述问题重新生成完整的 Markdown 解读，不要输出 JSON，不要解释你在做什么。",
            user_prompt, retry_issues
        );
        let retry_messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.clone(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: retry_user_prompt,
            },
        ];

        if let Ok(retry_resp) = state.llm.send_chat(&effective, retry_messages).await {
            if !retry_resp.trim().is_empty() {
                generated_markdown = retry_resp;
                quality = QualityGate::evaluate_layer(
                    &layer_key,
                    &generated_markdown,
                    &snippet,
                    is_deep,
                    &generic_phrases,
                );
            }
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

    // ---------------------------------------------------------------------------------------------
    // H1-5: 引用解析 + 程序核验
    // ---------------------------------------------------------------------------------------------
    let citations = verify_markdown_citations(&root_path, &file, &generated_markdown);

    // ---------------------------------------------------------------------------------------------
    // H1-4: 落库缓存（同一选区的四层共享 group_id，历史卡片档案可整组打开）
    // ---------------------------------------------------------------------------------------------
    let explanation_id = format!(
        "exp-{}-{}",
        Utc::now().timestamp_millis(),
        layer_key
    );
    let group_id = format!("grp-{}", pack.basis_hash.chars().take(16).collect::<String>());
    let cached_entry = noesis_store::CachedExplanation {
        id: explanation_id.clone(),
        project_id: project_id.clone(),
        file_id: file.clone(),
        range: format!("{}-{}", range.0, range.1),
        basis_hash: pack.basis_hash.clone(),
        prompt_version: prompt_version.clone(),
        model: effective.model.clone(),
        request_kind: "explain_layer".to_string(),
        json: serde_json::json!({ "layer": layer_key, "markdown": generated_markdown }).to_string(),
        stats: serde_json::json!({
            "word_count": quality.word_count,
            "score": quality.score,
            "passed": quality.passed,
        })
        .to_string(),
        created_at: Utc::now().timestamp(),
        title: Some(format!(
            "{} · {}:L{}-L{}",
            layer_label,
            file.rsplit('/').next().unwrap_or(&file),
            range.0,
            range.1
        )),
        file_path: Some(file.clone()),
        selected_preview: Some(snippet.chars().take(240).collect()),
        pinned: false,
        deleted: false,
        depth_mode: Some(depth_str.clone()),
        last_opened_at: None,
        layer: Some(layer_key.clone()),
        markdown: Some(generated_markdown.clone()),
        citations_json: serde_json::to_string(&citations).ok(),
        concepts_json: serde_json::to_string(&concepts).ok(),
        quality_json: serde_json::to_string(&quality).ok(),
        group_id: Some(group_id.clone()),
    };
    if let Err(err) = state.store.save_explanation_cache(&cached_entry) {
        tracing::warn!("explain_layer 缓存写入失败: {}", err);
    }

    if let Some(s) = sink {
        let _ = s.send_event(json!({
            "type": "done",
            "layer": layer_key,
            "markdown": generated_markdown,
            "quality": quality,
            "citations": citations,
            "concepts": concepts,
            "cached": false,
            "explanation_id": explanation_id,
            "group_id": group_id,
        }));
    }

    Ok(LayerExplainResponse {
        layer: layer_key,
        markdown: generated_markdown,
        quality,
        citations,
        concepts,
        cached: false,
        explanation_id: Some(explanation_id),
        group_id: Some(group_id),
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
            content: format!("{}\n\n---\n{}", SAFETY_RULES_PROMPT, PROMPT_CONFUSION),
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

    // ---------------------------------------------------------------------------------------------
    // H5-d: use_tools 为真时走只读工具循环（读文件 / 搜索 / ROS2 拓扑），并把每一步推给前端。
    // 关闭时保持原来的单次流式问答。
    // ---------------------------------------------------------------------------------------------
    let wants_tools = use_tools.unwrap_or(true);

    let final_answer = if wants_tools {
        let active_domains = state
            .project_map
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|m| m.detected_domains.clone()))
            .unwrap_or_default();

        let agent_messages: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| json!({ "role": m.role, "content": m.content }))
            .collect();

        let handle = tokio::runtime::Handle::current();
        let root_for_loop = root_path.clone();
        let project_for_loop = project_id.clone();
        let store_for_loop = state.store.clone();
        let gateway_for_loop = state.llm.clone();
        let config_for_loop = effective_with_tokens.clone();

        // H5-d: 工具循环是同步的，用 block_in_place 在当前 worker 线程上跑，
        // 这样闭包可以安全借用 sink（不需要 'static）。
        let loop_result = tokio::task::block_in_place(move || {
            agent_ask::run_ask_agent_loop(
                &root_for_loop,
                &project_for_loop,
                Some(&store_for_loop),
                &active_domains,
                agent_messages,
                &gateway_for_loop,
                config_for_loop,
                handle,
                None,
                |step| {
                    if let Some(s) = sink {
                        let _ = s.send_event(json!({
                            "type": "tool_call",
                            "tool_name": step.tool_name,
                            "tool_args": step.args_summary,
                            "tool_result": step.result_summary,
                            "step": step.step,
                            "duration_ms": step.duration_ms,
                            "cached_duplicate": step.cached_duplicate
                        }));
                    }
                },
            )
        });

        match loop_result {
            Ok(res) => {
                if let Some(s) = sink {
                    let _ = s.send_event(json!({
                        "type": "agent_summary",
                        "steps": res.steps.len(),
                        "forced_termination_reason": res.forced_termination_reason
                    }));
                }
                if res.final_content.trim().is_empty() {
                    // 工具循环没有拿到可用回答时，回退到普通流式问答
                    if let Some(s) = sink {
                        state
                            .llm
                            .send_chat_stream(&effective_with_tokens, messages, |delta| {
                                let _ = s.send_event(json!({ "type": "token", "token": delta }));
                            })
                            .await
                            .map_err(|e| e.to_string())?
                    } else {
                        state
                            .llm
                            .send_chat(&effective_with_tokens, messages)
                            .await
                            .map_err(|e| e.to_string())?
                    }
                } else {
                    // 把最终答案按块推给前端，保持和流式一致的体验
                    if let Some(s) = sink {
                        for chunk in res.final_content.as_bytes().chunks(48) {
                            let piece = String::from_utf8_lossy(chunk).to_string();
                            let _ = s.send_event(json!({ "type": "token", "token": piece }));
                        }
                    }
                    res.final_content
                }
            }
            Err(err) => {
                if let Some(s) = sink {
                    let _ = s.send_event(json!({ "type": "error", "error": err.clone() }));
                }
                return Err(format!("智能体工具循环失败: {}", err));
            }
        }
    } else if let Some(s) = sink {
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

/// H1: 读取一条新版按层记录（或整个分组），供历史卡片档案打开新版解读。
pub async fn get_explanation_layers(
    state: &AppState,
    id: Option<String>,
    group_id: Option<String>,
) -> Result<Vec<LayerExplainResponse>, String> {
    let entries = if let Some(gid) = group_id {
        state.store.list_explanation_group(&gid).map_err(|e| e.to_string())?
    } else if let Some(explanation_id) = id {
        let item = state.store.get_explanation(&explanation_id).map_err(|e| e.to_string())?;
        match item {
            Some(entry) => match entry.group_id.clone() {
                Some(gid) => state
                    .store
                    .list_explanation_group(&gid)
                    .map_err(|e| e.to_string())?,
                None => vec![entry],
            },
            None => vec![],
        }
    } else {
        return Err("需要提供 id 或 group_id".to_string());
    };

    Ok(entries
        .into_iter()
        .filter_map(|entry| {
            let layer = entry.layer.clone()?;
            let markdown = entry.markdown.clone()?;
            let quality = entry
                .quality_json
                .as_deref()
                .and_then(|j| serde_json::from_str::<LayerQualityReport>(j).ok())
                .unwrap_or_default();
            let citations = entry
                .citations_json
                .as_deref()
                .and_then(|j| serde_json::from_str::<Vec<CitationItem>>(j).ok())
                .unwrap_or_default();
            let concepts = entry
                .concepts_json
                .as_deref()
                .and_then(|j| serde_json::from_str::<Vec<String>>(j).ok())
                .unwrap_or_default();
            Some(LayerExplainResponse {
                layer,
                markdown,
                quality,
                citations,
                concepts,
                cached: true,
                explanation_id: Some(entry.id),
                group_id: entry.group_id,
            })
        })
        .collect())
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
            let project_id = args["projectId"].as_str().or_else(|| args["project_id"].as_str()).unwrap_or_default().to_string();
            let res = read_file(state, path, project_id).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "save_file" => {
            let path = args["path"].as_str().unwrap_or_default().to_string();
            let content = args["content"].as_str().unwrap_or_default().to_string();
            let project_id = args["projectId"].as_str().or_else(|| args["project_id"].as_str()).unwrap_or_default().to_string();
            let res = save_file(state, path, content, project_id).await?;
            Ok(Value::String(res))
        }
        "index_project" => {
            let res = index_project(Arc::clone(state)).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "get_index_status" => {
            let res = get_index_status(state).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "cancel_index" => {
            let res = cancel_index(state).await?;
            Ok(Value::Bool(res))
        }
        "get_ui_settings" => {
            let res = get_ui_settings(state).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "set_ui_settings" => {
            let settings: UiSettings = serde_json::from_value(
                args.get("settings").cloned().unwrap_or_else(|| args.clone()),
            )
            .map_err(|e| format!("Invalid UiSettings: {}", e))?;
            let res = set_ui_settings(state, settings).await?;
            Ok(serde_json::to_value(res).unwrap())
        }
        "get_recent_projects" => {
            let res = get_recent_projects(state).await?;
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
            let perspective = args["category"].as_str().or_else(|| args["cardPerspective"].as_str()).or_else(|| args["card_perspective"].as_str()).or_else(|| args["perspective"].as_str()).unwrap_or("整体").to_string();
            let note = args["userNote"].as_str().or_else(|| args["user_note"].as_str()).or_else(|| args["note"].as_str()).map(|s| s.to_string());
            let range: (u32, u32) = if let Some(arr) = args["range"].as_array() {
                (arr.get(0).and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                 arr.get(1).and_then(|v| v.as_u64()).unwrap_or(0) as u32)
            } else {
                (args["startLine"].as_u64().or_else(|| args["start_line"].as_u64()).or_else(|| args["lineStart"].as_u64()).or_else(|| args["line_start"].as_u64()).unwrap_or(0) as u32,
                 args["endLine"].as_u64().or_else(|| args["end_line"].as_u64()).or_else(|| args["lineEnd"].as_u64()).or_else(|| args["line_end"].as_u64()).unwrap_or(0) as u32)
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
        "get_explanation_layers" => {
            let id = args["id"].as_str().map(|s| s.to_string());
            let group_id = args["groupId"].as_str().or_else(|| args["group_id"].as_str()).map(|s| s.to_string());
            let res = get_explanation_layers(state, id, group_id).await?;
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

    // ---------------------------------------------------------------------------------------------
    // H1: Markdown 引用解析
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn test_extract_markdown_citations_shapes() {
        let md = "看这里 [L12-L18]，还有 [src/node.cpp:L40-L52]，以及 [L30]。\n\
                  非法写法 [Lxx-Lyy] 与 [../escape.cpp:L1-L2] 不应产生有效引用。";
        let cites = extract_markdown_citations("src/main.cpp", md);

        assert!(
            cites.iter().any(|c| c.file == "src/main.cpp" && c.line_start == 12 && c.line_end == 18),
            "应当解析出当前文件的 L12-L18：{:?}",
            cites
        );
        assert!(
            cites.iter().any(|c| c.file == "src/node.cpp" && c.line_start == 40 && c.line_end == 52),
            "应当解析出跨文件引用：{:?}",
            cites
        );
        assert!(
            cites.iter().any(|c| c.file == "src/main.cpp" && c.line_start == 30 && c.line_end == 30),
            "单行引用 L30 应被解析成 30-30：{:?}",
            cites
        );
        assert!(
            !cites.iter().any(|c| c.line_start == 0 || c.line_end < c.line_start),
            "不应产生非法行号区间：{:?}",
            cites
        );
    }

    #[test]
    fn test_explain_layer_rejects_unknown_layer() {
        let state = AppState::new(std::env::temp_dir().join("noesis-test-unknown-layer"));
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt
            .block_on(explain_layer(
                &state,
                "nonsense".to_string(),
                "src/main.cpp".to_string(),
                (1, 2),
                "int main() {}".to_string(),
                None,
                None,
                None,
            ))
            .expect_err("未知层名必须报错");
        assert!(err.contains("Unsupported layer"), "实际错误: {}", err);
    }

    #[test]
    fn dispatch_mark_confusion_preserves_frontend_field_names() {
        let dir = std::env::temp_dir().join(format!("noesis-mark-dispatch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let state = Arc::new(AppState::new(dir.clone()));
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(dispatch(
            "mark_confusion",
            &state,
            serde_json::json!({
                "filePath": "src/main.rs",
                "startLine": 1,
                "endLine": 6,
                "snippet": "fn main() {}",
                "category": "logic",
                "userNote": "Explain the control flow"
            }),
            None,
        ))
        .expect("dispatch frontend mark payload");

        let marks = state.store.list_marks(None, None).expect("read saved marks");
        let mark = marks.first().expect("mark should be stored");
        assert_eq!(mark.category, "logic");
        assert_eq!(mark.line_start, 1);
        assert_eq!(mark.line_end, 6);
        assert_eq!(mark.user_note.as_deref(), Some("Explain the control flow"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---------------------------------------------------------------------------------------------
    // H2: 索引任务按项目管理、可取消、只写自己的地图
    // ---------------------------------------------------------------------------------------------

    fn write_sample_project(root: &std::path::Path, file_name: &str, symbol: &str) {
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src").join(file_name),
            format!(
                "#include <cstdio>\n\nvoid {symbol}() {{\n  printf(\"hello\\n\");\n}}\n\nint main() {{\n  {symbol}();\n  return 0;\n}}\n"
            ),
        )
        .unwrap();
        std::fs::write(
            root.join("CMakeLists.txt"),
            "cmake_minimum_required(VERSION 3.10)\nproject(demo)\n",
        )
        .unwrap();
    }

    #[test]
    fn project_map_is_restored_from_store_after_reopen() {
        let base = std::env::temp_dir().join(format!("noesis-map-restore-{}", std::process::id()));
        let project = base.join("project");
        let app_data = base.join("appdata");
        let _ = std::fs::remove_dir_all(&base);
        write_sample_project(&project, "map_node.cpp", "map_node");
        std::fs::create_dir_all(&app_data).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let project_id;

        {
            let state = Arc::new(AppState::new(app_data.clone()));
            let tree = rt
                .block_on(open_project(&state, project.to_string_lossy().to_string()))
                .expect("open project before indexing");
            project_id = tree.absolute_path;
            assert!(rt.block_on(get_project_map(&state)).unwrap().is_none());

            let generated = rt
                .block_on(index_project(Arc::clone(&state)))
                .expect("index project");
            assert!(generated.total_symbols > 0, "fixture should produce symbols");
        }

        let reopened = AppState::new(app_data);
        let tree = rt
            .block_on(open_project(&reopened, project.to_string_lossy().to_string()))
            .expect("reopen project after restarting state");
        assert_eq!(tree.absolute_path, project_id);
        assert!(reopened.project_map.lock().unwrap().is_none());

        let restored = rt
            .block_on(get_project_map(&reopened))
            .expect("load restored map")
            .expect("indexed map should be rebuilt from SQLite");
        assert_eq!(restored.root, project_id);
        assert!(restored.total_symbols > 0);
        assert!(restored.modules.iter().any(|m| m.path == "src/map_node.cpp"));
        assert_eq!(
            reopened.project_map.lock().unwrap().as_ref().map(|map| map.root.as_str()),
            Some(project_id.as_str())
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn file_commands_reject_stale_project_identity() {
        let base = std::env::temp_dir().join(format!("noesis-file-scope-{}", std::process::id()));
        let proj_a = base.join("project_a");
        let proj_b = base.join("project_b");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&proj_a).unwrap();
        std::fs::create_dir_all(&proj_b).unwrap();
        std::fs::write(proj_a.join("main.rs"), "fn project_a() {}\n").unwrap();
        std::fs::write(proj_b.join("main.rs"), "fn project_b() {}\n").unwrap();

        let state = Arc::new(AppState::new(base.join("appdata")));
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let tree_a = open_project(&state, proj_a.to_string_lossy().to_string())
                .await
                .expect("open A");
            let project_id_a = tree_a.absolute_path;
            let payload_a = read_file(&state, "main.rs".to_string(), project_id_a.clone())
                .await
                .expect("read A");
            assert!(payload_a.content.contains("project_a"));

            let tree_b = open_project(&state, proj_b.to_string_lossy().to_string())
                .await
                .expect("open B");
            let project_id_b = tree_b.absolute_path;
            let stale_read = read_file(&state, "main.rs".to_string(), project_id_a.clone())
                .await
                .expect_err("A's stale read must not be redirected to B");
            assert!(stale_read.contains("项目已切换"));
            let stale_save = save_file(
                &state,
                "main.rs".to_string(),
                "fn stale_content() {}\n".to_string(),
                project_id_a,
            )
            .await
            .expect_err("A's stale save must not overwrite B");
            assert!(stale_save.contains("项目已切换"));
            assert_eq!(std::fs::read_to_string(proj_b.join("main.rs")).unwrap(), "fn project_b() {}\n");

            save_file(
                &state,
                "main.rs".to_string(),
                "fn project_b_updated() {}\n".to_string(),
                project_id_b,
            )
            .await
            .expect("B's active save");
            assert_eq!(
                std::fs::read_to_string(proj_b.join("main.rs")).unwrap(),
                "fn project_b_updated() {}\n"
            );
        });
        let _ = std::fs::remove_dir_all(&base);
    }

    /// H2 验收：A 项目索引进行中时打开 B 项目并索引，B 成功，A 的结果不写进 project_map。
    #[test]
    fn test_h2_switch_project_during_index_discards_old_result() {
        let base = std::env::temp_dir().join(format!("noesis-h2-{}", std::process::id()));
        let proj_a = base.join("proj_a");
        let proj_b = base.join("proj_b");
        let _ = std::fs::remove_dir_all(&base);
        write_sample_project(&proj_a, "a_node.cpp", "alpha_node");
        write_sample_project(&proj_b, "b_node.cpp", "beta_node");

        let state = Arc::new(AppState::new(base.join("appdata")));
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            // 1. 打开 A 并登记一个「正在进行」的索引任务
            open_project(&state, proj_a.to_string_lossy().to_string())
                .await
                .expect("open A");
            let job_a = IndexJob::new({
                let guard = state.project_manager.lock().unwrap();
                guard.as_ref().unwrap().normalized_root()
            });
            *state.index_job.lock().unwrap() = Some(job_a.clone());

            // 2. 切到 B：open_project 必须取消 A 的任务
            open_project(&state, proj_b.to_string_lossy().to_string())
                .await
                .expect("open B");
            assert!(job_a.is_cancelled(), "切换到新项目时必须取消旧索引任务");

            // 3. 索引 B
            let map_b = index_project(Arc::clone(&state)).await.expect("index B");
            assert!(
                map_b.root.contains("proj_b"),
                "索引结果应属于 B 项目，实际: {}",
                map_b.root
            );

            // 4. project_map 里必须是 B 的地图，不能是 A 的
            let current = state.project_map.lock().unwrap().clone().expect("map exists");
            assert!(
                current.root.contains("proj_b"),
                "project_map 被错误地写成了其他项目: {}",
                current.root
            );
            assert!(
                current.modules.iter().any(|m| m.path.contains("b_node.cpp")),
                "B 项目的地图应包含 b_node.cpp"
            );
            assert!(
                !current.modules.iter().any(|m| m.path.contains("a_node.cpp")),
                "B 项目的地图不应包含 A 的文件"
            );
        });

        let _ = std::fs::remove_dir_all(&base);
    }

    /// H2 验收：同一项目连点两次不报错（返回「已在索引中」而不是全局锁错误）。
    #[test]
    fn test_h2_same_project_double_click_is_not_an_error() {
        let base = std::env::temp_dir().join(format!("noesis-h2-dup-{}", std::process::id()));
        let proj = base.join("proj");
        let _ = std::fs::remove_dir_all(&base);
        write_sample_project(&proj, "dup_node.cpp", "dup_node");

        let state = Arc::new(AppState::new(base.join("appdata")));
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            open_project(&state, proj.to_string_lossy().to_string())
                .await
                .expect("open");

            // 模拟「第一次点击仍在跑」
            let project_id = {
                let guard = state.project_manager.lock().unwrap();
                guard.as_ref().unwrap().normalized_root()
            };
            *state.index_job.lock().unwrap() = Some(IndexJob::new(project_id));

            let err = index_project(Arc::clone(&state))
                .await
                .expect_err("第二次点击应返回状态而不是崩溃");
            assert!(
                err.contains("INDEX_ALREADY_RUNNING"),
                "应返回「已在索引中」状态，实际: {}",
                err
            );
            assert!(
                !err.contains("索引正在进行中，请稍候"),
                "不应再出现全局锁错误文案"
            );
        });

        let _ = std::fs::remove_dir_all(&base);
    }

    /// H2 验收：索引被取消后返回 Cancelled，并且不写 project_map。
    #[test]
    fn test_h2_cancelled_index_does_not_write_map() {
        let base = std::env::temp_dir().join(format!("noesis-h2-cancel-{}", std::process::id()));
        let proj = base.join("proj");
        let _ = std::fs::remove_dir_all(&base);
        write_sample_project(&proj, "cancel_node.cpp", "cancel_node");

        // 1. 索引器层：预先取消的旗标必须让索引立刻返回 Cancelled
        let store = noesis_store::DbStore::in_memory().unwrap();
        let indexer = noesis_indexer::CodeIndexer::new();
        let cancel = std::sync::atomic::AtomicBool::new(true);
        let err = indexer
            .index_workspace_with_progress(&proj, &store, &[], Some(&cancel), None)
            .expect_err("预先取消的索引必须失败");
        assert!(
            matches!(err, noesis_indexer::IndexerError::Cancelled),
            "应返回 IndexerError::Cancelled，实际: {}",
            err
        );

        // 2. 命令层：取消中的任务不写 project_map
        let state = Arc::new(AppState::new(base.join("appdata")));
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            open_project(&state, proj.to_string_lossy().to_string())
                .await
                .expect("open");
            let project_id = {
                let guard = state.project_manager.lock().unwrap();
                guard.as_ref().unwrap().normalized_root()
            };
            let job = IndexJob::new(project_id);
            job.request_cancel();
            *state.index_job.lock().unwrap() = Some(job);

            // 已取消的旧任务会被新任务取代（用户主动重试），但旧任务的结果不应落库；
            // 这里只验证取消标记本身不会污染 project_map。
            assert!(
                state.project_map.lock().unwrap().is_none(),
                "取消前后 project_map 都应保持为空"
            );
        });

        let _ = std::fs::remove_dir_all(&base);
    }

    /// H2 验收：家目录 / 根目录不应被当成项目打开。
    #[test]
    fn test_h2_rejects_home_directory() {
        let home = dirs_home().expect("HOME/USERPROFILE 应存在");
        let state = AppState::new(std::env::temp_dir().join("noesis-h2-home"));
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt
            .block_on(open_project(&state, home.to_string_lossy().to_string()))
            .expect_err("家目录必须被拒绝");
        assert!(err.contains("系统级大目录"), "实际错误: {}", err);
    }

    /// H2: get_index_status 在没有任务时返回 idle。
    #[test]
    fn test_h2_index_status_idle() {
        let state = AppState::new(std::env::temp_dir().join("noesis-h2-status"));
        let rt = tokio::runtime::Runtime::new().unwrap();
        let st = rt.block_on(get_index_status(&state)).expect("status");
        assert!(!st.running);
        assert_eq!(st.processed, 0);
        assert_eq!(st.phase, "idle");
    }

    // ---------------------------------------------------------------------------------------------
    // H4: UI 设置
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn test_h4_ui_settings_roundtrip() {
        let dir = std::env::temp_dir().join(format!("noesis-h4-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let state = AppState::new(dir.clone());
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            remember_project(&state, "/tmp/noesis-recent-a").await.unwrap();
            remember_project(&state, "/tmp/noesis-recent-b").await.unwrap();
            let mut s = get_ui_settings(&state).await.expect("default settings");
            assert_eq!(s.theme, "catppuccin");
            s.last_project = None;
            s.recent_projects.clear();
            assert_eq!(s.tab_size, 4);

            s.theme = "dracula".to_string();
            s.ui_font_size = 15;
            s.font_family = "\"Fira Code\", monospace".to_string();
            set_ui_settings(&state, s).await.expect("save settings");

            let again = get_ui_settings(&state).await.expect("reload settings");
            assert_eq!(again.theme, "dracula");
            assert_eq!(again.last_project.as_deref(), Some("/tmp/noesis-recent-b"));
            assert_eq!(
                again.recent_projects,
                vec!["/tmp/noesis-recent-b".to_string(), "/tmp/noesis-recent-a".to_string()]
            );
            assert_eq!(again.ui_font_size, 15);
            assert_eq!(again.font_family, "\"Fira Code\", monospace");
        });

        let _ = std::fs::remove_dir_all(&dir);
    }

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
