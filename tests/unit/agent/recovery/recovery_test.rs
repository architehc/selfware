use super::*;

#[test]
fn extract_mentioned_image_paths_finds_absolute_image() {
    let paths = extract_mentioned_image_paths(
        "Use vision_analyze on /tmp/project/samples/frame_00000693.jpg and answer.",
    );
    assert_eq!(
        paths,
        vec!["/tmp/project/samples/frame_00000693.jpg".to_string()]
    );
}

#[test]
fn build_vision_analyze_fallback_args_uses_task_prompt_suffix() {
    let args = build_vision_analyze_fallback_args(
            "Use vision_analyze on /tmp/frame.png and answer in one short sentence describing the main subject.",
        )
        .expect("expected fallback args");
    let parsed: serde_json::Value = serde_json::from_str(&args).expect("valid json");
    assert_eq!(parsed["image_path"], "/tmp/frame.png");
    assert_eq!(
        parsed["prompt"],
        "answer in one short sentence describing the main subject."
    );
}

#[test]
fn extract_mentioned_path_finds_absolute_markdown_file() {
    let path =
        extract_mentioned_path("Use file_read on /tmp/project/AGENTS.md and answer in one line.")
            .expect("expected path");
    assert_eq!(path, "/tmp/project/AGENTS.md");
}

#[test]
fn strip_think_blocks_removes_paired_tags() {
    let content = "<think>inner thought</think>answer";
    assert_eq!(strip_think_blocks(content), "answer");
}

#[test]
fn strip_think_blocks_preserves_unclosed_tag_content() {
    // An unmatched </think> should not erase the preceding answer.
    let content = "final answer text </think>";
    assert_eq!(strip_think_blocks(content), "final answer text </think>");
}

#[test]
fn strip_think_blocks_extracts_after_paired_end_tag() {
    let content = "<think>thinking</think>  the answer  ";
    assert_eq!(strip_think_blocks(content), "the answer");
}

#[test]
fn strip_think_blocks_preserves_content_after_unclosed_open_tag() {
    // D10: an unclosed marker after the answer has begun is literal text —
    // kept verbatim (it used to lose the marker), never a truncation point.
    let content = "prefix <think> actual answer";
    assert_eq!(strip_think_blocks(content), "prefix <think> actual answer");
}

#[test]
fn strip_think_blocks_preserves_text_before_first_block() {
    let content = "leading answer <think>inner thought</think> trailing answer";
    assert_eq!(
        strip_think_blocks(content),
        "leading answer  trailing answer"
    );
}

#[test]
fn strip_think_blocks_removes_multiple_blocks() {
    let content = "a <think>t1</think> b <think>t2</think> c";
    assert_eq!(strip_think_blocks(content), "a  b  c");
}

#[test]
fn strip_think_blocks_handles_unclosed_trailing_block() {
    // D10: unclosed mid-text marker is literal text, kept verbatim.
    let content = "answer <think> still answer";
    assert_eq!(strip_think_blocks(content), "answer <think> still answer");
}

/// Verbatim final answer of runs/long_review turn_0117 (0.8.2 validation,
/// qwen38-flash-next): 9,284 chars, 12 path:line citations, and literal
/// `<|channel>` / `<|channel>thought<channel|>` quoted in backticks.
const LONG_REVIEW_TURN_0117: &str = include_str!("fixtures/long_review_turn_0117_answer.md");

#[test]
fn d10_turn_0117_final_answer_round_trips_to_full_length() {
    let answer = LONG_REVIEW_TURN_0117.trim();
    assert_eq!(answer.chars().count(), 9284);
    let stripped = strip_think_blocks(LONG_REVIEW_TURN_0117);
    // 0.8.2 delivered 731 chars, cut at the first quoted `<|channel>`.
    assert_eq!(stripped.chars().count(), 9284);
    assert_eq!(stripped, answer);
    assert!(stripped.contains("`<|channel>thought<channel|>`"));
    assert!(stripped.contains("Final Audit Report"));
}

#[test]
fn d10_genuine_leading_reasoning_block_is_still_removed() {
    let answer = LONG_REVIEW_TURN_0117.trim();
    let think = format!("<think>\nplan the report\n</think>\n\n{answer}");
    assert_eq!(strip_think_blocks(&think), answer);
    let gemma = format!("<|channel>thought\nplan the report<channel|>{answer}");
    assert_eq!(strip_think_blocks(&gemma), answer);
    let both = format!("  <|channel>thought\nx<channel|>\n<think>y</think>\n{answer}");
    assert_eq!(strip_think_blocks(&both), answer);
}

#[test]
fn d10_leading_unclosed_gemma_block_is_all_reasoning() {
    // The model opened reasoning and never closed it: no answer was produced.
    assert_eq!(strip_think_blocks("<|channel>thought\nstill thinking"), "");
}

#[test]
fn d10_quoted_markers_survive_in_code_spans_and_fences() {
    let content = "Bug: an unclosed `<think>` or `<|channel>` drops the rest.\n\n```text\n<|channel>thought<channel|>\n<think>x</think>\n```\nTrailing text survives.";
    assert_eq!(strip_think_blocks(content), content);
}

#[test]
fn d10_unclosed_gemma_marker_mid_text_never_truncates() {
    let content = "The answer starts here. <|channel> is quoted without backticks, and everything after it stays.";
    assert_eq!(strip_think_blocks(content), content);
}

#[test]
fn strip_think_blocks_leaves_plain_text_unchanged() {
    let content = "just a plain answer, no tags";
    assert_eq!(strip_think_blocks(content), content);
}

#[tokio::test]
async fn error_recovery_hint_truncates_multibyte_error_without_panic() {
    use crate::agent::Agent;
    use crate::config::Config;

    // 199 ASCII bytes, then a 3-byte box-drawing char straddling byte 200:
    // the old `&error[..error.len().min(200)]` byte slice panicked here when
    // WARN logging was enabled.
    let mut err = "x".repeat(199);
    err.push('─'); // U+2500, occupies bytes 199..202
    err.push_str(" connection refused tail");

    // Char-boundary-safe truncation stops before the multibyte char.
    assert_eq!(
        crate::agent::interactive::safe_truncate(&err, 200).len(),
        199
    );

    let agent = Agent::new(Config::default()).await.unwrap();
    // Force the warn! arguments to be evaluated (no subscriber = lazily
    // skipped, which would hide the panic).
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_writer(std::io::sink)
        .finish();
    let hint = tracing::subscriber::with_default(subscriber, || {
        agent.build_error_recovery_hint("shell_exec", &err)
    });
    assert!(hint.contains("Endpoint/connection issue"));
}

#[test]
fn looks_like_malformed_tool_xml_detects_unparsed_delimiters() {
    assert!(looks_like_malformed_tool_xml("<|open|>tools<|sep|>broken"));
    assert!(looks_like_malformed_tool_xml(
        "<|open|>call tool=\"shell_exec\""
    ));
    assert!(looks_like_malformed_tool_xml(
        "<tool_call>{\"name\": \"git_diff\"}"
    ));
    assert!(looks_like_malformed_tool_xml("</tool_call>"));
    assert!(looks_like_malformed_tool_xml("<function=run_command>"));
    assert!(!looks_like_malformed_tool_xml(
        "Just normal plain text summary."
    ));
    // Valid standard XML format
    let valid_xml = "<tool>\n<name>git_diff</name>\n<arguments>{}</arguments>\n</tool>";
    assert!(!looks_like_malformed_tool_xml(valid_xml));
}

// ── W7d: no-action nudge after a FILES:-guard discard ────────────────
//
// The discard directive tells the model a write is pending; the follow-up
// no-action nudge must name that accepted path, never a read-only-only tool
// list (the contradiction burned a greenfield run: the write was thrown away
// and the model was then told to call directory_tree/grep).

/// Push a user message carrying the same marker the guard's discard branch
/// embeds, simulating "a write was just discarded and not yet re-issued".
fn push_discard_marker(agent: &mut crate::agent::Agent) {
    agent
        .messages
        .push(crate::api::types::Message::user(format!(
            "Your edit was NOT applied and has been discarded — no FILES: checklist ... {}",
            crate::agent::execution::FILES_GUARD_DISCARD_MARKER
        )));
}

#[tokio::test]
async fn reissue_pending_detected_from_marker_and_cleared_by_write() {
    let mut agent = crate::agent::Agent::new(crate::config::Config::default())
        .await
        .unwrap();

    assert!(
        !agent.files_guard_reissue_pending(),
        "no marker → nothing pending"
    );
    push_discard_marker(&mut agent);
    assert!(
        agent.files_guard_reissue_pending(),
        "marker in the message tail → re-issue pending"
    );

    // A successful write resolves the re-issue, even with the marker present.
    agent.has_written_any_file = true;
    assert!(
        !agent.files_guard_reissue_pending(),
        "a landed write clears the pending re-issue"
    );
}

#[tokio::test]
async fn reissue_pending_scrolls_out_of_the_bounded_window() {
    let mut agent = crate::agent::Agent::new(crate::config::Config::default())
        .await
        .unwrap();
    push_discard_marker(&mut agent);
    // Push enough turns that the marker leaves the bounded scan window (6).
    for i in 0..6 {
        agent
            .messages
            .push(crate::api::types::Message::user(format!("turn {i}")));
    }
    assert!(
        !agent.files_guard_reissue_pending(),
        "an old, unresolved discard eventually stops driving the nudge"
    );
}

#[tokio::test]
async fn nudge_after_discard_with_checklist_names_the_write_not_readonly_tools() {
    let mut agent = crate::agent::Agent::new(crate::config::Config::default())
        .await
        .unwrap();
    push_discard_marker(&mut agent);
    // Model complied with step 1: declared FILES: (checklist now recorded).
    agent.files_checklist_seen = true;

    let nudge = agent.build_no_action_prompt_message();
    assert!(
        nudge.contains("RE-ISSUE") && (nudge.contains("file_edit") || nudge.contains("file_write")),
        "checklist-recorded nudge must name the write as the accepted action: {nudge}"
    );
    assert!(
        !nudge.contains("directory_tree"),
        "must never answer a discarded write with a read-only-only tool list: {nudge}"
    );
}

#[tokio::test]
async fn nudge_after_discard_without_checklist_demands_both_in_one_response() {
    let mut agent = crate::agent::Agent::new(crate::config::Config::default())
        .await
        .unwrap();
    push_discard_marker(&mut agent);

    let nudge = agent.build_no_action_prompt_message();
    assert!(
        nudge.contains("FILES: <path>"),
        "must ask for the FILES: line: {nudge}"
    );
    assert!(
        nudge.contains("ONE response")
            && (nudge.contains("file_edit") || nudge.contains("file_write")),
        "must demand FILES: + write together in one response: {nudge}"
    );
    assert!(
        !nudge.contains("directory_tree"),
        "must never answer a discarded write with a read-only-only tool list: {nudge}"
    );
}

#[tokio::test]
async fn nudge_without_discard_keeps_generic_discovery_list() {
    let agent = crate::agent::Agent::new(crate::config::Config::default())
        .await
        .unwrap();
    let nudge = agent.build_no_action_prompt_message();
    assert!(
        nudge.contains("directory_tree"),
        "the ordinary no-action nudge is unchanged: {nudge}"
    );
}

#[test]
fn test_looks_like_malformed_tool_xml() {
    // Valid tool XML is not malformed
    let valid = "<tool>\n<name>file_read</name>\n<arguments>{\"path\": \"src/main.rs\"}</arguments>\n</tool>";
    assert!(!looks_like_malformed_tool_xml(valid));

    // Invalid JSON in arguments is malformed
    let invalid_json =
        "<tool>\n<name>file_read</name>\n<arguments>{invalid json}</arguments>\n</tool>";
    assert!(looks_like_malformed_tool_xml(invalid_json));

    // Missing closing tag is malformed
    let unclosed = "<tool>\n<name>file_read</name>\n<arguments>{\"path\": \"x\"}";
    assert!(looks_like_malformed_tool_xml(unclosed));

    // Empty name is malformed
    let empty_name = "<tool>\n<name></name>\n<arguments>{}</arguments>\n</tool>";
    assert!(looks_like_malformed_tool_xml(empty_name));
}

#[test]
fn empty_response_loop_message_distinguishes_reasoning_only_from_empty() {
    let empty = empty_response_loop_message(2, 0);
    assert!(empty.starts_with("EMPTY_RESPONSE_LOOP: 2 consecutive empty assistant responses"));
    assert!(
        empty.contains("no content, no reasoning and no tool calls"),
        "{empty}"
    );

    let reasoning = empty_response_loop_message(2, 812);
    assert!(reasoning.starts_with("EMPTY_RESPONSE_LOOP: 2 consecutive empty assistant responses"));
    assert!(reasoning.contains("reasoning-only"), "{reasoning}");
    assert!(reasoning.contains("812 reasoning chars"), "{reasoning}");
    assert!(
        !reasoning.contains("no reasoning"),
        "must not claim no reasoning when reasoning arrived: {reasoning}"
    );
}

#[test]
fn empty_response_nudge_matches_what_the_response_was() {
    assert!(empty_response_nudge(10).contains("(reasoning only)"));
    let empty = empty_response_nudge(0);
    assert!(!empty.contains("reasoning only"), "{empty}");
    assert!(
        empty.contains("no content, no reasoning, no tool calls"),
        "{empty}"
    );
}
