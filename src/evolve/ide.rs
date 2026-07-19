//! IDE file explorer and code viewer backend.
//!
//! Lists files under the source root, reads individual file contents, and
//! writes edited contents back, backing the `/api/ide/files`,
//! `/api/ide/read`, and `/api/ide/write` endpoints.

use anyhow::Result;
use std::path::PathBuf;

/// Metadata for a single entry under the source root.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub is_dir: bool,
    pub size: usize,
}

/// File explorer and code viewer for the IDE experience.
pub struct IdeEngine {
    src_root: PathBuf,
}

impl IdeEngine {
    pub fn new(src_root: impl AsRef<std::path::Path>) -> Self {
        Self {
            src_root: src_root.as_ref().to_path_buf(),
        }
    }

    /// Lists the top-level entries under the source root.
    pub fn list_files(&self) -> Result<Vec<FileInfo>> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(&self.src_root)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = entry.metadata()?;
            files.push(FileInfo {
                path: format!("src/{}", path.file_name().unwrap().to_string_lossy()),
                is_dir: metadata.is_dir(),
                size: metadata.len() as usize,
            });
        }
        Ok(files)
    }

    /// Reads a file relative to the source root, with or without the `src/` prefix.
    ///
    /// Rejects paths that resolve outside the source root (path traversal).
    pub fn read_file(&self, path: &str) -> Result<String> {
        let stripped = path.strip_prefix("src/").unwrap_or(path);
        let full = self.src_root.join(stripped);
        let canonical = full.canonicalize()?;
        let root_canonical = self.src_root.canonicalize()?;
        if !canonical.starts_with(&root_canonical) {
            anyhow::bail!("path traversal rejected");
        }
        Ok(std::fs::read_to_string(canonical)?)
    }

    /// Writes a file relative to the source root, with or without the `src/`
    /// prefix. Parent directories are created as needed.
    ///
    /// Rejects paths that resolve outside the source root (path traversal).
    pub fn write_file(&self, path: &str, content: &str) -> Result<()> {
        let stripped = path.strip_prefix("src/").unwrap_or(path);
        // Reject traversal up front: a `..` after a non-existent component
        // (e.g. `foo/../../escaped.txt`) defeats ancestor canonicalization.
        for component in std::path::Path::new(stripped).components() {
            match component {
                std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_) => {
                    anyhow::bail!("path traversal rejected");
                }
                _ => {}
            }
        }
        let full = self.src_root.join(stripped);
        let root_canonical = self.src_root.canonicalize()?;
        // The file may not exist yet, so canonicalize the nearest existing
        // ancestor instead and verify it stays under the source root.
        let mut ancestor = full.parent();
        let canonical_ancestor = loop {
            let dir = ancestor.ok_or_else(|| anyhow::anyhow!("path traversal rejected"))?;
            if let Ok(canonical) = dir.canonicalize() {
                break canonical;
            }
            ancestor = dir.parent();
        };
        if !canonical_ancestor.starts_with(&root_canonical) {
            anyhow::bail!("path traversal rejected");
        }
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(std::fs::write(&full, content)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ide_engine_lists_src_files() {
        let engine = IdeEngine::new("src");
        let files = engine.list_files().unwrap();
        assert!(files.iter().any(|f| f.path == "src/lib.rs"));
    }

    #[test]
    fn test_ide_engine_reads_file_with_and_without_prefix() {
        let engine = IdeEngine::new("src");
        let with_prefix = engine.read_file("src/lib.rs").unwrap();
        let without_prefix = engine.read_file("lib.rs").unwrap();
        assert_eq!(with_prefix, without_prefix);
        assert!(with_prefix.contains("pub mod evolve;"));
    }

    #[test]
    fn test_ide_engine_read_missing_file_errors() {
        let engine = IdeEngine::new("src");
        assert!(engine.read_file("src/definitely-not-here.rs").is_err());
    }

    #[test]
    fn test_ide_engine_rejects_path_traversal() {
        let engine = IdeEngine::new("src");
        assert!(engine.read_file("../Cargo.toml").is_err());
        assert!(engine.read_file("src/../Cargo.toml").is_err());
        assert!(engine.read_file("../../etc/hosts").is_err());
        // Legitimate paths still work.
        assert!(engine.read_file("src/lib.rs").is_ok());
    }

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("selfware-ide-test-{}", name));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn test_ide_engine_write_file_round_trip() {
        let root = temp_root("write-round-trip");
        let engine = IdeEngine::new(&root);
        engine.write_file("hello.rs", "fn main() {}\n").unwrap();
        assert_eq!(engine.read_file("hello.rs").unwrap(), "fn main() {}\n");
        // Overwrite works too.
        engine.write_file("hello.rs", "fn main() { todo!() }\n").unwrap();
        assert_eq!(engine.read_file("hello.rs").unwrap(), "fn main() { todo!() }\n");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn test_ide_engine_write_file_rejects_path_traversal() {
        let root = temp_root("write-traversal");
        let engine = IdeEngine::new(&root);
        assert!(engine.write_file("../escape.txt", "nope").is_err());
        assert!(engine.write_file("../../escape.txt", "nope").is_err());
        // `..` after a non-existent intermediate component must not bypass
        // the ancestor-canonicalization guard.
        assert!(engine.write_file("foo/../../escaped.txt", "nope").is_err());
        assert!(!root.parent().unwrap().join("escaped.txt").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn test_ide_engine_write_file_creates_parent_dirs() {
        let root = temp_root("write-parents");
        let engine = IdeEngine::new(&root);
        engine.write_file("sub/dir/new.rs", "pub fn x() {}\n").unwrap();
        assert_eq!(engine.read_file("sub/dir/new.rs").unwrap(), "pub fn x() {}\n");
        std::fs::remove_dir_all(&root).ok();
    }
}
