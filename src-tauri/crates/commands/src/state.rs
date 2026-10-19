use noesis_indexer::{CodeIndexer, ProjectMap};
use noesis_learn::knowledge::KnowledgeBase;
use noesis_llm::stream::CancellationRegistry;
use noesis_llm::{CredentialStore, LlmGateway};
use noesis_lsp::LspManager;
use noesis_project::ProjectManager;
use noesis_store::DbStore;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::Mutex;

pub static IS_INDEXING: AtomicBool = AtomicBool::new(false);
pub static ACTIVE_TASKS: AtomicUsize = AtomicUsize::new(0);

pub struct ActiveTaskGuard;

impl ActiveTaskGuard {
    pub fn new() -> Self {
        ACTIVE_TASKS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Self
    }
}

impl Drop for ActiveTaskGuard {
    fn drop(&mut self) {
        ACTIVE_TASKS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
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

impl AppState {
    pub fn new(app_data: std::path::PathBuf) -> Self {
        let db_path = app_data.join("noesis_local.db");
        let (store, db_degraded, db_error) = match DbStore::open(&db_path) {
            Ok(s) => (s, false, None),
            Err(err) => {
                let msg = format!("数据库打开失败 ({:?}): {}", db_path, err);
                let mem = DbStore::in_memory().expect("In-memory database initialization failed");
                (mem, true, Some(msg))
            }
        };

        Self {
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
        }
    }
}

