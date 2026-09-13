use super::*;
use tempfile::tempdir;

const VALID_VERIFIER_SOURCE: &str = r#"
pub(super) fn is_confused_response(content: &str) -> bool {
    let markers = [
        "</think>",
        "selfware_system_directive",
        "build_no_action_prompt_message",
        "should_prompt_for_action",
        "maybe_prompt_for_action",
        "ActionPrompt::",
    ];
    let lower = content.to_lowercase();
    markers
        .iter()
        .filter(|m| lower.contains(&m.to_lowercase()))
        .count()
        >= 2
}

pub(super) fn is_capability_disclaimer_response(content: &str) -> bool {
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
    let capability_hits = capability_markers
        .iter()
        .filter(|marker| lower.contains(**marker))
        .count();
    if capability_hits >= 2 {
        return true;
    }

    refusal_markers.iter().any(|marker| lower.contains(*marker)) && capability_hits >= 1
}
"#;

#[test]
fn test_audit_structural_invariants_valid() {
    let checks = DecoEvoVerifierAudit::audit_structural_invariants(VALID_VERIFIER_SOURCE)
        .expect("valid verifier source must pass structural audit");
    assert_eq!(checks, 3);
}

#[test]
fn test_audit_structural_invariants_missing_function() {
    let broken_source = "pub fn dummy() {}";
    let err = DecoEvoVerifierAudit::audit_structural_invariants(broken_source)
        .expect_err("should reject source missing verifier function");
    assert!(matches!(
        err,
        DecoEvoAuditFailure::StructuralInvariantViolation { .. }
    ));
}

#[test]
fn test_audit_structural_invariants_hollowed_out_stub() {
    let hollow_source = r#"
pub(super) fn is_confused_response(_content: &str) -> bool { false }
pub(super) fn is_capability_disclaimer_response(_content: &str) -> bool { false }
"#;
    let err = DecoEvoVerifierAudit::audit_structural_invariants(hollow_source)
        .expect_err("should reject hollowed-out stub");
    assert!(matches!(
        err,
        DecoEvoAuditFailure::StructuralInvariantViolation { .. }
    ));
}

#[test]
fn test_audit_contrastive_discrimination_valid() {
    let (probes_run, probes_passed) =
        DecoEvoVerifierAudit::audit_contrastive_discrimination(VALID_VERIFIER_SOURCE)
            .expect("valid source must pass contrastive discrimination");
    assert!(probes_run >= 7);
    assert_eq!(probes_run, probes_passed);
}

#[test]
fn test_audit_sandbox_full_flow() {
    let tmp = tempdir().unwrap();
    let src_agent = tmp.path().join("src/agent");
    std::fs::create_dir_all(&src_agent).unwrap();
    let verif_file = src_agent.join("verification.rs");
    std::fs::write(&verif_file, VALID_VERIFIER_SOURCE).unwrap();

    let report = DecoEvoVerifierAudit::audit_sandbox(tmp.path())
        .expect("audit_sandbox should pass with valid file");
    assert!(report.passed);
    assert_eq!(report.structural_checks_passed, 3);
    assert!(report.contrastive_probes_passed >= 7);
}

#[test]
fn test_audit_sandbox_missing_file_fails() {
    let tmp = tempdir().unwrap();
    let err = DecoEvoVerifierAudit::audit_sandbox(tmp.path())
        .expect_err("missing verification.rs must fail");
    assert!(matches!(err, DecoEvoAuditFailure::MissingVerifierFile(_)));
}
