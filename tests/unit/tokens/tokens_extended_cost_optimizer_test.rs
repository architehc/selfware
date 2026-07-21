use super::*;

#[test]
fn test_cost_optimizer_new_custom_configs() {
    let optimizer = CostOptimizer::new(
        PruningConfig {
            target_tokens: 50_000,
            ..Default::default()
        },
        ModelSelectionConfig::default(),
        BudgetConfig {
            daily_budget: 5.0,
            ..Default::default()
        },
    );
    assert_eq!(optimizer.tracker().total_tokens(), 0);
    assert!(optimizer.pruner().needs_pruning(60_000));
    assert!(!optimizer.pruner().needs_pruning(40_000));
}

#[test]
fn test_cost_optimizer_components_interact() {
    let optimizer = CostOptimizer::default();
    optimizer.tracker().record_usage(5000, 2000);
    optimizer.budget().record_spending(0.05);

    let summary = optimizer.summary();
    assert_eq!(summary.token_summary.prompt_tokens, 5000);
    assert_eq!(summary.token_summary.completion_tokens, 2000);
    assert!(summary.budget_status.daily_spent > 0.0);
}

#[test]
fn test_cost_optimizer_recommendations_budget_nearly_exhausted() {
    let optimizer = CostOptimizer::new(
        PruningConfig::default(),
        ModelSelectionConfig::default(),
        BudgetConfig {
            daily_budget: 10.0,
            monthly_budget: 100.0,
            alert_threshold: 0.8,
            hard_limit: false,
        },
    );
    // Spend 9.0 out of 10.0, leaving only 10% remaining
    optimizer.budget().record_spending(9.0);

    let recommendations = optimizer.get_recommendations();
    let budget_rec = recommendations
        .iter()
        .any(|r| r.category == "Budget" && r.message.contains("nearly exhausted"));
    assert!(budget_rec);
}

#[test]
fn test_cost_optimizer_no_recommendations_fresh() {
    let optimizer = CostOptimizer::default();
    let recommendations = optimizer.get_recommendations();
    // With fresh state, budget is full → no budget recommendation
    // No pruning or model usage → no other recommendations
    assert!(
        recommendations.is_empty(),
        "Expected no recommendations for fresh optimizer, got: {:?}",
        recommendations
    );
}

#[test]
fn test_cost_optimizer_summary_fields() {
    let optimizer = CostOptimizer::default();
    optimizer.tracker().record_usage(1000, 500);
    optimizer.tracker().record_drift(1100, 1000);

    let summary = optimizer.summary();
    assert_eq!(summary.token_summary.total_tokens, 1500);
    assert_eq!(summary.token_summary.api_calls, 1);
    assert_eq!(summary.pruning_stats.total_operations, 0);
    assert_eq!(summary.model_usage.total_requests, 0);
    assert_eq!(summary.token_summary.drift.samples, 1);
}
