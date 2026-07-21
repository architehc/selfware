use super::*;

#[test]
fn test_cost_optimizer_default() {
    let optimizer = CostOptimizer::default();
    assert_eq!(optimizer.tracker().total_tokens(), 0);
}

#[test]
fn test_cost_optimizer_components() {
    let optimizer = CostOptimizer::default();
    optimizer.tracker().record_usage(100, 50);
    assert_eq!(optimizer.tracker().total_tokens(), 150);
}

#[test]
fn test_cost_optimizer_recommendations() {
    let optimizer = CostOptimizer::default();
    let recommendations = optimizer.get_recommendations();
    // Should have no recommendations with fresh state
    assert!(recommendations.len() <= 3);
}

#[test]
fn test_cost_optimizer_summary() {
    let optimizer = CostOptimizer::default();
    let summary = optimizer.summary();
    assert_eq!(summary.token_summary.total_tokens, 0);
}

#[test]
fn test_optimization_priority() {
    assert_eq!(OptimizationPriority::Low, OptimizationPriority::Low);
    assert_ne!(OptimizationPriority::Low, OptimizationPriority::High);
}

// ── estimate_image_tokens tests ──

#[test]
fn test_image_tokens_low_detail() {
    // Low detail always returns 85 regardless of size
    assert_eq!(estimate_image_tokens(4096, 4096, "low"), 85);
    assert_eq!(estimate_image_tokens(1, 1, "low"), 85);
    assert_eq!(estimate_image_tokens(1920, 1080, "low"), 85);
}

#[test]
fn test_image_tokens_high_detail_small() {
    // 512×512: fits in 1 tile → 170 + 85 = 255
    assert_eq!(estimate_image_tokens(512, 512, "high"), 170 + 85);
}

#[test]
fn test_image_tokens_high_detail_1024x1024() {
    // 1024×1024: shortest side > 768, scaled to 768×768
    // tiles: ceil(768/512) × ceil(768/512) = 2 × 2 = 4
    // cost: 4 × 170 + 85 = 765
    assert_eq!(estimate_image_tokens(1024, 1024, "high"), 765);
}

#[test]
fn test_image_tokens_high_detail_1920x1080() {
    // 1920×1080: shortest=1080 > 768, scale by 768/1080 ≈ 0.7111
    // → 1365.3 × 768
    // tiles: ceil(1365.3/512) × ceil(768/512) = 3 × 2 = 6
    // cost: 6 × 170 + 85 = 1105
    assert_eq!(estimate_image_tokens(1920, 1080, "high"), 1105);
}

#[test]
fn test_image_tokens_auto_small_uses_low() {
    // 256×256: both sides ≤ 512, auto chooses low
    assert_eq!(estimate_image_tokens(256, 256, "auto"), 85);
}

#[test]
fn test_image_tokens_auto_large_uses_high() {
    // 1024×1024: both sides > 512, auto chooses high
    assert_eq!(estimate_image_tokens(1024, 1024, "auto"), 765);
}

// ── Additional coverage tests ──

#[test]
fn test_estimate_tokens_empty_string() {
    // Empty string should return at least 1 (due to .max(1))
    let estimate = estimate_tokens("");
    assert_eq!(estimate, 1);
}

#[test]
fn test_estimate_tokens_whitespace_only() {
    let estimate = estimate_tokens("   \n\t  ");
    assert!(estimate >= 1);
}

#[test]
fn test_estimate_tokens_very_long_text() {
    let text = "word ".repeat(10_000);
    let estimate = estimate_tokens(&text);
    assert!(estimate > 1000);
}

#[test]
fn test_estimate_json_tokens_empty_object() {
    let json = serde_json::json!({});
    let estimate = estimate_json_tokens(&json);
    assert!(estimate >= 1);
}

#[test]
fn test_estimate_json_tokens_null() {
    let json = serde_json::json!(null);
    let estimate = estimate_json_tokens(&json);
    assert!(estimate >= 1);
}

#[test]
fn test_estimate_json_tokens_array() {
    let json = serde_json::json!([1, 2, 3, 4, 5]);
    let estimate = estimate_json_tokens(&json);
    assert!(estimate >= 1);
}

#[test]
fn test_estimate_json_tokens_deeply_nested() {
    let json = serde_json::json!({
        "a": {"b": {"c": {"d": {"e": "deep"}}}}
    });
    let estimate = estimate_json_tokens(&json);
    assert!(estimate > 3);
}

#[test]
fn test_image_tokens_high_detail_very_large() {
    // 8000×6000: longest side 8000 > 2048, scale by 2048/8000 = 0.256
    // → 2048 × 1536; shortest side 1536 > 768, scale by 768/1536 = 0.5
    // → 1024 × 768
    // tiles: ceil(1024/512) × ceil(768/512) = 2 × 2 = 4
    // cost: 4 × 170 + 85 = 765
    assert_eq!(estimate_image_tokens(8000, 6000, "high"), 765);
}

#[test]
fn test_image_tokens_high_detail_no_scaling_needed() {
    // 400×300: no scaling needed (both < 2048, shortest 300 < 768)
    // tiles: ceil(400/512) × ceil(300/512) = 1 × 1 = 1
    // cost: 1 × 170 + 85 = 255
    assert_eq!(estimate_image_tokens(400, 300, "high"), 255);
}

#[test]
fn test_image_tokens_high_detail_only_longest_side_scaling() {
    // 3000×500: longest=3000 > 2048, scale by 2048/3000 ≈ 0.6827
    // → 2048 × 341.3; shortest=341.3 < 768, no second scaling
    // tiles: ceil(2048/512) × ceil(341.3/512) = 4 × 1 = 4
    // cost: 4 × 170 + 85 = 765
    assert_eq!(estimate_image_tokens(3000, 500, "high"), 765);
}

#[test]
fn test_image_tokens_auto_one_side_large_one_small() {
    // 600×200: width > 512 but height <= 512, auto checks BOTH > 512
    // Since height is not > 512, auto chooses low
    assert_eq!(estimate_image_tokens(600, 200, "auto"), 85);
}

#[test]
fn test_image_tokens_auto_both_at_boundary() {
    // 512×512: both sides are exactly 512, not > 512, so auto chooses low
    assert_eq!(estimate_image_tokens(512, 512, "auto"), 85);
}

#[test]
fn test_image_tokens_unknown_detail_treated_as_auto() {
    // Unknown detail string treated like "auto"
    assert_eq!(estimate_image_tokens(256, 256, "medium"), 85);
    assert_eq!(estimate_image_tokens(1024, 1024, "something"), 765);
}

#[test]
fn test_image_tokens_high_detail_tall_narrow() {
    // 200×4000: longest=4000 > 2048, scale by 2048/4000 = 0.512
    // → 102.4 × 2048; shortest=102.4 < 768, no second scaling
    // tiles: ceil(102.4/512) × ceil(2048/512) = 1 × 4 = 4
    // cost: 4 × 170 + 85 = 765
    assert_eq!(estimate_image_tokens(200, 4000, "high"), 765);
}

#[test]
fn test_summary_display_without_duration_and_without_drift() {
    // Construct a TokenSummary manually with no duration and no drift
    let summary = TokenSummary {
        prompt_tokens: 500,
        completion_tokens: 200,
        total_tokens: 700,
        api_calls: 3,
        estimated_cost: 0.0045,
        cost_basis: "rates for claude-3-5-sonnet".to_string(),
        duration: None,
        drift: DriftStats::default(),
    };
    let display = format!("{}", summary);
    assert!(display.contains("700"));
    assert!(display.contains("500"));
    assert!(display.contains("200"));
    assert!(display.contains("3"));
    // Should not contain Duration or Drift sections
    assert!(!display.contains("Duration"));
    assert!(!display.contains("Drift"));
}

#[test]
fn test_summary_display_with_duration() {
    let summary = TokenSummary {
        prompt_tokens: 1000,
        completion_tokens: 500,
        total_tokens: 1500,
        api_calls: 2,
        estimated_cost: 0.01,
        cost_basis: "rates for claude-3-5-sonnet".to_string(),
        duration: Some(std::time::Duration::from_secs_f64(5.3)),
        drift: DriftStats::default(),
    };
    let display = format!("{}", summary);
    assert!(display.contains("Duration: 5.3s"));
    // No drift samples, so no drift section
    assert!(!display.contains("Drift"));
}

#[test]
fn test_summary_display_with_drift() {
    let summary = TokenSummary {
        prompt_tokens: 1000,
        completion_tokens: 500,
        total_tokens: 1500,
        api_calls: 2,
        estimated_cost: 0.01,
        cost_basis: "rates for claude-3-5-sonnet".to_string(),
        duration: None,
        drift: DriftStats {
            samples: 4,
            cumulative_drift: 40,
            cumulative_abs_drift: 60,
            max_over: 30,
            max_under: -10,
        },
    };
    let display = format!("{}", summary);
    // avg drift = 40/4 = 10, MAE = 60/4 = 15
    assert!(display.contains("Drift"));
    assert!(display.contains("4 samples"));
}

#[test]
fn test_summary_display_with_both_duration_and_drift() {
    let summary = TokenSummary {
        prompt_tokens: 1000,
        completion_tokens: 500,
        total_tokens: 1500,
        api_calls: 2,
        estimated_cost: 0.01,
        cost_basis: "rates for claude-3-5-sonnet".to_string(),
        duration: Some(std::time::Duration::from_secs(10)),
        drift: DriftStats {
            samples: 2,
            cumulative_drift: -20,
            cumulative_abs_drift: 20,
            max_over: 0,
            max_under: -10,
        },
    };
    let display = format!("{}", summary);
    assert!(display.contains("Duration"));
    assert!(display.contains("Drift"));
}

#[test]
fn test_tracker_record_step_no_tool_name() {
    let tracker = TokenTracker::new();
    tracker.record_step(0, 50, 25, None);
    let steps = tracker.step_usage();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].tool_name, None);
    assert_eq!(steps[0].prompt_tokens, 50);
    assert_eq!(steps[0].completion_tokens, 25);
}

#[test]
fn test_tracker_estimate_cost_zero_tokens() {
    let tracker = TokenTracker::new();
    let cost = tracker.estimate_cost();
    assert!((cost - 0.0).abs() < f64::EPSILON);
}

#[test]
fn test_drift_exact_match() {
    let tracker = TokenTracker::new();
    tracker.record_drift(100, 100);
    let drift = tracker.drift_stats();
    assert_eq!(drift.samples, 1);
    assert_eq!(drift.cumulative_drift, 0);
    assert_eq!(drift.cumulative_abs_drift, 0);
    assert_eq!(drift.max_over, 0);
    assert_eq!(drift.max_under, 0);
}

#[test]
fn test_drift_zero_actual() {
    // When actual is 0, drift percentage check is skipped (no divide-by-zero)
    let tracker = TokenTracker::new();
    tracker.record_drift(50, 0);
    let drift = tracker.drift_stats();
    assert_eq!(drift.samples, 1);
    assert_eq!(drift.cumulative_drift, 50);
    assert_eq!(drift.max_over, 50);
}

#[test]
fn test_drift_large_deviation_triggers_log() {
    // >15% deviation with actual > 0: exercises the tracing::warn branch
    let tracker = TokenTracker::new();
    // estimated=200, actual=100 -> 100% deviation
    tracker.record_drift(200, 100);
    let drift = tracker.drift_stats();
    assert_eq!(drift.samples, 1);
    assert_eq!(drift.cumulative_drift, 100);
}

#[test]
fn test_drift_small_deviation_no_warn() {
    // <15% deviation: does not trigger the warn branch
    let tracker = TokenTracker::new();
    // estimated=105, actual=100 -> 5% deviation
    tracker.record_drift(105, 100);
    let drift = tracker.drift_stats();
    assert_eq!(drift.samples, 1);
    assert_eq!(drift.cumulative_drift, 5);
}

#[test]
fn test_reset_clears_step_usage() {
    let tracker = TokenTracker::new();
    tracker.record_step(1, 100, 50, Some("tool".to_string()));
    tracker.record_step(2, 200, 100, None);
    assert_eq!(tracker.step_usage().len(), 2);
    tracker.reset();
    assert_eq!(tracker.step_usage().len(), 0);
    assert_eq!(tracker.total_tokens(), 0);
}

#[test]
fn test_reset_clears_drift() {
    let tracker = TokenTracker::new();
    tracker.record_drift(200, 100);
    tracker.record_drift(50, 100);
    assert_eq!(tracker.drift_stats().samples, 2);
    tracker.reset();
    let drift = tracker.drift_stats();
    assert_eq!(drift.samples, 0);
    assert_eq!(drift.cumulative_drift, 0);
    assert_eq!(drift.cumulative_abs_drift, 0);
    assert_eq!(drift.max_over, 0);
    assert_eq!(drift.max_under, 0);
}

#[test]
fn test_model_pricing_calculate_cost_zero_tokens() {
    let pricing = ModelPricing::claude_sonnet();
    let cost = pricing.calculate_cost(0, 0);
    assert!((cost - 0.0).abs() < f64::EPSILON);
}

#[test]
fn test_model_pricing_calculate_cost_large_tokens() {
    let pricing = ModelPricing::claude_haiku();
    // 1M input at 0.00025/1K + 1M output at 0.00125/1K
    // = 1000 * 0.00025 + 1000 * 0.00125 = 0.25 + 1.25 = 1.50
    let cost = pricing.calculate_cost(1_000_000, 1_000_000);
    assert!((cost - 1.50).abs() < 0.001);
}

#[test]
fn test_model_pricing_opus_cost() {
    let pricing = ModelPricing::claude_opus();
    // 10K input at 0.015/1K + 5K output at 0.075/1K
    // = 10 * 0.015 + 5 * 0.075 = 0.15 + 0.375 = 0.525
    let cost = pricing.calculate_cost(10_000, 5_000);
    assert!((cost - 0.525).abs() < 0.0001);
}
