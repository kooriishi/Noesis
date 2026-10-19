pub mod cpp;
pub mod python;

pub use cpp::{CompileCommandsDb, CppExtractor};
pub use python::PythonExtractor;

use chrono::Utc;
use ignore::WalkBuilder;
use noesis_project::{compute_hash, detect_language, is_sensitive_path, normalize_path};
use noesis_store::{
    DbStore, GraphEdge, GraphEntity, GraphFile, GraphModule, GraphSymbol, StoreError,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IndexerError {
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Store error: {0}")]
    Store(#[from] StoreError),
}

#[derive(Debug, Clone, Default)]
pub struct ExtractedFile {
    pub symbols: Vec<GraphSymbol>,
    pub entities: Vec<GraphEntity>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Default)]
pub struct GraphBatch {
    pub modules: Vec<GraphModule>,
    pub entities: Vec<GraphEntity>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IndexReport {
    pub project_id: String,
    pub total_files: usize,
    pub updated_files: usize,
    pub skipped_files: usize,
    pub deleted_files: usize,
    pub total_symbols: usize,
    pub total_entities: usize,
    pub total_edges: usize,
    pub detected_domains: Vec<String>,
}

/// Pluggable language AST extractor trait (B1)
pub trait LanguageExtractor: Send + Sync {
    fn language_id(&self) -> &'static str;
    fn supported_extensions(&self) -> &[&str];
    fn extract(
        &self,
        project_id: &str,
        rel_path: &Path,
        source: &str,
    ) -> Result<ExtractedFile, IndexerError>;
}

/// Pluggable domain adapter trait (B1/B2/B4)
pub trait DomainAdapter: Send + Sync {
    fn domain_id(&self) -> &'static str;
    fn detect(&self, project_root: &Path) -> bool;
    fn analyze_project(
        &self,
        project_id: &str,
        project_root: &Path,
        store: &DbStore,
    ) -> Result<GraphBatch, IndexerError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub line: u32,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleNode {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub kind: String,
    pub is_entry: bool,
    pub symbols_count: usize,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingStep {
    #[serde(default)]
    pub id: String,
    pub step: u32,
    #[serde(default)]
    pub stage: String,
    pub file_path: String,
    #[serde(default)]
    pub symbol_id: Option<String>,
    #[serde(default)]
    pub line_start: Option<u32>,
    #[serde(default)]
    pub line_end: Option<u32>,
    pub title: String,
    pub rationale: String,
    pub symbol_count: usize,
    pub priority: u32,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMap {
    pub root: String,
    #[serde(default)]
    pub detected_domains: Vec<String>,
    pub modules: Vec<ModuleNode>,
    pub entry_points: Vec<String>,
    pub reading_route: Vec<ReadingStep>,
    pub total_symbols: usize,
    pub indexed_files_count: usize,
    #[serde(default)]
    pub skipped_files_count: usize,
    #[serde(default)]
    pub entities: Vec<GraphEntity>,
    #[serde(default)]
    pub edges: Vec<GraphEdge>,
}

pub struct CodeIndexer {
    py_extractor: PythonExtractor,
    rust_fn_re: Regex,
    ts_fn_re: Regex,
    ts_arrow_fn_re: Regex,
    rust_struct_re: Regex,
    ts_class_re: Regex,
    go_fn_re: Regex,
    go_type_re: Regex,
    java_class_re: Regex,
}

impl Default for CodeIndexer {
    fn default() -> Self {
        Self {
            py_extractor: PythonExtractor::new(),
            rust_fn_re: Regex::new(r"(?m)^\s*(?:pub\s+)?(?:async\s+)?fn\s+([a-zA-Z0-9_]+)\s*\(")
                .unwrap(),
            ts_fn_re: Regex::new(
                r"(?m)^\s*(?:export\s+)?(?:async\s+)?function\s+([a-zA-Z0-9_]+)\s*\(",
            )
            .unwrap(),
            ts_arrow_fn_re: Regex::new(
                r"(?m)^\s*(?:export\s+)?(?:const|let)\s+([a-zA-Z0-9_]+)\s*(?::\s*[^=]+)?\s*=\s*(?:async\s*)?(?:\([^)]*\)|[a-zA-Z0-9_]+)\s*=>",
            )
            .unwrap(),
            rust_struct_re: Regex::new(
                r"(?m)^\s*(?:pub\s+)?(?:struct|enum|trait)\s+([a-zA-Z0-9_]+)",
            )
            .unwrap(),
            ts_class_re: Regex::new(
                r"(?m)^\s*(?:export\s+)?(?:class|interface|type)\s+([a-zA-Z0-9_]+)",
            )
            .unwrap(),
            go_fn_re: Regex::new(r"(?m)^\s*func\s+(?:\([^)]+\)\s+)?([a-zA-Z0-9_]+)\s*\(").unwrap(),
            go_type_re: Regex::new(r"(?m)^\s*type\s+([a-zA-Z0-9_]+)\s+(?:struct|interface)")
                .unwrap(),
            java_class_re: Regex::new(
                r"(?m)^\s*(?:public|protected|private)?\s*(?:static\s+)?(?:final\s+)?(?:class|interface|enum|record)\s+([a-zA-Z0-9_]+)",
            )
            .unwrap(),
        }
    }
}

impl CodeIndexer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Incrementally indexes a project directory into SQLite `DbStore`, running Tree-sitter
    /// extractors (`PythonExtractor`, `CppExtractor`) and any registered `DomainAdapter`s (B1/B3).
    pub fn index_workspace(
        &self,
        project_root: &Path,
        store: &DbStore,
        adapters: &[&dyn DomainAdapter],
    ) -> Result<(ProjectMap, IndexReport), IndexerError> {
        let canonical_root =
            fs::canonicalize(project_root).unwrap_or_else(|_| project_root.to_path_buf());
        let project_id = normalize_path(&canonical_root.to_string_lossy());
        let cpp_extractor = CppExtractor::with_project_root(&canonical_root);

        let mut active_file_ids = HashSet::new();
        let mut updated_files = 0usize;
        let mut skipped_files = 0usize;
        let mut total_files = 0usize;

        let walker = WalkBuilder::new(&canonical_root)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .hidden(false)
            .filter_entry(|entry| {
                let name = entry.file_name().to_string_lossy();
                !matches!(
                    name.as_ref(),
                    ".git"
                        | "node_modules"
                        | "target"
                        | "dist"
                        | "__pycache__"
                        | ".venv"
                        | "build"
                        | "install"
                        | "log"
                )
            })
            .build();

        for entry in walker.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let rel_path = match path.strip_prefix(&canonical_root) {
                Ok(r) => r,
                Err(_) => continue,
            };
            if is_sensitive_path(rel_path) {
                continue;
            }

            let file_name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let lang = detect_language(&file_name);

            let meta = match fs::metadata(path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.len() > 2 * 1024 * 1024 {
                continue;
            }

            let content = match fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let rel_str = normalize_path(&rel_path.to_string_lossy());
            let file_id = format!("{}:{}", project_id, rel_str);
            active_file_ids.insert(file_id.clone());
            total_files += 1;

            let sha256 = compute_hash(&content);
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            if let Ok(Some((old_sha, _old_mtime))) = store.get_file_meta(&file_id) {
                if old_sha == sha256 && !old_sha.is_empty() {
                    skipped_files += 1;
                    continue;
                }
            }

            let extracted = match lang.as_str() {
                "python" => self
                    .py_extractor
                    .extract(&project_id, rel_path, &content)
                    .unwrap_or_default(),
                "c" | "cpp" => cpp_extractor
                    .extract(&project_id, rel_path, &content)
                    .unwrap_or_default(),
                _ => self.extract_fallback(&project_id, &file_id, &rel_str, &content, &lang),
            };

            let graph_file = GraphFile {
                id: file_id,
                project_id: project_id.clone(),
                path: rel_str,
                language: lang,
                sha256,
                mtime,
                size: meta.len() as i64,
                indexed_at: Utc::now().timestamp(),
            };

            store.upsert_file_graph(
                &graph_file,
                &extracted.symbols,
                &extracted.entities,
                &extracted.edges,
            )?;
            updated_files += 1;
        }

        let deleted_files = store.delete_stale_files(&project_id, &active_file_ids)?;

        // Run domain adapters (ROS2, LLM App, etc.)
        let mut detected_domains = Vec::new();
        for adapter in adapters {
            if adapter.detect(&canonical_root) {
                let dom_id = adapter.domain_id().to_string();
                detected_domains.push(dom_id.clone());
                let batch = adapter.analyze_project(&project_id, &canonical_root, store)?;
                store.upsert_domain_batch(
                    &project_id,
                    &dom_id,
                    &batch.modules,
                    &batch.entities,
                    &batch.edges,
                )?;
            }
        }

        let map = self.build_project_map_from_store(
            &project_id,
            &detected_domains,
            total_files,
            skipped_files,
            store,
        )?;

        let report = IndexReport {
            project_id,
            total_files,
            updated_files,
            skipped_files,
            deleted_files,
            total_symbols: map.total_symbols,
            total_entities: map.entities.len(),
            total_edges: map.edges.len(),
            detected_domains,
        };

        Ok((map, report))
    }

    pub fn build_project_map_from_store(
        &self,
        project_id: &str,
        detected_domains: &[String],
        total_files: usize,
        skipped_files: usize,
        store: &DbStore,
    ) -> Result<ProjectMap, IndexerError> {
        let files = store.list_project_files(project_id)?;
        let symbols = store.list_project_symbols(project_id, None)?;
        let domain_modules = store.list_project_modules(project_id)?;
        let entities = store.get_entities_by_domain(project_id, None, None)?;
        let edges = store.get_edges_filtered(project_id, None)?;
        let progress = store.get_reading_progress(project_id)?;

        let progress_map: HashMap<String, String> = progress
            .into_iter()
            .map(|p| (p.step_id, p.status))
            .collect();

        let mut file_symbol_counts: HashMap<String, usize> = HashMap::new();
        for sym in &symbols {
            *file_symbol_counts.entry(sym.file_path.clone()).or_insert(0) += 1;
        }

        let mut file_deps: HashMap<String, Vec<String>> = HashMap::new();
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        for edge in &edges {
            if edge.kind == "imports" {
                file_deps
                    .entry(edge.file_path.clone())
                    .or_default()
                    .push(edge.dst_id.clone());
                *in_degree.entry(edge.dst_id.clone()).or_insert(0) += 1;
            } else if edge.kind == "calls" || edge.kind == "inherits" {
                *in_degree.entry(edge.dst_id.clone()).or_insert(0) += 1;
            }
        }

        let mut entry_points = Vec::new();
        for f in &files {
            let lower = f.path.to_lowercase();
            let is_entry = lower.ends_with("main.rs")
                || lower.ends_with("lib.rs")
                || lower.ends_with("main.ts")
                || lower.ends_with("main.py")
                || lower.ends_with("app.py")
                || lower.ends_with("app.tsx")
                || lower.ends_with(".launch.py")
                || lower.ends_with("_node.cpp")
                || lower.ends_with("_node.py")
                || lower.ends_with("package.xml");
            if is_entry {
                entry_points.push(f.path.clone());
            }
        }

        let mut modules = Vec::new();
        for dm in &domain_modules {
            modules.push(ModuleNode {
                id: dm.id.clone(),
                name: dm.name.clone(),
                path: dm.root_path.clone(),
                kind: dm.kind.clone(),
                is_entry: true,
                symbols_count: symbols
                    .iter()
                    .filter(|s| s.file_path.starts_with(&dm.root_path))
                    .count(),
                dependencies: Vec::new(),
            });
        }

        for f in &files {
            let is_entry = entry_points.contains(&f.path);
            let name = Path::new(&f.path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| f.path.clone());

            modules.push(ModuleNode {
                id: f.path.clone(),
                name,
                path: f.path.clone(),
                kind: f.language.clone(),
                is_entry,
                symbols_count: *file_symbol_counts.get(&f.path).unwrap_or(&0),
                dependencies: file_deps.get(&f.path).cloned().unwrap_or_default(),
            });
        }

        // Generate 4-stage deterministic reading route (B5)
        let reading_route = self.generate_reading_route(
            &files,
            &symbols,
            &entities,
            &edges,
            &file_symbol_counts,
            &in_degree,
            &progress_map,
        );

        Ok(ProjectMap {
            root: project_id.to_string(),
            detected_domains: detected_domains.to_vec(),
            modules,
            entry_points,
            reading_route,
            total_symbols: symbols.len(),
            indexed_files_count: if total_files > 0 {
                total_files
            } else {
                files.len()
            },
            skipped_files_count: skipped_files,
            entities,
            edges,
        })
    }

    fn generate_reading_route(
        &self,
        files: &[GraphFile],
        symbols: &[GraphSymbol],
        entities: &[GraphEntity],
        edges: &[GraphEdge],
        file_symbol_counts: &HashMap<String, usize>,
        in_degree: &HashMap<String, usize>,
        progress_map: &HashMap<String, String>,
    ) -> Vec<ReadingStep> {
        let mut steps = Vec::new();
        let mut seen_files = HashSet::new();
        let mut step_num = 1u32;

        // Stage 1: Entry & Launch & Manifests (entry_and_build)
        for f in files {
            let lower = f.path.to_lowercase();
            if lower.ends_with(".launch.py")
                || lower.ends_with("package.xml")
                || lower.ends_with("main.py")
                || lower.ends_with("main.rs")
                || lower.ends_with("lib.rs")
            {
                if seen_files.insert(f.path.clone()) {
                    let step_id = format!("step:{}", f.path);
                    let status = progress_map
                        .get(&step_id)
                        .cloned()
                        .unwrap_or_else(|| "unread".to_string());
                    let fname = Path::new(&f.path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy();
                    let rationale = if lower.ends_with(".launch.py") {
                        "ROS2 启动编排入口：定义了系统启动时拉起的节点、参数文件及子 launch 包含关系。".to_string()
                    } else if lower.ends_with("package.xml") {
                        "ROS2 包清单：声明包名、构建类型与上下游包依赖边界。".to_string()
                    } else {
                        "系统主入口：初始化运行时并装配核心业务组件。".to_string()
                    };
                    steps.push(ReadingStep {
                        id: step_id,
                        step: step_num,
                        stage: "entry_and_build".to_string(),
                        file_path: f.path.clone(),
                        symbol_id: None,
                        line_start: Some(1),
                        line_end: None,
                        title: format!("阶段一 [入口与拓扑]: {}", fname),
                        rationale,
                        symbol_count: *file_symbol_counts.get(&f.path).unwrap_or(&0),
                        priority: 1,
                        status,
                    });
                    step_num += 1;
                }
            }
        }

        // Stage 2: Core Interfaces, Messages, State Machines & Schemas (core_data_and_interfaces)
        for f in files {
            if seen_files.contains(&f.path) {
                continue;
            }
            let lower = f.path.to_lowercase();
            let is_interface = f.language == "ros2_interface"
                || lower.contains("/include/")
                || lower.ends_with("types.hpp")
                || lower.ends_with("state_machine.hpp")
                || lower.ends_with("interfaces.hpp")
                || lower.contains("schema")
                || lower.contains("models");

            if is_interface && seen_files.insert(f.path.clone()) {
                let step_id = format!("step:{}", f.path);
                let status = progress_map
                    .get(&step_id)
                    .cloned()
                    .unwrap_or_else(|| "unread".to_string());
                let fname = Path::new(&f.path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                let deg = in_degree.get(&f.path).copied().unwrap_or(0);
                steps.push(ReadingStep {
                    id: step_id,
                    step: step_num,
                    stage: "core_data_and_interfaces".to_string(),
                    file_path: f.path.clone(),
                    symbol_id: None,
                    line_start: Some(1),
                    line_end: None,
                    title: format!("阶段二 [契约与数据结构]: {}", fname),
                    rationale: format!(
                        "核心通信接口或领域状态头文件（被引用度 {}），先掌握消息字段与接口抽象再看实现。",
                        deg
                    ),
                    symbol_count: *file_symbol_counts.get(&f.path).unwrap_or(&0),
                    priority: 2,
                    status,
                });
                step_num += 1;
            }
        }

        // Stage 3: Nodes, Conversation Engines, LLM Providers & Workflows (execution_and_comm)
        let mut exec_candidates: Vec<&GraphFile> = files
            .iter()
            .filter(|f| {
                !seen_files.contains(&f.path)
                    && !f.path.to_lowercase().contains("/test/")
                    && !f.path.to_lowercase().ends_with("_test.cpp")
                    && !f.path.to_lowercase().ends_with("_test.py")
                    && (*file_symbol_counts.get(&f.path).unwrap_or(&0) > 0
                        || entities.iter().any(|e| e.file_path == f.path))
            })
            .collect();

        exec_candidates.sort_by(|a, b| {
            let ent_a = entities.iter().filter(|e| e.file_path == a.path).count();
            let ent_b = entities.iter().filter(|e| e.file_path == b.path).count();
            let sym_a = *file_symbol_counts.get(&a.path).unwrap_or(&0);
            let sym_b = *file_symbol_counts.get(&b.path).unwrap_or(&0);
            (ent_b * 3 + sym_b).cmp(&(ent_a * 3 + sym_a))
        });

        for f in exec_candidates {
            if seen_files.insert(f.path.clone()) {
                let step_id = format!("step:{}", f.path);
                let status = progress_map
                    .get(&step_id)
                    .cloned()
                    .unwrap_or_else(|| "unread".to_string());
                let fname = Path::new(&f.path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                let file_ents: Vec<&GraphEntity> =
                    entities.iter().filter(|e| e.file_path == f.path).collect();
                let first_sym = symbols.iter().find(|s| s.file_path == f.path);
                let out_calls = edges
                    .iter()
                    .filter(|e| e.file_path == f.path && e.kind == "calls")
                    .count();

                let rationale = if !file_ents.is_empty() {
                    let kinds: Vec<String> = file_ents
                        .iter()
                        .map(|e| format!("{}:{}", e.kind, e.name))
                        .take(3)
                        .collect();
                    format!(
                        "主干业务执行单元：包含领域实体 [{}]，触发 {} 处跨函数调用。",
                        kinds.join(", "),
                        out_calls
                    )
                } else {
                    format!(
                        "核心实现模块：包含 {} 个符号与 {} 处调用链路。",
                        file_symbol_counts.get(&f.path).unwrap_or(&0),
                        out_calls
                    )
                };

                steps.push(ReadingStep {
                    id: step_id,
                    step: step_num,
                    stage: "execution_and_comm".to_string(),
                    file_path: f.path.clone(),
                    symbol_id: first_sym.map(|s| s.id.clone()),
                    line_start: first_sym.map(|s| s.line_start),
                    line_end: first_sym.map(|s| s.line_end),
                    title: format!("阶段三 [节点与执行链路]: {}", fname),
                    rationale,
                    symbol_count: *file_symbol_counts.get(&f.path).unwrap_or(&0),
                    priority: 3,
                    status,
                });
                step_num += 1;
            }
        }

        // Stage 4: Configs & Verification Tests (config_and_tests)
        for f in files {
            if seen_files.contains(&f.path) {
                continue;
            }
            let lower = f.path.to_lowercase();
            if lower.ends_with(".yaml")
                || lower.ends_with(".yml")
                || lower.contains("/test/")
                || lower.ends_with("_test.cpp")
                || lower.ends_with("_test.py")
            {
                if seen_files.insert(f.path.clone()) {
                    let step_id = format!("step:{}", f.path);
                    let status = progress_map
                        .get(&step_id)
                        .cloned()
                        .unwrap_or_else(|| "unread".to_string());
                    let fname = Path::new(&f.path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy();
                    steps.push(ReadingStep {
                        id: step_id,
                        step: step_num,
                        stage: "config_and_tests".to_string(),
                        file_path: f.path.clone(),
                        symbol_id: None,
                        line_start: Some(1),
                        line_end: None,
                        title: format!("阶段四 [配置与边界测试]: {}", fname),
                        rationale: "运行时参数配置或自动化边界测试用例，可用于验证对异常分支的理解。".to_string(),
                        symbol_count: *file_symbol_counts.get(&f.path).unwrap_or(&0),
                        priority: 4,
                        status,
                    });
                    step_num += 1;
                }
            }
        }

        steps
    }

    fn extract_fallback(
        &self,
        project_id: &str,
        file_id: &str,
        rel_path: &str,
        content: &str,
        lang: &str,
    ) -> ExtractedFile {
        let legacy_syms = self.extract_symbols(content, rel_path, lang);
        let symbols = legacy_syms
            .into_iter()
            .map(|s| GraphSymbol {
                id: format!("{}:{}", project_id, s.id),
                project_id: project_id.to_string(),
                file_id: file_id.to_string(),
                file_path: s.file_path,
                qualified_name: s.name.clone(),
                name: s.name,
                kind: s.kind,
                line_start: s.line,
                line_end: s.line + 5,
                col_start: 0,
                col_end: 0,
                signature: s.signature,
                docstring: None,
                parent_id: None,
                metadata_json: "{}".to_string(),
            })
            .collect();
        ExtractedFile {
            symbols,
            entities: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub fn extract_symbols(&self, content: &str, file_path: &str, lang: &str) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        if lang == "python" {
            if let Ok(ext) = self
                .py_extractor
                .extract("local", Path::new(file_path), content)
            {
                return ext
                    .symbols
                    .into_iter()
                    .map(|s| Symbol {
                        id: s.id,
                        name: s.name,
                        kind: s.kind,
                        file_path: s.file_path,
                        line: s.line_start,
                        signature: s.signature,
                    })
                    .collect();
            }
        } else if lang == "cpp" || lang == "c" {
            let cpp_ext = CppExtractor::new();
            if let Ok(ext) = cpp_ext.extract("local", Path::new(file_path), content) {
                return ext
                    .symbols
                    .into_iter()
                    .map(|s| Symbol {
                        id: s.id,
                        name: s.name,
                        kind: s.kind,
                        file_path: s.file_path,
                        line: s.line_start,
                        signature: s.signature,
                    })
                    .collect();
            }
        }

        for (idx, line) in lines.iter().enumerate() {
            let line_num = (idx + 1) as u32;

            match lang {
                "rust" => {
                    if let Some(caps) = self.rust_fn_re.captures(line) {
                        symbols.push(Symbol {
                            id: format!("{}:{}:{}", file_path, line_num, &caps[1]),
                            name: caps[1].to_string(),
                            kind: "function".to_string(),
                            file_path: file_path.to_string(),
                            line: line_num,
                            signature: Some(line.trim().to_string()),
                        });
                    } else if let Some(caps) = self.rust_struct_re.captures(line) {
                        symbols.push(Symbol {
                            id: format!("{}:{}:{}", file_path, line_num, &caps[1]),
                            name: caps[1].to_string(),
                            kind: "type".to_string(),
                            file_path: file_path.to_string(),
                            line: line_num,
                            signature: Some(line.trim().to_string()),
                        });
                    }
                }
                "typescript" | "javascript" => {
                    if let Some(caps) = self.ts_fn_re.captures(line) {
                        symbols.push(Symbol {
                            id: format!("{}:{}:{}", file_path, line_num, &caps[1]),
                            name: caps[1].to_string(),
                            kind: "function".to_string(),
                            file_path: file_path.to_string(),
                            line: line_num,
                            signature: Some(line.trim().to_string()),
                        });
                    } else if let Some(caps) = self.ts_arrow_fn_re.captures(line) {
                        symbols.push(Symbol {
                            id: format!("{}:{}:{}", file_path, line_num, &caps[1]),
                            name: caps[1].to_string(),
                            kind: "function".to_string(),
                            file_path: file_path.to_string(),
                            line: line_num,
                            signature: Some(line.trim().to_string()),
                        });
                    } else if let Some(caps) = self.ts_class_re.captures(line) {
                        symbols.push(Symbol {
                            id: format!("{}:{}:{}", file_path, line_num, &caps[1]),
                            name: caps[1].to_string(),
                            kind: "type".to_string(),
                            file_path: file_path.to_string(),
                            line: line_num,
                            signature: Some(line.trim().to_string()),
                        });
                    }
                }
                "go" => {
                    if let Some(caps) = self.go_fn_re.captures(line) {
                        symbols.push(Symbol {
                            id: format!("{}:{}:{}", file_path, line_num, &caps[1]),
                            name: caps[1].to_string(),
                            kind: "function".to_string(),
                            file_path: file_path.to_string(),
                            line: line_num,
                            signature: Some(line.trim().to_string()),
                        });
                    } else if let Some(caps) = self.go_type_re.captures(line) {
                        symbols.push(Symbol {
                            id: format!("{}:{}:{}", file_path, line_num, &caps[1]),
                            name: caps[1].to_string(),
                            kind: "type".to_string(),
                            file_path: file_path.to_string(),
                            line: line_num,
                            signature: Some(line.trim().to_string()),
                        });
                    }
                }
                "java" | "csharp" => {
                    if let Some(caps) = self.java_class_re.captures(line) {
                        symbols.push(Symbol {
                            id: format!("{}:{}:{}", file_path, line_num, &caps[1]),
                            name: caps[1].to_string(),
                            kind: "class".to_string(),
                            file_path: file_path.to_string(),
                            line: line_num,
                            signature: Some(line.trim().to_string()),
                        });
                    }
                }
                _ => {}
            }
        }

        symbols
    }

    pub fn compute_project_map(&self, files: &[(String, String, String)]) -> ProjectMap {
        let mut all_symbols = Vec::new();
        let mut entry_points = Vec::new();
        let mut file_symbol_counts: HashMap<String, usize> = HashMap::new();
        let mut file_dependencies: HashMap<String, Vec<String>> = HashMap::new();

        let import_re =
            Regex::new(r#"(?:import|from|use)\s+['"]?([a-zA-Z0-9_\-\./]+)['"]?"#).unwrap();

        for (path, content, lang) in files {
            let syms = self.extract_symbols(content, path, lang);
            file_symbol_counts.insert(path.clone(), syms.len());
            all_symbols.extend(syms);

            let lower = path.to_lowercase();
            let is_entry = lower.ends_with("main.rs")
                || lower.ends_with("lib.rs")
                || lower.ends_with("main.ts")
                || lower.ends_with("index.ts")
                || lower.ends_with("index.tsx")
                || lower.ends_with("main.py")
                || lower.ends_with("app.tsx");

            if is_entry {
                entry_points.push(path.clone());
            }

            let mut deps = Vec::new();
            for cap in import_re.captures_iter(content) {
                deps.push(cap[1].to_string());
            }
            file_dependencies.insert(path.clone(), deps);
        }

        let mut modules = Vec::new();
        for (path, _, lang) in files {
            let is_entry = entry_points.contains(path);
            let name = Path::new(path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());

            modules.push(ModuleNode {
                id: path.clone(),
                name,
                path: path.clone(),
                kind: lang.clone(),
                is_entry,
                symbols_count: *file_symbol_counts.get(path).unwrap_or(&0),
                dependencies: file_dependencies.get(path).cloned().unwrap_or_default(),
            });
        }

        let mut reading_route = Vec::new();
        let mut visited = HashSet::new();
        let mut step = 1;

        for ep in &entry_points {
            if visited.insert(ep.clone()) {
                let sym_count = *file_symbol_counts.get(ep).unwrap_or(&0);
                reading_route.push(ReadingStep {
                    id: format!("step:{}", ep),
                    step,
                    stage: "entry_and_build".to_string(),
                    file_path: ep.clone(),
                    symbol_id: None,
                    line_start: Some(1),
                    line_end: None,
                    title: format!(
                        "入口启动单元: {}",
                        Path::new(ep).file_name().unwrap_or_default().to_string_lossy()
                    ),
                    rationale: "项目的执行入口或核心导出点，理清生命周期与启动流程。".to_string(),
                    symbol_count: sym_count,
                    priority: 1,
                    status: "unread".to_string(),
                });
                step += 1;
            }
        }

        let mut remaining_files: Vec<&String> = files
            .iter()
            .map(|(p, _, _)| p)
            .filter(|p| !visited.contains(*p))
            .collect();
        remaining_files.sort_by(|a, b| {
            let count_a = file_symbol_counts.get(*a).unwrap_or(&0);
            let count_b = file_symbol_counts.get(*b).unwrap_or(&0);
            count_b.cmp(count_a)
        });

        for f in remaining_files {
            let sym_count = *file_symbol_counts.get(f).unwrap_or(&0);
            if sym_count > 0 {
                reading_route.push(ReadingStep {
                    id: format!("step:{}", f),
                    step,
                    stage: "execution_and_comm".to_string(),
                    file_path: f.clone(),
                    symbol_id: None,
                    line_start: Some(1),
                    line_end: None,
                    title: format!(
                        "核心功能实现: {}",
                        Path::new(f).file_name().unwrap_or_default().to_string_lossy()
                    ),
                    rationale: "包含关键业务逻辑或数据类型定义，建议跟随依赖图下钻。".to_string(),
                    symbol_count: sym_count,
                    priority: 2,
                    status: "unread".to_string(),
                });
                step += 1;
            }
        }

        ProjectMap {
            root: ".".to_string(),
            detected_domains: Vec::new(),
            modules,
            entry_points,
            reading_route,
            total_symbols: all_symbols.len(),
            indexed_files_count: files.len(),
            skipped_files_count: 0,
            entities: Vec::new(),
            edges: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_symbols_rust() {
        let indexer = CodeIndexer::new();
        let code = r#"
            pub fn handle_request() {}
            async fn fetch_data() {}
            pub struct UserSession {}
        "#;
        let syms = indexer.extract_symbols(code, "src/main.rs", "rust");
        assert_eq!(syms.len(), 3);
        assert_eq!(syms[0].name, "handle_request");
        assert_eq!(syms[0].kind, "function");
        assert_eq!(syms[1].name, "fetch_data");
        assert_eq!(syms[2].name, "UserSession");
        assert_eq!(syms[2].kind, "type");
    }

    #[test]
    fn test_incremental_workspace_indexing_skips_unchanged_and_updates_modified() {
        let temp_dir = std::env::temp_dir().join(format!(
            "noesis_idx_test_{}",
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        let file_a = temp_dir.join("src").join("a.py");
        let file_b = temp_dir.join("src").join("b.cpp");

        fs::write(&file_a, "def alpha():\n    return 1\n").unwrap();
        fs::write(&file_b, "int beta() { return 2; }\n").unwrap();

        let store = DbStore::in_memory().unwrap();
        let indexer = CodeIndexer::new();

        // First run: both files updated
        let (_map1, rep1) = indexer.index_workspace(&temp_dir, &store, &[]).unwrap();
        assert_eq!(rep1.total_files, 2);
        assert_eq!(rep1.updated_files, 2);
        assert_eq!(rep1.skipped_files, 0);

        // Second run without changes: both files skipped!
        let (_map2, rep2) = indexer.index_workspace(&temp_dir, &store, &[]).unwrap();
        assert_eq!(rep2.total_files, 2);
        assert_eq!(rep2.updated_files, 0);
        assert_eq!(rep2.skipped_files, 2);

        // Modify file_a only: 1 updated, 1 skipped!
        fs::write(
            &file_a,
            "def alpha():\n    return 1\ndef gamma():\n    return alpha()\n",
        )
        .unwrap();
        let (_map3, rep3) = indexer.index_workspace(&temp_dir, &store, &[]).unwrap();
        assert_eq!(rep3.updated_files, 1);
        assert_eq!(rep3.skipped_files, 1);
        assert_eq!(rep3.total_symbols, 3);

        // Delete file_b: deleted_files == 1
        fs::remove_file(&file_b).unwrap();
        let (_map4, rep4) = indexer.index_workspace(&temp_dir, &store, &[]).unwrap();
        assert_eq!(rep4.deleted_files, 1);
        assert_eq!(rep4.total_symbols, 2);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_concurrent_workspace_indexing() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("noesis_idx_test_proj_{}", ts));
        let temp_db = std::env::temp_dir().join(format!("noesis_idx_test_db_{}.db", ts));
        fs::create_dir_all(&temp_dir).unwrap();

        // Generate 200 dummy python files
        for i in 0..200 {
            let fpath = temp_dir.join(format!("file_{:03}.py", i));
            fs::write(&fpath, format!("def func_{}():\n    return {}\n", i, i)).unwrap();
        }

        // Initialize DB store first to ensure schema exists
        {
            let store = DbStore::open(&temp_db).expect("create test db");
            let _ = store.ensure_schema();
        }

        let db_path_1 = temp_db.clone();
        let proj_dir_1 = temp_dir.clone();
        let handle1 = std::thread::spawn(move || {
            let store = DbStore::open(&db_path_1).expect("open store 1");
            let indexer = CodeIndexer::new();
            indexer.index_workspace(&proj_dir_1, &store, &[])
        });

        let db_path_2 = temp_db.clone();
        let proj_dir_2 = temp_dir.clone();
        let handle2 = std::thread::spawn(move || {
            let store = DbStore::open(&db_path_2).expect("open store 2");
            let indexer = CodeIndexer::new();
            indexer.index_workspace(&proj_dir_2, &store, &[])
        });

        let res1 = handle1.join().expect("thread 1 panic");
        let res2 = handle2.join().expect("thread 2 panic");

        assert!(res1.is_ok(), "indexer thread 1 failed: {:?}", res1.err());
        assert!(res2.is_ok(), "indexer thread 2 failed: {:?}", res2.err());

        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::remove_file(&temp_db);
        let _ = fs::remove_file(format!("{}-wal", temp_db.to_string_lossy()));
        let _ = fs::remove_file(format!("{}-shm", temp_db.to_string_lossy()));
    }
}
