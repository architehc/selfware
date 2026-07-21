use super::*;

#[test]
fn test_completer_creation() {
    let completer = SelfwareCompleter::new(
        vec!["file_read".into(), "git_status".into()],
        vec!["/help".into(), "/status".into()],
    );
    assert_eq!(completer.tool_names.len(), 2);
    assert_eq!(completer.commands.len(), 2);
}

#[test]
fn test_command_completions() {
    let completer = SelfwareCompleter::new(
        vec![],
        vec!["/help".into(), "/status".into(), "/memory".into()],
    );

    let suggestions = completer.complete_commands("/he");
    assert!(!suggestions.is_empty());
    assert!(suggestions.iter().any(|s| s.value == "/help"));
}

#[test]
fn test_command_completions_all() {
    let completer = SelfwareCompleter::new(
        vec![],
        vec![
            "/help".into(),
            "/status".into(),
            "/memory".into(),
            "/clear".into(),
        ],
    );

    // Empty prefix should return all commands
    let suggestions = completer.complete_commands("/");
    assert_eq!(suggestions.len(), 4);
}

#[test]
fn test_fuzzy_matching() {
    let completer = SelfwareCompleter::new(
        vec!["file_read".into(), "file_write".into(), "git_status".into()],
        vec![],
    );

    // "fr" should match "file_read" (fuzzy)
    let suggestions = completer.complete_tools("fr");
    assert!(!suggestions.is_empty());
}

#[test]
fn test_fuzzy_matching_order() {
    let completer = SelfwareCompleter::new(
        vec!["file_read".into(), "file_write".into(), "git_status".into()],
        vec![],
    );

    // "file" should have file_read and file_write at top
    let suggestions = completer.complete_tools("file");
    assert!(suggestions.len() >= 2);
    assert!(suggestions[0].value.starts_with("file"));
}

#[test]
fn test_tool_completions_empty() {
    let completer = SelfwareCompleter::new(vec![], vec![]);
    let suggestions = completer.complete_tools("anything");
    assert!(suggestions.is_empty());
}

#[test]
fn test_command_description() {
    let completer = SelfwareCompleter::new(vec![], vec![]);

    assert_eq!(
        completer.command_description("/help"),
        "Show help and available commands"
    );
    assert_eq!(
        completer.command_description("/status"),
        "Show agent status and context usage"
    );
    assert_eq!(
        completer.command_description("/memory"),
        "Show memory hierarchy status"
    );
    assert_eq!(completer.command_description("/clear"), "Clear the screen");
    assert_eq!(
        completer.command_description("/tools"),
        "List available tools"
    );
    assert_eq!(
        completer.command_description("/analyze"),
        "Analyze the current codebase"
    );
    assert_eq!(
        completer.command_description("/review"),
        "Review recent changes"
    );
    assert_eq!(
        completer.command_description("/swarm"),
        "Launch multi-agent swarm"
    );
    assert_eq!(
        completer.command_description("/queue"),
        "Enqueue a message for later processing"
    );
    assert_eq!(completer.command_description("/unknown"), "Command");
}

#[test]
fn test_detect_context_command() {
    let completer = SelfwareCompleter::new(vec![], vec!["/help".into()]);

    // Starting with / should be command context
    let ctx = completer.detect_context("/hel", 4);
    assert!(matches!(ctx, CompletionContext::Command(prefix) if prefix == "/hel"));
}

#[test]
fn test_detect_context_path_after_analyze() {
    let completer = SelfwareCompleter::new(vec![], vec![]);

    // After /analyze, should be path context
    let ctx = completer.detect_context("/analyze ./src", 14);
    assert!(matches!(ctx, CompletionContext::Path(prefix) if prefix == "./src"));
}

#[test]
fn test_detect_context_path_after_review() {
    let completer = SelfwareCompleter::new(vec![], vec![]);

    let ctx = completer.detect_context("/review main.rs", 15);
    assert!(matches!(ctx, CompletionContext::Path(prefix) if prefix == "main.rs"));
}

#[test]
fn test_detect_context_tool() {
    let completer = SelfwareCompleter::new(vec!["file_read".into()], vec![]);

    // Alphanumeric with underscore should be tool context
    let ctx = completer.detect_context("file_re", 7);
    assert!(matches!(ctx, CompletionContext::Tool(prefix) if prefix == "file_re"));
}

#[test]
fn test_detect_context_none() {
    let completer = SelfwareCompleter::new(vec![], vec![]);

    // Empty should be None
    let ctx = completer.detect_context("", 0);
    assert!(matches!(ctx, CompletionContext::None));
}

#[test]
fn test_complete_interface() {
    let mut completer = SelfwareCompleter::new(vec!["file_read".into()], vec!["/help".into()]);

    // Test the Completer trait implementation
    let suggestions = completer.complete("/he", 3);
    assert!(!suggestions.is_empty());
}

#[test]
fn test_complete_empty_line() {
    let mut completer = SelfwareCompleter::new(vec![], vec!["/help".into(), "/status".into()]);

    // Empty line should show commands
    let suggestions = completer.complete("", 0);
    assert!(!suggestions.is_empty());
}

#[test]
fn test_suggestion_has_description() {
    let completer = SelfwareCompleter::new(vec![], vec!["/help".into()]);

    let suggestions = completer.complete_commands("/help");
    assert!(!suggestions.is_empty());
    assert!(suggestions[0].description.is_some());
}

#[test]
fn test_path_completions_current_dir() {
    let completer = SelfwareCompleter::new(vec![], vec![]);

    // Should be able to complete from current directory
    let suggestions = completer.complete_paths("./");
    // May or may not have results depending on current directory
    // Just verify it doesn't panic
    let _ = suggestions;
}

#[test]
fn test_path_completions_nonexistent() {
    let completer = SelfwareCompleter::new(vec![], vec![]);

    // Nonexistent directory should return empty
    let suggestions = completer.complete_paths("/nonexistent/path/here/");
    assert!(suggestions.is_empty());
}
