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
}

impl DreamIntegratedMemorySystem {
    /// Create a new dream-integrated memory system.
    pub fn new(project_path: &Path) -> Self {
        let project_key = MemorySystem::project_key_from_path(project_path);

        Self {
            project_key,
            dream_config: crate::cognitive::dream::DreamConfig::new(),
            // Use the user's OWN configured backend for consolidation — the
            // hardcoded default endpoint/model made `/dream force` 404 against
            // localhost:8000 while reporting success.
            auto_dream_config:
                crate::cognitive::dream_subprocess::AutoDreamConfig::from_user_config(),
        }
    }

    /// Create with custom dream configuration.
    pub fn with_dream_config(mut self, config: crate::cognitive::dream::DreamConfig) -> Self {
        self.dream_config = config;
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
#[path = "../../tests/unit/cognitive/memory_system/memory_system_test.rs"]
mod tests;
