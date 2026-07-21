use super::*;

#[test]
fn test_selector_auto_select_disabled() {
    let config = ModelSelectionConfig {
        auto_select: false,
        ..Default::default()
    };
    let selector = ModelSelector::new(config);
    // With auto_select disabled, always returns default model
    let model = selector.select(TaskComplexity::Simple, 1000);
    assert_eq!(model, "claude-3-5-sonnet");
    let model = selector.select(TaskComplexity::Critical, 1000);
    assert_eq!(model, "claude-3-5-sonnet");
}

#[test]
fn test_selector_select_standard() {
    let selector = ModelSelector::default();
    let model = selector.select(TaskComplexity::Standard, 1000);
    // Standard falls through to "balance cost and capability" branch,
    // finds tier 2 model
    assert_eq!(model, "claude-3-5-sonnet");
}

#[test]
fn test_selector_select_complex() {
    let selector = ModelSelector::default();
    let model = selector.select(TaskComplexity::Complex, 1000);
    // Complex prefers most capable (highest tier)
    assert_eq!(model, "claude-3-opus");
}

#[test]
fn test_selector_select_no_suitable_models() {
    let config = ModelSelectionConfig {
        models: vec![ModelPricing {
            model_id: "tiny-model".to_string(),
            input_cost_per_1k: 0.0001,
            output_cost_per_1k: 0.0005,
            max_context: 100,
            capability_tier: 1,
            speed_tier: 3,
        }],
        default_model: "fallback".to_string(),
        auto_select: true,
        max_cost_per_request: 0.50,
    };
    let selector = ModelSelector::new(config);
    // Token count exceeds max_context, no suitable models
    let model = selector.select(TaskComplexity::Standard, 50_000);
    assert_eq!(model, "fallback");
}

#[test]
fn test_selector_select_no_tier_match() {
    let config = ModelSelectionConfig {
        models: vec![ModelPricing {
            model_id: "basic-model".to_string(),
            input_cost_per_1k: 0.0001,
            output_cost_per_1k: 0.0005,
            max_context: 200_000,
            capability_tier: 1,
            speed_tier: 3,
        }],
        default_model: "fallback".to_string(),
        auto_select: true,
        max_cost_per_request: 0.50,
    };
    let selector = ModelSelector::new(config);
    // Critical needs tier 3, only have tier 1
    let model = selector.select(TaskComplexity::Critical, 1000);
    assert_eq!(model, "fallback");
}

#[test]
fn test_selector_simple_picks_cheapest() {
    let config = ModelSelectionConfig {
        models: vec![
            ModelPricing {
                model_id: "expensive".to_string(),
                input_cost_per_1k: 0.01,
                output_cost_per_1k: 0.05,
                max_context: 200_000,
                capability_tier: 1,
                speed_tier: 2,
            },
            ModelPricing {
                model_id: "cheap".to_string(),
                input_cost_per_1k: 0.001,
                output_cost_per_1k: 0.005,
                max_context: 200_000,
                capability_tier: 1,
                speed_tier: 3,
            },
        ],
        default_model: "expensive".to_string(),
        auto_select: true,
        max_cost_per_request: 1.0,
    };
    let selector = ModelSelector::new(config);
    let model = selector.select(TaskComplexity::Simple, 1000);
    assert_eq!(model, "cheap");
}

#[test]
fn test_selector_standard_no_tier2_falls_to_first() {
    // If no tier-2 model exists, Standard balance branch takes first suitable
    let config = ModelSelectionConfig {
        models: vec![ModelPricing {
            model_id: "tier3-only".to_string(),
            input_cost_per_1k: 0.01,
            output_cost_per_1k: 0.05,
            max_context: 200_000,
            capability_tier: 3,
            speed_tier: 1,
        }],
        default_model: "fallback".to_string(),
        auto_select: true,
        max_cost_per_request: 1.0,
    };
    let selector = ModelSelector::new(config);
    let model = selector.select(TaskComplexity::Standard, 1000);
    assert_eq!(model, "tier3-only");
}

#[test]
fn test_selector_record_usage_overflow() {
    let selector = ModelSelector::default();
    // Record > 100 entries to exercise the pop_front trimming
    for i in 0..110 {
        selector.record_usage(ModelUsage {
            model_id: format!("model-{}", i),
            complexity: TaskComplexity::Standard,
            input_tokens: 100,
            output_tokens: 50,
            cost: 0.001,
            success: true,
            timestamp: i as u64,
        });
    }
    let summary = selector.usage_summary();
    assert_eq!(summary.total_requests, 100);
}

#[test]
fn test_selector_usage_summary_empty() {
    let selector = ModelSelector::default();
    let summary = selector.usage_summary();
    assert_eq!(summary.total_requests, 0);
    assert_eq!(summary.total_cost, 0.0);
    assert_eq!(summary.total_tokens, 0);
    assert_eq!(summary.success_rate, 0.0);
    assert!(summary.by_model.is_empty());
}

#[test]
fn test_selector_usage_summary_success_rate() {
    let selector = ModelSelector::default();
    for i in 0..10 {
        selector.record_usage(ModelUsage {
            model_id: "claude-3-5-sonnet".to_string(),
            complexity: TaskComplexity::Standard,
            input_tokens: 100,
            output_tokens: 50,
            cost: 0.01,
            success: i < 8, // 8 successes, 2 failures
            timestamp: i as u64,
        });
    }
    let summary = selector.usage_summary();
    assert_eq!(summary.total_requests, 10);
    assert!((summary.success_rate - 0.8).abs() < 0.001);
    assert_eq!(*summary.by_model.get("claude-3-5-sonnet").unwrap(), 10);
}

#[test]
fn test_selector_usage_summary_multiple_models() {
    let selector = ModelSelector::default();
    selector.record_usage(ModelUsage {
        model_id: "model-a".to_string(),
        complexity: TaskComplexity::Simple,
        input_tokens: 100,
        output_tokens: 50,
        cost: 0.01,
        success: true,
        timestamp: 1,
    });
    selector.record_usage(ModelUsage {
        model_id: "model-b".to_string(),
        complexity: TaskComplexity::Complex,
        input_tokens: 200,
        output_tokens: 100,
        cost: 0.05,
        success: true,
        timestamp: 2,
    });
    selector.record_usage(ModelUsage {
        model_id: "model-a".to_string(),
        complexity: TaskComplexity::Standard,
        input_tokens: 150,
        output_tokens: 75,
        cost: 0.02,
        success: false,
        timestamp: 3,
    });

    let summary = selector.usage_summary();
    assert_eq!(summary.total_requests, 3);
    assert!((summary.total_cost - 0.08).abs() < 0.001);
    assert_eq!(summary.total_tokens, 100 + 50 + 200 + 100 + 150 + 75);
    assert_eq!(*summary.by_model.get("model-a").unwrap(), 2);
    assert_eq!(*summary.by_model.get("model-b").unwrap(), 1);
}

#[test]
fn test_recommend_simple() {
    let selector = ModelSelector::default();
    let rec = selector.recommend(TaskComplexity::Simple, 1000);
    assert_eq!(rec.model_id, "claude-3-haiku");
    assert_eq!(rec.reason, "Using faster, cheaper model for simple task");
    assert_eq!(rec.alternative, Some("claude-3-5-sonnet".to_string()));
    assert!(rec.estimated_cost > 0.0);
}

#[test]
fn test_recommend_complex() {
    let selector = ModelSelector::default();
    let rec = selector.recommend(TaskComplexity::Complex, 5000);
    assert_eq!(rec.reason, "Using capable model for complex task");
    assert_eq!(rec.alternative, Some("claude-3-opus".to_string()));
}

#[test]
fn test_recommend_critical() {
    let selector = ModelSelector::default();
    let rec = selector.recommend(TaskComplexity::Critical, 5000);
    assert_eq!(rec.reason, "Using most capable model for critical task");
    assert_eq!(rec.alternative, None);
}

#[test]
fn test_get_alternative_standard() {
    let selector = ModelSelector::default();
    let rec = selector.recommend(TaskComplexity::Standard, 1000);
    assert_eq!(rec.alternative, Some("claude-3-haiku".to_string()));
}
