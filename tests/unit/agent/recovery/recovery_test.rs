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
