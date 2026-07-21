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
/// True if `word` appears in `haystack` as a whole word (bounded by non-
/// alphanumeric characters on both sides). Prevents substring false positives
/// like "ship" inside "relationship" or "page" inside a longer identifier.
fn contains_word(haystack: &str, word: &str) -> bool {
    let mut from = 0;
    while let Some(pos) = haystack[from..].find(word) {
        let abs = from + pos;
        let before_ok = abs == 0
            || !haystack[..abs]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric());
        let after = &haystack[abs + word.len()..];
        let after_ok = !after.chars().next().is_some_and(|c| c.is_alphanumeric());
        if before_ok && after_ok {
            return true;
        }
        from = abs + word.len();
    }
    false
}

pub fn classify_task(task: &str) -> TaskType {
    let t = task.to_lowercase();

    // Explicit read-only intent wins — UNLESS the prompt also carries a clear
    // standalone mutation instruction ("fix the bug, but do not edit the tests"
    // is still an Edit task). Without this override, a review prompt like
    // "... Do NOT edit any files ..." was classified as Edit (it contains the
    // substring "edit"), so the preamble told the model to modify code — it then
    // tried to edit a read-only task and looped.
    let has_standalone_mutation_intent = ["fix ", "implement ", "refactor ", "create ", "write "]
        .iter()
        .any(|verb| t.contains(verb));
    if !has_standalone_mutation_intent
        && (t.contains("do not edit")
        || t.contains("don't edit")
        || t.contains("do not modify")
        || t.contains("don't modify")
        || t.contains("do not change")
        || t.contains("don't change")
        || t.contains("without editing")
        || t.contains("without modifying")
        || t.contains("read-only")
        || t.contains("read only")
        // Review / audit / inspect are read-only intents. Catch them here,
        // before the Test/Edit classifiers, so a review whose target or wording
        // happens to contain "spec"(ific), "test", or "bug" is not mis-typed as
        // a mutation task and handed a "write code / run tests" workflow it can
        // never satisfy (found debugging a review of verification.rs → Testing).
        || contains_word(&t, "review")
        || contains_word(&t, "reviewing")
        || contains_word(&t, "audit")
        || contains_word(&t, "inspect"))
    {
        return TaskType::Read;
    }

    // Ship/deploy — check first because "commit" could appear in edit tasks
    // Word-boundary matches so "ship"/"release" don't fire inside "relationship",
    // "membership", "township", etc. (CLS-SHIP-SUBSTRING).
    if t.contains("commit")
        && (t.contains("push") || contains_word(&t, "ship") || t.contains("deploy"))
        || t.contains("deploy")
        || contains_word(&t, "release")
        || t.contains("merge to main")
        || contains_word(&t, "ship")
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
        // Word-boundary so "page"/"css" don't fire inside longer identifiers
        // (CLS-VISUAL-SUBSTRING).
        || contains_word(&t, "page")
        || contains_word(&t, "css")
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
        // Whole-word so "spec"(ification), "spec"(ific), "e"spec"ially",
        // "in"spec"t", "re"spec"t" don't false-positive a task into Testing
        // (same substring-match class as the earlier "thread"->"read" bug).
        || contains_word(&t, "spec")
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
    // Short words use whole-word matching: bare contains("read") also matched
    // "thread"/"already", contains("list") matched "enlist"/"playlist", and
    // contains("count") matched "account"/"discount" — so "Update the thread
    // pool" was misclassified as Read (found by GLM-5.2 reviewing task_focus.rs).
    if contains_word(&t, "read")
        || t.contains("explain")
        || t.contains("describe")
        || t.contains("analyze")
        || t.contains("understand")
        || t.contains("how does")
        || t.contains("what is")
        || t.contains("show me")
        || t.contains("tell me")
        || contains_word(&t, "list")
        || contains_word(&t, "count")
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
#[path = "../../tests/unit/tools/task_focus/task_focus_test.rs"]
mod tests;
