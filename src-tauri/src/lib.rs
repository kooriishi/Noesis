use noesis_indexer::{CodeIndexer, ProjectMap};
use noesis_learn::{
    ExplanationCard, LearnEngine, RewordingResult,
};
use noesis_llm::{ChatMessage, LlmConfig, LlmGateway, OutboundPreview};
use noesis_project::{FileContentPayload, FileNode, ProjectManager};
use noesis_runner::{CodeRunner, RunRequest, RunResponse};
use noesis_store::{ConfusionMark, DbStore, ExerciseRecord};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

pub struct AppState {
    pub project_manager: Mutex<Option<ProjectManager>>,
    pub project_map: Mutex<Option<ProjectMap>>,
    pub store: DbStore,
    pub llm: LlmGateway,
    pub indexer: CodeIndexer,
}

#[tauri::command]
fn open_project(path: String, state: State<'_, AppState>) -> Result<FileNode, String> {
    let pm = ProjectManager::new(&path).map_err(|e| e.to_string())?;
    let tree = pm.build_tree(6).map_err(|e| e.to_string())?;
    *state.project_manager.lock().unwrap() = Some(pm);
    Ok(tree)
}

#[tauri::command]
fn get_project_tree(state: State<'_, AppState>) -> Result<Option<FileNode>, String> {
    let pm_guard = state.project_manager.lock().unwrap();
    if let Some(pm) = pm_guard.as_ref() {
        let tree = pm.build_tree(6).map_err(|e| e.to_string())?;
        Ok(Some(tree))
    } else {
        Ok(None)
    }
}

#[tauri::command]
fn read_file(path: String, state: State<'_, AppState>) -> Result<FileContentPayload, String> {
    let pm_guard = state.project_manager.lock().unwrap();
    if let Some(pm) = pm_guard.as_ref() {
        pm.read_file(&path).map_err(|e| e.to_string())
    } else {
        // Fallback: direct read if path is absolute
        let direct_pm = ProjectManager::new(".").map_err(|e| e.to_string())?;
        direct_pm.read_file(&path).map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn save_file(path: String, content: String, state: State<'_, AppState>) -> Result<String, String> {
    let pm_guard = state.project_manager.lock().unwrap();
    if let Some(pm) = pm_guard.as_ref() {
        pm.save_file(&path, &content).map_err(|e| e.to_string())
    } else {
        let direct_pm = ProjectManager::new(".").map_err(|e| e.to_string())?;
        direct_pm.save_file(&path, &content).map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn index_project(state: State<'_, AppState>) -> Result<ProjectMap, String> {
    let pm_guard = state.project_manager.lock().unwrap();
    let pm = pm_guard.as_ref().ok_or("No project opened yet")?;

    // Gather text files for indexing
    let mut files_to_index = Vec::new();
    let tree = pm.build_tree(10).map_err(|e| e.to_string())?;

    fn collect_files(node: &FileNode, pm: &ProjectManager, acc: &mut Vec<(String, String, String)>) {
        if node.is_dir {
            if let Some(children) = &node.children {
                for c in children {
                    collect_files(c, pm, acc);
                }
            }
        } else if let Some(lang) = &node.language {
            if lang != "plaintext" {
                if let Ok(payload) = pm.read_file(&node.path) {
                    acc.push((node.path.clone(), payload.content, lang.clone()));
                }
            }
        }
    }

    collect_files(&tree, pm, &mut files_to_index);
    let map = state.indexer.compute_project_map(&files_to_index);
    *state.project_map.lock().unwrap() = Some(map.clone());

    Ok(map)
}

#[tauri::command]
fn get_project_map(state: State<'_, AppState>) -> Result<Option<ProjectMap>, String> {
    let map = state.project_map.lock().unwrap().clone();
    Ok(map)
}

#[tauri::command]
fn explain_code(
    file_path: String,
    snippet: String,
    start_line: u32,
    end_line: u32,
) -> Result<Vec<ExplanationCard>, String> {
    Ok(LearnEngine::generate_structured_cards(&file_path, &snippet, start_line, end_line))
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
    let rewording = LearnEngine::handle_confusion_mark(&category, &snippet);

    let mark = ConfusionMark {
        id: format!("mark-{}-{}", chrono::Utc::now().timestamp_millis(), start_line),
        file_path,
        line_start: start_line,
        line_end: end_line,
        code_snippet: snippet,
        category,
        explanation: Some(rewording.analogy.clone()),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    let _ = state.store.save_mark(&mark);
    Ok(rewording)
}

#[tauri::command]
fn get_confusion_marks(
    file_path: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<ConfusionMark>, String> {
    state
        .store
        .list_marks(file_path.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn preview_outbound(
    config: LlmConfig,
    messages: Vec<ChatMessage>,
    state: State<'_, AppState>,
) -> Result<OutboundPreview, String> {
    state
        .llm
        .filter()
        .inspect_outbound(&config, &messages)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn send_llm_chat(
    config: LlmConfig,
    messages: Vec<ChatMessage>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    state
        .llm
        .send_chat(&config, messages)
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
    let db_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("noesis_local.db")))
        .unwrap_or_else(|| PathBuf::from("noesis_local.db"));

    let store = DbStore::open(&db_path).unwrap_or_else(|_| DbStore::in_memory().unwrap());

    let state = AppState {
        project_manager: Mutex::new(None),
        project_map: Mutex::new(None),
        store,
        llm: LlmGateway::new(),
        indexer: CodeIndexer::new(),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            open_project,
            get_project_tree,
            read_file,
            save_file,
            index_project,
            get_project_map,
            explain_code,
            mark_confusion,
            get_confusion_marks,
            preview_outbound,
            send_llm_chat,
            run_scratch_code,
            list_due_reviews
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
