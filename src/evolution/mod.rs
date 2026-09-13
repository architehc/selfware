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

pub mod ast_tools;
pub mod daemon;
pub mod fitness;
pub mod micro_mode;
pub mod sandbox;
pub mod telemetry;
pub mod tournament;

use std::path::PathBuf;

/// Files and directories that must NEVER be modified by any self-improvement or evolution loop.
///
/// This is the cardinal safety invariant — judges, verifiers, test suites, and
/// safety mechanisms must be externally defined and immutable from the agent's
/// perspective. A mutation cannot edit the auditor to return Ok or the orchestrator
/// to skip verification.
pub const PROTECTED_PATHS: &[&str] = &[
    "src/evolution/",
    "src/safety/",
    "src/evolve/",
    "src/agent/verification.rs",
    "src/agent/verification_scope.rs",
    "src/agent/checkpointing.rs",
    "src/agent/tool_dispatch/",
    "src/cognitive/rsi_orchestrator.rs",
    "src/cognitive/self_edit.rs",
    "src/cognitive/compilation_manager.rs",
    "src/testing/",
    "system_tests/",
    "tests/",
    "benches/",
    "Cargo.toml",
    "Cargo.lock",
    ".github/",
    "src/main.rs",
];

/// LLM endpoint configuration for hypothesis generation
#[derive(Clone)]
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

impl std::fmt::Debug for LlmConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print credentials — dry-run and log output include this struct.
        f.debug_struct("LlmConfig")
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .field(
                "api_key",
                &self.api_key.as_ref().map(|k| {
                    let mut m: String = k.chars().take(4).collect();
                    m.push_str("…[redacted]");
                    m
                }),
            )
            .field("max_tokens", &self.max_tokens)
            .field("temperature", &self.temperature)
            .finish()
    }
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

impl FitnessMetrics {
    /// Whether every term the weights score was actually measured.
    ///
    /// Two candidates are only comparable when this holds for both: a score
    /// computed over four terms and one computed over five are different
    /// quantities, whatever the renormalisation.
    pub fn is_complete(&self) -> bool {
        self.tokens_used.is_some()
    }
}

impl FitnessWeights {
    /// Compute composite fitness score from raw metrics.
    ///
    /// A term that was not measured is EXCLUDED and the remaining weights are
    /// renormalised, rather than being scored as if it were zero. Scoring an
    /// unobserved token count as 0 awarded full marks for efficiency to a run
    /// that never reported any.
    ///
    /// Exclusion keeps the number meaningful on its own, but it does NOT make
    /// an incomplete measurement comparable to a complete one — see
    /// [`FitnessMetrics::is_complete`], which promotion should gate on.
    pub fn composite(&self, metrics: &FitnessMetrics) -> f64 {
        let normalized_latency = 1.0 - (metrics.wall_clock_secs / metrics.timeout_secs).min(1.0);
        let normalized_coverage = metrics.test_coverage_pct / 100.0;
        let normalized_size = 1.0 - (metrics.binary_size_mb / metrics.max_binary_size_mb).min(1.0);
        let normalized_visual = metrics.visual_score / 100.0;

        let mut score = self.sab_score * (metrics.sab_score / 100.0)
            + self.latency * normalized_latency
            + self.test_coverage * normalized_coverage
            + self.binary_size * normalized_size
            + self.visual_quality * normalized_visual;
        let mut weight_total = self.sab_score
            + self.latency
            + self.test_coverage
            + self.binary_size
            + self.visual_quality;

        if let Some(tokens) = metrics.tokens_used {
            let normalized_tokens =
                1.0 - (tokens as f64 / metrics.token_budget.max(1) as f64).min(1.0);
            score += self.token_efficiency * normalized_tokens;
            weight_total += self.token_efficiency;
        }

        if weight_total > 0.0 {
            score / weight_total
        } else {
            0.0
        }
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
    /// `None` when token usage was not observed.
    ///
    /// This was `u64`, and an unmeasured run recorded 0 — which `composite`
    /// scores as PERFECT token efficiency (`1.0 - 0/budget`). A candidate
    /// nobody measured therefore outranked one that was measured honestly.
    pub tokens_used: Option<u64>,
    pub token_budget: u64,
    /// Duration of the phase both arms measure, so the two are comparable.
    pub wall_clock_secs: f64,
    pub timeout_secs: f64,
    /// Total evaluation cost including build and lint phases, where it was
    /// measured. Recorded separately from `wall_clock_secs` rather than folded
    /// into it: the baseline used to include four phases the candidate never
    /// timed, which made every candidate look faster.
    pub full_evaluation_secs: Option<f64>,
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
///
/// Uses proper path prefix matching with canonicalization to prevent bypasses
/// via symlinks, relative paths, or substring tricks.
pub fn is_protected(path: &std::path::Path) -> bool {
    // Canonicalize the path to resolve symlinks and normalize separators.
    // We use the safety-checker normalize_path so the Windows `\\?\` UNC
    // prefix is stripped — without that, `path_str.contains("src/evolution/")`
    // would never match an extended-length canonicalized path.
    let canonical_path = crate::safety::checker::normalize_path(path);

    // PROTECTED_PATHS uses forward slashes; convert any `\` to `/` so the
    // contains/starts_with checks work on Windows too.
    let path_str = canonical_path.to_string_lossy().replace('\\', "/");

    PROTECTED_PATHS.iter().any(|protected| {
        let protected_norm = protected.trim_end_matches('/');
        let is_dir = protected.ends_with('/');

        if is_dir {
            path_str == protected_norm
                || path_str.starts_with(protected)
                || path_str.contains(&format!("/{protected}"))
                || path_str.ends_with(&format!("/{protected_norm}"))
        } else {
            path_str == *protected
                || path_str.starts_with(&format!("{protected}/"))
                || path_str.ends_with(&format!("/{protected}"))
                || path_str.contains(&format!("/{protected}/"))
        }
    })
}

#[cfg(test)]
#[path = "../../tests/unit/evolution/mod_test.rs"]
mod tests;
