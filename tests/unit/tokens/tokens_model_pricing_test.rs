use super::*;

#[test]
fn test_model_pricing_calculate_cost() {
    let pricing = ModelPricing::claude_sonnet();
    let cost = pricing.calculate_cost(1000, 500);
    // 1000/1000 * 0.003 + 500/1000 * 0.015 = 0.003 + 0.0075 = 0.0105
    assert!((cost - 0.0105).abs() < 0.0001);
}

#[test]
fn test_model_pricing_haiku() {
    let pricing = ModelPricing::claude_haiku();
    assert_eq!(pricing.capability_tier, 1);
    assert_eq!(pricing.speed_tier, 3);
}

#[test]
fn test_model_pricing_sonnet() {
    let pricing = ModelPricing::claude_sonnet();
    assert_eq!(pricing.capability_tier, 2);
    assert_eq!(pricing.speed_tier, 2);
}

#[test]
fn test_model_pricing_opus() {
    let pricing = ModelPricing::claude_opus();
    assert_eq!(pricing.capability_tier, 3);
    assert_eq!(pricing.speed_tier, 1);
}
