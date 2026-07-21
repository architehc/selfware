use super::*;

#[test]
fn test_web_task_builder() {
    let task = WebTask::new("test", "Test Task")
        .with_description("A test")
        .with_action(WebAction::Navigate {
            url: "https://example.com".into(),
        })
        .with_criterion(SuccessCriterion::PageContains("Example".into()))
        .with_timeout(30);

    assert_eq!(task.id, "test");
    assert_eq!(task.actions.len(), 1);
    assert_eq!(task.success_criteria.len(), 1);
    assert_eq!(task.timeout_secs, 30);
}

#[test]
fn test_all_scenarios() {
    let scenarios = all_scenarios();
    assert_eq!(scenarios.len(), 4);
    assert_eq!(scenarios[0].id, "search-extract");
    assert_eq!(scenarios[1].id, "multi-step-nav");
    assert_eq!(scenarios[2].id, "form-fill");
    assert_eq!(scenarios[3].id, "data-extract");
}

#[test]
fn test_scenario_serde_roundtrip() {
    let task = scenario_search_extract();
    let json = serde_json::to_string(&task).unwrap();
    let parsed: WebTask = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, task.id);
    assert_eq!(parsed.actions.len(), task.actions.len());
}
