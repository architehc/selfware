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
        }
    }

    /// Generate an evolution plan
    pub async fn generate_plan(&self) -> Result<EvolutionPlan> {
        // Extract keywords from goal to understand intent
        let keywords = extract_keywords(&self.goal);

        // Analyze goal to determine strategy
        let strategy = self.determine_strategy(&keywords);

        // Find relevant files
        let relevant_files = self.find_relevant_files(&keywords).await?;

        // Generate phases based on strategy
        let phases = match strategy {
            PlanningStrategy::IntrospectionFirst => {
                self.plan_introspection_first(&relevant_files).await?
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
        let mut files = Vec::new();

        // Walk the codebase
        let mut entries = tokio::fs::read_dir(&self.codebase_root).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            // Skip non-source directories
            if let Some(name) = path.file_name() {
                let name = name.to_string_lossy();
                if matches!(
                    name.as_ref(),
                    "target" | "node_modules" | ".git" | "__pycache__"
                ) {
                    continue;
                }
            }

            if path.is_file() && Self::is_source_file(&path) {
                files.push(path);
            } else if path.is_dir() {
                files.extend(self.collect_source_files(&path).await?);
            }
        }

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

    #[allow(clippy::only_used_in_recursion)]
    fn collect_source_files<'a>(
        &'a self,
        dir: &'a Path,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<PathBuf>>> + Send + 'a>>
    {
        Box::pin(async move {
            let mut files = Vec::new();

            if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let path = entry.path();

                    if let Some(name) = path.file_name() {
                        let name = name.to_string_lossy();
                        if matches!(
                            name.as_ref(),
                            "target" | "node_modules" | ".git" | "__pycache__"
                        ) {
                            continue;
                        }
                    }

                    if path.is_file() && Self::is_source_file(&path) {
                        files.push(path);
                    } else if path.is_dir() {
                        files.extend(self.collect_source_files(&path).await?);
                    }
                }
            }

            Ok(files)
        })
    }

    fn is_source_file(path: &Path) -> bool {
        let extensions = ["rs", "py", "js", "ts", "go", "java"];
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| extensions.contains(&e))
            .unwrap_or(false)
    }

    /// Plan with introspection-first strategy
    async fn plan_introspection_first(&self, files: &[PathBuf]) -> Result<Vec<PlanPhase>> {
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
                estimated_tokens: 1000,
                dependencies: vec![phase_num - 1],
            });
            phase_num += 1;
        }

        // Phase 4: Impact analysis if modifying
        if self.goal.to_lowercase().contains("modify")
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
                estimated_tokens: 800,
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

/// Planning strategies
#[derive(Debug, Clone)]
enum PlanningStrategy {
    /// Start with broad introspection, then narrow down
    IntrospectionFirst,
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
    target_file: &PathBuf,
    symbol: Option<&str>,
    codebase_root: &PathBuf,
) -> Result<ImpactAnalysis> {
    let mut direct_callers = Vec::new();
    let transitive_deps = Vec::new();
    let mut tests_affected = Vec::new();

    // Get target file content
    let _target_content = tokio::fs::read_to_string(target_file).await?;
    let target_name = target_file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    // Walk codebase to find references
    let mut entries = tokio::fs::read_dir(codebase_root).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        if path.is_file() && path != *target_file {
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                // Check for imports/references to target
                let file_name = path.to_string_lossy().to_lowercase();

                // Simple text-based search for references
                if content.contains(&target_name)
                    || (symbol.is_some() && content.contains(symbol.unwrap()))
                {
                    let info = CallerInfo {
                        file: path.to_string_lossy().to_string(),
                        line: 1, // Would need line-by-line analysis
                        context: format!("References {}", target_name),
                    };

                    if file_name.contains("test") {
                        tests_affected.push(path.to_string_lossy().to_string());
                    } else {
                        direct_callers.push(info);
                    }
                }
            }
        }
    }

    let estimated_files = direct_callers.len();

    Ok(ImpactAnalysis {
        target_file: target_file.to_string_lossy().to_string(),
        target_symbol: symbol.map(|s| s.to_string()),
        direct_callers,
        transitive_deps,
        tests_affected,
        estimated_files_to_update: estimated_files,
        suggested_order: vec![
            "Update direct callers".to_string(),
            "Update tests".to_string(),
            "Verify compilation".to_string(),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_planning_strategy_detection() {
        let planner = EvolutionPlanner::new(
            "Optimize the agent loop".to_string(),
            20,
            10000,
            PathBuf::from("."),
        );

        let keywords = extract_keywords("Optimize the agent loop");
        let strategy = planner.determine_strategy(&keywords);

        // Should detect introspection-first for "optimize"
        match strategy {
            PlanningStrategy::IntrospectionFirst => (),
            _ => panic!("Expected IntrospectionFirst strategy"),
        }
    }

    #[test]
    fn test_risk_assessment() {
        let planner = EvolutionPlanner::new("Fix bug".to_string(), 20, 10000, PathBuf::from("."));

        let phases = vec![];
        let files = vec![PathBuf::from("src/main.rs")];

        let risk = planner.assess_risk(&phases, &files);
        assert!(matches!(risk, RiskLevel::Low | RiskLevel::Medium));
    }

    // =========================================================================
    // ActionType tests
    // =========================================================================

    #[test]
    fn test_action_type_as_str_introspect() {
        assert_eq!(ActionType::Introspect.as_str(), "introspect");
    }

    #[test]
    fn test_action_type_as_str_query() {
        assert_eq!(ActionType::Query.as_str(), "query");
    }

    #[test]
    fn test_action_type_as_str_read() {
        assert_eq!(ActionType::Read.as_str(), "read");
    }

    #[test]
    fn test_action_type_as_str_impact_analysis() {
        assert_eq!(ActionType::ImpactAnalysis.as_str(), "impact_analysis");
    }

    #[test]
    fn test_action_type_as_str_modify() {
        assert_eq!(ActionType::Modify.as_str(), "modify");
    }

    #[test]
    fn test_action_type_as_str_verify() {
        assert_eq!(ActionType::Verify.as_str(), "verify");
    }

    #[test]
    fn test_action_type_as_str_test() {
        assert_eq!(ActionType::Test.as_str(), "test");
    }

    #[test]
    fn test_action_type_as_str_complete() {
        assert_eq!(ActionType::Complete.as_str(), "complete");
    }

    // =========================================================================
    // RiskLevel tests
    // =========================================================================

    #[test]
    fn test_risk_level_as_str_low() {
        assert_eq!(RiskLevel::Low.as_str(), "low");
    }

    #[test]
    fn test_risk_level_as_str_medium() {
        assert_eq!(RiskLevel::Medium.as_str(), "medium");
    }

    #[test]
    fn test_risk_level_as_str_high() {
        assert_eq!(RiskLevel::High.as_str(), "high");
    }

    #[test]
    fn test_risk_level_as_str_critical() {
        assert_eq!(RiskLevel::Critical.as_str(), "critical");
    }

    // =========================================================================
    // ActionType serialization tests
    // =========================================================================

    #[test]
    fn test_action_type_serde_roundtrip() {
        let actions = vec![
            ActionType::Introspect,
            ActionType::Query,
            ActionType::Read,
            ActionType::ImpactAnalysis,
            ActionType::Modify,
            ActionType::Verify,
            ActionType::Test,
            ActionType::Complete,
        ];
        for action in actions {
            let json = serde_json::to_string(&action).unwrap();
            let parsed: ActionType = serde_json::from_str(&json).unwrap();
            assert_eq!(action.as_str(), parsed.as_str());
        }
    }

    #[test]
    fn test_risk_level_serde_roundtrip() {
        let levels = vec![
            RiskLevel::Low,
            RiskLevel::Medium,
            RiskLevel::High,
            RiskLevel::Critical,
        ];
        for level in levels {
            let json = serde_json::to_string(&level).unwrap();
            let parsed: RiskLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(level.as_str(), parsed.as_str());
        }
    }

    // =========================================================================
    // determine_strategy tests
    // =========================================================================

    #[test]
    fn test_strategy_fix_bug_is_targeted() {
        let planner = EvolutionPlanner::new("Fix the failing test".to_string(), 20, 10000, PathBuf::from("."));
        let keywords = extract_keywords("Fix the failing test");
        let strategy = planner.determine_strategy(&keywords);
        assert!(matches!(strategy, PlanningStrategy::TargetedChange));
    }

    #[test]
    fn test_strategy_error_is_targeted() {
        let planner = EvolutionPlanner::new("Resolve error in parser".to_string(), 20, 10000, PathBuf::from("."));
        let keywords = extract_keywords("Resolve error in parser");
        let strategy = planner.determine_strategy(&keywords);
        assert!(matches!(strategy, PlanningStrategy::TargetedChange));
    }

    #[test]
    fn test_strategy_refactor_is_introspection() {
        let planner = EvolutionPlanner::new("Refactor the agent module".to_string(), 20, 10000, PathBuf::from("."));
        let keywords = extract_keywords("Refactor the agent module");
        let strategy = planner.determine_strategy(&keywords);
        assert!(matches!(strategy, PlanningStrategy::IntrospectionFirst));
    }

    #[test]
    fn test_strategy_improve_is_introspection() {
        let planner = EvolutionPlanner::new("Improve error handling".to_string(), 20, 10000, PathBuf::from("."));
        let keywords = extract_keywords("Improve error handling");
        let strategy = planner.determine_strategy(&keywords);
        assert!(matches!(strategy, PlanningStrategy::IntrospectionFirst));
    }

    #[test]
    fn test_strategy_add_is_exploratory() {
        let planner = EvolutionPlanner::new("Add new logging module".to_string(), 20, 10000, PathBuf::from("."));
        let keywords = extract_keywords("Add new logging module");
        let strategy = planner.determine_strategy(&keywords);
        assert!(matches!(strategy, PlanningStrategy::Exploratory));
    }

    #[test]
    fn test_strategy_implement_is_exploratory() {
        let planner = EvolutionPlanner::new("Implement rate limiting".to_string(), 20, 10000, PathBuf::from("."));
        let keywords = extract_keywords("Implement rate limiting");
        let strategy = planner.determine_strategy(&keywords);
        assert!(matches!(strategy, PlanningStrategy::Exploratory));
    }

    #[test]
    fn test_strategy_understand_is_exploratory() {
        let planner = EvolutionPlanner::new("understand the codebase".to_string(), 20, 10000, PathBuf::from("."));
        let keywords = extract_keywords("understand the codebase");
        let strategy = planner.determine_strategy(&keywords);
        assert!(matches!(strategy, PlanningStrategy::Exploratory));
    }

    #[test]
    fn test_strategy_default_is_introspection() {
        let planner = EvolutionPlanner::new("do something general".to_string(), 20, 10000, PathBuf::from("."));
        let keywords = extract_keywords("do something general");
        let strategy = planner.determine_strategy(&keywords);
        assert!(matches!(strategy, PlanningStrategy::IntrospectionFirst));
    }

    // =========================================================================
    // assess_risk tests
    // =========================================================================

    #[test]
    fn test_risk_safety_file_is_critical() {
        let planner = EvolutionPlanner::new("change safety".to_string(), 20, 10000, PathBuf::from("."));
        let files = vec![PathBuf::from("src/safety/checker.rs")];
        let risk = planner.assess_risk(&[], &files);
        assert!(matches!(risk, RiskLevel::Critical));
    }

    #[test]
    fn test_risk_evolution_file_is_critical() {
        let planner = EvolutionPlanner::new("edit core".to_string(), 20, 10000, PathBuf::from("."));
        let files = vec![PathBuf::from("src/evolution/mod.rs")];
        let risk = planner.assess_risk(&[], &files);
        assert!(matches!(risk, RiskLevel::Critical));
    }

    #[test]
    fn test_risk_many_files_is_high() {
        let planner = EvolutionPlanner::new("update docs".to_string(), 20, 10000, PathBuf::from("."));
        let files: Vec<PathBuf> = (0..15).map(|i| PathBuf::from(format!("src/file{}.rs", i))).collect();
        let risk = planner.assess_risk(&[], &files);
        assert!(matches!(risk, RiskLevel::High));
    }

    #[test]
    fn test_risk_api_goal_is_high() {
        let planner = EvolutionPlanner::new("change the public api".to_string(), 20, 10000, PathBuf::from("."));
        let files = vec![PathBuf::from("src/lib.rs")];
        let risk = planner.assess_risk(&[], &files);
        assert!(matches!(risk, RiskLevel::High));
    }

    #[test]
    fn test_risk_modify_is_medium() {
        let planner = EvolutionPlanner::new("modify config loader".to_string(), 20, 10000, PathBuf::from("."));
        let files = vec![PathBuf::from("src/config.rs")];
        let risk = planner.assess_risk(&[], &files);
        assert!(matches!(risk, RiskLevel::Medium));
    }

    #[test]
    fn test_risk_simple_task_is_low() {
        let planner = EvolutionPlanner::new("read docs".to_string(), 20, 10000, PathBuf::from("."));
        let files = vec![PathBuf::from("README.md")];
        let risk = planner.assess_risk(&[], &files);
        assert!(matches!(risk, RiskLevel::Low));
    }

    // =========================================================================
    // define_success_criteria tests
    // =========================================================================

    #[test]
    fn test_success_criteria_always_has_compiles() {
        let planner = EvolutionPlanner::new("anything".to_string(), 20, 10000, PathBuf::from("."));
        let criteria = planner.define_success_criteria(&[]);
        assert!(criteria.iter().any(|c| c.contains("compiles")));
    }

    #[test]
    fn test_success_criteria_always_has_tests_pass() {
        let planner = EvolutionPlanner::new("anything".to_string(), 20, 10000, PathBuf::from("."));
        let criteria = planner.define_success_criteria(&[]);
        assert!(criteria.iter().any(|c| c.contains("tests pass")));
    }

    #[test]
    fn test_success_criteria_fix_adds_resolved() {
        let planner = EvolutionPlanner::new("fix bug".to_string(), 20, 10000, PathBuf::from("."));
        let criteria = planner.define_success_criteria(&["fix".to_string()]);
        assert!(criteria.iter().any(|c| c.contains("resolved")));
    }

    #[test]
    fn test_success_criteria_optimize_adds_performance() {
        let planner = EvolutionPlanner::new("optimize".to_string(), 20, 10000, PathBuf::from("."));
        let criteria = planner.define_success_criteria(&["optimize".to_string()]);
        assert!(criteria.iter().any(|c| c.contains("Performance")));
    }

    #[test]
    fn test_success_criteria_test_adds_coverage() {
        let planner = EvolutionPlanner::new("test".to_string(), 20, 10000, PathBuf::from("."));
        let criteria = planner.define_success_criteria(&["test".to_string()]);
        assert!(criteria.iter().any(|c| c.contains("cover")));
    }

    // =========================================================================
    // is_source_file tests
    // =========================================================================

    #[test]
    fn test_is_source_file_rs() {
        assert!(EvolutionPlanner::is_source_file(Path::new("src/main.rs")));
    }

    #[test]
    fn test_is_source_file_py() {
        assert!(EvolutionPlanner::is_source_file(Path::new("script.py")));
    }

    #[test]
    fn test_is_source_file_js() {
        assert!(EvolutionPlanner::is_source_file(Path::new("app.js")));
    }

    #[test]
    fn test_is_source_file_ts() {
        assert!(EvolutionPlanner::is_source_file(Path::new("app.ts")));
    }

    #[test]
    fn test_is_source_file_go() {
        assert!(EvolutionPlanner::is_source_file(Path::new("main.go")));
    }

    #[test]
    fn test_is_source_file_java() {
        assert!(EvolutionPlanner::is_source_file(Path::new("App.java")));
    }

    #[test]
    fn test_is_source_file_txt_not() {
        assert!(!EvolutionPlanner::is_source_file(Path::new("readme.txt")));
    }

    #[test]
    fn test_is_source_file_toml_not() {
        assert!(!EvolutionPlanner::is_source_file(Path::new("Cargo.toml")));
    }

    #[test]
    fn test_is_source_file_no_extension_not() {
        assert!(!EvolutionPlanner::is_source_file(Path::new("Makefile")));
    }

    // =========================================================================
    // PlanPhase struct tests
    // =========================================================================

    #[test]
    fn test_plan_phase_serialization() {
        let phase = PlanPhase {
            phase: 1,
            action: ActionType::Introspect,
            target: "src/main.rs".to_string(),
            params: HashMap::new(),
            reason: "Understand the codebase".to_string(),
            estimated_tokens: 1000,
            dependencies: vec![],
        };
        let json = serde_json::to_string(&phase).unwrap();
        assert!(json.contains("introspect"));
        assert!(json.contains("src/main.rs"));
    }

    #[test]
    fn test_plan_phase_with_dependencies() {
        let phase = PlanPhase {
            phase: 3,
            action: ActionType::Modify,
            target: "src/lib.rs".to_string(),
            params: HashMap::from([("goal".to_string(), "add feature".to_string())]),
            reason: "Apply changes".to_string(),
            estimated_tokens: 2000,
            dependencies: vec![1, 2],
        };
        assert_eq!(phase.dependencies, vec![1, 2]);
    }

    // =========================================================================
    // EvolutionPlan struct tests
    // =========================================================================

    #[test]
    fn test_evolution_plan_serialization() {
        let plan = EvolutionPlan {
            goal: "Test goal".to_string(),
            phases: vec![],
            estimated_tokens: 0,
            token_budget: 10000,
            iteration_budget: 20,
            risk: RiskLevel::Low,
            success_criteria: vec!["Code compiles".to_string()],
        };
        let json = serde_json::to_string(&plan).unwrap();
        assert!(json.contains("Test goal"));
        assert!(json.contains("low"));
    }

    // =========================================================================
    // ImpactAnalysis / CallerInfo struct tests
    // =========================================================================

    #[test]
    fn test_caller_info_serialization() {
        let info = CallerInfo {
            file: "src/main.rs".to_string(),
            line: 42,
            context: "References config".to_string(),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("src/main.rs"));
        assert!(json.contains("42"));
    }

    #[test]
    fn test_impact_analysis_serialization() {
        let analysis = ImpactAnalysis {
            target_file: "src/lib.rs".to_string(),
            target_symbol: Some("my_fn".to_string()),
            direct_callers: vec![],
            transitive_deps: vec![],
            tests_affected: vec!["test_my_fn".to_string()],
            estimated_files_to_update: 0,
            suggested_order: vec!["Update callers".to_string()],
        };
        let json = serde_json::to_string(&analysis).unwrap();
        assert!(json.contains("src/lib.rs"));
        assert!(json.contains("my_fn"));
        assert!(json.contains("test_my_fn"));
    }
}
