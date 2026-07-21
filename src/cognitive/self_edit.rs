//! Self-Edit Orchestration
//!
//! Enables the agent to analyze its own codebase, identify improvement targets,
//! and safely apply edits with verification and rollback.

use anyhow::{anyhow, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::metrics::{MetricsStore, PerformanceSnapshot};
use crate::cognitive::compilation_manager::CompilationSandbox;

/// Source of an improvement target
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ImprovementSource {
    /// Detected code smell
    CodeSmell,
    /// Recurring error pattern
    ErrorPattern,
    /// Metrics regression
    MetricsRegression,
    /// Technical debt scan
    TechDebt,
    /// LLM reflection during execution
    LLMReflection,
}

/// Category of improvement
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ImprovementCategory {
    PromptTemplate,
    ToolPipeline,
    ErrorHandling,
    VerificationLogic,
    ContextManagement,
    CodeQuality,
    NewCapability,
}

impl std::fmt::Display for ImprovementCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PromptTemplate => write!(f, "prompt_template"),
            Self::ToolPipeline => write!(f, "tool_pipeline"),
            Self::ErrorHandling => write!(f, "error_handling"),
            Self::VerificationLogic => write!(f, "verification_logic"),
            Self::ContextManagement => write!(f, "context_management"),
            Self::CodeQuality => write!(f, "code_quality"),
            Self::NewCapability => write!(f, "new_capability"),
        }
    }
}

/// Status of an improvement target
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ImprovementStatus {
    Proposed,
    Approved,
    InProgress,
    Verified,
    RolledBack,
    Failed,
}

/// An identified improvement target
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImprovementTarget {
    pub id: String,
    pub category: ImprovementCategory,
    /// Priority = impact * confidence
    pub priority: f64,
    pub impact: f64,
    pub confidence: f64,
    pub file: Option<String>,
    pub description: String,
    pub rationale: String,
    pub source: ImprovementSource,
    pub status: ImprovementStatus,
    pub created_at: u64,
}

impl ImprovementTarget {
    pub fn new(
        category: ImprovementCategory,
        description: impl Into<String>,
        rationale: impl Into<String>,
        source: ImprovementSource,
    ) -> Self {
        let id = format!(
            "imp-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        Self {
            id,
            category,
            priority: 0.0,
            impact: 0.5,
            confidence: 0.5,
            file: None,
            description: description.into(),
            rationale: rationale.into(),
            source,
            status: ImprovementStatus::Proposed,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    pub fn with_scores(mut self, impact: f64, confidence: f64) -> Self {
        self.impact = impact.clamp(0.0, 1.0);
        self.confidence = confidence.clamp(0.0, 1.0);
        self.priority = self.impact * self.confidence;
        self
    }
}

/// A self-editing session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfEditSession {
    pub session_id: String,
    pub target_id: String,
    pub git_branch: String,
    pub checkpoint_commit: Option<String>,
    pub edits_made: Vec<String>,
    pub verification_passed: bool,
    pub status: ImprovementStatus,
    pub started_at: u64,
    pub completed_at: Option<u64>,
}

/// Record of a completed improvement attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImprovementRecord {
    pub target_id: String,
    pub category: ImprovementCategory,
    pub description: String,
    pub before_metrics: Option<PerformanceSnapshot>,
    pub after_metrics: Option<PerformanceSnapshot>,
    pub git_commits: Vec<String>,
    pub verified: bool,
    pub rolled_back: bool,
    pub effectiveness_score: f64,
    pub completed_at: u64,
}

/// Result of applying a concrete self-edit mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedMutation {
    /// Relative project files edited in the sandbox.
    pub edited_files: Vec<String>,
    /// Human-readable summary of the applied mutation.
    pub summary: String,
}

/// Files and patterns that must never be self-edited
const DENY_LIST: &[&str] = &[
    "safety/checker.rs",
    "safety/path_validator.rs",
    "Cargo.toml",
    ".github/workflows/",
    "src/main.rs",
];

/// Orchestrates the self-improvement loop
pub struct SelfEditOrchestrator {
    /// History of improvement attempts
    history: Vec<ImprovementRecord>,
    /// Path to persisted history
    history_path: PathBuf,
    /// Project root
    project_root: PathBuf,
}

impl SelfEditOrchestrator {
    pub fn new(project_root: PathBuf) -> Self {
        let history_path = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("selfware")
            .join("improvements")
            .join("history.json");

        let history = Self::load_history(&history_path).unwrap_or_default();

        Self {
            history,
            history_path,
            project_root,
        }
    }

    /// Create with a custom history path (for testing)
    #[cfg(test)]
    pub fn with_history_path(project_root: PathBuf, history_path: PathBuf) -> Self {
        let history = Self::load_history(&history_path).unwrap_or_default();
        Self {
            history,
            history_path,
            project_root,
        }
    }

    /// Introspect past performance to identify systemic weaknesses
    pub fn introspect_performance(&self) -> Vec<ImprovementTarget> {
        let snapshots = MetricsStore::new().trend(12).unwrap_or_default();
        self.introspect_performance_from_snapshots(&snapshots)
    }

    fn introspect_performance_from_snapshots(
        &self,
        snapshots: &[PerformanceSnapshot],
    ) -> Vec<ImprovementTarget> {
        let mut targets = Vec::new();

        if snapshots.is_empty() {
            return targets;
        }

        let latest = snapshots.last().expect("checked non-empty");

        let recent_count = snapshots.len().min(5);
        let recent = &snapshots[snapshots.len() - recent_count..];
        let previous = if snapshots.len() > recent_count {
            let prev_count = recent_count.min(snapshots.len() - recent_count);
            Some(
                &snapshots
                    [snapshots.len() - recent_count - prev_count..snapshots.len() - recent_count],
            )
        } else {
            None
        };

        let avg = |set: &[PerformanceSnapshot], f: fn(&PerformanceSnapshot) -> f64| -> f64 {
            set.iter().map(f).sum::<f64>() / set.len() as f64
        };

        let recent_comp_errors = avg(recent, |s| s.compilation_errors_per_task);
        if recent_comp_errors >= 1.0 {
            targets.push(
                ImprovementTarget::new(
                    ImprovementCategory::CodeQuality,
                    format!(
                        "Reduce compilation errors (recent avg {:.2} per task)",
                        recent_comp_errors
                    ),
                    "Performance introspection detected repeated compile failures across recent tasks.",
                    ImprovementSource::ErrorPattern,
                )
                .with_file("src/agent/execution.rs")
                .with_scores(0.9, 0.85),
            );
        }

        let recent_tool_calls = avg(recent, |s| s.avg_tool_calls);
        let prev_tool_calls = previous.map(|set| avg(set, |s| s.avg_tool_calls));
        if recent_tool_calls >= 14.0
            || prev_tool_calls.is_some_and(|prev| prev > 0.0 && recent_tool_calls / prev > 1.2)
        {
            let rationale = if let Some(prev) = prev_tool_calls {
                format!(
                    "Recent tool-call average {:.1} regressed from {:.1} (>20% increase).",
                    recent_tool_calls, prev
                )
            } else {
                format!(
                    "Recent tool-call average {:.1} exceeds efficiency threshold.",
                    recent_tool_calls
                )
            };
            targets.push(
                ImprovementTarget::new(
                    ImprovementCategory::ToolPipeline,
                    "Reduce tool-call churn by batching read/search operations",
                    rationale,
                    ImprovementSource::MetricsRegression,
                )
                .with_file("src/agent/execution.rs")
                .with_scores(0.8, 0.75),
            );
        }

        let recent_verify = avg(recent, |s| s.first_try_verification_rate);
        let prev_verify = previous.map(|set| avg(set, |s| s.first_try_verification_rate));
        if recent_verify <= 0.5 || prev_verify.is_some_and(|prev| recent_verify + 0.15 < prev) {
            let rationale = if let Some(prev) = prev_verify {
                format!(
                    "First-try verification dropped from {:.0}% to {:.0}%.",
                    prev * 100.0,
                    recent_verify * 100.0
                )
            } else {
                format!(
                    "First-try verification remains low at {:.0}%.",
                    recent_verify * 100.0
                )
            };
            targets.push(
                ImprovementTarget::new(
                    ImprovementCategory::VerificationLogic,
                    "Improve verification-first execution behavior",
                    rationale,
                    ImprovementSource::MetricsRegression,
                )
                .with_file("src/agent/mod.rs")
                .with_scores(0.85, 0.8),
            );
        }

        let recent_recovery = avg(recent, |s| s.error_recovery_rate);
        if recent_recovery <= 0.65 && latest.task_success_rate < 0.9 {
            targets.push(
                ImprovementTarget::new(
                    ImprovementCategory::ErrorHandling,
                    "Harden error recovery and retry strategy",
                    format!(
                        "Recovery rate {:.0}% is below target and success rate is {:.0}%.",
                        recent_recovery * 100.0,
                        latest.task_success_rate * 100.0
                    ),
                    ImprovementSource::MetricsRegression,
                )
                .with_file("src/agent/mod.rs")
                .with_scores(0.8, 0.7),
            );
        }

        targets
    }

    pub fn analyze_self(&self) -> Vec<ImprovementTarget> {
        let mut targets = Vec::new();

        // Check for recurring error patterns in improvement history
        let failed_categories = self.recently_failed_categories(5);

        // Scan for common code quality improvements
        targets.extend(self.scan_code_quality());
        if self.project_root.join("src").exists() {
            targets.extend(self.introspect_performance());
        }

        // Filter out targets in denied files
        targets.retain(|t| !self.is_denied(t));

        // Filter out recently-failed categories (cooldown)
        targets.retain(|t| !failed_categories.contains(&t.category));

        // Filter by minimum confidence
        targets.retain(|t| t.confidence > 0.5);

        // Sort by priority (descending)
        targets.sort_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        targets
    }

    /// Creates a compilation sandbox for the agent to safely apply changes
    pub fn create_sandbox(&self) -> Result<CompilationSandbox> {
        CompilationSandbox::new(&self.project_root)
    }

    /// Select the best target to work on
    pub fn select_target<'a>(
        &self,
        targets: &'a [ImprovementTarget],
    ) -> Option<&'a ImprovementTarget> {
        targets.iter().find(|target| self.supports_target(target))
    }

    /// Returns true when this target has a concrete mutation strategy.
    pub fn supports_target(&self, target: &ImprovementTarget) -> bool {
        matches!(target.category, ImprovementCategory::CodeQuality)
            && target.file.is_some()
            && (target.description.contains("TODO") || target.description.contains("FIXME"))
    }

    /// Apply a supported mutation to the provided sandbox.
    pub fn apply_target_in_sandbox(
        &self,
        target: &ImprovementTarget,
        sandbox: &CompilationSandbox,
    ) -> Result<AppliedMutation> {
        if !self.supports_target(target) {
            return Err(anyhow!(
                "No concrete mutation strategy available for target '{}'",
                target.description
            ));
        }

        let file = target
            .file
            .as_ref()
            .ok_or_else(|| anyhow!("Target missing file path"))?;
        let path = sandbox.work_dir().join(file);
        let original = std::fs::read_to_string(&path)?;
        let line_hint = parse_line_hint(&target.description);
        let (updated, line_number) = rewrite_todo_fixme_marker(&original, line_hint)
            .ok_or_else(|| anyhow!("Failed to locate a mutable TODO/FIXME marker in {}", file))?;

        std::fs::write(&path, updated)?;

        Ok(AppliedMutation {
            edited_files: vec![file.clone()],
            summary: format!("Rewrote TODO/FIXME marker in {}:{}", file, line_number),
        })
    }

    /// Build a task prompt for the agent to apply an improvement
    pub fn build_improvement_prompt(&self, target: &ImprovementTarget) -> String {
        let mut prompt = format!(
            "You are improving your own codebase. Apply the following improvement:\n\n\
             ## Target\n\
             - **Category**: {}\n\
             - **Description**: {}\n\
             - **Rationale**: {}\n",
            target.category, target.description, target.rationale
        );

        if let Some(ref file) = target.file {
            prompt.push_str(&format!("- **File**: {}\n", file));
        }

        prompt.push_str(
            "\n## Instructions\n\
             1. Read the relevant file(s)\n\
             2. Make the minimal change needed\n\
             3. Run `cargo check` to verify compilation\n\
             4. Run `cargo test` on the affected module\n\
             5. If tests fail, fix or revert the change\n\
             6. Summarize what you changed and why\n\n\
             IMPORTANT: Make only the change described above. Do not refactor unrelated code.",
        );

        prompt
    }

    /// Check if a target is in the deny list.
    ///
    /// Uses path canonicalization to catch symlink-based bypasses: the
    /// target file is resolved relative to `project_root` so that
    /// `../../safety/checker.rs` or a symlink pointing there is still
    /// caught.
    ///
    /// **Fail-closed**: if the target path exists on disk but
    /// canonicalization fails (e.g. broken symlink, permission error),
    /// the path is denied by default to prevent bypass.  Non-existent
    /// paths (common in tests and for proposed-but-not-yet-created files)
    /// fall through to substring matching.
    fn is_denied(&self, target: &ImprovementTarget) -> bool {
        if let Some(ref file) = target.file {
            let raw_path = self.project_root.join(file);

            // If the path exists on disk, we MUST be able to canonicalize it.
            // Failure here (broken symlink, permission denied, etc.) is
            // treated as denied to prevent symlink-based bypass attacks.
            let resolved = match raw_path.canonicalize() {
                Ok(p) => p,
                Err(_) if raw_path.exists() || raw_path.symlink_metadata().is_ok() => {
                    // Path exists (or is a symlink) but can't be resolved —
                    // fail closed.
                    return true;
                }
                Err(_) => {
                    // Path doesn't exist — fall through to substring check
                    // (covers tests and proposed files).
                    for denied in DENY_LIST {
                        if file.contains(denied) {
                            return true;
                        }
                    }
                    return false;
                }
            };
            let resolved_str = resolved.to_string_lossy();

            for denied in DENY_LIST {
                // Canonicalize the denied path against project_root too.
                let denied_resolved = self
                    .project_root
                    .join(denied)
                    .canonicalize()
                    .unwrap_or_else(|_| self.project_root.join(denied));
                let denied_str = denied_resolved.to_string_lossy();

                // Check if the resolved path starts with (is inside) a denied
                // directory, or equals a denied file exactly.
                if resolved_str.starts_with(denied_str.as_ref()) {
                    return true;
                }

                // Also check the raw file string for substring matches,
                // covering cases where the denied path itself doesn't exist.
                if file.contains(denied) {
                    return true;
                }
            }
        }
        false
    }

    /// Get categories that failed recently (within the last N attempts)
    fn recently_failed_categories(&self, n: usize) -> Vec<ImprovementCategory> {
        self.history
            .iter()
            .rev()
            .take(n)
            .filter(|r| r.rolled_back || r.effectiveness_score < 0.0)
            .map(|r| r.category.clone())
            .collect()
    }

    /// Basic code quality scan (pattern-based, no AST)
    fn scan_code_quality(&self) -> Vec<ImprovementTarget> {
        let mut targets = Vec::new();
        let src_dir = self.project_root.join("src");

        if !src_dir.exists() {
            return targets;
        }

        // Scan for TODO/FIXME comments as improvement targets
        if let Ok(entries) = glob_rs_files(&src_dir) {
            for path in entries {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let rel_path = path
                        .strip_prefix(&self.project_root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string();

                    for (i, line) in content.lines().enumerate() {
                        if line.contains("TODO") || line.contains("FIXME") {
                            let desc = line.trim().to_string();
                            let target = ImprovementTarget::new(
                                ImprovementCategory::CodeQuality,
                                format!("Address TODO at {}:{}: {}", rel_path, i + 1, desc),
                                "TODO/FIXME markers indicate known issues or missing features",
                                ImprovementSource::TechDebt,
                            )
                            .with_file(rel_path.clone())
                            .with_scores(0.3, 0.6);
                            targets.push(target);
                        }
                    }
                }
            }
        }

        targets
    }

    /// Record the result of an improvement attempt
    pub fn record_result(&mut self, record: ImprovementRecord) -> Result<()> {
        self.history.push(record);
        self.save_history()?;
        Ok(())
    }

    /// Evaluate effectiveness of an improvement from before/after metrics
    pub fn evaluate(before: &PerformanceSnapshot, after: &PerformanceSnapshot) -> f64 {
        after.effectiveness_delta(before)
    }

    /// Get improvement history
    pub fn history(&self) -> &[ImprovementRecord] {
        &self.history
    }

    fn save_history(&self) -> Result<()> {
        if let Some(parent) = self.history_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(&self.history)?;
        std::fs::write(&self.history_path, content)?;
        Ok(())
    }

    fn load_history(path: &Path) -> Result<Vec<ImprovementRecord>> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(path)?;
        let history: Vec<ImprovementRecord> = serde_json::from_str(&content)?;
        Ok(history)
    }
}

/// Recursively collect .rs files from a directory
fn glob_rs_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut results = Vec::new();
    if !dir.is_dir() {
        return Ok(results);
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            results.extend(glob_rs_files(&path)?);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            results.push(path);
        }
    }
    Ok(results)
}

fn parse_line_hint(description: &str) -> Option<usize> {
    let re = Regex::new(r":(\d+):").ok()?;
    let captures = re.captures(description)?;
    captures.get(1)?.as_str().parse::<usize>().ok()
}

fn rewrite_todo_fixme_marker(
    content: &str,
    preferred_line: Option<usize>,
) -> Option<(String, usize)> {
    let todo_re = Regex::new(r"(?i)\b(?:TODO|FIXME)\b[:\-\s]*").ok()?;
    let mut lines: Vec<String> = content.lines().map(ToString::to_string).collect();

    let mut candidate_indices = Vec::new();
    if let Some(line) = preferred_line {
        let idx = line.saturating_sub(1);
        if idx < lines.len() {
            candidate_indices.push(idx);
        }
    }
    candidate_indices.extend(
        lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.contains("TODO") || line.contains("FIXME"))
            .map(|(idx, _)| idx),
    );
    candidate_indices.dedup();

    for idx in candidate_indices {
        let original = &lines[idx];
        let replaced = todo_re
            .replace(original, "Resolved: ")
            .to_string()
            .replace("  ", " ");
        if replaced != *original {
            lines[idx] = replaced;
            let mut updated = lines.join("\n");
            if content.ends_with('\n') {
                updated.push('\n');
            }
            return Some((updated, idx + 1));
        }
    }

    None
}

#[cfg(test)]
#[path = "../../tests/unit/cognitive/self_edit/self_edit_test.rs"]
mod tests;
