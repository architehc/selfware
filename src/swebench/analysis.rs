//! SWE-bench Analysis Engine
//!
//! Analyzes evaluation results to identify patterns, failure modes,
//! and generate actionable recommendations for improvement.

use std::collections::HashMap;

use super::{SWEEvaluationReport, TaskResult, TaskDifficulty};

/// Identified failure pattern
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FailurePattern {
    /// Task hit timeout
    Timeout,
    /// Agent modified wrong files
    WrongFiles,
    /// Patch was incomplete
    IncompletePatch,
    /// Fix broke other tests
    TestRegression,
    /// Generated invalid syntax
    SyntaxError,
    /// Agent got stuck in a loop
    StuckLoop,
    /// Failed to understand problem
    MisunderstoodProblem,
    /// Tool execution failure
    ToolFailure,
    /// Out of tokens
    TokenExhaustion,
    /// Other/Unknown
    Other(String),
}

impl FailurePattern {
    /// Get human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            FailurePattern::Timeout => "Task exceeded time limit",
            FailurePattern::WrongFiles => "Modified incorrect files",
            FailurePattern::IncompletePatch => "Patch didn't cover all necessary changes",
            FailurePattern::TestRegression => "Fix broke existing functionality",
            FailurePattern::SyntaxError => "Generated code had syntax errors",
            FailurePattern::StuckLoop => "Agent got stuck in repetitive behavior",
            FailurePattern::MisunderstoodProblem => "Failed to understand the problem",
            FailurePattern::ToolFailure => "Tool execution failed",
            FailurePattern::TokenExhaustion => "Ran out of tokens",
            FailurePattern::Other(_) => "Other failure",
        }
    }

    /// Get recommended fix
    pub fn recommendation(&self) -> &'static str {
        match self {
            FailurePattern::Timeout => "Increase timeout or optimize agent efficiency",
            FailurePattern::WrongFiles => "Improve file search and relevance ranking",
            FailurePattern::IncompletePatch => "Enhance agent's ability to identify all necessary changes",
            FailurePattern::TestRegression => "Add regression test awareness to agent",
            FailurePattern::SyntaxError => "Add syntax validation before patch generation",
            FailurePattern::StuckLoop => "Improve loop detection and breaking",
            FailurePattern::MisunderstoodProblem => "Enhance problem statement parsing",
            FailurePattern::ToolFailure => "Improve error handling and retry logic",
            FailurePattern::TokenExhaustion => "Optimize prompt efficiency or increase budget",
            FailurePattern::Other(_) => "Review logs for details",
        }
    }
}

/// Analysis recommendation
#[derive(Debug, Clone)]
pub struct Recommendation {
    /// Category of improvement
    pub category: String,
    /// Specific recommendation
    pub recommendation: String,
    /// Expected impact (0.0-1.0)
    pub impact: f64,
    /// Effort required (Low/Medium/High)
    pub effort: String,
    /// Affected tasks
    pub affected_tasks: Vec<String>,
}

/// Comprehensive analysis of evaluation results
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    /// Failure patterns identified
    pub failure_patterns: HashMap<FailurePattern, usize>,
    /// Per-difficulty analysis
    pub difficulty_analysis: HashMap<TaskDifficulty, DifficultyAnalysis>,
    /// Tool usage statistics
    pub tool_usage: HashMap<String, usize>,
    /// Error distribution
    pub error_types: HashMap<String, usize>,
    /// Recommendations
    pub recommendations: Vec<Recommendation>,
}

/// Analysis for a specific difficulty level
#[derive(Debug, Clone)]
pub struct DifficultyAnalysis {
    pub difficulty: TaskDifficulty,
    pub total: usize,
    pub resolved: usize,
    pub avg_iterations: f64,
    pub avg_tokens: f64,
    pub common_failures: Vec<FailurePattern>,
}

/// Analysis engine
pub struct AnalysisEngine;

impl AnalysisEngine {
    /// Create a new analysis engine
    pub fn new() -> Self {
        Self
    }

    /// Analyze evaluation results
    pub fn analyze(&self, report: &SWEEvaluationReport) -> AnalysisResult {
        let failure_patterns = self.identify_failure_patterns(&report.results);
        let difficulty_analysis = self.analyze_by_difficulty(&report.results);
        let tool_usage = self.analyze_tool_usage(&report.results);
        let error_types = self.categorize_errors(&report.results);
        let recommendations = self.generate_recommendations(&failure_patterns, &difficulty_analysis);

        AnalysisResult {
            failure_patterns,
            difficulty_analysis,
            tool_usage,
            error_types,
            recommendations,
        }
    }

    /// Identify failure patterns from results
    fn identify_failure_patterns(&self, results: &[TaskResult]) -> HashMap<FailurePattern, usize> {
        let mut patterns: HashMap<FailurePattern, usize> = HashMap::new();

        for result in results {
            if result.resolved {
                continue;
            }

            let pattern = self.classify_failure(result);
            *patterns.entry(pattern).or_default() += 1;
        }

        patterns
    }

    /// Classify a single failure
    fn classify_failure(&self, result: &TaskResult) -> FailurePattern {
        // Check error message first
        if let Some(ref error) = result.error {
            let error_lower = error.to_lowercase();
            
            if error_lower.contains("timeout") || result.duration_secs > 600.0 {
                return FailurePattern::Timeout;
            }
            
            if error_lower.contains("token") && error_lower.contains("exhaust") {
                return FailurePattern::TokenExhaustion;
            }
            
            if error_lower.contains("syntax") || error_lower.contains("parse") {
                return FailurePattern::SyntaxError;
            }
            
            if error_lower.contains("tool") && error_lower.contains("fail") {
                return FailurePattern::ToolFailure;
            }
        }

        // Check trajectory for stuck loops
        if self.detect_loop(&result.trajectory) {
            return FailurePattern::StuckLoop;
        }

        // Check if patch was generated but tests still fail
        if !result.files_modified.is_empty() {
            if let Some(ref test_output) = result.test_output {
                if !test_output.success {
                    if test_output.stderr.contains("regression") || 
                       test_output.stdout.contains("passed") && test_output.stdout.contains("failed") {
                        return FailurePattern::TestRegression;
                    }
                    return FailurePattern::IncompletePatch;
                }
            }
        }

        // Check if wrong files were modified
        if !result.files_modified.is_empty() && result.patch_quality < 0.3 {
            return FailurePattern::WrongFiles;
        }

        // Default
        FailurePattern::Other(result.error.clone().unwrap_or_default())
    }

    /// Detect repetitive behavior in trajectory
    fn detect_loop(&self, trajectory: &[super::TrajectoryStep]) -> bool {
        if trajectory.len() < 6 {
            return false;
        }

        // Check for repeated actions
        let actions: Vec<&str> = trajectory.iter().map(|s| s.action.as_str()).collect();
        
        // Look for patterns of length 2-4 repeating at least 3 times
        for pattern_len in 2..=4 {
            if trajectory.len() < pattern_len * 3 {
                continue;
            }
            
            let last_chunk = &actions[actions.len() - pattern_len..];
            let prev_chunk = &actions[actions.len() - pattern_len * 2..actions.len() - pattern_len];
            let prev_prev_chunk = &actions[actions.len() - pattern_len * 3..actions.len() - pattern_len * 2];
            
            if last_chunk == prev_chunk && prev_chunk == prev_prev_chunk {
                return true;
            }
        }

        false
    }

    /// Analyze results by difficulty
    fn analyze_by_difficulty(&self, results: &[TaskResult]) -> HashMap<TaskDifficulty, DifficultyAnalysis> {
        let mut grouped: HashMap<TaskDifficulty, Vec<&TaskResult>> = HashMap::new();

        for result in results {
            let difficulty = self.estimate_difficulty(result);
            grouped.entry(difficulty).or_default().push(result);
        }

        grouped
            .into_iter()
            .map(|(diff, results)| {
                let total = results.len();
                let resolved = results.iter().filter(|r| r.resolved).count();
                let avg_iterations = results.iter().map(|r| r.iterations as f64).sum::<f64>() / total as f64;
                let avg_tokens = results.iter().map(|r| r.tokens_used as f64).sum::<f64>() / total as f64;

                // Find common failures
                let failures: Vec<FailurePattern> = results
                    .iter()
                    .filter(|r| !r.resolved)
                    .map(|r| self.classify_failure(r))
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .take(3)
                    .collect();

                (diff.clone(), DifficultyAnalysis {
                    difficulty: diff,
                    total,
                    resolved,
                    avg_iterations,
                    avg_tokens,
                    common_failures: failures,
                })
            })
            .collect()
    }

    /// Estimate difficulty from task characteristics
    fn estimate_difficulty(&self, result: &TaskResult) -> TaskDifficulty {
        match (result.duration_secs, result.iterations) {
            (d, i) if d < 60.0 && i < 5 => TaskDifficulty::Easy,
            (d, i) if d < 180.0 && i < 15 => TaskDifficulty::Medium,
            (d, i) if d < 300.0 && i < 30 => TaskDifficulty::Hard,
            _ => TaskDifficulty::Expert,
        }
    }

    /// Analyze tool usage from trajectories
    fn analyze_tool_usage(&self, results: &[TaskResult]) -> HashMap<String, usize> {
        let mut usage: HashMap<String, usize> = HashMap::new();

        for result in results {
            for step in &result.trajectory {
                if let Some(ref tools) = step.tool_calls {
                    for tool in tools {
                        *usage.entry(tool.clone()).or_default() += 1;
                    }
                }
            }
        }

        usage
    }

    /// Categorize error types
    fn categorize_errors(&self, results: &[TaskResult]) -> HashMap<String, usize> {
        let mut errors: HashMap<String, usize> = HashMap::new();

        for result in results {
            if let Some(ref error) = result.error {
                // Extract first word/category
                let category = if error.contains(':') {
                    error.split(':').next().unwrap_or("Unknown").to_string()
                } else {
                    "General".to_string()
                };
                *errors.entry(category).or_default() += 1;
            }
        }

        errors
    }

    /// Generate improvement recommendations
    fn generate_recommendations(
        &self,
        failure_patterns: &HashMap<FailurePattern, usize>,
        difficulty_analysis: &HashMap<TaskDifficulty, DifficultyAnalysis>,
    ) -> Vec<Recommendation> {
        let mut recommendations = Vec::new();

        // Sort patterns by frequency
        let mut sorted_patterns: Vec<_> = failure_patterns.iter().collect();
        sorted_patterns.sort_by(|a, b| b.1.cmp(a.1));

        // Generate recommendations for top 3 failure patterns
        for (pattern, count) in sorted_patterns.iter().take(3) {
            let impact = (*count as f64 / failure_patterns.values().sum::<usize>() as f64).min(1.0);
            
            recommendations.push(Recommendation {
                category: "Failure Pattern".to_string(),
                recommendation: pattern.recommendation().to_string(),
                impact,
                effort: "Medium".to_string(),
                affected_tasks: vec![], // Would populate from actual results
            });
        }

        // Add difficulty-specific recommendations
        if let Some(expert) = difficulty_analysis.get(&TaskDifficulty::Expert) {
            if expert.resolved as f64 / expert.total as f64 < 0.1 {
                recommendations.push(Recommendation {
                    category: "Difficulty Gap".to_string(),
                    recommendation: "Consider adding specialized handling for complex tasks".to_string(),
                    impact: 0.3,
                    effort: "High".to_string(),
                    affected_tasks: vec![],
                });
            }
        }

        recommendations
    }
}

impl Default for AnalysisEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate analysis report in Markdown
pub fn generate_analysis_markdown(analysis: &AnalysisResult) -> String {
    let mut md = String::new();

    md.push_str("# SWE-bench Analysis Report\n\n");

    // Failure Patterns
    md.push_str("## Failure Patterns\n\n");
    md.push_str("| Pattern | Count | Description | Recommendation |\n");
    md.push_str("|---------|-------|-------------|----------------|\n");

    let mut sorted_patterns: Vec<_> = analysis.failure_patterns.iter().collect();
    sorted_patterns.sort_by(|a, b| b.1.cmp(a.1));

    for (pattern, count) in sorted_patterns {
        md.push_str(&format!(
            "| {:?} | {} | {} | {} |\n",
            pattern, count, pattern.description(), pattern.recommendation()
        ));
    }
    md.push('\n');

    // Difficulty Analysis
    md.push_str("## Performance by Difficulty\n\n");
    md.push_str("| Difficulty | Tasks | Resolved | Rate | Avg Iterations | Avg Tokens |\n");
    md.push_str("|------------|-------|----------|------|----------------|------------|\n");

    let difficulties = vec![
        TaskDifficulty::Easy,
        TaskDifficulty::Medium,
        TaskDifficulty::Hard,
        TaskDifficulty::Expert,
    ];

    for diff in difficulties {
        if let Some(stats) = analysis.difficulty_analysis.get(&diff) {
            md.push_str(&format!(
                "| {} | {} | {} | {:.1}% | {:.1} | {:.0} |\n",
                diff,
                stats.total,
                stats.resolved,
                (stats.resolved as f64 / stats.total as f64) * 100.0,
                stats.avg_iterations,
                stats.avg_tokens
            ));
        }
    }
    md.push('\n');

    // Recommendations
    md.push_str("## Recommendations\n\n");
    for (i, rec) in analysis.recommendations.iter().enumerate() {
        md.push_str(&format!("{}. **{}** (Impact: {:.0}%, Effort: {})\n", 
            i + 1, rec.category, rec.impact * 100.0, rec.effort));
        md.push_str(&format!("   - {}\n\n", rec.recommendation));
    }

    md
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_result(resolved: bool, error: Option<&str>) -> TaskResult {
        TaskResult {
            task_id: "test-1".to_string(),
            resolved,
            patch_quality: if resolved { 0.8 } else { 0.3 },
            duration_secs: if resolved { 60.0 } else { 300.0 },
            tokens_used: 5000,
            iterations: if resolved { 5 } else { 50 },
            error: error.map(|e| e.to_string()),
            trajectory: vec![],
            test_output: None,
            files_modified: vec![],
        }
    }

    #[test]
    fn test_analysis_engine() {
        let engine = AnalysisEngine::new();
        
        let results = vec![
            create_test_result(true, None),
            create_test_result(false, Some("timeout")),
            create_test_result(false, Some("syntax error")),
        ];

        let patterns = engine.identify_failure_patterns(&results);
        
        assert_eq!(patterns.len(), 2);
        assert!(patterns.contains_key(&FailurePattern::Timeout));
        assert!(patterns.contains_key(&FailurePattern::SyntaxError));
    }

    #[test]
    fn test_detect_loop() {
        let engine = AnalysisEngine::new();

        let no_loop = vec![
            super::super::TrajectoryStep { step: 1, action: "read".to_string(), observation: "a".to_string(), timestamp: "t".to_string(), tool_calls: None, tokens_used: None },
            super::super::TrajectoryStep { step: 2, action: "edit".to_string(), observation: "b".to_string(), timestamp: "t".to_string(), tool_calls: None, tokens_used: None },
        ];
        assert!(!engine.detect_loop(&no_loop));

        let with_loop = vec![
            super::super::TrajectoryStep { step: 1, action: "read".to_string(), observation: "a".to_string(), timestamp: "t".to_string(), tool_calls: None, tokens_used: None },
            super::super::TrajectoryStep { step: 2, action: "edit".to_string(), observation: "b".to_string(), timestamp: "t".to_string(), tool_calls: None, tokens_used: None },
            super::super::TrajectoryStep { step: 3, action: "read".to_string(), observation: "c".to_string(), timestamp: "t".to_string(), tool_calls: None, tokens_used: None },
            super::super::TrajectoryStep { step: 4, action: "edit".to_string(), observation: "d".to_string(), timestamp: "t".to_string(), tool_calls: None, tokens_used: None },
            super::super::TrajectoryStep { step: 5, action: "read".to_string(), observation: "e".to_string(), timestamp: "t".to_string(), tool_calls: None, tokens_used: None },
            super::super::TrajectoryStep { step: 6, action: "edit".to_string(), observation: "f".to_string(), timestamp: "t".to_string(), tool_calls: None, tokens_used: None },
        ];
        assert!(engine.detect_loop(&with_loop));
    }

    #[test]
    fn test_failure_pattern_descriptions() {
        assert!(!FailurePattern::Timeout.description().is_empty());
        assert!(!FailurePattern::Timeout.recommendation().is_empty());
    }
}
