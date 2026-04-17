//! Task-focused tool prioritization.
//!
//! Given a task description, classifies it into a task type and returns
//! an ordered list of tool names: primary tools first (the ones the model
//! should reach for immediately), then secondary tools (available but not
//! the default action), then the rest.
//!
//! This doesn't remove tools — it reorders them and generates a preamble
//! that steers the model toward the right starting action.

/// Task type inferred from the task description.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskType {
    /// Read/analyze/explain code — start with file_read, grep_search
    Read,
    /// Fix a bug or implement a feature — start with file_read, then file_edit
    Edit,
    /// Write or run tests — start with cargo_test, file_read
    Test,
    /// Refactor/restructure code — start with file_read, grep_search, file_edit
    Refactor,
    /// Deploy, commit, push — start with git operations
    Ship,
    /// Browser/visual task — start with browser tools
    Visual,
    /// General/unknown — no reordering, standard preamble
    General,
}

impl TaskType {
    /// Primary tools for this task type — the model should use these first.
    pub fn primary_tools(&self) -> &'static [&'static str] {
        match self {
            TaskType::Read => &[
                "file_read",
                "grep_search",
                "directory_tree",
                "glob_find",
                "symbol_search",
            ],
            TaskType::Edit => &["file_read", "file_edit", "grep_search", "cargo_check"],
            TaskType::Test => &["cargo_test", "file_read", "file_edit", "cargo_check"],
            TaskType::Refactor => &[
                "file_read",
                "grep_search",
                "file_edit",
                "file_write",
                "cargo_check",
                "cargo_clippy",
            ],
            TaskType::Ship => &[
                "git_status",
                "git_diff",
                "git_commit",
                "git_push",
                "cargo_test",
                "cargo_check",
            ],
            TaskType::Visual => &[
                "browser_fetch",
                "screen_capture",
                "vision_analyze",
                "file_read",
                "file_edit",
            ],
            TaskType::General => &[
                "file_read",
                "grep_search",
                "file_edit",
                "directory_tree",
                "cargo_check",
            ],
        }
    }

    /// Secondary tools — available but not the default starting action.
    pub fn secondary_tools(&self) -> &'static [&'static str] {
        match self {
            TaskType::Read => &["file_edit", "cargo_check"],
            TaskType::Edit => &[
                "file_write",
                "cargo_test",
                "cargo_clippy",
                "directory_tree",
                "glob_find",
            ],
            TaskType::Test => &["grep_search", "cargo_clippy", "directory_tree"],
            TaskType::Refactor => &["cargo_test", "directory_tree", "symbol_search", "glob_find"],
            TaskType::Ship => &["file_read", "grep_search", "shell_exec"],
            TaskType::Visual => &[
                "browser_screenshot",
                "browser_eval",
                "shell_exec",
                "page_control",
            ],
            TaskType::General => &["file_write", "cargo_test", "shell_exec", "git_status"],
        }
    }

    /// Generate a preamble instruction that steers the model toward
    /// the right starting action for this task type.
    pub fn preamble(&self) -> &'static str {
        match self {
            TaskType::Read => {
                "\
TASK TYPE: Code reading/analysis.
START by using file_read on the target file(s). Use grep_search to find relevant code.
Do NOT call git_status, context_status, or process_list — go directly to the file."
            }

            TaskType::Edit => {
                "\
TASK TYPE: Code modification.
START by using file_read to understand the current code, then use file_edit to make changes.
After editing, use cargo_check to verify the code compiles.
Do NOT explore the repository first — go directly to the target file."
            }

            TaskType::Test => {
                "\
TASK TYPE: Testing.
START by reading the relevant source and test files with file_read.
Use cargo_test to run tests. Use file_edit to fix or add tests.
Do NOT explore the repository first — go directly to the test file."
            }

            TaskType::Refactor => {
                "\
TASK TYPE: Refactoring.
START by using file_read and grep_search to understand the current structure.
Use file_edit to make changes. Run cargo_check and cargo_clippy after each change.
Do NOT call git_status or process_list — focus on the code."
            }

            TaskType::Ship => {
                "\
TASK TYPE: Ship/deploy.
START by using git_status and git_diff to see what's changed.
Run cargo_test to verify, then git_commit and git_push."
            }

            TaskType::Visual => {
                "\
TASK TYPE: Visual/browser task.
START by using browser_fetch or screen_capture on the target.
Use vision_analyze for visual assessment."
            }

            TaskType::General => {
                "\
START by using file_read on the most relevant file for this task.
Use grep_search if you need to find where something is defined.
Do NOT call git_status, context_status, or process_list unless specifically asked."
            }
        }
    }
}

impl std::fmt::Display for TaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskType::Read => write!(f, "read"),
            TaskType::Edit => write!(f, "edit"),
            TaskType::Test => write!(f, "test"),
            TaskType::Refactor => write!(f, "refactor"),
            TaskType::Ship => write!(f, "ship"),
            TaskType::Visual => write!(f, "visual"),
            TaskType::General => write!(f, "general"),
        }
    }
}

/// Classify a task description into a task type.
pub fn classify_task(task: &str) -> TaskType {
    let t = task.to_lowercase();

    // Ship/deploy — check first because "commit" could appear in edit tasks
    if t.contains("commit") && (t.contains("push") || t.contains("ship") || t.contains("deploy"))
        || t.contains("deploy")
        || t.contains("release")
        || t.contains("merge to main")
        || t.contains("ship")
    {
        return TaskType::Ship;
    }

    // Visual/browser
    if t.contains("browser")
        || t.contains("screenshot")
        || t.contains("visual")
        || t.contains("vision_analyze")
        || t.contains("vision_compare")
        || t.contains("website")
        || t.contains("page")
        || t.contains("css")
        || t.contains("ui look")
        || t.contains(".png")
        || t.contains(".jpg")
        || t.contains(".jpeg")
        || t.contains(".webp")
        || t.contains(".gif")
    {
        return TaskType::Visual;
    }

    // Test
    if t.contains("test")
        || t.contains("coverage")
        || t.contains("regression")
        || t.contains("spec")
    {
        // Distinguish "write tests" (Test) from "fix the test" (Edit)
        if t.contains("fix") || t.contains("repair") || t.contains("broken") {
            return TaskType::Edit;
        }
        // If diagnostic words appear but primary intent is test-writing, keep Test.
        // Only redirect to Read when the prompt is purely investigative.
        if t.contains("diagnose") || t.contains("debug") || t.contains("why") {
            let has_test_writing_intent = t.contains("write")
                || t.contains("add")
                || t.contains("create")
                || t.contains("implement");
            if !has_test_writing_intent {
                return TaskType::Read;
            }
        }
        return TaskType::Test;
    }

    // Read/analyze/explain/diagnose — no mutation implied
    if t.contains("read")
        || t.contains("explain")
        || t.contains("describe")
        || t.contains("analyze")
        || t.contains("understand")
        || t.contains("how does")
        || t.contains("what is")
        || t.contains("show me")
        || t.contains("tell me")
        || t.contains("list")
        || t.contains("count")
        || t.contains("summarize")
        || t.contains("why")
        || t.contains("crash")
        || t.contains("diagnose")
        || t.contains("debug")
    {
        return TaskType::Read;
    }

    // Refactor — structural changes without new features
    if t.contains("refactor")
        || t.contains("restructure")
        || t.contains("split")
        || t.contains("extract")
        || t.contains("rename")
        || (t.contains("move") && !t.contains("remove"))
        || t.contains("consolidate")
    {
        return TaskType::Refactor;
    }

    // Edit — bug fix, feature, implementation
    if t.contains("fix")
        || t.contains("bug")
        || t.contains("implement")
        || t.contains("add")
        || t.contains("create")
        || t.contains("update")
        || t.contains("change")
        || t.contains("modify")
        || t.contains("edit")
        || t.contains("write")
        || t.contains("remove")
        || t.contains("delete")
    {
        return TaskType::Edit;
    }

    TaskType::General
}

/// Reorder tool definitions so primary tools appear first in the list.
///
/// Takes the full list of tool definitions and the classified task type,
/// returns a new list with primary tools first, secondary next, rest last.
/// Tools not in the registry are skipped (no panic).
pub fn reorder_tools(
    tools: Vec<crate::api::types::ToolDefinition>,
    task_type: TaskType,
) -> Vec<crate::api::types::ToolDefinition> {
    let primary = task_type.primary_tools();
    let secondary = task_type.secondary_tools();

    let mut primary_tools = Vec::new();
    let mut secondary_tools = Vec::new();
    let mut rest_tools = Vec::new();

    for tool in tools {
        let name = tool.function.name.as_str();
        if primary.contains(&name) {
            primary_tools.push(tool);
        } else if secondary.contains(&name) {
            secondary_tools.push(tool);
        } else {
            rest_tools.push(tool);
        }
    }

    // Sort primary/secondary by the order defined in the task type
    primary_tools.sort_by_key(|t| {
        primary
            .iter()
            .position(|&n| n == t.function.name)
            .unwrap_or(usize::MAX)
    });
    secondary_tools.sort_by_key(|t| {
        secondary
            .iter()
            .position(|&n| n == t.function.name)
            .unwrap_or(usize::MAX)
    });

    let mut result =
        Vec::with_capacity(primary_tools.len() + secondary_tools.len() + rest_tools.len());
    result.extend(primary_tools);
    result.extend(secondary_tools);
    result.extend(rest_tools);
    result
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
}
