use super::*;

#[test]
fn test_budget_allocation() {
    let mut budget = TokenBudget::new(1000);
    budget.reserve(20); // Reserve 200 tokens

    assert_eq!(budget.remaining(), 800);

    assert!(budget.try_allocate(500));
    assert_eq!(budget.used(), 500);
    assert_eq!(budget.remaining(), 300);
}

#[test]
fn test_budget_exhaustion() {
    let mut budget = TokenBudget::new(1000);
    budget.reserve(20);

    assert!(budget.try_allocate(800)); // Use all available
    assert!(budget.exhausted());

    // Further allocations are refused
    assert!(!budget.try_allocate(100));
    assert_eq!(budget.used(), 800);
}

/// The budget is a hard limit: a request larger than what remains is
/// refused whole — never granted in part (the old `allocate` returned
/// `requested.min(available)` and its caller rendered the whole file).
#[test]
fn test_budget_refuses_partial_grant() {
    let mut budget = TokenBudget::new(1000);
    assert!(budget.try_allocate(900));
    assert!(!budget.try_allocate(200));
    assert_eq!(budget.used(), 900, "a refused request records nothing");
    assert_eq!(budget.remaining(), 100);
    assert!(budget.try_allocate(100));
    assert_eq!(budget.remaining(), 0);
}

fn option(depth: Depth, complete: bool, files: usize, total: usize, inc: usize) -> MeasuredDepth {
    MeasuredDepth {
        depth,
        complete,
        files_included: files,
        symbols_total: total,
        symbols_included: inc,
    }
}

#[test]
fn test_suggest_depth_prefers_first_complete_option() {
    let options = [
        option(Depth::Signatures, true, 3, 10, 10),
        option(Depth::Overview, true, 3, 20, 20),
    ];
    assert_eq!(
        TokenBudget::suggest_depth(&options),
        Some(Depth::Signatures)
    );

    let options = [
        option(Depth::Signatures, false, 2, 10, 6),
        option(Depth::Overview, true, 3, 20, 20),
    ];
    assert_eq!(TokenBudget::suggest_depth(&options), Some(Depth::Overview));
}

/// When nothing fits whole, the depth reaching the most files wins — never
/// a depth already measured to cover less (the old inverted branch returned
/// Signatures whenever Overview could not cover the set).
#[test]
fn test_suggest_depth_without_complete_option_maximises_reach() {
    let options = [
        option(Depth::Signatures, false, 2, 10, 4),
        option(Depth::Overview, false, 5, 30, 12),
    ];
    assert_eq!(TokenBudget::suggest_depth(&options), Some(Depth::Overview));

    // Same file reach: the larger symbol fraction wins.
    let options = [
        option(Depth::Signatures, false, 5, 10, 9),
        option(Depth::Overview, false, 5, 30, 12),
    ];
    assert_eq!(
        TokenBudget::suggest_depth(&options),
        Some(Depth::Signatures)
    );
    assert_eq!(TokenBudget::suggest_depth(&[]), None);
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
