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
    let content = "prefix <think> actual answer";
    assert_eq!(strip_think_blocks(content), "prefix  actual answer");
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
    let content = "answer <think> still answer";
    assert_eq!(strip_think_blocks(content), "answer  still answer");
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
