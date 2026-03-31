use std::path::{Path, PathBuf};

/// A discovered `.selfware.md` memory file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryFile {
    pub path: PathBuf,
    pub content: String,
}

/// Hierarchical memory file system for `.selfware.md` files.
///
/// Discovers memory files by walking up from the current working directory
/// toward the home directory, similar to Claude Code's `.claude.md` mechanism.
pub struct MemorySystem;

impl MemorySystem {
    /// Walk from `cwd` up to the home directory, collecting every `.selfware.md`
    /// found along the way. Files are returned in discovery order (closest to
    /// `cwd` first).
    pub fn discover(cwd: &Path) -> Vec<MemoryFile> {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let mut files = Vec::new();

        for ancestor in cwd.ancestors() {
            let candidate = ancestor.join(".selfware.md");
            if candidate.is_file() {
                if let Ok(content) = std::fs::read_to_string(&candidate) {
                    files.push(MemoryFile {
                        path: candidate,
                        content,
                    });
                }
            }
            if ancestor == home {
                break;
            }
        }

        files
    }

    /// Format a slice of memory files for injection into a system prompt.
    pub fn format_for_prompt(files: &[MemoryFile]) -> String {
        if files.is_empty() {
            return String::new();
        }

        let mut parts = vec!["## Memory Files".to_string()];
        for file in files {
            parts.push(format!(
                "### From `{}`\n{}",
                file.path.display(),
                file.content
            ));
        }
        parts.join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_finds_files_up_to_home() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let project = home.join("projects").join("myapp");
        let src = project.join("src");
        std::fs::create_dir_all(&src).unwrap();

        // Create two memory files: one in project root, one in home
        let project_memory = project.join(".selfware.md");
        let home_memory = home.join(".selfware.md");
        let outside_memory = temp.path().join(".selfware.md");

        std::fs::write(&project_memory, "project memory").unwrap();
        std::fs::write(&home_memory, "home memory").unwrap();
        std::fs::write(&outside_memory, "outside memory").unwrap();

        // Temporarily override home_dir by using a path inside our temp tree.
        // Since `dirs::home_dir()` reads env vars, we can set HOME.
        let original_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &home);

        let files = MemorySystem::discover(&src);

        // Restore HOME
        match original_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, project_memory);
        assert_eq!(files[0].content, "project memory");
        assert_eq!(files[1].path, home_memory);
        assert_eq!(files[1].content, "home memory");
    }

    #[test]
    fn test_format_for_prompt_empty() {
        let formatted = MemorySystem::format_for_prompt(&[]);
        assert!(formatted.is_empty());
    }

    #[test]
    fn test_format_for_prompt_with_files() {
        let files = vec![
            MemoryFile {
                path: PathBuf::from("/project/.selfware.md"),
                content: "Use Rust 2021 edition.".to_string(),
            },
            MemoryFile {
                path: PathBuf::from("/home/.selfware.md"),
                content: "Prefer anyhow for errors.".to_string(),
            },
        ];
        let formatted = MemorySystem::format_for_prompt(&files);
        assert!(formatted.starts_with("## Memory Files"));
        assert!(formatted.contains("### From `/project/.selfware.md`"));
        assert!(formatted.contains("Use Rust 2021 edition."));
        assert!(formatted.contains("### From `/home/.selfware.md`"));
        assert!(formatted.contains("Prefer anyhow for errors."));
    }
}
