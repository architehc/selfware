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
