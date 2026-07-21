use super::*;

#[test]
fn test_shell_exec_prompt_description() {
    let prompt = ShellExecPrompt::new();
    let desc = prompt.description();
    assert!(desc.contains("Execute shell"));
    assert!(desc.contains("builds"));
    assert!(desc.contains("timeout"));
}

#[test]
fn test_shell_exec_prompt_when_to_use() {
    let prompt = ShellExecPrompt::new();
    let when = prompt.when_to_use();
    assert!(when.contains("shell_exec"));
    assert!(when.contains("file_read"));
    assert!(when.contains("grep_search"));
}

#[test]
fn test_shell_exec_prompt_examples() {
    let prompt = ShellExecPrompt::new();
    let examples = prompt.examples();
    assert_eq!(examples.len(), 4);

    // Check build example
    assert!(examples[1].description.contains("Build"));
    assert!(examples[1]
        .input
        .get("command")
        .unwrap()
        .as_str()
        .unwrap()
        .contains("cargo build"));
}

#[test]
fn test_shell_exec_prompt_notes() {
    let prompt = ShellExecPrompt::new();
    let notes = prompt.important_notes();
    assert!(notes.is_some());
    let notes_str = notes.unwrap();
    assert!(notes_str.contains("timeout"));
    assert!(notes_str.contains("10,000"));
}
