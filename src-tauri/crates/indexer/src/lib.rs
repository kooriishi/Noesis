use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IndexerError {
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub id: String,
    pub name: String,
    pub kind: String, // "function" | "class" | "struct" | "interface" | "type" | "enum"
    pub file_path: String,
    pub line: u32,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleNode {
    pub id: String,
    pub name: String,
    pub path: String,
    pub is_entry: bool,
    pub symbols_count: usize,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingStep {
    pub step: u32,
    pub file_path: String,
    pub title: String,
    pub rationale: String,
    pub symbol_count: usize,
    pub priority: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMap {
    pub root: String,
    pub modules: Vec<ModuleNode>,
    pub entry_points: Vec<String>,
    pub reading_route: Vec<ReadingStep>,
    pub total_symbols: usize,
    pub indexed_files_count: usize,
}

pub struct CodeIndexer {
    rust_fn_re: Regex,
    ts_fn_re: Regex,
    py_fn_re: Regex,
    rust_struct_re: Regex,
    ts_class_re: Regex,
}

impl Default for CodeIndexer {
    fn default() -> Self {
        Self {
            rust_fn_re: Regex::new(r"(?m)^\s*(?:pub\s+)?(?:async\s+)?fn\s+([a-zA-Z0-9_]+)\s*\(").unwrap(),
            ts_fn_re: Regex::new(r"(?m)^\s*(?:export\s+)?(?:async\s+)?function\s+([a-zA-Z0-9_]+)\s*\(").unwrap(),
            py_fn_re: Regex::new(r"(?m)^\s*(?:async\s+)?def\s+([a-zA-Z0-9_]+)\s*\(").unwrap(),
            rust_struct_re: Regex::new(r"(?m)^\s*(?:pub\s+)?(?:struct|enum|trait)\s+([a-zA-Z0-9_]+)").unwrap(),
            ts_class_re: Regex::new(r"(?m)^\s*(?:export\s+)?(?:class|interface|type)\s+([a-zA-Z0-9_]+)").unwrap(),
        }
    }
}

impl CodeIndexer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn extract_symbols(&self, content: &str, file_path: &str, lang: &str) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

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
                "python" => {
                    if let Some(caps) = self.py_fn_re.captures(line) {
                        symbols.push(Symbol {
                            id: format!("{}:{}:{}", file_path, line_num, &caps[1]),
                            name: caps[1].to_string(),
                            kind: "function".to_string(),
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
        // files: slice of (file_path, content, language)
        let mut all_symbols = Vec::new();
        let mut entry_points = Vec::new();
        let mut file_symbol_counts: HashMap<String, usize> = HashMap::new();
        let mut file_dependencies: HashMap<String, Vec<String>> = HashMap::new();

        let import_re = Regex::new(r#"(?:import|from|use)\s+['"]?([a-zA-Z0-9_\-\./]+)['"]?"#).unwrap();

        for (path, content, lang) in files {
            let syms = self.extract_symbols(content, path, lang);
            file_symbol_counts.insert(path.clone(), syms.len());
            all_symbols.extend(syms);

            // Entrypoint detection
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

            // Extract imports
            let mut deps = Vec::new();
            for cap in import_re.captures_iter(content) {
                deps.push(cap[1].to_string());
            }
            file_dependencies.insert(path.clone(), deps);
        }

        // Generate modules
        let mut modules = Vec::new();
        for (path, _, _) in files {
            let is_entry = entry_points.contains(path);
            let name = Path::new(path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());

            modules.push(ModuleNode {
                id: path.clone(),
                name,
                path: path.clone(),
                is_entry,
                symbols_count: *file_symbol_counts.get(path).unwrap_or(&0),
                dependencies: file_dependencies.get(path).cloned().unwrap_or_default(),
            });
        }

        // Build reading route: Sort entrypoints first, then files by symbol count & dependencies
        let mut reading_route = Vec::new();
        let mut visited = HashSet::new();
        let mut step = 1;

        // 1. Entrypoints first
        for ep in &entry_points {
            if visited.insert(ep.clone()) {
                let sym_count = *file_symbol_counts.get(ep).unwrap_or(&0);
                reading_route.push(ReadingStep {
                    step,
                    file_path: ep.clone(),
                    title: format!("入口启动单元: {}", Path::new(ep).file_name().unwrap_or_default().to_string_lossy()),
                    rationale: "项目的执行入口或核心导出点，理清生命周期与启动流程。".to_string(),
                    symbol_count: sym_count,
                    priority: 1,
                });
                step += 1;
            }
        }

        // 2. Core modules sorted by symbol density
        let mut remaining_files: Vec<&String> = files.iter().map(|(p, _, _)| p).filter(|p| !visited.contains(*p)).collect();
        remaining_files.sort_by(|a, b| {
            let count_a = file_symbol_counts.get(*a).unwrap_or(&0);
            let count_b = file_symbol_counts.get(*b).unwrap_or(&0);
            count_b.cmp(count_a)
        });

        for f in remaining_files {
            let sym_count = *file_symbol_counts.get(f).unwrap_or(&0);
            if sym_count > 0 {
                reading_route.push(ReadingStep {
                    step,
                    file_path: f.clone(),
                    title: format!("核心功能实现: {}", Path::new(f).file_name().unwrap_or_default().to_string_lossy()),
                    rationale: "包含关键业务逻辑或数据类型定义，建议跟随依赖图下钻。".to_string(),
                    symbol_count: sym_count,
                    priority: 2,
                });
                step += 1;
            }
        }

        ProjectMap {
            root: ".".to_string(),
            modules,
            entry_points,
            reading_route,
            total_symbols: all_symbols.len(),
            indexed_files_count: files.len(),
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
    fn test_compute_project_map() {
        let indexer = CodeIndexer::new();
        let files = vec![
            (
                "src/main.rs".to_string(),
                "use crate::helper;\npub fn main() {}".to_string(),
                "rust".to_string(),
            ),
            (
                "src/helper.rs".to_string(),
                "pub fn compute_sum() {}\npub fn compute_avg() {}".to_string(),
                "rust".to_string(),
            ),
        ];

        let map = indexer.compute_project_map(&files);
        assert_eq!(map.entry_points.len(), 1);
        assert_eq!(map.entry_points[0], "src/main.rs");
        assert_eq!(map.total_symbols, 3);
        assert_eq!(map.reading_route.len(), 2);
        assert_eq!(map.reading_route[0].file_path, "src/main.rs");
        assert_eq!(map.reading_route[0].priority, 1);
    }
}
