use noesis_commands::{
    AppState, EventSink, IndexStatus, LayerExplainResponse, UiSettings,
    SystemStatus, StructuredExplainResponse, SubmitExerciseResult, AskPreviewResponse,
    EffectivePromptPreview, ConfusionDetailResponse,
};
use noesis_indexer::ProjectMap;
use noesis_learn::RewordingResult;
use noesis_llm::{ChatMessage, ConnectionTestResult, LlmConfig, OutboundPreview};
use noesis_lsp::{DefinitionLocation, HoverInfo, LspServerStatus, ReferenceLocation};
use noesis_project::{FileContentPayload, FileNode};
use noesis_runner::RunResponse;
use noesis_store::{
    ConceptProfileItem, ConfusionMark, ConversationRecord, ExerciseRecord,
    ExplanationSummary, FeedbackStats, GraphSymbol, PromptPreset, ReadingProgressRecord,
    StoredMessageRecord, SymbolNeighborhood,
};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Manager, State};

pub struct TauriChannelSink(pub tauri::ipc::Channel<serde_json::Value>);

impl EventSink for TauriChannelSink {
    fn send_event(&self, event: serde_json::Value) -> Result<(), String> {
        self.0.send(event).map_err(|e| e.to_string())
    }
}

// -------------------------------------------------------------------------------------------------
// Thin Command Glue Layer (Delegates to noesis-commands)
// -------------------------------------------------------------------------------------------------

#[tauri::command]
async fn get_system_status(state: State<'_, Arc<AppState>>) -> Result<SystemStatus, String> {
    noesis_commands::get_system_status(&state).await
}

#[tauri::command]
async fn get_llm_config(state: State<'_, Arc<AppState>>) -> Result<LlmConfig, String> {
    noesis_commands::get_llm_config(&state).await
}

#[tauri::command]
async fn set_llm_config(config: LlmConfig, state: State<'_, Arc<AppState>>) -> Result<LlmConfig, String> {
    noesis_commands::set_llm_config(&state, config).await
}

#[tauri::command]
async fn set_api_key(api_key: String, state: State<'_, Arc<AppState>>) -> Result<LlmConfig, String> {
    noesis_commands::set_api_key(&state, api_key).await
}

#[tauri::command]
async fn clear_api_key(state: State<'_, Arc<AppState>>) -> Result<LlmConfig, String> {
    noesis_commands::clear_api_key(&state).await
}

#[tauri::command]
async fn test_llm_connection(config: LlmConfig, state: State<'_, Arc<AppState>>) -> Result<ConnectionTestResult, String> {
    noesis_commands::test_llm_connection(&state, config).await
}

#[tauri::command]
async fn cancel_llm_request(request_id: String, state: State<'_, Arc<AppState>>) -> Result<bool, String> {
    noesis_commands::cancel_llm_request(&state, request_id).await
}

#[tauri::command]
async fn open_project(path: String, state: State<'_, Arc<AppState>>) -> Result<FileNode, String> {
    noesis_commands::open_project(&state, path).await
}

#[tauri::command]
async fn list_project_dir(rel_dir: Option<String>, state: State<'_, Arc<AppState>>) -> Result<Vec<FileNode>, String> {
    noesis_commands::list_project_dir(&state, rel_dir).await
}

#[tauri::command]
async fn get_project_tree(state: State<'_, Arc<AppState>>) -> Result<Option<FileNode>, String> {
    noesis_commands::get_project_tree(&state).await
}

#[tauri::command]
async fn read_file(path: String, state: State<'_, Arc<AppState>>) -> Result<FileContentPayload, String> {
    noesis_commands::read_file(&state, path).await
}

#[tauri::command]
async fn save_file(path: String, content: String, state: State<'_, Arc<AppState>>) -> Result<String, String> {
    noesis_commands::save_file(&state, path, content).await
}

#[tauri::command]
async fn index_project(state: State<'_, Arc<AppState>>) -> Result<ProjectMap, String> {
    noesis_commands::index_project(state.inner().clone()).await
}

#[tauri::command]
async fn get_index_status(state: State<'_, Arc<AppState>>) -> Result<IndexStatus, String> {
    noesis_commands::get_index_status(&state).await
}

#[tauri::command]
async fn cancel_index(state: State<'_, Arc<AppState>>) -> Result<bool, String> {
    noesis_commands::cancel_index(&state).await
}

#[tauri::command]
async fn get_ui_settings(state: State<'_, Arc<AppState>>) -> Result<UiSettings, String> {
    noesis_commands::get_ui_settings(&state).await
}

#[tauri::command]
async fn set_ui_settings(settings: UiSettings, state: State<'_, Arc<AppState>>) -> Result<UiSettings, String> {
    noesis_commands::set_ui_settings(&state, settings).await
}

#[tauri::command]
async fn get_recent_projects(state: State<'_, Arc<AppState>>) -> Result<Vec<String>, String> {
    noesis_commands::get_recent_projects(&state).await
}

#[tauri::command]
async fn get_explanation_layers(
    id: Option<String>,
    group_id: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<LayerExplainResponse>, String> {
    noesis_commands::get_explanation_layers(&state, id, group_id).await
}

#[tauri::command]
async fn get_project_map(state: State<'_, Arc<AppState>>) -> Result<Option<ProjectMap>, String> {
    noesis_commands::get_project_map(&state).await
}

#[tauri::command]
async fn set_reading_step_status(step_id: String, status: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    noesis_commands::set_reading_step_status(&state, step_id, status).await
}

#[tauri::command]
async fn get_reading_progress(state: State<'_, Arc<AppState>>) -> Result<Vec<ReadingProgressRecord>, String> {
    noesis_commands::get_reading_progress(&state).await
}

#[tauri::command]
async fn get_symbol_neighborhood(
    symbol_id: Option<String>,
    symbol_name: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<SymbolNeighborhood>, String> {
    let id = symbol_id.or(symbol_name).ok_or_else(|| "缺少 symbolId".to_string())?;
    noesis_commands::get_symbol_neighborhood(&state, id).await
}

#[tauri::command]
async fn list_project_symbols(file_path: Option<String>, state: State<'_, Arc<AppState>>) -> Result<Vec<GraphSymbol>, String> {
    noesis_commands::list_project_symbols(&state, file_path).await
}

#[tauri::command]
async fn explain_code(
    file_path: String,
    snippet: String,
    start_line: u32,
    end_line: u32,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<noesis_learn::ExplanationCard>, String> {
    noesis_commands::explain_code(&state, snippet, file_path, (start_line, end_line)).await
}

#[tauri::command]
async fn explain_selection_structured(
    file_path: String,
    snippet: String,
    start_line: u32,
    end_line: u32,
    force_refresh: Option<bool>,
    depth_mode: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<StructuredExplainResponse, String> {
    noesis_commands::explain_selection_structured(
        &state,
        file_path,
        snippet,
        start_line,
        end_line,
        force_refresh.unwrap_or(false),
        depth_mode,
    ).await
}

#[tauri::command]
async fn explain_layer(
    layer: String,
    file: String,
    range: (u32, u32),
    snippet: String,
    depth_mode: Option<String>,
    force_refresh: Option<bool>,
    on_event: tauri::ipc::Channel<serde_json::Value>,
    state: State<'_, Arc<AppState>>,
) -> Result<LayerExplainResponse, String> {
    let sink = TauriChannelSink(on_event);
    let sink_ref: &dyn EventSink = &sink;
    noesis_commands::explain_layer(
        &state,
        layer,
        file,
        range,
        snippet,
        depth_mode,
        force_refresh,
        Some(sink_ref),
    ).await
}

#[tauri::command]
async fn mark_confusion(
    file_path: String,
    start_line: u32,
    end_line: u32,
    snippet: String,
    category: String,
    user_note: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<RewordingResult, String> {
    noesis_commands::mark_confusion(&state, file_path, start_line, end_line, snippet, category, user_note).await
}

#[tauri::command]
async fn update_confusion_mark_explanation(
    id: String,
    ai_explanation: String,
    model: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    noesis_commands::update_confusion_mark_explanation(&state, id, ai_explanation, model).await
}

#[tauri::command]
async fn resolve_confusion_mark(
    id: String,
    level: i32,
    resolved: bool,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    noesis_commands::resolve_confusion_mark(&state, id, Some(level as u8), resolved).await
}

#[tauri::command]
async fn get_confusion_marks(
    file_path: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<ConfusionMark>, String> {
    noesis_commands::get_confusion_marks(&state, file_path).await
}

#[tauri::command]
async fn get_learning_profile(state: State<'_, Arc<AppState>>) -> Result<Vec<ConceptProfileItem>, String> {
    noesis_commands::get_learning_profile(&state).await
}

#[tauri::command]
async fn list_conversations(state: State<'_, Arc<AppState>>) -> Result<Vec<ConversationRecord>, String> {
    noesis_commands::list_conversations(&state).await
}

#[tauri::command]
async fn create_conversation(title: Option<String>, state: State<'_, Arc<AppState>>) -> Result<ConversationRecord, String> {
    noesis_commands::create_conversation(&state, title.unwrap_or_else(|| "新会话".to_string())).await
}

#[tauri::command]
async fn load_conversation_messages(
    conversation_id: String,
    limit: Option<usize>,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<StoredMessageRecord>, String> {
    let _ = limit;
    noesis_commands::load_conversation_messages(&state, conversation_id).await
}

#[tauri::command]
async fn append_conversation_message(
    conversation_id: String,
    role: String,
    content: String,
    state: State<'_, Arc<AppState>>,
) -> Result<StoredMessageRecord, String> {
    noesis_commands::append_conversation_message(&state, conversation_id, role, content).await
}

#[tauri::command]
async fn delete_conversation(
    id: Option<String>,
    conversation_id: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let conv_id = id.or(conversation_id).ok_or_else(|| "缺少会话ID".to_string())?;
    let _ = noesis_commands::delete_conversation(&state, conv_id).await?;
    Ok(())
}

#[tauri::command]
async fn preview_outbound(
    config: LlmConfig,
    messages: Vec<ChatMessage>,
    state: State<'_, Arc<AppState>>,
) -> Result<OutboundPreview, String> {
    noesis_commands::preview_outbound(&state, config, messages).await
}

#[tauri::command]
async fn send_llm_chat(
    config: LlmConfig,
    messages: Vec<ChatMessage>,
    state: State<'_, Arc<AppState>>,
) -> Result<String, String> {
    noesis_commands::send_llm_chat(&state, config, messages).await
}

#[tauri::command]
async fn run_scratch_code(
    req: noesis_runner::RunRequest,
) -> Result<RunResponse, String> {
    noesis_commands::run_scratch_code(req).await
}

#[tauri::command]
async fn generate_project_exercises(
    count: Option<usize>,
    file_path: Option<String>,
    snippet: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<ExerciseRecord>, String> {
    noesis_commands::generate_project_exercises(&state, count, file_path, snippet).await
}

#[tauri::command]
async fn submit_exercise_answer(
    exercise_id: String,
    user_answer: String,
    quality: Option<serde_json::Value>,
    state: State<'_, Arc<AppState>>,
) -> Result<SubmitExerciseResult, String> {
    let hesitation = quality.and_then(|v| {
        if let Some(s) = v.as_str() {
            Some(s.to_string())
        } else if let Some(n) = v.as_i64() {
            Some(match n {
                5 => "easy".to_string(),
                3 => "hesitant".to_string(),
                _ => "normal".to_string(),
            })
        } else {
            None
        }
    });
    noesis_commands::submit_exercise_answer(&state, exercise_id, user_answer, hesitation).await
}

#[tauri::command]
async fn list_due_reviews(state: State<'_, Arc<AppState>>) -> Result<Vec<ExerciseRecord>, String> {
    noesis_commands::list_due_reviews(&state).await
}

#[tauri::command]
async fn detect_lsp_servers(state: State<'_, Arc<AppState>>) -> Result<Vec<LspServerStatus>, String> {
    noesis_commands::detect_lsp_servers(&state).await
}

#[tauri::command]
async fn lsp_find_definition(
    symbol_name: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<DefinitionLocation>, String> {
    noesis_commands::lsp_find_definition(&state, symbol_name).await
}

#[tauri::command]
async fn lsp_find_references(
    symbol_name: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<ReferenceLocation>, String> {
    noesis_commands::lsp_find_references(&state, symbol_name).await
}

#[tauri::command]
async fn lsp_get_hover(
    file_path: String,
    line: u32,
    word: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<HoverInfo>, String> {
    noesis_commands::lsp_get_hover(&state, file_path, line, word).await
}

#[tauri::command]
async fn get_expert_system_prompt() -> Result<String, String> {
    noesis_commands::get_expert_system_prompt().await
}

#[tauri::command]
async fn list_explanations(
    pinned_only: Option<bool>,
    limit: Option<usize>,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<ExplanationSummary>, String> {
    noesis_commands::list_explanations(&state, None, None, pinned_only, limit, None).await
}

#[tauri::command]
async fn get_explanation(id: String, state: State<'_, Arc<AppState>>) -> Result<Option<StructuredExplainResponse>, String> {
    noesis_commands::get_explanation(&state, id).await
}

#[tauri::command]
async fn rename_explanation(id: String, new_title: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    noesis_commands::rename_explanation(&state, id, new_title).await
}

#[tauri::command]
async fn pin_explanation(id: String, pinned: bool, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    noesis_commands::pin_explanation(&state, id, pinned).await
}

#[tauri::command]
async fn delete_explanation(id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    noesis_commands::delete_explanation(&state, id).await
}

#[tauri::command]
async fn preview_ask(
    question: String,
    file: Option<String>,
    range: Option<(u32, u32)>,
    selected_code: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<AskPreviewResponse, String> {
    noesis_commands::preview_ask(&state, question, file, range, selected_code).await
}

#[tauri::command]
async fn ask_project_question(
    conversation_id: Option<String>,
    question: String,
    file: Option<String>,
    range: Option<(u32, u32)>,
    selected_code: Option<String>,
    use_tools: Option<bool>,
    on_event: tauri::ipc::Channel<serde_json::Value>,
    state: State<'_, Arc<AppState>>,
) -> Result<String, String> {
    let sink = TauriChannelSink(on_event);
    let sink_ref: &dyn EventSink = &sink;
    noesis_commands::ask_project_question(
        &state,
        conversation_id,
        question,
        file,
        range,
        selected_code,
        use_tools,
        Some(sink_ref),
    ).await
}

#[tauri::command]
async fn list_prompt_presets(
    scope: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<PromptPreset>, String> {
    state.store.list_prompt_presets(scope.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_prompt_preset(
    preset: PromptPreset,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    noesis_commands::save_prompt_preset(&state, preset).await
}

#[tauri::command]
async fn set_active_prompt_preset(
    id: String,
    scope: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state.store.set_active_prompt_preset(&id, &scope).map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_prompt_preset(
    id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    noesis_commands::delete_prompt_preset(&state, id).await
}

#[tauri::command]
async fn reset_prompt_to_default(
    scope: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    noesis_commands::reset_prompt_to_default(&state, scope).await
}

#[tauri::command]
async fn preview_effective_prompt(
    scope: String,
    state: State<'_, Arc<AppState>>,
) -> Result<EffectivePromptPreview, String> {
    noesis_commands::preview_effective_prompt(&state, scope).await
}

#[tauri::command]
async fn explain_confusion(
    mark_id: Option<String>,
    file: Option<String>,
    range: Option<(u32, u32)>,
    snippet: Option<String>,
    confused_layer: Option<String>,
    user_note: Option<String>,
    level: Option<u8>,
    on_event: tauri::ipc::Channel<serde_json::Value>,
    state: State<'_, Arc<AppState>>,
) -> Result<ConfusionDetailResponse, String> {
    // H5-c: 通过 Channel 把每个 token 推给前端
    let sink = TauriChannelSink(on_event);
    let sink_ref: Option<&dyn EventSink> = Some(&sink);
    noesis_commands::explain_confusion(
        &state,
        mark_id,
        file,
        range,
        snippet,
        confused_layer,
        user_note,
        level,
        sink_ref,
    )
    .await
}

#[tauri::command]
async fn get_confusion_detail(
    mark_id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<ConfusionDetailResponse>, String> {
    noesis_commands::get_confusion_detail(&state, mark_id).await
}

#[tauri::command]
async fn reopen_confusion_mark(
    id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<bool, String> {
    noesis_commands::reopen_confusion_mark(&state, id).await
}

#[tauri::command]
async fn delete_confusion_mark(
    id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<bool, String> {
    noesis_commands::delete_confusion_mark(&state, id).await
}

#[tauri::command]
async fn export_confusion_markdown(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    noesis_commands::export_confusion_markdown(&state).await
}

#[tauri::command]
async fn submit_explanation_feedback(
    explanation_id: String,
    card_perspective: String,
    feedback: String,
    note: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<bool, String> {
    let fb = noesis_store::ExplanationFeedback {
        id: format!("fb-{}", chrono::Utc::now().timestamp_millis()),
        explanation_id,
        card_perspective,
        feedback,
        note,
        prompt_version: Some(noesis_learn::context::PROMPT_VERSION.to_string()),
        model: None,
        created_at: chrono::Utc::now().timestamp(),
    };
    noesis_commands::submit_explanation_feedback(&state, fb).await
}

#[tauri::command]
async fn get_feedback_stats(
    prompt_version: Option<String>,
    model: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<FeedbackStats, String> {
    noesis_commands::get_feedback_stats(&state, prompt_version, model).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            println!("检测到重复启动，已将窗口切换到前台。");
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            let _ = std::fs::create_dir_all(&app_data);

            let state = Arc::new(AppState::new(app_data));
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
            get_index_status,
            cancel_index,
            get_ui_settings,
            set_ui_settings,
            get_recent_projects,
            get_explanation_layers,
            get_project_map,
            set_reading_step_status,
            get_reading_progress,
            get_symbol_neighborhood,
            list_project_symbols,
            explain_code,
            explain_selection_structured,
            explain_layer,
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
    use noesis_indexer::{CodeIndexer, DomainAdapter};
    use noesis_learn::context::{ContextAssembler, ContextRequest, HeuristicTokenEstimator};
    use noesis_learn::exercises::{generate_cloze_exercise, generate_fact_exercises};
    use noesis_learn::knowledge::KnowledgeBase;
    use noesis_learn::verifier::{
        CitationItem, CitationVerifier, ExplanationBullet, ExplanationSection, StructuredExplanation,
    };
    use noesis_llmapp::LlmAppAdapter;
    use noesis_lsp::LspManager;
    use noesis_project::ProjectManager;
    use noesis_ros2::Ros2Adapter;
    use noesis_store::DbStore;

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
