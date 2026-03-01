//! Self-Edit Orchestration
//!
//! Enables the agent to analyze its own codebase, identify improvement targets,
//! and safely apply edits with verification and rollback.

use anyhow::Result;
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
        targets.first()
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
    /// caught.  Falls back to plain substring matching when
    /// canonicalization is not possible (e.g. the file doesn't exist yet).
    fn is_denied(&self, target: &ImprovementTarget) -> bool {
        if let Some(ref file) = target.file {
            // Attempt to canonicalize the target path so symlinks and
            // traversals (../.. etc.) resolve to the real location.
            let resolved = self
                .project_root
                .join(file)
                .canonicalize()
                .unwrap_or_else(|_| self.project_root.join(file));
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

                // Fallback: plain substring check covers cases where
                // canonicalization isn't possible (non-existent paths in tests).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_improvement_target_new() {
        let target = ImprovementTarget::new(
            ImprovementCategory::ErrorHandling,
            "Add retry logic to API calls",
            "API calls sometimes fail transiently",
            ImprovementSource::ErrorPattern,
        );
        assert_eq!(target.category, ImprovementCategory::ErrorHandling);
        assert_eq!(target.status, ImprovementStatus::Proposed);
        assert!(target.id.starts_with("imp-"));
    }

    #[test]
    fn test_improvement_target_with_scores() {
        let target = ImprovementTarget::new(
            ImprovementCategory::CodeQuality,
            "desc",
            "rationale",
            ImprovementSource::TechDebt,
        )
        .with_scores(0.8, 0.9);
        assert!((target.priority - 0.72).abs() < 0.001);
    }

    #[test]
    fn test_deny_list() {
        let orchestrator = SelfEditOrchestrator::new(PathBuf::from("/tmp/selfware_test"));
        let target = ImprovementTarget::new(
            ImprovementCategory::CodeQuality,
            "edit safety",
            "reason",
            ImprovementSource::CodeSmell,
        )
        .with_file("src/safety/checker.rs");
        assert!(orchestrator.is_denied(&target));

        let safe_target = ImprovementTarget::new(
            ImprovementCategory::CodeQuality,
            "edit tools",
            "reason",
            ImprovementSource::CodeSmell,
        )
        .with_file("src/tools/file_ops.rs");
        assert!(!orchestrator.is_denied(&safe_target));
    }

    #[test]
    fn test_build_improvement_prompt() {
        let orchestrator = SelfEditOrchestrator::new(PathBuf::from("/tmp/selfware_test"));
        let target = ImprovementTarget::new(
            ImprovementCategory::ErrorHandling,
            "Add retry logic",
            "Transient failures",
            ImprovementSource::ErrorPattern,
        )
        .with_file("src/api/client.rs");
        let prompt = orchestrator.build_improvement_prompt(&target);
        assert!(prompt.contains("Add retry logic"));
        assert!(prompt.contains("cargo check"));
    }

    #[test]
    fn test_evaluate_effectiveness() {
        let before = PerformanceSnapshot::from_checkpoint_data(10, 20, 5, 2, false, 10000, false);
        let after = PerformanceSnapshot::from_checkpoint_data(5, 10, 2, 2, true, 5000, true);
        let score = SelfEditOrchestrator::evaluate(&before, &after);
        assert!(score > 0.0);
    }

    #[test]
    fn test_improvement_target_with_file() {
        let target = ImprovementTarget::new(
            ImprovementCategory::ToolPipeline,
            "desc",
            "rationale",
            ImprovementSource::CodeSmell,
        )
        .with_file("src/tools/registry.rs");
        assert_eq!(target.file, Some("src/tools/registry.rs".to_string()));
    }

    #[test]
    fn test_improvement_target_scores_clamped() {
        let target = ImprovementTarget::new(
            ImprovementCategory::CodeQuality,
            "d",
            "r",
            ImprovementSource::TechDebt,
        )
        .with_scores(1.5, -0.2); // out of range
        assert_eq!(target.impact, 1.0);
        assert_eq!(target.confidence, 0.0);
        assert_eq!(target.priority, 0.0); // 1.0 * 0.0
    }

    #[test]
    fn test_improvement_category_display() {
        assert_eq!(
            format!("{}", ImprovementCategory::PromptTemplate),
            "prompt_template"
        );
        assert_eq!(
            format!("{}", ImprovementCategory::ErrorHandling),
            "error_handling"
        );
        assert_eq!(
            format!("{}", ImprovementCategory::NewCapability),
            "new_capability"
        );
    }

    #[test]
    fn test_deny_list_all_patterns() {
        let orchestrator = SelfEditOrchestrator::new(PathBuf::from("/tmp/selfware_test"));

        let make_target = |file: &str| {
            ImprovementTarget::new(
                ImprovementCategory::CodeQuality,
                "d",
                "r",
                ImprovementSource::CodeSmell,
            )
            .with_file(file)
        };

        // All denied patterns
        assert!(orchestrator.is_denied(&make_target("src/safety/checker.rs")));
        assert!(orchestrator.is_denied(&make_target("src/safety/path_validator.rs")));
        assert!(orchestrator.is_denied(&make_target("Cargo.toml")));
        assert!(orchestrator.is_denied(&make_target(".github/workflows/ci.yml")));
        assert!(orchestrator.is_denied(&make_target("src/main.rs")));

        // Not denied
        assert!(!orchestrator.is_denied(&make_target("src/agent/mod.rs")));
        assert!(!orchestrator.is_denied(&make_target("src/cognitive/metrics.rs")));

        // No file — not denied
        let no_file = ImprovementTarget::new(
            ImprovementCategory::CodeQuality,
            "d",
            "r",
            ImprovementSource::CodeSmell,
        );
        assert!(!orchestrator.is_denied(&no_file));
    }

    #[test]
    fn test_select_target_returns_first() {
        let orchestrator = SelfEditOrchestrator::new(PathBuf::from("/tmp/selfware_test"));
        let targets = vec![
            ImprovementTarget::new(
                ImprovementCategory::ErrorHandling,
                "first",
                "r",
                ImprovementSource::ErrorPattern,
            )
            .with_scores(0.9, 0.9),
            ImprovementTarget::new(
                ImprovementCategory::CodeQuality,
                "second",
                "r",
                ImprovementSource::TechDebt,
            )
            .with_scores(0.5, 0.5),
        ];
        let selected = orchestrator.select_target(&targets).unwrap();
        assert_eq!(selected.description, "first");
    }

    #[test]
    fn test_select_target_empty_returns_none() {
        let orchestrator = SelfEditOrchestrator::new(PathBuf::from("/tmp/selfware_test"));
        assert!(orchestrator.select_target(&[]).is_none());
    }

    #[test]
    fn test_analyze_self_on_temp_dir_with_todo() {
        // Create a temp dir with a fake .rs file containing a TODO
        let tmp = std::env::temp_dir().join("selfware_test_analyze");
        let src = tmp.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("example.rs"),
            "fn main() {\n    // TODO: fix this\n}\n",
        )
        .unwrap();

        let orchestrator = SelfEditOrchestrator::new(tmp.clone());
        let targets = orchestrator.analyze_self();

        // Should find at least the TODO
        assert!(!targets.is_empty(), "Should find TODO target in test dir");
        assert!(targets.iter().any(|t| t.description.contains("TODO")));
        assert!(targets
            .iter()
            .any(|t| t.source == ImprovementSource::TechDebt));
        assert!(targets
            .iter()
            .any(|t| t.category == ImprovementCategory::CodeQuality));

        // Cleanup
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_analyze_self_filters_low_confidence() {
        // analyze_self filters confidence <= 0.5
        // Our scan_code_quality sets confidence to 0.6, so they should pass
        let tmp = std::env::temp_dir().join("selfware_test_analyze_conf");
        let src = tmp.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), "// FIXME: broken\n").unwrap();

        let orchestrator = SelfEditOrchestrator::new(tmp.clone());
        let targets = orchestrator.analyze_self();
        assert!(!targets.is_empty());
        // All returned targets should have confidence > 0.5
        for t in &targets {
            assert!(
                t.confidence > 0.5,
                "confidence {} should be > 0.5",
                t.confidence
            );
        }

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_analyze_self_no_src_dir() {
        let tmp = std::env::temp_dir().join("selfware_test_no_src");
        std::fs::create_dir_all(&tmp).unwrap();
        // No src/ subdirectory

        let orchestrator = SelfEditOrchestrator::new(tmp.clone());
        let targets = orchestrator.analyze_self();
        assert!(targets.is_empty());

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_record_result_and_history() {
        let tmp = std::env::temp_dir().join("selfware_test_history");
        std::fs::create_dir_all(&tmp).ok();
        let history_path = tmp.join("history.json");
        std::fs::remove_file(&history_path).ok();

        let mut orchestrator =
            SelfEditOrchestrator::with_history_path(tmp.clone(), history_path.clone());
        assert!(orchestrator.history().is_empty());

        let record = ImprovementRecord {
            target_id: "imp-1".to_string(),
            category: ImprovementCategory::ErrorHandling,
            description: "Added retry".to_string(),
            before_metrics: None,
            after_metrics: None,
            git_commits: vec!["abc123".to_string()],
            verified: true,
            rolled_back: false,
            effectiveness_score: 0.5,
            completed_at: 12345,
        };

        orchestrator.record_result(record).unwrap();
        assert_eq!(orchestrator.history().len(), 1);
        assert_eq!(orchestrator.history()[0].target_id, "imp-1");

        // Verify persistence — create new orchestrator from same path
        let orchestrator2 = SelfEditOrchestrator::with_history_path(tmp.clone(), history_path);
        assert_eq!(orchestrator2.history().len(), 1);
        assert_eq!(orchestrator2.history()[0].description, "Added retry");

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_recently_failed_categories_cooldown() {
        let tmp = std::env::temp_dir().join("selfware_test_cooldown");
        std::fs::create_dir_all(&tmp).ok();
        let history_path = tmp.join("history.json");
        std::fs::remove_file(&history_path).ok();

        let mut orchestrator = SelfEditOrchestrator::with_history_path(tmp.clone(), history_path);

        // Record a rolled-back attempt
        let record = ImprovementRecord {
            target_id: "imp-fail".to_string(),
            category: ImprovementCategory::PromptTemplate,
            description: "bad change".to_string(),
            before_metrics: None,
            after_metrics: None,
            git_commits: vec![],
            verified: false,
            rolled_back: true,
            effectiveness_score: -0.3,
            completed_at: 0,
        };
        orchestrator.record_result(record).unwrap();

        let failed = orchestrator.recently_failed_categories(5);
        assert!(failed.contains(&ImprovementCategory::PromptTemplate));

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_introspect_performance_from_snapshots_detects_regression() {
        let orchestrator = SelfEditOrchestrator::new(PathBuf::from("/tmp/selfware_test"));
        let mut snapshots = Vec::new();
        for _ in 0..5 {
            snapshots.push(PerformanceSnapshot {
                timestamp: 1,
                task_success_rate: 0.95,
                avg_iterations: 5.0,
                avg_tool_calls: 8.0,
                error_recovery_rate: 0.9,
                first_try_verification_rate: 0.85,
                avg_tokens: 5000.0,
                test_pass_rate: 0.95,
                compilation_errors_per_task: 0.1,
                label: None,
            });
        }
        for _ in 0..5 {
            snapshots.push(PerformanceSnapshot {
                timestamp: 2,
                task_success_rate: 0.7,
                avg_iterations: 8.0,
                avg_tool_calls: 18.0,
                error_recovery_rate: 0.5,
                first_try_verification_rate: 0.35,
                avg_tokens: 9000.0,
                test_pass_rate: 0.7,
                compilation_errors_per_task: 2.1,
                label: None,
            });
        }

        let targets = orchestrator.introspect_performance_from_snapshots(&snapshots);
        assert!(targets
            .iter()
            .any(|t| t.category == ImprovementCategory::VerificationLogic));
        assert!(targets
            .iter()
            .any(|t| t.category == ImprovementCategory::ToolPipeline));
        assert!(targets
            .iter()
            .any(|t| t.category == ImprovementCategory::CodeQuality));
    }

    #[test]
    fn test_build_improvement_prompt_no_file() {
        let orchestrator = SelfEditOrchestrator::new(PathBuf::from("/tmp/selfware_test"));
        let target = ImprovementTarget::new(
            ImprovementCategory::ContextManagement,
            "Reduce context window usage",
            "Too many tokens wasted",
            ImprovementSource::MetricsRegression,
        );
        let prompt = orchestrator.build_improvement_prompt(&target);
        assert!(prompt.contains("Reduce context window usage"));
        assert!(prompt.contains("context_management"));
        // Should not contain "File:" line since no file set
        assert!(!prompt.contains("**File**:"));
    }

    #[test]
    fn test_improvement_target_serialization_roundtrip() {
        let target = ImprovementTarget::new(
            ImprovementCategory::VerificationLogic,
            "desc",
            "rationale",
            ImprovementSource::LLMReflection,
        )
        .with_file("src/verification.rs")
        .with_scores(0.7, 0.8);

        let json = serde_json::to_string(&target).unwrap();
        let deserialized: ImprovementTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.category,
            ImprovementCategory::VerificationLogic
        );
        assert_eq!(deserialized.source, ImprovementSource::LLMReflection);
        assert!((deserialized.priority - 0.56).abs() < 0.001);
    }

    #[test]
    fn test_improvement_record_serialization_roundtrip() {
        let record = ImprovementRecord {
            target_id: "imp-42".to_string(),
            category: ImprovementCategory::ToolPipeline,
            description: "test record".to_string(),
            before_metrics: Some(PerformanceSnapshot::from_checkpoint_data(
                5, 10, 1, 1, true, 5000, true,
            )),
            after_metrics: Some(PerformanceSnapshot::from_checkpoint_data(
                3, 6, 0, 0, true, 3000, true,
            )),
            git_commits: vec!["abc".to_string(), "def".to_string()],
            verified: true,
            rolled_back: false,
            effectiveness_score: 0.75,
            completed_at: 99999,
        };

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: ImprovementRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.target_id, "imp-42");
        assert!(deserialized.before_metrics.is_some());
        assert_eq!(deserialized.git_commits.len(), 2);
    }

    #[test]
    fn test_glob_rs_files() {
        let tmp = std::env::temp_dir().join("selfware_test_glob");
        let sub = tmp.join("subdir");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(tmp.join("a.rs"), "").unwrap();
        std::fs::write(tmp.join("b.txt"), "").unwrap(); // not .rs
        std::fs::write(sub.join("c.rs"), "").unwrap();

        let files = glob_rs_files(&tmp).unwrap();
        assert_eq!(files.len(), 2);
        let names: Vec<_> = files
            .iter()
            .map(|f| f.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"a.rs".to_string()));
        assert!(names.contains(&"c.rs".to_string()));

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_glob_rs_files_nonexistent_dir() {
        let result = glob_rs_files(Path::new("/tmp/selfware_nonexistent_dir_123456"));
        assert!(result.unwrap().is_empty());
    }
}
