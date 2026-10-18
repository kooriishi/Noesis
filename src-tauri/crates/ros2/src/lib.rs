use noesis_indexer::{DomainAdapter, GraphBatch, IndexerError};
use noesis_project::{is_sensitive_path, normalize_path};
use noesis_store::{DbStore, GraphEdge, GraphEntity, GraphModule};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ros2PackageInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub build_type: String,
    pub root_path: String,
    pub dependencies: Vec<String>,
    pub executables: Vec<Ros2ExecutableTarget>,
    pub interfaces: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ros2ExecutableTarget {
    pub name: String,
    pub sources: Vec<String>,
    pub entry_symbol: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ros2InterfaceField {
    pub field_type: String,
    pub name: String,
    pub default_or_const: Option<String>,
    pub is_constant: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ros2InterfaceDef {
    pub package: String,
    pub kind: String, // "msg" | "srv" | "action"
    pub name: String,
    pub full_name: String,
    pub file_path: String,
    pub fields: Vec<Ros2InterfaceField>,
    pub response_fields: Vec<Ros2InterfaceField>,
    pub feedback_fields: Vec<Ros2InterfaceField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ros2Endpoint {
    pub node_name: String,
    pub node_file: String,
    pub kind: String, // "publishes" | "subscribes" | "serves" | "clients" | "action_serves" | "action_clients" | "declares_param" | "timer"
    pub target_name: String,
    pub msg_or_srv_type: String,
    pub callback: Option<String>,
    pub line: u32,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ros2NodeInfo {
    pub name: String,
    pub class_or_fn: String,
    pub package: String,
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub endpoints: Vec<Ros2Endpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ros2LaunchInfo {
    pub name: String,
    pub package: String,
    pub file_path: String,
    pub launched_nodes: Vec<String>,
    pub included_launches: Vec<String>,
    pub declared_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrdfElementInfo {
    pub kind: String, // "link" | "joint"
    pub name: String,
    pub joint_type: Option<String>,
    pub parent_link: Option<String>,
    pub child_link: Option<String>,
    pub file_path: String,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Ros2WorkspaceSummary {
    pub packages: Vec<Ros2PackageInfo>,
    pub interfaces: Vec<Ros2InterfaceDef>,
    pub nodes: Vec<Ros2NodeInfo>,
    pub launches: Vec<Ros2LaunchInfo>,
    pub urdf_elements: Vec<UrdfElementInfo>,
    pub dangling_topics: Vec<String>,
}

pub struct Ros2Adapter;

impl Ros2Adapter {
    pub fn new() -> Self {
        Self
    }

    pub fn analyze_workspace(
        &self,
        project_id: &str,
        project_root: &Path,
    ) -> Result<(GraphBatch, Ros2WorkspaceSummary), IndexerError> {
        let canonical_root =
            fs::canonicalize(project_root).unwrap_or_else(|_| project_root.to_path_buf());

        let mut batch = GraphBatch::default();
        let mut summary = Ros2WorkspaceSummary::default();

        // 1. Discover all ROS2 packages (by `package.xml`)
        let pkg_dirs = find_package_dirs(&canonical_root);
        let mut pkg_by_dir: Vec<(PathBuf, String)> = Vec::new();
        let mut yaml_topic_params: HashMap<String, String> = HashMap::new();

        // Pre-scan YAML files for parameter key -> topic string mappings (e.g. `playback_topic: "/audio/playback"`)
        collect_yaml_parameters(&canonical_root, &mut yaml_topic_params);

        for pkg_dir in &pkg_dirs {
            let pkg_xml_path = pkg_dir.join("package.xml");
            let rel_pkg_dir = pkg_dir
                .strip_prefix(&canonical_root)
                .map(|p| normalize_path(&p.to_string_lossy()))
                .unwrap_or_else(|_| normalize_path(&pkg_dir.to_string_lossy()));

            if let Ok(xml_content) = fs::read_to_string(&pkg_xml_path) {
                let mut pkg_info = parse_package_xml(&xml_content, &rel_pkg_dir);

                // Parse CMakeLists.txt if present
                let cmake_path = pkg_dir.join("CMakeLists.txt");
                if cmake_path.exists() {
                    if let Ok(cmake_text) = fs::read_to_string(&cmake_path) {
                        let (execs, idls) = parse_cmakelists(&cmake_text, &rel_pkg_dir);
                        pkg_info.executables.extend(execs);
                        pkg_info.interfaces.extend(idls);
                    }
                }

                // Parse setup.py if present
                let setup_py_path = pkg_dir.join("setup.py");
                if setup_py_path.exists() {
                    if let Ok(setup_text) = fs::read_to_string(&setup_py_path) {
                        let py_execs = parse_setup_py(&setup_text, &rel_pkg_dir);
                        pkg_info.executables.extend(py_execs);
                    }
                }

                pkg_by_dir.push((pkg_dir.clone(), pkg_info.name.clone()));

                let mod_id = format!("{}:ros2_pkg:{}", project_id, pkg_info.name);
                let entry_files: Vec<String> = pkg_info
                    .executables
                    .iter()
                    .flat_map(|e| e.sources.clone())
                    .collect();

                batch.modules.push(GraphModule {
                    id: mod_id.clone(),
                    project_id: project_id.to_string(),
                    name: pkg_info.name.clone(),
                    kind: "ros2_package".to_string(),
                    root_path: rel_pkg_dir.clone(),
                    entry_files_json: serde_json::to_string(&entry_files).unwrap_or_default(),
                    metadata_json: serde_json::to_string(&pkg_info).unwrap_or_default(),
                });

                for dep in &pkg_info.dependencies {
                    batch.edges.push(GraphEdge {
                        id: format!("{}:pkg_dep:{}:{}", project_id, pkg_info.name, dep),
                        project_id: project_id.to_string(),
                        src_type: "module".to_string(),
                        src_id: mod_id.clone(),
                        dst_type: "module".to_string(),
                        dst_id: format!("{}:ros2_pkg:{}", project_id, dep),
                        kind: "imports".to_string(),
                        file_id: format!("{}:{}/package.xml", project_id, rel_pkg_dir),
                        file_path: if rel_pkg_dir.is_empty() {
                            "package.xml".to_string()
                        } else {
                            format!("{}/package.xml", rel_pkg_dir)
                        },
                        line: 1,
                        confidence: "exact".to_string(),
                        evidence: format!("<depend>{}</depend>", dep),
                    });
                }

                summary.packages.push(pkg_info);
            }
        }

        // Sort package directories longest-first so nested paths match their most specific package
        pkg_by_dir.sort_by(|a, b| b.0.as_os_str().len().cmp(&a.0.as_os_str().len()));

        // 2. Scan workspace files for .msg/.srv/.action, .urdf/.xacro, launch files, and C++/Python nodes
        for entry in WalkDir::new(&canonical_root)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !matches!(
                    name.as_ref(),
                    ".git" | "node_modules" | "target" | "dist" | "build" | "install" | "log" | "__pycache__"
                )
            })
            .flatten()
        {
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

            let rel_str = normalize_path(&rel_path.to_string_lossy());
            let file_id = format!("{}:{}", project_id, rel_str);
            let file_name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let lower_name = file_name.to_lowercase();

            let pkg_name = pkg_by_dir
                .iter()
                .find(|(pdir, _)| path.starts_with(pdir))
                .map(|(_, name)| name.clone())
                .unwrap_or_else(|| "workspace".to_string());

            // 2a. ROS2 Interface definitions (.msg, .srv, .action)
            if lower_name.ends_with(".msg")
                || lower_name.ends_with(".srv")
                || lower_name.ends_with(".action")
            {
                if let Ok(content) = fs::read_to_string(path) {
                    let iface = parse_interface_file(&pkg_name, &file_name, &rel_str, &content);
                    let ent_kind = match iface.kind.as_str() {
                        "msg" => "ros2_interface_msg",
                        "srv" => "ros2_interface_srv",
                        _ => "ros2_interface_action",
                    };
                    let line_count = content.lines().count().max(1) as u32;
                    batch.entities.push(GraphEntity {
                        id: format!("{}:ros2_iface:{}", project_id, iface.full_name),
                        project_id: project_id.to_string(),
                        domain: "ros2".to_string(),
                        kind: ent_kind.to_string(),
                        name: iface.full_name.clone(),
                        file_id: file_id.clone(),
                        file_path: rel_str.clone(),
                        symbol_id: None,
                        line_start: 1,
                        line_end: line_count,
                        metadata_json: serde_json::to_string(&iface).unwrap_or_default(),
                    });
                    summary.interfaces.push(iface);
                }
                continue;
            }

            // 2b. URDF / Xacro files
            if lower_name.ends_with(".urdf") || lower_name.ends_with(".xacro") {
                if let Ok(content) = fs::read_to_string(path) {
                    let elements = parse_urdf_file(&rel_str, &content);
                    for el in elements {
                        let ent_kind = if el.kind == "link" {
                            "urdf_link"
                        } else {
                            "urdf_joint"
                        };
                        let ent_id = format!("{}:{}:{}", project_id, ent_kind, el.name);
                        batch.entities.push(GraphEntity {
                            id: ent_id.clone(),
                            project_id: project_id.to_string(),
                            domain: "ros2".to_string(),
                            kind: ent_kind.to_string(),
                            name: el.name.clone(),
                            file_id: file_id.clone(),
                            file_path: rel_str.clone(),
                            symbol_id: None,
                            line_start: el.line,
                            line_end: el.line,
                            metadata_json: serde_json::to_string(&el).unwrap_or_default(),
                        });
                        if let (Some(parent), Some(child)) = (&el.parent_link, &el.child_link) {
                            batch.edges.push(GraphEdge {
                                id: format!("{}:urdf_joint:{}:{}:{}", project_id, el.name, parent, child),
                                project_id: project_id.to_string(),
                                src_type: "entity".to_string(),
                                src_id: format!("{}:urdf_link:{}", project_id, parent),
                                dst_type: "entity".to_string(),
                                dst_id: format!("{}:urdf_link:{}", project_id, child),
                                kind: "routes_to".to_string(),
                                file_id: file_id.clone(),
                                file_path: rel_str.clone(),
                                line: el.line,
                                confidence: "exact".to_string(),
                                evidence: format!("joint {} ({} -> {})", el.name, parent, child),
                            });
                        }
                        summary.urdf_elements.push(el);
                    }
                }
                continue;
            }

            // 2c. Launch files (.launch.py, .launch.xml, .launch.yaml)
            if lower_name.ends_with(".launch.py")
                || lower_name.ends_with(".launch.xml")
                || lower_name.ends_with(".launch.yaml")
                || (rel_str.contains("/launch/") && (lower_name.ends_with(".py") || lower_name.ends_with(".xml") || lower_name.ends_with(".yaml")))
            {
                if let Ok(content) = fs::read_to_string(path) {
                    let launch_info = parse_launch_file(&pkg_name, &file_name, &rel_str, &content);
                    let launch_ent_id = format!("{}:ros2_launch:{}", project_id, rel_str);
                    let line_count = content.lines().count().max(1) as u32;
                    batch.entities.push(GraphEntity {
                        id: launch_ent_id.clone(),
                        project_id: project_id.to_string(),
                        domain: "ros2".to_string(),
                        kind: "ros2_launch".to_string(),
                        name: launch_info.name.clone(),
                        file_id: file_id.clone(),
                        file_path: rel_str.clone(),
                        symbol_id: None,
                        line_start: 1,
                        line_end: line_count,
                        metadata_json: serde_json::to_string(&launch_info).unwrap_or_default(),
                    });

                    for node_target in &launch_info.launched_nodes {
                        batch.edges.push(GraphEdge {
                            id: format!("{}:launches:{}:{}", project_id, rel_str, node_target),
                            project_id: project_id.to_string(),
                            src_type: "entity".to_string(),
                            src_id: launch_ent_id.clone(),
                            dst_type: "entity".to_string(),
                            dst_id: format!("{}:ros2_node:{}", project_id, node_target),
                            kind: "launches".to_string(),
                            file_id: file_id.clone(),
                            file_path: rel_str.clone(),
                            line: 1,
                            confidence: "exact".to_string(),
                            evidence: format!("Node({})", node_target),
                        });
                    }

                    for inc in &launch_info.included_launches {
                        batch.edges.push(GraphEdge {
                            id: format!("{}:includes_launch:{}:{}", project_id, rel_str, inc),
                            project_id: project_id.to_string(),
                            src_type: "entity".to_string(),
                            src_id: launch_ent_id.clone(),
                            dst_type: "entity".to_string(),
                            dst_id: format!("{}:ros2_launch:{}", project_id, inc),
                            kind: "includes_launch".to_string(),
                            file_id: file_id.clone(),
                            file_path: rel_str.clone(),
                            line: 1,
                            confidence: "exact".to_string(),
                            evidence: format!("IncludeLaunchDescription({})", inc),
                        });
                    }

                    summary.launches.push(launch_info);
                }
                continue;
            }

            // 2d. C++ / Python ROS2 Nodes & Communication Endpoints
            if lower_name.ends_with(".cpp")
                || lower_name.ends_with(".hpp")
                || lower_name.ends_with(".cc")
                || lower_name.ends_with(".py")
            {
                if let Ok(content) = fs::read_to_string(path) {
                    let nodes = extract_ros2_nodes_and_endpoints(
                        &pkg_name,
                        &rel_str,
                        &content,
                        &yaml_topic_params,
                    );
                    for node_info in nodes {
                        let node_ent_id = format!("{}:ros2_node:{}", project_id, node_info.name);
                        batch.entities.push(GraphEntity {
                            id: node_ent_id.clone(),
                            project_id: project_id.to_string(),
                            domain: "ros2".to_string(),
                            kind: "ros2_node".to_string(),
                            name: node_info.name.clone(),
                            file_id: file_id.clone(),
                            file_path: rel_str.clone(),
                            symbol_id: Some(node_info.class_or_fn.clone()),
                            line_start: node_info.line_start,
                            line_end: node_info.line_end,
                            metadata_json: serde_json::to_string(&node_info).unwrap_or_default(),
                        });

                        for ep in &node_info.endpoints {
                            let (target_ent_kind, is_comm_edge) = match ep.kind.as_str() {
                                "publishes" | "subscribes" => ("ros2_topic", true),
                                "serves" | "clients" => ("ros2_service", true),
                                "action_serves" | "action_clients" => ("ros2_action", true),
                                "declares_param" => ("ros2_param", true),
                                _ => ("ros2_timer", false),
                            };

                            let target_ent_id = format!(
                                "{}:{}:{}",
                                project_id, target_ent_kind, ep.target_name
                            );

                            if !batch.entities.iter().any(|e| e.id == target_ent_id) {
                                batch.entities.push(GraphEntity {
                                    id: target_ent_id.clone(),
                                    project_id: project_id.to_string(),
                                    domain: "ros2".to_string(),
                                    kind: target_ent_kind.to_string(),
                                    name: ep.target_name.clone(),
                                    file_id: file_id.clone(),
                                    file_path: rel_str.clone(),
                                    symbol_id: ep.callback.clone(),
                                    line_start: ep.line,
                                    line_end: ep.line,
                                    metadata_json: json!({
                                        "type": ep.msg_or_srv_type,
                                        "confidence": ep.confidence,
                                    })
                                    .to_string(),
                                });
                            }

                            if is_comm_edge {
                                batch.edges.push(GraphEdge {
                                    id: format!(
                                        "{}:{}:{}:{}:{}",
                                        project_id, ep.kind, node_info.name, ep.target_name, ep.line
                                    ),
                                    project_id: project_id.to_string(),
                                    src_type: "entity".to_string(),
                                    src_id: node_ent_id.clone(),
                                    dst_type: "entity".to_string(),
                                    dst_id: target_ent_id,
                                    kind: ep.kind.clone(),
                                    file_id: file_id.clone(),
                                    file_path: rel_str.clone(),
                                    line: ep.line,
                                    confidence: ep.confidence.clone(),
                                    evidence: format!(
                                        "{} ({})",
                                        ep.target_name, ep.msg_or_srv_type
                                    ),
                                });
                            }
                        }

                        summary.nodes.push(node_info);
                    }
                }
            }
        }

        // Compute dangling topics (published but never subscribed, or subscribed but never published)
        let mut pub_topics = HashSet::new();
        let mut sub_topics = HashSet::new();
        for n in &summary.nodes {
            for ep in &n.endpoints {
                if ep.kind == "publishes" {
                    pub_topics.insert(ep.target_name.clone());
                } else if ep.kind == "subscribes" {
                    sub_topics.insert(ep.target_name.clone());
                }
            }
        }
        let all_topics: HashSet<String> = pub_topics.union(&sub_topics).cloned().collect();
        for t in all_topics {
            if !pub_topics.contains(&t) || !sub_topics.contains(&t) {
                summary.dangling_topics.push(t);
            }
        }
        summary.dangling_topics.sort();

        Ok((batch, summary))
    }
}

impl Default for Ros2Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl DomainAdapter for Ros2Adapter {
    fn domain_id(&self) -> &'static str {
        "ros2"
    }

    fn detect(&self, project_root: &Path) -> bool {
        !find_package_dirs(project_root).is_empty()
    }

    fn analyze_project(
        &self,
        project_id: &str,
        project_root: &Path,
        _store: &DbStore,
    ) -> Result<GraphBatch, IndexerError> {
        let (batch, _summary) = self.analyze_workspace(project_id, project_root)?;
        Ok(batch)
    }
}

fn find_package_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for entry in WalkDir::new(root)
        .max_depth(5)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !matches!(
                name.as_ref(),
                ".git" | "node_modules" | "target" | "dist" | "build" | "install" | "log"
            )
        })
        .flatten()
    {
        if entry.file_type().is_file() && entry.file_name() == "package.xml" {
            if let Some(parent) = entry.path().parent() {
                dirs.push(parent.to_path_buf());
            }
        }
    }
    dirs
}

fn collect_yaml_parameters(root: &Path, map: &mut HashMap<String, String>) {
    let kv_re = Regex::new(r#"^\s*([a-zA-Z0-9_]+)\s*:\s*["']?(/[^"'\s#]+)["']?"#).unwrap();
    for entry in WalkDir::new(root)
        .max_depth(6)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !matches!(
                name.as_ref(),
                ".git" | "node_modules" | "target" | "dist" | "build" | "install" | "log"
            )
        })
        .flatten()
    {
        let p = entry.path();
        if p.is_file() {
            let ext = p
                .extension()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if ext == "yaml" || ext == "yml" {
                if let Ok(text) = fs::read_to_string(p) {
                    for line in text.lines() {
                        if let Some(caps) = kv_re.captures(line) {
                            let key = caps[1].to_string();
                            let val = caps[2].to_string();
                            map.entry(key).or_insert(val);
                        }
                    }
                }
            }
        }
    }
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let pattern = format!(r"(?s)<{0}[^>]*>(.*?)</{0}>", regex::escape(tag));
    let re = Regex::new(&pattern).ok()?;
    re.captures(xml).map(|c| c[1].trim().to_string())
}

fn extract_all_xml_tags(xml: &str, tag: &str) -> Vec<String> {
    let pattern = format!(r"(?s)<{0}[^>]*>(.*?)</{0}>", regex::escape(tag));
    let re = match Regex::new(&pattern) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    re.captures_iter(xml)
        .map(|c| c[1].trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn parse_package_xml(xml: &str, rel_pkg_dir: &str) -> Ros2PackageInfo {
    let name = extract_xml_tag(xml, "name").unwrap_or_else(|| "unknown_package".to_string());
    let version = extract_xml_tag(xml, "version").unwrap_or_else(|| "0.0.0".to_string());
    let description = extract_xml_tag(xml, "description").unwrap_or_default();
    let build_type = extract_xml_tag(xml, "build_type").unwrap_or_else(|| "ament_cmake".to_string());

    let mut deps_set = HashSet::new();
    for tag in ["depend", "build_depend", "exec_depend", "run_depend"] {
        for d in extract_all_xml_tags(xml, tag) {
            deps_set.insert(d);
        }
    }
    let mut dependencies: Vec<String> = deps_set.into_iter().collect();
    dependencies.sort();

    Ros2PackageInfo {
        name,
        version,
        description,
        build_type,
        root_path: rel_pkg_dir.to_string(),
        dependencies,
        executables: Vec::new(),
        interfaces: Vec::new(),
    }
}

pub fn parse_cmakelists(
    cmake_text: &str,
    rel_pkg_dir: &str,
) -> (Vec<Ros2ExecutableTarget>, Vec<String>) {
    let mut executables = Vec::new();
    let mut interfaces = Vec::new();

    let add_exec_re = Regex::new(r"(?s)add_executable\s*\(\s*([^\s\)]+)\s+([^\)]+)\)").unwrap();
    for cap in add_exec_re.captures_iter(cmake_text) {
        let target_name = cap[1].trim().to_string();
        let srcs_raw = &cap[2];
        let sources: Vec<String> = srcs_raw
            .split_whitespace()
            .filter(|s| !s.starts_with('#') && !s.starts_with('$'))
            .map(|s| {
                if rel_pkg_dir.is_empty() {
                    s.to_string()
                } else {
                    format!("{}/{}", rel_pkg_dir, s)
                }
            })
            .collect();
        executables.push(Ros2ExecutableTarget {
            name: target_name,
            sources,
            entry_symbol: Some("main".to_string()),
        });
    }

    let rosidl_re =
        Regex::new(r"(?s)rosidl_generate_interfaces\s*\(\s*[^\s\)]+\s+([^\)]+)\)").unwrap();
    for cap in rosidl_re.captures_iter(cmake_text) {
        let body = &cap[1];
        for tok in body.split_whitespace() {
            let clean = tok.trim_matches('"').trim_matches('\'');
            if clean.ends_with(".msg") || clean.ends_with(".srv") || clean.ends_with(".action") {
                interfaces.push(clean.to_string());
            }
        }
    }

    (executables, interfaces)
}

pub fn parse_setup_py(setup_text: &str, rel_pkg_dir: &str) -> Vec<Ros2ExecutableTarget> {
    let mut executables = Vec::new();
    let script_re =
        Regex::new(r#"['"]([a-zA-Z0-9_\-]+)\s*=\s*([a-zA-Z0-9_\.]+):([a-zA-Z0-9_]+)['"]"#)
            .unwrap();
    for cap in script_re.captures_iter(setup_text) {
        let exec_name = cap[1].to_string();
        let mod_path = cap[2].replace('.', "/") + ".py";
        let func_name = cap[3].to_string();
        let full_src = if rel_pkg_dir.is_empty() {
            mod_path
        } else {
            format!("{}/{}", rel_pkg_dir, mod_path)
        };
        executables.push(Ros2ExecutableTarget {
            name: exec_name,
            sources: vec![full_src],
            entry_symbol: Some(func_name),
        });
    }
    executables
}

pub fn parse_interface_file(
    pkg_name: &str,
    file_name: &str,
    rel_path: &str,
    content: &str,
) -> Ros2InterfaceDef {
    let (name, kind) = if let Some(n) = file_name.strip_suffix(".msg") {
        (n.to_string(), "msg".to_string())
    } else if let Some(n) = file_name.strip_suffix(".srv") {
        (n.to_string(), "srv".to_string())
    } else if let Some(n) = file_name.strip_suffix(".action") {
        (n.to_string(), "action".to_string())
    } else {
        (file_name.to_string(), "msg".to_string())
    };

    let mut sections: Vec<Vec<Ros2InterfaceField>> = vec![Vec::new()];
    for raw_line in content.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line == "---" {
            sections.push(Vec::new());
            continue;
        }

        let current_sec = sections.last_mut().unwrap();
        if let Some((left, const_val)) = line.split_once('=') {
            let parts: Vec<&str> = left.split_whitespace().collect();
            if parts.len() >= 2 {
                current_sec.push(Ros2InterfaceField {
                    field_type: parts[0].to_string(),
                    name: parts[1].to_string(),
                    default_or_const: Some(const_val.trim().to_string()),
                    is_constant: true,
                });
            }
        } else {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let default_val = if parts.len() > 2 {
                    Some(parts[2..].join(" "))
                } else {
                    None
                };
                current_sec.push(Ros2InterfaceField {
                    field_type: parts[0].to_string(),
                    name: parts[1].to_string(),
                    default_or_const: default_val,
                    is_constant: false,
                });
            }
        }
    }

    let fields = sections.first().cloned().unwrap_or_default();
    let response_fields = sections.get(1).cloned().unwrap_or_default();
    let feedback_fields = sections.get(2).cloned().unwrap_or_default();

    Ros2InterfaceDef {
        package: pkg_name.to_string(),
        kind: kind.clone(),
        name: name.clone(),
        full_name: format!("{}/{}/{}", pkg_name, kind, name),
        file_path: rel_path.to_string(),
        fields,
        response_fields,
        feedback_fields,
    }
}

pub fn parse_urdf_file(rel_path: &str, content: &str) -> Vec<UrdfElementInfo> {
    let mut elements = Vec::new();
    let link_re = Regex::new(r#"<link\s+[^>]*name\s*=\s*["']([^"']+)["']"#).unwrap();
    let joint_re = Regex::new(
        r#"(?s)<joint\s+[^>]*name\s*=\s*["']([^"']+)["'][^>]*type\s*=\s*["']([^"']+)["'][^>]*>(.*?)</joint>"#,
    )
    .unwrap();
    let parent_re = Regex::new(r#"<parent\s+[^>]*link\s*=\s*["']([^"']+)["']"#).unwrap();
    let child_re = Regex::new(r#"<child\s+[^>]*link\s*=\s*["']([^"']+)["']"#).unwrap();

    for cap in link_re.captures_iter(content) {
        let byte_idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let line = content[..byte_idx].lines().count().max(1) as u32;
        elements.push(UrdfElementInfo {
            kind: "link".to_string(),
            name: cap[1].to_string(),
            joint_type: None,
            parent_link: None,
            child_link: None,
            file_path: rel_path.to_string(),
            line,
        });
    }

    for cap in joint_re.captures_iter(content) {
        let byte_idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let line = content[..byte_idx].lines().count().max(1) as u32;
        let body = &cap[3];
        let parent_link = parent_re.captures(body).map(|c| c[1].to_string());
        let child_link = child_re.captures(body).map(|c| c[1].to_string());
        elements.push(UrdfElementInfo {
            kind: "joint".to_string(),
            name: cap[1].to_string(),
            joint_type: Some(cap[2].to_string()),
            parent_link,
            child_link,
            file_path: rel_path.to_string(),
            line,
        });
    }

    elements
}

pub fn parse_launch_file(
    pkg_name: &str,
    file_name: &str,
    rel_path: &str,
    content: &str,
) -> Ros2LaunchInfo {
    let mut launched_nodes = Vec::new();
    let mut included_launches = Vec::new();
    let mut declared_args = Vec::new();

    // Python launch: Node(package="...", executable="...", name="...")
    let py_node_re = Regex::new(r"(?s)\bNode\s*\((.*?)\n\s*\)").unwrap();
    let pkg_kv_re = Regex::new(r#"package\s*=\s*['"]([^'"]+)['"]"#).unwrap();
    let exec_kv_re = Regex::new(r#"executable\s*=\s*['"]([^'"]+)['"]"#).unwrap();
    let name_kv_re = Regex::new(r#"name\s*=\s*['"]([^'"]+)['"]"#).unwrap();

    for cap in py_node_re.captures_iter(content) {
        let body = &cap[1];
        let node_name = name_kv_re
            .captures(body)
            .or_else(|| exec_kv_re.captures(body))
            .or_else(|| pkg_kv_re.captures(body))
            .map(|c| c[1].to_string());
        if let Some(n) = node_name {
            if !launched_nodes.contains(&n) {
                launched_nodes.push(n);
            }
        }
    }

    // Included launch files: e.g. "minicpm.launch.py", "cascade.launch.py", "audio.launch.py"
    let inc_re = Regex::new(r#"['"]([a-zA-Z0-9_\-/]+\.launch\.(?:py|xml|yaml))['"]"#).unwrap();
    for cap in inc_re.captures_iter(content) {
        let inc = cap[1].to_string();
        if inc != file_name && !included_launches.contains(&inc) {
            included_launches.push(inc);
        }
    }

    // DeclareLaunchArgument("arg_name", ...)
    let arg_re = Regex::new(r#"DeclareLaunchArgument\s*\(\s*['"]([^'"]+)['"]"#).unwrap();
    for cap in arg_re.captures_iter(content) {
        let arg = cap[1].to_string();
        if !declared_args.contains(&arg) {
            declared_args.push(arg);
        }
    }

    Ros2LaunchInfo {
        name: file_name.to_string(),
        package: pkg_name.to_string(),
        file_path: rel_path.to_string(),
        launched_nodes,
        included_launches,
        declared_args,
    }
}

pub fn extract_ros2_nodes_and_endpoints(
    pkg_name: &str,
    rel_path: &str,
    content: &str,
    yaml_topic_params: &HashMap<String, String>,
) -> Vec<Ros2NodeInfo> {
    let mut nodes = Vec::new();

    // Detect C++ class inheriting rclcpp::Node or LifecycleNode
    let cpp_class_re = Regex::new(
        r"(?s)class\s+([a-zA-Z0-9_]+)\s*(?:final)?\s*:\s*public\s+(?:rclcpp::Node|rclcpp_lifecycle::LifecycleNode)",
    )
    .unwrap();
    // Detect Python class inheriting Node
    let py_class_re =
        Regex::new(r"class\s+([a-zA-Z0-9_]+)\s*\(\s*(?:rclpy\.node\.)?(?:Node|RosNode)\s*\)")
            .unwrap();

    // Node name literal in constructor `: Node("voice_orchestrator")` or `super().__init__("node_name")`
    let cpp_node_init_re = Regex::new(r#":\s*(?:rclcpp::)?Node\s*\(\s*["']([^"']+)["']"#).unwrap();
    let py_node_init_re =
        Regex::new(r#"super\(\)\.__init__\s*\(\s*["']([^"']+)["']"#).unwrap();

    let mut class_name = None;
    let mut line_start = 1u32;
    if let Some(cap) = cpp_class_re.captures(content) {
        class_name = Some(cap[1].to_string());
        let idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
        line_start = content[..idx].lines().count().max(1) as u32;
    } else if let Some(cap) = py_class_re.captures(content) {
        class_name = Some(cap[1].to_string());
        let idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
        line_start = content[..idx].lines().count().max(1) as u32;
    }

    let Some(cls_name) = class_name else {
        return nodes;
    };

    let node_name = cpp_node_init_re
        .captures(content)
        .or_else(|| py_node_init_re.captures(content))
        .map(|c| c[1].to_string())
        .unwrap_or_else(|| cls_name.clone());

    let line_end = content.lines().count().max(1) as u32;
    let mut endpoints = Vec::new();

    // Helper to resolve topic argument (either `"literal"` or `config_.ros.playback_topic` resolved via YAML)
    let resolve_arg = |raw_arg: &str| -> (String, String) {
        let trimmed = raw_arg.trim();
        if (trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        {
            return (
                trimmed[1..trimmed.len() - 1].to_string(),
                "exact".to_string(),
            );
        }
        // Check if expression ends with `.field_name` that matches a YAML topic parameter
        let field_key = trimmed.rsplit('.').next().unwrap_or(trimmed).trim();
        if let Some(resolved_topic) = yaml_topic_params.get(field_key) {
            return (resolved_topic.clone(), "exact".to_string());
        }
        (trimmed.to_string(), "heuristic".to_string())
    };

    // 1. C++ create_publisher<MsgT>(topic, qos)
    let cpp_pub_re =
        Regex::new(r"(?s)create_publisher\s*<\s*([^>]+)\s*>\s*\(\s*([^,\)]+)").unwrap();
    for cap in cpp_pub_re.captures_iter(content) {
        let idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let line = content[..idx].lines().count().max(1) as u32;
        let msg_type = cap[1].trim().to_string();
        let (topic_name, confidence) = resolve_arg(&cap[2]);
        endpoints.push(Ros2Endpoint {
            node_name: node_name.clone(),
            node_file: rel_path.to_string(),
            kind: "publishes".to_string(),
            target_name: topic_name,
            msg_or_srv_type: msg_type,
            callback: None,
            line,
            confidence,
        });
    }

    // 2. C++ create_subscription<MsgT>(topic, qos, callback)
    let cpp_sub_re = Regex::new(
        r"(?s)create_subscription\s*<\s*([^>]+)\s*>\s*\(\s*([^,]+)\s*,\s*([^,]+)\s*,\s*([^\)]+)\)",
    )
    .unwrap();
    for cap in cpp_sub_re.captures_iter(content) {
        let idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let line = content[..idx].lines().count().max(1) as u32;
        let msg_type = cap[1].trim().to_string();
        let (topic_name, confidence) = resolve_arg(&cap[2]);
        let cb_raw: String = cap[4]
            .lines()
            .map(|l| l.trim())
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(80)
            .collect();
        endpoints.push(Ros2Endpoint {
            node_name: node_name.clone(),
            node_file: rel_path.to_string(),
            kind: "subscribes".to_string(),
            target_name: topic_name,
            msg_or_srv_type: msg_type,
            callback: Some(cb_raw),
            line,
            confidence,
        });
    }

    // 3. C++ create_service<SrvT>(service_name, callback)
    let cpp_srv_re =
        Regex::new(r"(?s)create_service\s*<\s*([^>]+)\s*>\s*\(\s*([^,]+)\s*,\s*([^\)]+)\)")
            .unwrap();
    for cap in cpp_srv_re.captures_iter(content) {
        let idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let line = content[..idx].lines().count().max(1) as u32;
        let srv_type = cap[1].trim().to_string();
        let (srv_name, confidence) = resolve_arg(&cap[2]);
        let cb_raw: String = cap[3]
            .lines()
            .map(|l| l.trim())
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(80)
            .collect();
        endpoints.push(Ros2Endpoint {
            node_name: node_name.clone(),
            node_file: rel_path.to_string(),
            kind: "serves".to_string(),
            target_name: srv_name,
            msg_or_srv_type: srv_type,
            callback: Some(cb_raw),
            line,
            confidence,
        });
    }

    // 4. C++ create_client<SrvT>(service_name)
    let cpp_cli_re =
        Regex::new(r"(?s)create_client\s*<\s*([^>]+)\s*>\s*\(\s*([^,\)]+)").unwrap();
    for cap in cpp_cli_re.captures_iter(content) {
        let idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let line = content[..idx].lines().count().max(1) as u32;
        let srv_type = cap[1].trim().to_string();
        let (srv_name, confidence) = resolve_arg(&cap[2]);
        endpoints.push(Ros2Endpoint {
            node_name: node_name.clone(),
            node_file: rel_path.to_string(),
            kind: "clients".to_string(),
            target_name: srv_name,
            msg_or_srv_type: srv_type,
            callback: None,
            line,
            confidence,
        });
    }

    // 5. Python self.create_publisher(MsgT, topic, qos) & self.create_subscription(MsgT, topic, cb, qos)
    let py_pub_re =
        Regex::new(r"create_publisher\s*\(\s*([^,]+)\s*,\s*([^,]+)\s*,").unwrap();
    for cap in py_pub_re.captures_iter(content) {
        let idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let line = content[..idx].lines().count().max(1) as u32;
        let msg_type = cap[1].trim().to_string();
        let (topic_name, confidence) = resolve_arg(&cap[2]);
        endpoints.push(Ros2Endpoint {
            node_name: node_name.clone(),
            node_file: rel_path.to_string(),
            kind: "publishes".to_string(),
            target_name: topic_name,
            msg_or_srv_type: msg_type,
            callback: None,
            line,
            confidence,
        });
    }

    let py_sub_re =
        Regex::new(r"create_subscription\s*\(\s*([^,]+)\s*,\s*([^,]+)\s*,\s*([^,]+)\s*,").unwrap();
    for cap in py_sub_re.captures_iter(content) {
        let idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let line = content[..idx].lines().count().max(1) as u32;
        let msg_type = cap[1].trim().to_string();
        let (topic_name, confidence) = resolve_arg(&cap[2]);
        endpoints.push(Ros2Endpoint {
            node_name: node_name.clone(),
            node_file: rel_path.to_string(),
            kind: "subscribes".to_string(),
            target_name: topic_name,
            msg_or_srv_type: msg_type,
            callback: Some(cap[3].trim().to_string()),
            line,
            confidence,
        });
    }

    // 6. Parameters: declare_parameter<T>("param_name", ...) or declare_parameter("param_name", ...)
    let param_re = Regex::new(
        r#"declare_parameter(?:\s*<\s*([^>]+)\s*>)?\s*\(\s*["']([^"']+)["']"#,
    )
    .unwrap();
    for cap in param_re.captures_iter(content) {
        let idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let line = content[..idx].lines().count().max(1) as u32;
        let param_type = cap
            .get(1)
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_else(|| "auto".to_string());
        let param_name = cap[2].to_string();
        endpoints.push(Ros2Endpoint {
            node_name: node_name.clone(),
            node_file: rel_path.to_string(),
            kind: "declares_param".to_string(),
            target_name: param_name,
            msg_or_srv_type: param_type,
            callback: None,
            line,
            confidence: "exact".to_string(),
        });
    }

    // 7. Timers: create_wall_timer(period, cb) or self.create_timer(period, cb)
    let timer_re = Regex::new(r"(?:create_wall_timer|create_timer)\s*\(\s*([^,]+)\s*,\s*([^\)]+)\)").unwrap();
    for cap in timer_re.captures_iter(content) {
        let idx = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let line = content[..idx].lines().count().max(1) as u32;
        let period = cap[1].trim().to_string();
        let cb: String = cap[2]
            .lines()
            .map(|l| l.trim())
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(60)
            .collect();
        endpoints.push(Ros2Endpoint {
            node_name: node_name.clone(),
            node_file: rel_path.to_string(),
            kind: "timer".to_string(),
            target_name: format!("{}:timer@L{}", node_name, line),
            msg_or_srv_type: period,
            callback: Some(cb),
            line,
            confidence: "exact".to_string(),
        });
    }

    nodes.push(Ros2NodeInfo {
        name: node_name,
        class_or_fn: cls_name,
        package: pkg_name.to_string(),
        file_path: rel_path.to_string(),
        line_start,
        line_end,
        endpoints,
    });

    nodes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_interface_msg_and_srv() {
        let msg_src = r#"
# Audio frame message
uint32 sample_rate 16000
uint8 CHANNELS_MONO = 1
uint8[] pcm_data
"#;
        let iface = parse_interface_file(
            "robot_voice_interfaces",
            "AudioFrame.msg",
            "src/robot_voice_interfaces/msg/AudioFrame.msg",
            msg_src,
        );
        assert_eq!(iface.full_name, "robot_voice_interfaces/msg/AudioFrame");
        assert_eq!(iface.fields.len(), 3);
        assert!(!iface.fields[0].is_constant);
        assert!(iface.fields[1].is_constant);

        let srv_src = r#"
Correlation target
uint8 mode
string reason
---
bool accepted
string message
"#;
        let srv = parse_interface_file(
            "robot_voice_interfaces",
            "CancelTurn.srv",
            "src/robot_voice_interfaces/srv/CancelTurn.srv",
            srv_src,
        );
        assert_eq!(srv.kind, "srv");
        assert_eq!(srv.fields.len(), 3);
        assert_eq!(srv.response_fields.len(), 2);
    }

    #[test]
    fn test_analyze_robot_voice_ws_sample_project() {
        let sample_path = Path::new(r"C:\Users\123\Desktop\RobotVoiceWS-main");
        if !sample_path.exists() {
            return;
        }
        let adapter = Ros2Adapter::new();
        assert!(adapter.detect(sample_path));

        let (batch, summary) = adapter
            .analyze_workspace("robot_voice_ws", sample_path)
            .unwrap();

        // RobotVoiceWS-main has 6 ROS2 packages!
        assert_eq!(summary.packages.len(), 6);
        // 10 .msg + 2 .srv = 12 interfaces!
        assert_eq!(summary.interfaces.len(), 12);
        // 4 launch.py files!
        assert_eq!(summary.launches.len(), 4);
        // 5 C++ rclcpp nodes (VoiceOrchestratorNode, AudioManagerNode, AudioProcessorNode, CascadeNode, MiniCpmRealtimeNode)!
        assert!(summary.nodes.len() >= 4);
        assert!(!batch.entities.is_empty());
        assert!(!batch.edges.is_empty());
    }
}
