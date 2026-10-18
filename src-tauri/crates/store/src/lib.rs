use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    pub id: String,
    pub project_id: String,
    pub path: String,
    pub language: String,
    pub content_hash: String,
    pub size: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRecord {
    pub id: String,
    pub file_path: String,
    pub name: String,
    pub kind: String,
    pub line_start: u32,
    pub line_end: u32,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfusionMark {
    pub id: String,
    pub project_id: Option<String>,
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub code_snippet: String,
    pub category: String, // "overall" | "logic" | "syntax" | "principle"
    pub explanation: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExerciseRecord {
    pub id: String,
    pub unit_id: String,
    pub exercise_type: String, // "prediction" | "fill_in" | "bug_fix"
    pub question: String,
    pub snippet: String,
    pub options_json: Option<String>,
    pub reference_answer: String,
    pub file_path: String,
    pub next_review_at: String,
    pub repetition: i32,
    pub interval_days: i32,
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

    fn init_schema(&self) -> std::result::Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
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
                content_hash TEXT NOT NULL,
                size INTEGER NOT NULL,
                FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS symbols (
                id TEXT PRIMARY KEY,
                file_path TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                line_start INTEGER NOT NULL,
                line_end INTEGER NOT NULL,
                signature TEXT
            );

            CREATE TABLE IF NOT EXISTS edges (
                id TEXT PRIMARY KEY,
                source_symbol TEXT NOT NULL,
                target_symbol TEXT NOT NULL,
                edge_type TEXT NOT NULL,
                file_path TEXT NOT NULL,
                line INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS modules (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                name TEXT NOT NULL,
                path TEXT NOT NULL,
                summary TEXT,
                FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
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
                explanation TEXT,
                created_at TEXT NOT NULL
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
                interval_days INTEGER NOT NULL DEFAULT 1
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
        let _ = conn.execute("ALTER TABLE marks ADD COLUMN project_id TEXT;", []);
        Ok(())
    }

    pub fn save_mark(&self, mark: &ConfusionMark) -> std::result::Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT OR REPLACE INTO marks (id, project_id, file_path, line_start, line_end, code_snippet, category, explanation, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                mark.id,
                mark.project_id,
                mark.file_path,
                mark.line_start,
                mark.line_end,
                mark.code_snippet,
                mark.category,
                mark.explanation,
                mark.created_at
            ],
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

        let query = match (file_path, project_id) {
            (Some(_), Some(_)) => "SELECT id, project_id, file_path, line_start, line_end, code_snippet, category, explanation, created_at FROM marks WHERE file_path = ?1 AND project_id = ?2 ORDER BY created_at DESC",
            (Some(_), None) => "SELECT id, project_id, file_path, line_start, line_end, code_snippet, category, explanation, created_at FROM marks WHERE file_path = ?1 ORDER BY created_at DESC",
            (None, Some(_)) => "SELECT id, project_id, file_path, line_start, line_end, code_snippet, category, explanation, created_at FROM marks WHERE project_id = ?1 ORDER BY created_at DESC",
            (None, None) => "SELECT id, project_id, file_path, line_start, line_end, code_snippet, category, explanation, created_at FROM marks ORDER BY created_at DESC",
        };

        let mut stmt = conn.prepare(query)?;
        let row_mapper = |row: &rusqlite::Row| {
            Ok(ConfusionMark {
                id: row.get(0)?,
                project_id: row.get(1)?,
                file_path: row.get(2)?,
                line_start: row.get(3)?,
                line_end: row.get(4)?,
                code_snippet: row.get(5)?,
                category: row.get(6)?,
                explanation: row.get(7)?,
                created_at: row.get(8)?,
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

    pub fn insert_exercise(&self, ex: &ExerciseRecord) -> std::result::Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT OR REPLACE INTO exercises (id, unit_id, exercise_type, question, snippet, options_json, reference_answer, file_path, next_review_at, repetition, interval_days)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
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
                ex.interval_days
            ],
        )?;
        Ok(())
    }

    pub fn list_due_exercises(&self) -> std::result::Result<Vec<ExerciseRecord>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let now_str = Utc::now().to_rfc3339();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, unit_id, exercise_type, question, snippet, options_json, reference_answer, file_path, next_review_at, repetition, interval_days
            FROM exercises
            WHERE next_review_at <= ?1
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
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_init_and_mark() {
        let store = DbStore::in_memory().unwrap();
        let mark = ConfusionMark {
            id: "mark-1".to_string(),
            project_id: Some("proj-noesis".to_string()),
            file_path: "src/main.rs".to_string(),
            line_start: 10,
            line_end: 20,
            code_snippet: "fn complex() {}".to_string(),
            category: "logic".to_string(),
            explanation: Some("analogy".to_string()),
            created_at: Utc::now().to_rfc3339(),
        };

        store.save_mark(&mark).unwrap();
        let marks = store.list_marks(None, None).unwrap();
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].id, "mark-1");
        assert_eq!(marks[0].project_id, Some("proj-noesis".to_string()));
        assert_eq!(marks[0].category, "logic");

        let filtered = store.list_marks(Some("src/main.rs"), Some("proj-noesis")).unwrap();
        assert_eq!(filtered.len(), 1);

        let none_match = store.list_marks(Some("nonexistent.rs"), None).unwrap();
        assert_eq!(none_match.len(), 0);

        let diff_proj = store.list_marks(None, Some("other-proj")).unwrap();
        assert_eq!(diff_proj.len(), 0);
    }
}
