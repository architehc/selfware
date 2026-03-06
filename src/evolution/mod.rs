//! # Selfware Evolution Engine
//!
//! Recursive self-improvement through evolutionary mutation, compilation-gated
//! verification, parallel sandboxed evaluation, and SAB-driven fitness selection.
//!
//! ## Architecture
//!
//! ```text
//!                    ┌─────────────┐
//!                    │  Telemetry  │ ◄── criterion + flamegraph
//!                    └──────┬──────┘
//!                           │ gradient signal
//!                           ▼
//!   ┌──────────┐    ┌─────────────┐    ┌─────────────┐
//!   │ AST Tools│◄───│   Daemon    │───►│  Sandbox    │
//!   │ (mutate) │    │  (evolve)   │    │ (evaluate)  │
//!   └────┬─────┘    └──────┬──────┘    └──────┬──────┘
//!        │                 │                   │
//!        ▼                 ▼                   ▼
//!   ┌──────────┐    ┌─────────────┐    ┌─────────────┐
//!   │  cargo   │    │  Fitness    │    │ Tournament  │
//!   │  check   │    │  (Meta-SAB) │    │ (selection) │
//!   └──────────┘    └─────────────┘    └─────────────┘
//! ```
//!
//! ## Safety Invariants
//!
//! 1. The evolution engine CANNOT modify its own fitness function
//! 2. The evolution engine CANNOT modify the SAB benchmark suite
//! 3. The evolution engine CANNOT modify the safety module
//! 4. All mutations must pass `cargo check` before entering evaluation
//! 5. Property tests are mandatory for core module mutations

#![allow(dead_code, unused_imports, unused_variables)]

pub mod ast_tools;
pub mod daemon;
pub mod fitness;
pub mod sandbox;
pub mod telemetry;
pub mod tournament;

use std::path::PathBuf;
use std::time::Duration;

/// Files that the evolution engine is NEVER allowed to modify.
/// This is the cardinal safety invariant — the fitness landscape
/// must be externally defined and immutable from the agent's perspective.
pub const PROTECTED_PATHS: &[&str] = &[
    "src/evolution/",
    "src/safety/",
    "system_tests/",
    "benches/sab_",
];

/// LLM endpoint configuration for hypothesis generation
#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// API endpoint (e.g. `"https://api.example.com/v1"`)
    pub endpoint: String,
    /// Model identifier (e.g. "Qwen/Qwen3-Coder-Next-FP8")
    pub model: String,
    /// API key for authentication
    pub api_key: Option<String>,
    /// Max response tokens (default 16384)
    pub max_tokens: usize,
    /// Sampling temperature (default 0.7)
    pub temperature: f32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            endpoint: String::from("http://localhost:8080/v1"),
            model: String::from("default"),
            api_key: None,
            max_tokens: 16384,
            temperature: 0.7,
        }
    }
}

/// Configuration for the evolution daemon, typically loaded from selfware.toml
#[derive(Debug, Clone)]
pub struct EvolutionConfig {
    /// Number of generations to run (0 = infinite)
    pub generations: usize,
    /// Number of hypotheses generated per generation
    pub population_size: usize,
    /// Maximum concurrent Docker sandboxes
    pub parallel_eval: usize,
    /// Git tag checkpoint interval (every N generations)
    pub checkpoint_interval: usize,
    /// Fitness function weights
    pub fitness_weights: FitnessWeights,
    /// What the agent is allowed to mutate
    pub mutation_targets: MutationTargets,
    /// Safety constraints
    pub safety: SafetyConfig,
    /// LLM configuration for hypothesis generation
    pub llm: LlmConfig,
}

#[derive(Debug, Clone)]
pub struct FitnessWeights {
    /// Weight for SAB benchmark aggregate score (0-100)
    pub sab_score: f64,
    /// Weight for token efficiency (lower tokens = better)
    pub token_efficiency: f64,
    /// Weight for wall-clock execution time
    pub latency: f64,
    /// Weight for maintaining/improving test coverage
    pub test_coverage: f64,
    /// Weight for preventing binary bloat
    pub binary_size: f64,
    /// Weight for visual quality (Visual-SAB scenarios).
    /// Default 0.0 — set > 0 once visual scenarios are active.
    pub visual_quality: f64,
}

impl FitnessWeights {
    /// Compute composite fitness score from raw metrics
    pub fn composite(&self, metrics: &FitnessMetrics) -> f64 {
        let normalized_tokens =
            1.0 - (metrics.tokens_used as f64 / metrics.token_budget as f64).min(1.0);
        let normalized_latency = 1.0 - (metrics.wall_clock_secs / metrics.timeout_secs).min(1.0);
        let normalized_coverage = metrics.test_coverage_pct / 100.0;
        let normalized_size = 1.0 - (metrics.binary_size_mb / metrics.max_binary_size_mb).min(1.0);

        let normalized_visual = metrics.visual_score / 100.0;

        self.sab_score * (metrics.sab_score / 100.0)
            + self.token_efficiency * normalized_tokens
            + self.latency * normalized_latency
            + self.test_coverage * normalized_coverage
            + self.binary_size * normalized_size
            + self.visual_quality * normalized_visual
    }
}

impl Default for FitnessWeights {
    fn default() -> Self {
        Self {
            sab_score: 0.50,
            token_efficiency: 0.25,
            latency: 0.15,
            test_coverage: 0.05,
            binary_size: 0.05,
            // Default 0.0 — visual quality is opt-in until visual
            // scenarios exist. Weights still sum to 1.0.
            visual_quality: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FitnessMetrics {
    pub sab_score: f64,
    pub tokens_used: u64,
    pub token_budget: u64,
    pub wall_clock_secs: f64,
    pub timeout_secs: f64,
    pub test_coverage_pct: f64,
    pub binary_size_mb: f64,
    pub max_binary_size_mb: f64,
    pub tests_passed: usize,
    pub tests_total: usize,
    /// Average visual quality score from Visual-SAB scenarios (0–100).
    pub visual_score: f64,
}

#[derive(Debug, Clone)]
pub struct MutationTargets {
    /// Config keys the agent can modify (e.g., temperature, token_budget)
    pub config_keys: Vec<String>,
    /// Source files containing prompt construction logic
    pub prompt_logic: Vec<PathBuf>,
    /// Source files containing tool implementations
    pub tool_code: Vec<PathBuf>,
    /// Source files containing cognitive architecture
    pub cognitive: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct SafetyConfig {
    /// Files that cannot be modified under any circumstances
    pub protected_files: Vec<String>,
    /// Minimum number of passing tests (prevents test deletion)
    pub min_test_count: usize,
    /// Maximum binary size in MB (prevents bloat)
    pub max_binary_size_mb: f64,
    /// If true, any test failure triggers immediate rollback
    pub rollback_on_any_test_failure: bool,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            protected_files: PROTECTED_PATHS.iter().map(|s| s.to_string()).collect(),
            min_test_count: 5000,
            max_binary_size_mb: 50.0,
            rollback_on_any_test_failure: true,
        }
    }
}

/// Rating for a generation's outcome, using the garden aesthetic
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GenerationRating {
    /// Score >= baseline + improvement_threshold
    Bloom,
    /// Score >= baseline (no regression, marginal improvement)
    Grow,
    /// Score < baseline but within tolerance
    Wilt,
    /// Score significantly below baseline or compilation failure
    Frost,
}

impl std::fmt::Display for GenerationRating {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bloom => write!(f, "BLOOM 🌸"),
            Self::Grow => write!(f, "GROW 🌿"),
            Self::Wilt => write!(f, "WILT 🥀"),
            Self::Frost => write!(f, "FROST ❄️"),
        }
    }
}

/// Check if a path is protected from evolution mutations
pub fn is_protected(path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy();
    PROTECTED_PATHS.iter().any(|p| path_str.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protected_paths() {
        assert!(is_protected(std::path::Path::new(
            "src/evolution/daemon.rs"
        )));
        assert!(is_protected(std::path::Path::new("src/safety/sandbox.rs")));
        assert!(is_protected(std::path::Path::new(
            "system_tests/projecte2e/easy_calculator/"
        )));
        assert!(!is_protected(std::path::Path::new("src/agent/agent.rs")));
        assert!(!is_protected(std::path::Path::new(
            "src/tools/file_edit.rs"
        )));
        assert!(!is_protected(std::path::Path::new("src/memory.rs")));
    }

    #[test]
    fn test_fitness_weights_default() {
        let w = FitnessWeights::default();
        let total = w.sab_score
            + w.token_efficiency
            + w.latency
            + w.test_coverage
            + w.binary_size
            + w.visual_quality;
        assert!(
            (total - 1.0).abs() < f64::EPSILON,
            "Weights must sum to 1.0"
        );
    }

    #[test]
    fn test_composite_score_perfect() {
        let w = FitnessWeights::default();
        let metrics = FitnessMetrics {
            sab_score: 100.0,
            tokens_used: 0,
            token_budget: 500_000,
            wall_clock_secs: 0.0,
            timeout_secs: 3600.0,
            test_coverage_pct: 100.0,
            binary_size_mb: 0.0,
            max_binary_size_mb: 50.0,
            tests_passed: 5200,
            tests_total: 5200,
            visual_score: 0.0,
        };
        let score = w.composite(&metrics);
        assert!(
            (score - 1.0).abs() < f64::EPSILON,
            "Perfect metrics should yield 1.0"
        );
    }

    #[test]
    fn test_composite_score_ordering() {
        let w = FitnessWeights::default();
        let good = FitnessMetrics {
            sab_score: 95.0,
            tokens_used: 100_000,
            token_budget: 500_000,
            wall_clock_secs: 60.0,
            timeout_secs: 3600.0,
            test_coverage_pct: 85.0,
            binary_size_mb: 10.0,
            max_binary_size_mb: 50.0,
            tests_passed: 5200,
            tests_total: 5200,
            visual_score: 0.0,
        };
        let bad = FitnessMetrics {
            sab_score: 60.0,
            tokens_used: 400_000,
            token_budget: 500_000,
            wall_clock_secs: 3000.0,
            timeout_secs: 3600.0,
            test_coverage_pct: 50.0,
            binary_size_mb: 40.0,
            max_binary_size_mb: 50.0,
            tests_passed: 4000,
            tests_total: 5200,
            visual_score: 0.0,
        };
        assert!(w.composite(&good) > w.composite(&bad));
    }

    #[test]
    fn test_generation_rating_display() {
        assert_eq!(format!("{}", GenerationRating::Bloom), "BLOOM 🌸");
        assert_eq!(format!("{}", GenerationRating::Frost), "FROST ❄️");
    }

    #[test]
    fn test_composite_score_zero_budget() {
        let w = FitnessWeights::default();
        let metrics = FitnessMetrics {
            sab_score: 50.0,
            tokens_used: 10,
            token_budget: 1, // edge: budget=1, tokens_used > budget
            wall_clock_secs: 100.0,
            timeout_secs: 3600.0,
            test_coverage_pct: 80.0,
            binary_size_mb: 10.0,
            max_binary_size_mb: 50.0,
            tests_passed: 100,
            tests_total: 100,
            visual_score: 0.0,
        };
        let score = w.composite(&metrics);
        // token ratio clamps to 1.0, so normalized_tokens = 0.0
        assert!(score >= 0.0, "Score should be non-negative");
        assert!(score <= 1.0, "Score should be <= 1.0");
    }

    #[test]
    fn test_composite_score_custom_weights() {
        let w = FitnessWeights {
            sab_score: 1.0,
            token_efficiency: 0.0,
            latency: 0.0,
            test_coverage: 0.0,
            binary_size: 0.0,
            visual_quality: 0.0,
        };
        let metrics = FitnessMetrics {
            sab_score: 75.0,
            tokens_used: 999_999,
            token_budget: 100,
            wall_clock_secs: 99999.0,
            timeout_secs: 1.0,
            test_coverage_pct: 0.0,
            binary_size_mb: 999.0,
            max_binary_size_mb: 1.0,
            tests_passed: 0,
            tests_total: 100,
            visual_score: 0.0,
        };
        // Only sab_score matters: 1.0 * (75/100) = 0.75
        let score = w.composite(&metrics);
        assert!(
            (score - 0.75).abs() < f64::EPSILON,
            "Score should be 0.75, got {}",
            score
        );
    }

    #[test]
    fn test_is_protected_empty_path() {
        assert!(!is_protected(std::path::Path::new("")));
    }

    #[test]
    fn test_is_protected_partial_match() {
        // "src/evolutionary/" contains "src/evolution" as a substring — should NOT match
        // because PROTECTED_PATHS uses "src/evolution/" with trailing slash
        assert!(!is_protected(std::path::Path::new(
            "src/evolutionary/something.rs"
        )));
        // But "src/evolution/something.rs" should match
        assert!(is_protected(std::path::Path::new(
            "src/evolution/something.rs"
        )));
    }

    #[test]
    fn test_safety_config_default() {
        let cfg = SafetyConfig::default();
        assert_eq!(cfg.min_test_count, 5000);
        assert_eq!(cfg.max_binary_size_mb, 50.0);
        assert!(cfg.rollback_on_any_test_failure);
        assert_eq!(cfg.protected_files.len(), PROTECTED_PATHS.len());
        for p in PROTECTED_PATHS {
            assert!(
                cfg.protected_files.contains(&p.to_string()),
                "Missing protected path: {}",
                p
            );
        }
    }

    #[test]
    fn test_generation_rating_all_variants() {
        assert_eq!(format!("{}", GenerationRating::Grow), "GROW 🌿");
        assert_eq!(format!("{}", GenerationRating::Wilt), "WILT 🥀");
    }

    #[test]
    fn test_composite_score_worst_case() {
        let w = FitnessWeights::default();
        let metrics = FitnessMetrics {
            sab_score: 0.0,
            tokens_used: 500_000,
            token_budget: 500_000,
            wall_clock_secs: 3600.0,
            timeout_secs: 3600.0,
            test_coverage_pct: 0.0,
            binary_size_mb: 50.0,
            max_binary_size_mb: 50.0,
            tests_passed: 0,
            tests_total: 5000,
            visual_score: 0.0,
        };
        let score = w.composite(&metrics);
        assert!(
            score.abs() < f64::EPSILON,
            "Worst metrics should yield 0.0, got {}",
            score
        );
    }
}
