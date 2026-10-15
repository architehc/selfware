//! Hypothesis and evaluation result types for evolution cycles.

use super::fitness::SabResult;
use super::{FitnessMetrics, GenerationRating};
use std::path::PathBuf;

/// A mutation hypothesis proposed by the agent
#[derive(Debug, Clone)]
pub struct Hypothesis {
    /// Unique identifier
    pub id: String,
    /// Human-readable description of what the mutation does
    pub description: String,
    /// Unified diff (patch format)
    pub patch: String,
    /// Files affected by this mutation
    pub target_files: Vec<PathBuf>,
    /// Optional: property test that should pass after mutation
    pub property_test: Option<String>,
}

/// Result of evaluating a single hypothesis
#[derive(Debug)]
pub struct HypothesisResult {
    pub id: String,
    pub description: String,
    pub compiled: bool,
    pub sab_result: Option<SabResult>,
    pub fitness: Option<FitnessMetrics>,
    pub composite_score: f64,
    pub rating: GenerationRating,
    pub patch: String,
}

#[cfg(test)]
#[path = "../../tests/unit/evolution/tournament/tournament_test.rs"]
mod tests;
