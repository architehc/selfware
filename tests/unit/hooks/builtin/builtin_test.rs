use super::*;

#[test]
fn test_builtin_hooks_are_valid() {
    let hooks = vec![
        format_on_save_rust(),
        lint_after_edit_rust(),
        test_on_stop_rust(),
        auto_commit(),
        format_on_save_python(),
        format_on_save_node(),
    ];

    for hook in &hooks {
        assert!(!hook.command.is_empty());
        assert!(hook.timeout_secs > 0);
    }

    // format_on_save should match file_write and file_edit
    let fmt = format_on_save_rust();
    assert!(fmt.match_tools.contains(&"file_write".to_string()));
    assert!(fmt.match_tools.contains(&"file_edit".to_string()));

    // test_on_stop should match all tools (empty list)
    let test = test_on_stop_rust();
    assert!(test.match_tools.is_empty());
}
