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
    false
}

/// A skill loaded from a markdown file with YAML frontmatter.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
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
    /// Markdown body content (instructions).
    #[serde(skip)]
    pub content: String,
    /// Source file path (for debugging).
    #[serde(skip)]
    pub source: Option<PathBuf>,
}

/// Entry in the external admission ledger tracking provenance and cryptographic integrity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdmittedSkillEntry {
    pub name: String,
    #[serde(default)]
    pub file_name: String,
    pub content_hash: String,
    #[serde(default)]
    pub metadata_hash: Option<String>,
    pub admitted_at: u64,
    pub source_origin: Option<String>,
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

/// Compute canonical SHA-256 hash of trust-critical metadata.
pub fn compute_metadata_hash(
    description: &str,
    tools: &[String],
    scope: Option<&str>,
    origin: Option<&str>,
) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(description.trim().as_bytes());
    hasher.update(b"\0");
    let mut sorted_tools = tools.to_vec();
    sorted_tools.sort();
    for tool in &sorted_tools {
        hasher.update(tool.trim().as_bytes());
        hasher.update(b",");
    }
    hasher.update(b"\0");
    if let Some(s) = scope {
        hasher.update(s.trim().as_bytes());
    }
    hasher.update(b"\0");
    if let Some(o) = origin {
        hasher.update(o.trim().as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

/// External ledger of admitted skills, stored separately from the skill markdown files.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdmissionLedger {
    #[serde(default)]
    pub entries: HashMap<String, AdmittedSkillEntry>,
}

impl AdmissionLedger {
    pub const FILE_NAME: &'static str = ".admitted_ledger.json";

    pub fn load_from_dir(dir: &Path) -> Result<Self, String> {
        let ledger_path = dir.join(Self::FILE_NAME);
        match ledger_path.symlink_metadata() {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err(format!(
                        "Admission ledger is a symlink: {}",
                        ledger_path.display()
                    ));
                }
                if !meta.file_type().is_file() {
                    return Err(format!(
                        "Admission ledger is not a regular file: {}",
                        ledger_path.display()
                    ));
                }
                use std::io::Read;
                let file = std::fs::File::open(&ledger_path).map_err(|e| {
                    format!(
                        "Failed to open admission ledger {}: {e}",
                        ledger_path.display()
                    )
                })?;
                let mut content = String::new();
                file.take(1_048_576)
                    .read_to_string(&mut content)
                    .map_err(|e| {
                        format!(
                            "Failed to read admission ledger {}: {e}",
                            ledger_path.display()
                        )
                    })?;
                serde_json::from_str(&content).map_err(|e| {
                    format!(
                        "Admission ledger {} is corrupt (invalid JSON): {e} — failing closed to prevent tamper bypass",
                        ledger_path.display()
                    )
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!(
                "Cannot access admission ledger {}: {e}",
                ledger_path.display()
            )),
        }
    }

    pub fn save_to_dir(&self, dir: &Path) -> Result<(), String> {
        let ledger_path = dir.join(Self::FILE_NAME);
        if !ledger_path.starts_with(dir) {
            return Err(format!(
                "Admission ledger path escapes target directory: {}",
                ledger_path.display()
            ));
        }
        if let Ok(meta) = ledger_path.symlink_metadata() {
            if meta.file_type().is_symlink() {
                return Err(format!(
                    "Admission ledger target is a symlink: {}",
                    ledger_path.display()
                ));
            }
            if !meta.file_type().is_file() {
                return Err(format!(
                    "Admission ledger target is not a regular file: {}",
                    ledger_path.display()
                ));
            }
            if meta.permissions().readonly() {
                return Err(format!(
                    "Admission ledger target is read-only: {}",
                    ledger_path.display()
                ));
            }
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize admission ledger: {e}"))?;

        let temp_path = dir.join(format!(".admitted_ledger.tmp.{}", uuid::Uuid::new_v4()));
        let write_res = (|| -> std::io::Result<()> {
            let mut open_opts = std::fs::OpenOptions::new();
            open_opts.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                open_opts.custom_flags(libc::O_NOFOLLOW);
            }
            use std::io::Write;
            let mut file = open_opts.open(&temp_path)?;
            file.write_all(json.as_bytes())?;
            file.sync_all()?;
            std::fs::rename(&temp_path, &ledger_path)?;
            Ok(())
        })();

        if let Err(e) = write_res {
            let _ = std::fs::remove_file(&temp_path);
            return Err(format!(
                "Failed to save admission ledger to {}: {e}",
                ledger_path.display()
            ));
        }
        Ok(())
    }
}

/// Validate that a skill name contains only safe alphanumeric characters and no path traversal sequences.
pub fn validate_skill_name(name: &str) -> Result<String, String> {
    if name.is_empty() {
        return Err("Skill name cannot be empty".to_string());
    }
    if name.starts_with('.')
        || name.contains("..")
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
    {
        return Err(format!(
            "Skill name contains invalid path traversal or special characters: '{name}'"
        ));
    }
    let safe_name: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if safe_name != name {
        return Err(format!(
            "Skill name '{name}' contains disallowed characters (only a-z, A-Z, 0-9, _, - permitted)"
        ));
    }
    Ok(safe_name)
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
        let meta = path
            .symlink_metadata()
            .map_err(|e| format!("Failed to read metadata for {}: {e}", path.display()))?;
        if meta.file_type().is_symlink() {
            return Err(format!(
                "Skill file cannot be a symlink: {}",
                path.display()
            ));
        }
        if !meta.file_type().is_file() {
            return Err(format!(
                "Skill file must be a regular file: {}",
                path.display()
            ));
        }
        use std::io::Read;
        let file = std::fs::File::open(path).map_err(|e| format!("Failed to open file: {e}"))?;
        let mut source = String::new();
        file.take(1_048_576)
            .read_to_string(&mut source)
            .map_err(|e| format!("Failed to read file: {e}"))?;
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
        } else if self.candidate
            || self.admitted
            || matches!(self.origin.as_deref(), Some("distilled" | "generated"))
        {
            format!(
                "[Skill: {} (UNVERIFIED - Distilled from unverified execution trace)]",
                self.name
            )
        } else {
            // Hand-written user-authored skill: not distilled from execution traces
            format!("[Skill: {}]", self.name)
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

/// A skill or command file that was refused during discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusedSkill {
    pub name: String,
    pub path: PathBuf,
    pub reason: String,
}

/// Registry of discovered skills.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
    refused: Vec<RefusedSkill>,
}

impl SkillRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Refused skills that failed admission or validation during discovery.
    pub fn refused(&self) -> &[RefusedSkill] {
        &self.refused
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

        let ledger = match AdmissionLedger::load_from_dir(dir) {
            Ok(l) => l,
            Err(e) => {
                warn!(
                    "Refusing to discover skills from {} (admission ledger error): {e}",
                    dir.display()
                );
                return;
            }
        };

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
                Ok(mut skill) => {
                    let file_name_str = path
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or_default();

                    let actual_hash =
                        format!("{:x}", sha2::Sha256::digest(skill.content.as_bytes()));

                    // Consult ledger by skill name OR file name
                    let ledger_entry = ledger.entries.get(&skill.name).or_else(|| {
                        ledger
                            .entries
                            .values()
                            .find(|e| !e.file_name.is_empty() && e.file_name == file_name_str)
                    });

                    if let Some(entry) = ledger_entry {
                        // This skill is registered in the ledger!
                        // Content hash verification:
                        if actual_hash != entry.content_hash {
                            warn!(
                                "Ignoring admitted skill '{}' in {}: content hash mismatch (expected {}, computed {}) — candidate tampered after admission",
                                skill.name,
                                path.display(),
                                entry.content_hash,
                                actual_hash
                            );
                            self.refused.push(RefusedSkill {
                                name: skill.name.clone(),
                                path: path.clone(),
                                reason: "content hash mismatch (file modified after admission)"
                                    .to_string(),
                            });
                            continue;
                        }

                        // Metadata hash verification:
                        if let Some(ref expected_meta_hash) = entry.metadata_hash {
                            let actual_meta_hash = compute_metadata_hash(
                                &skill.description,
                                &skill.tools,
                                skill.scope.as_deref(),
                                skill.origin.as_deref(),
                            );
                            if &actual_meta_hash != expected_meta_hash {
                                warn!(
                                    "Ignoring admitted skill '{}' in {}: metadata hash mismatch (expected {}, computed {}) — metadata tampered after admission",
                                    skill.name,
                                    path.display(),
                                    expected_meta_hash,
                                    actual_meta_hash
                                );
                                self.refused.push(RefusedSkill {
                                    name: skill.name.clone(),
                                    path: path.clone(),
                                    reason:
                                        "metadata hash mismatch (metadata tampered after admission)"
                                            .to_string(),
                                });
                                continue;
                            }
                        }

                        // Derive trust and provenance from the protected ledger record, NOT untrusted markdown
                        skill.candidate = true;
                        skill.admitted = true;
                        skill.verified = entry.verified;
                        if !entry.tools.is_empty() {
                            skill.tools = entry.tools.clone();
                        }
                        if entry.source_origin.is_some() {
                            skill.origin = entry.source_origin.clone();
                        }
                        if entry.scope.is_some() {
                            skill.scope = entry.scope.clone();
                        }
                    } else {
                        // Project discovery directory is the untrusted class:
                        // ANY skill or command in this directory without an entry in
                        // .admitted_ledger.json is rejected! A file cannot self-admit by
                        // setting or omitting frontmatter flags (structural trust).
                        warn!(
                            "Ignoring unadmitted skill '{}' in {}: no entry in admission ledger",
                            skill.name,
                            path.display()
                        );
                        self.refused.push(RefusedSkill {
                            name: skill.name.clone(),
                            path: path.clone(),
                            reason: "missing from .admitted_ledger.json".to_string(),
                        });
                        continue;
                    }

                    // Precedence gate: generated/candidate skills cannot overwrite user-defined skills
                    let is_skill_candidate = skill.candidate
                        || skill.admitted
                        || matches!(skill.origin.as_deref(), Some("distilled" | "generated"));

                    if let Some(existing) = self.skills.get(&skill.name) {
                        let existing_is_user = !existing.candidate
                            && !existing.admitted
                            && !matches!(
                                existing.origin.as_deref(),
                                Some("distilled" | "generated")
                            );
                        if existing_is_user && is_skill_candidate {
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

    /// Discover user-authored skills in the given directory (e.g. `~/.selfware/skills/`).
    ///
    /// User skills in home directories are trusted user tools and do not require an
    /// admission ledger, but any candidate or generated file claiming admission
    /// must still be verified against a ledger if present.
    pub fn discover_user_dir(&mut self, dir: &Path) {
        if !dir.is_dir() {
            debug!("User skill directory does not exist: {}", dir.display());
            return;
        }

        let ledger = match AdmissionLedger::load_from_dir(dir) {
            Ok(l) => Some(l),
            Err(e) => {
                warn!(
                    "Refusing to discover user skills from {} (admission ledger error): {e}",
                    dir.display()
                );
                return;
            }
        };

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                warn!("Failed to read user skill directory {}: {e}", dir.display());
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }

            match Skill::from_file(&path) {
                Ok(mut skill) => {
                    let file_name_str = path
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or_default();

                    // Consult ledger FIRST: check if file has known admission provenance
                    // even if markdown flags (candidate, origin, admitted) were stripped.
                    let ledger_entry = ledger.as_ref().and_then(|l| {
                        l.entries.get(&skill.name).or_else(|| {
                            l.entries
                                .values()
                                .find(|e| !e.file_name.is_empty() && e.file_name == file_name_str)
                        })
                    });

                    let is_candidate = ledger_entry.is_some()
                        || skill.candidate
                        || matches!(skill.origin.as_deref(), Some("distilled" | "generated"))
                        || skill.admitted;

                    if is_candidate {
                        if let Some(entry) = ledger_entry {
                            let actual_hash =
                                format!("{:x}", sha2::Sha256::digest(skill.content.as_bytes()));
                            if actual_hash != entry.content_hash {
                                warn!(
                                    "Ignoring admitted candidate skill '{}' in user dir {}: content hash mismatch",
                                    skill.name,
                                    path.display()
                                );
                                self.refused.push(RefusedSkill {
                                    name: skill.name.clone(),
                                    path: path.clone(),
                                    reason: "content hash mismatch".to_string(),
                                });
                                continue;
                            }

                            // Metadata hash verification:
                            if let Some(ref expected_meta_hash) = entry.metadata_hash {
                                let actual_meta_hash = compute_metadata_hash(
                                    &skill.description,
                                    &skill.tools,
                                    skill.scope.as_deref(),
                                    skill.origin.as_deref(),
                                );
                                if &actual_meta_hash != expected_meta_hash {
                                    warn!(
                                        "Ignoring admitted candidate skill '{}' in user dir {}: metadata hash mismatch (expected {}, computed {}) — metadata tampered after admission",
                                        skill.name,
                                        path.display(),
                                        expected_meta_hash,
                                        actual_meta_hash
                                    );
                                    self.refused.push(RefusedSkill {
                                        name: skill.name.clone(),
                                        path: path.clone(),
                                        reason:
                                            "metadata hash mismatch (metadata tampered after admission)"
                                                .to_string(),
                                    });
                                    continue;
                                }
                            }

                            skill.candidate = true;
                            skill.admitted = true;
                            skill.verified = entry.verified;
                        } else {
                            warn!(
                                "Ignoring unadmitted candidate skill '{}' in user directory {}: no entry in admission ledger",
                                skill.name,
                                path.display()
                            );
                            self.refused.push(RefusedSkill {
                                name: skill.name.clone(),
                                path: path.clone(),
                                reason:
                                    "unadmitted candidate in user directory missing ledger entry"
                                        .to_string(),
                            });
                            continue;
                        }
                    } else {
                        // User-authored skill: trusted, not a machine candidate
                        skill.candidate = false;
                        skill.admitted = false;
                        skill.verified = false;
                    }

                    debug!(
                        "Discovered user skill '{}' from {}",
                        skill.name,
                        path.display()
                    );
                    self.skills.insert(skill.name.clone(), skill);
                }
                Err(e) => {
                    warn!("Failed to load user skill from {}: {e}", path.display());
                }
            }
        }
    }

    /// Discover skills in the standard locations:
    /// - `~/.selfware/skills/` and `~/.selfware/commands/` (user-global)
    /// - `./.selfware/skills/` and `./.selfware/commands/` (project-local, ledger-gated)
    ///
    /// (`commands/` is the Claude-Code-parity alias — same markdown +
    /// frontmatter format, not a second template system.)
    pub fn discover() -> Self {
        let mut registry = Self::new();

        // User-global skills
        if let Some(home) = dirs::home_dir() {
            registry.discover_user_dir(&home.join(".selfware").join("skills"));
            registry.discover_user_dir(&home.join(".selfware").join("commands"));
        }

        // Project-local skills
        if let Ok(cwd) = std::env::current_dir() {
            registry.discover_dir(&cwd.join(".selfware").join("skills"));
            registry.discover_dir(&cwd.join(".selfware").join("commands"));
        }

        registry
    }

    /// Detect any skill or command `.md` files in a project discovery directory that lack
    /// an admission ledger entry, returning their paths for actionable operator diagnostics.
    pub fn detect_unadmitted_in_dir(dir: &Path) -> Vec<PathBuf> {
        let mut unadmitted = Vec::new();
        if !dir.is_dir() {
            return unadmitted;
        }

        let ledger = AdmissionLedger::load_from_dir(dir).unwrap_or_default();
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return unadmitted,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }

            let file_name_str = path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or_default();

            if let Ok(skill) = Skill::from_file(&path) {
                let has_ledger_entry = ledger.entries.contains_key(&skill.name)
                    || ledger
                        .entries
                        .values()
                        .any(|e| !e.file_name.is_empty() && e.file_name == file_name_str);
                if !has_ledger_entry {
                    unadmitted.push(path);
                }
            } else {
                unadmitted.push(path);
            }
        }
        unadmitted.sort();
        unadmitted
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

    /// Validate whether a path matches any denied pattern (credentials, git internals, killswitch, system dirs, or operator-configured denylist).
    /// Prevents plant-then-admit attacks from using sensitive directories or files as candidates or targets.
    fn is_admission_denied(path: &Path, extra_denied: Option<&[String]>) -> Option<String> {
        let path_str = path.to_string_lossy().replace('\\', "/");
        let canonical = std::fs::canonicalize(path).ok();
        let canonical_str = canonical
            .as_ref()
            .map(|p| p.to_string_lossy().replace('\\', "/"));

        let mut denied = crate::config::default_denied_paths();
        if let Some(extras) = extra_denied {
            denied.extend(extras.iter().cloned());
        }
        for pattern in &denied {
            // Exclude allowed skill directories from the denylist for admission
            if pattern.contains(".selfware/skills")
                || pattern.contains(".selfware/skill-candidates")
            {
                continue;
            }
            let pattern_glob = crate::safety::checker::to_glob_form(pattern);
            if let Ok(glob) = glob::Pattern::new(&pattern_glob) {
                if glob.matches(&path_str) {
                    return Some(pattern.clone());
                }
                if let Some(ref c_str) = canonical_str {
                    if glob.matches(c_str) {
                        return Some(pattern.clone());
                    }
                }
                if !pattern.contains('/') && !pattern.contains('\\') {
                    for comp in path.components() {
                        if let std::path::Component::Normal(name) = comp {
                            if glob.matches(&name.to_string_lossy()) {
                                return Some(pattern.clone());
                            }
                        }
                    }
                }
            }
        }
        for comp in path.components() {
            if let std::path::Component::Normal(name) = comp {
                if name == ".git" {
                    return Some(".git/**".to_string());
                }
            }
        }
        let sys_prefixes = ["/etc", "/root", "/proc", "/sys", "/dev"];
        for prefix in sys_prefixes {
            if path_str.starts_with(prefix) {
                return Some(format!("{prefix}/**"));
            }
            if let Some(ref c_str) = canonical_str {
                if c_str.starts_with(prefix) {
                    return Some(format!("{prefix}/**"));
                }
            }
        }
        None
    }

    /// Explicit admission gate: admit a candidate skill into the target active skills directory.
    pub fn admit_candidate(
        candidate_path: &Path,
        target_skills_dir: &Path,
    ) -> Result<Skill, String> {
        Self::admit_candidate_with_denied(candidate_path, target_skills_dir, None)
    }

    /// Explicit admission gate with operator-configured denied paths.
    pub fn admit_candidate_with_denied(
        candidate_path: &Path,
        target_skills_dir: &Path,
        extra_denied: Option<&[String]>,
    ) -> Result<Skill, String> {
        static ADMISSION_MUTEX: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
        let _guard = ADMISSION_MUTEX.lock();

        if crate::safety::killswitch::is_killswitch_active() {
            return Err("Killswitch is active: candidate admission blocked".to_string());
        }

        if let Some(pat) = Self::is_admission_denied(candidate_path, extra_denied) {
            return Err(format!("Candidate path matches denied pattern: {pat}"));
        }
        if let Some(pat) = Self::is_admission_denied(target_skills_dir, extra_denied) {
            return Err(format!(
                "Target skills directory matches denied pattern: {pat}"
            ));
        }

        let mut skill = Skill::from_file(candidate_path)?;
        let safe_name = validate_skill_name(&skill.name)?;

        std::fs::create_dir_all(target_skills_dir)
            .map_err(|e| format!("Failed to create target skills dir: {e}"))?;

        let target_file = target_skills_dir.join(format!("{safe_name}.md"));
        if let Some(pat) = Self::is_admission_denied(&target_file, extra_denied) {
            return Err(format!("Target file matches denied pattern: {pat}"));
        }
        if !target_file.starts_with(target_skills_dir) {
            return Err(format!(
                "Destination path escapes skills directory: {}",
                target_file.display()
            ));
        }
        if let Ok(meta) = target_file.symlink_metadata() {
            if meta.file_type().is_symlink() {
                return Err(format!(
                    "Destination file is a symlink: {}",
                    target_file.display()
                ));
            }
        }

        let is_self_admission = target_file == candidate_path
            || std::fs::canonicalize(&target_file).ok()
                == std::fs::canonicalize(candidate_path).ok();
        let previous_file_content: Option<Vec<u8>> = if target_file.exists() {
            let existing = Skill::from_file(&target_file)?;
            let existing_is_user = !existing.candidate
                && !existing.admitted
                && !matches!(existing.origin.as_deref(), Some("distilled" | "generated"));
            if existing_is_user && !is_self_admission {
                return Err(format!(
                    "Cannot admit candidate '{}': shadows existing user skill",
                    safe_name
                ));
            }
            Some(
                std::fs::read(&target_file)
                    .map_err(|e| format!("Failed to backup existing skill file: {e}"))?,
            )
        } else {
            None
        };

        // 1. Validate and load ledger FIRST before any file modifications.
        // A corrupt or unreadable ledger fails fast, preserving existing file state untouched.
        let mut ledger = AdmissionLedger::load_from_dir(target_skills_dir)?;

        skill.name = safe_name.clone();
        skill.candidate = true;
        skill.admitted = true;
        skill.verified = false;
        let content_hash = format!("{:x}", sha2::Sha256::digest(skill.content.as_bytes()));
        skill.content_hash = Some(content_hash.clone());
        let metadata_hash = compute_metadata_hash(
            &skill.description,
            &skill.tools,
            skill.scope.as_deref(),
            skill.origin.as_deref(),
        );

        if !is_self_admission {
            // Format updated markdown frontmatter preserving all metadata (including scope & trace_ids)
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
            if let Some(ref sc) = skill.scope {
                frontmatter_map.insert(
                    serde_yaml::Value::String("scope".to_string()),
                    serde_yaml::Value::String(sc.clone()),
                );
            }
            if !skill.trace_ids.is_empty() {
                let trace_val: Vec<serde_yaml::Value> = skill
                    .trace_ids
                    .iter()
                    .map(|t| serde_yaml::Value::String(t.clone()))
                    .collect();
                frontmatter_map.insert(
                    serde_yaml::Value::String("trace_ids".to_string()),
                    serde_yaml::Value::Sequence(trace_val),
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

            // 2. Stage candidate file write via temporary file + atomic rename
            let temp_file_path =
                target_skills_dir.join(format!(".{safe_name}.md.tmp.{}", uuid::Uuid::new_v4()));
            let write_res = (|| -> std::io::Result<()> {
                let mut open_opts = std::fs::OpenOptions::new();
                open_opts.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    open_opts.custom_flags(libc::O_NOFOLLOW);
                }
                use std::io::Write;
                let mut file = open_opts.open(&temp_file_path)?;
                file.write_all(rendered.as_bytes())?;
                file.sync_all()?;
                std::fs::rename(&temp_file_path, &target_file)?;
                Ok(())
            })();

            if let Err(e) = write_res {
                let _ = std::fs::remove_file(&temp_file_path);
                return Err(format!("Failed to stage admitted skill file: {e}"));
            }
        }

        // 3. Update and persist external admission ledger
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let file_name = format!("{safe_name}.md");
        ledger.entries.insert(
            safe_name.clone(),
            AdmittedSkillEntry {
                name: safe_name.clone(),
                file_name,
                content_hash: content_hash.clone(),
                metadata_hash: Some(metadata_hash),
                admitted_at: now,
                source_origin: skill.origin.clone(),
                verified: skill.verified,
                tools: skill.tools.clone(),
                scope: skill.scope.clone(),
            },
        );

        if let Err(e) = ledger.save_to_dir(target_skills_dir) {
            // TRANSACTIONAL ROLLBACK: restore previous file content (or delete if newly created)
            let rollback_err = match previous_file_content {
                Some(prev_bytes) => std::fs::write(&target_file, prev_bytes).err(),
                None => std::fs::remove_file(&target_file).err(),
            };
            if let Some(r_err) = rollback_err {
                return Err(format!(
                    "Failed to persist admission ledger: {e}; ROLLBACK FAILED: {r_err}"
                ));
            }
            return Err(format!(
                "Failed to persist admission ledger: {e} (previous skill version preserved)"
            ));
        }

        skill.source = Some(target_file);
        Ok(skill)
    }

    /// Get a skill by name. If the killswitch is active, all skills are blocked.
    pub fn get(&self, name: &str) -> Option<&Skill> {
        if crate::safety::killswitch::is_killswitch_active() {
            warn!("Killswitch active; blocking skill retrieval for '{name}'");
            return None;
        }
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
