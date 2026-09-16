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

#[test]
fn test_candidate_playbook_description_with_colons_valid_yaml() {
    let evaluator = DualAnalystEvaluator::new();
    let event = SessionLogEvent {
        timestamp: Utc::now(),
        session_id: "s_colon".to_string(),
        event_type: crate::agent::session_log::SessionEventType::ToolCall,
        task_id: None,
        tool_name: Some("bash".to_string()),
        input: None,
        arguments: None,
        result: Some("error: missing required argument: --target".to_string()),
        success: Some(false),
        duration_ms: Some(100),
        details: None,
    };

    let res = evaluator.evaluate(
        &[event],
        "bash_arg_playbook",
        &["src/cli/mod.rs".to_string()],
    );

    match res {
        ConsensusResult::ConsensusReached {
            candidate_playbook_content,
            ..
        } => {
            // Must parse cleanly as a valid Skill without YAML parse error
            let skill = crate::skills::Skill::from_markdown(&candidate_playbook_content).expect(
                "Candidate playbook with colon in description must parse as valid YAML frontmatter",
            );
            assert_eq!(skill.name, "bash_arg_playbook");
            assert!(!skill.verified);
            assert!(skill.candidate);
            assert!(!skill.admitted);
        }
        other => panic!("Expected ConsensusReached, got: {:?}", other),
    }
}

#[test]
fn test_save_candidate_playbook_rejects_path_traversal_and_symlinks() {
    let tmp = tempdir().unwrap();
    let candidates_dir = tmp.path().join("skill-candidates");
    std::fs::create_dir_all(&candidates_dir).unwrap();

    // 1. Traversal attempts must be rejected
    assert!(DualAnalystEvaluator::save_candidate_playbook(
        &candidates_dir,
        "../escaped",
        "some content"
    )
    .is_err());

    assert!(DualAnalystEvaluator::save_candidate_playbook(
        &candidates_dir,
        "/etc/passwd",
        "some content"
    )
    .is_err());

    assert!(DualAnalystEvaluator::save_candidate_playbook(
        &candidates_dir,
        "foo/bar",
        "some content"
    )
    .is_err());

    // 2. Existing symlink target must be rejected
    let real_file = tmp.path().join("real_target.txt");
    std::fs::write(&real_file, "original").unwrap();
    let symlink_dest = candidates_dir.join("symlink_target.md");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&real_file, &symlink_dest).unwrap();
        let err = DualAnalystEvaluator::save_candidate_playbook(
            &candidates_dir,
            "symlink_target",
            "malicious overwrite",
        );
        assert!(err.is_err());
        assert_eq!(std::fs::read_to_string(&real_file).unwrap(), "original");
    }
}
