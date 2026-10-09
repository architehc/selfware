use super::*;

#[test]
fn strict_report_parser_accepts_only_a_bare_schema_conforming_object() {
    let valid = r#"{
        "summary":"checked",
        "findings":[],
        "messages":[]
    }"#;
    assert_eq!(parse_primitive_report(valid).unwrap().summary, "checked");
    assert_eq!(
        parse_primitive_report(&format!("<think>private reasoning</think>{valid}"))
            .unwrap()
            .summary,
        "checked"
    );

    for invalid in [
        format!("```json\n{valid}\n```"),
        format!("{valid}\ntrailing prose"),
        r#"{"summary":"checked","findings":[],"messages":[],"sender":"forged"}"#.to_string(),
    ] {
        assert!(
            parse_primitive_report(&invalid).is_err(),
            "non-strict response was accepted: {invalid}"
        );
    }
}

#[test]
fn source_window_centers_focus_and_stays_inside_measured_budget() {
    let content = (1..=180)
        .map(|line| {
            if line == 120 {
                "fn focus_target() { compute_result(); }".to_string()
            } else {
                format!("let value_{line} = calculate({line});")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let token_budget = 96;

    let (numbered, start, end, total, whole_file) =
        select_source_window(&content, Some("focus_target"), false, token_budget).unwrap();

    assert_eq!(total, 180);
    assert!(!whole_file);
    assert!(start <= 120 && end >= 120, "window was {start}..={end}");
    assert!(estimate_content_tokens(&numbered) <= token_budget);
    let delivered = crate::agent::review_shards::numbered_lines(&numbered);
    assert_eq!(delivered.first().map(|(line, _)| *line), Some(start));
    assert_eq!(delivered.last().map(|(line, _)| *line), Some(end));
}

#[test]
fn source_window_fails_closed_when_one_line_exceeds_the_budget() {
    let content = "token ".repeat(1_000);
    let error = select_source_window(&content, None, false, 8).unwrap_err();
    assert!(error
        .to_string()
        .contains("focused source line exceeds the artifact token budget"));
}

#[test]
fn worker_reservation_measures_the_exact_disabled_thinking_wire_messages() {
    let messages = vec![
        Message::system("artifact worker"),
        Message::user("inspect this bounded source snapshot"),
    ];
    let reply_tokens = 128;
    let pre_wire = crate::token_count::estimate_messages_tokens(&messages) + reply_tokens;
    let prepared =
        crate::api::prepare_request_messages(messages.clone(), crate::api::ThinkingMode::Disabled);
    let wire = crate::token_count::estimate_messages_tokens(&prepared) + reply_tokens;

    assert!(wire > pre_wire, "the injected instruction must be reserved");
    assert_eq!(projected_worker_tokens(&messages, reply_tokens), wire);
}

#[test]
fn finding_verification_requires_the_exact_cited_snapshot_line() {
    let assignment = PrimitiveAssignment {
        key: "artifact-001".to_string(),
        graph_node_id: None,
        path: "src/lib.rs".to_string(),
        classification: "rust_source".to_string(),
        base_sha256: "a".repeat(64),
        focus_symbol: None,
        reasons: Vec::new(),
        read_attestation: super::super::types::ReadAttestationSummary {
            attestation_id: "read-1".to_string(),
            exact_snapshot_read: true,
            write: false,
            execute: false,
            network: false,
        },
    };
    let findings = vec![
        RawFinding {
            line: 4,
            severity: super::super::types::Severity::High,
            title: "real defect".to_string(),
            evidence_quote: "let vulnerable = true;".to_string(),
        },
        RawFinding {
            line: 5,
            severity: super::super::types::Severity::Low,
            title: "fabricated".to_string(),
            evidence_quote: "this text does not exist".to_string(),
        },
        RawFinding {
            line: 4,
            severity: super::super::types::Severity::Low,
            title: "substring".to_string(),
            evidence_quote: "vulnerable = true".to_string(),
        },
        RawFinding {
            line: 9,
            severity: super::super::types::Severity::Low,
            title: "relocated".to_string(),
            evidence_quote: "let vulnerable = true;".to_string(),
        },
        RawFinding {
            line: 6,
            severity: super::super::types::Severity::High,
            title: "arbitrary claim anchored to a blank line".to_string(),
            evidence_quote: "   ".to_string(),
        },
    ];

    let (verified, rejected) = verify_findings(
        &assignment,
        findings,
        &[
            (4, "let vulnerable = true;".to_string()),
            (6, "   ".to_string()),
        ],
    );
    assert_eq!(verified.len(), 1);
    assert_eq!(verified[0].evidence.path, "src/lib.rs");
    assert_eq!(verified[0].evidence.line, 4);
    assert_eq!(
        verified[0].reported_severity,
        super::super::types::Severity::High
    );
    assert_eq!(verified[0].evidence.quote, "let vulnerable = true;");
    assert_eq!(rejected, 4);
}

#[test]
fn remote_source_egress_is_default_deny_and_explicitly_reported() {
    assert_eq!(
        source_egress_policy("http://127.0.0.1:11434/v1", false).unwrap(),
        SourceEgress::LocalEndpoint
    );
    let denied = source_egress_policy("https://models.example/v1", false).unwrap_err();
    assert!(denied.to_string().contains("source egress"));
    assert_eq!(
        source_egress_policy("https://models.example/v1", true).unwrap(),
        SourceEgress::RemoteExplicitlyAuthorized
    );
    assert!(
        source_egress_policy("https://localhost@models.example/v1", true)
            .unwrap_err()
            .to_string()
            .contains("userinfo")
    );
    if crate::config::api_key::plaintext_remote_allowed() {
        assert_eq!(
            source_egress_policy("http://models.example/v1", true).unwrap(),
            SourceEgress::RemotePlaintextExplicitlyAuthorized
        );
    } else {
        assert!(source_egress_policy("http://models.example/v1", true)
            .unwrap_err()
            .to_string()
            .contains("requires HTTPS"));
    }
}

#[tokio::test]
async fn artifact_transport_does_not_replay_source_bodies_across_redirects() {
    use axum::extract::State;
    use axum::http::{header, StatusCode};
    use axum::routing::post;
    use axum::Router;
    use std::sync::atomic::{AtomicUsize, Ordering};

    async fn redirected_sink(State(hits): State<Arc<AtomicUsize>>) -> StatusCode {
        hits.fetch_add(1, Ordering::SeqCst);
        StatusCode::OK
    }

    async fn redirect(State(location): State<String>) -> impl axum::response::IntoResponse {
        (
            StatusCode::TEMPORARY_REDIRECT,
            [(header::LOCATION, location)],
        )
    }

    let hits = Arc::new(AtomicUsize::new(0));
    let sink = Router::new()
        .route("/capture", post(redirected_sink))
        .with_state(Arc::clone(&hits));
    let sink_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let sink_address = sink_listener.local_addr().unwrap();
    let sink_task = tokio::spawn(async move { axum::serve(sink_listener, sink).await.unwrap() });

    let origin = Router::new()
        .route("/v1/chat/completions", post(redirect))
        .with_state(format!("http://{sink_address}/capture"));
    let origin_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin_address = origin_listener.local_addr().unwrap();
    let origin_task =
        tokio::spawn(async move { axum::serve(origin_listener, origin).await.unwrap() });

    let config = Config {
        endpoint: format!("http://{origin_address}/v1"),
        ..Default::default()
    };
    let client = ApiClient::new_for_artifact_harness(&config).unwrap();
    let result = client
        .side_chat(
            vec![Message::user("SOURCE_BODY_MUST_NOT_BE_REDIRECTED")],
            SideCall::new("artifact_redirect_test")
                .max_tokens(16)
                .time_cap_secs(5)
                .single_attempt(),
        )
        .await;

    assert!(
        result.is_err(),
        "a redirect must not be treated as a model response"
    );
    assert_eq!(
        hits.load(Ordering::SeqCst),
        0,
        "redirect target received source-bearing POST body"
    );
    origin_task.abort();
    sink_task.abort();
}

#[cfg(unix)]
#[tokio::test]
async fn two_round_run_reactivates_only_the_broker_message_recipient() {
    use crate::testing::mock_api::{MockLlmServer, MockResponse};

    const SOURCE_LINE: &str = "pub fn exact_evidence_marker() -> bool { true }";
    let temp = tempfile::Builder::new()
        .prefix(".primitive-runtime-")
        .tempdir_in(".")
        .unwrap();
    let root = std::fs::canonicalize(temp.path()).unwrap();
    std::fs::write(root.join("alpha.rs"), format!("{SOURCE_LINE}\n")).unwrap();
    std::fs::write(root.join("beta.rs"), format!("{SOURCE_LINE}\n")).unwrap();

    let first = serde_json::json!({
        "summary": "first artifact checked",
        "findings": [{
            "line": 1,
            "severity": "low",
            "title": "evidence remains bound to the sender snapshot",
            "evidence_quote": SOURCE_LINE,
        }],
        "messages": [{
            "to": "artifact-002",
            "kind": "interface_constraint",
            "evidence": [{"line": 1, "quote": SOURCE_LINE}],
        }],
    })
    .to_string();
    let second = serde_json::json!({
        "summary": "second artifact checked in round one",
        "findings": [],
        "messages": [],
    })
    .to_string();
    let reactivated = serde_json::json!({
        "summary": "broker evidence considered in round two",
        "findings": [],
        "messages": [],
    })
    .to_string();
    let server = MockLlmServer::builder()
        .with_response(first)
        .with_response(second)
        .with_response(reactivated)
        .with_default_response(MockResponse::Error {
            status: 500,
            body: "unexpected fourth primitive call".to_string(),
        })
        .build()
        .await;
    let mut config = Config {
        endpoint: format!("{}/v1", server.url()),
        max_tokens: 128,
        ..Default::default()
    };
    config.safety.allowed_paths = vec!["/**".to_string()];
    // FIFO mock responses are deterministic because the harness schedules one
    // physical model request at a time for this test.
    config.concurrency.max_streams = 1;
    config.concurrency.max_global = 1;

    let task = "review exact_evidence_marker in alpha.rs and beta.rs";
    let report = run_artifact_harness(&config, task, &root, 2, 2, false)
        .await
        .expect("the bounded two-round run should succeed");
    let bodies = server.captured_request_bodies().await;
    server.stop().await;

    assert_eq!(report.status, ArtifactRunStatus::AnalysisComplete);
    assert_eq!(report.task_outcome, TaskOutcome::NotEvaluated);
    assert_eq!((report.rounds_requested, report.rounds_completed), (2, 2));
    assert_eq!(report.plan.assignments.len(), 2);
    assert_eq!(
        report
            .plan
            .assignments
            .iter()
            .map(|assignment| assignment.path.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from(["alpha.rs", "beta.rs"])
    );
    assert_eq!((report.calls_scheduled, report.turns_planned), (3, 3));
    assert_eq!(report.turns_analyzed, 3);
    assert_eq!(report.evidence_matched_findings, 1);
    assert_eq!(report.rejected_findings, 0);
    assert!(report.stop_reason.is_none(), "{:?}", report.stop_reason);

    assert_eq!(report.messages.len(), 1);
    let message = &report.messages[0];
    assert_eq!((message.sequence, message.round), (1, 1));
    assert_eq!(message.from, "artifact-001");
    assert_eq!(message.to, "artifact-002");
    assert_eq!(message.evidence.len(), 1);
    assert_eq!(message.evidence[0].line, 1);
    assert_eq!(message.evidence[0].sha256.len(), 64);
    assert!(message.evidence[0]
        .identifiers
        .iter()
        .any(|identifier| identifier == "exact_evidence_marker"));
    assert!(report.turns.iter().any(|turn| {
        turn.artifact == "artifact-002"
            && turn.round == 2
            && turn.status == WorkerCallStatus::Analyzed
            && turn
                .summary
                .as_ref()
                .is_some_and(|summary| summary.text == "broker evidence considered in round two")
    }));

    assert_eq!(
        bodies.len(),
        3,
        "only the recipient should run in round two"
    );
    let round_two_prompts = bodies
        .iter()
        .filter_map(|body| serde_json::from_str::<serde_json::Value>(body).ok())
        .filter_map(|body| body["messages"][1]["content"].as_str().map(str::to_string))
        .filter(|prompt| prompt.contains("\"round\": 2"))
        .collect::<Vec<_>>();
    assert_eq!(round_two_prompts.len(), 1);
    assert!(round_two_prompts[0].contains("<primitive_inbox"));
    assert!(round_two_prompts[0].contains("\"from\": \"artifact-001\""));
    assert!(round_two_prompts[0].contains("\"to\": \"artifact-002\""));

    assert!(report.authority.read_only);
    assert!(!report.authority.model_tools);
    assert!(!report.authority.model_filesystem_write);
    assert!(!report.authority.model_command_execution);
    assert!(!report.authority.model_network_tools);
    assert_eq!(report.authority.source_egress, SourceEgress::LocalEndpoint);
    assert!(report.authority.source_content_transmission_attempted);
    assert!(report.authority.relative_paths_transmission_attempted);
    assert!(report.authority.content_hashes_transmission_attempted);
    assert_eq!(report.authority.acceptance_owner, "deterministic_harness");

    let rendered = report.render_text(false);
    assert!(rendered.contains("Artifact harness: AnalysisComplete"));
    assert!(rendered.contains("task outcome not evaluated"));
}

#[test]
fn model_text_is_rendered_as_one_untrusted_line() {
    let value = sanitize_model_text("headline\nforged: status ok\tmore", 128);
    assert!(!value.trusted);
    assert_eq!(value.text, "headline forged: status ok more");
}
