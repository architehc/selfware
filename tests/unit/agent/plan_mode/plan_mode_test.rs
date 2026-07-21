use super::*;

#[test]
fn test_plan_step_creation() {
    let step = PlanStep::new(1, "Read the main file");
    assert_eq!(step.id, 1);
    assert_eq!(step.description, "Read the main file");
    assert!(matches!(step.status, StepStatus::Pending));
}

#[test]
fn test_plan_step_with_tool_hint() {
    let step = PlanStep::new(1, "Read file").with_tool_hint("file_read");
    assert_eq!(step.tool_hint, Some("file_read".to_string()));
}

#[test]
fn test_plan_step_status_transitions() {
    let mut step = PlanStep::new(1, "Test step");
    assert!(matches!(step.status, StepStatus::Pending));

    step.mark_in_progress();
    assert!(matches!(step.status, StepStatus::InProgress));

    step.mark_done();
    assert!(matches!(step.status, StepStatus::Done));
}

#[test]
fn test_plan_step_failed() {
    let mut step = PlanStep::new(1, "Test step");
    step.mark_failed("File not found");
    assert!(matches!(step.status, StepStatus::Failed(ref r) if r == "File not found"));
}

#[test]
fn test_plan_creation() {
    let plan = Plan::with_summary("Fix the bug");
    assert_eq!(plan.summary, "Fix the bug");
    assert!(plan.steps.is_empty());
    assert!(plan.created_at.is_some());
}

#[test]
fn test_plan_add_step() {
    let mut plan = Plan::new();
    {
        if let Some(step) = plan.add_step("Step 1") {
            step.tool_hint = Some("file_read".to_string());
        }
    }
    plan.add_step("Step 2");
    assert_eq!(plan.step_count(), 2);
    assert_eq!(plan.steps[0].id, 1);
    assert_eq!(plan.steps[0].tool_hint, Some("file_read".to_string()));
    assert_eq!(plan.steps[1].id, 2);
}

#[test]
fn test_plan_add_file() {
    let mut plan = Plan::new();
    plan.add_file_to_read("src/main.rs");
    plan.add_file_to_read("src/main.rs"); // Duplicate should be ignored
    assert_eq!(plan.files_to_read.len(), 1);
    assert_eq!(plan.files_to_read[0], "src/main.rs");
}

#[test]
fn test_plan_completion() {
    let mut plan = Plan::new();
    plan.add_step("Step 1");
    plan.add_step("Step 2");

    assert!(!plan.is_complete());
    assert_eq!(plan.completed_count(), 0);

    plan.steps[0].mark_done();
    assert!(!plan.is_complete());
    assert_eq!(plan.completed_count(), 1);

    plan.steps[1].mark_done();
    assert!(plan.is_complete());
    assert_eq!(plan.completed_count(), 2);
}

#[test]
fn test_plan_format() {
    let mut plan = Plan::with_summary("Test plan");
    {
        if let Some(step) = plan.add_step("Step 1") {
            step.tool_hint = Some("file_read".to_string());
        }
    }
    plan.add_file_to_read("src/main.rs");
    plan.estimated_tokens = 1000;

    let formatted = plan.format();
    assert!(formatted.contains("Test plan"));
    assert!(formatted.contains("Step 1"));
    assert!(formatted.contains("file_read"));
    assert!(formatted.contains("src/main.rs"));
    assert!(formatted.contains("1000"));
}

#[test]
fn test_plan_mode_state_transitions() {
    let mut manager = PlanModeManager::new();
    assert!(!manager.is_in_plan_mode());
    assert!(!manager.is_planning());

    manager.enter_plan_mode();
    assert!(manager.is_in_plan_mode());
    assert!(manager.is_planning());

    manager.approve_plan();
    assert!(manager.is_in_plan_mode());
    assert!(!manager.is_planning());
    assert!(manager.is_approved());

    manager.exit_plan_mode();
    assert!(!manager.is_in_plan_mode());
    assert!(!manager.is_approved());
}

#[test]
fn test_store_and_get_plan() {
    let mut manager = PlanModeManager::new();
    let plan = Plan::with_summary("Test plan");

    manager.store_plan(plan.clone());
    assert!(manager.get_plan().is_some());
    assert_eq!(manager.get_plan().unwrap().summary, "Test plan");
}

#[test]
fn test_plan_text_storage() {
    let mut manager = PlanModeManager::new();
    manager.enter_plan_mode();

    manager.store_plan_text("1. Do this\n2. Do that");
    assert_eq!(manager.get_plan_text(), Some("1. Do this\n2. Do that"));
}

#[test]
fn test_is_tool_allowed() {
    let mut manager = PlanModeManager::new();

    // Inactive mode - all tools allowed
    assert!(manager.is_tool_allowed("file_edit", false));
    assert!(manager.is_tool_allowed("file_read", true));

    // Planning mode - only read-only tools allowed
    manager.enter_plan_mode();
    assert!(!manager.is_tool_allowed("file_edit", false));
    assert!(manager.is_tool_allowed("file_read", true));

    // Executing mode - all tools allowed
    manager.approve_plan();
    assert!(manager.is_tool_allowed("file_edit", false));
    assert!(manager.is_tool_allowed("file_read", true));
}

#[test]
fn test_readonly_tool_list() {
    assert!(is_readonly_tool("file_read"));
    assert!(is_readonly_tool("grep_search"));
    assert!(is_readonly_tool("glob_find"));
    assert!(is_readonly_tool("directory_tree"));
    assert!(is_readonly_tool("symbol_search"));
    assert!(!is_readonly_tool("file_edit"));
    assert!(!is_readonly_tool("file_write"));
    assert!(!is_readonly_tool("shell_exec"));
}

#[test]
fn test_step_status_display() {
    assert_eq!(StepStatus::Pending.to_string(), "pending");
    assert_eq!(StepStatus::InProgress.to_string(), "in_progress");
    assert_eq!(StepStatus::Done.to_string(), "done");
    assert_eq!(
        StepStatus::Failed("error".to_string()).to_string(),
        "failed: error"
    );
}

#[test]
fn test_plan_next_pending_step() {
    let mut plan = Plan::new();
    plan.add_step("Step 1");
    plan.add_step("Step 2");
    plan.add_step("Step 3");

    assert_eq!(plan.next_pending_step().unwrap().id, 1);

    plan.steps[0].mark_done();
    assert_eq!(plan.next_pending_step().unwrap().id, 2);

    plan.steps[1].mark_in_progress();
    assert_eq!(plan.next_pending_step().unwrap().id, 2); // Still the same
}

#[test]
fn test_clear_plan() {
    let mut manager = PlanModeManager::new();
    manager.enter_plan_mode();
    manager.store_plan(Plan::with_summary("Test"));
    manager.approve_plan();

    manager.clear_plan();
    assert!(!manager.is_in_plan_mode());
    assert!(!manager.is_approved());
    assert!(manager.get_plan().is_none());
}

#[test]
fn test_parse_plan_from_llm_raw_json() {
    let content = r#"{
            "summary": "Fix the bug",
            "estimated_tokens": 1500,
            "files_to_read": ["src/lib.rs", "src/main.rs"],
            "steps": [
                {"description": "Read lib.rs", "tool_hint": "file_read", "file_path": "src/lib.rs"},
                {"description": "Edit main.rs", "tool_hint": "file_edit", "file_path": "src/main.rs", "context": "add logging"}
            ]
        }"#;
    let plan = parse_plan_from_llm(content).expect("should parse");
    assert_eq!(plan.summary, "Fix the bug");
    assert_eq!(plan.estimated_tokens, 1500);
    assert_eq!(plan.files_to_read, vec!["src/lib.rs", "src/main.rs"]);
    assert_eq!(plan.steps.len(), 2);
    assert_eq!(plan.steps[0].description, "Read lib.rs");
    assert_eq!(plan.steps[0].tool_hint, Some("file_read".to_string()));
    assert_eq!(plan.steps[1].file_path, Some("src/main.rs".to_string()));
    assert_eq!(plan.steps[1].context, Some("add logging".to_string()));
}

#[test]
fn test_parse_plan_from_llm_markdown_fence() {
    let content = "Here is the plan:\n```json\n{\n  \"summary\": \"Plan in fence\",\n  \"steps\": [\n    {\"description\": \"Step one\"}\n  ]\n}\n```\nLet me know.";
    let plan = parse_plan_from_llm(content).expect("should parse fenced json");
    assert_eq!(plan.summary, "Plan in fence");
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].description, "Step one");
}

#[test]
fn test_parse_plan_from_llm_missing_fields() {
    let content = r#"{"steps": [{"description": "Only a step"}]}"#;
    let plan = parse_plan_from_llm(content).expect("should parse partial plan");
    assert!(plan.summary.is_empty());
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].description, "Only a step");
}

#[test]
fn test_parse_plan_from_llm_invalid_content() {
    let content = "This is just plain text with no JSON.";
    assert!(parse_plan_from_llm(content).is_none());
}
