//! Self-Edit Orchestration
//!
//! Enables the agent to analyze its own codebase, identify improvement targets,
//! and safely apply edits with verification and rollback.

use anyhow::{anyhow, Context, Result};
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

/// Status of an improvement proposal attempt
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ProposalStatus {
    /// Proposal was skipped before evaluation (e.g. trivial comment-only diff, no-op)
    SkippedTrivial,
    /// Compilation or local test verification failed in sandbox
    #[default]
    VerificationFailed,
    /// Proposal was evaluated on benchmark suite and succeeded
    EvaluatedSuccess,
    /// Proposal was evaluated on benchmark suite and regressed, rolled back
    EvaluatedRegression,
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
    #[serde(default)]
    pub status: ProposalStatus,
}

/// Result of applying a concrete self-edit mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedMutation {
    /// Relative project files edited in the sandbox.
    pub edited_files: Vec<String>,
    /// Human-readable summary of the applied mutation.
    pub summary: String,
}

/// Cooldown applied to a category that recently failed, measured on the wall
/// clock. See `SelfEditOrchestrator::recently_failed_categories`.
pub const FAILURE_COOLDOWN_SECS: u64 = 1800;

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
        if crate::safety::killswitch::is_killswitch_active() {
            tracing::warn!("Killswitch active; skipping self analysis");
            return Vec::new();
        }

        let mut targets = Vec::new();

        // Check for recurring error patterns in improvement history
        let failed_categories = self.recently_failed_categories(FAILURE_COOLDOWN_SECS);

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
        if crate::safety::killswitch::is_killswitch_active() {
            tracing::warn!("Killswitch active; blocking target selection");
            return None;
        }
        targets.iter().find(|target| self.supports_target(target))
    }

    /// Returns true when this target has a concrete mutation strategy.
    pub fn supports_target(&self, target: &ImprovementTarget) -> bool {
        if target.file.is_none() {
            return false;
        }
        match target.category {
            ImprovementCategory::CodeQuality => {
                target.description.contains("TODO") || target.description.contains("FIXME")
            }
            ImprovementCategory::PromptTemplate
            | ImprovementCategory::ToolPipeline
            | ImprovementCategory::ErrorHandling
            | ImprovementCategory::ContextManagement
            | ImprovementCategory::VerificationLogic
            | ImprovementCategory::NewCapability => false,
        }
    }

    /// Apply a supported mutation to the provided sandbox.
    pub fn apply_target_in_sandbox(
        &self,
        target: &ImprovementTarget,
        sandbox: &CompilationSandbox,
    ) -> Result<AppliedMutation> {
        if crate::safety::killswitch::is_killswitch_active() {
            return Err(anyhow!("Killswitch is active: mutation blocked in sandbox"));
        }
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

        let file_path = Path::new(file);
        if file_path.is_absolute()
            || file_path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(anyhow!(
                "Path traversal detected or absolute path not allowed: {}",
                file
            ));
        }

        if self.is_denied(target) {
            return Err(anyhow!("Target file is in deny list: {:?}", target.file));
        }

        let sandbox_root = sandbox
            .work_dir()
            .canonicalize()
            .context("Failed to canonicalize sandbox directory")?;

        let path = sandbox.work_dir().join(file_path);
        if !path.exists() {
            return Err(anyhow!("Target file '{}' does not exist in sandbox", file));
        }

        let canonical_path = path
            .canonicalize()
            .map_err(|e| anyhow!("Failed to canonicalize target file path '{}': {}", file, e))?;

        if !canonical_path.starts_with(&sandbox_root) {
            return Err(anyhow!("Target file '{}' escapes sandbox directory", file));
        }

        let original = std::fs::read_to_string(&canonical_path)?;

        let (updated, summary) = match target.category {
            ImprovementCategory::CodeQuality => {
                let line_hint = parse_line_hint(&target.description);
                if let Some((rewritten, line_number)) =
                    rewrite_todo_fixme_marker(&original, line_hint)
                {
                    (
                        rewritten,
                        format!("Rewrote TODO/FIXME marker in {}:{}", file, line_number),
                    )
                } else {
                    return Err(anyhow!(
                        "Failed to locate mutable code quality pattern in {}",
                        file
                    ));
                }
            }
            _ => {
                return Err(anyhow!(
                    "Category {:?} requires generative agent synthesis",
                    target.category
                ));
            }
        };

        std::fs::write(&canonical_path, updated)?;

        Ok(AppliedMutation {
            edited_files: vec![file.clone()],
            summary,
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
    pub(crate) fn is_denied(&self, target: &ImprovementTarget) -> bool {
        if crate::safety::killswitch::is_killswitch_active() {
            return true;
        }

        if let Some(ref file) = target.file {
            let file_path = Path::new(file);

            // Path traversal or absolute paths outside root are denied
            if file_path.is_absolute()
                || file_path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return true;
            }

            let file_str = file_path.to_string_lossy();
            if file_str.contains(".admitted_ledger.json")
                || file_str.contains(".selfware/skills")
                || file_str.contains(".selfware/commands")
                || file_str.contains(".selfware/skill-candidates")
                || file_str.contains(".selfware/KILLSWITCH")
            {
                return true;
            }

            // Direct check against PROTECTED_PATHS (also testing with src/ prefix if omitted)
            if crate::evolution::is_protected(file_path)
                || crate::evolution::is_protected(&Path::new("src").join(file_path))
            {
                return true;
            }

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
                    // Path doesn't exist — check if file path itself is protected
                    return crate::evolution::is_protected(file_path)
                        || crate::evolution::is_protected(&Path::new("src").join(file_path));
                }
            };

            // Check if the resolved canonical path is protected
            if crate::evolution::is_protected(&resolved) {
                return true;
            }

            // Ensure resolved path does not escape project_root
            if let Ok(canonical_root) = self.project_root.canonicalize() {
                if !resolved.starts_with(&canonical_root) {
                    return true;
                }
            }
        }
        false
    }

    /// Get categories with a recorded failure still inside the cooldown window.
    ///
    /// Age is measured from each record's `completed_at`, NOT from how many
    /// records exist. A record-count window can never age out its own blocker:
    /// an idle cycle (no eligible target, or a mutation the trivial gate
    /// discards) appends no record, so the window never advances and the failed
    /// category stays excluded — and since the history is persisted, that block
    /// survived restarts. Wall-clock age expires regardless of activity.
    ///
    /// The returned order is unspecified (callers use `contains`).
    fn recently_failed_categories(&self, window_secs: u64) -> Vec<ImprovementCategory> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.history
            .iter()
            .filter(|r| {
                if r.status == ProposalStatus::SkippedTrivial {
                    return false;
                }
                if !(r.rolled_back || r.effectiveness_score < 0.0) {
                    return false;
                }
                // An unknown timestamp (0 from an older history file) counts as
                // expired: expiring only means the category may be retried,
                // which is the safe direction for a cooldown.
                r.completed_at > 0 && now.saturating_sub(r.completed_at) <= window_secs
            })
            .map(|r| r.category.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
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
                        if !line.contains("TODO") && !line.contains("FIXME") {
                            continue;
                        }
                        // A marker on a whole-line comment can only be rewritten
                        // as comment text, and `mutation_is_trivial` discards a
                        // comment-only diff before evaluation — proposing it
                        // spends a cycle to learn nothing, and the target stays
                        // eligible for the next cycle. Markers that share their
                        // line with code stay proposable, but their rewrite is
                        // comment-text-only too, so the same gate (now with
                        // inline-comment stripping) skips them as trivial before
                        // any paid suite runs. (`.rs` files only:
                        // `glob_rs_files`.)
                        if line_is_non_code(line, false, false) {
                            continue;
                        }
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

/// True when `line` contributes no code: blank, or a whole-line comment under
/// the given stripping rules (`strip_hash` for `#`-comment formats,
/// `strip_asterisk` for `*`-prefixed block-comment bodies — neither applies to
/// Rust, where `#` opens an attribute and `*` dereferences).
///
/// The single source of truth behind both `rsi_orchestrator::code_lines` (the
/// trivial-diff gate) and `scan_code_quality`. An inline trailing comment does
/// NOT make a line non-code, so `42 // TODO: x` is proposable — but its marker
/// rewrite is comment-text-only, which the gate's inline stripping classifies
/// as trivial before any paid suite runs.
pub(crate) fn line_is_non_code(line: &str, strip_hash: bool, strip_asterisk: bool) -> bool {
    let line = line.trim();
    line.is_empty()
        || line.starts_with("//")
        || (strip_hash && line.starts_with('#'))
        || line.starts_with("/*")
        || (strip_asterisk && line.starts_with('*'))
        || line.starts_with("--")
}

/// Drop a trailing `//` line comment from `line`, returning the code prefix
/// (right-trimmed). `//` sequences inside string or char literals are left
/// intact, so `let url = "http://a/b";` is untouched — that keeps code that
/// differs only inside a string literal from comparing equal after stripping.
///
/// This is what lets the trivial-mutation gate see the *code* content of a
/// line that also carries an inline comment: a mutation that only rewrites
/// comment text (e.g. a TODO marker rewrite) then compares equal and is
/// skipped before any paid suite runs.
///
/// Corners err towards NOT stripping (the evaluate direction), never towards
/// stripping real code: a Rust lifetime (`&'a str`) leaves the char-literal
/// state open until the next `'`, so a `//` on such a line is not stripped;
/// raw strings are not escaped like normal strings, so a `\` inside one leaves
/// the string state open and shields any following `//`. Both misreadings can
/// only cost one cycle's evaluation, never silently skip a real change.
pub(crate) fn strip_inline_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut in_string = false;
    let mut in_char = false;
    let mut escaped = false;
    while i < bytes.len() {
        let b = bytes[i];
        if escaped {
            escaped = false;
            i += 1;
            continue;
        }
        if b == b'\\' {
            escaped = true;
            i += 1;
            continue;
        }
        if in_string {
            if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if in_char {
            if b == b'\'' {
                in_char = false;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'\'' => in_char = true,
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                return line[..i].trim_end();
            }
            _ => {}
        }
        i += 1;
    }
    line
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
    // Fallback candidates are restricted to lines that carry code. If the hint
    // misses (the file shifted between scan and apply, which is what the hint
    // exists to absorb), falling through to a whole-line comment would produce
    // a comment-only diff that `mutation_is_trivial` discards — the same no-op
    // the scanner refuses to propose, arriving by a side door.
    candidate_indices.extend(
        lines
            .iter()
            .enumerate()
            .filter(|(_, line)| {
                (line.contains("TODO") || line.contains("FIXME"))
                    && !line_is_non_code(line, false, false)
            })
            .map(|(idx, _)| idx),
    );
    candidate_indices.dedup();

    for idx in candidate_indices {
        let original = &lines[idx];
        // Only a candidate whose marker the regex actually matched may be
        // rewritten. The old guard was `replaced != original` computed *after* a
        // blanket `.replace("  ", " ")`, which is true for ANY line holding two
        // consecutive spaces — so a hint that had drifted onto an unrelated line
        // was "rewritten" by collapsing its indentation and returned as a
        // successful mutation. The marker match is the real condition, and the
        // regex already swallows the marker's trailing `: `/`- `, so the blanket
        // collapse only ever damaged whitespace outside the replacement.
        if !todo_re.is_match(original) {
            continue;
        }
        lines[idx] = todo_re.replace(original, "Resolved: ").to_string();
        let mut updated = lines.join("\n");
        if content.ends_with('\n') {
            updated.push('\n');
        }
        return Some((updated, idx + 1));
    }

    None
}

#[cfg(test)]
#[path = "../../tests/unit/cognitive/self_edit/self_edit_test.rs"]
mod tests;
