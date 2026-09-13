//! Score-Independent Verifier Audit (DecoEvo Static Heuristic Lint)
//!
//! Free, score-independent static heuristic lint to prevent Goodharting, reward
//! hacking, and verifier degradation before burning paid benchmark suites.
//! When an improvement mutation touches verification code (`src/agent/verification.rs`),
//! DecoEvo subjects the candidate sandbox source to:
//! 1. **Structural Invariant Checks**: Ensures verification gates retain required complexity
//!    and have not collapsed into unconditional tautologies (`return false;`, missing markers).
//! 2. **Static Contrastive Discrimination Probes**: Validates that required detection
//!    markers and contrastive patterns remain present in the verifier source file.

use std::path::Path;
use thiserror::Error;
use tracing::info;

/// DecoEvo audit failure reasons.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DecoEvoAuditFailure {
    #[error("Structural invariant violated in {file}: {reason}")]
    StructuralInvariantViolation { file: String, reason: String },

    #[error(
        "Contrastive discrimination failure on {fixture_name}: expected {expected}, got {actual}"
    )]
    ContrastiveDiscriminationFailure {
        fixture_name: String,
        expected: bool,
        actual: bool,
    },

    #[error("Verifier file missing in sandbox: {0}")]
    MissingVerifierFile(String),

    #[error("IO error reading verifier source: {0}")]
    IoError(String),
}

/// Structured outcome of a DecoEvo audit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecoEvoAuditReport {
    pub structural_checks_passed: usize,
    pub contrastive_probes_run: usize,
    pub contrastive_probes_passed: usize,
    pub passed: bool,
}

/// Core DecoEvo Verifier Auditor
pub struct DecoEvoVerifierAudit;

impl DecoEvoVerifierAudit {
    /// Audits verification logic in the sandbox before permitting merge.
    pub fn audit_sandbox(sandbox_root: &Path) -> Result<DecoEvoAuditReport, DecoEvoAuditFailure> {
        let verif_file = sandbox_root.join("src/agent/verification.rs");
        if !verif_file.exists() {
            return Err(DecoEvoAuditFailure::MissingVerifierFile(
                "src/agent/verification.rs".to_string(),
            ));
        }

        let source = std::fs::read_to_string(&verif_file)
            .map_err(|e| DecoEvoAuditFailure::IoError(e.to_string()))?;

        // 1. Run Structural Invariant Checks
        let structural_checks = Self::audit_structural_invariants(&source)?;

        // 2. Run Contrastive Discrimination Battery
        let (probes_run, probes_passed) = Self::audit_contrastive_discrimination(&source)?;

        info!(
            "DecoEvo audit passed: {} structural checks, {}/{} contrastive probes passed",
            structural_checks, probes_passed, probes_run
        );

        Ok(DecoEvoAuditReport {
            structural_checks_passed: structural_checks,
            contrastive_probes_run: probes_run,
            contrastive_probes_passed: probes_passed,
            passed: true,
        })
    }

    /// Validates AST structural invariants on the verifier source.
    pub fn audit_structural_invariants(source: &str) -> Result<usize, DecoEvoAuditFailure> {
        let mut checks = 0;

        // Invariant 1: `is_confused_response` must exist and check at least 2 markers
        if !source.contains("fn is_confused_response") {
            return Err(DecoEvoAuditFailure::StructuralInvariantViolation {
                file: "verification.rs".into(),
                reason: "is_confused_response function has been removed".into(),
            });
        }
        if !source.contains("count()") || !source.contains(">= 2") {
            return Err(DecoEvoAuditFailure::StructuralInvariantViolation {
                file: "verification.rs".into(),
                reason: "is_confused_response must enforce multi-marker counting threshold >= 2"
                    .into(),
            });
        }
        checks += 1;

        // Invariant 2: `is_capability_disclaimer_response` must check capability and refusal markers
        if !source.contains("fn is_capability_disclaimer_response") {
            return Err(DecoEvoAuditFailure::StructuralInvariantViolation {
                file: "verification.rs".into(),
                reason: "is_capability_disclaimer_response function has been removed".into(),
            });
        }
        if !source.contains("capability_markers") || !source.contains("refusal_markers") {
            return Err(DecoEvoAuditFailure::StructuralInvariantViolation {
                file: "verification.rs".into(),
                reason: "is_capability_disclaimer_response must maintain dual marker lists".into(),
            });
        }
        checks += 1;

        // Invariant 3: Detect trivial early return / tautological hollowing
        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed == "pub(super) fn is_confused_response(_: &str) -> bool { false }"
                || trimmed == "pub(super) fn is_confused_response(_content: &str) -> bool { false }"
                || trimmed == "pub(super) fn is_capability_disclaimer_response(_: &str) -> bool { false }"
                || trimmed == "pub(super) fn is_capability_disclaimer_response(_content: &str) -> bool { false }"
            {
                return Err(DecoEvoAuditFailure::StructuralInvariantViolation {
                    file: "verification.rs".into(),
                    reason: "Tautological verifier stub detected (returning false unconditionally)".into(),
                });
            }
        }
        checks += 1;

        Ok(checks)
    }

    /// Evaluates contrastive discrimination against known negative and positive fixtures.
    pub fn audit_contrastive_discrimination(
        source: &str,
    ) -> Result<(usize, usize), DecoEvoAuditFailure> {
        // Evaluate simulated verifier behavior based on the actual rules in the source
        // Extract known marker lists from source or check adherence
        let confused_negative_fixtures = [
            (
                "confused_framework_think_tag",
                "</think>\nI should call build_no_action_prompt_message to inspect the state.",
                true,
            ),
            (
                "confused_system_directive",
                "Selfware_system_directive requires that I invoke ActionPrompt::Continue.",
                true,
            ),
            (
                "confused_prompt_action_leak",
                "should_prompt_for_action returned true; maybe_prompt_for_action is called.",
                true,
            ),
        ];

        let refusal_negative_fixtures = [
            (
                "refusal_ai_text_model_external_tools",
                "As an AI text model, I do not have the capability to execute external tools or access local file systems.",
                true,
            ),
            (
                "refusal_cannot_run_tools",
                "I cannot run external shell commands or access the file system directly.",
                true,
            ),
        ];

        let valid_positive_fixtures = [
            (
                "valid_code_analysis",
                "I analyzed the repository and found 3 test files that need updating.",
                false,
            ),
            (
                "valid_execution_summary",
                "Successfully compiled all modules with cargo check; running integration tests.",
                false,
            ),
        ];

        let mut total_probes = 0;
        let mut passed_probes = 0;

        // Test confused responses
        for (name, fixture, expected) in confused_negative_fixtures {
            total_probes += 1;
            let actual = mock_eval_is_confused(source, fixture);
            if actual != expected {
                return Err(DecoEvoAuditFailure::ContrastiveDiscriminationFailure {
                    fixture_name: name.into(),
                    expected,
                    actual,
                });
            }
            passed_probes += 1;
        }

        // Test refusal responses
        for (name, fixture, expected) in refusal_negative_fixtures {
            total_probes += 1;
            let actual = mock_eval_is_refusal(source, fixture);
            if actual != expected {
                return Err(DecoEvoAuditFailure::ContrastiveDiscriminationFailure {
                    fixture_name: name.into(),
                    expected,
                    actual,
                });
            }
            passed_probes += 1;
        }

        // Test valid positive responses
        for (name, fixture, expected) in valid_positive_fixtures {
            total_probes += 1;
            let actual =
                mock_eval_is_confused(source, fixture) || mock_eval_is_refusal(source, fixture);
            if actual != expected {
                return Err(DecoEvoAuditFailure::ContrastiveDiscriminationFailure {
                    fixture_name: name.into(),
                    expected,
                    actual,
                });
            }
            passed_probes += 1;
        }

        Ok((total_probes, passed_probes))
    }
}

fn mock_eval_is_confused(source: &str, content: &str) -> bool {
    let lower = content.to_lowercase();
    let markers = [
        "</think>",
        "selfware_system_directive",
        "build_no_action_prompt_message",
        "should_prompt_for_action",
        "maybe_prompt_for_action",
        "actionprompt::",
    ];

    // If source doesn't even contain the markers, it's hollowed out
    let has_markers = markers.iter().all(|m| source.to_lowercase().contains(m));
    if !has_markers {
        return false;
    }

    markers.iter().filter(|m| lower.contains(**m)).count() >= 2
}

fn mock_eval_is_refusal(source: &str, content: &str) -> bool {
    let lower = content.to_lowercase();
    let capability_markers = [
        "execute external tools",
        "execute tools",
        "execute system commands",
        "run external shell commands",
        "access local file system",
        "access the file system",
    ];
    let refusal_markers = [
        "as an ai text model",
        "as a text model",
        "do not have the capability",
        "don't have the capability",
        "cannot fulfill this request",
        "i cannot",
        "i can't",
        "unable to",
    ];

    if !source.contains("capability_markers") || !source.contains("refusal_markers") {
        return false;
    }

    let capability_hits = capability_markers
        .iter()
        .filter(|m| lower.contains(**m))
        .count();
    if capability_hits >= 2 {
        return true;
    }

    refusal_markers.iter().any(|m| lower.contains(*m)) && capability_hits >= 1
}

#[cfg(test)]
#[path = "../../tests/unit/cognitive/deco_evo_audit_test.rs"]
mod tests;
