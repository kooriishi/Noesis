use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
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
    pub path: String,       // Normalized relative or canonical path
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
        Ok(Self { root_path: canonical })
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
                // Standard ignores
                if name == ".git" || name == "node_modules" || name == "target" || name == "dist" {
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
            let rel_path = path
                .strip_prefix(&self.root_path)
                .map(|p| normalize_path(&p.to_string_lossy()))
                .unwrap_or_else(|_| abs_path.clone());

            let mut node = FileNode {
                name: file_name.clone(),
                path: rel_path,
                absolute_path: abs_path,
                is_dir,
                children: if is_dir { Some(Vec::new()) } else { None },
                size: if is_dir { 0 } else { entry.metadata().map(|m| m.len()).unwrap_or(0) },
                language: if is_dir { None } else { Some(detect_language(&file_name)) },
            };

            if is_dir {
                let _ = self.populate_children(path, &mut node, depth + 1, max_depth);
            }

            children.push(node);
        }

        // Sort: directories first, then alphabetical
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

        if is_sensitive_path(&canonical_target) {
            return Err(ProjectError::InvalidPath(format!(
                "Security: access to sensitive file '{}' is prohibited",
                canonical_target.to_string_lossy()
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
            path: normalize_path(&canonical_target.to_string_lossy()),
            content,
            hash,
            language: detect_language(&file_name),
            size: meta.len(),
        })
    }

    pub fn save_file<P: AsRef<Path>>(&self, rel_or_abs: P, content: &str) -> Result<String, ProjectError> {
        let p = rel_or_abs.as_ref();
        let target_path = if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root_path.join(p)
        };

        let parent = target_path.parent().unwrap_or(&target_path);
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

        if is_sensitive_path(&target_path) {
            return Err(ProjectError::InvalidPath(format!(
                "Security: saving sensitive file '{}' is prohibited",
                target_path.to_string_lossy()
            )));
        }

        fs::write(&target_path, content)?;
        Ok(compute_hash(content))
    }
}

pub fn is_sensitive_path<P: AsRef<Path>>(path: P) -> bool {
    let lower = path.as_ref().to_string_lossy().to_lowercase();
    let blocked = [
        ".env", ".key", ".pem", ".pfx", ".cert", ".crt",
        "id_rsa", "id_ed25519", "credentials", "secrets",
    ];
    for b in blocked {
        if lower.contains(b) {
            return true;
        }
    }
    false
}

pub fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

pub fn compute_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn detect_language(filename: &str) -> String {
    let lower = filename.to_lowercase();
    if lower.ends_with(".rs") {
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
    } else if lower.ends_with(".cpp") || lower.ends_with(".hpp") || lower.ends_with(".cc") {
        "cpp".to_string()
    } else if lower.ends_with(".toml") {
        "toml".to_string()
    } else if lower.ends_with(".yaml") || lower.ends_with(".yml") {
        "yaml".to_string()
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
        assert_eq!(normalize_path("C:\\Users\\dev\\project"), "C:/Users/dev/project");
        assert_eq!(normalize_path("src/lib.rs"), "src/lib.rs");
    }

    #[test]
    fn test_detect_language() {
        assert_eq!(detect_language("main.rs"), "rust");
        assert_eq!(detect_language("App.tsx"), "typescript");
        assert_eq!(detect_language("script.py"), "python");
        assert_eq!(detect_language("config.toml"), "toml");
        assert_eq!(detect_language("server.go"), "go");
        assert_eq!(detect_language("Application.java"), "java");
        assert_eq!(detect_language("Program.cs"), "csharp");
        assert_eq!(detect_language("Main.kt"), "kotlin");
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
    fn test_is_sensitive_path() {
        assert!(is_sensitive_path(".env"));
        assert!(is_sensitive_path("config/.env.local"));
        assert!(is_sensitive_path("certs/server.pem"));
        assert!(is_sensitive_path("id_rsa"));
        assert!(is_sensitive_path("secrets/api_token.json"));
        assert!(!is_sensitive_path("src/main.rs"));
        assert!(!is_sensitive_path("package.json"));
    }
}
