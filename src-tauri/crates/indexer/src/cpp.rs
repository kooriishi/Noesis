use crate::{ExtractedFile, IndexerError, LanguageExtractor};
use noesis_store::{GraphEdge, GraphSymbol};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileCommandEntry {
    pub directory: String,
    pub file: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub arguments: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct CompileCommandsDb {
    pub include_dirs: Vec<PathBuf>,
}

impl CompileCommandsDb {
    /// Loads `compile_commands.json` from `<project_root>/compile_commands.json` or
    /// `<project_root>/build/compile_commands.json`, and also scans `<project_root>/src/*/include`
    /// for ROS2 `colcon` package include directories.
    pub fn load_for_project(project_root: &Path) -> Self {
        let mut include_dirs = Vec::new();

        let candidates = [
            project_root.join("compile_commands.json"),
            project_root.join("build").join("compile_commands.json"),
        ];

        for candidate in &candidates {
            if candidate.exists() {
                if let Ok(content) = fs::read_to_string(candidate) {
                    if let Ok(entries) = serde_json::from_str::<Vec<CompileCommandEntry>>(&content)
                    {
                        for entry in entries {
                            let base_dir = PathBuf::from(&entry.directory);
                            let tokens: Vec<String> = if let Some(args) = entry.arguments {
                                args
                            } else if let Some(cmd) = entry.command {
                                cmd.split_whitespace().map(|s| s.to_string()).collect()
                            } else {
                                Vec::new()
                            };

                            let mut idx = 0;
                            while idx < tokens.len() {
                                let tok = &tokens[idx];
                                if (tok == "-I" || tok == "-isystem") && idx + 1 < tokens.len() {
                                    let p = PathBuf::from(&tokens[idx + 1]);
                                    let resolved = if p.is_absolute() {
                                        p
                                    } else {
                                        base_dir.join(p)
                                    };
                                    if !include_dirs.contains(&resolved) {
                                        include_dirs.push(resolved);
                                    }
                                    idx += 2;
                                    continue;
                                } else if let Some(stripped) =
                                    tok.strip_prefix("-I").or_else(|| tok.strip_prefix("-isystem"))
                                {
                                    if !stripped.is_empty() {
                                        let p = PathBuf::from(stripped);
                                        let resolved = if p.is_absolute() {
                                            p
                                        } else {
                                            base_dir.join(p)
                                        };
                                        if !include_dirs.contains(&resolved) {
                                            include_dirs.push(resolved);
                                        }
                                    }
                                }
                                idx += 1;
                            }
                        }
                    }
                }
            }
        }

        // Also auto-discover ROS2 package include directories (`include/` and `src/*/include`)
        let root_inc = project_root.join("include");
        if root_inc.is_dir() && !include_dirs.contains(&root_inc) {
            include_dirs.push(root_inc);
        }
        let src_dir = project_root.join("src");
        if src_dir.is_dir() {
            if let Ok(read_dir) = fs::read_dir(&src_dir) {
                for entry in read_dir.flatten() {
                    let pkg_inc = entry.path().join("include");
                    if pkg_inc.is_dir() && !include_dirs.contains(&pkg_inc) {
                        include_dirs.push(pkg_inc);
                    }
                }
            }
        }

        Self { include_dirs }
    }

    /// Resolves a `#include "header.hpp"` path to a project-relative path if found in `include_dirs`.
    pub fn resolve_include(&self, project_root: &Path, header: &str) -> Option<String> {
        for inc_dir in &self.include_dirs {
            let candidate = inc_dir.join(header);
            if candidate.exists() {
                if let Ok(rel) = candidate.strip_prefix(project_root) {
                    return Some(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        None
    }
}

pub struct CppExtractor {
    compile_db: Option<(PathBuf, CompileCommandsDb)>,
}

impl CppExtractor {
    pub fn new() -> Self {
        Self { compile_db: None }
    }

    pub fn with_project_root(project_root: &Path) -> Self {
        let db = CompileCommandsDb::load_for_project(project_root);
        Self {
            compile_db: Some((project_root.to_path_buf(), db)),
        }
    }
}

impl Default for CppExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageExtractor for CppExtractor {
    fn language_id(&self) -> &'static str {
        "cpp"
    }

    fn supported_extensions(&self) -> &[&str] {
        &["c", "h", "cpp", "hpp", "cc", "cxx", "hh"]
    }

    fn extract(
        &self,
        project_id: &str,
        rel_path: &Path,
        source: &str,
    ) -> Result<ExtractedFile, IndexerError> {
        let mut parser = Parser::new();
        let language = tree_sitter_cpp::LANGUAGE;
        parser
            .set_language(&language.into())
            .map_err(|e| IndexerError::Parse(format!("tree-sitter-cpp init error: {}", e)))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| IndexerError::Parse("Failed to parse C++ source".to_string()))?;

        let rel_str = rel_path.to_string_lossy().replace('\\', "/");
        let file_id = format!("{}:{}", project_id, rel_str);
        let bytes = source.as_bytes();

        let mut symbols = Vec::new();
        let mut edges = Vec::new();

        let mut ctx = CppWalkContext {
            project_id,
            file_id: &file_id,
            rel_path: &rel_str,
            bytes,
            compile_db: self.compile_db.as_ref(),
            symbols: &mut symbols,
            edges: &mut edges,
        };

        let mut ns_stack = Vec::new();
        walk_cpp(
            &mut ctx,
            tree.root_node(),
            &mut ns_stack,
            None,
            None,
        );

        // Post-process calls within the same file/class to exact symbol IDs
        let by_name: HashMap<String, String> = symbols
            .iter()
            .map(|s| (s.name.clone(), s.id.clone()))
            .collect();
        let by_qname: HashMap<String, String> = symbols
            .iter()
            .map(|s| (s.qualified_name.clone(), s.id.clone()))
            .collect();

        for edge in &mut edges {
            if edge.kind == "calls" {
                if let Some(exact_id) = by_qname.get(&edge.dst_id) {
                    edge.dst_id = exact_id.clone();
                    edge.confidence = "exact".to_string();
                } else if let Some(exact_id) = by_name.get(&edge.dst_id) {
                    edge.dst_id = exact_id.clone();
                    edge.confidence = "exact".to_string();
                }
            }
        }

        Ok(ExtractedFile {
            symbols,
            entities: Vec::new(),
            edges,
        })
    }
}

struct CppWalkContext<'a> {
    project_id: &'a str,
    file_id: &'a str,
    rel_path: &'a str,
    bytes: &'a [u8],
    compile_db: Option<&'a (PathBuf, CompileCommandsDb)>,
    symbols: &'a mut Vec<GraphSymbol>,
    edges: &'a mut Vec<GraphEdge>,
}

fn node_text<'a>(node: Node<'a>, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

fn extract_preceding_doxygen(node: Node, bytes: &[u8]) -> Option<String> {
    let mut comments = Vec::new();
    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        if p.kind() == "comment" {
            let text = node_text(p, bytes).trim();
            if text.starts_with("/**") || text.starts_with("///") || text.starts_with("/*!") {
                let cleaned = text
                    .trim_start_matches("/**")
                    .trim_start_matches("/*!")
                    .trim_end_matches("*/")
                    .lines()
                    .map(|l| {
                        l.trim()
                            .trim_start_matches("///")
                            .trim_start_matches('*')
                            .trim()
                    })
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                if !cleaned.is_empty() {
                    comments.push(cleaned);
                }
            }
            prev = p.prev_sibling();
        } else {
            break;
        }
    }
    if comments.is_empty() {
        None
    } else {
        comments.reverse();
        Some(comments.join("\n"))
    }
}

fn build_qualified(ns_stack: &[String], class_name: Option<&str>, name: &str) -> String {
    let mut parts: Vec<&str> = ns_stack
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| s.as_str())
        .collect();
    if let Some(cls) = class_name {
        if !cls.is_empty() {
            parts.push(cls);
        }
    }
    parts.push(name);
    parts.join("::")
}

fn find_function_declarator(node: Node) -> Option<Node> {
    if node.kind() == "function_declarator" {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(found) = find_function_declarator(child) {
            return Some(found);
        }
    }
    None
}

fn walk_cpp(
    ctx: &mut CppWalkContext,
    node: Node,
    ns_stack: &mut Vec<String>,
    current_class: Option<(&str, &str)>, // (class_sym_id, class_name)
    current_scope_id: Option<&str>,
) {
    let kind = node.kind();

    match kind {
        "preproc_include" => {
            if let Some(path_node) = node.child_by_field_name("path") {
                let raw_inc = node_text(path_node, ctx.bytes).trim();
                let clean_inc = raw_inc
                    .trim_matches(|c| c == '"' || c == '<' || c == '>')
                    .to_string();
                let line = (node.start_position().row + 1) as u32;

                let resolved_dst = ctx
                    .compile_db
                    .and_then(|(root, db)| db.resolve_include(root, &clean_inc))
                    .unwrap_or_else(|| clean_inc.clone());

                ctx.edges.push(GraphEdge {
                    id: format!("{}:include:{}:{}", ctx.file_id, line, resolved_dst),
                    project_id: ctx.project_id.to_string(),
                    src_type: "file".to_string(),
                    src_id: ctx.file_id.to_string(),
                    dst_type: "file".to_string(),
                    dst_id: resolved_dst,
                    kind: "imports".to_string(),
                    file_id: ctx.file_id.to_string(),
                    file_path: ctx.rel_path.to_string(),
                    line,
                    confidence: "exact".to_string(),
                    evidence: node_text(node, ctx.bytes).trim().to_string(),
                });
            }
            return;
        }
        "namespace_definition" => {
            let ns_name = node
                .child_by_field_name("name")
                .map(|n| node_text(n, ctx.bytes).trim().to_string())
                .unwrap_or_default();

            if !ns_name.is_empty() {
                let line_start = (node.start_position().row + 1) as u32;
                let line_end = (node.end_position().row + 1) as u32;
                let qname = build_qualified(ns_stack, None, &ns_name);
                let sym_id = format!("{}:ns:{}:{}", ctx.file_id, qname, line_start);
                ctx.symbols.push(GraphSymbol {
                    id: sym_id,
                    project_id: ctx.project_id.to_string(),
                    file_id: ctx.file_id.to_string(),
                    file_path: ctx.rel_path.to_string(),
                    name: ns_name.clone(),
                    qualified_name: qname,
                    kind: "namespace".to_string(),
                    line_start,
                    line_end,
                    col_start: node.start_position().column as u32,
                    col_end: node.end_position().column as u32,
                    signature: Some(format!("namespace {}", ns_name)),
                    docstring: extract_preceding_doxygen(node, ctx.bytes),
                    parent_id: None,
                    metadata_json: "{}".to_string(),
                });
            }

            ns_stack.push(ns_name);
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.named_children(&mut cursor) {
                    walk_cpp(ctx, child, ns_stack, current_class, current_scope_id);
                }
            }
            ns_stack.pop();
            return;
        }
        "class_specifier" | "struct_specifier" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, ctx.bytes).trim().to_string();
                let body = node.child_by_field_name("body");
                if body.is_none() {
                    // Forward declaration only
                    return;
                }

                let line_start = (node.start_position().row + 1) as u32;
                let line_end = (node.end_position().row + 1) as u32;
                let qname = build_qualified(
                    ns_stack,
                    current_class.map(|(_, cn)| cn),
                    &name,
                );
                let sym_id = format!("{}:{}:{}", ctx.file_id, qname, line_start);
                let sym_kind = if kind == "class_specifier" {
                    "class"
                } else {
                    "struct"
                };

                // Extract base classes from base_class_clause
                let mut bases = Vec::new();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if child.kind() == "base_class_clause" {
                        let clause_text = node_text(child, ctx.bytes)
                            .trim()
                            .trim_start_matches(':')
                            .trim();
                        for part in clause_text.split(',') {
                            let base_clean = part
                                .trim()
                                .trim_start_matches("public")
                                .trim_start_matches("protected")
                                .trim_start_matches("private")
                                .trim_start_matches("virtual")
                                .trim()
                                .to_string();
                            if !base_clean.is_empty() {
                                bases.push(base_clean.clone());
                                ctx.edges.push(GraphEdge {
                                    id: format!("{}:inherits:{}:{}", sym_id, line_start, base_clean),
                                    project_id: ctx.project_id.to_string(),
                                    src_type: "symbol".to_string(),
                                    src_id: sym_id.clone(),
                                    dst_type: "symbol".to_string(),
                                    dst_id: base_clean.clone(),
                                    kind: "inherits".to_string(),
                                    file_id: ctx.file_id.to_string(),
                                    file_path: ctx.rel_path.to_string(),
                                    line: line_start,
                                    confidence: "exact".to_string(),
                                    evidence: format!("{} : {}", name, base_clean),
                                });
                            }
                        }
                    }
                }

                let sig = if bases.is_empty() {
                    format!("{} {}", sym_kind, name)
                } else {
                    format!("{} {} : {}", sym_kind, name, bases.join(", "))
                };

                ctx.symbols.push(GraphSymbol {
                    id: sym_id.clone(),
                    project_id: ctx.project_id.to_string(),
                    file_id: ctx.file_id.to_string(),
                    file_path: ctx.rel_path.to_string(),
                    name: name.clone(),
                    qualified_name: qname,
                    kind: sym_kind.to_string(),
                    line_start,
                    line_end,
                    col_start: node.start_position().column as u32,
                    col_end: node.end_position().column as u32,
                    signature: Some(sig),
                    docstring: extract_preceding_doxygen(node, ctx.bytes),
                    parent_id: current_class.map(|(cid, _)| cid.to_string()),
                    metadata_json: json!({
                        "bases": bases,
                    })
                    .to_string(),
                });

                if let Some(b) = body {
                    let mut b_cursor = b.walk();
                    for child in b.named_children(&mut b_cursor) {
                        walk_cpp(
                            ctx,
                            child,
                            ns_stack,
                            Some((&sym_id, &name)),
                            Some(&sym_id),
                        );
                    }
                }
            }
            return;
        }
        "enum_specifier" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, ctx.bytes).trim().to_string();
                let line_start = (node.start_position().row + 1) as u32;
                let line_end = (node.end_position().row + 1) as u32;
                let qname = build_qualified(
                    ns_stack,
                    current_class.map(|(_, cn)| cn),
                    &name,
                );
                let sym_id = format!("{}:enum:{}:{}", ctx.file_id, qname, line_start);
                ctx.symbols.push(GraphSymbol {
                    id: sym_id,
                    project_id: ctx.project_id.to_string(),
                    file_id: ctx.file_id.to_string(),
                    file_path: ctx.rel_path.to_string(),
                    name: name.clone(),
                    qualified_name: qname,
                    kind: "enum".to_string(),
                    line_start,
                    line_end,
                    col_start: node.start_position().column as u32,
                    col_end: node.end_position().column as u32,
                    signature: Some(format!("enum {}", name)),
                    docstring: extract_preceding_doxygen(node, ctx.bytes),
                    parent_id: current_class.map(|(cid, _)| cid.to_string()),
                    metadata_json: "{}".to_string(),
                });
            }
            return;
        }
        "function_definition" => {
            if let Some(declarator) = node
                .child_by_field_name("declarator")
                .and_then(find_function_declarator)
            {
                if let Some(decl_name_node) = declarator.child_by_field_name("declarator") {
                    let raw_decl_name = node_text(decl_name_node, ctx.bytes).trim().to_string();
                    let (short_name, is_qualified_method) =
                        if let Some((_, short)) = raw_decl_name.rsplit_once("::") {
                            (short.to_string(), true)
                        } else {
                            (raw_decl_name.clone(), false)
                        };

                    let line_start = (node.start_position().row + 1) as u32;
                    let line_end = (node.end_position().row + 1) as u32;
                    let is_method = current_class.is_some() || is_qualified_method;
                    let sym_kind = if is_method { "method" } else { "function" };

                    let qname = if is_qualified_method {
                        build_qualified(ns_stack, None, &raw_decl_name)
                    } else {
                        build_qualified(
                            ns_stack,
                            current_class.map(|(_, cn)| cn),
                            &short_name,
                        )
                    };
                    let sym_id = format!("{}:{}:{}", ctx.file_id, qname, line_start);

                    let ret_type = node
                        .child_by_field_name("type")
                        .map(|t| node_text(t, ctx.bytes).trim().to_string())
                        .unwrap_or_default();
                    let decl_text = node_text(declarator, ctx.bytes)
                        .lines()
                        .map(|l| l.trim())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let sig = if ret_type.is_empty() {
                        decl_text
                    } else {
                        format!("{} {}", ret_type, decl_text)
                    };

                    ctx.symbols.push(GraphSymbol {
                        id: sym_id.clone(),
                        project_id: ctx.project_id.to_string(),
                        file_id: ctx.file_id.to_string(),
                        file_path: ctx.rel_path.to_string(),
                        name: short_name,
                        qualified_name: qname,
                        kind: sym_kind.to_string(),
                        line_start,
                        line_end,
                        col_start: node.start_position().column as u32,
                        col_end: node.end_position().column as u32,
                        signature: Some(sig),
                        docstring: extract_preceding_doxygen(node, ctx.bytes),
                        parent_id: current_class.map(|(cid, _)| cid.to_string()),
                        metadata_json: "{}".to_string(),
                    });

                    // Walk body and member initializer list under this function/method scope
                    let mut cursor = node.walk();
                    for child in node.named_children(&mut cursor) {
                        if child.kind() == "compound_statement"
                            || child.kind() == "field_initializer_list"
                        {
                            walk_cpp(ctx, child, ns_stack, current_class, Some(&sym_id));
                        }
                    }
                    return;
                }
            }
        }
        "field_declaration" | "declaration" => {
            // Check if this declaration inside a class/header is a method prototype
            if let Some(declarator) = find_function_declarator(node) {
                if let Some(decl_name_node) = declarator.child_by_field_name("declarator") {
                    let short_name = node_text(decl_name_node, ctx.bytes).trim().to_string();
                    if !short_name.is_empty() {
                        let line_start = (node.start_position().row + 1) as u32;
                        let line_end = (node.end_position().row + 1) as u32;
                        let is_method = current_class.is_some();
                        let sym_kind = if is_method { "method" } else { "function" };
                        let qname = build_qualified(
                            ns_stack,
                            current_class.map(|(_, cn)| cn),
                            &short_name,
                        );
                        let sym_id = format!("{}:{}:{}", ctx.file_id, qname, line_start);
                        let sig: String = node_text(node, ctx.bytes)
                            .lines()
                            .map(|l| l.trim())
                            .collect::<Vec<_>>()
                            .join(" ");

                        ctx.symbols.push(GraphSymbol {
                            id: sym_id,
                            project_id: ctx.project_id.to_string(),
                            file_id: ctx.file_id.to_string(),
                            file_path: ctx.rel_path.to_string(),
                            name: short_name,
                            qualified_name: qname,
                            kind: sym_kind.to_string(),
                            line_start,
                            line_end,
                            col_start: node.start_position().column as u32,
                            col_end: node.end_position().column as u32,
                            signature: Some(sig),
                            docstring: extract_preceding_doxygen(node, ctx.bytes),
                            parent_id: current_class.map(|(cid, _)| cid.to_string()),
                            metadata_json: json!({ "is_declaration": true }).to_string(),
                        });
                    }
                }
            }
        }
        "call_expression" => {
            if let Some(fn_node) = node.child_by_field_name("function") {
                let raw_callee = node_text(fn_node, ctx.bytes).trim().to_string();
                let line = (node.start_position().row + 1) as u32;
                let snippet: String = node_text(node, ctx.bytes)
                    .lines()
                    .map(|l| l.trim())
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(220)
                    .collect();

                // Strip template arguments `<...>` for the symbol name lookup
                let base_callee = raw_callee
                    .split('<')
                    .next()
                    .unwrap_or(&raw_callee)
                    .trim();

                let (dst_id, confidence) = if let Some(member) = base_callee.strip_prefix("this->")
                {
                    if let Some((_, cls_name)) = current_class {
                        (
                            build_qualified(ns_stack, Some(cls_name), member),
                            "exact",
                        )
                    } else {
                        (member.to_string(), "heuristic")
                    }
                } else if base_callee.contains('.') || base_callee.contains("->") {
                    let method_part = base_callee
                        .rsplit(|c| c == '.' || c == '>')
                        .next()
                        .unwrap_or(base_callee)
                        .trim();
                    (method_part.to_string(), "heuristic")
                } else if !base_callee.contains("::") && current_class.is_some() {
                    (base_callee.to_string(), "exact")
                } else {
                    (base_callee.to_string(), "exact")
                };

                let src_id = current_scope_id
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| ctx.file_id.to_string());
                let src_type = if current_scope_id.is_some() {
                    "symbol"
                } else {
                    "file"
                };

                ctx.edges.push(GraphEdge {
                    id: format!(
                        "{}:call:{}:{}:{}",
                        ctx.file_id,
                        line,
                        node.start_position().column,
                        dst_id
                    ),
                    project_id: ctx.project_id.to_string(),
                    src_type: src_type.to_string(),
                    src_id,
                    dst_type: "symbol".to_string(),
                    dst_id,
                    kind: "calls".to_string(),
                    file_id: ctx.file_id.to_string(),
                    file_path: ctx.rel_path.to_string(),
                    line,
                    confidence: confidence.to_string(),
                    evidence: snippet,
                });
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk_cpp(ctx, child, ns_stack, current_class, current_scope_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpp_extractor_on_rclcpp_node() {
        let extractor = CppExtractor::new();
        let cpp_src = r#"
#include "robot_voice_core/voice_state_machine.hpp"
#include "rclcpp/rclcpp.hpp"

namespace robot_voice_core {

/** @brief 语音状态编排节点 */
class VoiceOrchestratorNode final : public rclcpp::Node {
public:
  VoiceOrchestratorNode() : Node("voice_orchestrator") {
    state_publisher_ = create_publisher<robot_voice_interfaces::msg::VoiceState>("/voice/state", 10);
    publishState();
  }

private:
  /** @brief 发布当前状态 */
  void publishState() {
    state_publisher_->publish(msg_);
  }
};

} // namespace robot_voice_core
"#;

        let extracted = extractor
            .extract(
                "robot-voice",
                Path::new("src/robot_voice_core/src/voice_orchestrator_node.cpp"),
                cpp_src,
            )
            .unwrap();

        let sym_names: Vec<&str> = extracted.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(sym_names.contains(&"robot_voice_core"));
        assert!(sym_names.contains(&"VoiceOrchestratorNode"));
        assert!(sym_names.contains(&"publishState"));

        let cls = extracted
            .symbols
            .iter()
            .find(|s| s.name == "VoiceOrchestratorNode")
            .unwrap();
        assert_eq!(cls.kind, "class");
        assert_eq!(
            cls.qualified_name,
            "robot_voice_core::VoiceOrchestratorNode"
        );
        assert_eq!(cls.docstring.as_deref(), Some("@brief 语音状态编排节点"));

        // Check inherits edge to rclcpp::Node
        let inherits: Vec<&GraphEdge> = extracted
            .edges
            .iter()
            .filter(|e| e.kind == "inherits")
            .collect();
        assert_eq!(inherits.len(), 1);
        assert_eq!(inherits[0].dst_id, "rclcpp::Node");

        // Check #include edges
        let includes: Vec<&GraphEdge> = extracted
            .edges
            .iter()
            .filter(|e| e.kind == "imports")
            .collect();
        assert_eq!(includes.len(), 2);

        // Check call edge from constructor to publishState
        let publish_sym = extracted
            .symbols
            .iter()
            .find(|s| s.name == "publishState")
            .unwrap();
        let calls: Vec<&GraphEdge> = extracted
            .edges
            .iter()
            .filter(|e| e.kind == "calls")
            .collect();
        assert!(calls.iter().any(|c| c.dst_id == publish_sym.id));
    }
}
