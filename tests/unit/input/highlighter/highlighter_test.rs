use super::*;

#[test]
fn test_highlighter_creation() {
    let h = SelfwareHighlighter::new();
    assert!(h.is_command("/help"));
    assert!(!h.is_command("help"));
}

#[test]
fn test_highlighter_default() {
    let h = SelfwareHighlighter::default();
    assert!(h.is_command("/status"));
}

#[test]
fn test_all_commands_recognized() {
    let h = SelfwareHighlighter::new();
    assert!(h.is_command("/help"));
    assert!(h.is_command("/status"));
    assert!(h.is_command("/stats"));
    assert!(h.is_command("/mode"));
    assert!(h.is_command("/ctx"));
    assert!(h.is_command("/context"));
    assert!(h.is_command("/compress"));
    assert!(h.is_command("/memory"));
    assert!(h.is_command("/clear"));
    assert!(h.is_command("/tools"));
    assert!(h.is_command("/analyze"));
    assert!(h.is_command("/review"));
    assert!(h.is_command("/plan"));
    assert!(h.is_command("/diff"));
    assert!(h.is_command("/git"));
    assert!(h.is_command("/undo"));
    assert!(h.is_command("/cost"));
    assert!(h.is_command("/model"));
    assert!(h.is_command("/compact"));
    assert!(h.is_command("/verbose"));
    assert!(h.is_command("/config"));
    assert!(h.is_command("/garden"));
    assert!(h.is_command("/journal"));
    assert!(h.is_command("/palette"));
}

#[test]
fn test_invalid_commands() {
    let h = SelfwareHighlighter::new();
    assert!(!h.is_command("/unknown"));
    assert!(!h.is_command("help"));
    assert!(!h.is_command(""));
    assert!(!h.is_command("//help"));
}

#[test]
fn test_path_detection() {
    let h = SelfwareHighlighter::new();
    assert!(h.is_path("./src/main.rs"));
    assert!(h.is_path("config.toml"));
    assert!(h.is_path("~/projects"));
    assert!(!h.is_path("hello"));
}

#[test]
fn test_path_detection_extensions() {
    let h = SelfwareHighlighter::new();
    assert!(h.is_path("main.rs"));
    assert!(h.is_path("script.py"));
    assert!(h.is_path("app.js"));
    assert!(h.is_path("component.ts"));
    assert!(h.is_path("config.toml"));
    assert!(h.is_path("data.json"));
    assert!(h.is_path("README.md"));
}

#[test]
fn test_path_detection_prefixes() {
    let h = SelfwareHighlighter::new();
    assert!(h.is_path("./file"));
    assert!(h.is_path("../parent"));
    assert!(h.is_path("~/home"));
    assert!(h.is_path("/absolute/path"));
}

#[test]
fn test_keyword_detection() {
    let h = SelfwareHighlighter::new();
    assert!(h.is_keyword("exit"));
    assert!(h.is_keyword("quit"));
    assert!(h.is_keyword("help"));
    assert!(h.is_keyword("yes"));
    assert!(h.is_keyword("no"));
    assert!(h.is_keyword("true"));
    assert!(h.is_keyword("false"));
    // Case insensitive
    assert!(h.is_keyword("EXIT"));
    assert!(h.is_keyword("True"));
}

#[test]
fn test_string_finding() {
    let h = SelfwareHighlighter::new();
    let strings = h.find_strings(r#"hello "world" and 'test'"#);
    assert_eq!(strings.len(), 2);
}

#[test]
fn test_string_finding_double_quotes() {
    let h = SelfwareHighlighter::new();
    let strings = h.find_strings(r#""hello world""#);
    assert_eq!(strings.len(), 1);
    assert_eq!(strings[0], (0, 13));
}

#[test]
fn test_string_finding_single_quotes() {
    let h = SelfwareHighlighter::new();
    let strings = h.find_strings("'hello world'");
    assert_eq!(strings.len(), 1);
    assert_eq!(strings[0], (0, 13));
}

#[test]
fn test_string_finding_unclosed() {
    let h = SelfwareHighlighter::new();
    let line = r#"hello "unclosed"#;
    let strings = h.find_strings(line);
    assert_eq!(strings.len(), 1);
    // Should extend to end of line
    assert_eq!(strings[0], (6, line.len()));
}

#[test]
fn test_string_finding_empty() {
    let h = SelfwareHighlighter::new();
    let strings = h.find_strings("no strings here");
    assert!(strings.is_empty());
}

#[test]
fn test_string_finding_adjacent() {
    let h = SelfwareHighlighter::new();
    let strings = h.find_strings(r#""first""second""#);
    assert_eq!(strings.len(), 2);
}

#[test]
fn test_in_string() {
    let h = SelfwareHighlighter::new();
    let strings = vec![(5, 10), (15, 20)];

    assert!(!h.in_string(0, &strings));
    assert!(!h.in_string(4, &strings));
    assert!(h.in_string(5, &strings));
    assert!(h.in_string(7, &strings));
    assert!(h.in_string(9, &strings));
    assert!(!h.in_string(10, &strings));
    assert!(!h.in_string(14, &strings));
    assert!(h.in_string(15, &strings));
}

#[test]
fn test_highlight_command() {
    let h = SelfwareHighlighter::new();
    let styled = h.highlight("/help", 0);
    assert!(!styled.buffer.is_empty());
}

#[test]
fn test_highlight_command_with_path() {
    let h = SelfwareHighlighter::new();
    let styled = h.highlight("/analyze ./src", 0);
    assert!(!styled.buffer.is_empty());
    // Should have at least 2 parts (command and path)
    assert!(styled.buffer.len() >= 2);
}

#[test]
fn test_highlight_exit() {
    let h = SelfwareHighlighter::new();
    let styled = h.highlight("exit", 0);
    assert!(!styled.buffer.is_empty());
}

#[test]
fn test_highlight_quit() {
    let h = SelfwareHighlighter::new();
    let styled = h.highlight("quit", 0);
    assert!(!styled.buffer.is_empty());
}

#[test]
fn test_highlight_empty() {
    let h = SelfwareHighlighter::new();
    let styled = h.highlight("", 0);
    assert!(styled.buffer.is_empty());
}

#[test]
fn test_highlight_with_string() {
    let h = SelfwareHighlighter::new();
    let styled = h.highlight(r#"echo "hello world""#, 0);
    assert!(!styled.buffer.is_empty());
}

#[test]
fn test_highlight_plain_text() {
    let h = SelfwareHighlighter::new();
    let styled = h.highlight("hello world", 0);
    assert!(!styled.buffer.is_empty());
    assert_eq!(styled.buffer.len(), 1);
}
