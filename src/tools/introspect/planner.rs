//! Evolution Planning Tool
//!
//! Generates structured execution plans for evolution tasks, respecting
//! token and iteration budgets. Breaks down goals into phases with
//! specific actions.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::budget::PlanBudget;
use super::query::{extract_keywords, find_related_symbols};

/// An evolution plan with multiple phases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionPlan {
    /// Goal description
    pub goal: String,
    /// Plan phases in execution order
    pub phases: Vec<PlanPhase>,
    /// Estimated token usage
    pub estimated_tokens: usize,
    /// Token budget
    pub token_budget: usize,
    /// Iteration budget
    pub iteration_budget: usize,
    /// Risk assessment
    pub risk: RiskLevel,
    /// Success criteria
    pub success_criteria: Vec<String>,
}

/// A single phase in the evolution plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanPhase {
    /// Phase number (1-indexed)
    pub phase: usize,
    /// Action type
    pub action: ActionType,
    /// Target file(s) or directory
    pub target: String,
    /// Additional parameters
    pub params: HashMap<String, String>,
    /// Reasoning for this phase
    pub reason: String,
    /// Estimated tokens for this phase
    pub estimated_tokens: usize,
    /// Dependencies on previous phases
    pub dependencies: Vec<usize>,
}

/// Types of actions in a plan
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    /// Introspect codebase structure
    Introspect,
    /// Query for specific code
    Query,
    /// Read full file content
    Read,
    /// Analyze impact of change
    ImpactAnalysis,
    /// Generate modification
    Modify,
    /// Verify with compilation
    Verify,
    /// Run tests
    Test,
    /// Complete task
    Complete,
}

impl ActionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Introspect => "introspect",
            Self::Query => "query",
            Self::Read => "read",
            Self::ImpactAnalysis => "impact_analysis",
            Self::Modify => "modify",
            Self::Verify => "verify",
            Self::Test => "test",
            Self::Complete => "complete",
        }
    }
}

/// Risk level for a plan
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

/// Planner for evolution tasks
pub struct EvolutionPlanner {
    goal: String,
    budget: PlanBudget,
    codebase_root: PathBuf,
    /// Workspace path policy for every file and directory the walk reaches
    /// (`None`: no per-path validation, library callers only).
    safety: Option<crate::config::SafetyConfig>,
    /// Optional caller-requested planning strategy override.
    strategy_override: Option<PlanningStrategy>,
}

impl EvolutionPlanner {
    /// Create a new planner
    pub fn new(
        goal: String,
        max_iterations: usize,
        max_tokens: usize,
        codebase_root: PathBuf,
    ) -> Self {
        Self {
            goal,
            budget: PlanBudget::new(max_iterations, max_tokens),
            codebase_root,
            safety: None,
            strategy_override: None,
        }
    }

    /// Validate every path the walk reaches against `safety` (the same
    /// policy `code_introspect` / `code_query` enforce).
    pub fn with_safety_config(mut self, safety: crate::config::SafetyConfig) -> Self {
        self.safety = Some(safety);
        self
    }

    /// Override the heuristic planning strategy with a caller-specified strategy
    /// (`"breadth_first"`, `"depth_first"`, or `"impact_analysis"`).
    pub fn with_strategy_override(mut self, strategy: Option<&str>) -> Result<Self> {
        self.strategy_override = match strategy.map(str::trim).filter(|s| !s.is_empty()) {
            None => None,
            Some("breadth_first") => Some(PlanningStrategy::IntrospectionFirst),
            Some("depth_first") => Some(PlanningStrategy::TargetedChange),
            Some("impact_analysis") => Some(PlanningStrategy::ImpactAnalysis),
            Some(other) => {
                anyhow::bail!(
                    "Unknown planning strategy '{}'. Expected 'breadth_first', 'depth_first', or 'impact_analysis'",
                    other
                );
            }
        };
        Ok(self)
    }

    /// Generate an evolution plan
    pub async fn generate_plan(&self) -> Result<EvolutionPlan> {
        // Extract keywords from goal to understand intent
        let keywords = extract_keywords(&self.goal);

        // Analyze goal to determine strategy (honoring explicit caller override)
        let strategy = self
            .strategy_override
            .clone()
            .unwrap_or_else(|| self.determine_strategy(&keywords));

        // Find relevant files
        let relevant_files = self.find_relevant_files(&keywords).await?;

        // Generate phases based on strategy
        let phases = match strategy {
            PlanningStrategy::IntrospectionFirst => {
                self.plan_introspection_first(&relevant_files, false)
                    .await?
            }
            PlanningStrategy::ImpactAnalysis => {
                self.plan_introspection_first(&relevant_files, true).await?
            }
            PlanningStrategy::TargetedChange => self.plan_targeted_change(&relevant_files).await?,
            PlanningStrategy::Exploratory => self.plan_exploratory(&relevant_files).await?,
        };

        // Calculate estimates
        let estimated_tokens = phases.iter().map(|p| p.estimated_tokens).sum();

        // Assess risk
        let risk = self.assess_risk(&phases, &relevant_files);

        // Define success criteria
        let success_criteria = self.define_success_criteria(&keywords);

        Ok(EvolutionPlan {
            goal: self.goal.clone(),
            phases,
            estimated_tokens,
            token_budget: self.budget.max_tokens,
            iteration_budget: self.budget.max_iterations,
            risk,
            success_criteria,
        })
    }

    /// Determine planning strategy based on goal analysis
    fn determine_strategy(&self, keywords: &[String]) -> PlanningStrategy {
        let goal_lower = self.goal.to_lowercase();

        // Check for specific patterns
        if keywords
            .iter()
            .any(|k| ["improve", "optimize", "refactor"].contains(&k.as_str()))
        {
            PlanningStrategy::IntrospectionFirst
        } else if keywords
            .iter()
            .any(|k| ["fix", "bug", "error", "issue"].contains(&k.as_str()))
        {
            PlanningStrategy::TargetedChange
        } else if keywords
            .iter()
            .any(|k| ["add", "implement", "create"].contains(&k.as_str()))
            || goal_lower.contains("understand")
            || goal_lower.contains("explore")
        {
            PlanningStrategy::Exploratory
        } else {
            // Default strategy
            PlanningStrategy::IntrospectionFirst
        }
    }

    /// Find files relevant to the goal
    async fn find_relevant_files(&self, keywords: &[String]) -> Result<Vec<PathBuf>> {
        // The shared introspection walk (review 2026-09-27: this planner
        // had its own duplicate walker — no per-path validation, no depth
        // bound, no `.venv` skip, read errors swallowed). One
        // implementation: same skips, symlink cycles walked once,
        // `.gitignore` respected, every path validated against the
        // workspace policy, errors surfaced.
        let root = self.codebase_root.clone();
        let safety = self.safety.clone();
        let files = crate::tools::workspace_root::spawn_blocking(move || {
            super::walk::source_files(&root, safety.as_ref(), super::MAX_WALK_DEPTH)
        })
        .await??
        .files;

        // If we have keywords, try to find most relevant files
        if !keywords.is_empty() {
            let query = keywords.join(" ");
            let related = find_related_symbols(&query, &files, 20).await?;

            // Extract unique file paths from results
            let mut relevant: Vec<PathBuf> = related
                .into_iter()
                .map(|(path, _)| path)
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            // Sort by path for consistency
            relevant.sort();

            if !relevant.is_empty() {
                return Ok(relevant);
            }
        }

        Ok(files)
    }

    /// Same language table as `code_introspect` and the repository
    /// inventory (one source of truth).
    #[cfg(test)]
    fn is_source_file(path: &std::path::Path) -> bool {
        super::CodeIntrospect::is_source_file(path)
    }

    /// Plan with introspection-first strategy
    async fn plan_introspection_first(
        &self,
        files: &[PathBuf],
        force_impact_analysis: bool,
    ) -> Result<Vec<PlanPhase>> {
        let mut phases = Vec::new();
        let mut phase_num = 1;

        // Phase 1: Overview of relevant directories
        if let Some(first_file) = files.first() {
            if let Some(dir) = first_file.parent() {
                phases.push(PlanPhase {
                    phase: phase_num,
                    action: ActionType::Introspect,
                    target: dir.to_string_lossy().to_string(),
                    params: [
                        ("depth".to_string(), "overview".to_string()),
                        ("max_tokens".to_string(), "2000".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                    reason: "Understand the structure of the codebase area relevant to the goal"
                        .to_string(),
                    estimated_tokens: 1000,
                    dependencies: vec![],
                });
                phase_num += 1;
            }
        }

        // Phase 2: Query for specific symbols
        let keywords = extract_keywords(&self.goal);
        if !keywords.is_empty() {
            phases.push(PlanPhase {
                phase: phase_num,
                action: ActionType::Query,
                target: keywords.join(", "),
                params: [
                    (
                        "scope".to_string(),
                        self.codebase_root.to_string_lossy().to_string(),
                    ),
                    ("max_results".to_string(), "10".to_string()),
                ]
                .into_iter()
                .collect(),
                reason: "Find specific code related to the goal".to_string(),
                estimated_tokens: 1500,
                dependencies: vec![phase_num - 1],
            });
            phase_num += 1;
        }

        // Phase 3: Detailed introspection of most relevant files
        let top_files: Vec<_> = files.iter().take(3).cloned().collect();
        for file in top_files {
            let estimated_tokens = measure_signature_phase_tokens(&file, 1500, 1000);
            phases.push(PlanPhase {
                phase: phase_num,
                action: ActionType::Introspect,
                target: file.to_string_lossy().to_string(),
                params: [
                    ("depth".to_string(), "signatures".to_string()),
                    ("max_tokens".to_string(), "1500".to_string()),
                ]
                .into_iter()
                .collect(),
                reason: format!(
                    "Understand the API of {}",
                    file.file_name().unwrap_or_default().to_string_lossy()
                ),
                estimated_tokens,
                dependencies: vec![phase_num - 1],
            });
            phase_num += 1;
        }

        // Phase 4: Impact analysis if modifying or explicitly requested
        if force_impact_analysis
            || self.goal.to_lowercase().contains("modify")
            || self.goal.to_lowercase().contains("change")
            || self.goal.to_lowercase().contains("fix")
        {
            if let Some(target_file) = files.first() {
                phases.push(PlanPhase {
                    phase: phase_num,
                    action: ActionType::ImpactAnalysis,
                    target: target_file.to_string_lossy().to_string(),
                    params: [("change_type".to_string(), "modify".to_string())]
                        .into_iter()
                        .collect(),
                    reason: "Understand what code depends on the target file".to_string(),
                    estimated_tokens: 800,
                    dependencies: vec![phase_num - 1],
                });
                phase_num += 1;
            }
        }

        // Phase 5: Modification
        phases.push(PlanPhase {
            phase: phase_num,
            action: ActionType::Modify,
            target: files
                .first()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default(),
            params: [("goal".to_string(), self.goal.clone())]
                .into_iter()
                .collect(),
            reason: "Apply the necessary changes to achieve the goal".to_string(),
            estimated_tokens: 2000,
            dependencies: vec![phase_num - 1],
        });
        phase_num += 1;

        // Phase 6: Verification
        phases.push(PlanPhase {
            phase: phase_num,
            action: ActionType::Verify,
            target: self.codebase_root.to_string_lossy().to_string(),
            params: HashMap::new(),
            reason: "Verify the changes compile correctly".to_string(),
            estimated_tokens: 500,
            dependencies: vec![phase_num - 1],
        });
        phase_num += 1;

        // Phase 7: Completion
        phases.push(PlanPhase {
            phase: phase_num,
            action: ActionType::Complete,
            target: "task".to_string(),
            params: HashMap::new(),
            reason: "Task completed".to_string(),
            estimated_tokens: 100,
            dependencies: vec![phase_num - 1],
        });

        Ok(phases)
    }

    /// Plan for targeted changes
    async fn plan_targeted_change(&self, files: &[PathBuf]) -> Result<Vec<PlanPhase>> {
        let mut phases = Vec::new();
        let mut phase_num = 1;

        // Targeted changes skip broad introspection
        // Go straight to relevant files

        let target_files: Vec<_> = files.iter().take(5).cloned().collect();

        for file in target_files {
            let estimated_tokens = measure_signature_phase_tokens(&file, 1000, 800);
            // Read signatures of each relevant file
            phases.push(PlanPhase {
                phase: phase_num,
                action: ActionType::Introspect,
                target: file.to_string_lossy().to_string(),
                params: [
                    ("depth".to_string(), "signatures".to_string()),
                    ("max_tokens".to_string(), "1000".to_string()),
                ]
                .into_iter()
                .collect(),
                reason: format!(
                    "Understand {}",
                    file.file_name().unwrap_or_default().to_string_lossy()
                ),
                estimated_tokens,
                dependencies: if phase_num > 1 {
                    vec![phase_num - 1]
                } else {
                    vec![]
                },
            });
            phase_num += 1;
        }

        // Modify the primary target
        if let Some(target) = files.first() {
            phases.push(PlanPhase {
                phase: phase_num,
                action: ActionType::Modify,
                target: target.to_string_lossy().to_string(),
                params: [("goal".to_string(), self.goal.clone())]
                    .into_iter()
                    .collect(),
                reason: "Apply the fix/change".to_string(),
                estimated_tokens: 1500,
                dependencies: vec![phase_num - 1],
            });
            phase_num += 1;
        }

        // Verify
        phases.push(PlanPhase {
            phase: phase_num,
            action: ActionType::Verify,
            target: self.codebase_root.to_string_lossy().to_string(),
            params: HashMap::new(),
            reason: "Verify compilation".to_string(),
            estimated_tokens: 500,
            dependencies: vec![phase_num - 1],
        });

        Ok(phases)
    }

    /// Plan for exploratory tasks
    async fn plan_exploratory(&self, _files: &[PathBuf]) -> Result<Vec<PlanPhase>> {
        let mut phases = Vec::new();

        // Exploratory: Broad overview first
        phases.push(PlanPhase {
            phase: 1,
            action: ActionType::Introspect,
            target: self.codebase_root.to_string_lossy().to_string(),
            params: [
                ("depth".to_string(), "overview".to_string()),
                ("max_tokens".to_string(), "3000".to_string()),
            ]
            .into_iter()
            .collect(),
            reason: "Get broad overview of codebase structure".to_string(),
            estimated_tokens: 2000,
            dependencies: vec![],
        });

        // Query for specific topics
        let keywords = extract_keywords(&self.goal);
        if !keywords.is_empty() {
            phases.push(PlanPhase {
                phase: 2,
                action: ActionType::Query,
                target: keywords.join(", "),
                params: [
                    (
                        "scope".to_string(),
                        self.codebase_root.to_string_lossy().to_string(),
                    ),
                    ("max_results".to_string(), "15".to_string()),
                ]
                .into_iter()
                .collect(),
                reason: "Find code related to the exploration topic".to_string(),
                estimated_tokens: 2000,
                dependencies: vec![1],
            });
        }

        // Complete
        phases.push(PlanPhase {
            phase: 3,
            action: ActionType::Complete,
            target: "exploration".to_string(),
            params: HashMap::new(),
            reason: "Exploration complete".to_string(),
            estimated_tokens: 100,
            dependencies: vec![2],
        });

        Ok(phases)
    }

    /// Assess risk level for the plan
    fn assess_risk(&self, _phases: &[PlanPhase], files: &[PathBuf]) -> RiskLevel {
        let goal_lower = self.goal.to_lowercase();

        // Critical: Modifying core/safety/evolution
        let critical_patterns = ["safety", "evolution", "core", "verification"];
        if files.iter().any(|f| {
            let s = f.to_string_lossy().to_lowercase();
            critical_patterns.iter().any(|p| s.contains(p))
        }) {
            return RiskLevel::Critical;
        }

        // High: Many files or complex changes
        if files.len() > 10 {
            return RiskLevel::High;
        }

        // High: Modifying public APIs
        if goal_lower.contains("api") || goal_lower.contains("public") {
            return RiskLevel::High;
        }

        // Medium: Standard modifications
        if goal_lower.contains("modify") || goal_lower.contains("change") {
            return RiskLevel::Medium;
        }

        RiskLevel::Low
    }

    /// Define success criteria based on goal
    fn define_success_criteria(&self, keywords: &[String]) -> Vec<String> {
        let mut criteria = vec![
            "Code compiles without errors".to_string(),
            "Existing tests pass".to_string(),
        ];

        if keywords
            .iter()
            .any(|k| k.contains("fix") || k.contains("bug"))
        {
            criteria.push("The specific issue is resolved".to_string());
        }

        if keywords
            .iter()
            .any(|k| k.contains("optimize") || k.contains("performance"))
        {
            criteria.push("Performance metrics improve".to_string());
        }

        if keywords.iter().any(|k| k.contains("test")) {
            criteria.push("New tests cover the changes".to_string());
        }

        criteria
    }
}

fn measure_signature_phase_tokens(path: &Path, max_cap: usize, fallback: usize) -> usize {
    let Ok(content) =
        crate::tools::file::read_to_string_capped(path, crate::tools::file::MAX_SOURCE_READ_SIZE)
    else {
        return fallback;
    };
    let measured = if path.extension().and_then(|e| e.to_str()) == Some("rs") {
        crate::evolve::skeleton::extract_rust_skeleton(path, &content).token_count
    } else {
        crate::token_count::estimate_content_tokens(&content)
    };
    measured.clamp(1, max_cap)
}

/// Planning strategies
#[derive(Debug, Clone)]
enum PlanningStrategy {
    /// Start with broad introspection, then narrow down
    IntrospectionFirst,
    /// Broad introspection with an explicit dependency impact analysis phase
    ImpactAnalysis,
    /// Go directly to relevant files
    TargetedChange,
    /// Broad exploration without specific modifications
    Exploratory,
}

/// Change impact analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactAnalysis {
    pub target_file: String,
    pub target_symbol: Option<String>,
    pub direct_callers: Vec<CallerInfo>,
    pub transitive_deps: Vec<String>,
    pub tests_affected: Vec<String>,
    pub estimated_files_to_update: usize,
    pub suggested_order: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallerInfo {
    pub file: String,
    pub line: u32,
    pub context: String,
}

/// Analyze impact of a code change
pub async fn analyze_impact(
    target_file: &Path,
    symbol: Option<&str>,
    codebase_root: &Path,
) -> Result<ImpactAnalysis> {
    analyze_impact_with_safety(target_file, symbol, None, codebase_root, None).await
}

pub(super) async fn analyze_impact_with_safety(
    target_file: &Path,
    symbol: Option<&str>,
    change_type: Option<&str>,
    codebase_root: &Path,
    safety: Option<&crate::config::SafetyConfig>,
) -> Result<ImpactAnalysis> {
    let mut direct_callers = Vec::new();
    let mut transitive_deps = Vec::new();
    let mut tests_affected = Vec::new();

    // Get target file content and parse exported symbol names when no
    // explicit symbol was requested.
    let target_content = crate::tools::file::read_to_string_capped_async(
        target_file,
        crate::tools::file::MAX_SOURCE_READ_SIZE,
    )
    .await?;
    let target_name = target_file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let parsed_target = super::parser::parse(
        &target_content,
        super::parser::Language::detect(target_file, None),
    );
    let target_symbols: Vec<String> = parsed_target
        .symbols
        .into_iter()
        .filter(|s| {
            matches!(
                s.visibility,
                super::parser::Visibility::Public | super::parser::Visibility::PublicCrate
            )
        })
        .map(|s| s.name)
        .collect();

    // Use the same bounded, policy-checked, no-symlink walk as every other
    // introspection tool. `DirEntry::path().is_file()` follows symlinks, so
    // the old top-level loop could read a symlinked source even though the
    // other introspection paths refused implicit symlink inputs.
    let root = codebase_root.to_path_buf();
    let safety = safety.cloned();
    let files = crate::tools::workspace_root::spawn_blocking(move || {
        super::walk::source_files(&root, safety.as_ref(), super::MAX_WALK_DEPTH)
    })
    .await??
    .files;

    let mut non_caller_files: Vec<(PathBuf, String)> = Vec::new();
    let mut caller_stems: Vec<String> = Vec::new();

    for path in files {
        if path == target_file {
            continue;
        }
        // The walk already chose this path. Read that descriptor so a
        // hard-link swap cannot replace the bytes after the link-count check.
        if let Some(content) = super::read_introspect_source(&path, false).await {
            let file_name = path.to_string_lossy().to_lowercase();

            let matched_line = content.lines().enumerate().find_map(|(idx, line)| {
                let matches_symbol = match symbol {
                    Some(s) => !s.is_empty() && line.contains(s),
                    None => target_symbols
                        .iter()
                        .any(|sym| !sym.is_empty() && line.contains(sym.as_str())),
                };
                let matches_target = !target_name.is_empty()
                    && target_name != "mod"
                    && target_name != "lib"
                    && line.contains(&target_name);
                if matches_symbol || matches_target {
                    Some(((idx + 1) as u32, line.trim().to_string()))
                } else {
                    None
                }
            });

            if let Some((line_no, line_text)) = matched_line {
                let context = if line_text.is_empty() {
                    format!("References {}", symbol.unwrap_or(&target_name))
                } else {
                    line_text
                };
                let info = CallerInfo {
                    file: path.to_string_lossy().to_string(),
                    line: line_no,
                    context,
                };

                if file_name.contains("test") {
                    tests_affected.push(path.to_string_lossy().to_string());
                } else {
                    if let Some(stem) = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .filter(|s| !s.is_empty() && s != "mod" && s != "lib" && s != "main")
                    {
                        caller_stems.push(stem);
                    }
                    direct_callers.push(info);
                }
            } else if !file_name.contains("test") {
                non_caller_files.push((path, content));
            }
        }
    }

    if !caller_stems.is_empty() {
        for (path, content) in non_caller_files {
            if caller_stems
                .iter()
                .any(|stem| content.lines().any(|line| line.contains(stem.as_str())))
            {
                transitive_deps.push(path.to_string_lossy().to_string());
            }
        }
    }

    let estimated_files = direct_callers.len();
    let suggested_order = match change_type {
        Some("delete") => vec![
            "Remove or migrate references in direct callers".to_string(),
            "Update transitive dependencies".to_string(),
            "Update or remove affected tests".to_string(),
            "Delete target and verify compilation".to_string(),
        ],
        Some("rename") => vec![
            "Rename symbol/module in target".to_string(),
            "Update direct callers to new name".to_string(),
            "Update transitive dependencies".to_string(),
            "Update tests".to_string(),
            "Verify compilation".to_string(),
        ],
        _ => vec![
            "Update direct callers".to_string(),
            "Update tests".to_string(),
            "Verify compilation".to_string(),
        ],
    };

    Ok(ImpactAnalysis {
        target_file: target_file.to_string_lossy().to_string(),
        target_symbol: symbol.map(|s| s.to_string()),
        direct_callers,
        transitive_deps,
        tests_affected,
        estimated_files_to_update: estimated_files,
        suggested_order,
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/tools/introspect/planner/planner_test.rs"]
mod tests;
