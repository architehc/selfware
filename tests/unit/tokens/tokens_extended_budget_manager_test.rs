use super::*;

#[test]
fn test_budget_can_spend_no_hard_limit() {
    let config = BudgetConfig {
        hard_limit: false,
        daily_budget: 5.0,
        ..Default::default()
    };
    let manager = BudgetManager::new(config);
    manager.record_spending(100.0); // Way over budget
                                    // Without hard limit, can always spend
    assert!(manager.can_spend(1000.0));
}

#[test]
fn test_budget_monthly_spending_and_remaining() {
    let manager = BudgetManager::default();
    manager.record_spending(20.0);
    assert!((manager.monthly_spending() - 20.0).abs() < 0.001);
    assert!((manager.monthly_remaining() - 80.0).abs() < 0.001);
}

#[test]
fn test_budget_daily_exceeded_alert() {
    let config = BudgetConfig {
        daily_budget: 10.0,
        monthly_budget: 100.0,
        alert_threshold: 0.8,
        hard_limit: false,
    };
    let manager = BudgetManager::new(config);
    // Spend 11.0 > daily budget of 10.0 → should trigger DailyExceeded
    manager.record_spending(11.0);
    let alerts = manager.alerts();
    assert!(!alerts.is_empty());
    let daily_exceeded = alerts
        .iter()
        .any(|a| a.alert_type == BudgetAlertType::DailyExceeded);
    assert!(daily_exceeded);
}

#[test]
fn test_budget_daily_warning_alert() {
    let config = BudgetConfig {
        daily_budget: 10.0,
        monthly_budget: 100.0,
        alert_threshold: 0.8,
        hard_limit: false,
    };
    let manager = BudgetManager::new(config);
    // Spend 8.5 → 85% of 10.0, above threshold but below budget
    manager.record_spending(8.5);
    let alerts = manager.alerts();
    let daily_warning = alerts
        .iter()
        .any(|a| a.alert_type == BudgetAlertType::DailyWarning);
    assert!(daily_warning);
}

#[test]
fn test_budget_monthly_exceeded_alert() {
    let config = BudgetConfig {
        daily_budget: 1000.0, // High daily to avoid daily alerts
        monthly_budget: 50.0,
        alert_threshold: 0.8,
        hard_limit: false,
    };
    let manager = BudgetManager::new(config);
    manager.record_spending(55.0);
    let alerts = manager.alerts();
    let monthly_exceeded = alerts
        .iter()
        .any(|a| a.alert_type == BudgetAlertType::MonthlyExceeded);
    assert!(monthly_exceeded);
}

#[test]
fn test_budget_monthly_warning_alert() {
    let config = BudgetConfig {
        daily_budget: 1000.0,
        monthly_budget: 100.0,
        alert_threshold: 0.8,
        hard_limit: false,
    };
    let manager = BudgetManager::new(config);
    // 85% of 100 = 85
    manager.record_spending(85.0);
    let alerts = manager.alerts();
    let monthly_warning = alerts
        .iter()
        .any(|a| a.alert_type == BudgetAlertType::MonthlyWarning);
    assert!(monthly_warning);
}

#[test]
fn test_budget_no_alert_below_threshold() {
    let config = BudgetConfig {
        daily_budget: 100.0,
        monthly_budget: 1000.0,
        alert_threshold: 0.8,
        hard_limit: false,
    };
    let manager = BudgetManager::new(config);
    // Spend only 10% of daily/monthly
    manager.record_spending(10.0);
    let alerts = manager.alerts();
    assert!(alerts.is_empty());
}

#[test]
fn test_budget_hard_limit_blocks_monthly() {
    let config = BudgetConfig {
        daily_budget: 1000.0,
        monthly_budget: 5.0,
        alert_threshold: 0.8,
        hard_limit: true,
    };
    let manager = BudgetManager::new(config);
    manager.record_spending(4.0);
    // 4.0 + 2.0 = 6.0 > monthly budget 5.0
    assert!(!manager.can_spend(2.0));
    assert!(manager.can_spend(0.5));
}

#[test]
fn test_budget_status_full() {
    let config = BudgetConfig {
        daily_budget: 20.0,
        monthly_budget: 200.0,
        alert_threshold: 0.8,
        hard_limit: false,
    };
    let manager = BudgetManager::new(config);
    manager.record_spending(5.0);
    let status = manager.status();
    assert!((status.daily_spent - 5.0).abs() < 0.001);
    assert_eq!(status.daily_budget, 20.0);
    assert!((status.daily_remaining - 15.0).abs() < 0.001);
    assert!((status.monthly_spent - 5.0).abs() < 0.001);
    assert_eq!(status.monthly_budget, 200.0);
    assert!((status.monthly_remaining - 195.0).abs() < 0.001);
}

#[test]
fn test_budget_daily_remaining_saturates_at_zero() {
    let manager = BudgetManager::default(); // daily_budget = 10.0
    manager.record_spending(15.0);
    assert_eq!(manager.daily_remaining(), 0.0);
}

#[test]
fn test_budget_monthly_remaining_saturates_at_zero() {
    let manager = BudgetManager::default(); // monthly_budget = 100.0
    manager.record_spending(150.0);
    assert_eq!(manager.monthly_remaining(), 0.0);
}

#[test]
fn test_budget_multiple_spending_records() {
    let manager = BudgetManager::default();
    manager.record_spending(1.0);
    manager.record_spending(2.0);
    manager.record_spending(3.0);
    assert!((manager.daily_spending() - 6.0).abs() < 0.001);
    assert!((manager.monthly_spending() - 6.0).abs() < 0.001);
}

#[test]
fn test_budget_alert_message_format() {
    let config = BudgetConfig {
        daily_budget: 10.0,
        monthly_budget: 100.0,
        alert_threshold: 0.5,
        hard_limit: false,
    };
    let manager = BudgetManager::new(config);
    manager.record_spending(6.0); // 60% of daily
    let alerts = manager.alerts();
    assert!(!alerts.is_empty());
    let alert = &alerts[0];
    assert!(alert.message.contains("budget at"));
    assert_eq!(alert.threshold, 0.5);
    assert!((alert.current_usage - 6.0).abs() < 0.001);
}
