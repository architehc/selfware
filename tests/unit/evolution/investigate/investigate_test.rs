use super::*;
use crate::evolution::tree_log::{compute_sha256, AttemptNode, AttemptStatus};
use std::path::Path;

fn make_test_node(id: &str, patch: &str, status: AttemptStatus) -> AttemptNode {
    AttemptNode {
        id: id.to_string(),
        parent_id: Some("att-baseline".to_string()),
        generation: 1,
        branch_id: "branch-1".to_string(),
        hypothesis_id: format!("hyp-{id}"),
        description: "Test hypothesis for investigate engine".to_string(),
        diff_sha256: compute_sha256(patch.as_bytes()),
        patch: Some(patch.to_string()),
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.85),
        tokens_used: Some(1500),
        wall_time_ms: 1200,
        status,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: Some("deadbeef0123456789".to_string()),
        committed_commit: None,
        created_at: "2026-09-18T20:00:00Z".to_string(),
    }
}

#[test]
fn test_scan_patch_detects_unchecked_unwrap_in_production() {
    let repo_root = Path::new("/workspace");
    let patch = r#"[{"file": "src/tools/worker.rs", "search": "let x = foo();", "replace": "let x = foo().unwrap();"}]"#;
    let (findings, citations) =
        scan_patch_for_opaque_structures(patch, repo_root, "att-test", None);

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].category, OpaqueCategory::UncheckedUnwrap);
    assert_eq!(findings[0].severity, FindingSeverity::Critical);
    assert!(findings[0].title.contains("Unchecked panic vector"));
    assert_eq!(citations.len(), 1);
    assert_eq!(citations[0].file_path, "src/tools/worker.rs");
    assert!(citations[0]
        .hyperlink
        .contains("/workspace/src/tools/worker.rs#L1-L10"));
}

#[test]
fn test_scan_patch_detects_implicit_constant() {
    let repo_root = Path::new("/workspace");
    let patch = r#"[{"file": "src/tools/runner.rs", "search": "let timeout = default;", "replace": "let timeout = 5000;"}]"#;
    let (findings, _) = scan_patch_for_opaque_structures(patch, repo_root, "att-test", None);

    assert!(findings
        .iter()
        .any(|f| f.category == OpaqueCategory::ImplicitConstant));
}

#[test]
fn test_scan_patch_detects_undocumented_public_api() {
    let repo_root = Path::new("/workspace");
    let patch = r#"[{"file": "src/tools/api.rs", "search": "// empty", "replace": "pub fn execute_action() -> bool { true }"}]"#;
    let (findings, _) = scan_patch_for_opaque_structures(patch, repo_root, "att-test", None);

    assert!(findings
        .iter()
        .any(|f| f.category == OpaqueCategory::UndocumentedPublicApi));
}

#[test]
fn test_scan_patch_detects_silent_fallback() {
    let repo_root = Path::new("/workspace");
    let patch = r#"[{"file": "src/tools/loader.rs", "search": "let v = get_data();", "replace": "let v = get_data().unwrap_or_default();"}]"#;
    let (findings, _) = scan_patch_for_opaque_structures(patch, repo_root, "att-test", None);

    assert!(findings
        .iter()
        .any(|f| f.category == OpaqueCategory::SilentFallback));
}

#[test]
fn test_simulate_10000_reviewer_governance_veto_on_safety() {
    let node = make_test_node("att-safety-fail", "", AttemptStatus::SafetyRejected);
    let findings = Vec::new();
    let safety = Degree5Safety {
        protected_paths_clean: false,
        rule1_verified: false,
        merkle_tree_equality: None,
        has_killswitch_bypass: false,
    };

    let consensus = simulate_10000_reviewer_governance(&node, &findings, &safety);
    assert_eq!(consensus.total_reviewers, 10000);
    assert!(consensus.has_safety_veto);
    assert_eq!(consensus.decision, GovernanceDecision::HardRejectVeto);
    assert!(consensus.votes_veto >= 3500);
}

#[test]
fn test_simulate_10000_reviewer_governance_approve_on_clean_mutation() {
    let node = make_test_node("att-clean", "", AttemptStatus::Evaluated);
    let findings = Vec::new();
    let safety = Degree5Safety {
        protected_paths_clean: true,
        rule1_verified: true,
        merkle_tree_equality: None,
        has_killswitch_bypass: false,
    };

    let consensus = simulate_10000_reviewer_governance(&node, &findings, &safety);
    assert_eq!(consensus.total_reviewers, 10000);
    assert!(!consensus.has_safety_veto);
    assert_eq!(consensus.decision, GovernanceDecision::ApproveForPromotion);
    assert_eq!(consensus.votes_approve, 10000);
    assert_eq!(consensus.consensus_score, 1.0);
}

#[test]
fn test_investigate_attempt_full_6_degrees() {
    let repo_root = Path::new("/workspace");
    let patch = r#"[{"file": "src/tools/helper.rs", "search": "// a", "replace": "// b"}]"#;
    let node = make_test_node("att-full", patch, AttemptStatus::Evaluated);

    let dossier = investigate_attempt(&node, repo_root);
    assert_eq!(dossier.attempt_id, "att-full");
    assert_eq!(
        dossier.degrees.degree_1_intent.hypothesis_id,
        "hyp-att-full"
    );
    assert_eq!(
        dossier.degrees.degree_2_syntax.files_touched,
        vec!["src/tools/helper.rs"]
    );
    assert_eq!(
        dossier.degrees.degree_4_empirical.status,
        AttemptStatus::Evaluated
    );
    assert!(dossier.degrees.degree_5_safety.protected_paths_clean);
    assert_eq!(dossier.degrees.degree_6_lineage.generation, 1);
    assert_eq!(
        dossier.consensus.decision,
        GovernanceDecision::ApproveForPromotion
    );
}

#[test]
fn test_export_markdown_renders_table_and_consensus() {
    let repo_root = Path::new("/workspace");
    let patch = r#"[{"file": "src/tools/worker.rs", "search": "let x = foo();", "replace": "let x = foo().unwrap();"}]"#;
    let node = make_test_node("att-doc", patch, AttemptStatus::Evaluated);

    let dossier = investigate_attempt(&node, repo_root);
    let md = export_markdown(&dossier);

    assert!(md.contains("# RSI Investigative Review Dossier: `att-doc`"));
    assert!(md.contains("## 1. The 6 Degrees of Grounded Connection"));
    assert!(md.contains("## 2. Opaque Structure Findings"));
    assert!(md.contains("## 3. Maximum-Power Grounded Citations"));
    assert!(md.contains("## 4. 10,000-Reviewer Governance & Deliberation"));
    assert!(md.contains("Frontier Safety & Boundary Compliance"));
}
