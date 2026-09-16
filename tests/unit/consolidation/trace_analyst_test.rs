use super::*;
use chrono::Utc;
use tempfile::tempdir;

#[test]
fn test_attribution_analyst_decouples_environment_friction() {
    let analyst = AttributionAnalyst::new();

    // Environment friction cases
    assert_eq!(
        analyst.classify("HTTP 429 Too Many Requests: rate limit exceeded", None),
        FailureCategory::EnvironmentFriction
    );
    assert_eq!(
        analyst.classify("connection to api.anthropic.com timed out after 30s", None),
        FailureCategory::EnvironmentFriction
    );
    assert_eq!(
        analyst.classify("connection refused on localhost:8000", None),
        FailureCategory::EnvironmentFriction
    );

    // Actionable syntax / compilation cases
    assert_eq!(
        analyst.classify(
            "error[E0308]: mismatched types: expected `bool`, found `i32`",
            Some("cargo_check")
        ),
        FailureCategory::SyntaxOrCompilation
    );

    // Actionable reasoning gap cases
    assert_eq!(
        analyst.classify("missing required parameter `file_path`", Some("file_edit")),
        FailureCategory::ReasoningGap
    );
}

#[test]
fn test_safety_invariant_auditor_rejects_protected_paths() {
    let auditor = SafetyInvariantAuditor::new();

    // Evolution and safety modules are strictly protected
    let res = auditor.audit_candidate(
        "test_playbook",
        "# Steps\n1. Do something safe",
        &["src/evolution/daemon.rs".to_string()],
    );
    assert!(matches!(res, SafetyAuditResult::Rejected { .. }));

    // Unprotected path is approved
    let safe_res = auditor.audit_candidate(
        "test_playbook",
        "# Steps\n1. Do something safe",
        &["src/ui/helpers.rs".to_string()],
    );
    assert_eq!(safe_res, SafetyAuditResult::Approved);
}

#[test]
fn test_safety_invariant_auditor_rejects_assertion_weakening_and_destructive_cmds() {
    let auditor = SafetyInvariantAuditor::new();

    // Assertion weakening
    let res_weaken = auditor.audit_candidate(
        "test_weaken",
        "# Playbook\nLower threshold from 0.8 to 0.4 and delete test case",
        &[],
    );
    assert!(matches!(res_weaken, SafetyAuditResult::Rejected { .. }));

    // Dangerous command
    let res_cmd = auditor.audit_candidate(
        "test_cmd",
        "# Playbook\nRun rm -rf / to clean workspace",
        &[],
    );
    assert!(matches!(res_cmd, SafetyAuditResult::Rejected { .. }));
}

#[test]
fn test_dual_analyst_consensus_flow() {
    let evaluator = DualAnalystEvaluator::new();

    // Scenario 1: Transient network timeout -> ignored
    let timeout_event = SessionLogEvent {
        timestamp: Utc::now(),
        session_id: "s1".to_string(),
        event_type: crate::agent::session_log::SessionEventType::ToolCall,
        task_id: None,
        tool_name: Some("web_fetch".to_string()),
        input: None,
        arguments: None,
        result: Some("Request timed out after 30s".to_string()),
        success: Some(false),
        duration_ms: Some(30000),
        details: None,
    };

    let res = evaluator.evaluate(&[timeout_event], "fetch_mitigation", &[]);
    assert!(matches!(
        res,
        ConsensusResult::IgnoredEnvironmentFriction { .. }
    ));

    // Scenario 2: Actionable compile failure + safe paths -> Consensus reached!
    let compile_event = SessionLogEvent {
        timestamp: Utc::now(),
        session_id: "s2".to_string(),
        event_type: crate::agent::session_log::SessionEventType::ToolCall,
        task_id: None,
        tool_name: Some("cargo_check".to_string()),
        input: None,
        arguments: None,
        result: Some("error[E0432]: unresolved import `crate::utils`".to_string()),
        success: Some(false),
        duration_ms: Some(1500),
        details: None,
    };

    let res_consensus = evaluator.evaluate(
        &[compile_event],
        "import_fix_playbook",
        &["src/utils/mod.rs".to_string()],
    );

    match res_consensus {
        ConsensusResult::ConsensusReached {
            finding,
            candidate_playbook_content,
        } => {
            assert_eq!(finding.category, FailureCategory::SyntaxOrCompilation);
            assert!(candidate_playbook_content.contains("candidate: true"));
            assert!(candidate_playbook_content.contains("admitted: false"));
            assert!(candidate_playbook_content.contains("origin: distilled"));
            assert!(candidate_playbook_content.contains("# Playbook: import_fix_playbook"));

            // Verify saving to candidate store
            let tmp = tempdir().unwrap();
            let saved = DualAnalystEvaluator::save_candidate_playbook(
                tmp.path(),
                "import_fix_playbook",
                &candidate_playbook_content,
            )
            .unwrap();
            assert!(saved.exists());
        }
        other => panic!("Expected ConsensusReached, got: {:?}", other),
    }
}
