use super::*;

#[test]
fn test_budget_allocation() {
    let mut budget = TokenBudget::new(1000);
    budget.reserve(20); // Reserve 200 tokens

    assert_eq!(budget.remaining(), 800);

    let granted = budget.allocate(500);
    assert_eq!(granted, 500);
    assert_eq!(budget.used(), 500);
    assert_eq!(budget.remaining(), 300);
}

#[test]
fn test_budget_exhaustion() {
    let mut budget = TokenBudget::new(1000);
    budget.reserve(20);

    budget.allocate(800); // Use all available
    assert!(budget.exhausted());

    // Further allocations return 0
    let granted = budget.allocate(100);
    assert_eq!(granted, 0);
}

#[test]
fn test_depth_downgrade() {
    assert!(Depth::Full.downgrade().is_some());
    assert_eq!(Depth::Full.downgrade().unwrap(), Depth::Signatures);
    assert_eq!(Depth::Signatures.downgrade().unwrap(), Depth::Overview);
    assert!(Depth::Overview.downgrade().is_none());
}

#[test]
fn test_plan_budget_iterations() {
    let mut budget = PlanBudget::new(10, 10000);
    assert_eq!(budget.iterations_remaining(), 10);

    budget.next_iteration();
    assert_eq!(budget.current_iteration, 1);
    assert_eq!(budget.iterations_remaining(), 9);
}
