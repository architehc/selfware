use super::*;

#[test]
fn test_action_cost_multipliers() {
    assert!((ContextAction::Inspect.cost_multiplier() - 0.06).abs() < f64::EPSILON);
    assert!((ContextAction::ReadFull.cost_multiplier() - 1.0).abs() < f64::EPSILON);
    assert!((ContextAction::Alter.cost_multiplier() - 1.5).abs() < f64::EPSILON);
    assert!((ContextAction::BuildNew.cost_multiplier() - 2.0).abs() < f64::EPSILON);
}

#[test]
fn test_fusion_multipliers() {
    assert!((FusionLevel::Binary.multiplier() - 1.0).abs() < f64::EPSILON);
    assert!((FusionLevel::Trinary.multiplier() - 2.5).abs() < f64::EPSILON);
    assert!((FusionLevel::Quaternary.multiplier() - 4.0).abs() < f64::EPSILON);
}

#[test]
fn test_action_from_str_loose() {
    assert_eq!(
        ContextAction::from_str_loose("inspect"),
        Some(ContextAction::Inspect)
    );
    assert_eq!(
        ContextAction::from_str_loose("READ"),
        Some(ContextAction::ReadFull)
    );
    assert_eq!(
        ContextAction::from_str_loose("edit"),
        Some(ContextAction::Alter)
    );
    assert_eq!(
        ContextAction::from_str_loose("commit"),
        Some(ContextAction::Ship)
    );
    assert_eq!(ContextAction::from_str_loose("unknown"), None);
}

#[test]
fn test_is_cargo_op() {
    assert!(ContextAction::Verify.is_cargo_op());
    assert!(ContextAction::Test.is_cargo_op());
    assert!(!ContextAction::Inspect.is_cargo_op());
    assert!(!ContextAction::Alter.is_cargo_op());
}

#[test]
fn test_estimate_action_cost_nonexistent() {
    // fits_in_budget reads the global budget atomics; hold+reset them so a
    // concurrent budget-mutating test can't perturb the result.
    let _budget = crate::test_support::BudgetGuard::hold();
    let est = estimate_action_cost(
        ContextAction::ReadFull,
        Path::new("/nonexistent/file.rs"),
        FusionLevel::Binary,
    );
    // Should use fallback of 500 tokens
    assert_eq!(est.estimated_tokens, 500);
    assert!(est.fits_in_budget);
}

#[test]
fn test_estimate_action_cost_fusion_scaling() {
    let _budget = crate::test_support::BudgetGuard::hold();
    let binary = estimate_action_cost(
        ContextAction::ReadFull,
        Path::new("/nonexistent/file.rs"),
        FusionLevel::Binary,
    );
    let trinary = estimate_action_cost(
        ContextAction::ReadFull,
        Path::new("/nonexistent/file.rs"),
        FusionLevel::Trinary,
    );
    let quaternary = estimate_action_cost(
        ContextAction::ReadFull,
        Path::new("/nonexistent/file.rs"),
        FusionLevel::Quaternary,
    );

    assert!(trinary.estimated_tokens > binary.estimated_tokens);
    assert!(quaternary.estimated_tokens > trinary.estimated_tokens);
}

#[test]
fn test_cargo_op_adds_overhead() {
    let verify = estimate_action_cost(
        ContextAction::Verify,
        Path::new("/nonexistent/file.rs"),
        FusionLevel::Binary,
    );
    let inspect = estimate_action_cost(
        ContextAction::Inspect,
        Path::new("/nonexistent/file.rs"),
        FusionLevel::Binary,
    );
    // Verify should have more time due to cargo overhead
    assert!(verify.estimated_time_ms > inspect.estimated_time_ms);
}

#[test]
fn test_update_and_read_budget() {
    let _budget = crate::test_support::BudgetGuard::hold();
    update_budget(50_000, 200_000, 10);
    let (used, total, files) = read_budget();
    assert_eq!(used, 50_000);
    assert_eq!(total, 200_000);
    assert_eq!(files, 10);
}

#[test]
fn test_extract_use_deps_empty() {
    let deps = extract_use_deps(Path::new("/nonexistent/file.rs"));
    assert!(deps.is_empty());
}
