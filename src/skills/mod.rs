//! User-extensible skill system for Selfware.
//!
//! Skills are markdown files with YAML frontmatter that define reusable
//! system prompts/instructions invoked via slash commands in the TUI.

use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

fn default_verified() -> bool {
    true
}

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
    /// Whether this skill is verified or distilled from unverified traces.
    #[serde(default = "default_verified")]
    pub verified: bool,
    /// Whether this skill is a candidate requiring admission.
    #[serde(default)]
    pub candidate: bool,
    /// Origin of the skill (e.g. "user", "distilled", "generated").
    #[serde(default)]
    pub origin: Option<String>,
    /// Whether a candidate skill has been explicitly admitted.
    #[serde(default)]
    pub admitted: bool,
    /// Source execution trace IDs.
    #[serde(default)]
    pub trace_ids: Vec<String>,
    /// Content digest for integrity verification.
    #[serde(default)]
    pub content_hash: Option<String>,
    /// Applicable task or repository scope.
    #[serde(default)]
    pub scope: Option<String>,
    /// The body of the skill (markdown content after frontmatter).
    #[serde(skip)]
    pub content: String,
    /// Source file path (for debugging).
    #[serde(skip)]
    pub source: Option<PathBuf>,
}

impl Default for Skill {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            tools: Vec::new(),
            verified: true,
            candidate: false,
            origin: None,
            admitted: false,
            trace_ids: Vec::new(),
            content_hash: None,
            scope: None,
            content: String::new(),
            source: None,
        }
    }
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

    /// Format the trust header badge for this skill.
    pub fn trust_badge(&self) -> String {
        if (self.candidate || matches!(self.origin.as_deref(), Some("distilled" | "generated")))
            && !self.admitted
        {
            format!("[Skill: {} (CANDIDATE - Unadmitted)]", self.name)
        } else if self.verified {
            format!("[Skill: {}]", self.name)
        } else {
            format!(
                "[Skill: {} (UNVERIFIED - Distilled from unverified execution trace)]",
                self.name
            )
        }
    }

    /// Render the skill body with its trust gate badge applied.
    pub fn render_with_trust_gate(&self, arguments: &str) -> String {
        let content = self.render_content(arguments);
        format!("{}\n{}", self.trust_badge(), content)
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
    /// Searches one level deep for `*.md` files. Only admitted skills
    /// are inserted into the active registry.
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
                    // Check admission barrier: candidate skills require explicit admission
                    let is_candidate = skill.candidate
                        || matches!(skill.origin.as_deref(), Some("distilled" | "generated"));
                    if is_candidate && !skill.admitted {
                        warn!(
                            "Ignoring unadmitted candidate skill '{}' in {}",
                            skill.name,
                            path.display()
                        );
                        continue;
                    }

                    // Precedence gate: generated/candidate skills cannot overwrite user-defined skills
                    if let Some(existing) = self.skills.get(&skill.name) {
                        let existing_is_user = !existing.candidate
                            && !matches!(
                                existing.origin.as_deref(),
                                Some("distilled" | "generated")
                            );
                        if existing_is_user && is_candidate {
                            warn!(
                                "Skill candidate '{}' in {} would overwrite user-defined skill; preserving user version",
                                skill.name,
                                path.display()
                            );
                            continue;
                        }
                    }

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

    /// Discover unadmitted skill candidates in the candidate directory (e.g. `.selfware/skill-candidates/`).
    pub fn discover_candidates(candidates_dir: &Path) -> Vec<Skill> {
        let mut candidates = Vec::new();
        if !candidates_dir.is_dir() {
            return candidates;
        }
        let entries = match std::fs::read_dir(candidates_dir) {
            Ok(e) => e,
            Err(_) => return candidates,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Ok(mut skill) = Skill::from_file(&path) {
                    skill.candidate = true;
                    candidates.push(skill);
                }
            }
        }
        candidates.sort_by(|a, b| a.name.cmp(&b.name));
        candidates
    }

    /// Explicit admission gate: admit a candidate skill into the target active skills directory.
    pub fn admit_candidate(
        candidate_path: &Path,
        target_skills_dir: &Path,
    ) -> Result<Skill, String> {
        if crate::safety::killswitch::is_killswitch_active() {
            return Err("Killswitch is active: candidate admission blocked".to_string());
        }

        let mut skill = Skill::from_file(candidate_path)?;

        std::fs::create_dir_all(target_skills_dir)
            .map_err(|e| format!("Failed to create target skills dir: {e}"))?;

        let target_file = target_skills_dir.join(format!("{}.md", skill.name));
        if target_file.exists() {
            let existing = Skill::from_file(&target_file)?;
            let existing_is_user = !existing.candidate
                && !matches!(existing.origin.as_deref(), Some("distilled" | "generated"));
            if existing_is_user {
                return Err(format!(
                    "Cannot admit candidate '{}': shadows existing user skill",
                    skill.name
                ));
            }
        }

        skill.candidate = true;
        skill.admitted = true;
        let content_hash = format!("{:x}", sha2::Sha256::digest(skill.content.as_bytes()));
        skill.content_hash = Some(content_hash);

        // Format updated markdown frontmatter
        let mut frontmatter_map = serde_yaml::Mapping::new();
        frontmatter_map.insert(
            serde_yaml::Value::String("name".to_string()),
            serde_yaml::Value::String(skill.name.clone()),
        );
        frontmatter_map.insert(
            serde_yaml::Value::String("description".to_string()),
            serde_yaml::Value::String(skill.description.clone()),
        );
        if !skill.tools.is_empty() {
            let tools_val: Vec<serde_yaml::Value> = skill
                .tools
                .iter()
                .map(|t| serde_yaml::Value::String(t.clone()))
                .collect();
            frontmatter_map.insert(
                serde_yaml::Value::String("tools".to_string()),
                serde_yaml::Value::Sequence(tools_val),
            );
        }
        frontmatter_map.insert(
            serde_yaml::Value::String("verified".to_string()),
            serde_yaml::Value::Bool(skill.verified),
        );
        frontmatter_map.insert(
            serde_yaml::Value::String("candidate".to_string()),
            serde_yaml::Value::Bool(true),
        );
        frontmatter_map.insert(
            serde_yaml::Value::String("admitted".to_string()),
            serde_yaml::Value::Bool(true),
        );
        if let Some(ref orig) = skill.origin {
            frontmatter_map.insert(
                serde_yaml::Value::String("origin".to_string()),
                serde_yaml::Value::String(orig.clone()),
            );
        }
        if let Some(ref ch) = skill.content_hash {
            frontmatter_map.insert(
                serde_yaml::Value::String("content_hash".to_string()),
                serde_yaml::Value::String(ch.clone()),
            );
        }

        let yaml = serde_yaml::to_string(&frontmatter_map)
            .map_err(|e| format!("Failed to serialize admitted frontmatter: {e}"))?;
        let rendered = format!("---\n{}---\n\n{}", yaml, skill.content);
        std::fs::write(&target_file, rendered)
            .map_err(|e| format!("Failed to write admitted skill: {e}"))?;

        skill.source = Some(target_file);
        Ok(skill)
    }

    /// Get a skill by name. If the killswitch is active, candidate skills are blocked.
    pub fn get(&self, name: &str) -> Option<&Skill> {
        let skill = self.skills.get(name)?;
        if (skill.candidate || matches!(skill.origin.as_deref(), Some("distilled" | "generated")))
            && crate::safety::killswitch::is_killswitch_active()
        {
            warn!("Killswitch active; blocking candidate skill '{}'", name);
            return None;
        }
        Some(skill)
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
        self.get(skill_name)
            .map(|skill| format!("{}\n\n[Task]\n{}", skill.render_with_trust_gate(""), task))
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
