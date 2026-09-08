//! User-extensible skill system for Selfware.
//!
//! Skills are markdown files with YAML frontmatter that define reusable
//! system prompts/instructions invoked via slash commands in the TUI.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// A skill loaded from a markdown file with YAML frontmatter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Skill {
    /// Short machine-friendly name (used as `/name` command).
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Optional list of tool names the skill may use.
    #[serde(default)]
    pub tools: Vec<String>,
    /// The body of the skill (markdown content after frontmatter).
    #[serde(skip)]
    pub content: String,
    /// Source file path (for debugging).
    #[serde(skip)]
    pub source: Option<PathBuf>,
}

impl Skill {
    /// Parse a skill from a markdown string with YAML frontmatter.
    ///
    /// Expected format:
    /// ```markdown
    /// ---
    /// name: commit
    /// description: Create a git commit
    /// tools: [bash, file_read]
    /// ---
    /// Create a git commit with the staged changes...
    /// ```
    pub fn from_markdown(source: &str) -> Result<Self, String> {
        let trimmed = source.trim_start();
        if !trimmed.starts_with("---") {
            return Err("Missing YAML frontmatter".to_string());
        }

        // Find the end of the frontmatter
        let after_open = &trimmed[3..];
        let Some(end_idx) = after_open.find("\n---") else {
            return Err("Unclosed YAML frontmatter".to_string());
        };

        let yaml_text = &after_open[..end_idx].trim();
        let content = after_open[end_idx + 4..].trim_start().to_string();

        let mut skill: Skill = serde_yaml::from_str(yaml_text)
            .map_err(|e| format!("Invalid YAML frontmatter: {e}"))?;
        skill.content = content;

        if skill.name.is_empty() {
            return Err("Skill name cannot be empty".to_string());
        }

        Ok(skill)
    }

    /// Load a skill from a file path.
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let source =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read file: {e}"))?;
        let mut skill = Self::from_markdown(&source)?;
        skill.source = Some(path.to_path_buf());
        Ok(skill)
    }

    /// Render the skill body for an invocation: `$ARGUMENTS` substituted
    /// when the template names it, otherwise arguments are appended (the
    /// Claude Code commands convention).
    pub fn render_content(&self, arguments: &str) -> String {
        let arguments = arguments.trim();
        if self.content.contains("$ARGUMENTS") {
            self.content.replace("$ARGUMENTS", arguments)
        } else if arguments.is_empty() {
            self.content.clone()
        } else {
            format!("{}\n\nArguments: {arguments}", self.content)
        }
    }
}

/// Registry of discovered skills.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
}

impl SkillRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Discover skills in the given directory.
    ///
    /// Searches one level deep for `*.md` files.
    pub fn discover_dir(&mut self, dir: &Path) {
        if !dir.is_dir() {
            debug!("Skill directory does not exist: {}", dir.display());
            return;
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                warn!("Failed to read skill directory {}: {e}", dir.display());
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }

            match Skill::from_file(&path) {
                Ok(skill) => {
                    debug!("Discovered skill '{}' from {}", skill.name, path.display());
                    self.skills.insert(skill.name.clone(), skill);
                }
                Err(e) => {
                    warn!("Failed to load skill from {}: {e}", path.display());
                }
            }
        }
    }

    /// Discover skills in the standard locations:
    /// - `~/.selfware/skills/` and `~/.selfware/commands/`
    /// - `./.selfware/skills/` and `./.selfware/commands/`
    ///
    /// (`commands/` is the Claude-Code-parity alias — same markdown +
    /// frontmatter format, not a second template system.)
    pub fn discover() -> Self {
        let mut registry = Self::new();

        // User-global skills
        if let Some(home) = dirs::home_dir() {
            registry.discover_dir(&home.join(".selfware").join("skills"));
            registry.discover_dir(&home.join(".selfware").join("commands"));
        }

        // Project-local skills
        if let Ok(cwd) = std::env::current_dir() {
            registry.discover_dir(&cwd.join(".selfware").join("skills"));
            registry.discover_dir(&cwd.join(".selfware").join("commands"));
        }

        registry
    }

    /// Get a skill by name.
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    /// Return all discovered skills, sorted by name.
    pub fn list(&self) -> Vec<&Skill> {
        let mut skills: Vec<_> = self.skills.values().collect();
        skills.sort_by_key(|s| &s.name);
        skills
    }

    /// Wrap a task string with the named skill's instructions, for headless
    /// `run --skill`. Returns `None` when the skill is unknown.
    pub fn wrap_task_with_skill(&self, task: &str, skill_name: &str) -> Option<String> {
        self.get(skill_name).map(|skill| {
            format!(
                "[Skill: {}]\n{}\n\n[Task]\n{}",
                skill.name, skill.content, task
            )
        })
    }

    /// Number of discovered skills.
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/skills/mod_test.rs"]
mod tests;
