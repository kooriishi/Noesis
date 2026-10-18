use noesis_indexer::{CodeIndexer, DomainAdapter, ProjectMap};
use noesis_learn::{ExplanationCard, LearnEngine, RewordingResult};
use noesis_llm::{
    ChatMessage, ConnectionTestResult, CredentialStore, LlmConfig, LlmGateway, OutboundPreview,
};
use noesis_llmapp::LlmAppAdapter;
use noesis_project::{FileContentPayload, FileNode, ProjectManager};
use noesis_ros2::Ros2Adapter;
use noesis_runner::{CodeRunner, RunRequest, RunResponse};
use noesis_store::{
    ConfusionMark, DbStore, ExerciseRecord, GraphSymbol, ReadingProgressRecord, SymbolNeighborhood,
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

pub struct AppState {
    pub project_manager: Mutex<Option<ProjectManager>>,
    pub project_map: Mutex<Option<ProjectMap>>,
    pub store: DbStore,
    pub db_degraded: bool,
    pub db_error: Option<String>,
    pub credentials: CredentialStore,
    pub llm: LlmGateway,
    pub indexer: CodeIndexer,
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
    let root_path = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        let pm = pm_guard.as_ref().ok_or("No project opened yet")?;
        pm.root_path().to_path_buf()
    };

    let ros2_adapter = Ros2Adapter::new();
    let llm_adapter = LlmAppAdapter::new();
    let adapters: [&dyn DomainAdapter; 2] = [&ros2_adapter, &llm_adapter];

    let (map, _report) = state
        .indexer
        .index_workspace(&root_path, &state.store, &adapters)
        .map_err(|e| e.to_string())?;

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

    // Also update in-memory project_map reading_route status so subsequent get_project_map reflects it immediately
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

#[tauri::command]
fn mark_confusion(
    file_path: String,
    start_line: u32,
    end_line: u32,
    snippet: String,
    category: String,
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
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    let _ = state.store.save_mark(&mark);
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

#[tauri::command]
fn list_due_reviews(state: State<'_, AppState>) -> Result<Vec<ExerciseRecord>, String> {
    state.store.list_due_exercises().map_err(|e| e.to_string())
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
            mark_confusion,
            update_confusion_mark_explanation,
            get_confusion_marks,
            preview_outbound,
            send_llm_chat,
            run_scratch_code,
            list_due_reviews,
            get_expert_system_prompt
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
