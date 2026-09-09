use super::*;
use serde_json::{json, Value};
use std::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const EXHAUSTED: &str = r#"{"id":"x","object":"chat.completion","created":0,"model":"m","choices":[{"index":0,"message":{"role":"assistant","content":"","reasoning_content":"hidden"},"finish_reason":"length"}],"usage":{"prompt_tokens":5,"completion_tokens":100,"total_tokens":105,"cost":0.1}}"#;
const ANSWER: &str = r#"{"id":"x","object":"chat.completion","created":0,"model":"m","choices":[{"index":0,"message":{"role":"assistant","content":"done"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":10,"total_tokens":15,"cost":0.02}}"#;

pub(crate) async fn server(
    responses: Vec<(u16, &'static str)>,
) -> (String, Arc<Mutex<Vec<Value>>>, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let captured = Arc::new(Mutex::new(Vec::new()));
    let saved = Arc::clone(&captured);
    let task = tokio::spawn(async move {
        for (status, body) in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let mut chunk = [0; 4096];
                let count = socket.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                request.extend_from_slice(&chunk[..count]);
                if let Some(split) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..split]);
                    let length: usize = headers
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse().unwrap())
                        })
                        .unwrap();
                    if request.len() >= split + 4 + length {
                        saved.lock().unwrap().push(
                            serde_json::from_slice(&request[split + 4..split + 4 + length])
                                .unwrap(),
                        );
                        break;
                    }
                }
            }
            let wire = format!("HTTP/1.1 {status} Response\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            socket.write_all(wire.as_bytes()).await.unwrap();
        }
    });
    (endpoint, captured, task)
}

fn config(endpoint: String) -> crate::config::Config {
    let mut config = crate::config::Config {
        endpoint,
        ..Default::default()
    };
    config.retry.max_retries = 0;
    config.agent.native_function_calling = true;
    config
}

fn tools() -> Option<Vec<ToolDefinition>> {
    Some(serde_json::from_value(json!([{"type":"function","function":{"name":"read_widget","description":"Read a widget","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}}])).unwrap())
}

#[tokio::test]
async fn native_fallback_rebuilds_schema_history_and_metadata_without_retry_budget() {
    let (endpoint, captured, task) = server(vec![
        (400, "unsupported tool schema"),
        (200, ANSWER),
        (200, ANSWER),
    ])
    .await;
    let client = ApiClient::new(&config(endpoint)).unwrap();
    let history: Vec<Message> = serde_json::from_value(json!([
        {"role":"system","content":"Use tools to inspect the project"},
        {"role":"user","content":"inspect"},
        {"role":"assistant","content":"","tool_calls":[{"id":"c1","type":"function","function":{"name":"read_widget","arguments":"{\"path\":\"widget.rs\"}"}}]},
        {"role":"tool","tool_call_id":"c1","content":[{"type":"text","text":"source"},{"type":"image_url","image_url":{"url":"data:image/png;base64,YQ=="}}]}
    ])).unwrap();
    let (_, meta) = client
        .chat_with_meta(history.clone(), tools(), ThinkingMode::Enabled)
        .await
        .unwrap();
    client
        .chat(history, tools(), ThinkingMode::Enabled)
        .await
        .unwrap();
    task.await.unwrap();
    let bodies = captured.lock().unwrap();
    assert_eq!(bodies.len(), 3);
    assert!(bodies[0].get("tools").is_some());
    for body in &bodies[1..] {
        assert!(body.get("tools").is_none());
        assert!(body.get("tool_choice").is_none());
        let messages = body["messages"].as_array().unwrap();
        let prompt = messages[0]["content"].as_str().unwrap();
        assert!(prompt.contains("<tool><name>TOOL_NAME"));
        assert!(prompt.contains("read_widget"));
        assert!(prompt.contains("required"));
        assert!(messages.iter().all(|m| m["role"] != "tool"
            && m.get("tool_calls").is_none()
            && m.get("tool_call_id").is_none()));
        assert!(messages.iter().any(|m| m["content"].is_array()
            && m["content"]
                .as_array()
                .unwrap()
                .iter()
                .any(|block| block["type"] == "image_url")));
    }
    assert_eq!(meta.request_body, bodies[1]);
    assert!(client.native_fc_latched());
}

#[tokio::test]
async fn profile_fallback_is_latched_per_model_even_when_global_native_is_false() {
    let (endpoint, captured, task) = server(vec![
        (400, "tools unsupported"),
        (200, ANSWER),
        (200, ANSWER),
        (200, ANSWER),
    ])
    .await;
    let mut config = config(endpoint.clone());
    config.agent.native_function_calling = false;
    let client = ApiClient::new(&config).unwrap();
    let mut profile: crate::config::ModelProfile = serde_json::from_value(
        json!({"endpoint":endpoint,"model":"a","native_function_calling":true,"max_retries":0}),
    )
    .unwrap();
    for _ in 0..2 {
        client
            .chat_with_profile(
                vec![Message::user("inspect")],
                tools(),
                ThinkingMode::Enabled,
                &profile,
            )
            .await
            .unwrap();
    }
    profile.model = "b".into();
    client
        .chat_with_profile(
            vec![Message::user("inspect")],
            tools(),
            ThinkingMode::Enabled,
            &profile,
        )
        .await
        .unwrap();
    task.await.unwrap();
    let bodies = captured.lock().unwrap();
    assert!(bodies[0].get("tools").is_some());
    assert!(bodies[1].get("tools").is_none());
    assert!(bodies[2].get("tools").is_none());
    assert_eq!(bodies[3]["tool_choice"], "auto");
    assert!(
        !client.native_fc_latched(),
        "alternate model must not disable the main model"
    );
}

#[tokio::test]
async fn reasoning_attempt_usage_survives_success_failure_and_pinned_exhaustion() {
    for (replies, pinned, succeeds, expected) in [
        (vec![(200, EXHAUSTED), (200, ANSWER)], false, true, 120),
        (vec![(200, EXHAUSTED), (200, EXHAUSTED)], false, false, 210),
        (vec![(200, EXHAUSTED), (500, "failed")], false, false, 105),
        (vec![(200, EXHAUSTED)], true, false, 105),
    ] {
        let (endpoint, _, task) = server(replies).await;
        let mut config = config(endpoint);
        if pinned {
            config.extra_body = Some(
                json!({"reasoning_effort":"high"})
                    .as_object()
                    .unwrap()
                    .clone(),
            );
        }
        let client = ApiClient::new(&config).unwrap();
        let result = client
            .chat(vec![Message::user("answer")], None, ThinkingMode::Enabled)
            .await;
        assert_eq!(result.is_ok(), succeeds);
        if let Ok(response) = result {
            assert_eq!(response.usage.total_tokens, expected);
            assert!((response.usage.cost.unwrap() - 0.12).abs() < 1e-10);
        }
        assert_eq!(client.accounted_usage().total_tokens, expected);
        let attempts = client.usage_attempts();
        assert_eq!(
            attempts[0].outcome,
            super::super::usage::AttemptOutcome::Failed
        );
        assert_eq!(attempts[0].usage.as_ref().unwrap().total_tokens, 105);
        task.await.unwrap();
    }
}

#[tokio::test]
async fn exhausted_attempt_stops_reasoning_retry_at_hard_budget() {
    let (endpoint, captured, task) = server(vec![(200, EXHAUSTED)]).await;
    let mut config = config(endpoint);
    config.agent.max_budget_tokens = Some(100);
    let client = ApiClient::new(&config).unwrap();
    let error = client
        .chat(vec![Message::user("answer")], None, ThinkingMode::Enabled)
        .await
        .unwrap_err();
    assert!(error.downcast_ref::<UsageBudgetExceeded>().is_some());
    assert!(!counts_toward_circuit_breaker(&error));
    assert_eq!(client.accounted_usage().total_tokens, 105);
    assert_eq!(captured.lock().unwrap().len(), 1);
    task.await.unwrap();
}

#[tokio::test]
async fn mixed_reported_and_unknown_costs_are_not_presented_as_complete() {
    const UNKNOWN_COST: &str = r#"{"id":"x","object":"chat.completion","created":0,"model":"m","choices":[{"index":0,"message":{"role":"assistant","content":"done"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":10,"total_tokens":15}}"#;
    for (replies, expected_tokens, known_cost) in [
        (vec![(200, EXHAUSTED), (200, UNKNOWN_COST)], 120, 0.1),
        (
            vec![(500, "provider did not report usage"), (200, ANSWER)],
            15,
            0.02,
        ),
    ] {
        let (endpoint, _, task) = server(replies).await;
        let mut config = config(endpoint);
        config.retry.max_retries = 1;
        config.retry.base_delay_ms = 0;
        let client = ApiClient::new(&config).unwrap();
        let (response, metadata) = client
            .chat_with_meta(vec![Message::user("answer")], None, ThinkingMode::Enabled)
            .await
            .unwrap();
        assert_eq!(response.usage.total_tokens, expected_tokens);
        assert_eq!(
            response.usage.cost, None,
            "a partial cost must not masquerade as the complete charge"
        );
        assert_eq!(metadata.cost, None);
        assert_eq!(
            client.accounted_usage().cost,
            Some(known_cost),
            "hard caps must retain known charges"
        );
        assert_eq!(client.cost_accounting_status(), (false, 1));
        task.await.unwrap();
    }
}

#[tokio::test]
async fn restored_usage_is_not_reported_again_in_a_new_response() {
    let (endpoint, _, task) = server(vec![(200, ANSWER)]).await;
    let client = ApiClient::new(&config(endpoint)).unwrap();
    client.ensure_budget_floor(1000, 0.5);
    client.mark_restored_usage();
    let response = client
        .chat(vec![Message::user("answer")], None, ThinkingMode::Enabled)
        .await
        .unwrap();
    assert_eq!(response.usage.total_tokens, 15);
    assert_eq!(response.usage.cost, Some(0.02));
    assert_eq!(client.accounted_usage().total_tokens, 1015);
    assert_eq!(client.cost_accounting_status(), (false, 0));
    task.await.unwrap();
}

#[tokio::test]
async fn stream_retry_with_unknown_cost_keeps_known_charge_only_in_run_ledger() {
    const SSE: &str = "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":10,\"total_tokens\":15,\"cost\":0.02}}\n\ndata: [DONE]\n\n";
    let (endpoint, _, task) = server(vec![(400, "unsupported tools"), (200, SSE)]).await;
    let client = ApiClient::new(&config(endpoint)).unwrap();
    let response = client
        .chat_stream(
            vec![Message::user("inspect")],
            tools(),
            ThinkingMode::Enabled,
        )
        .await
        .unwrap()
        .collect()
        .await
        .unwrap();
    assert_eq!(response.usage.total_tokens, 15);
    assert_eq!(response.usage.cost, None);
    assert_eq!(client.accounted_usage().cost, Some(0.02));
    assert_eq!(client.cost_accounting_status(), (false, 1));
    task.await.unwrap();
}

#[tokio::test]
async fn streaming_fallback_and_failed_stream_retain_usage() {
    const SSE: &str = "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":10,\"total_tokens\":15,\"cost\":0.02},\"error\":{\"message\":\"provider failed\"}}\n\ndata: [DONE]\n\n";
    let (endpoint, captured, task) =
        server(vec![(400, "unsupported tool schema"), (200, SSE)]).await;
    let client = ApiClient::new(&config(endpoint)).unwrap();
    let (stream, metadata) = client
        .chat_stream_with_meta(
            vec![Message::user("inspect")],
            tools(),
            ThinkingMode::Enabled,
        )
        .await
        .unwrap();
    assert!(stream.collect().await.is_err());
    task.await.unwrap();
    assert_eq!(client.accounted_usage().total_tokens, 15);
    assert_eq!(client.accounted_usage().cost, Some(0.02));
    assert!(metadata.request_body.get("tools").is_none());
    assert!(metadata.request_body["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("read_widget"));
    assert_eq!(captured.lock().unwrap().len(), 2);
    assert_eq!(
        client.usage_attempts()[1].outcome,
        super::super::usage::AttemptOutcome::Failed
    );
}

#[tokio::test]
async fn fim_without_provider_usage_preserves_unknown_usage() {
    const COMPLETION: &str = r#"{"id":"x","object":"text_completion","created":0,"model":"m","choices":[{"index":0,"text":"replacement","finish_reason":"stop"}]}"#;
    let (endpoint, _, task) = server(vec![(200, COMPLETION)]).await;
    let client = ApiClient::new(&config(endpoint)).unwrap();
    let response = client.completion("prefix", Some(32), None).await.unwrap();
    assert!(response.usage.is_none());
    assert!(client.usage_attempts()[0].usage.is_none());
    assert_eq!(client.cost_accounting_status(), (false, 1));
    task.await.unwrap();
}

#[test]
fn rebuild_and_resume_preserve_run_limits_and_provider_latches() {
    let mut config = config("http://localhost:11434/v1".into());
    config.agent.max_wall_secs = Some(60);
    let client = ApiClient::new(&config).unwrap();
    client.restore_wall_budget(65);
    client.ensure_budget_floor(123, 0.5);
    client.latch_tool_mode(&config.endpoint, &config.model);
    let same = client.rebuild(&config).unwrap();
    assert!(same.wall_budget_stop().is_some());
    assert_eq!(same.accounted_usage().total_tokens, 123);
    assert!(same.native_fc_latched());
    config.model = "different-model".into();
    let changed = same.rebuild(&config).unwrap();
    assert!(changed.wall_budget_stop().is_some());
    assert!(!changed.native_fc_latched());
    changed.reset_wall_budget();
    assert!(changed.wall_budget_stop().is_none());
    assert_eq!(changed.accounted_usage().total_tokens, 0);
}

#[tokio::test]
async fn response_headers_and_body_share_one_absolute_run_deadline() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 8192];
        assert!(
            socket.read(&mut request).await.unwrap() > 0,
            "request must arrive before delayed response"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            ANSWER.len()
        );
        if socket.write_all(headers.as_bytes()).await.is_ok() {
            tokio::time::sleep(Duration::from_millis(900)).await;
            let _ = socket.write_all(ANSWER.as_bytes()).await;
        }
    });
    let mut config = config(endpoint);
    config.agent.max_wall_secs = Some(1);
    let client = ApiClient::new(&config).unwrap();
    let error = client
        .chat(vec![Message::user("answer")], None, ThinkingMode::Enabled)
        .await
        .unwrap_err();
    assert!(
        error.downcast_ref::<WallClockBudgetExceeded>().is_some(),
        "{error:#}"
    );
    let attempts = client.usage_attempts();
    assert_eq!(attempts.len(), 1);
    assert_eq!(
        attempts[0].outcome,
        super::super::usage::AttemptOutcome::Failed
    );
    assert!(
        attempts[0].usage.is_none(),
        "unfinished responses have unknown usage"
    );
    task.await.unwrap();
}

#[tokio::test]
async fn absent_provider_usage_keeps_metadata_unknown_and_charges_measured_fallback() {
    const NO_USAGE: &str = r#"{"id":"x","object":"chat.completion","created":0,"model":"m","choices":[{"index":0,"message":{"role":"assistant","content":"Measured answer"},"finish_reason":"stop"}]}"#;
    let (endpoint, _, task) = server(vec![(200, NO_USAGE)]).await;
    let client = ApiClient::new(&config(endpoint)).unwrap();
    let (_, meta) = client
        .chat_with_meta(
            vec![Message::user("Explain token accounting")],
            None,
            ThinkingMode::Enabled,
        )
        .await
        .unwrap();
    assert_eq!(meta.prompt_tokens, None);
    assert_eq!(meta.completion_tokens, None);
    assert_eq!(meta.total_tokens, None);
    assert_eq!(meta.cost, None);
    let measured = meta.accounted_usage.unwrap();
    assert!(measured.prompt_tokens > 0);
    assert_eq!(
        measured.completion_tokens,
        crate::token_count::estimate_content_tokens("Measured answer")
    );
    assert_eq!(client.accounted_usage().total_tokens, measured.total_tokens);
    let attempt = client.usage_attempts().remove(0);
    assert!(attempt.usage.is_none());
    assert_eq!(
        attempt.estimated_usage.unwrap().total_tokens,
        measured.total_tokens
    );
    task.await.unwrap();
}

#[tokio::test]
async fn clean_sse_eof_after_finish_reason_is_completed_in_attempt_ledger() {
    const SSE: &str = "data: {\"choices\":[{\"delta\":{\"content\":\"answer\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":10,\"total_tokens\":15,\"cost\":0}}\n\n";
    let (endpoint, _, task) = server(vec![(200, SSE)]).await;
    let client = ApiClient::new(&config(endpoint)).unwrap();
    let response = client
        .chat_stream(vec![Message::user("answer")], None, ThinkingMode::Enabled)
        .await
        .unwrap()
        .collect()
        .await
        .unwrap();
    assert_eq!(response.choices[0].message.content.text(), "answer");
    assert_eq!(response.choices[0].finish_reason.as_deref(), Some("stop"));
    assert_eq!(
        client.usage_attempts()[0].outcome,
        super::super::usage::AttemptOutcome::Completed
    );
    task.await.unwrap();
}
