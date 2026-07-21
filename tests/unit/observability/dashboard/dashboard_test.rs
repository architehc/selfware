use super::*;

#[test]
fn test_token_usage_new() {
    let usage = TokenUsage::new(100, 50);
    assert_eq!(usage.input, 100);
    assert_eq!(usage.output, 50);
    assert_eq!(usage.total, 150);
}

#[test]
fn test_token_usage_with_cost() {
    let usage = TokenUsage::new(1000, 1000).with_cost(0.01, 0.03);
    assert!(usage.cost.is_some());
    assert_eq!(usage.cost.unwrap(), 0.04);
}

#[test]
fn test_token_usage_add() {
    let mut usage1 = TokenUsage::new(100, 50);
    let usage2 = TokenUsage::new(200, 100);
    usage1.add(&usage2);

    assert_eq!(usage1.input, 300);
    assert_eq!(usage1.output, 150);
    assert_eq!(usage1.total, 450);
}

#[test]
fn test_token_usage_display() {
    let usage = TokenUsage::new(100, 50);
    let display = usage.display();
    assert!(display.contains("150"));
    assert!(display.contains("100 in"));
    assert!(display.contains("50 out"));
}
