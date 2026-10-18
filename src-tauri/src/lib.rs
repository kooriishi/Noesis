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
use tauri::{Manager, State};

const EXPERT_SYSTEM_PROMPT: &str = include_str!("../../prompts/code_guide_expert.system.md");

pub struct AppState {
    pub project_manager: Mutex<Option<ProjectManager>>,
    pub project_map: Mutex<Option<ProjectMap>>,
    pub store: DbStore,
    pub llm: LlmGateway,
    pub indexer: CodeIndexer,
}

#[tauri::command]
fn get_expert_system_prompt() -> Result<String, String> {
    Ok(EXPERT_SYSTEM_PROMPT.to_string())
}

#[tauri::command]
fn open_project(path: String, state: State<'_, AppState>) -> Result<FileNode, String> {
    let pm = ProjectManager::new(&path).map_err(|e| e.to_string())?;
    let tree = pm.build_tree(6).map_err(|e| e.to_string())?;
    let mut guard = state.project_manager.lock().map_err(|e| e.to_string())?;
    *guard = Some(pm);
    Ok(tree)
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
    let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
    let pm = pm_guard.as_ref().ok_or("No project opened yet")?;

    // Gather valid source code files for indexing (exclude plaintext/markdown/json from code symbols)
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
            let code_langs = [
                "rust", "typescript", "javascript", "python",
                "go", "java", "csharp", "kotlin", "swift", "c", "cpp", "php",
            ];
            if code_langs.contains(&lang.as_str()) {
                if let Ok(payload) = pm.read_file(&node.path) {
                    acc.push((node.path.clone(), payload.content, lang.clone()));
                }
            }
        }
    }

    collect_files(&tree, pm, &mut files_to_index);
    let map = state.indexer.compute_project_map(&files_to_index);
    let mut map_guard = state.project_map.lock().map_err(|e| e.to_string())?;
    *map_guard = Some(map.clone());

    Ok(map)
}

#[tauri::command]
fn get_project_map(state: State<'_, AppState>) -> Result<Option<ProjectMap>, String> {
    let map = state.project_map.lock().map_err(|e| e.to_string())?.clone();
    Ok(map)
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
    let verified = if let Some(pm) = pm_guard.as_ref() {
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
        verified,
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
    let rewording = LearnEngine::handle_confusion_mark(&category, &snippet);
    let project_id = {
        let pm_guard = state.project_manager.lock().map_err(|e| e.to_string())?;
        pm_guard.as_ref().map(|pm| pm.normalized_root())
    };

    let mark = ConfusionMark {
        id: format!("mark-{}-{}", chrono::Utc::now().timestamp_millis(), start_line),
        project_id,
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
    // 1. Verify outbound content limits and sanitize
    let _ = state
        .llm
        .filter()
        .inspect_outbound(&config, &messages)
        .map_err(|e| e.to_string())?;

    // 2. Validate that no message carries sensitive configuration files
    for m in &messages {
        for word in m.content.split_whitespace() {
            if word.contains(".env") || word.contains("id_rsa") || word.contains(".pem") {
                let _ = state.llm.filter().check_file_path_allowed(word).map_err(|e| e.to_string())?;
            }
        }
    }

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
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data = app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
            let _ = std::fs::create_dir_all(&app_data);
            let db_path = app_data.join("noesis_local.db");

            let store = DbStore::open(&db_path).unwrap_or_else(|err| {
                eprintln!("Warning: Failed to open AppData database ({:?}), falling back to memory: {}", db_path, err);
                DbStore::in_memory().expect("In-memory database initialization failed")
            });

            let state = AppState {
                project_manager: Mutex::new(None),
                project_map: Mutex::new(None),
                store,
                llm: LlmGateway::new(),
                indexer: CodeIndexer::new(),
            };

            app.manage(state);
            Ok(())
        })
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
            list_due_reviews,
            get_expert_system_prompt
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
