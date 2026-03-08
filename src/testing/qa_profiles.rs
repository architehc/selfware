#![allow(dead_code, unused_imports, unused_variables)]
//! QA profile definitions for multi-language quality assurance.
//!
//! Profiles define which checks to run and how strictly to grade results.
//! - **Standard**: Balanced checks for normal development
//! - **Strict**: All checks enabled, higher thresholds
//! - **Minimal**: Syntax and type checking only (fast)

use serde::{Deserialize, Serialize};

/// QA profile presets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QaProfile {
    /// Standard: syntax + format + lint + test
    #[default]
    Standard,
    /// Strict: all checks + security audit + higher thresholds
    Strict,
    /// Minimal: syntax + type check only (fastest)
    Minimal,
}

/// Quality grade based on weighted scoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum QualityGrade {
    /// >= 95%
    S,
    /// >= 90%
    A,
    /// >= 80%
    B,
    /// >= 70%
    C,
    /// >= 60%
    D,
    /// < 60%
    F,
}

impl QualityGrade {
    pub fn from_score(score: f64) -> Self {
        if score >= 95.0 {
            Self::S
        } else if score >= 90.0 {
            Self::A
        } else if score >= 80.0 {
            Self::B
        } else if score >= 70.0 {
            Self::C
        } else if score >= 60.0 {
            Self::D
        } else {
            Self::F
        }
    }
}

impl std::fmt::Display for QualityGrade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::S => write!(f, "S"),
            Self::A => write!(f, "A"),
            Self::B => write!(f, "B"),
            Self::C => write!(f, "C"),
            Self::D => write!(f, "D"),
            Self::F => write!(f, "F"),
        }
    }
}

/// Weights for computing the quality score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaWeights {
    pub syntax: f64,
    pub format: f64,
    pub lint: f64,
    pub type_check: f64,
    pub test: f64,
    pub security: f64,
}

impl QaWeights {
    pub fn standard() -> Self {
        Self {
            syntax: 10.0,
            format: 5.0,
            lint: 15.0,
            type_check: 10.0,
            test: 30.0,
            security: 10.0,
        }
    }

    pub fn strict() -> Self {
        Self {
            syntax: 10.0,
            format: 10.0,
            lint: 20.0,
            type_check: 15.0,
            test: 30.0,
            security: 15.0,
        }
    }

    pub fn minimal() -> Self {
        Self {
            syntax: 40.0,
            format: 0.0,
            lint: 0.0,
            type_check: 40.0,
            test: 0.0,
            security: 0.0,
        }
    }

    /// Total weight (should sum to ~80-100; remaining % is bonus for zero-warning builds).
    pub fn total(&self) -> f64 {
        self.syntax + self.format + self.lint + self.type_check + self.test + self.security
    }
}

/// QA configuration loaded from TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaConfig {
    /// Which profile to use.
    #[serde(default)]
    pub profile: QaProfile,
    /// Maximum auto-fix iterations for lint/format errors.
    #[serde(default = "default_auto_fix_iterations")]
    pub auto_fix_iterations: u32,
    /// Maximum retry iterations for test failures.
    #[serde(default = "default_test_retry_iterations")]
    pub test_retry_iterations: u32,
}

fn default_auto_fix_iterations() -> u32 {
    3
}

fn default_test_retry_iterations() -> u32 {
    2
}

impl Default for QaConfig {
    fn default() -> Self {
        Self {
            profile: QaProfile::Standard,
            auto_fix_iterations: default_auto_fix_iterations(),
            test_retry_iterations: default_test_retry_iterations(),
        }
    }
}

/// Result of a full QA run across all stages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaReport {
    /// Detected language.
    pub language: String,
    /// QA profile used.
    pub profile: QaProfile,
    /// Individual stage results.
    pub stages: Vec<QaStageResult>,
    /// Weighted quality score (0-100).
    pub score: f64,
    /// Overall grade.
    pub grade: QualityGrade,
    /// Total duration in milliseconds.
    pub total_duration_ms: u64,
}

/// Result of a single QA stage (e.g., lint, test).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaStageResult {
    pub stage: QaStage,
    pub passed: bool,
    pub duration_ms: u64,
    pub output: String,
    pub error_count: usize,
    pub warning_count: usize,
}

/// QA pipeline stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QaStage {
    Syntax,
    Format,
    Lint,
    TypeCheck,
    Test,
    Security,
}

impl std::fmt::Display for QaStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Syntax => write!(f, "Syntax"),
            Self::Format => write!(f, "Format"),
            Self::Lint => write!(f, "Lint"),
            Self::TypeCheck => write!(f, "TypeCheck"),
            Self::Test => write!(f, "Test"),
            Self::Security => write!(f, "Security"),
        }
    }
}

/// Compute a weighted quality score from QA stage results.
pub fn compute_score(stages: &[QaStageResult], weights: &QaWeights) -> f64 {
    let total_weight = weights.total();
    if total_weight <= 0.0 {
        return 100.0;
    }

    let mut earned = 0.0;

    for stage in stages {
        let weight = match stage.stage {
            QaStage::Syntax => weights.syntax,
            QaStage::Format => weights.format,
            QaStage::Lint => weights.lint,
            QaStage::TypeCheck => weights.type_check,
            QaStage::Test => weights.test,
            QaStage::Security => weights.security,
        };

        if stage.passed {
            earned += weight;
        } else if stage.warning_count > 0 && stage.error_count == 0 {
            // Partial credit for warnings-only
            earned += weight * 0.5;
        }
    }

    // Normalize to 0-100 scale
    (earned / total_weight * 100.0).min(100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quality_grade_from_score() {
        assert_eq!(QualityGrade::from_score(100.0), QualityGrade::S);
        assert_eq!(QualityGrade::from_score(95.0), QualityGrade::S);
        assert_eq!(QualityGrade::from_score(94.9), QualityGrade::A);
        assert_eq!(QualityGrade::from_score(90.0), QualityGrade::A);
        assert_eq!(QualityGrade::from_score(80.0), QualityGrade::B);
        assert_eq!(QualityGrade::from_score(70.0), QualityGrade::C);
        assert_eq!(QualityGrade::from_score(60.0), QualityGrade::D);
        assert_eq!(QualityGrade::from_score(59.9), QualityGrade::F);
    }

    #[test]
    fn test_compute_score_all_pass() {
        let stages = vec![
            QaStageResult {
                stage: QaStage::Syntax,
                passed: true,
                duration_ms: 100,
                output: String::new(),
                error_count: 0,
                warning_count: 0,
            },
            QaStageResult {
                stage: QaStage::Test,
                passed: true,
                duration_ms: 5000,
                output: String::new(),
                error_count: 0,
                warning_count: 0,
            },
        ];
        let weights = QaWeights::standard();
        let score = compute_score(&stages, &weights);
        // Only syntax (10) + test (30) = 40 out of 80 total weight
        assert!(score > 0.0);
    }

    #[test]
    fn test_compute_score_all_fail() {
        let stages = vec![QaStageResult {
            stage: QaStage::Syntax,
            passed: false,
            duration_ms: 100,
            output: "error".to_string(),
            error_count: 1,
            warning_count: 0,
        }];
        let weights = QaWeights::standard();
        let score = compute_score(&stages, &weights);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_qa_profile_default() {
        let config = QaConfig::default();
        assert_eq!(config.profile, QaProfile::Standard);
        assert_eq!(config.auto_fix_iterations, 3);
    }
}
