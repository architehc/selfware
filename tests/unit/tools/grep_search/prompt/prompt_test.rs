use super::*;

#[test]
fn test_grep_search_prompt_description() {
    let prompt = GrepSearchPrompt::new();
    let desc = prompt.description();
    assert!(desc.contains("regex"));
    assert!(desc.contains("context"));
}

#[test]
fn test_grep_search_prompt_when_to_use() {
    let prompt = GrepSearchPrompt::new();
    let when = prompt.when_to_use();
    assert!(when.contains("function"));
    assert!(when.contains("glob_find"));
    assert!(when.contains("symbol_search"));
}

#[test]
fn test_grep_search_prompt_examples() {
    let prompt = GrepSearchPrompt::new();
    let examples = prompt.examples();
    assert_eq!(examples.len(), 5);

    // Check TODO example
    let todo_example = &examples[3];
    assert!(todo_example.description.contains("TODO"));
    assert!(todo_example
        .input
        .get("pattern")
        .unwrap()
        .as_str()
        .unwrap()
        .contains("TODO"));
}

#[test]
fn test_grep_search_prompt_notes() {
    let prompt = GrepSearchPrompt::new();
    let notes = prompt.important_notes();
    assert!(notes.is_some());
    let notes_str = notes.unwrap();
    assert!(notes_str.contains("1000"));
    assert!(notes_str.contains("regex"));
}
