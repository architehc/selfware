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
