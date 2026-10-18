use crate::{ExtractedFile, IndexerError, LanguageExtractor};
use noesis_store::{GraphEdge, GraphSymbol};
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use tree_sitter::{Node, Parser};

pub struct PythonExtractor;

impl PythonExtractor {
    pub fn new() -> Self {
        Self
    }

    fn module_name_from_path(rel_path: &str) -> String {
        let norm = rel_path.replace('\\', "/");
        let without_ext = norm
            .strip_suffix(".py")
            .unwrap_or(&norm)
            .strip_suffix("/__init__")
            .unwrap_or(&norm);
        without_ext
            .trim_start_matches("./")
            .trim_start_matches("src/")
            .replace('/', ".")
    }
}

impl Default for PythonExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageExtractor for PythonExtractor {
    fn language_id(&self) -> &'static str {
        "python"
    }

    fn supported_extensions(&self) -> &[&str] {
        &["py"]
    }

    fn extract(
        &self,
        project_id: &str,
        rel_path: &Path,
        source: &str,
    ) -> Result<ExtractedFile, IndexerError> {
        let mut parser = Parser::new();
        let language = tree_sitter_python::LANGUAGE;
        parser
            .set_language(&language.into())
            .map_err(|e| IndexerError::Parse(format!("tree-sitter-python init error: {}", e)))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| IndexerError::Parse("Failed to parse Python source".to_string()))?;

        let rel_str = rel_path.to_string_lossy().replace('\\', "/");
        let file_id = format!("{}:{}", project_id, rel_str);
        let module_qname = Self::module_name_from_path(&rel_str);

        let mut symbols = Vec::new();
        let mut edges = Vec::new();
        let mut import_map: HashMap<String, String> = HashMap::new();

        let root = tree.root_node();
        let bytes = source.as_bytes();

        let mut ctx = PyWalkContext {
            project_id,
            file_id: &file_id,
            rel_path: &rel_str,
            module_qname: &module_qname,
            bytes,
            symbols: &mut symbols,
            edges: &mut edges,
            import_map: &mut import_map,
        };

        walk_module(&mut ctx, root, None, None, &[]);

        // Post-process call edges to upgrade same-file targets to exact symbol IDs
        let local_by_name: HashMap<String, String> = symbols
            .iter()
            .map(|s| (s.name.clone(), s.id.clone()))
            .collect();
        let local_by_qname: HashMap<String, String> = symbols
            .iter()
            .map(|s| (s.qualified_name.clone(), s.id.clone()))
            .collect();

        for edge in &mut edges {
            if edge.kind == "calls" {
                if let Some(exact_id) = local_by_qname.get(&edge.dst_id) {
                    edge.dst_id = exact_id.clone();
                    edge.confidence = "exact".to_string();
                } else if let Some(exact_id) = local_by_name.get(&edge.dst_id) {
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

struct PyWalkContext<'a> {
    project_id: &'a str,
    file_id: &'a str,
    rel_path: &'a str,
    module_qname: &'a str,
    bytes: &'a [u8],
    symbols: &'a mut Vec<GraphSymbol>,
    edges: &'a mut Vec<GraphEdge>,
    import_map: &'a mut HashMap<String, String>,
}

fn node_text<'a>(node: Node<'a>, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

fn clean_docstring(raw: &str) -> String {
    let trimmed = raw.trim();
    for quote in ["\"\"\"", "'''", "\"", "'"] {
        if trimmed.starts_with(quote) && trimmed.ends_with(quote) && trimmed.len() >= quote.len() * 2
        {
            return trimmed[quote.len()..trimmed.len() - quote.len()]
                .trim()
                .to_string();
        }
    }
    trimmed.to_string()
}

fn extract_block_docstring(block_node: Node, bytes: &[u8]) -> Option<String> {
    let mut cursor = block_node.walk();
    for child in block_node.named_children(&mut cursor) {
        if child.kind() == "comment" {
            continue;
        }
        if child.kind() == "expression_statement" {
            if let Some(first_expr) = child.named_child(0) {
                if first_expr.kind() == "string" {
                    return Some(clean_docstring(node_text(first_expr, bytes)));
                }
            }
        }
        break;
    }
    None
}

fn walk_module(
    ctx: &mut PyWalkContext,
    node: Node,
    current_class_id: Option<&str>,
    current_scope_id: Option<&str>,
    pending_decorators: &[String],
) {
    let kind = node.kind();

    match kind {
        "import_statement" => {
            let line = (node.start_position().row + 1) as u32;
            let raw = node_text(node, ctx.bytes).trim().to_string();
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "dotted_name" {
                    let mod_name = node_text(child, ctx.bytes).trim().to_string();
                    ctx.import_map.insert(mod_name.clone(), mod_name.clone());
                    ctx.edges.push(GraphEdge {
                        id: format!("{}:import:{}:{}", ctx.file_id, line, mod_name),
                        project_id: ctx.project_id.to_string(),
                        src_type: "file".to_string(),
                        src_id: ctx.file_id.to_string(),
                        dst_type: "module".to_string(),
                        dst_id: mod_name,
                        kind: "imports".to_string(),
                        file_id: ctx.file_id.to_string(),
                        file_path: ctx.rel_path.to_string(),
                        line,
                        confidence: "exact".to_string(),
                        evidence: raw.clone(),
                    });
                } else if child.kind() == "aliased_import" {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let mod_name = node_text(name_node, ctx.bytes).trim().to_string();
                        let alias = child
                            .child_by_field_name("alias")
                            .map(|a| node_text(a, ctx.bytes).trim().to_string())
                            .unwrap_or_else(|| mod_name.clone());
                        ctx.import_map.insert(alias, mod_name.clone());
                        ctx.edges.push(GraphEdge {
                            id: format!("{}:import:{}:{}", ctx.file_id, line, mod_name),
                            project_id: ctx.project_id.to_string(),
                            src_type: "file".to_string(),
                            src_id: ctx.file_id.to_string(),
                            dst_type: "module".to_string(),
                            dst_id: mod_name,
                            kind: "imports".to_string(),
                            file_id: ctx.file_id.to_string(),
                            file_path: ctx.rel_path.to_string(),
                            line,
                            confidence: "exact".to_string(),
                            evidence: raw.clone(),
                        });
                    }
                }
            }
            return;
        }
        "import_from_statement" => {
            let line = (node.start_position().row + 1) as u32;
            let raw = node_text(node, ctx.bytes).trim().to_string();
            let module_name = node
                .child_by_field_name("module_name")
                .map(|n| node_text(n, ctx.bytes).trim().to_string())
                .unwrap_or_default();

            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if Some(child.id()) == node.child_by_field_name("module_name").map(|n| n.id()) {
                    continue;
                }
                if child.kind() == "dotted_name" {
                    let sym = node_text(child, ctx.bytes).trim().to_string();
                    let full = if module_name.is_empty() {
                        sym.clone()
                    } else {
                        format!("{}.{}", module_name, sym)
                    };
                    ctx.import_map.insert(sym, full.clone());
                    ctx.edges.push(GraphEdge {
                        id: format!("{}:import:{}:{}", ctx.file_id, line, full),
                        project_id: ctx.project_id.to_string(),
                        src_type: "file".to_string(),
                        src_id: ctx.file_id.to_string(),
                        dst_type: "module".to_string(),
                        dst_id: full,
                        kind: "imports".to_string(),
                        file_id: ctx.file_id.to_string(),
                        file_path: ctx.rel_path.to_string(),
                        line,
                        confidence: "exact".to_string(),
                        evidence: raw.clone(),
                    });
                } else if child.kind() == "aliased_import" {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let sym = node_text(name_node, ctx.bytes).trim().to_string();
                        let alias = child
                            .child_by_field_name("alias")
                            .map(|a| node_text(a, ctx.bytes).trim().to_string())
                            .unwrap_or_else(|| sym.clone());
                        let full = if module_name.is_empty() {
                            sym
                        } else {
                            format!("{}.{}", module_name, sym)
                        };
                        ctx.import_map.insert(alias, full.clone());
                        ctx.edges.push(GraphEdge {
                            id: format!("{}:import:{}:{}", ctx.file_id, line, full),
                            project_id: ctx.project_id.to_string(),
                            src_type: "file".to_string(),
                            src_id: ctx.file_id.to_string(),
                            dst_type: "module".to_string(),
                            dst_id: full,
                            kind: "imports".to_string(),
                            file_id: ctx.file_id.to_string(),
                            file_path: ctx.rel_path.to_string(),
                            line,
                            confidence: "exact".to_string(),
                            evidence: raw.clone(),
                        });
                    }
                }
            }
            return;
        }
        "decorated_definition" => {
            let mut decorators = Vec::new();
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "decorator" {
                    decorators.push(node_text(child, ctx.bytes).trim().to_string());
                } else {
                    walk_module(
                        ctx,
                        child,
                        current_class_id,
                        current_scope_id,
                        &decorators,
                    );
                }
            }
            return;
        }
        "class_definition" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, ctx.bytes).trim().to_string();
                let line_start = (node.start_position().row + 1) as u32;
                let line_end = (node.end_position().row + 1) as u32;
                let col_start = node.start_position().column as u32;
                let col_end = node.end_position().column as u32;

                let qname = format!("{}.{}", ctx.module_qname, name);
                let sym_id = format!("{}:{}", ctx.file_id, qname);

                let mut bases = Vec::new();
                if let Some(superclasses) = node.child_by_field_name("superclasses") {
                    let mut sc_cursor = superclasses.walk();
                    for base_child in superclasses.named_children(&mut sc_cursor) {
                        let base_name = node_text(base_child, ctx.bytes).trim().to_string();
                        if !base_name.is_empty() {
                            bases.push(base_name.clone());
                            let resolved_base = ctx
                                .import_map
                                .get(&base_name)
                                .cloned()
                                .unwrap_or(base_name.clone());
                            ctx.edges.push(GraphEdge {
                                id: format!("{}:inherits:{}:{}", sym_id, line_start, resolved_base),
                                project_id: ctx.project_id.to_string(),
                                src_type: "symbol".to_string(),
                                src_id: sym_id.clone(),
                                dst_type: "symbol".to_string(),
                                dst_id: resolved_base,
                                kind: "inherits".to_string(),
                                file_id: ctx.file_id.to_string(),
                                file_path: ctx.rel_path.to_string(),
                                line: line_start,
                                confidence: "exact".to_string(),
                                evidence: format!("class {}({})", name, base_name),
                            });
                        }
                    }
                }

                let docstring = node
                    .child_by_field_name("body")
                    .and_then(|b| extract_block_docstring(b, ctx.bytes));

                let sig = if bases.is_empty() {
                    format!("class {}:", name)
                } else {
                    format!("class {}({}):", name, bases.join(", "))
                };

                ctx.symbols.push(GraphSymbol {
                    id: sym_id.clone(),
                    project_id: ctx.project_id.to_string(),
                    file_id: ctx.file_id.to_string(),
                    file_path: ctx.rel_path.to_string(),
                    name: name.clone(),
                    qualified_name: qname,
                    kind: "class".to_string(),
                    line_start,
                    line_end,
                    col_start,
                    col_end,
                    signature: Some(sig),
                    docstring,
                    parent_id: current_class_id.map(|s| s.to_string()),
                    metadata_json: json!({
                        "bases": bases,
                        "decorators": pending_decorators,
                    })
                    .to_string(),
                });

                if let Some(body) = node.child_by_field_name("body") {
                    let mut b_cursor = body.walk();
                    for child in body.named_children(&mut b_cursor) {
                        walk_module(ctx, child, Some(&sym_id), Some(&sym_id), &[]);
                    }
                }
            }
            return;
        }
        "function_definition" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, ctx.bytes).trim().to_string();
                let line_start = (node.start_position().row + 1) as u32;
                let line_end = (node.end_position().row + 1) as u32;
                let col_start = node.start_position().column as u32;
                let col_end = node.end_position().column as u32;

                let is_method = current_class_id.is_some();
                let sym_kind = if is_method { "method" } else { "function" };

                let parent_qname = current_class_id
                    .and_then(|cid| ctx.symbols.iter().find(|s| s.id == cid))
                    .map(|s| s.qualified_name.clone())
                    .unwrap_or_else(|| ctx.module_qname.to_string());

                let qname = format!("{}.{}", parent_qname, name);
                let sym_id = format!("{}:{}:{}", ctx.file_id, qname, line_start);

                let params_text = node
                    .child_by_field_name("parameters")
                    .map(|p| node_text(p, ctx.bytes).trim().to_string())
                    .unwrap_or_else(|| "()".to_string());
                let ret_text = node
                    .child_by_field_name("return_type")
                    .map(|r| format!(" -> {}", node_text(r, ctx.bytes).trim()))
                    .unwrap_or_default();

                let raw_fn = node_text(node, ctx.bytes).trim_start();
                let prefix = if raw_fn.starts_with("async ") {
                    "async def"
                } else {
                    "def"
                };
                let sig = format!("{} {}{}{}:", prefix, name, params_text, ret_text);

                let docstring = node
                    .child_by_field_name("body")
                    .and_then(|b| extract_block_docstring(b, ctx.bytes));

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
                    col_start,
                    col_end,
                    signature: Some(sig),
                    docstring,
                    parent_id: current_class_id.map(|s| s.to_string()),
                    metadata_json: json!({
                        "decorators": pending_decorators,
                        "return_type": ret_text.trim_start_matches(" -> "),
                    })
                    .to_string(),
                });

                if let Some(body) = node.child_by_field_name("body") {
                    let mut b_cursor = body.walk();
                    for child in body.named_children(&mut b_cursor) {
                        walk_module(ctx, child, current_class_id, Some(&sym_id), &[]);
                    }
                }
            }
            return;
        }
        "assignment" => {
            // Extract top-level UPPER_CASE constants
            if current_scope_id.is_none() {
                if let Some(left) = node.child_by_field_name("left") {
                    if left.kind() == "identifier" {
                        let name = node_text(left, ctx.bytes).trim().to_string();
                        if !name.is_empty()
                            && name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                            && name.chars().any(|c| c.is_ascii_uppercase())
                        {
                            let line_start = (node.start_position().row + 1) as u32;
                            let line_end = (node.end_position().row + 1) as u32;
                            let qname = format!("{}.{}", ctx.module_qname, name);
                            let sym_id = format!("{}:{}", ctx.file_id, qname);
                            let val_preview: String = node_text(node, ctx.bytes)
                                .lines()
                                .next()
                                .unwrap_or("")
                                .chars()
                                .take(120)
                                .collect();

                            ctx.symbols.push(GraphSymbol {
                                id: sym_id,
                                project_id: ctx.project_id.to_string(),
                                file_id: ctx.file_id.to_string(),
                                file_path: ctx.rel_path.to_string(),
                                name,
                                qualified_name: qname,
                                kind: "constant".to_string(),
                                line_start,
                                line_end,
                                col_start: node.start_position().column as u32,
                                col_end: node.end_position().column as u32,
                                signature: Some(val_preview),
                                docstring: None,
                                parent_id: None,
                                metadata_json: "{}".to_string(),
                            });
                        }
                    }
                }
            }
        }
        "call" => {
            if let Some(fn_node) = node.child_by_field_name("function") {
                let callee_raw = node_text(fn_node, ctx.bytes).trim().to_string();
                let line = (node.start_position().row + 1) as u32;
                let call_snippet: String = node_text(node, ctx.bytes)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .chars()
                    .take(160)
                    .collect();

                let src_id = current_scope_id
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| ctx.file_id.to_string());
                let src_type = if current_scope_id.is_some() {
                    "symbol"
                } else {
                    "file"
                };

                let (dst_id, confidence) = if let Some(method_name) = callee_raw.strip_prefix("self.") {
                    if let Some(cid) = current_class_id {
                        if let Some(cls_sym) = ctx.symbols.iter().find(|s| s.id == cid) {
                            (format!("{}.{}", cls_sym.qualified_name, method_name), "exact")
                        } else {
                            (method_name.to_string(), "heuristic")
                        }
                    } else {
                        (method_name.to_string(), "heuristic")
                    }
                } else if let Some(imported) = ctx.import_map.get(&callee_raw) {
                    (imported.clone(), "exact")
                } else if let Some((prefix, rest)) = callee_raw.split_once('.') {
                    if let Some(imported_mod) = ctx.import_map.get(prefix) {
                        (format!("{}.{}", imported_mod, rest), "exact")
                    } else {
                        (callee_raw.clone(), "heuristic")
                    }
                } else {
                    (callee_raw.clone(), "exact")
                };

                ctx.edges.push(GraphEdge {
                    id: format!("{}:call:{}:{}:{}", ctx.file_id, line, node.start_position().column, dst_id),
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
                    evidence: call_snippet,
                });
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk_module(ctx, child, current_class_id, current_scope_id, &[]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_python_extractor_classes_methods_imports_calls_and_constants() {
        let extractor = PythonExtractor::new();
        let src = r#"
import rclpy
from rclpy.node import Node as RosNode

SYSTEM_PROMPT = "You are a helpful robot assistant."

class VoiceAgentNode(RosNode):
    """Handles voice commands and dispatches LLM calls."""

    def __init__(self):
        super().__init__("voice_agent")
        self.setup_pipeline()

    def setup_pipeline(self) -> bool:
        """Initializes pub/sub."""
        return helper_func(42)

def helper_func(val: int) -> bool:
    return val > 0
"#;
        let extracted = extractor
            .extract("test-proj", Path::new("src/agent/voice_node.py"), src)
            .unwrap();

        // Check symbols: SYSTEM_PROMPT (constant), VoiceAgentNode (class), __init__ (method), setup_pipeline (method), helper_func (function)
        let names: Vec<&str> = extracted.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"SYSTEM_PROMPT"));
        assert!(names.contains(&"VoiceAgentNode"));
        assert!(names.contains(&"__init__"));
        assert!(names.contains(&"setup_pipeline"));
        assert!(names.contains(&"helper_func"));

        let cls = extracted
            .symbols
            .iter()
            .find(|s| s.name == "VoiceAgentNode")
            .unwrap();
        assert_eq!(cls.kind, "class");
        assert_eq!(
            cls.docstring.as_deref(),
            Some("Handles voice commands and dispatches LLM calls.")
        );

        let setup_m = extracted
            .symbols
            .iter()
            .find(|s| s.name == "setup_pipeline")
            .unwrap();
        assert_eq!(setup_m.kind, "method");
        assert_eq!(setup_m.parent_id.as_deref(), Some(cls.id.as_str()));

        // Check inherits edge
        let inherits: Vec<&GraphEdge> = extracted
            .edges
            .iter()
            .filter(|e| e.kind == "inherits")
            .collect();
        assert_eq!(inherits.len(), 1);
        assert_eq!(inherits[0].dst_id, "rclpy.node.Node");

        // Check imports edge
        let imports: Vec<&GraphEdge> = extracted
            .edges
            .iter()
            .filter(|e| e.kind == "imports")
            .collect();
        assert_eq!(imports.len(), 2);

        // Check calls edge from __init__ to setup_pipeline and setup_pipeline to helper_func
        let calls: Vec<&GraphEdge> = extracted
            .edges
            .iter()
            .filter(|e| e.kind == "calls")
            .collect();
        assert!(calls.iter().any(|c| c.dst_id == setup_m.id && c.confidence == "exact"));
    }

    #[test]
    fn test_python_extractor_fault_tolerance_on_syntax_error() {
        let extractor = PythonExtractor::new();
        let broken_src = r#"
def valid_before():
    return 1

def broken_syntax(x, :
    pass

class StillExtracted:
    def ok_method(self):
        return valid_before()
"#;
        let res = extractor.extract("test-proj", Path::new("broken.py"), broken_src);
        assert!(res.is_ok());
        let extracted = res.unwrap();
        let names: Vec<&str> = extracted.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"valid_before"));
        assert!(names.contains(&"StillExtracted"));
    }
}
