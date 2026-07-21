use super::*;

#[test]
fn test_create_plan_includes_task() {
    let plan = Planner::create_plan("Fix the bug", "Some context");
    assert!(plan.contains("<task>"));
    assert!(plan.contains("Fix the bug"));
    assert!(plan.contains("</task>"));
}

#[test]
fn test_create_plan_includes_context() {
    let plan = Planner::create_plan("Some task", "Important context info");
    assert!(plan.contains("<context>"));
    assert!(plan.contains("Important context info"));
    assert!(plan.contains("</context>"));
}

#[test]
fn test_create_plan_includes_instructions() {
    let plan = Planner::create_plan("Task", "Context");
    assert!(plan.contains("step-by-step plan"));
    assert!(plan.contains("Analyze the codebase"));
}

#[test]
fn test_create_plan_with_empty_task() {
    let plan = Planner::create_plan("", "Context");
    assert!(plan.contains("<task>"));
    assert!(plan.contains("</task>"));
}

#[test]
fn test_create_plan_with_empty_context() {
    let plan = Planner::create_plan("Task", "");
    assert!(plan.contains("<context>"));
    assert!(plan.contains("</context>"));
}

#[test]
fn test_create_plan_with_special_characters() {
    let plan = Planner::create_plan("Fix <xml> &amp; \"quotes\"", "Context with 'special' chars");
    assert!(plan.contains("Fix <xml> &amp; \"quotes\""));
    assert!(plan.contains("Context with 'special' chars"));
}

#[test]
fn test_analyze_prompt_includes_path() {
    let prompt = Planner::analyze_prompt("./src");
    assert!(prompt.contains("./src"));
    assert!(prompt.contains("Directory structure"));
    assert!(prompt.contains("Dependencies"));
}

#[test]
fn test_review_prompt_includes_content() {
    let prompt = Planner::review_prompt("src/main.rs", "fn main() {}");
    assert!(prompt.contains("src/main.rs"));
    assert!(prompt.contains("fn main() {}"));
    assert!(prompt.contains("bugs"));
    assert!(prompt.contains("Security"));
}
