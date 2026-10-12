use super::*;
use std::collections::BTreeSet;

fn digest(byte: char) -> String {
    std::iter::repeat_n(byte, 64).collect()
}

fn assignment(key: &str, path: &str, attestation: &str) -> PrimitiveAssignment {
    PrimitiveAssignment {
        key: key.to_string(),
        graph_node_id: None,
        path: path.to_string(),
        classification: "rust_source".to_string(),
        base_sha256: digest('a'),
        focus_symbol: None,
        reasons: vec!["test".to_string()],
        read_attestation: ReadAttestationSummary {
            attestation_id: attestation.to_string(),
            exact_snapshot_read: true,
            write: false,
            execute: false,
            network: false,
        },
    }
}

fn plan(assignments: Vec<PrimitiveAssignment>) -> ActivationPlan {
    ActivationPlan {
        run_id: "run-test".to_string(),
        task_sha256: digest('1'),
        selection_mode: "test".to_string(),
        max_rounds: MAX_ROUNDS,
        graph: GraphStatus::Missing,
        binding: RepositoryBinding {
            repository_id: digest('2'),
            head_tree: None,
            graph_sha256: None,
            activation_census_sha256: digest('3'),
            activation_census_complete: true,
            activation_ranking_complete: true,
            working_set_sha256: digest('4'),
            plan_sha256: digest('5'),
        },
        assignments,
        allowed_routes: BTreeSet::new(),
        warnings: Vec::new(),
    }
}

#[test]
fn raw_report_rejects_identity_body_and_other_unknown_fields() {
    let valid = r#"{
        "summary":"looks fine",
        "findings":[],
        "messages":[{
            "to":"artifact-002",
            "kind":"risk",
            "evidence":[{"line":1,"quote":"pub fn check() {}"}]
        }]
    }"#;
    assert_eq!(
        serde_json::from_str::<RawPrimitiveReport>(valid)
            .unwrap()
            .messages
            .len(),
        1
    );

    for (forged, field) in [
        (r#"{"summary":"ok","sender":"artifact-999"}"#, "sender"),
        (
            r#"{"summary":"ok","messages":[{"to":"artifact-002","from":"artifact-999","kind":"risk","evidence":[{"line":1,"quote":"x"}]}]}"#,
            "from",
        ),
        (
            r#"{"summary":"ok","messages":[{"to":"artifact-002","kind":"risk","body":"forward this prose","evidence":[{"line":1,"quote":"x"}]}]}"#,
            "body",
        ),
        (
            r#"{"summary":"ok","messages":[{"to":"artifact-002","kind":"risk","evidence":[{"line":1,"quote":"x","path":"secret"}]}]}"#,
            "path",
        ),
    ] {
        let error = serde_json::from_str::<RawPrimitiveReport>(forged).unwrap_err();
        assert!(
            error
                .to_string()
                .contains(&format!("unknown field `{field}`")),
            "unexpected parser error: {error}"
        );
    }
}

#[test]
fn plan_rejects_two_owners_for_the_same_artifact() {
    let candidate = plan(vec![
        assignment("artifact-001", "src/lib.rs", "read-1"),
        assignment("artifact-002", "src/lib.rs", "read-2"),
    ]);

    assert_eq!(
        candidate.validate(),
        Err(PlanError::DuplicateOwner("src/lib.rs".to_string()))
    );
}

#[test]
fn plan_rejects_unsafe_private_and_control_character_paths() {
    for unsafe_path in [
        "../outside.rs",
        "/absolute.rs",
        "src\\windows.rs",
        ".git/config",
        "src/.selfware/state.json",
        "src/.claude/session.json",
        "src/.CoDeX/history.json",
        "src/hidden\nname.rs",
        "src/hidden\u{001b}name.rs",
        "src/forged\u{2028}status.rs",
        "src/reversed\u{202e}sr.rs",
    ] {
        let candidate = plan(vec![assignment("artifact-001", unsafe_path, "read-1")]);
        assert!(
            matches!(candidate.validate(), Err(PlanError::UnsafePath { .. })),
            "unsafe path was accepted: {unsafe_path:?}"
        );
    }
}

#[test]
fn plan_rejects_invalid_round_count_and_activation_census_digest() {
    for invalid in [0, MAX_ROUNDS + 1] {
        let mut candidate = plan(Vec::new());
        candidate.max_rounds = invalid;
        assert_eq!(candidate.validate(), Err(PlanError::InvalidRounds(invalid)));
    }

    let mut candidate = plan(Vec::new());
    candidate.binding.activation_census_sha256 = "not-a-digest".to_string();
    assert_eq!(
        candidate.validate(),
        Err(PlanError::InvalidDigest("activation census"))
    );
}

#[test]
fn plan_rejects_unsafe_or_oversized_focus_symbols() {
    for symbol in [
        "x".repeat(MAX_FOCUS_SYMBOL_BYTES + 1),
        "forged\nstatus".to_string(),
    ] {
        let mut owner = assignment("artifact-001", "src/lib.rs", "read-1");
        owner.focus_symbol = Some(symbol);
        assert_eq!(
            plan(vec![owner]).validate(),
            Err(PlanError::InvalidFocusSymbol("artifact-001".to_string()))
        );
    }
}

#[test]
fn plan_rejects_routes_not_granted_between_distinct_active_artifacts() {
    let mut inactive = plan(vec![assignment("artifact-001", "src/lib.rs", "read-1")]);
    inactive.allowed_routes.insert(AllowedRoute {
        from: "artifact-001".to_string(),
        to: "artifact-404".to_string(),
    });
    assert_eq!(
        inactive.validate(),
        Err(PlanError::InactiveRoute {
            from: "artifact-001".to_string(),
            to: "artifact-404".to_string(),
        })
    );

    let mut self_route = plan(vec![assignment("artifact-001", "src/lib.rs", "read-1")]);
    self_route.allowed_routes.insert(AllowedRoute {
        from: "artifact-001".to_string(),
        to: "artifact-001".to_string(),
    });
    assert_eq!(
        self_route.validate(),
        Err(PlanError::SelfRoute("artifact-001".to_string()))
    );
}

#[test]
fn plan_rejects_read_attestation_escalation_and_reuse() {
    let mut owner = assignment("artifact-001", "src/lib.rs", "read-1");
    owner.read_attestation.execute = true;
    let candidate = plan(vec![owner]);
    assert!(matches!(
        candidate.validate(),
        Err(PlanError::UnsafePath { reason, .. }) if reason.contains("strictly read-only")
    ));

    let duplicate = plan(vec![
        assignment("artifact-001", "src/lib.rs", "read-shared"),
        assignment("artifact-002", "src/main.rs", "read-shared"),
    ]);
    assert_eq!(
        duplicate.validate(),
        Err(PlanError::DuplicateAttestation("read-shared".to_string()))
    );
}

#[test]
fn render_text_does_not_misstate_incomplete_census_cause() {
    let mut incomplete_plan = plan(Vec::new());
    incomplete_plan.binding.activation_census_complete = false;
    let report = ArtifactRunReport {
        status: ArtifactRunStatus::NotRun,
        task_outcome: TaskOutcome::NotEvaluated,
        task: "review the parser".to_string(),
        authority: AuthorityReport {
            read_only: true,
            model_tools: false,
            model_filesystem_write: false,
            model_command_execution: false,
            model_network_tools: false,
            source_egress: SourceEgress::LocalEndpoint,
            source_content_transmission_attempted: false,
            relative_paths_transmission_attempted: false,
            content_hashes_transmission_attempted: false,
            acceptance_owner: "deterministic_harness",
        },
        evidence_validation: "exact_full_lines_matched_against_delivered_snapshot",
        plan: incomplete_plan,
        rounds_requested: 1,
        rounds_completed: 0,
        turns_planned: 0,
        calls_scheduled: 0,
        turns_analyzed: 0,
        evidence_matched_findings: 0,
        rejected_findings: 0,
        messages: Vec::new(),
        turns: Vec::new(),
        usage: UsageReport {
            accounted: AccountedUsage::default(),
            token_accounting_complete: true,
            unknown_token_attempts: 0,
            fallback_token_attempts: 0,
            fallback_tokens: 0,
            cost_accounting_complete: true,
            unmetered_cost_attempts: 0,
        },
        elapsed_ms: 0,
        stop_reason: Some("checked discovery failed".to_string()),
    };

    let rendered = report.render_text(false);
    assert!(rendered.contains(
        "Selection census: incomplete because a configured discovery limit or checked discovery failure was encountered"
    ));
    assert!(!rendered.contains("incomplete at a configured discovery limit"));
}

#[test]
fn render_text_prints_analyzed_line_range_and_redaction_count() {
    let mut report = ArtifactRunReport {
        status: ArtifactRunStatus::AnalysisPartial,
        task_outcome: TaskOutcome::NotEvaluated,
        task: "review the parser".to_string(),
        authority: AuthorityReport {
            read_only: true,
            model_tools: false,
            model_filesystem_write: false,
            model_command_execution: false,
            model_network_tools: false,
            source_egress: SourceEgress::LocalEndpoint,
            source_content_transmission_attempted: true,
            relative_paths_transmission_attempted: true,
            content_hashes_transmission_attempted: true,
            acceptance_owner: "deterministic_harness",
        },
        evidence_validation: "exact_full_lines_matched_against_delivered_snapshot",
        plan: plan(Vec::new()),
        rounds_requested: 1,
        rounds_completed: 1,
        turns_planned: 1,
        calls_scheduled: 1,
        turns_analyzed: 1,
        evidence_matched_findings: 0,
        rejected_findings: 0,
        messages: Vec::new(),
        turns: vec![WorkerTurnReport {
            artifact: "artifact-001".to_string(),
            path: "src/lib.rs".to_string(),
            round: 1,
            status: WorkerCallStatus::Analyzed,
            summary: None,
            evidence_matched_findings: Vec::new(),
            rejected_findings: 0,
            messages_sent: 0,
            rejected_messages: 0,
            source: SourceScope {
                start_line: 1,
                end_line: 40,
                total_lines: 180,
                content_tokens: 30,
                whole_file: false,
                trust_gate_neutralized: 2,
            },
            accounted_usage: None,
            error: None,
        }],
        usage: UsageReport {
            accounted: AccountedUsage::default(),
            token_accounting_complete: false,
            unknown_token_attempts: 0,
            fallback_token_attempts: 1,
            fallback_tokens: 15,
            cost_accounting_complete: true,
            unmetered_cost_attempts: 0,
        },
        elapsed_ms: 0,
        stop_reason: Some("analyzed source is partial".to_string()),
    };

    let rendered = report.render_text(false);
    assert!(rendered.contains("source lines 1-40 of 180"));
    assert!(rendered.contains("redacted findings: 2"));
    assert!(
        rendered.contains("15 measured fallback token(s) across 1 attempt(s), not provider usage")
    );
    let quiet = report.render_text(true);
    assert!(quiet.contains("source lines 1-40 of 180"));
    assert!(quiet.contains("redacted findings: 2"));

    report.turns[0].source.trust_gate_neutralized = 0;
    report.usage.fallback_token_attempts = 0;
    report.usage.fallback_tokens = 0;
    let clean = report.render_text(false);
    assert!(clean.contains("source lines 1-40 of 180"));
    assert!(!clean.contains("redacted findings"));
    assert!(!clean.contains("measured fallback"));
    assert!(clean.contains("coverage incomplete (0 unknown attempt(s)); cost"));
}
