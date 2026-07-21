use super::*;

#[test]
fn test_budget_config_default() {
    let config = BudgetConfig::default();
    assert_eq!(config.daily_budget, 10.0);
    assert_eq!(config.monthly_budget, 100.0);
}

#[test]
fn test_budget_record_spending() {
    let manager = BudgetManager::default();
    manager.record_spending(1.0);
    assert!((manager.daily_spending() - 1.0).abs() < 0.001);
}

#[test]
fn test_budget_remaining() {
    let manager = BudgetManager::default();
    manager.record_spending(3.0);
    assert!((manager.daily_remaining() - 7.0).abs() < 0.001);
}

#[test]
fn test_budget_can_spend() {
    let config = BudgetConfig {
        hard_limit: true,
        daily_budget: 5.0,
        ..Default::default()
    };
    let manager = BudgetManager::new(config);

    assert!(manager.can_spend(3.0));
    manager.record_spending(4.0);
    assert!(!manager.can_spend(2.0));
}

#[test]
fn test_budget_status() {
    let manager = BudgetManager::default();
    manager.record_spending(2.0);

    let status = manager.status();
    assert!((status.daily_spent - 2.0).abs() < 0.001);
    assert_eq!(status.daily_budget, 10.0);
}

#[test]
fn test_budget_reset_daily() {
    let manager = BudgetManager::default();
    manager.record_spending(5.0);
    manager.reset_daily();

    assert_eq!(manager.daily_spending(), 0.0);
}

#[test]
fn test_budget_alert_type() {
    assert_eq!(BudgetAlertType::DailyWarning, BudgetAlertType::DailyWarning);
    assert_ne!(
        BudgetAlertType::DailyWarning,
        BudgetAlertType::DailyExceeded
    );
}
