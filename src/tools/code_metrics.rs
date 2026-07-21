//! Code metrics analysis tool.
//!
//! Analyzes Rust source files and reports cyclomatic complexity
//! for each function, providing suggestions for simplification.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;

use super::Tool;

/// Cyclomatic complexity threshold levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ComplexityLevel {
    /// 1-10: Simple, easy to understand
    Low,
    /// 11-20: Moderate complexity
    Medium,
    /// 21-50: High complexity, should be refactored
    High,
    /// 50+: Critical complexity, immediate attention needed
    Critical,
}

impl ComplexityLevel {
    /// Classify complexity score into a level
    pub fn from_score(score: u32) -> Self {
        match score {
            1..=10 => Self::Low,
            11..=20 => Self::Medium,
            21..=50 => Self::High,
            _ => Self::Critical,
        }
    }

    /// Get a human-readable name
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    /// Get a suggestion based on complexity level
    pub fn suggestion(&self) -> &'static str {
        match self {
            Self::Low => "Complexity is good. No action needed.",
            Self::Medium => "Consider breaking into smaller functions if readability suffers.",
            Self::High => "Should be refactored. Extract helper functions to reduce complexity.",
            Self::Critical => "Critical complexity! Immediate refactoring required. Split into multiple functions.",
        }
    }
}

/// Function complexity report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionMetrics {
    /// Function name
    pub name: String,
    /// Line number where function starts
    pub line_number: usize,
    /// Cyclomatic complexity score
    pub complexity: u32,
    /// Complexity level classification
    pub level: String,
    /// Improvement suggestion
    pub suggestion: String,
}

/// Overall file metrics report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeMetricsReport {
    /// Path to analyzed file
    pub file: String,
    /// Per-function metrics
    pub functions: Vec<FunctionMetrics>,
    /// Summary statistics
    pub summary: MetricsSummary,
}

/// Summary statistics for the file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    /// Total number of functions analyzed
    pub total_functions: usize,
    /// Average complexity across all functions
    pub avg_complexity: f64,
    /// Maximum complexity found
    pub max_complexity: u32,
    /// Number of functions at each level
    pub level_counts: std::collections::HashMap<String, usize>,
}

/// Code metrics analysis tool
pub struct CodeMetricsTool;

impl CodeMetricsTool {
    /// Create a new code metrics tool
    pub fn new() -> Self {
        Self
    }

    /// Calculate cyclomatic complexity for a function body
    ///
    /// Uses simple heuristics:
    /// - Base complexity: 1
    /// - +1 for each: if, else, match, loop, for, while, &&, ||, ?
    fn calculate_complexity(function_body: &str) -> u32 {
        let mut complexity: u32 = 1;
        let body_lower = function_body.to_lowercase();

        // Count decision points
        complexity += body_lower.matches("if ").count() as u32;
        complexity += body_lower.matches("else").count() as u32;
        complexity += body_lower.matches("match ").count() as u32;
        complexity += body_lower.matches("loop").count() as u32;
        complexity += body_lower.matches("for ").count() as u32;
        complexity += body_lower.matches("while ").count() as u32;
        complexity += body_lower.matches(" && ").count() as u32;
        complexity += body_lower.matches(" || ").count() as u32;
        complexity += body_lower.matches("?").count() as u32;
        complexity += body_lower.matches("return ").count() as u32;

        complexity
    }

    /// Analyze a single function for complexity
    fn analyze_function(name: &str, body: &str, line_number: usize) -> FunctionMetrics {
        let complexity = Self::calculate_complexity(body);
        let level = ComplexityLevel::from_score(complexity);

        FunctionMetrics {
            name: name.to_string(),
            line_number,
            complexity,
            level: level.as_str().to_string(),
            suggestion: level.suggestion().to_string(),
        }
    }
}

impl Default for CodeMetricsTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for CodeMetricsTool {
    fn name(&self) -> &str {
        "code_metrics"
    }

    fn description(&self) -> &str {
        "Analyze a Rust source file and report cyclomatic complexity for each function. \
         Returns a JSON report with complexity scores, classifications, and refactoring \
         suggestions. Use this to identify complex functions that may need simplification."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["file_path"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the Rust source file to analyze"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required 'file_path' argument"))?;

        debug!("Analyzing code metrics for: {}", file_path);

        // Read the file
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {}", file_path))?;

        // Use the introspection parser to get function information
        let parsed = crate::tools::introspect::parser::parse_rust(&content);

        // Extract and analyze functions
        let mut functions: Vec<FunctionMetrics> = Vec::new();

        for symbol in &parsed.symbols {
            if let crate::tools::introspect::parser::SymbolKind::Function = symbol.kind {
                // Get function body from signature
                let body_start = symbol.line_start;
                let lines: Vec<&str> = content.lines().collect();
                let body_end = (body_start + 30).min(lines.len());
                let body = if body_start < lines.len() {
                    lines[body_start.saturating_sub(1)..body_end].join("\n")
                } else {
                    symbol.signature.clone()
                };

                let metrics = Self::analyze_function(&symbol.name, &body, symbol.line_start);
                functions.push(metrics);
            }
        }

        // Calculate summary statistics
        let total_functions = functions.len();
        let total_complexity: u32 = functions.iter().map(|f| f.complexity).sum();
        let avg_complexity = if total_functions > 0 {
            total_complexity as f64 / total_functions as f64
        } else {
            0.0
        };
        let max_complexity = functions.iter().map(|f| f.complexity).max().unwrap_or(0);

        // Count functions at each level
        let mut level_counts = std::collections::HashMap::new();
        for func in &functions {
            *level_counts.entry(func.level.clone()).or_insert(0) += 1;
        }

        let report = CodeMetricsReport {
            file: file_path.to_string(),
            functions,
            summary: MetricsSummary {
                total_functions,
                avg_complexity,
                max_complexity,
                level_counts,
            },
        };

        Ok(serde_json::to_value(report)?)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tools/code_metrics/code_metrics_test.rs"]
mod tests;
