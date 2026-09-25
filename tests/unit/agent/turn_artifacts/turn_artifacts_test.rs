use super::*;
use crate::api::types::{ToolCall, ToolFunction};

#[test]
fn ensure_selfware_gitignore_writes_wildcard_once() {
    let tmp = tempfile::TempDir::new().unwrap();
    let selfware = tmp.path().join(".selfware");
    std::fs::create_dir_all(&selfware).unwrap();

    ensure_selfware_gitignore(&selfware);
    let gi = selfware.join(".gitignore");
    assert!(gi.is_file(), "gitignore should be created");
    assert!(std::fs::read_to_string(&gi).unwrap().contains('*'));

    // Idempotent + non-clobbering: an existing file is left untouched.
    std::fs::write(&gi, "custom\n").unwrap();
    ensure_selfware_gitignore(&selfware);
    assert_eq!(std::fs::read_to_string(&gi).unwrap(), "custom\n");
}

#[test]
fn ensure_selfware_gitignore_noop_when_dir_absent() {
    let tmp = tempfile::TempDir::new().unwrap();
    let missing = tmp.path().join(".selfware");
    ensure_selfware_gitignore(&missing); // must not create the dir
    assert!(!missing.exists());
}

fn sample_artifact() -> TurnArtifact {
    TurnArtifact {
        step: 1,
        timestamp: Utc::now(),
        request_body: serde_json::json!({
            "model": "selfware",
            "messages": [{"role": "user", "content": "hi"}],
            "api_key": "sk-secret",
        }),
        response_body: serde_json::json!({
            "choices": [{"message": {"role": "assistant", "content": "ok"}}],
            "usage": {"prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7},
        }),
        // Pre-ledger artifacts had no evidence field; `None` is the shape
        // those sessions produce, and must stay loadable.
        evidence: None,
        finish_reason: Some("stop".to_string()),
        completion_tokens: Some(2),
        prompt_tokens: Some(5),
        reasoning_content: Some("<think>this is the plan</think>".to_string()),
        parsed_tool_calls: vec![ToolCall {
            id: "call_0".into(),
            call_type: "function".into(),
            function: ToolFunction {
                name: "file_read".into(),
                arguments: r#"{"path":"src/lib.rs"}"#.into(),
            },
        }],
        agent_decision: AgentDecision::ExecutedTools {
            tools: vec!["file_read".into()],
        },
        elapsed_ms: 1234,
        logprobs: None,
    }
}

#[test]
fn sanitize_strips_top_level_api_key_and_authorization() {
    let mut body = serde_json::json!({
        "model": "selfware",
        "api_key": "sk-secret-123",
        "Authorization": "Bearer abc",
    });
    sanitize_request_body(&mut body);
    assert_eq!(body["api_key"], "<redacted>");
    assert_eq!(body["Authorization"], "<redacted>");
    // Non-secret keys preserved.
    assert_eq!(body["model"], "selfware");
}

#[test]
fn sanitize_strips_nested_headers_with_mixed_case() {
    let mut body = serde_json::json!({
        "headers": {
            "Authorization": "Bearer abc",
            "X-Api-Key": "sk",
            "AUTHORIZATION": "Bearer XYZ",
            "X-API-KEY": "sk-upper",
            "Content-Type": "application/json",
        },
    });
    sanitize_request_body(&mut body);
    assert_eq!(body["headers"]["Authorization"], "<redacted>");
    assert_eq!(body["headers"]["X-Api-Key"], "<redacted>");
    assert_eq!(body["headers"]["AUTHORIZATION"], "<redacted>");
    assert_eq!(body["headers"]["X-API-KEY"], "<redacted>");
    // Non-secret header preserved.
    assert_eq!(body["headers"]["Content-Type"], "application/json");
}

#[test]
fn sanitize_walks_deeply_nested_extra_body() {
    // Real-world shape: `extra_body.headers.api_key`, plus `extra_body.api_key`
    // (some OpenAI-compatible backends accept this), plus `auth.bearer_token`.
    let mut body = serde_json::json!({
        "model": "selfware",
        "extra_body": {
            "api_key": "sk-extra-body-leak",
            "headers": {
                "Authorization": "Bearer deep",
                "X-API-KEY": "sk-deep",
            },
            "auth": {
                "bearer_token": "tok-1",
                "access_token": "tok-2",
            },
        },
    });
    sanitize_request_body(&mut body);
    assert_eq!(body["extra_body"]["api_key"], "<redacted>");
    assert_eq!(body["extra_body"]["headers"]["Authorization"], "<redacted>");
    assert_eq!(body["extra_body"]["headers"]["X-API-KEY"], "<redacted>");
    assert_eq!(body["extra_body"]["auth"]["bearer_token"], "<redacted>");
    assert_eq!(body["extra_body"]["auth"]["access_token"], "<redacted>");
    // Sibling field preserved.
    assert_eq!(body["model"], "selfware");
}

#[test]
fn sanitize_handles_arrays_and_prefix_variants() {
    // Authorization headers buried inside a `tools[].config.headers` array
    // and a top-level `secrets[]` array of credential objects.
    let mut body = serde_json::json!({
        "tools": [
            {"name": "fetch", "config": {"headers": {"authorization": "Bearer leak1"}}},
            {"name": "post",  "config": {"headers": {"X-Api-Key": "leak2"}}},
        ],
        "secrets": [
            {"password": "p1", "api_key": "k1"},
            {"refresh_token": "rt1"},
        ],
        "openai_api_key": "sk-inline",
    });
    sanitize_request_body(&mut body);
    assert_eq!(
        body["tools"][0]["config"]["headers"]["authorization"],
        "<redacted>"
    );
    assert_eq!(
        body["tools"][1]["config"]["headers"]["X-Api-Key"],
        "<redacted>"
    );
    assert_eq!(body["secrets"][0]["password"], "<redacted>");
    assert_eq!(body["secrets"][0]["api_key"], "<redacted>");
    assert_eq!(body["secrets"][1]["refresh_token"], "<redacted>");
    assert_eq!(body["openai_api_key"], "<redacted>");
}

#[test]
fn sanitize_preserves_token_count_fields() {
    // The `token` matcher must not eat `completion_tokens`, `prompt_tokens`,
    // `total_tokens`, `max_tokens`, or `tool_call_id`.  These are usage /
    // identifier fields that we explicitly want preserved in artifacts.
    let mut body = serde_json::json!({
        "messages": [{
            "role": "tool",
            "tool_call_id": "call_123",
            "content": "ok",
        }],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 5,
            "total_tokens": 15,
        },
        "max_tokens": 4096,
        "tokenizer": "cl100k",
    });
    sanitize_request_body(&mut body);
    assert_eq!(body["messages"][0]["tool_call_id"], "call_123");
    assert_eq!(body["usage"]["prompt_tokens"], 10);
    assert_eq!(body["usage"]["completion_tokens"], 5);
    assert_eq!(body["usage"]["total_tokens"], 15);
    assert_eq!(body["max_tokens"], 4096);
    assert_eq!(body["tokenizer"], "cl100k");
}

#[test]
fn sanitize_redacts_token_suffix_keys() {
    // `*_token` and bare `token` should be redacted.
    let mut body = serde_json::json!({
        "token": "raw",
        "access_token": "at",
        "refresh_token": "rt",
        "id_token": "idt",
        "auth-token": "ath",
    });
    sanitize_request_body(&mut body);
    assert_eq!(body["token"], "<redacted>");
    assert_eq!(body["access_token"], "<redacted>");
    assert_eq!(body["refresh_token"], "<redacted>");
    assert_eq!(body["id_token"], "<redacted>");
    assert_eq!(body["auth-token"], "<redacted>");
}

#[test]
fn turn_artifact_roundtrips_through_json() {
    let original = sample_artifact();
    let json = serde_json::to_string_pretty(&original).expect("serialize");
    let decoded: TurnArtifact = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(decoded.step, original.step);
    assert_eq!(decoded.finish_reason, original.finish_reason);
    assert_eq!(decoded.completion_tokens, original.completion_tokens);
    assert_eq!(decoded.prompt_tokens, original.prompt_tokens);
    assert_eq!(decoded.reasoning_content, original.reasoning_content);
    assert_eq!(decoded.elapsed_ms, original.elapsed_ms);
    assert_eq!(decoded.parsed_tool_calls.len(), 1);
    assert_eq!(decoded.parsed_tool_calls[0].function.name, "file_read");
    assert_eq!(decoded.agent_decision, original.agent_decision);
    // Timestamp roundtrips with microsecond precision in chrono's RFC3339;
    // compare via formatted string to avoid float-style flakiness.
    assert_eq!(
        decoded.timestamp.timestamp_millis(),
        original.timestamp.timestamp_millis()
    );
}

#[tokio::test]
async fn write_artifact_creates_file() {
    let dir = tempfile::tempdir().expect("create tempdir");
    let artifact = sample_artifact();
    write_artifact(dir.path(), &artifact).await;
    let written = artifact_dir(dir.path()).join("turn_0001.json");
    assert!(
        written.exists(),
        "turn_0001.json should exist at {:?}",
        written
    );
    let content = std::fs::read_to_string(&written).expect("read written artifact");
    let _decoded: TurnArtifact =
        serde_json::from_str(&content).expect("written artifact must be valid JSON");
}

#[tokio::test]
async fn write_artifact_prunes_to_cap() {
    let dir = tempfile::tempdir().expect("create tempdir");
    // Write more than the retention cap; the directory must stay bounded.
    let over = MAX_TURN_ARTIFACTS + 25;
    for step in 1..=over {
        let mut a = sample_artifact();
        a.step = step;
        write_artifact(dir.path(), &a).await;
    }
    let turns = artifact_dir(dir.path());
    let count = std::fs::read_dir(&turns)
        .expect("turns dir exists")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|n| n.starts_with("turn_") && n.ends_with(".json"))
                .unwrap_or(false)
        })
        .count();
    assert_eq!(
        count, MAX_TURN_ARTIFACTS,
        "turns dir must be capped at {} files, found {}",
        MAX_TURN_ARTIFACTS, count
    );
}

#[tokio::test]
async fn turn_artifact_preserves_logprobs() {
    let dir = tempfile::tempdir().expect("create tempdir");
    let mut artifact = sample_artifact();
    let sample_logprobs = serde_json::json!({
        "content": [
            {"token": "hello", "logprob": -0.05},
            {"token": " world", "logprob": -0.12}
        ]
    });
    artifact.logprobs = Some(sample_logprobs.clone());
    artifact.response_body = serde_json::json!({
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "hello world",
            },
            "finish_reason": "stop",
            "logprobs": sample_logprobs,
        }],
        "usage": {
            "prompt_tokens": 5,
            "completion_tokens": 2,
        },
    });

    write_artifact(dir.path(), &artifact).await;
    let written = artifact_dir(dir.path()).join("turn_0001.json");
    assert!(written.exists());
    let content = std::fs::read_to_string(&written).expect("read written artifact");
    let decoded: TurnArtifact =
        serde_json::from_str(&content).expect("written artifact must be valid JSON");

    assert_eq!(decoded.logprobs, Some(sample_logprobs.clone()));
    assert_eq!(
        decoded.response_body["choices"][0]["logprobs"],
        sample_logprobs
    );
}

// =========================================================================
// Honest decisions + append-only history (2026-09-24 live validation)
// =========================================================================

fn executed(name: &str, ok: bool) -> DispatchEvent {
    DispatchEvent::Executed {
        name: name.to_string(),
        ok,
    }
}

fn answered(name: &str, success: bool, text: &str) -> DispatchEvent {
    DispatchEvent::Answered {
        name: name.to_string(),
        success,
        text: text.to_string(),
    }
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn classify_lists_only_executed_calls_as_executed() {
    let journal = vec![
        answered(
            "nope",
            false,
            "Safety check failed: tool 'nope' does not exist",
        ),
        executed("file_read", true),
        answered("file_read", true, "{...}"),
        executed("cargo_test", false),
        answered("cargo_test", false, "1 test failed"),
    ];
    let decision = classify_dispatch(
        &names(&["nope", "file_read", "cargo_test"]),
        &journal,
        "not dispatched",
    );
    assert_eq!(
        decision,
        AgentDecision::Dispatched {
            tools: vec![
                ExecutedTool {
                    name: "file_read".into(),
                    ok: true
                },
                ExecutedTool {
                    name: "cargo_test".into(),
                    ok: false
                },
            ],
            rejected_tools: vec![RejectedTool {
                name: "nope".into(),
                reason: "Safety check failed: tool 'nope' does not exist".into(),
            }],
        },
        "a failed execution is executed with ok=false, never a rejection"
    );
}

#[test]
fn classify_all_refused_is_rejected_tools() {
    let journal = vec![
        answered(
            "file_edit",
            false,
            "Tool call validation failed: missing old_str",
        ),
        answered("file_read", false, "Suppressed repeated failing call"),
    ];
    let decision = classify_dispatch(&names(&["file_edit", "file_read"]), &journal, "x");
    match decision {
        AgentDecision::RejectedTools { rejected_tools } => {
            assert_eq!(rejected_tools.len(), 2);
            assert!(rejected_tools[0].reason.contains("validation failed"));
        }
        other => panic!("expected rejected_tools, got {other:?}"),
    }
}

#[test]
fn classify_nothing_dispatched_is_stopped_before_dispatch() {
    let decision = classify_dispatch(
        &names(&["file_read"]),
        &[],
        "not dispatched: Token budget exhausted: 30 >= 20 tokens",
    );
    assert_eq!(
        decision,
        AgentDecision::StoppedBeforeDispatch {
            reason: "not dispatched: Token budget exhausted: 30 >= 20 tokens".into(),
            tools: names(&["file_read"]),
        }
    );
}

#[test]
fn classify_reports_calls_that_never_got_a_result() {
    // Cancellation broke the loop after the first call ran.
    let journal = vec![
        executed("file_read", true),
        answered("file_read", true, "ok"),
    ];
    let decision = classify_dispatch(
        &names(&["file_read", "file_write"]),
        &journal,
        "not dispatched: cancelled",
    );
    match decision {
        AgentDecision::Dispatched {
            tools,
            rejected_tools,
        } => {
            assert_eq!(tools.len(), 1);
            assert_eq!(
                rejected_tools,
                vec![RejectedTool {
                    name: "file_write".into(),
                    reason: "not dispatched: cancelled".into(),
                }]
            );
        }
        other => panic!("expected executed_tools, got {other:?}"),
    }
}

#[test]
fn decision_kinds_serialize_to_what_happened() {
    let kind = |d: &AgentDecision| serde_json::to_value(d).unwrap()["kind"].clone();
    // Recorded before dispatch: pending, never "executed".
    assert_eq!(
        kind(&AgentDecision::ExecutedTools {
            tools: names(&["file_read"])
        }),
        "pending_dispatch"
    );
    assert_eq!(
        kind(&AgentDecision::Dispatched {
            tools: vec![],
            rejected_tools: vec![]
        }),
        "executed_tools"
    );
    assert_eq!(
        kind(&AgentDecision::FinalAnswer { text: "x".into() }),
        "final_answer"
    );
    assert_eq!(
        kind(&AgentDecision::StoppedBeforeDispatch {
            reason: "r".into(),
            tools: vec![]
        }),
        "stopped_before_dispatch"
    );
    assert_eq!(
        kind(&AgentDecision::RejectedTools {
            rejected_tools: vec![]
        }),
        "rejected_tools"
    );
    // Artifacts written before the rename still load.
    let legacy: AgentDecision =
        serde_json::from_value(serde_json::json!({"kind": "completed", "text": "done"})).unwrap();
    assert_eq!(
        legacy,
        AgentDecision::FinalAnswer {
            text: "done".into()
        }
    );
}

#[test]
fn next_free_step_skips_existing_files() {
    let tmp = tempfile::TempDir::new().unwrap();
    assert_eq!(next_free_step(tmp.path(), 1), 1);
    assert_eq!(next_free_step(tmp.path(), 0), 1, "steps are 1-based");
    let dir = artifact_dir(tmp.path());
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(artifact_path(&dir, 1), "{}").unwrap();
    std::fs::write(artifact_path(&dir, 2), "{}").unwrap();
    std::fs::write(artifact_path(&dir, 4), "{}").unwrap();
    assert_eq!(next_free_step(tmp.path(), 1), 3);
    assert_eq!(next_free_step(tmp.path(), 4), 5);
}
