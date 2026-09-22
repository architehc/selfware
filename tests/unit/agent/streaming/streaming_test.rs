use super::*;

// =========================================================================
// extract_tool_name tests: <function=name> pattern
// =========================================================================

#[test]
fn test_extract_tool_name_function_equals_pattern() {
    let xml = r#"<function=file_read>{"path": "foo.rs"}</function>"#;
    let result = Agent::extract_tool_name(xml);
    assert_eq!(result, Some("file_read".to_string()));
}

#[test]
fn test_extract_tool_name_function_equals_with_angle_bracket() {
    let xml = "<function=shell_exec>";
    let result = Agent::extract_tool_name(xml);
    assert_eq!(result, Some("shell_exec".to_string()));
}

#[test]
fn test_extract_tool_name_function_equals_with_newline() {
    let xml = "<function=git_status\nsome other content";
    let result = Agent::extract_tool_name(xml);
    assert_eq!(result, Some("git_status".to_string()));
}

#[test]
fn test_extract_tool_name_function_equals_with_surrounding_text() {
    let xml = "some text before <function=cargo_check> and after";
    let result = Agent::extract_tool_name(xml);
    assert_eq!(result, Some("cargo_check".to_string()));
}

#[test]
fn test_extract_tool_name_function_equals_with_less_than_terminator() {
    let xml = "<function=my_tool<extra>";
    let result = Agent::extract_tool_name(xml);
    assert_eq!(result, Some("my_tool".to_string()));
}

// =========================================================================
// extract_tool_name tests: <function>name</function> pattern
// =========================================================================

#[test]
fn test_extract_tool_name_function_tag_pattern() {
    let xml = "<function>file_write</function>";
    let result = Agent::extract_tool_name(xml);
    assert_eq!(result, Some("file_write".to_string()));
}

#[test]
fn test_extract_tool_name_function_tag_with_whitespace() {
    let xml = "<function>  grep_search  </function>";
    let result = Agent::extract_tool_name(xml);
    assert_eq!(
        result,
        Some("grep_search".to_string()),
        "Whitespace around the name should be trimmed"
    );
}

#[test]
fn test_extract_tool_name_function_tag_with_surrounding_content() {
    let xml = "prefix text <function>directory_tree</function> suffix text";
    let result = Agent::extract_tool_name(xml);
    assert_eq!(result, Some("directory_tree".to_string()));
}

// =========================================================================
// extract_tool_name tests: empty / None cases
// =========================================================================

#[test]
fn test_extract_tool_name_empty_string() {
    let result = Agent::extract_tool_name("");
    assert_eq!(result, None, "Empty string should return None");
}

#[test]
fn test_extract_tool_name_no_function_tag() {
    let result = Agent::extract_tool_name("just some regular text with no tags");
    assert_eq!(
        result, None,
        "Text without function tags should return None"
    );
}

#[test]
fn test_extract_tool_name_function_equals_empty_name() {
    // <function=> followed immediately by a terminator yields an empty name
    let result = Agent::extract_tool_name("<function=>");
    assert_eq!(
        result, None,
        "Empty name after function= should return None"
    );
}

#[test]
fn test_extract_tool_name_function_tag_empty_body() {
    let result = Agent::extract_tool_name("<function></function>");
    assert_eq!(
        result, None,
        "Empty body inside <function></function> should return None"
    );
}

#[test]
fn test_extract_tool_name_function_tag_whitespace_only_body() {
    let result = Agent::extract_tool_name("<function>   </function>");
    assert_eq!(
        result, None,
        "Whitespace-only body inside <function> tags should return None"
    );
}

// =========================================================================
// extract_tool_name tests: malformed input
// =========================================================================

#[test]
fn test_extract_tool_name_unclosed_function_tag() {
    // <function>name but no closing </function>
    let result = Agent::extract_tool_name("<function>file_read");
    assert_eq!(result, None, "Unclosed <function> tag should return None");
}

#[test]
fn test_extract_tool_name_partial_function_equals() {
    // <function without = sign
    let result = Agent::extract_tool_name("<function something>");
    assert_eq!(
        result, None,
        "Partial <function without = should return None"
    );
}

#[test]
fn test_extract_tool_name_other_xml_tags() {
    let result = Agent::extract_tool_name("<tool>file_read</tool>");
    assert_eq!(result, None, "Non-function XML tags should return None");
}

#[test]
fn test_extract_tool_name_function_equals_takes_priority() {
    // When both patterns are present, <function=name> is checked first
    let xml = "<function=first_tool> <function>second_tool</function>";
    let result = Agent::extract_tool_name(xml);
    assert_eq!(
        result,
        Some("first_tool".to_string()),
        "<function=name> pattern should take priority"
    );
}

#[test]
fn test_extract_tool_name_complex_xml_content() {
    let xml = r#"<tool_call>
<function=file_edit>
{"path": "src/main.rs", "old_str": "hello", "new_str": "world"}
</function>
</tool_call>"#;
    let result = Agent::extract_tool_name(xml);
    assert_eq!(result, Some("file_edit".to_string()));
}

// =========================================================================
// Streaming display filter tests
// =========================================================================

#[test]
fn find_earliest_open_tag_tool_call() {
    let buf = "hello <tool_call>stuff";
    let result = find_earliest_open_tag(buf);
    assert_eq!(result, Some((6, 0)));
}

#[test]
fn find_earliest_open_tag_tool() {
    let buf = "hello <tool>stuff";
    let result = find_earliest_open_tag(buf);
    assert_eq!(result, Some((6, 1)));
}

#[test]
fn find_earliest_open_tag_think() {
    let buf = "text <think>reasoning here";
    let result = find_earliest_open_tag(buf);
    assert_eq!(result, Some((5, 2)));
}

#[test]
fn find_earliest_open_tag_thinking() {
    let buf = "text <thinking>reasoning here";
    let result = find_earliest_open_tag(buf);
    assert_eq!(result, Some((5, 3)));
}

#[test]
fn find_earliest_open_tag_none() {
    let buf = "just regular text no tags";
    assert_eq!(find_earliest_open_tag(buf), None);
}

#[test]
fn find_earliest_open_tag_picks_first_when_multiple() {
    let buf = "<think>reasoning</think> <tool>call</tool>";
    let result = find_earliest_open_tag(buf);
    assert_eq!(result.unwrap().0, 0); // <think> is first
}

#[test]
fn has_partial_tag_at_end_detects_partial_tool() {
    assert!(has_partial_tag_at_end("hello <too"));
    assert!(has_partial_tag_at_end("hello <tool"));
    assert!(has_partial_tag_at_end("hello <t"));
}

#[test]
fn has_partial_tag_at_end_detects_partial_think() {
    assert!(has_partial_tag_at_end("hello <thin"));
    assert!(has_partial_tag_at_end("hello <think"));
}

#[test]
fn has_partial_tag_at_end_no_partial() {
    assert!(!has_partial_tag_at_end("hello world"));
    assert!(!has_partial_tag_at_end("hello <tool>complete"));
    assert!(!has_partial_tag_at_end(""));
}

#[test]
fn extract_display_name_from_tool_block() {
    let xml = "<tool>\n<name>file_write</name>\n<arguments>{}</arguments>\n</tool>";
    assert_eq!(extract_display_name(xml), Some("file_write".to_string()));
}

#[test]
fn extract_display_name_from_function_block() {
    let xml = r#"<tool_call><function=shell_exec>{"cmd":"ls"}</function></tool_call>"#;
    assert_eq!(extract_display_name(xml), Some("shell_exec".to_string()));
}

#[test]
fn extract_display_name_from_think_block() {
    let xml = "<think>I should read the file first</think>";
    assert_eq!(extract_display_name(xml), None);
}

#[test]
fn suppressed_tags_covers_all_local_model_formats() {
    // Ensure all common local model XML formats are covered
    let formats = ["<tool_call>", "<tool>", "<think>", "<thinking>"];
    for fmt in &formats {
        assert!(
            SUPPRESSED_TAGS.iter().any(|(open, _)| open == fmt),
            "Missing suppressed tag: {}",
            fmt
        );
    }
}

// --- Runaway monologue cutoff ---
// TB 3.0 failure mode (cli-2ph-simplex, 2026-08-24): the agent burned a 2500s
// task timeout inside two giant no-tool responses (~1,059 log lines of
// analysis in the first alone) and never wrote a line of code. A mutation
// task's response that streams past the analysis budget with no tool call in
// flight must be cut so the no-action escalation fires on schedule.

#[test]
fn runaway_monologue_requires_all_three_conditions() {
    let over = MONOLOGUE_CHAR_BUDGET + 1;
    // All three: over budget, no tool call in flight, mutation task.
    assert!(is_runaway_monologue(over, false, true));
    // Under budget: fine.
    assert!(!is_runaway_monologue(MONOLOGUE_CHAR_BUDGET, false, true));
    // Tool call in flight (native or XML markup in content): don't cut —
    // a long file_write argument is legitimate content, not a monologue.
    assert!(!is_runaway_monologue(over, true, true));
    // Read-only task: long prose IS the deliverable — never cut.
    assert!(!is_runaway_monologue(over, false, false));
}

#[tokio::test]
async fn runaway_monologue_is_cut_on_mutation_task() {
    use crate::testing::mock_api::MockLlmServer;

    let big = "analysis text without any tool call. ".repeat(2000); // ~70k chars
    let server = MockLlmServer::builder().with_response(big).build().await;
    let config = crate::config::Config {
        endpoint: format!("{}/v1", server.url()),
        ..Default::default()
    };
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_task_context =
        "Implement the two-phase simplex solver in /app/simplex.py".to_string();

    let (content, _reasoning, tools) = agent
        .chat_streaming(
            vec![Message::user("implement it now")],
            None,
            ThinkingMode::Enabled,
            None,
        )
        .await
        .expect("stream completes");

    assert!(
        content.contains("response truncated"),
        "runaway monologue must carry the cut marker, got: {}",
        &content[..content.len().min(200)]
    );
    assert!(
        tools.as_ref().map(|t| t.is_empty()).unwrap_or(true),
        "no tool calls in this response"
    );
    server.stop().await;
}

#[tokio::test]
async fn long_answer_is_not_cut_on_read_only_task() {
    use crate::testing::mock_api::MockLlmServer;

    let big = "thorough explanation paragraph. ".repeat(2000); // ~64k chars
    let server = MockLlmServer::builder().with_response(big).build().await;
    let config = crate::config::Config {
        endpoint: format!("{}/v1", server.url()),
        ..Default::default()
    };
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_task_context = "Explain how this repository handles retries".to_string();

    let (content, _reasoning, _tools) = agent
        .chat_streaming(
            vec![Message::user("explain in detail")],
            None,
            ThinkingMode::Enabled,
            None,
        )
        .await
        .expect("stream completes");

    assert!(
        !content.contains("response truncated"),
        "read-only deliverables are never cut"
    );
    assert_eq!(
        content.len(),
        2000 * "thorough explanation paragraph. ".len()
    );
    server.stop().await;
}

#[test]
fn cumulative_stream_usage_snapshots_emit_only_new_token_deltas() {
    let mut previous = crate::api::Usage::default();
    let mut added = (0, 0);
    for (prompt, completion) in [(10, 5), (10, 7), (10, 7), (9, 6), (10, 9)] {
        let usage = crate::api::Usage {
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: prompt + completion,
            cost: None,
            ..Default::default()
        };
        let delta = streaming_usage_delta(&mut previous, &usage);
        added.0 += delta.0;
        added.1 += delta.1;
    }
    assert_eq!(added, (10, 9));
    assert_eq!(previous.total_tokens, 19);
}

#[tokio::test]
async fn streaming_session_usage_events_match_run_ledger_for_repeated_snapshots() {
    #[derive(Default)]
    struct Events(std::sync::Mutex<Vec<AgentEvent>>);
    impl super::super::tui_events::EventEmitter for Events {
        fn emit(&self, event: AgentEvent) {
            self.0.lock().unwrap().push(event);
        }
    }
    const SSE: &str = "data: {\"choices\":[{\"delta\":{\"content\":\"answer\"}}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"total_tokens\":15}}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":7,\"total_tokens\":17}}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":7,\"total_tokens\":17}}\n\ndata: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":9,\"total_tokens\":19}}\n\n";
    let (endpoint, _, server) =
        crate::api::client::review_regressions::server(vec![(200, SSE)]).await;
    let events = std::sync::Arc::new(Events::default());
    let mut config = crate::config::Config {
        endpoint,
        ..Default::default()
    };
    config.cache.enabled = false;
    let agent = Agent::new(config)
        .await
        .unwrap()
        .with_event_emitter(events.clone());
    agent
        .chat_streaming(
            vec![Message::user("Explain accounting")],
            None,
            ThinkingMode::Enabled,
            None,
        )
        .await
        .unwrap();
    let (prompt, completion) = events
        .0
        .lock()
        .unwrap()
        .iter()
        .fold((0, 0), |(p, c), event| match event {
            AgentEvent::TokenUsage {
                prompt_tokens,
                completion_tokens,
            } => (p + prompt_tokens, c + completion_tokens),
            _ => (p, c),
        });
    assert_eq!((prompt, completion), (10, 9));
    assert_eq!(agent.run_summary().total_tokens, 19);
    server.await.unwrap();
}

#[tokio::test]
async fn streaming_omitted_completion_tokens_triggers_measured_output_fallback() {
    // When a provider stream emits generated text but reports only prompt_tokens
    // (omitting completion_tokens), the agent must not treat completion as Some(0).
    // Instead, it must fall back to measured token estimation of the generated output
    // so budget caps remain honest and cannot be undercounted (AGENTS.md Rule 3 & 4).
    const SSE: &str = "data: {\"choices\":[{\"delta\":{\"content\":\"hello world generated by model\"}}]}\n\ndata: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10}}\n\n";
    let (endpoint, _, server) =
        crate::api::client::review_regressions::server(vec![(200, SSE)]).await;
    let mut config = crate::config::Config {
        endpoint,
        ..Default::default()
    };
    config.cache.enabled = false;
    config.agent.streaming = true;
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages = vec![Message::user("generate text")];

    let resp = agent.get_assistant_step_response(false).await.unwrap();

    assert_eq!(resp.content, "hello world generated by model");
    let chat_meta = resp.metadata.expect("metadata should be captured");
    assert_eq!(chat_meta.prompt_tokens, Some(10));
    assert_eq!(
        chat_meta.completion_tokens, None,
        "missing completion tokens must not deserialize/record as Some(0)"
    );

    let usage = agent.cumulative_token_usage();
    assert_eq!(usage.input, 10);
    assert!(
        usage.output > 0,
        "output tokens must be measured from generated text fallback, got: {}",
        usage.output
    );
    assert_eq!(usage.total, usage.input + usage.output);
    assert_eq!(agent.run_summary().total_tokens, usage.total);
    server.await.unwrap();
}

#[tokio::test]
async fn stream_that_closes_without_terminal_is_incomplete_not_success() {
    // Consumer-side mirror of the W2b producer contract (2026-09-21 review,
    // P2): a mock that sends CONTENT then closes without [DONE] and without a
    // provider finish_reason is a TRUNCATED stream. The consumer must fail it
    // with a typed incomplete outcome — never store it as a success by
    // promoting it with a synthesized `finish_reason: "stream_end"`.
    const SSE: &str = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"The incomplete answer is\"},\"finish_reason\":null}]}\n\n";
    let (endpoint, _, server) =
        crate::api::client::review_regressions::server(vec![(200, SSE)]).await;
    let config = crate::config::Config {
        endpoint,
        cache: crate::session::cache::LlmCacheConfig {
            enabled: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let agent = Agent::new(config).await.unwrap();

    let err = agent
        .chat_streaming(
            vec![Message::user("finish the answer")],
            None,
            ThinkingMode::Enabled,
            None,
        )
        .await
        .expect_err("a stream that closes without a terminal must not succeed");

    let text = format!("{err:#}");
    assert!(
        text.contains("accepted terminal indication"),
        "the failure must name the missing terminal, got: {text}"
    );
    assert!(
        err.downcast_ref::<crate::errors::ApiError>().is_some(),
        "the failure must be a typed ApiError (incomplete outcome), got: {text}"
    );
    server.await.unwrap();
}

#[tokio::test]
async fn stream_ending_with_finish_reason_but_no_done_is_complete() {
    // Clean-EOF providers that finish with a finish_reason chunk but send no
    // [DONE] sentinel are complete streams — the producer's collect() accepts
    // them, and the consumer must too (the terminal-indication guard accepts
    // either [DONE] or a provider finish_reason).
    const SSE: &str = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"The complete answer\"},\"finish_reason\":null}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n";
    let (endpoint, _, server) =
        crate::api::client::review_regressions::server(vec![(200, SSE)]).await;
    let config = crate::config::Config {
        endpoint,
        cache: crate::session::cache::LlmCacheConfig {
            enabled: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let agent = Agent::new(config).await.unwrap();

    let (content, _reasoning, _tools) = agent
        .chat_streaming(
            vec![Message::user("finish the answer")],
            None,
            ThinkingMode::Enabled,
            None,
        )
        .await
        .expect("a finish_reason-without-[DONE] stream is complete");
    assert_eq!(content, "The complete answer");
    server.await.unwrap();
}

#[tokio::test]
async fn streaming_prompt_and_total_without_completion_does_not_double_count() {
    // If usage reports prompt=100, total=150 but omits completion, the ledger charges 150.
    // The agent falls back to measured output estimation for the completion tokens, but must
    // reconcile against the already-accounted total so it does NOT charge 150 + output (200),
    // which would stop prematurely at budget limits (AGENTS.md Rule 4).
    const SSE: &str = "data: {\"choices\":[{\"delta\":{\"content\":\"hello world generated by model\"}}]}\n\ndata: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":100,\"total_tokens\":150}}\n\n";
    let (endpoint, _, server) =
        crate::api::client::review_regressions::server(vec![(200, SSE)]).await;
    let mut config = crate::config::Config {
        endpoint,
        ..Default::default()
    };
    config.cache.enabled = false;
    config.agent.streaming = true;
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages = vec![Message::user("generate text")];

    let resp = agent.get_assistant_step_response(false).await.unwrap();

    assert_eq!(resp.content, "hello world generated by model");
    let chat_meta = resp.metadata.expect("metadata should be captured");
    assert_eq!(chat_meta.prompt_tokens, Some(100));
    assert_eq!(chat_meta.completion_tokens, None);
    assert_eq!(chat_meta.total_tokens, Some(150));

    let usage = agent.cumulative_token_usage();
    assert_eq!(usage.input, 100);
    assert!(
        usage.output > 0,
        "output tokens should have measured estimate"
    );
    // Total must be 150 (the reported total that already accounts for output), NOT 150 + output.
    assert_eq!(
        usage.total, 150,
        "cumulative total must reconcile against reported total and not double-count: got {}",
        usage.total
    );
    assert_eq!(agent.run_summary().total_tokens, 150);
    server.await.unwrap();
}
