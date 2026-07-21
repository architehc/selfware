use super::*;

#[test]
fn test_classify_word_boundary_no_substring_false_positives() {
    // "ship" inside "relationship" must not route to Ship (CLS-SHIP-SUBSTRING).
    assert_eq!(
        classify_task("Fix the relationship manager bug"),
        TaskType::Edit
    );
    // "page"/"css" as substrings of identifiers must not route to Visual.
    assert_eq!(
        classify_task("Refactor the pagerank scorer in ranker.rs"),
        TaskType::Refactor
    );
    // "read" inside "thread", "list" inside identifiers, "count" inside
    // "account" must not route to Read (found by GLM-5.2 reviewing task_focus).
    assert_eq!(classify_task("Update the thread pool size"), TaskType::Edit);
    assert_eq!(
        classify_task("Fix the account balance calculation"),
        TaskType::Edit
    );
    // Genuine whole-word matches still classify as before.
    assert_eq!(classify_task("Ship the v2 release"), TaskType::Ship);
    assert_eq!(
        classify_task("The login page is broken, fix it"),
        TaskType::Visual
    );
    assert_eq!(
        classify_task("Read the config module and explain it"),
        TaskType::Read
    );
}

#[test]
fn test_classify_do_not_edit_is_read_only() {
    // Regression: a prohibition on editing must classify as Read, not Edit
    // (the prompt contains the substring "edit").
    assert_eq!(
        classify_task(
            "In one sentence, which module is registered as the shell tool? Do NOT edit any files."
        ),
        TaskType::Read
    );
    assert_eq!(
        classify_task("Review src/ for dead code without modifying anything"),
        TaskType::Read
    );
    // But a standalone mutation instruction beats the read-only override.
    assert_eq!(
        classify_task("Fix the bug in parser.rs, but do not edit the tests."),
        TaskType::Edit
    );
}

#[test]
fn test_classify_review_intent_is_read_not_test_or_edit() {
    // Regression: "specific" contains "spec" and the file is named
    // verification.rs — previously mis-typed as Testing, which handed the
    // model a write-code/run-tests workflow it could never satisfy on a
    // read-only review (livelock). Review intent must win → Read.
    assert_eq!(
            classify_task(
                "Review the file src/agent/verification.rs for any real bugs with specific line references"
            ),
            TaskType::Read
        );
    assert_eq!(
        classify_task("Audit the auth module for issues"),
        TaskType::Read
    );
    assert_eq!(
        classify_task("Inspect config.rs and report code smells"),
        TaskType::Read
    );
    // "specific" alone must not pull a non-review task into Testing.
    assert_ne!(
        classify_task("Explain the specific ownership rules in mod.rs"),
        TaskType::Test
    );
    // A genuine test-writing task is still Test (review override needs the
    // review verb, which this lacks).
    assert_eq!(
        classify_task("Write tests for the tier allocator"),
        TaskType::Test
    );
}

#[test]
fn test_classify_read_tasks() {
    assert_eq!(
        classify_task("Read src/main.rs and tell me what it does"),
        TaskType::Read
    );
    assert_eq!(
        classify_task("Explain how the config loader works"),
        TaskType::Read
    );
    assert_eq!(
        classify_task("How does the tier allocator work?"),
        TaskType::Read
    );
    assert_eq!(
        classify_task("Show me the public API of agent/mod.rs"),
        TaskType::Read
    );
    assert_eq!(
        classify_task("Count the lines in tier_allocator.rs"),
        TaskType::Read
    );
    assert_eq!(
        classify_task("Summarize the safety checker"),
        TaskType::Read
    );
    assert_eq!(
        classify_task("List all public functions in api/client.rs"),
        TaskType::Read
    );
    // Diagnostic/troubleshooting queries
    assert_eq!(
        classify_task("Why does this crash when I call allocate_tiers?"),
        TaskType::Read
    );
    assert_eq!(
        classify_task("Debug the config loading issue"),
        TaskType::Read
    );
    assert_eq!(classify_task("Diagnose the test failure"), TaskType::Read);
}

#[test]
fn test_classify_edit_tasks() {
    assert_eq!(
        classify_task("Fix the bug in config/loader.rs"),
        TaskType::Edit
    );
    assert_eq!(
        classify_task("Implement retry logic for the API client"),
        TaskType::Edit
    );
    assert_eq!(
        classify_task("Add a timeout parameter to the function"),
        TaskType::Edit
    );
    assert_eq!(
        classify_task("Update the error message for invalid config"),
        TaskType::Edit
    );
    assert_eq!(
        classify_task("Remove the deprecated function"),
        TaskType::Edit
    );
}

#[test]
fn test_classify_test_tasks() {
    assert_eq!(
        classify_task("Write tests for the tier allocator"),
        TaskType::Test
    );
    assert_eq!(
        classify_task("Add coverage for the safety checker"),
        TaskType::Test
    );
    assert_eq!(
        classify_task("Create regression tests for the config loader"),
        TaskType::Test
    );
}

#[test]
fn test_classify_test_with_diagnostic_words() {
    // Mixed-intent: test-writing with diagnostic context should stay Test
    assert_eq!(
        classify_task("write regression tests showing why the parser fails"),
        TaskType::Test
    );
    assert_eq!(
        classify_task("add tests to debug the allocation logic"),
        TaskType::Test
    );
    assert_eq!(
        classify_task("create a test that shows why X crashes"),
        TaskType::Test
    );
}

#[test]
fn test_classify_fix_broken_test_is_edit() {
    assert_eq!(
        classify_task("Fix the broken test in test_git.rs"),
        TaskType::Edit
    );
    assert_eq!(
        classify_task("Repair the failing test suite"),
        TaskType::Edit
    );
}

#[test]
fn test_classify_refactor_tasks() {
    assert_eq!(
        classify_task("Refactor the config module into submodules"),
        TaskType::Refactor
    );
    assert_eq!(
        classify_task("Split cli.rs into smaller files"),
        TaskType::Refactor
    );
    assert_eq!(
        classify_task("Extract the init wizard into its own module"),
        TaskType::Refactor
    );
    assert_eq!(
        classify_task("Rename the function to be more descriptive"),
        TaskType::Refactor
    );
}

#[test]
fn test_classify_ship_tasks() {
    assert_eq!(classify_task("Commit and push to main"), TaskType::Ship);
    assert_eq!(classify_task("Deploy the latest changes"), TaskType::Ship);
    assert_eq!(classify_task("Ship the release"), TaskType::Ship);
    assert_eq!(classify_task("Merge to main and deploy"), TaskType::Ship);
}

#[test]
fn test_classify_visual_tasks() {
    assert_eq!(
        classify_task("Check how the website looks"),
        TaskType::Visual
    );
    assert_eq!(
        classify_task("Take a screenshot of the page"),
        TaskType::Visual
    );
    assert_eq!(classify_task("Fix the CSS layout issue"), TaskType::Visual);
    assert_eq!(
        classify_task("Open the browser and navigate to the dashboard"),
        TaskType::Visual
    );
}

#[test]
fn test_classify_general_tasks() {
    assert_eq!(classify_task("Do something"), TaskType::General);
    assert_eq!(classify_task("Help me with this"), TaskType::General);
    assert_eq!(classify_task(""), TaskType::General);
}

#[test]
fn test_primary_tools_not_empty() {
    for task_type in [
        TaskType::Read,
        TaskType::Edit,
        TaskType::Test,
        TaskType::Refactor,
        TaskType::Ship,
        TaskType::Visual,
        TaskType::General,
    ] {
        assert!(
            !task_type.primary_tools().is_empty(),
            "{} has no primary tools",
            task_type
        );
    }
}

#[test]
fn test_preamble_not_empty() {
    for task_type in [
        TaskType::Read,
        TaskType::Edit,
        TaskType::Test,
        TaskType::Refactor,
        TaskType::Ship,
        TaskType::Visual,
        TaskType::General,
    ] {
        assert!(
            !task_type.preamble().is_empty(),
            "{} has no preamble",
            task_type
        );
    }
}

#[test]
fn test_preamble_contains_start() {
    // Every preamble should tell the model what to START with
    for task_type in [
        TaskType::Read,
        TaskType::Edit,
        TaskType::Test,
        TaskType::Refactor,
        TaskType::Ship,
        TaskType::Visual,
        TaskType::General,
    ] {
        assert!(
            task_type.preamble().contains("START"),
            "{} preamble missing START directive",
            task_type
        );
    }
}

#[test]
fn test_reorder_puts_primary_first() {
    use crate::api::types::{FunctionDefinition, ToolDefinition};

    fn make_tool(name: &str) -> ToolDefinition {
        ToolDefinition {
            def_type: "function".to_string(),
            function: FunctionDefinition {
                name: name.to_string(),
                description: format!("Tool {}", name),
                parameters: serde_json::json!({}),
            },
        }
    }

    let tools = vec![
        make_tool("git_status"),
        make_tool("process_list"),
        make_tool("file_read"),
        make_tool("grep_search"),
        make_tool("cargo_check"),
        make_tool("file_edit"),
        make_tool("shell_exec"),
    ];

    let reordered = reorder_tools(tools, TaskType::Edit);
    let names: Vec<&str> = reordered.iter().map(|t| t.function.name.as_str()).collect();

    // Primary tools for Edit: file_read, file_edit, grep_search, cargo_check
    assert_eq!(names[0], "file_read", "first tool should be file_read");
    assert_eq!(names[1], "file_edit", "second tool should be file_edit");
    assert_eq!(names[2], "grep_search", "third tool should be grep_search");
    assert_eq!(names[3], "cargo_check", "fourth tool should be cargo_check");
}

#[test]
fn test_reorder_preserves_all_tools() {
    use crate::api::types::{FunctionDefinition, ToolDefinition};

    fn make_tool(name: &str) -> ToolDefinition {
        ToolDefinition {
            def_type: "function".to_string(),
            function: FunctionDefinition {
                name: name.to_string(),
                description: String::new(),
                parameters: serde_json::json!({}),
            },
        }
    }

    let tools = vec![
        make_tool("a"),
        make_tool("b"),
        make_tool("file_read"),
        make_tool("c"),
    ];
    let reordered = reorder_tools(tools, TaskType::Read);
    assert_eq!(reordered.len(), 4, "reorder should not drop tools");
    assert_eq!(reordered[0].function.name, "file_read", "primary first");
}

#[test]
fn test_display() {
    assert_eq!(format!("{}", TaskType::Read), "read");
    assert_eq!(format!("{}", TaskType::Edit), "edit");
    assert_eq!(format!("{}", TaskType::Ship), "ship");
}

#[test]
fn test_classify_task_detects_explicit_vision_tool() {
    assert_eq!(
        classify_task("Use vision_analyze on ./sample.jpg and answer directly."),
        TaskType::Visual
    );
}

#[test]
fn test_classify_task_detects_image_path() {
    assert_eq!(
        classify_task("Describe /tmp/camera_frame.png in one sentence."),
        TaskType::Visual
    );
}
