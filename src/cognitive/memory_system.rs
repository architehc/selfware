use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

const WORKSPACE_GUIDANCE_FILENAMES: &[&str] = &["AGENTS.md", "CLAUDE.md", ".claude.md"];
const MAX_WORKSPACE_GUIDANCE_BYTES: usize = 24 * 1024;

/// A discovered `.selfware.md` memory file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryFile {
    pub path: PathBuf,
    pub content: String,
}

/// A discovered workspace guidance file such as `AGENTS.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceGuidanceFile {
    pub path: PathBuf,
    pub content: String,
}

/// A discovered `MEMORY.md` consolidated memory file from the dream system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsolidatedMemory {
    pub project_key: String,
    pub path: PathBuf,
    pub content: String,
}

/// Hierarchical memory file system for `.selfware.md` files.
///
/// Discovers memory files by walking up from the current working directory
/// toward the home directory, similar to Claude Code's `.claude.md` mechanism.
pub struct MemorySystem;

/// Dream-integrated memory system for managing consolidated memories.
///
/// This system integrates with the Dream System to:
/// - Load consolidated MEMORY.md files into prompts
/// - Check dream gates after session ends
/// - Spawn autoDream subprocess when appropriate
pub struct DreamIntegratedMemorySystem {
    /// Project key for memory storage
    project_key: String,
    /// Dream configuration
    dream_config: crate::cognitive::dream::DreamConfig,
    /// AutoDream configuration
    auto_dream_config: crate::cognitive::dream_subprocess::AutoDreamConfig,
}

impl MemorySystem {
    fn read_capped_text(path: &Path, max_bytes: usize) -> Option<String> {
        let text = std::fs::read_to_string(path).ok()?;
        if text.len() <= max_bytes {
            return Some(text);
        }

        let mut capped = String::with_capacity(max_bytes + 64);
        for ch in text.chars() {
            if capped.len() + ch.len_utf8() > max_bytes {
                break;
            }
            capped.push(ch);
        }
        capped.push_str("\n... [truncated]");
        Some(capped)
    }

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

    /// Walk from `cwd` up to the home directory, collecting repo-local guidance
    /// files such as `AGENTS.md`. These files provide project-specific operating
    /// instructions and should be treated as high-priority prompt context.
    pub fn discover_workspace_guidance(cwd: &Path) -> Vec<WorkspaceGuidanceFile> {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let mut files = Vec::new();

        for ancestor in cwd.ancestors() {
            for filename in WORKSPACE_GUIDANCE_FILENAMES {
                let candidate = ancestor.join(filename);
                if candidate.is_file() {
                    if let Some(content) =
                        Self::read_capped_text(&candidate, MAX_WORKSPACE_GUIDANCE_BYTES)
                    {
                        files.push(WorkspaceGuidanceFile {
                            path: candidate,
                            content,
                        });
                    }
                }
            }

            if ancestor == home {
                break;
            }
        }

        files
    }

    /// Discover consolidated MEMORY.md files from the dream system.
    ///
    /// Looks in `~/.selfware/memory/` for MEMORY.md files associated with projects
    /// in the ancestor path hierarchy.
    pub fn discover_consolidated(cwd: &Path) -> Vec<ConsolidatedMemory> {
        let home = dirs::home_dir();
        let memory_base = home
            .as_ref()
            .map(|h| h.join(".selfware").join("memory"))
            .unwrap_or_else(|| PathBuf::from(".selfware").join("memory"));

        let mut memories = Vec::new();

        for ancestor in cwd.ancestors() {
            // Generate project key from path
            let project_key = Self::project_key_from_path(ancestor);
            let memory_path = memory_base.join(format!("{}_MEMORY.md", project_key));

            if memory_path.is_file() {
                if let Ok(content) = std::fs::read_to_string(&memory_path) {
                    memories.push(ConsolidatedMemory {
                        project_key,
                        path: memory_path,
                        content,
                    });
                }
            }

            // Stop at home directory
            if home
                .as_ref()
                .map(|h| ancestor == h.as_path())
                .unwrap_or(false)
            {
                break;
            }
        }

        memories
    }

    /// Generate a project key from a path.
    ///
    /// Uses the last two components of the path to create a unique key.
    fn project_key_from_path(path: &Path) -> String {
        let components: Vec<_> = path
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect();

        if components.len() >= 2 {
            format!(
                "{}_{}",
                components[components.len() - 2],
                components[components.len() - 1]
            )
        } else if let Some(last) = components.last() {
            last.to_string()
        } else {
            "unknown".to_string()
        }
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

    /// Format workspace guidance files for prompt injection.
    pub fn format_workspace_guidance_for_prompt(files: &[WorkspaceGuidanceFile]) -> String {
        if files.is_empty() {
            return String::new();
        }

        let mut parts = vec![
            "## Workspace Guidance".to_string(),
            "Follow the most local guidance file when instructions conflict.".to_string(),
        ];
        for file in files {
            parts.push(format!(
                "### From `{}`\n{}",
                file.path.display(),
                file.content
            ));
        }
        parts.join("\n\n")
    }

    /// Format consolidated memories for injection into a system prompt.
    pub fn format_consolidated_for_prompt(memories: &[ConsolidatedMemory]) -> String {
        if memories.is_empty() {
            return String::new();
        }

        let mut parts = vec!["## Consolidated Project Memory".to_string()];
        for memory in memories {
            parts.push(format!(
                "### Project: {}\n{}",
                memory.project_key, memory.content
            ));
        }
        parts.join("\n\n")
    }

    /// Load all memory sources (both .selfware.md and consolidated MEMORY.md)
    pub fn load_all_memories(cwd: &Path) -> String {
        let mut parts = Vec::new();

        // Load .selfware.md files
        let memory_files = Self::discover(cwd);
        let formatted = Self::format_for_prompt(&memory_files);
        if !formatted.is_empty() {
            parts.push(formatted);
        }

        // Load consolidated MEMORY.md files
        let consolidated = Self::discover_consolidated(cwd);
        let formatted_consolidated = Self::format_consolidated_for_prompt(&consolidated);
        if !formatted_consolidated.is_empty() {
            parts.push(formatted_consolidated);
        }

        parts.join("\n\n")
    }
}

impl DreamIntegratedMemorySystem {
    /// Create a new dream-integrated memory system.
    pub fn new(project_path: &Path) -> Self {
        let project_key = MemorySystem::project_key_from_path(project_path);

        Self {
            project_key,
            dream_config: crate::cognitive::dream::DreamConfig::new(),
            auto_dream_config: crate::cognitive::dream_subprocess::AutoDreamConfig::new(),
        }
    }

    /// Create with custom dream configuration.
    pub fn with_dream_config(mut self, config: crate::cognitive::dream::DreamConfig) -> Self {
        self.dream_config = config;
        self
    }

    /// Create with custom autoDream configuration.
    pub fn with_auto_dream_config(
        mut self,
        config: crate::cognitive::dream_subprocess::AutoDreamConfig,
    ) -> Self {
        self.auto_dream_config = config;
        self
    }

    /// Load consolidated MEMORY.md for the project.
    pub fn load_consolidated_memory(&self) -> Option<ConsolidatedMemory> {
        let memory_path = self.dream_config.memory_file_path(&self.project_key);

        if !memory_path.exists() {
            return None;
        }

        match std::fs::read_to_string(&memory_path) {
            Ok(content) => Some(ConsolidatedMemory {
                project_key: self.project_key.clone(),
                path: memory_path,
                content,
            }),
            Err(e) => {
                warn!("Failed to read consolidated memory: {}", e);
                None
            }
        }
    }

    /// Check dream gates and potentially spawn autoDream subprocess.
    ///
    /// Call this after a session ends. Returns the subprocess handle if spawned.
    pub async fn check_and_spawn_dream(
        &self,
        project_path: &Path,
    ) -> anyhow::Result<Option<crate::cognitive::dream_subprocess::AutoDreamHandle>> {
        crate::cognitive::dream_subprocess::check_and_spawn_autodream(
            project_path,
            &self.project_key,
            &self.auto_dream_config,
            &self.dream_config,
        )
        .await
    }

    /// Get dream status for display.
    pub async fn dream_status(&self) -> crate::cognitive::dream::DreamStatus {
        crate::cognitive::dream_subprocess::get_dream_status(&self.dream_config).await
    }

    /// Record a session end and update dream state.
    ///
    /// This should be called when a session ends, even if not spawning a dream.
    pub fn record_session_end(&self) -> anyhow::Result<()> {
        use crate::cognitive::dream::DreamState;

        let mut state = DreamState::load(&self.dream_config.state_path())?;
        state.record_session_end();
        state.save(&self.dream_config.state_path())?;

        debug!(
            "Recorded session end for {}: {} sessions since last dream",
            self.project_key, state.sessions_since_last_dream
        );

        Ok(())
    }

    /// Force trigger a dream (manual override).
    ///
    /// This bypasses the normal gate checks.
    pub async fn force_dream(
        &self,
        project_path: &Path,
    ) -> anyhow::Result<crate::cognitive::dream::DreamResult> {
        info!("Force-triggering dream for {}", self.project_key);

        crate::cognitive::dream_subprocess::run_dream_consolidation(
            project_path,
            &self.project_key,
            &self.auto_dream_config,
            &self.dream_config,
        )
        .await
    }

    /// Format consolidated memory for system prompt.
    pub fn format_for_prompt(&self) -> String {
        match self.load_consolidated_memory() {
            Some(memory) => {
                format!(
                    "## Consolidated Memory (from dream system)\n{}",
                    memory.content
                )
            }
            None => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[cfg(unix)]
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

    #[cfg(unix)]
    #[test]
    fn test_discover_workspace_guidance_finds_agents_files_up_to_home() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let project = home.join("projects").join("radarcam");
        let src = project.join("src");
        std::fs::create_dir_all(&src).unwrap();

        let project_guidance = project.join("AGENTS.md");
        let home_guidance = home.join("CLAUDE.md");
        let outside_guidance = temp.path().join("AGENTS.md");

        std::fs::write(&project_guidance, "project guidance").unwrap();
        std::fs::write(&home_guidance, "home guidance").unwrap();
        std::fs::write(&outside_guidance, "outside guidance").unwrap();

        let original_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &home);

        let files = MemorySystem::discover_workspace_guidance(&src);

        match original_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, project_guidance);
        assert_eq!(files[0].content, "project guidance");
        assert_eq!(files[1].path, home_guidance);
        assert_eq!(files[1].content, "home guidance");
    }

    #[test]
    fn test_format_workspace_guidance_for_prompt() {
        let files = vec![WorkspaceGuidanceFile {
            path: PathBuf::from("/project/AGENTS.md"),
            content: "Operate on /home/ivo/radarcam.".to_string(),
        }];

        let formatted = MemorySystem::format_workspace_guidance_for_prompt(&files);
        assert!(formatted.contains("## Workspace Guidance"));
        assert!(formatted.contains("Follow the most local guidance file"));
        assert!(formatted.contains("### From `/project/AGENTS.md`"));
        assert!(formatted.contains("Operate on /home/ivo/radarcam."));
    }

    #[test]
    fn test_discover_workspace_guidance_truncates_large_files() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let project = home.join("project");
        std::fs::create_dir_all(&project).unwrap();

        let large_guidance = project.join("AGENTS.md");
        std::fs::write(
            &large_guidance,
            "a".repeat(MAX_WORKSPACE_GUIDANCE_BYTES + 128),
        )
        .unwrap();

        let original_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &home);

        let files = MemorySystem::discover_workspace_guidance(&project);

        match original_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }

        assert_eq!(files.len(), 1);
        assert!(files[0].content.ends_with("... [truncated]"));
        assert!(files[0].content.len() <= MAX_WORKSPACE_GUIDANCE_BYTES + 32);
    }

    #[cfg(unix)]
    #[test]
    fn test_project_key_from_path() {
        let path = Path::new("/home/user/projects/myapp");
        let key = MemorySystem::project_key_from_path(path);
        assert_eq!(key, "projects_myapp");

        let path = Path::new("myapp");
        let key = MemorySystem::project_key_from_path(path);
        assert_eq!(key, "myapp");

        // Root path returns "/" as the key (single component)
        let path = Path::new("/");
        let key = MemorySystem::project_key_from_path(path);
        assert_eq!(key, "/");
    }

    #[cfg(unix)]
    #[test]
    fn test_discover_consolidated_memory() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        let project = home.join("projects").join("myapp");
        let memory_base = home.join(".selfware").join("memory");

        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&memory_base).unwrap();

        // Create a consolidated memory file
        let memory_content = "# Project Memory\n\n## Facts\n- [2026-03-31] Test fact\n";
        std::fs::write(memory_base.join("projects_myapp_MEMORY.md"), memory_content).unwrap();

        // Set HOME to temp directory
        let original_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &home);

        let memories = MemorySystem::discover_consolidated(&project);

        // Restore HOME
        match original_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }

        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].project_key, "projects_myapp");
        assert!(memories[0].content.contains("Test fact"));
    }

    #[test]
    fn test_format_consolidated_for_prompt() {
        let memories = vec![ConsolidatedMemory {
            project_key: "test_project".to_string(),
            path: PathBuf::from("/tmp/test_MEMORY.md"),
            content: "- Fact 1\n- Fact 2".to_string(),
        }];

        let formatted = MemorySystem::format_consolidated_for_prompt(&memories);
        assert!(formatted.contains("## Consolidated Project Memory"));
        assert!(formatted.contains("### Project: test_project"));
        assert!(formatted.contains("- Fact 1"));
    }

    #[tokio::test]
    async fn test_dream_integrated_memory_system_creation() {
        let temp = tempdir().unwrap();
        // Create a nested structure to avoid tempdir random prefix
        let parent = temp.path().join("projects");
        let project = parent.join("myapp");
        std::fs::create_dir_all(&project).unwrap();

        let system = DreamIntegratedMemorySystem::new(&project);
        // Should use "projects_myapp" as key (last two components)
        assert_eq!(system.project_key, "projects_myapp");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_dream_integrated_memory_system_load_consolidated() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        let project = home.join("myapp");
        let memory_base = home.join(".selfware").join("memory");

        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&memory_base).unwrap();

        // Create a consolidated memory file
        let memory_content = "# Project Memory\n\n## Facts\n- [2026-03-31] Test fact\n";
        std::fs::write(memory_base.join("home_myapp_MEMORY.md"), memory_content).unwrap();

        // Set HOME to temp directory
        let original_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &home);

        let system = DreamIntegratedMemorySystem::new(&project);
        let memory = system.load_consolidated_memory();

        // Restore HOME
        match original_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }

        assert!(memory.is_some());
        assert!(memory.unwrap().content.contains("Test fact"));
    }

    #[tokio::test]
    async fn test_dream_integrated_record_session_end() {
        use crate::cognitive::dream::DreamConfig;

        let temp = tempdir().unwrap();
        let memory_base = temp.path().join("memory");
        let project = temp.path().join("projects").join("myapp");
        std::fs::create_dir_all(&project).unwrap();

        // Create system with isolated dream config
        let dream_config = DreamConfig::new().with_base_dir(&memory_base);
        let system = DreamIntegratedMemorySystem::new(&project).with_dream_config(dream_config);

        // Record session end
        system.record_session_end().unwrap();

        // Verify state was updated
        let status = system.dream_status().await;
        assert_eq!(status.sessions_since_last_dream, 1);
    }
}
