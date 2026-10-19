use noesis_indexer::{CodeIndexer, ProjectMap};
use noesis_learn::knowledge::KnowledgeBase;
use noesis_llm::stream::CancellationRegistry;
use noesis_llm::{CredentialStore, LlmGateway};
use noesis_lsp::LspManager;
use noesis_project::ProjectManager;
use noesis_store::DbStore;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub static ACTIVE_TASKS: AtomicUsize = AtomicUsize::new(0);

/// H2: 兼容旧代码的「是否有索引在跑」查询位。真实状态由 `AppState::index_job` 持有，
/// 这里只是给 Linux 版心跳退出逻辑用的快速判断，不再承担互斥职责。
pub static IS_INDEXING: AtomicBool = AtomicBool::new(false);

pub struct ActiveTaskGuard;

impl Default for ActiveTaskGuard {
    fn default() -> Self {
        Self::new()
    }
}

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

/// H2: 一次索引任务的可取消句柄。按项目维度持有，不再使用进程级全局锁。
#[derive(Debug, Clone)]
pub struct IndexJob {
    pub project_id: String,
    pub cancel: Arc<AtomicBool>,
    pub processed: Arc<AtomicUsize>,
    pub total: Arc<AtomicUsize>,
    pub phase: Arc<Mutex<String>>,
}

impl IndexJob {
    pub fn new(project_id: String) -> Self {
        Self {
            project_id,
            cancel: Arc::new(AtomicBool::new(false)),
            processed: Arc::new(AtomicUsize::new(0)),
            total: Arc::new(AtomicUsize::new(0)),
            phase: Arc::new(Mutex::new("preparing".to_string())),
        }
    }

    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub fn set_phase(&self, phase: &str) {
        if let Ok(mut g) = self.phase.lock() {
            *g = phase.to_string();
        }
    }

    pub fn snapshot(&self) -> IndexStatus {
        let phase = self
            .phase
            .lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| "unknown".to_string());
        IndexStatus {
            running: !self.is_cancelled(),
            project_id: Some(self.project_id.clone()),
            processed: self.processed.load(Ordering::SeqCst),
            total: self.total.load(Ordering::SeqCst),
            phase,
        }
    }
}

/// H2: `get_index_status` 的返回体。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IndexStatus {
    pub running: bool,
    pub project_id: Option<String>,
    pub processed: usize,
    pub total: usize,
    pub phase: String,
}

pub struct AppState {
    pub project_manager: Mutex<Option<ProjectManager>>,
    pub project_map: Mutex<Option<ProjectMap>>,
    /// H2: 当前正在跑的索引任务（同一时刻最多一个）。
    pub index_job: Mutex<Option<IndexJob>>,
    pub store: DbStore,
    pub db_degraded: bool,
    pub db_error: Option<String>,
    pub credentials: CredentialStore,
    pub llm: LlmGateway,
    pub indexer: CodeIndexer,
    pub knowledge_base: KnowledgeBase,
    pub lsp: LspManager,
    pub cancellations: CancellationRegistry,
    /// H4: 应用数据目录，用于持久化 UI 设置（settings.json）。
    pub app_data_dir: std::path::PathBuf,
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
            index_job: Mutex::new(None),
            store,
            db_degraded,
            db_error,
            credentials: CredentialStore::new(Some(app_data.clone())),
            llm: LlmGateway::new(),
            indexer: CodeIndexer::new(),
            knowledge_base: KnowledgeBase::load_builtin(),
            lsp: LspManager::new(),
            cancellations: CancellationRegistry::new(),
            app_data_dir: app_data,
        }
    }
}
