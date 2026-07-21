use super::*;

#[test]
fn test_model_selection_config_default() {
    let config = ModelSelectionConfig::default();
    assert!(config.auto_select);
    assert_eq!(config.models.len(), 3);
}

#[test]
fn test_selector_select_simple() {
    let selector = ModelSelector::default();
    let model = selector.select(TaskComplexity::Simple, 1000);
    assert_eq!(model, "claude-3-haiku"); // Cheapest for simple
}

#[test]
fn test_selector_select_critical() {
    let selector = ModelSelector::default();
    let model = selector.select(TaskComplexity::Critical, 1000);
    assert_eq!(model, "claude-3-opus"); // Most capable for critical
}

#[test]
fn test_selector_recommend() {
    let selector = ModelSelector::default();
    let rec = selector.recommend(TaskComplexity::Standard, 5000);
    assert!(!rec.reason.is_empty());
    assert!(rec.estimated_cost > 0.0);
}

#[test]
fn test_selector_record_usage() {
    let selector = ModelSelector::default();
    selector.record_usage(ModelUsage {
        model_id: "claude-3-5-sonnet".to_string(),
        complexity: TaskComplexity::Standard,
        input_tokens: 1000,
        output_tokens: 500,
        cost: 0.01,
        success: true,
        timestamp: 12345,
    });

    let summary = selector.usage_summary();
    assert_eq!(summary.total_requests, 1);
}

#[test]
fn test_task_complexity_enum() {
    assert_eq!(TaskComplexity::Simple, TaskComplexity::Simple);
    assert_ne!(TaskComplexity::Simple, TaskComplexity::Complex);
}
