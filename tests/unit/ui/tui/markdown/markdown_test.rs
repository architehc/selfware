use super::*;

#[test]
fn test_markdown_renderer_creation() {
    let renderer = MarkdownRenderer::new();
    assert!(!renderer.syntax_set.syntaxes().is_empty());
}

#[test]
fn test_markdown_renderer_default() {
    let renderer = MarkdownRenderer::default();
    assert!(!renderer.syntax_set.syntaxes().is_empty());
}

#[test]
fn test_render_simple_text() {
    let renderer = MarkdownRenderer::new();
    let text = renderer.render("Hello, world!", 80);
    assert!(!text.lines.is_empty());
}

#[test]
fn test_render_heading() {
    let renderer = MarkdownRenderer::new();
    let text = renderer.render("# Heading 1", 80);
    assert!(!text.lines.is_empty());
}

#[test]
fn test_render_code_block() {
    let renderer = MarkdownRenderer::new();
    let lines = renderer.render_code_block("rust", "fn main() { }");
    assert!(!lines.is_empty());
    // Should have borders
    assert!(lines.len() >= 3);
}

#[test]
fn test_render_code_block_no_lang() {
    let renderer = MarkdownRenderer::new();
    let lines = renderer.render_code_block("", "plain text");
    assert!(!lines.is_empty());
}

#[test]
fn test_render_diff() {
    let renderer = MarkdownRenderer::new();
    let lines = renderer.render_diff("old line\n", "new line\n");
    assert!(!lines.is_empty());
}

#[test]
fn test_render_diff_same() {
    let renderer = MarkdownRenderer::new();
    let lines = renderer.render_diff("same", "same");
    assert!(!lines.is_empty());
}

#[test]
fn test_render_tool_card() {
    let renderer = MarkdownRenderer::new();
    let lines = renderer.render_tool_card("file_read", r#"{"path": "main.rs"}"#, None);
    assert!(!lines.is_empty());
}

#[test]
fn test_render_tool_card_with_result() {
    let renderer = MarkdownRenderer::new();
    let lines = renderer.render_tool_card(
        "file_read",
        r#"{"path": "main.rs"}"#,
        Some("File read successfully"),
    );
    assert!(!lines.is_empty());
}

#[test]
fn test_render_tool_card_with_error() {
    let renderer = MarkdownRenderer::new();
    let lines = renderer.render_tool_card(
        "file_read",
        r#"{"path": "missing.rs"}"#,
        Some("error: file not found"),
    );
    assert!(!lines.is_empty());
}

#[test]
fn test_render_thinking() {
    let renderer = MarkdownRenderer::new();
    let lines = renderer.render_thinking("Analyzing the problem...", 5);
    assert!(!lines.is_empty());
}

#[test]
fn test_render_thinking_long() {
    let renderer = MarkdownRenderer::new();
    let content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8";
    let lines = renderer.render_thinking(content, 10);
    assert!(!lines.is_empty());
}

#[test]
fn test_truncate_str_short() {
    assert_eq!(truncate_str("hello", 10), "hello");
}

#[test]
fn test_truncate_str_long() {
    assert_eq!(truncate_str("hello world this is long", 10), "hello w...");
}

#[test]
fn test_render_list() {
    let renderer = MarkdownRenderer::new();
    let text = renderer.render("- Item 1\n- Item 2\n- Item 3", 80);
    assert!(!text.lines.is_empty());
}

#[test]
fn test_render_emphasis() {
    let renderer = MarkdownRenderer::new();
    let text = renderer.render("*italic* and **bold**", 80);
    assert!(!text.lines.is_empty());
}

#[test]
fn test_render_inline_code() {
    let renderer = MarkdownRenderer::new();
    let text = renderer.render("Use `cargo build` to compile", 80);
    assert!(!text.lines.is_empty());
}

#[test]
fn test_render_blockquote() {
    let renderer = MarkdownRenderer::new();
    let text = renderer.render("> This is a quote", 80);
    assert!(!text.lines.is_empty());
}

#[test]
fn test_render_horizontal_rule() {
    let renderer = MarkdownRenderer::new();
    let text = renderer.render("---", 80);
    assert!(!text.lines.is_empty());
}

#[test]
fn test_render_complex_markdown() {
    let renderer = MarkdownRenderer::new();
    let md = r#"
# Title

This is a paragraph with *emphasis* and **strong** text.

## Code Example

```rust
fn main() {
    println!("Hello");
}
```

- List item 1
- List item 2

> A quote

---
"#;
    let text = renderer.render(md, 80);
    assert!(!text.lines.is_empty());
}

#[test]
fn test_render_nested_list() {
    let renderer = MarkdownRenderer::new();
    let md = "- Item 1\n  - Nested 1\n  - Nested 2\n- Item 2";
    let text = renderer.render(md, 80);
    assert!(!text.lines.is_empty());
}

#[test]
fn test_render_multiple_code_blocks() {
    let renderer = MarkdownRenderer::new();
    let md = "```python\nprint('hello')\n```\n\n```javascript\nconsole.log('world');\n```";
    let text = renderer.render(md, 80);
    assert!(!text.lines.is_empty());
}

#[test]
fn test_render_state_flush() {
    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let theme = &ts.themes["base16-ocean.dark"];

    let mut state = RenderState::new(80, &ss, theme);
    state.current_line.push(Span::raw("test"));
    state.flush_line();
    assert!(state.current_line.is_empty());
    assert_eq!(state.lines.len(), 1);
}
