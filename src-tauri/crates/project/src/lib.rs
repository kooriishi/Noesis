use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProjectError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid path: {0}")]
    InvalidPath(String),
    #[error("File too large ({size} bytes, max {max} bytes)")]
    FileTooLarge { size: u64, max: u64 },
    #[error("Non-UTF8 content")]
    NotUtf8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    pub name: String,
    pub path: String, // Normalized relative path from project root
    pub absolute_path: String,
    pub is_dir: bool,
    pub children: Option<Vec<FileNode>>,
    pub size: u64,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContentPayload {
    pub path: String,
    pub content: String,
    pub hash: String,
    pub language: String,
    pub size: u64,
}

pub struct ProjectManager {
    root_path: PathBuf,
}

impl ProjectManager {
    pub fn new<P: AsRef<Path>>(root: P) -> Result<Self, ProjectError> {
        let p = root.as_ref();
        if !p.exists() || !p.is_dir() {
            return Err(ProjectError::InvalidPath(p.to_string_lossy().to_string()));
        }
        let canonical = fs::canonicalize(p)?;
        Ok(Self {
            root_path: canonical,
        })
    }

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    pub fn normalized_root(&self) -> String {
        normalize_path(&self.root_path.to_string_lossy())
    }

    pub fn build_tree(&self, max_depth: usize) -> Result<FileNode, ProjectError> {
        let root_name = self
            .root_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "project".to_string());

        let mut root_node = FileNode {
            name: root_name,
            path: "".to_string(),
            absolute_path: normalize_path(&self.root_path.to_string_lossy()),
            is_dir: true,
            children: Some(Vec::new()),
            size: 0,
            language: None,
        };

        self.populate_children(&self.root_path, &mut root_node, 0, max_depth)?;
        Ok(root_node)
    }

    /// List direct children of a relative directory (supports lazy loading in B5)
    pub fn list_dir<P: AsRef<Path>>(&self, rel_dir: P) -> Result<Vec<FileNode>, ProjectError> {
        let rel = rel_dir.as_ref();
        let target_dir = if rel.as_os_str().is_empty() || rel == Path::new(".") {
            self.root_path.clone()
        } else {
            self.resolve_inside_root(rel)?
        };

        if !target_dir.exists() || !target_dir.is_dir() {
            return Err(ProjectError::InvalidPath(format!(
                "Directory does not exist: {}",
                rel.to_string_lossy()
            )));
        }

        let mut dummy = FileNode {
            name: String::new(),
            path: String::new(),
            absolute_path: String::new(),
            is_dir: true,
            children: Some(Vec::new()),
            size: 0,
            language: None,
        };
        self.populate_children(&target_dir, &mut dummy, 0, 1)?;
        Ok(dummy.children.unwrap_or_default())
    }

    fn populate_children(
        &self,
        current_dir: &Path,
        current_node: &mut FileNode,
        depth: usize,
        max_depth: usize,
    ) -> Result<(), ProjectError> {
        if depth >= max_depth {
            return Ok(());
        }

        let mut children = Vec::new();
        let walker = WalkBuilder::new(current_dir)
            .max_depth(Some(1))
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .hidden(false)
            .filter_entry(|entry| {
                let name = entry.file_name().to_string_lossy();
                if name == ".git"
                    || name == "node_modules"
                    || name == "target"
                    || name == "dist"
                    || name == "__pycache__"
                    || name == ".venv"
                    || name == "build"
                    || name == "install"
                    || name == "log"
                {
                    return false;
                }
                true
            })
            .build();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            if path == current_dir {
                continue;
            }

            let file_name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();

            let is_dir = path.is_dir();
            let abs_path = normalize_path(&path.to_string_lossy());
            let rel_path_buf = path
                .strip_prefix(&self.root_path)
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|_| path.to_path_buf());
            let rel_path = normalize_path(&rel_path_buf.to_string_lossy());

            // Skip sensitive files from project tree listing if they are blocked
            if !is_dir && is_sensitive_path(&rel_path_buf) {
                continue;
            }

            let mut node = FileNode {
                name: file_name.clone(),
                path: rel_path,
                absolute_path: abs_path,
                is_dir,
                children: if is_dir { Some(Vec::new()) } else { None },
                size: if is_dir {
                    0
                } else {
                    entry.metadata().map(|m| m.len()).unwrap_or(0)
                },
                language: if is_dir {
                    None
                } else {
                    Some(detect_language(&file_name))
                },
            };

            if is_dir && depth + 1 < max_depth {
                let _ = self.populate_children(path, &mut node, depth + 1, max_depth);
            }

            children.push(node);
        }

        children.sort_by(|a, b| {
            if a.is_dir != b.is_dir {
                b.is_dir.cmp(&a.is_dir)
            } else {
                a.name.to_lowercase().cmp(&b.name.to_lowercase())
            }
        });

        current_node.children = Some(children);
        Ok(())
    }

    /// Resolves a relative or absolute path against `self.root_path` WITHOUT creating any
    /// directories on disk, and verifies that the resolved path stays inside `self.root_path`.
    pub fn resolve_inside_root<P: AsRef<Path>>(&self, rel_or_abs: P) -> Result<PathBuf, ProjectError> {
        let p = rel_or_abs.as_ref();
        let combined = if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root_path.join(p)
        };

        // Find the deepest existing ancestor on disk so we can canonicalize symlinks/junctions
        let mut existing_ancestor = combined.as_path();
        let mut tail_components = Vec::new();
        while !existing_ancestor.exists() {
            if let Some(name) = existing_ancestor.file_name() {
                tail_components.push(name.to_os_string());
            }
            if let Some(parent) = existing_ancestor.parent() {
                if parent == existing_ancestor {
                    break;
                }
                existing_ancestor = parent;
            } else {
                break;
            }
        }

        let canonical_base = if existing_ancestor.exists() {
            fs::canonicalize(existing_ancestor)?
        } else {
            self.root_path.clone()
        };

        let mut resolved = canonical_base;
        for comp_os in tail_components.into_iter().rev() {
            let comp_path = Path::new(&comp_os);
            for c in comp_path.components() {
                match c {
                    Component::Normal(seg) => resolved.push(seg),
                    Component::CurDir => {}
                    Component::ParentDir => {
                        resolved.pop();
                    }
                    Component::RootDir | Component::Prefix(_) => {
                        return Err(ProjectError::InvalidPath(format!(
                            "Security: invalid path component in '{}'",
                            p.to_string_lossy()
                        )));
                    }
                }
            }
        }

        if !resolved.starts_with(&self.root_path) {
            return Err(ProjectError::InvalidPath(format!(
                "Security: path '{}' escapes project root",
                p.to_string_lossy()
            )));
        }

        Ok(resolved)
    }

    pub fn read_file<P: AsRef<Path>>(&self, rel_or_abs: P) -> Result<FileContentPayload, ProjectError> {
        let p = rel_or_abs.as_ref();
        let target_path = if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root_path.join(p)
        };

        let canonical_target = fs::canonicalize(&target_path)?;
        if !canonical_target.starts_with(&self.root_path) {
            return Err(ProjectError::InvalidPath(format!(
                "Security: path '{}' escapes project root",
                canonical_target.to_string_lossy()
            )));
        }

        let rel_path = canonical_target
            .strip_prefix(&self.root_path)
            .unwrap_or(&canonical_target);

        if is_sensitive_path(rel_path) {
            return Err(ProjectError::InvalidPath(format!(
                "Security: access to sensitive file '{}' is prohibited",
                rel_path.to_string_lossy()
            )));
        }

        let meta = fs::metadata(&canonical_target)?;
        let max_size = 5 * 1024 * 1024; // 5MB limit
        if meta.len() > max_size {
            return Err(ProjectError::FileTooLarge {
                size: meta.len(),
                max: max_size,
            });
        }

        let raw = fs::read(&canonical_target)?;
        let content = String::from_utf8(raw).map_err(|_| ProjectError::NotUtf8)?;
        let hash = compute_hash(&content);
        let file_name = canonical_target
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        Ok(FileContentPayload {
            path: normalize_path(&rel_path.to_string_lossy()),
            content,
            hash,
            language: detect_language(&file_name),
            size: meta.len(),
        })
    }

    pub fn save_file<P: AsRef<Path>>(&self, rel_or_abs: P, content: &str) -> Result<String, ProjectError> {
        // 1. Resolve and validate root prefix BEFORE creating any directories on disk
        let resolved_target = self.resolve_inside_root(&rel_or_abs)?;

        let rel_path = resolved_target
            .strip_prefix(&self.root_path)
            .unwrap_or(&resolved_target);

        // 2. Check sensitive file rules on relative path
        if is_sensitive_path(rel_path) {
            return Err(ProjectError::InvalidPath(format!(
                "Security: saving sensitive file '{}' is prohibited",
                rel_path.to_string_lossy()
            )));
        }

        // 3. Only after security checks pass, create parent directories if needed
        if let Some(parent) = resolved_target.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
            let canonical_parent = fs::canonicalize(parent)?;
            if !canonical_parent.starts_with(&self.root_path) {
                return Err(ProjectError::InvalidPath(format!(
                    "Security: target directory '{}' escapes project root",
                    canonical_parent.to_string_lossy()
                )));
            }
        }

        fs::write(&resolved_target, content)?;
        Ok(compute_hash(content))
    }
}

/// Checks if a relative path inside the project is a sensitive credential/key file.
/// Rules (A3):
/// - Inspects path segments and filename only (never substring-matching "credentials" or "secrets").
/// - Directory segments blocked: `.ssh`, `.aws`, `.gnupg`
/// - Exact filenames blocked: `.env`, `id_rsa`, `id_ed25519`, `id_ecdsa`, `id_dsa`, `.netrc`, `.npmrc`, `.pypirc`
/// - `.env.*` blocked EXCEPT `.env.example`, `.env.sample`, `.env.template`, `.env.schema`
/// - Extensions blocked: `pem`, `key`, `pfx`, `p12`, `crt`, `cer`, `der`, `jks`, `keystore`
pub fn is_sensitive_path<P: AsRef<Path>>(rel_path: P) -> bool {
    let path = rel_path.as_ref();

    // Normalize backslashes on Windows if someone passed a string path with `\`
    let normalized_str = path.to_string_lossy().replace('\\', "/");
    let norm_path = Path::new(&normalized_str);

    for comp in norm_path.components() {
        if let Component::Normal(seg) = comp {
            let seg_lower = seg.to_string_lossy().to_lowercase();
            if matches!(seg_lower.as_str(), ".ssh" | ".aws" | ".gnupg") {
                return true;
            }
        }
    }

    let file_name = match norm_path.file_name() {
        Some(n) => n.to_string_lossy().to_lowercase(),
        None => return false,
    };

    // Exact filename matches
    if matches!(
        file_name.as_str(),
        ".env"
            | "id_rsa"
            | "id_ed25519"
            | "id_ecdsa"
            | "id_dsa"
            | "id_rsa.pub"
            | ".netrc"
            | ".npmrc"
            | ".pypirc"
    ) {
        return true;
    }

    // .env.* variants except safe templates
    if file_name.starts_with(".env.") {
        if matches!(
            file_name.as_str(),
            ".env.example" | ".env.sample" | ".env.template" | ".env.schema"
        ) {
            return false;
        }
        return true;
    }

    // Extension matches
    if let Some(ext) = norm_path.extension() {
        let ext_lower = ext.to_string_lossy().to_lowercase();
        if matches!(
            ext_lower.as_str(),
            "pem" | "key" | "pfx" | "p12" | "crt" | "cer" | "der" | "jks" | "keystore"
        ) {
            return true;
        }
    }

    false
}

pub fn normalize_path(path: &str) -> String {
    let s = path.replace('\\', "/");
    // Strip Windows extended-length path prefix `//?/` if present for display consistency
    if let Some(stripped) = s.strip_prefix("//?/") {
        stripped.to_string()
    } else {
        s
    }
}

pub fn compute_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn detect_language(filename: &str) -> String {
    let lower = filename.to_lowercase();
    if lower == "cmakelists.txt" || lower.ends_with(".cmake") {
        "cmake".to_string()
    } else if lower.ends_with(".rs") {
        "rust".to_string()
    } else if lower.ends_with(".ts") || lower.ends_with(".tsx") {
        "typescript".to_string()
    } else if lower.ends_with(".js") || lower.ends_with(".jsx") || lower.ends_with(".mjs") {
        "javascript".to_string()
    } else if lower.ends_with(".py") {
        "python".to_string()
    } else if lower.ends_with(".json") {
        "json".to_string()
    } else if lower.ends_with(".html") || lower.ends_with(".htm") {
        "html".to_string()
    } else if lower.ends_with(".css") || lower.ends_with(".scss") {
        "css".to_string()
    } else if lower.ends_with(".md") || lower.ends_with(".markdown") {
        "markdown".to_string()
    } else if lower.ends_with(".c") || lower.ends_with(".h") {
        "c".to_string()
    } else if lower.ends_with(".cpp")
        || lower.ends_with(".hpp")
        || lower.ends_with(".cc")
        || lower.ends_with(".cxx")
        || lower.ends_with(".hh")
    {
        "cpp".to_string()
    } else if lower.ends_with(".toml") {
        "toml".to_string()
    } else if lower.ends_with(".yaml") || lower.ends_with(".yml") {
        "yaml".to_string()
    } else if lower.ends_with(".xml") || lower.ends_with(".urdf") || lower.ends_with(".xacro") {
        "xml".to_string()
    } else if lower.ends_with(".msg") || lower.ends_with(".srv") || lower.ends_with(".action") {
        "ros2_interface".to_string()
    } else if lower.ends_with(".sh") || lower.ends_with(".bash") {
        "shell".to_string()
    } else if lower.ends_with(".sql") {
        "sql".to_string()
    } else if lower.ends_with(".go") {
        "go".to_string()
    } else if lower.ends_with(".java") {
        "java".to_string()
    } else if lower.ends_with(".cs") {
        "csharp".to_string()
    } else if lower.ends_with(".kt") || lower.ends_with(".kts") {
        "kotlin".to_string()
    } else if lower.ends_with(".swift") {
        "swift".to_string()
    } else if lower.ends_with(".php") {
        "php".to_string()
    } else {
        "plaintext".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        assert_eq!(
            normalize_path("C:\\Users\\dev\\project"),
            "C:/Users/dev/project"
        );
        assert_eq!(normalize_path("src/lib.rs"), "src/lib.rs");
        assert_eq!(
            normalize_path("\\\\?\\F:\\Projects\\Noesis"),
            "F:/Projects/Noesis"
        );
    }

    #[test]
    fn test_detect_language() {
        assert_eq!(detect_language("main.rs"), "rust");
        assert_eq!(detect_language("App.tsx"), "typescript");
        assert_eq!(detect_language("script.py"), "python");
        assert_eq!(detect_language("config.toml"), "toml");
        assert_eq!(detect_language("CMakeLists.txt"), "cmake");
        assert_eq!(detect_language("AudioFrame.msg"), "ros2_interface");
        assert_eq!(detect_language("CancelTurn.srv"), "ros2_interface");
        assert_eq!(detect_language("robot.urdf.xacro"), "xml");
        assert_eq!(detect_language("unknown.xyz"), "plaintext");
    }

    #[test]
    fn test_compute_hash() {
        let hash1 = compute_hash("hello noesis");
        let hash2 = compute_hash("hello noesis");
        let hash3 = compute_hash("different");
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 64);
    }

    #[test]
    fn test_is_sensitive_path_table_24_cases() {
        let cases: Vec<(&str, bool)> = vec![
            // Positive (must block)
            (".env", true),
            (".env.local", true),
            (".env.production", true),
            (".env.development", true),
            ("config/.env", true),
            ("config/.env.staging", true),
            ("id_rsa", true),
            ("id_ed25519", true),
            ("keys/id_rsa", true),
            (".ssh/config", true),
            (".ssh/authorized_keys", true),
            (".aws/credentials", true),
            ("certs/server.pem", true),
            ("certs/private.key", true),
            ("certs/bundle.pfx", true),
            ("certs/store.p12", true),
            ("certs/root.crt", true),
            ("certs/ca.cer", true),
            // Negative (must allow - legitimate source & template files)
            (".env.example", false),
            (".env.sample", false),
            (".env.template", false),
            ("src/environment.py", false),
            ("src/monkey.py", false),
            ("src/keyboard.rs", false),
            ("src/auth/credentials_form.tsx", false),
            ("src/services/credentials_helper.py", false),
            ("docs/secrets_management.md", false),
            ("src/secrets_parser.rs", false),
            ("src/main.rs", false),
            ("package.json", false),
        ];

        for (path, expected_sensitive) in cases {
            assert_eq!(
                is_sensitive_path(path),
                expected_sensitive,
                "Path '{}' sensitivity mismatch: expected {}",
                path,
                expected_sensitive
            );
        }
    }

    #[test]
    fn test_save_file_rejects_escape_before_creating_directory() {
        let base_temp = std::env::temp_dir().join(format!(
            "noesis_proj_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let proj_root = base_temp.join("workspace");
        let outside_dir = base_temp.join("outside_escaped_dir");

        fs::create_dir_all(&proj_root).unwrap();
        assert!(!outside_dir.exists());

        let pm = ProjectManager::new(&proj_root).unwrap();

        // Attempt to escape root via ../outside_escaped_dir/evil.txt
        let res = pm.save_file("../outside_escaped_dir/evil.txt", "malicious");
        assert!(res.is_err(), "save_file should reject path escaping root");
        assert!(
            !outside_dir.exists(),
            "outside_escaped_dir must NOT be created on disk when save_file rejects escape"
        );

        // Legitimate save inside root should succeed
        let ok_res = pm.save_file("subdir/hello.py", "print('ok')");
        assert!(ok_res.is_ok());
        let payload = pm.read_file("subdir/hello.py").unwrap();
        assert_eq!(payload.content, "print('ok')");

        let _ = fs::remove_dir_all(&base_temp);
    }
}
