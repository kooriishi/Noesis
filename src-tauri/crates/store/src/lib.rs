use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("Database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub path: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Unified Graph File Record (B1)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphFile {
    pub id: String,
    pub project_id: String,
    pub path: String,
    pub language: String,
    pub sha256: String,
    pub mtime: i64,
    pub size: i64,
    pub indexed_at: i64,
}

pub type FileRecord = GraphFile;

/// Unified Graph Symbol Record (B1)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSymbol {
    pub id: String,
    pub project_id: String,
    pub file_id: String,
    pub file_path: String,
    pub name: String,
    pub qualified_name: String,
    /// `function | method | class | struct | namespace | enum | variable | constant | macro`
    pub kind: String,
    pub line_start: u32,
    pub line_end: u32,
    pub col_start: u32,
    pub col_end: u32,
    pub signature: Option<String>,
    pub docstring: Option<String>,
    pub parent_id: Option<String>,
    pub metadata_json: String,
}

pub type SymbolRecord = GraphSymbol;

/// Unified Graph Module Record (B1)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphModule {
    pub id: String,
    pub project_id: String,
    pub name: String,
    /// `ros2_package | python_package | directory_module | llm_service`
    pub kind: String,
    pub root_path: String,
    pub entry_files_json: String,
    pub metadata_json: String,
}

/// Unified Graph Domain Entity Record (B1/B2/B4)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEntity {
    pub id: String,
    pub project_id: String,
    /// `ros2 | llm_app | generic`
    pub domain: String,
    /// e.g. `ros2_node | ros2_topic | ros2_service | ros2_action | ros2_param | ros2_timer | ros2_launch | ros2_interface_msg | ros2_interface_srv | ros2_interface_action | urdf_link | urdf_joint | llm_entrypoint | llm_client | llm_call | prompt_template | tool_def | agent_workflow | rag_pipeline | config_key`
    pub kind: String,
    pub name: String,
    pub file_id: String,
    pub file_path: String,
    pub symbol_id: Option<String>,
    pub line_start: u32,
    pub line_end: u32,
    pub metadata_json: String,
}

/// Unified Graph Edge Record (B1)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: String,
    pub project_id: String,
    /// `file | symbol | module | entity`
    pub src_type: String,
    pub src_id: String,
    /// `file | symbol | module | entity`
    pub dst_type: String,
    pub dst_id: String,
    /// `imports | calls | inherits | overrides | publishes | subscribes | serves | clients | action_serves | action_clients | declares_param | launches | includes_launch | calls_llm | uses_prompt | binds_tool | invokes_tool | routes_to | retrieves_from | reads_config`
    pub kind: String,
    pub file_id: String,
    pub file_path: String,
    pub line: u32,
    /// `exact | heuristic | dynamic`
    pub confidence: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolNeighborhood {
    pub symbol: GraphSymbol,
    pub parent: Option<GraphSymbol>,
    pub children: Vec<GraphSymbol>,
    pub incoming_edges: Vec<GraphEdge>,
    pub outgoing_edges: Vec<GraphEdge>,
    pub callers: Vec<GraphSymbol>,
    pub callees: Vec<GraphSymbol>,
    pub related_entities: Vec<GraphEntity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingProgressRecord {
    pub project_id: String,
    pub step_id: String,
    /// `unread | reading | done | skipped`
    pub status: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfusionMark {
    pub id: String,
    pub project_id: Option<String>,
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub code_snippet: String,
    pub category: String,
    #[serde(default)]
    pub confusion_type: Option<String>,
    pub explanation: Option<String>,
    #[serde(default)]
    pub ai_explanation: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default = "default_mark_level")]
    pub level: i32,
    #[serde(default)]
    pub resolved_at: Option<String>,
    #[serde(default)]
    pub resolved_level: Option<i32>,
    #[serde(default)]
    pub user_note: Option<String>,
    pub created_at: String,
}

fn default_mark_level() -> i32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedExplanation {
    pub id: String,
    pub project_id: String,
    pub file_id: String,
    pub range: String,
    pub basis_hash: String,
    pub prompt_version: String,
    pub model: String,
    pub request_kind: String,
    pub json: String,
    pub stats: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationRecord {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub summary: String,
    pub summary_upto_message_id: Option<String>,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessageRecord {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub meta_json: String,
    pub tokens: usize,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptProfileItem {
    pub id: String,
    pub name: String,
    pub pack_ref: Option<String>,
    pub domain: String,
    pub confused_count: usize,
    pub resolved_count: usize,
    pub reviewed_correct: usize,
    pub reviewed_wrong: usize,
    pub last_event_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExerciseRecord {
    pub id: String,
    pub unit_id: String,
    pub exercise_type: String,
    pub question: String,
    pub snippet: String,
    pub options_json: Option<String>,
    pub reference_answer: String,
    pub file_path: String,
    pub next_review_at: String,
    pub repetition: i32,
    pub interval_days: i32,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub source_kind: String,
    #[serde(default)]
    pub verified_by_program: bool,
    #[serde(default)]
    pub ease: f64,
    #[serde(default)]
    pub lapses: i32,
    #[serde(default)]
    pub stale: bool,
}

#[derive(Clone)]
pub struct DbStore {
    conn: Arc<Mutex<Connection>>,
}

impl DbStore {
    pub fn open<P: AsRef<Path>>(path: P) -> std::result::Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.init_schema()?;
        Ok(store)
    }

    pub fn in_memory() -> std::result::Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.init_schema()?;
        Ok(store)
    }

    pub fn conn(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.conn)
    }

    fn init_schema(&self) -> std::result::Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            PRAGMA foreign_keys = OFF;

            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                path TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS files (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                path TEXT NOT NULL,
                language TEXT NOT NULL,
                sha256 TEXT NOT NULL DEFAULT '',
                mtime INTEGER NOT NULL DEFAULT 0,
                size INTEGER NOT NULL DEFAULT 0,
                indexed_at INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS symbols (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL DEFAULT '',
                file_id TEXT NOT NULL DEFAULT '',
                file_path TEXT NOT NULL DEFAULT '',
                name TEXT NOT NULL,
                qualified_name TEXT NOT NULL DEFAULT '',
                kind TEXT NOT NULL,
                line_start INTEGER NOT NULL,
                line_end INTEGER NOT NULL,
                col_start INTEGER NOT NULL DEFAULT 0,
                col_end INTEGER NOT NULL DEFAULT 0,
                signature TEXT,
                docstring TEXT,
                parent_id TEXT,
                metadata_json TEXT NOT NULL DEFAULT '{}'
            );

            CREATE TABLE IF NOT EXISTS modules (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL DEFAULT 'directory_module',
                root_path TEXT NOT NULL DEFAULT '',
                entry_files_json TEXT NOT NULL DEFAULT '[]',
                metadata_json TEXT NOT NULL DEFAULT '{}'
            );

            CREATE TABLE IF NOT EXISTS entities (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                domain TEXT NOT NULL,
                kind TEXT NOT NULL,
                name TEXT NOT NULL,
                file_id TEXT NOT NULL DEFAULT '',
                file_path TEXT NOT NULL DEFAULT '',
                symbol_id TEXT,
                line_start INTEGER NOT NULL DEFAULT 0,
                line_end INTEGER NOT NULL DEFAULT 0,
                metadata_json TEXT NOT NULL DEFAULT '{}'
            );

            CREATE TABLE IF NOT EXISTS edges (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL DEFAULT '',
                src_type TEXT NOT NULL DEFAULT 'symbol',
                src_id TEXT NOT NULL,
                dst_type TEXT NOT NULL DEFAULT 'symbol',
                dst_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                file_id TEXT NOT NULL DEFAULT '',
                file_path TEXT NOT NULL DEFAULT '',
                line INTEGER NOT NULL DEFAULT 0,
                confidence TEXT NOT NULL DEFAULT 'exact',
                evidence TEXT NOT NULL DEFAULT ''
            );

            CREATE TABLE IF NOT EXISTS reading_progress (
                project_id TEXT NOT NULL,
                step_id TEXT NOT NULL,
                status TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (project_id, step_id)
            );

            CREATE TABLE IF NOT EXISTS summaries (
                id TEXT PRIMARY KEY,
                target_type TEXT NOT NULL,
                target_id TEXT NOT NULL,
                level TEXT NOT NULL,
                summary_text TEXT NOT NULL,
                basis_hash TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS cards (
                id TEXT PRIMARY KEY,
                unit_id TEXT NOT NULL,
                perspective TEXT NOT NULL,
                summary TEXT NOT NULL,
                detail TEXT NOT NULL,
                source_ref TEXT,
                file_path TEXT,
                line_start INTEGER,
                line_end INTEGER
            );

            CREATE TABLE IF NOT EXISTS marks (
                id TEXT PRIMARY KEY,
                project_id TEXT,
                file_path TEXT NOT NULL,
                line_start INTEGER NOT NULL,
                line_end INTEGER NOT NULL,
                code_snippet TEXT NOT NULL,
                category TEXT NOT NULL,
                confusion_type TEXT,
                explanation TEXT,
                ai_explanation TEXT,
                model TEXT,
                level INTEGER NOT NULL DEFAULT 1,
                resolved_at TEXT,
                resolved_level INTEGER,
                user_note TEXT,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS explanations (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                file_id TEXT NOT NULL,
                range TEXT NOT NULL,
                basis_hash TEXT NOT NULL,
                prompt_version TEXT NOT NULL,
                model TEXT NOT NULL,
                request_kind TEXT NOT NULL,
                json TEXT NOT NULL,
                stats TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                title TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                summary TEXT NOT NULL DEFAULT '',
                summary_upto_message_id TEXT,
                archived INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                meta_json TEXT NOT NULL DEFAULT '{}',
                tokens INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS concepts (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                pack_ref TEXT,
                domain TEXT NOT NULL DEFAULT 'general'
            );

            CREATE TABLE IF NOT EXISTS concept_events (
                id TEXT PRIMARY KEY,
                concept_id TEXT NOT NULL,
                project_id TEXT NOT NULL,
                mark_id TEXT,
                event TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS exercises (
                id TEXT PRIMARY KEY,
                unit_id TEXT NOT NULL,
                exercise_type TEXT NOT NULL,
                question TEXT NOT NULL,
                snippet TEXT NOT NULL,
                options_json TEXT,
                reference_answer TEXT NOT NULL,
                file_path TEXT NOT NULL,
                next_review_at TEXT NOT NULL,
                repetition INTEGER NOT NULL DEFAULT 0,
                interval_days INTEGER NOT NULL DEFAULT 1,
                project_id TEXT NOT NULL DEFAULT '',
                source_kind TEXT NOT NULL DEFAULT 'fact',
                verified_by_program INTEGER NOT NULL DEFAULT 1,
                ease REAL NOT NULL DEFAULT 2.5,
                lapses INTEGER NOT NULL DEFAULT 0,
                stale INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS attempts (
                id TEXT PRIMARY KEY,
                exercise_id TEXT NOT NULL,
                answer_json TEXT NOT NULL,
                result TEXT NOT NULL,
                score REAL NOT NULL,
                time_ms INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS notes (
                id TEXT PRIMARY KEY,
                note_type TEXT NOT NULL,
                content TEXT NOT NULL,
                target_ref TEXT,
                created_at TEXT NOT NULL
            );
            "#,
        )?;

        // Idempotent column migrations for existing databases
        let migrations = [
            "ALTER TABLE files ADD COLUMN sha256 TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE files ADD COLUMN mtime INTEGER NOT NULL DEFAULT 0;",
            "ALTER TABLE files ADD COLUMN indexed_at INTEGER NOT NULL DEFAULT 0;",
            "ALTER TABLE symbols ADD COLUMN project_id TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE symbols ADD COLUMN file_id TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE symbols ADD COLUMN qualified_name TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE symbols ADD COLUMN col_start INTEGER NOT NULL DEFAULT 0;",
            "ALTER TABLE symbols ADD COLUMN col_end INTEGER NOT NULL DEFAULT 0;",
            "ALTER TABLE symbols ADD COLUMN docstring TEXT;",
            "ALTER TABLE symbols ADD COLUMN parent_id TEXT;",
            "ALTER TABLE symbols ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}';",
            "ALTER TABLE modules ADD COLUMN kind TEXT NOT NULL DEFAULT 'directory_module';",
            "ALTER TABLE modules ADD COLUMN root_path TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE modules ADD COLUMN entry_files_json TEXT NOT NULL DEFAULT '[]';",
            "ALTER TABLE modules ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}';",
            "ALTER TABLE edges ADD COLUMN project_id TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE edges ADD COLUMN src_type TEXT NOT NULL DEFAULT 'symbol';",
            "ALTER TABLE edges ADD COLUMN src_id TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE edges ADD COLUMN dst_type TEXT NOT NULL DEFAULT 'symbol';",
            "ALTER TABLE edges ADD COLUMN dst_id TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE edges ADD COLUMN kind TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE edges ADD COLUMN file_id TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE edges ADD COLUMN confidence TEXT NOT NULL DEFAULT 'exact';",
            "ALTER TABLE edges ADD COLUMN evidence TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE marks ADD COLUMN project_id TEXT;",
            "ALTER TABLE marks ADD COLUMN confusion_type TEXT;",
            "ALTER TABLE marks ADD COLUMN ai_explanation TEXT;",
            "ALTER TABLE marks ADD COLUMN model TEXT;",
            "ALTER TABLE marks ADD COLUMN level INTEGER NOT NULL DEFAULT 1;",
            "ALTER TABLE marks ADD COLUMN resolved_at TEXT;",
            "ALTER TABLE marks ADD COLUMN resolved_level INTEGER;",
            "ALTER TABLE marks ADD COLUMN user_note TEXT;",
            "ALTER TABLE exercises ADD COLUMN project_id TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE exercises ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'fact';",
            "ALTER TABLE exercises ADD COLUMN verified_by_program INTEGER NOT NULL DEFAULT 1;",
            "ALTER TABLE exercises ADD COLUMN ease REAL NOT NULL DEFAULT 2.5;",
            "ALTER TABLE exercises ADD COLUMN lapses INTEGER NOT NULL DEFAULT 0;",
            "ALTER TABLE exercises ADD COLUMN stale INTEGER NOT NULL DEFAULT 0;",
        ];
        for sql in migrations {
            let _ = conn.execute(sql, []);
        }

        // Create unified graph & Stage C/D indexes
        let indexes = [
            "CREATE INDEX IF NOT EXISTS idx_files_proj_path ON files(project_id, path);",
            "CREATE INDEX IF NOT EXISTS idx_symbols_proj_file ON symbols(project_id, file_id);",
            "CREATE INDEX IF NOT EXISTS idx_symbols_proj_qname ON symbols(project_id, qualified_name);",
            "CREATE INDEX IF NOT EXISTS idx_symbols_proj_name ON symbols(project_id, name);",
            "CREATE INDEX IF NOT EXISTS idx_entities_proj_domain ON entities(project_id, domain, kind);",
            "CREATE INDEX IF NOT EXISTS idx_edges_proj_src ON edges(project_id, src_id);",
            "CREATE INDEX IF NOT EXISTS idx_edges_proj_dst ON edges(project_id, dst_id);",
            "CREATE INDEX IF NOT EXISTS idx_edges_proj_kind ON edges(project_id, kind);",
            "CREATE INDEX IF NOT EXISTS idx_explanations_basis ON explanations(basis_hash, model, prompt_version);",
            "CREATE INDEX IF NOT EXISTS idx_messages_conv ON messages(conversation_id, created_at);",
            "CREATE INDEX IF NOT EXISTS idx_concept_events_proj ON concept_events(project_id, concept_id);",
        ];
        for idx_sql in indexes {
            let _ = conn.execute(idx_sql, []);
        }

        Ok(())
    }

    /// Returns `(sha256, mtime)` if the file is already indexed in `files`.
    pub fn get_file_meta(
        &self,
        file_id: &str,
    ) -> std::result::Result<Option<(String, i64)>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let res = conn
            .query_row(
                "SELECT sha256, mtime FROM files WHERE id = ?1",
                params![file_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        Ok(res)
    }

    /// Transactionally replaces a single file's record and its associated symbols, entities, and edges (B1).
    pub fn upsert_file_graph(
        &self,
        file: &GraphFile,
        symbols: &[GraphSymbol],
        entities: &[GraphEntity],
        edges: &[GraphEdge],
    ) -> std::result::Result<(), StoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        // Delete prior data originating from this file
        tx.execute("DELETE FROM symbols WHERE file_id = ?1", params![file.id])?;
        tx.execute("DELETE FROM entities WHERE file_id = ?1", params![file.id])?;
        tx.execute("DELETE FROM edges WHERE file_id = ?1", params![file.id])?;

        tx.execute(
            r#"
            INSERT OR REPLACE INTO files (id, project_id, path, language, sha256, mtime, size, indexed_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                file.id,
                file.project_id,
                file.path,
                file.language,
                file.sha256,
                file.mtime,
                file.size,
                file.indexed_at
            ],
        )?;

        for sym in symbols {
            tx.execute(
                r#"
                INSERT OR REPLACE INTO symbols (
                    id, project_id, file_id, file_path, name, qualified_name, kind,
                    line_start, line_end, col_start, col_end, signature, docstring, parent_id, metadata_json
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
                "#,
                params![
                    sym.id,
                    sym.project_id,
                    sym.file_id,
                    sym.file_path,
                    sym.name,
                    sym.qualified_name,
                    sym.kind,
                    sym.line_start,
                    sym.line_end,
                    sym.col_start,
                    sym.col_end,
                    sym.signature,
                    sym.docstring,
                    sym.parent_id,
                    sym.metadata_json
                ],
            )?;
        }

        for ent in entities {
            tx.execute(
                r#"
                INSERT OR REPLACE INTO entities (
                    id, project_id, domain, kind, name, file_id, file_path, symbol_id, line_start, line_end, metadata_json
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                "#,
                params![
                    ent.id,
                    ent.project_id,
                    ent.domain,
                    ent.kind,
                    ent.name,
                    ent.file_id,
                    ent.file_path,
                    ent.symbol_id,
                    ent.line_start,
                    ent.line_end,
                    ent.metadata_json
                ],
            )?;
        }

        for edge in edges {
            tx.execute(
                r#"
                INSERT OR REPLACE INTO edges (
                    id, project_id, src_type, src_id, dst_type, dst_id, kind, file_id, file_path, line, confidence, evidence
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                "#,
                params![
                    edge.id,
                    edge.project_id,
                    edge.src_type,
                    edge.src_id,
                    edge.dst_type,
                    edge.dst_id,
                    edge.kind,
                    edge.file_id,
                    edge.file_path,
                    edge.line,
                    edge.confidence,
                    edge.evidence
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Removes files (and their symbols, entities, edges) that no longer exist on disk.
    pub fn delete_stale_files(
        &self,
        project_id: &str,
        active_file_ids: &HashSet<String>,
    ) -> std::result::Result<usize, StoreError> {
        let mut conn = self.conn.lock().unwrap();
        let existing_ids: Vec<String> = {
            let mut stmt = conn.prepare("SELECT id FROM files WHERE project_id = ?1")?;
            let rows = stmt.query_map(params![project_id], |r| r.get::<_, String>(0))?;
            rows.filter_map(|r| r.ok()).collect()
        };

        let stale: Vec<String> = existing_ids
            .into_iter()
            .filter(|id| !active_file_ids.contains(id))
            .collect();

        if stale.is_empty() {
            return Ok(0);
        }

        let tx = conn.transaction()?;
        for stale_id in &stale {
            tx.execute("DELETE FROM symbols WHERE file_id = ?1", params![stale_id])?;
            tx.execute("DELETE FROM entities WHERE file_id = ?1", params![stale_id])?;
            tx.execute("DELETE FROM edges WHERE file_id = ?1", params![stale_id])?;
            tx.execute("DELETE FROM files WHERE id = ?1", params![stale_id])?;
        }
        tx.commit()?;
        Ok(stale.len())
    }

    /// Batch upsert domain modules, entities, and edges for a project (used by DomainAdapters in B2/B4).
    pub fn upsert_domain_batch(
        &self,
        project_id: &str,
        domain: &str,
        modules: &[GraphModule],
        entities: &[GraphEntity],
        edges: &[GraphEdge],
    ) -> std::result::Result<(), StoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        // Clear old domain entities for this project + domain
        tx.execute(
            "DELETE FROM entities WHERE project_id = ?1 AND domain = ?2",
            params![project_id, domain],
        )?;

        for m in modules {
            tx.execute(
                r#"
                INSERT OR REPLACE INTO modules (id, project_id, name, kind, root_path, entry_files_json, metadata_json)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![
                    m.id,
                    m.project_id,
                    m.name,
                    m.kind,
                    m.root_path,
                    m.entry_files_json,
                    m.metadata_json
                ],
            )?;
        }

        for ent in entities {
            tx.execute(
                r#"
                INSERT OR REPLACE INTO entities (
                    id, project_id, domain, kind, name, file_id, file_path, symbol_id, line_start, line_end, metadata_json
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                "#,
                params![
                    ent.id,
                    ent.project_id,
                    ent.domain,
                    ent.kind,
                    ent.name,
                    ent.file_id,
                    ent.file_path,
                    ent.symbol_id,
                    ent.line_start,
                    ent.line_end,
                    ent.metadata_json
                ],
            )?;
        }

        for edge in edges {
            tx.execute(
                r#"
                INSERT OR REPLACE INTO edges (
                    id, project_id, src_type, src_id, dst_type, dst_id, kind, file_id, file_path, line, confidence, evidence
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                "#,
                params![
                    edge.id,
                    edge.project_id,
                    edge.src_type,
                    edge.src_id,
                    edge.dst_type,
                    edge.dst_id,
                    edge.kind,
                    edge.file_id,
                    edge.file_path,
                    edge.line,
                    edge.confidence,
                    edge.evidence
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn list_project_files(
        &self,
        project_id: &str,
    ) -> std::result::Result<Vec<GraphFile>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, project_id, path, language, sha256, mtime, size, indexed_at FROM files WHERE project_id = ?1 ORDER BY path ASC",
        )?;
        let rows = stmt.query_map(params![project_id], |r| {
            Ok(GraphFile {
                id: r.get(0)?,
                project_id: r.get(1)?,
                path: r.get(2)?,
                language: r.get(3)?,
                sha256: r.get(4)?,
                mtime: r.get(5)?,
                size: r.get(6)?,
                indexed_at: r.get(7)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn list_project_symbols(
        &self,
        project_id: &str,
        file_path: Option<&str>,
    ) -> std::result::Result<Vec<GraphSymbol>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let cols = "id, project_id, file_id, file_path, name, qualified_name, kind, line_start, line_end, col_start, col_end, signature, docstring, parent_id, metadata_json";
        let sql = if file_path.is_some() {
            format!(
                "SELECT {} FROM symbols WHERE project_id = ?1 AND file_path = ?2 ORDER BY line_start ASC",
                cols
            )
        } else {
            format!(
                "SELECT {} FROM symbols WHERE project_id = ?1 ORDER BY file_path ASC, line_start ASC",
                cols
            )
        };
        let mut stmt = conn.prepare(&sql)?;
        let mapper = |r: &rusqlite::Row| {
            Ok(GraphSymbol {
                id: r.get(0)?,
                project_id: r.get(1)?,
                file_id: r.get(2)?,
                file_path: r.get(3)?,
                name: r.get(4)?,
                qualified_name: r.get(5)?,
                kind: r.get(6)?,
                line_start: r.get(7)?,
                line_end: r.get(8)?,
                col_start: r.get(9)?,
                col_end: r.get(10)?,
                signature: r.get(11)?,
                docstring: r.get(12)?,
                parent_id: r.get(13)?,
                metadata_json: r.get(14)?,
            })
        };

        let mut out = Vec::new();
        if let Some(fp) = file_path {
            let rows = stmt.query_map(params![project_id, fp], mapper)?;
            for r in rows {
                out.push(r?);
            }
        } else {
            let rows = stmt.query_map(params![project_id], mapper)?;
            for r in rows {
                out.push(r?);
            }
        }
        Ok(out)
    }

    pub fn list_project_modules(
        &self,
        project_id: &str,
    ) -> std::result::Result<Vec<GraphModule>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, project_id, name, kind, root_path, entry_files_json, metadata_json FROM modules WHERE project_id = ?1 ORDER BY name ASC",
        )?;
        let rows = stmt.query_map(params![project_id], |r| {
            Ok(GraphModule {
                id: r.get(0)?,
                project_id: r.get(1)?,
                name: r.get(2)?,
                kind: r.get(3)?,
                root_path: r.get(4)?,
                entry_files_json: r.get(5)?,
                metadata_json: r.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn get_entities_by_domain(
        &self,
        project_id: &str,
        domain: Option<&str>,
        kind_filter: Option<&str>,
    ) -> std::result::Result<Vec<GraphEntity>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let cols = "id, project_id, domain, kind, name, file_id, file_path, symbol_id, line_start, line_end, metadata_json";
        let sql = match (domain, kind_filter) {
            (Some(_), Some(_)) => format!(
                "SELECT {} FROM entities WHERE project_id = ?1 AND domain = ?2 AND kind = ?3 ORDER BY file_path ASC, line_start ASC",
                cols
            ),
            (Some(_), None) => format!(
                "SELECT {} FROM entities WHERE project_id = ?1 AND domain = ?2 ORDER BY file_path ASC, line_start ASC",
                cols
            ),
            (None, Some(_)) => format!(
                "SELECT {} FROM entities WHERE project_id = ?1 AND kind = ?2 ORDER BY file_path ASC, line_start ASC",
                cols
            ),
            (None, None) => format!(
                "SELECT {} FROM entities WHERE project_id = ?1 ORDER BY file_path ASC, line_start ASC",
                cols
            ),
        };

        let mut stmt = conn.prepare(&sql)?;
        let mapper = |r: &rusqlite::Row| {
            Ok(GraphEntity {
                id: r.get(0)?,
                project_id: r.get(1)?,
                domain: r.get(2)?,
                kind: r.get(3)?,
                name: r.get(4)?,
                file_id: r.get(5)?,
                file_path: r.get(6)?,
                symbol_id: r.get(7)?,
                line_start: r.get(8)?,
                line_end: r.get(9)?,
                metadata_json: r.get(10)?,
            })
        };

        let mut out = Vec::new();
        let rows = match (domain, kind_filter) {
            (Some(d), Some(k)) => stmt.query_map(params![project_id, d, k], mapper)?,
            (Some(d), None) => stmt.query_map(params![project_id, d], mapper)?,
            (None, Some(k)) => stmt.query_map(params![project_id, k], mapper)?,
            (None, None) => stmt.query_map(params![project_id], mapper)?,
        };
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn get_edges_filtered(
        &self,
        project_id: &str,
        edge_kinds: Option<&[String]>,
    ) -> std::result::Result<Vec<GraphEdge>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, project_id, src_type, src_id, dst_type, dst_id, kind, file_id, file_path, line, confidence, evidence FROM edges WHERE project_id = ?1 ORDER BY file_path ASC, line ASC",
        )?;
        let rows = stmt.query_map(params![project_id], |r| {
            Ok(GraphEdge {
                id: r.get(0)?,
                project_id: r.get(1)?,
                src_type: r.get(2)?,
                src_id: r.get(3)?,
                dst_type: r.get(4)?,
                dst_id: r.get(5)?,
                kind: r.get(6)?,
                file_id: r.get(7)?,
                file_path: r.get(8)?,
                line: r.get(9)?,
                confidence: r.get(10)?,
                evidence: r.get(11)?,
            })
        })?;
        let filter_set: Option<HashSet<&str>> =
            edge_kinds.map(|ks| ks.iter().map(|s| s.as_str()).collect());
        let mut out = Vec::new();
        for r in rows {
            let e = r?;
            if let Some(ref set) = filter_set {
                if !set.contains(e.kind.as_str()) {
                    continue;
                }
            }
            out.push(e);
        }
        Ok(out)
    }

    /// Queries a symbol along with its 1-hop neighborhood (parent, children, callers, callees, domain entities) (B1).
    pub fn get_symbol_with_neighbors(
        &self,
        project_id: &str,
        symbol_id: &str,
    ) -> std::result::Result<Option<SymbolNeighborhood>, StoreError> {
        let all_syms = self.list_project_symbols(project_id, None)?;
        let target = match all_syms
            .iter()
            .find(|s| s.id == symbol_id || s.qualified_name == symbol_id || s.name == symbol_id)
        {
            Some(s) => s.clone(),
            None => return Ok(None),
        };

        let parent = target
            .parent_id
            .as_ref()
            .and_then(|pid| all_syms.iter().find(|s| &s.id == pid).cloned());

        let children: Vec<GraphSymbol> = all_syms
            .iter()
            .filter(|s| s.parent_id.as_deref() == Some(target.id.as_str()))
            .cloned()
            .collect();

        let all_edges = self.get_edges_filtered(project_id, None)?;
        let outgoing_edges: Vec<GraphEdge> = all_edges
            .iter()
            .filter(|e| e.src_id == target.id || e.src_id == target.qualified_name)
            .cloned()
            .collect();
        let incoming_edges: Vec<GraphEdge> = all_edges
            .iter()
            .filter(|e| {
                e.dst_id == target.id
                    || e.dst_id == target.qualified_name
                    || e.dst_id == target.name
            })
            .cloned()
            .collect();

        let caller_ids: HashSet<&str> = incoming_edges
            .iter()
            .filter(|e| e.kind == "calls")
            .map(|e| e.src_id.as_str())
            .collect();
        let callee_ids: HashSet<&str> = outgoing_edges
            .iter()
            .filter(|e| e.kind == "calls")
            .map(|e| e.dst_id.as_str())
            .collect();

        let callers: Vec<GraphSymbol> = all_syms
            .iter()
            .filter(|s| caller_ids.contains(s.id.as_str()) || caller_ids.contains(s.qualified_name.as_str()))
            .cloned()
            .collect();

        let callees: Vec<GraphSymbol> = all_syms
            .iter()
            .filter(|s| {
                callee_ids.contains(s.id.as_str())
                    || callee_ids.contains(s.qualified_name.as_str())
                    || callee_ids.contains(s.name.as_str())
            })
            .cloned()
            .collect();

        let all_entities = self.get_entities_by_domain(project_id, None, None)?;
        let related_entities: Vec<GraphEntity> = all_entities
            .into_iter()
            .filter(|ent| {
                ent.symbol_id.as_deref() == Some(target.id.as_str())
                    || (ent.file_path == target.file_path
                        && ent.line_start >= target.line_start
                        && ent.line_end <= target.line_end)
            })
            .collect();

        Ok(Some(SymbolNeighborhood {
            symbol: target,
            parent,
            children,
            incoming_edges,
            outgoing_edges,
            callers,
            callees,
            related_entities,
        }))
    }

    pub fn get_reading_progress(
        &self,
        project_id: &str,
    ) -> std::result::Result<Vec<ReadingProgressRecord>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT project_id, step_id, status, updated_at FROM reading_progress WHERE project_id = ?1",
        )?;
        let rows = stmt.query_map(params![project_id], |r| {
            Ok(ReadingProgressRecord {
                project_id: r.get(0)?,
                step_id: r.get(1)?,
                status: r.get(2)?,
                updated_at: r.get(3)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn set_reading_progress(
        &self,
        project_id: &str,
        step_id: &str,
        status: &str,
    ) -> std::result::Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        conn.execute(
            r#"
            INSERT OR REPLACE INTO reading_progress (project_id, step_id, status, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            "#,
            params![project_id, step_id, status, now],
        )?;
        Ok(())
    }

    pub fn save_mark(&self, mark: &ConfusionMark) -> std::result::Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let confusion_type = mark
            .confusion_type
            .clone()
            .unwrap_or_else(|| mark.category.clone());
        conn.execute(
            r#"
            INSERT OR REPLACE INTO marks (
                id, project_id, file_path, line_start, line_end,
                code_snippet, category, confusion_type, explanation,
                ai_explanation, model, level, resolved_at, resolved_level, user_note, created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            "#,
            params![
                mark.id,
                mark.project_id,
                mark.file_path,
                mark.line_start,
                mark.line_end,
                mark.code_snippet,
                mark.category,
                confusion_type,
                mark.explanation,
                mark.ai_explanation,
                mark.model,
                mark.level.max(1),
                mark.resolved_at,
                mark.resolved_level,
                mark.user_note,
                mark.created_at
            ],
        )?;
        Ok(())
    }

    pub fn update_mark_ai_explanation(
        &self,
        id: &str,
        ai_explanation: &str,
        model: &str,
    ) -> std::result::Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute(
            r#"
            UPDATE marks
            SET ai_explanation = ?1, model = ?2
            WHERE id = ?3
            "#,
            params![ai_explanation, model, id],
        )?;
        if rows == 0 {
            return Err(StoreError::NotFound(format!("Mark '{}' not found", id)));
        }
        Ok(())
    }

    pub fn update_mark_level_and_resolution(
        &self,
        id: &str,
        level: i32,
        resolved: bool,
    ) -> std::result::Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let resolved_at = if resolved {
            Some(Utc::now().to_rfc3339())
        } else {
            None
        };
        let resolved_level = if resolved { Some(level) } else { None };
        conn.execute(
            r#"
            UPDATE marks
            SET level = ?1, resolved_at = ?2, resolved_level = ?3
            WHERE id = ?4
            "#,
            params![level, resolved_at, resolved_level, id],
        )?;
        Ok(())
    }

    pub fn list_marks(
        &self,
        file_path: Option<&str>,
        project_id: Option<&str>,
    ) -> std::result::Result<Vec<ConfusionMark>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut marks = Vec::new();

        let select_cols = "id, project_id, file_path, line_start, line_end, code_snippet, category, confusion_type, explanation, ai_explanation, model, COALESCE(level, 1), resolved_at, resolved_level, user_note, created_at";

        let query = match (file_path, project_id) {
            (Some(_), Some(_)) => format!(
                "SELECT {} FROM marks WHERE file_path = ?1 AND project_id = ?2 ORDER BY created_at DESC",
                select_cols
            ),
            (Some(_), None) => format!(
                "SELECT {} FROM marks WHERE file_path = ?1 ORDER BY created_at DESC",
                select_cols
            ),
            (None, Some(_)) => format!(
                "SELECT {} FROM marks WHERE project_id = ?1 ORDER BY created_at DESC",
                select_cols
            ),
            (None, None) => format!("SELECT {} FROM marks ORDER BY created_at DESC", select_cols),
        };

        let mut stmt = conn.prepare(&query)?;
        let row_mapper = |row: &rusqlite::Row| {
            Ok(ConfusionMark {
                id: row.get(0)?,
                project_id: row.get(1)?,
                file_path: row.get(2)?,
                line_start: row.get(3)?,
                line_end: row.get(4)?,
                code_snippet: row.get(5)?,
                category: row.get(6)?,
                confusion_type: row.get(7)?,
                explanation: row.get(8)?,
                ai_explanation: row.get(9)?,
                model: row.get(10)?,
                level: row.get(11)?,
                resolved_at: row.get(12)?,
                resolved_level: row.get(13)?,
                user_note: row.get(14)?,
                created_at: row.get(15)?,
            })
        };

        let rows = match (file_path, project_id) {
            (Some(f), Some(p)) => stmt.query_map(params![f, p], row_mapper)?,
            (Some(f), None) => stmt.query_map(params![f], row_mapper)?,
            (None, Some(p)) => stmt.query_map(params![p], row_mapper)?,
            (None, None) => stmt.query_map([], row_mapper)?,
        };

        for r in rows {
            marks.push(r?);
        }
        Ok(marks)
    }

    /// Stage C1: Lookup cached structured explanation by `(basis_hash, model, prompt_version)`
    pub fn get_cached_explanation(
        &self,
        basis_hash: &str,
        model: &str,
        prompt_version: &str,
    ) -> std::result::Result<Option<CachedExplanation>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let res = conn
            .query_row(
                r#"
                SELECT id, project_id, file_id, range, basis_hash, prompt_version, model, request_kind, json, stats, created_at
                FROM explanations
                WHERE basis_hash = ?1 AND model = ?2 AND prompt_version = ?3
                ORDER BY created_at DESC
                LIMIT 1
                "#,
                params![basis_hash, model, prompt_version],
                |r| {
                    Ok(CachedExplanation {
                        id: r.get(0)?,
                        project_id: r.get(1)?,
                        file_id: r.get(2)?,
                        range: r.get(3)?,
                        basis_hash: r.get(4)?,
                        prompt_version: r.get(5)?,
                        model: r.get(6)?,
                        request_kind: r.get(7)?,
                        json: r.get(8)?,
                        stats: r.get(9)?,
                        created_at: r.get(10)?,
                    })
                },
            )
            .optional()?;
        Ok(res)
    }

    pub fn save_explanation_cache(
        &self,
        entry: &CachedExplanation,
    ) -> std::result::Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT OR REPLACE INTO explanations (
                id, project_id, file_id, range, basis_hash, prompt_version, model, request_kind, json, stats, created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                entry.id,
                entry.project_id,
                entry.file_id,
                entry.range,
                entry.basis_hash,
                entry.prompt_version,
                entry.model,
                entry.request_kind,
                entry.json,
                entry.stats,
                entry.created_at
            ],
        )?;
        Ok(())
    }

    /// Stage C3: Conversation & Message persistence
    pub fn create_conversation(
        &self,
        id: &str,
        project_id: &str,
        title: &str,
    ) -> std::result::Result<ConversationRecord, StoreError> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        conn.execute(
            r#"
            INSERT OR REPLACE INTO conversations (id, project_id, title, created_at, updated_at, summary, summary_upto_message_id, archived)
            VALUES (?1, ?2, ?3, ?4, ?5, '', NULL, 0)
            "#,
            params![id, project_id, title, now, now],
        )?;
        Ok(ConversationRecord {
            id: id.to_string(),
            project_id: project_id.to_string(),
            title: title.to_string(),
            created_at: now,
            updated_at: now,
            summary: String::new(),
            summary_upto_message_id: None,
            archived: false,
        })
    }

    pub fn list_conversations(
        &self,
        project_id: Option<&str>,
    ) -> std::result::Result<Vec<ConversationRecord>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let sql = if project_id.is_some() {
            "SELECT id, project_id, title, created_at, updated_at, summary, summary_upto_message_id, archived FROM conversations WHERE project_id = ?1 AND archived = 0 ORDER BY updated_at DESC"
        } else {
            "SELECT id, project_id, title, created_at, updated_at, summary, summary_upto_message_id, archived FROM conversations WHERE archived = 0 ORDER BY updated_at DESC"
        };
        let mut stmt = conn.prepare(sql)?;
        let mapper = |r: &rusqlite::Row| {
            Ok(ConversationRecord {
                id: r.get(0)?,
                project_id: r.get(1)?,
                title: r.get(2)?,
                created_at: r.get(3)?,
                updated_at: r.get(4)?,
                summary: r.get(5)?,
                summary_upto_message_id: r.get(6)?,
                archived: r.get::<_, i32>(7)? != 0,
            })
        };
        let mut out = Vec::new();
        if let Some(pid) = project_id {
            for row in stmt.query_map(params![pid], mapper)? {
                out.push(row?);
            }
        } else {
            for row in stmt.query_map([], mapper)? {
                out.push(row?);
            }
        }
        Ok(out)
    }

    pub fn delete_conversation(&self, id: &str) -> std::result::Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM messages WHERE conversation_id = ?1", params![id])?;
        conn.execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn update_conversation_summary(
        &self,
        conversation_id: &str,
        summary: &str,
        upto_msg_id: Option<&str>,
    ) -> std::result::Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        conn.execute(
            "UPDATE conversations SET summary = ?1, summary_upto_message_id = ?2, updated_at = ?3 WHERE id = ?4",
            params![summary, upto_msg_id, now, conversation_id],
        )?;
        Ok(())
    }

    pub fn append_message(
        &self,
        msg: &StoredMessageRecord,
    ) -> std::result::Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT OR REPLACE INTO messages (id, conversation_id, role, content, meta_json, tokens, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                msg.id,
                msg.conversation_id,
                msg.role,
                msg.content,
                msg.meta_json,
                msg.tokens as i64,
                msg.created_at
            ],
        )?;
        conn.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![msg.created_at, msg.conversation_id],
        )?;
        Ok(())
    }

    pub fn list_messages(
        &self,
        conversation_id: &str,
        limit: usize,
    ) -> std::result::Result<Vec<StoredMessageRecord>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, conversation_id, role, content, meta_json, tokens, created_at
            FROM messages
            WHERE conversation_id = ?1
            ORDER BY created_at ASC
            "#,
        )?;
        let rows = stmt.query_map(params![conversation_id], |r| {
            Ok(StoredMessageRecord {
                id: r.get(0)?,
                conversation_id: r.get(1)?,
                role: r.get(2)?,
                content: r.get(3)?,
                meta_json: r.get(4)?,
                tokens: r.get::<_, i64>(5)? as usize,
                created_at: r.get(6)?,
            })
        })?;
        let mut all = Vec::new();
        for r in rows {
            all.push(r?);
        }
        if all.len() > limit && limit > 0 {
            all = all.split_off(all.len() - limit);
        }
        Ok(all)
    }

    /// Stage C4: Concept & Concept Event tracking
    pub fn record_concept_event(
        &self,
        concept_name: &str,
        pack_ref: Option<&str>,
        domain: &str,
        project_id: &str,
        mark_id: Option<&str>,
        event: &str,
    ) -> std::result::Result<(), StoreError> {
        let normalized_id = concept_name
            .trim()
            .to_lowercase()
            .replace(' ', "_");
        if normalized_id.is_empty() {
            return Ok(());
        }
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT OR REPLACE INTO concepts (id, name, pack_ref, domain)
            VALUES (?1, ?2, ?3, ?4)
            "#,
            params![normalized_id, concept_name.trim(), pack_ref, domain],
        )?;
        let now = Utc::now().timestamp();
        let ev_id = format!("cev-{}-{}-{}", normalized_id, event, Utc::now().timestamp_nanos_opt().unwrap_or(0));
        conn.execute(
            r#"
            INSERT INTO concept_events (id, concept_id, project_id, mark_id, event, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![ev_id, normalized_id, project_id, mark_id, event, now],
        )?;
        Ok(())
    }

    pub fn list_concept_profile(
        &self,
        project_id: Option<&str>,
    ) -> std::result::Result<Vec<ConceptProfileItem>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, pack_ref, domain FROM concepts ORDER BY name ASC",
        )?;
        let concepts: Vec<(String, String, Option<String>, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .flatten()
            .collect();

        let mut items = Vec::new();
        for (cid, name, pack_ref, domain) in concepts {
            let mut ev_stmt = if project_id.is_some() {
                conn.prepare("SELECT event, created_at FROM concept_events WHERE concept_id = ?1 AND project_id = ?2")?
            } else {
                conn.prepare("SELECT event, created_at FROM concept_events WHERE concept_id = ?1")?
            };
            let rows: Vec<(String, i64)> = if let Some(pid) = project_id {
                ev_stmt
                    .query_map(params![cid, pid], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .flatten()
                    .collect()
            } else {
                ev_stmt
                    .query_map(params![cid], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .flatten()
                    .collect()
            };
            if rows.is_empty() && project_id.is_some() {
                continue;
            }
            let mut confused_count = 0;
            let mut resolved_count = 0;
            let mut reviewed_correct = 0;
            let mut reviewed_wrong = 0;
            let mut last_event_at = 0;
            for (ev, ts) in rows {
                if ts > last_event_at {
                    last_event_at = ts;
                }
                match ev.as_str() {
                    "confused" => confused_count += 1,
                    "resolved" => resolved_count += 1,
                    "reviewed_correct" => reviewed_correct += 1,
                    "reviewed_wrong" => reviewed_wrong += 1,
                    _ => {}
                }
            }
            items.push(ConceptProfileItem {
                id: cid,
                name,
                pack_ref,
                domain,
                confused_count,
                resolved_count,
                reviewed_correct,
                reviewed_wrong,
                last_event_at,
            });
        }
        items.sort_by(|a, b| b.confused_count.cmp(&a.confused_count));
        Ok(items)
    }

    pub fn insert_exercise(&self, ex: &ExerciseRecord) -> std::result::Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let ease = if ex.ease <= 0.0 { 2.5 } else { ex.ease };
        let source_kind = if ex.source_kind.is_empty() {
            "fact"
        } else {
            &ex.source_kind
        };
        conn.execute(
            r#"
            INSERT OR REPLACE INTO exercises (
                id, unit_id, exercise_type, question, snippet, options_json,
                reference_answer, file_path, next_review_at, repetition, interval_days,
                project_id, source_kind, verified_by_program, ease, lapses, stale
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
            "#,
            params![
                ex.id,
                ex.unit_id,
                ex.exercise_type,
                ex.question,
                ex.snippet,
                ex.options_json,
                ex.reference_answer,
                ex.file_path,
                ex.next_review_at,
                ex.repetition,
                ex.interval_days,
                ex.project_id,
                source_kind,
                if ex.verified_by_program { 1 } else { 0 },
                ease,
                ex.lapses,
                if ex.stale { 1 } else { 0 }
            ],
        )?;
        Ok(())
    }

    pub fn list_due_exercises(&self) -> std::result::Result<Vec<ExerciseRecord>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let now_str = Utc::now().to_rfc3339();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, unit_id, exercise_type, question, snippet, options_json, reference_answer,
                   file_path, next_review_at, repetition, interval_days,
                   COALESCE(project_id, ''), COALESCE(source_kind, 'fact'),
                   COALESCE(verified_by_program, 1), COALESCE(ease, 2.5),
                   COALESCE(lapses, 0), COALESCE(stale, 0)
            FROM exercises
            WHERE next_review_at <= ?1 AND COALESCE(stale, 0) = 0
            ORDER BY next_review_at ASC
            "#,
        )?;

        let rows = stmt.query_map(params![now_str], |row| {
            Ok(ExerciseRecord {
                id: row.get(0)?,
                unit_id: row.get(1)?,
                exercise_type: row.get(2)?,
                question: row.get(3)?,
                snippet: row.get(4)?,
                options_json: row.get(5)?,
                reference_answer: row.get(6)?,
                file_path: row.get(7)?,
                next_review_at: row.get(8)?,
                repetition: row.get(9)?,
                interval_days: row.get(10)?,
                project_id: row.get(11)?,
                source_kind: row.get(12)?,
                verified_by_program: row.get::<_, i32>(13)? != 0,
                ease: row.get(14)?,
                lapses: row.get(15)?,
                stale: row.get::<_, i32>(16)? != 0,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn mark_exercise_stale(
        &self,
        exercise_id: &str,
        stale: bool,
    ) -> std::result::Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE exercises SET stale = ?1 WHERE id = ?2",
            params![if stale { 1 } else { 0 }, exercise_id],
        )?;
        Ok(())
    }

    pub fn record_exercise_attempt(
        &self,
        exercise_id: &str,
        answer_json: &str,
        result: &str,
        score: f64,
    ) -> std::result::Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        let id = format!("att-{}-{}", exercise_id, Utc::now().timestamp_nanos_opt().unwrap_or(0));
        conn.execute(
            r#"
            INSERT INTO attempts (id, exercise_id, answer_json, result, score, time_ms, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)
            "#,
            params![id, exercise_id, answer_json, result, score, now],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_init_and_mark_with_ai_explanation() {
        let store = DbStore::in_memory().unwrap();
        let mark = ConfusionMark {
            id: "mark-1".to_string(),
            project_id: Some("proj-noesis".to_string()),
            file_path: "src/main.rs".to_string(),
            line_start: 10,
            line_end: 20,
            code_snippet: "fn complex() {}".to_string(),
            category: "logic".to_string(),
            confusion_type: Some("control_flow".to_string()),
            explanation: Some("analogy".to_string()),
            ai_explanation: None,
            model: None,
            level: 1,
            resolved_at: None,
            resolved_level: None,
            user_note: None,
            created_at: Utc::now().to_rfc3339(),
        };

        store.save_mark(&mark).unwrap();

        store
            .update_mark_ai_explanation(
                "mark-1",
                r#"{"analogy":"分拣机","minimal_example":"if ok {}"}"#,
                "deepseek-chat",
            )
            .unwrap();

        let marks = store.list_marks(None, None).unwrap();
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].id, "mark-1");
        assert_eq!(marks[0].project_id, Some("proj-noesis".to_string()));
        assert_eq!(marks[0].category, "logic");
        assert_eq!(marks[0].confusion_type, Some("control_flow".to_string()));
        assert_eq!(
            marks[0].ai_explanation,
            Some(r#"{"analogy":"分拣机","minimal_example":"if ok {}"}"#.to_string())
        );
        assert_eq!(marks[0].model, Some("deepseek-chat".to_string()));
    }

    #[test]
    fn test_unified_graph_upsert_and_neighbors() {
        let store = DbStore::in_memory().unwrap();
        let file = GraphFile {
            id: "proj:src/app.py".to_string(),
            project_id: "proj".to_string(),
            path: "src/app.py".to_string(),
            language: "python".to_string(),
            sha256: "abc1234".to_string(),
            mtime: 100,
            size: 512,
            indexed_at: 200,
        };
        let symbols = vec![
            GraphSymbol {
                id: "proj:src/app.py:Service".to_string(),
                project_id: "proj".to_string(),
                file_id: file.id.clone(),
                file_path: "src/app.py".to_string(),
                name: "Service".to_string(),
                qualified_name: "app.Service".to_string(),
                kind: "class".to_string(),
                line_start: 1,
                line_end: 20,
                col_start: 0,
                col_end: 0,
                signature: Some("class Service(Base):".to_string()),
                docstring: Some("Core service class".to_string()),
                parent_id: None,
                metadata_json: "{}".to_string(),
            },
            GraphSymbol {
                id: "proj:src/app.py:Service.run".to_string(),
                project_id: "proj".to_string(),
                file_id: file.id.clone(),
                file_path: "src/app.py".to_string(),
                name: "run".to_string(),
                qualified_name: "app.Service.run".to_string(),
                kind: "method".to_string(),
                line_start: 5,
                line_end: 15,
                col_start: 4,
                col_end: 0,
                signature: Some("def run(self) -> bool:".to_string()),
                docstring: None,
                parent_id: Some("proj:src/app.py:Service".to_string()),
                metadata_json: "{}".to_string(),
            },
            GraphSymbol {
                id: "proj:src/app.py:helper".to_string(),
                project_id: "proj".to_string(),
                file_id: file.id.clone(),
                file_path: "src/app.py".to_string(),
                name: "helper".to_string(),
                qualified_name: "app.helper".to_string(),
                kind: "function".to_string(),
                line_start: 22,
                line_end: 25,
                col_start: 0,
                col_end: 0,
                signature: Some("def helper():".to_string()),
                docstring: None,
                parent_id: None,
                metadata_json: "{}".to_string(),
            },
        ];
        let edges = vec![GraphEdge {
            id: "edge-1".to_string(),
            project_id: "proj".to_string(),
            src_type: "symbol".to_string(),
            src_id: "proj:src/app.py:Service.run".to_string(),
            dst_type: "symbol".to_string(),
            dst_id: "proj:src/app.py:helper".to_string(),
            kind: "calls".to_string(),
            file_id: file.id.clone(),
            file_path: "src/app.py".to_string(),
            line: 10,
            confidence: "exact".to_string(),
            evidence: "helper()".to_string(),
        }];

        store
            .upsert_file_graph(&file, &symbols, &[], &edges)
            .unwrap();

        let meta = store.get_file_meta(&file.id).unwrap();
        assert_eq!(meta, Some(("abc1234".to_string(), 100)));

        let nb = store
            .get_symbol_with_neighbors("proj", "proj:src/app.py:Service.run")
            .unwrap()
            .unwrap();
        assert_eq!(nb.symbol.name, "run");
        assert_eq!(nb.parent.unwrap().name, "Service");
        assert_eq!(nb.callees.len(), 1);
        assert_eq!(nb.callees[0].name, "helper");

        store.set_reading_progress("proj", "step-1", "done").unwrap();
        let prog = store.get_reading_progress("proj").unwrap();
        assert_eq!(prog.len(), 1);
        assert_eq!(prog[0].status, "done");
    }
}
