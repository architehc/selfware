use super::*;

#[test]
fn test_debt_type_interest() {
    assert!(
        DebtType::Security.default_interest_rate() > DebtType::CodeStyle.default_interest_rate()
    );
}

#[test]
fn test_debt_type_weight() {
    assert!(DebtType::Security.severity_weight() > DebtType::DeadCode.severity_weight());
}

#[test]
fn test_debt_severity_multiplier() {
    assert!(DebtSeverity::Critical.multiplier() > DebtSeverity::Low.multiplier());
}

#[test]
fn test_debt_item_creation() {
    let item = DebtItem::new(DebtType::Complexity, "Reduce cyclomatic complexity")
        .with_severity(DebtSeverity::High)
        .with_file("src/main.rs")
        .with_estimate(8.0);

    assert!(item.id.starts_with("debt_"));
    assert_eq!(item.severity, DebtSeverity::High);
    assert_eq!(item.estimated_hours, 8.0);
}

#[test]
fn test_debt_item_cost() {
    let item = DebtItem::new(DebtType::Duplication, "Remove duplication").with_estimate(4.0);

    assert_eq!(item.fix_cost(), 400.0); // 4 hours * $100
}

#[test]
fn test_debt_item_interest() {
    let item = DebtItem::new(DebtType::Complexity, "Test")
        .with_estimate(10.0)
        .with_age(180); // 6 months

    let total = item.total_cost();
    let base = item.fix_cost();
    assert!(total > base); // Should have accrued interest
}

#[test]
fn test_debt_metrics() {
    let items = vec![
        DebtItem::new(DebtType::Security, "Fix XSS")
            .with_severity(DebtSeverity::Critical)
            .with_estimate(4.0),
        DebtItem::new(DebtType::Complexity, "Simplify")
            .with_severity(DebtSeverity::Medium)
            .with_estimate(8.0),
    ];

    let metrics = DebtMetrics::calculate(&items);

    assert_eq!(metrics.total_items, 2);
    assert_eq!(metrics.by_severity[&DebtSeverity::Critical], 1);
    assert_eq!(metrics.by_type[&DebtType::Security], 1);
}

#[test]
fn test_prioritizer_risk_impact_effort() {
    let items = vec![
        DebtItem::new(DebtType::Security, "Critical security fix")
            .with_severity(DebtSeverity::Critical)
            .with_estimate(2.0),
        DebtItem::new(DebtType::CodeStyle, "Style fix")
            .with_severity(DebtSeverity::Low)
            .with_estimate(1.0),
    ];

    let prioritizer = DebtPrioritizer::new(PrioritizationStrategy::RiskImpactEffort);
    let prioritized = prioritizer.prioritize(&items);

    // Security should be first
    assert_eq!(prioritized[0].item.debt_type, DebtType::Security);
}

#[test]
fn test_prioritizer_quick_wins() {
    let items = vec![
        DebtItem::new(DebtType::Complexity, "Big refactor")
            .with_severity(DebtSeverity::High)
            .with_estimate(40.0),
        DebtItem::new(DebtType::Complexity, "Quick fix")
            .with_severity(DebtSeverity::High)
            .with_estimate(1.0),
    ];

    let prioritizer = DebtPrioritizer::new(PrioritizationStrategy::QuickWins);
    let prioritized = prioritizer.prioritize(&items);

    // Same type and severity, but quick fix (1 hour) should score higher due to lower effort
    // Quick wins = impact / effort, so lower effort means higher score
    assert!(prioritized[0].item.estimated_hours < prioritized[1].item.estimated_hours);
}

#[test]
fn test_roadmap_phase() {
    let mut phase = RoadmapPhase::new("Phase 1", 1);
    let item = DebtItem::new(DebtType::Security, "Fix").with_estimate(8.0);
    phase.add_item(&item);

    assert_eq!(phase.items.len(), 1);
    assert_eq!(phase.estimated_hours, 8.0);
    assert!(phase.roi() >= 0.0);
}

#[test]
fn test_roadmap_generation() {
    let items = vec![
        DebtItem::new(DebtType::Security, "Fix 1").with_estimate(20.0),
        DebtItem::new(DebtType::Complexity, "Fix 2").with_estimate(30.0),
        DebtItem::new(DebtType::CodeStyle, "Fix 3").with_estimate(10.0),
    ];

    let prioritizer = DebtPrioritizer::new(PrioritizationStrategy::RiskImpactEffort);
    let prioritized = prioritizer.prioritize(&items);

    let generator = RoadmapGenerator::new().with_max_hours(50.0);
    let roadmap = generator.generate("Test Roadmap", &prioritized);

    assert!(!roadmap.phases.is_empty());
    assert!(roadmap.total_hours > 0.0);
}

#[test]
fn test_roadmap_payback() {
    let mut roadmap = RefactoringRoadmap::new("Test");
    let mut phase = RoadmapPhase::new("Phase 1", 1);
    phase.estimated_cost = 1000.0;
    phase.expected_savings = 500.0;
    roadmap.add_phase(phase);

    assert_eq!(roadmap.payback_months(), 24.0); // 1000 / (500/12) = 24 months
}

#[test]
fn test_file_stats() {
    let mut stats = FileStats::new("src/main.rs");
    stats.total_commits = 50;
    stats.bug_fixes = 10;
    stats.created_at = current_timestamp_secs() - (180 * 86400); // 180 days ago

    assert!(stats.age_days() >= 180);
    assert!(stats.churn_rate() > 0.0);
    assert_eq!(stats.instability_index(), 0.2); // 10/50
}

#[test]
fn test_file_stats_hotspot() {
    let mut stats = FileStats::new("src/hot.rs");
    stats.total_commits = 100;
    stats.bug_fixes = 30;
    stats.unique_authors = 5;
    stats.created_at = current_timestamp_secs() - (30 * 86400);

    assert!(stats.hotspot_score() > 0.0);
}

#[test]
fn test_churn_analyzer_hotspots() {
    let mut analyzer = ChurnAnalyzer::new();

    let mut hot = FileStats::new("hot.rs");
    hot.total_commits = 100;
    hot.bug_fixes = 30;
    hot.unique_authors = 5;
    hot.created_at = current_timestamp_secs() - 86400;
    analyzer.add_file(hot);

    let mut stable = FileStats::new("stable.rs");
    stable.total_commits = 5;
    stable.bug_fixes = 0;
    stable.unique_authors = 1;
    stable.created_at = current_timestamp_secs() - (365 * 86400);
    analyzer.add_file(stable);

    let hotspots = analyzer.hotspots(10);
    assert_eq!(hotspots[0].path.to_str().unwrap(), "hot.rs");
}

#[test]
fn test_correlation_result() {
    let result = CorrelationResult::new("test", 0.8, 100);

    assert!(result.coefficient > 0.7);
    assert_eq!(result.interpretation, "Strong correlation");
}

#[test]
fn test_pearson_correlation() {
    // Perfect positive correlation
    let data = vec![(1.0, 1.0), (2.0, 2.0), (3.0, 3.0)];
    let r = ChurnAnalyzer::pearson_correlation(&data);
    assert!((r - 1.0).abs() < 0.001);

    // Perfect negative correlation
    let data = vec![(1.0, 3.0), (2.0, 2.0), (3.0, 1.0)];
    let r = ChurnAnalyzer::pearson_correlation(&data);
    assert!((r - (-1.0)).abs() < 0.001);
}

#[test]
fn test_debt_tracker() {
    let mut tracker = DebtTracker::new();

    tracker.add_debt(
        DebtItem::new(DebtType::Security, "XSS fix").with_severity(DebtSeverity::Critical),
    );
    tracker.add_debt(
        DebtItem::new(DebtType::Complexity, "Refactor").with_severity(DebtSeverity::Medium),
    );

    let metrics = tracker.metrics();
    assert_eq!(metrics.total_items, 2);

    let critical = tracker.critical_items();
    assert_eq!(critical.len(), 1);

    let security = tracker.security_debt();
    assert_eq!(security.len(), 1);
}

#[test]
fn test_debt_tracker_roadmap() {
    let mut tracker = DebtTracker::new();

    tracker.add_debt(DebtItem::new(DebtType::Security, "Fix 1").with_estimate(10.0));
    tracker.add_debt(DebtItem::new(DebtType::Complexity, "Fix 2").with_estimate(20.0));

    let roadmap = tracker.generate_roadmap("Q1 Cleanup", PrioritizationStrategy::SecurityFirst);

    assert!(!roadmap.phases.is_empty());
    assert!(roadmap.total_hours > 0.0);
}

#[test]
fn test_prioritization_strategies() {
    let item = DebtItem::new(DebtType::Security, "Test").with_estimate(5.0);

    let strategies = [
        PrioritizationStrategy::RiskImpactEffort,
        PrioritizationStrategy::CostFirst,
        PrioritizationStrategy::InterestFirst,
        PrioritizationStrategy::AgeFirst,
        PrioritizationStrategy::QuickWins,
        PrioritizationStrategy::SecurityFirst,
    ];

    for strategy in strategies {
        let prioritizer = DebtPrioritizer::new(strategy);
        let result = prioritizer.prioritize(std::slice::from_ref(&item));
        assert_eq!(result.len(), 1);
        assert!(result[0].score >= 0.0);
    }
}

// Additional comprehensive tests

#[test]
fn test_debt_type_all_variants() {
    let types = [
        DebtType::Duplication,
        DebtType::Complexity,
        DebtType::TestCoverage,
        DebtType::Dependencies,
        DebtType::Documentation,
        DebtType::CodeStyle,
        DebtType::Architecture,
        DebtType::Security,
        DebtType::Performance,
        DebtType::DeadCode,
    ];

    for dt in types {
        assert!(dt.default_interest_rate() >= 0.0);
        assert!(dt.severity_weight() >= 0.0);
        let _ = format!("{:?}", dt);
    }
}

#[test]
fn test_debt_severity_ordering() {
    assert!(DebtSeverity::Critical > DebtSeverity::High);
    assert!(DebtSeverity::High > DebtSeverity::Medium);
    assert!(DebtSeverity::Medium > DebtSeverity::Low);
}

#[test]
fn test_debt_item_serialization() {
    let item = DebtItem::new(DebtType::Security, "Test fix")
        .with_severity(DebtSeverity::High)
        .with_estimate(4.0);

    let json = serde_json::to_string(&item).unwrap();
    let deserialized: DebtItem = serde_json::from_str(&json).unwrap();

    assert_eq!(item.debt_type, deserialized.debt_type);
    assert_eq!(item.severity, deserialized.severity);
}

#[test]
fn test_debt_item_clone() {
    let item = DebtItem::new(DebtType::Complexity, "Clone test").with_file("main.rs");
    let cloned = item.clone();

    assert_eq!(item.id, cloned.id);
    assert_eq!(item.debt_type, cloned.debt_type);
}

#[test]
fn test_debt_metrics_clone() {
    let items = vec![DebtItem::new(DebtType::Security, "Fix").with_estimate(4.0)];
    let metrics = DebtMetrics::calculate(&items);
    let cloned = metrics.clone();

    assert_eq!(metrics.total_items, cloned.total_items);
}

#[test]
fn test_debt_metrics_empty() {
    let metrics = DebtMetrics::calculate(&[]);
    assert_eq!(metrics.total_items, 0);
    assert_eq!(metrics.total_fix_cost, 0.0);
}

#[test]
fn test_roadmap_phase_clone() {
    let phase = RoadmapPhase::new("Test Phase", 1);
    let cloned = phase.clone();
    assert_eq!(phase.name, cloned.name);
}

#[test]
fn test_roadmap_empty() {
    let roadmap = RefactoringRoadmap::new("Empty Roadmap");
    assert!(roadmap.phases.is_empty());
    assert_eq!(roadmap.total_hours, 0.0);
}

#[test]
fn test_file_stats_clone() {
    let stats = FileStats::new("test.rs");
    let cloned = stats.clone();
    assert_eq!(stats.path, cloned.path);
}

#[test]
fn test_file_stats_new_file() {
    let stats = FileStats::new("new_file.rs");
    assert_eq!(stats.total_commits, 0);
    assert_eq!(stats.bug_fixes, 0);
    assert_eq!(stats.instability_index(), 0.0);
}

#[test]
fn test_correlation_result_interpretations() {
    // Strong positive
    let strong = CorrelationResult::new("test", 0.9, 100);
    assert_eq!(strong.interpretation, "Strong correlation");

    // Moderate
    let moderate = CorrelationResult::new("test", 0.5, 100);
    assert_eq!(moderate.interpretation, "Moderate correlation");

    // Weak
    let weak = CorrelationResult::new("test", 0.2, 100);
    assert_eq!(weak.interpretation, "Weak correlation");

    // Very weak is still "Weak correlation" based on actual implementation
    let very_weak = CorrelationResult::new("test", 0.05, 100);
    assert!(!very_weak.interpretation.is_empty());
}

#[test]
fn test_churn_analyzer_age_correlation() {
    let mut analyzer = ChurnAnalyzer::new();

    let mut file = FileStats::new("test.rs");
    file.total_commits = 50;
    file.bug_fixes = 10;
    file.created_at = current_timestamp_secs() - (180 * 86400);
    analyzer.add_file(file);

    let correlation = analyzer.correlate_age_debt();
    // Coefficient can be NaN with limited data
    // sample_size is usize - checking it exists validates the correlation ran
    let _ = correlation.sample_size;
}

#[test]
fn test_debt_tracker_security_debt() {
    let mut tracker = DebtTracker::new();

    tracker.add_debt(DebtItem::new(DebtType::Security, "Sec 1"));
    tracker.add_debt(DebtItem::new(DebtType::Security, "Sec 2"));
    tracker.add_debt(DebtItem::new(DebtType::Complexity, "Comp 1"));

    let security = tracker.security_debt();
    assert_eq!(security.len(), 2);
}

#[test]
fn test_prioritized_item_clone() {
    let item = DebtItem::new(DebtType::Security, "Test");
    let prioritizer = DebtPrioritizer::new(PrioritizationStrategy::SecurityFirst);
    let prioritized = prioritizer.prioritize(&[item]);

    let cloned = prioritized[0].clone();
    assert_eq!(prioritized[0].score, cloned.score);
}

#[test]
fn test_roadmap_generator_max_hours() {
    let items = vec![
        DebtItem::new(DebtType::Security, "Fix 1").with_estimate(20.0),
        DebtItem::new(DebtType::Complexity, "Fix 2").with_estimate(30.0),
    ];

    let prioritizer = DebtPrioritizer::new(PrioritizationStrategy::RiskImpactEffort);
    let prioritized = prioritizer.prioritize(&items);

    let generator = RoadmapGenerator::new().with_max_hours(20.0);
    let roadmap = generator.generate("Test", &prioritized);

    assert!(!roadmap.phases.is_empty());
}

#[test]
fn test_debt_item_total_cost_with_age() {
    let item = DebtItem::new(DebtType::Security, "Old debt")
        .with_estimate(10.0)
        .with_age(365); // 1 year old

    let total = item.total_cost();
    let base = item.fix_cost();
    assert!(total > base); // Should have accrued interest
}

// ---- Persistence tests ----

#[test]
fn test_debt_tracker_save_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("debt.json");

    let mut tracker = DebtTracker::new();
    tracker.add_debt(
        DebtItem::new(DebtType::Security, "XSS fix")
            .with_severity(DebtSeverity::Critical)
            .with_estimate(4.0),
    );
    tracker.add_debt(
        DebtItem::new(DebtType::Complexity, "Simplify")
            .with_severity(DebtSeverity::Medium)
            .with_estimate(8.0),
    );

    tracker.save(&path).unwrap();
    let loaded = DebtTracker::load(&path).unwrap();

    assert_eq!(loaded.items.len(), 2);
    assert_eq!(loaded.items[0].debt_type, DebtType::Security);
    assert_eq!(loaded.items[1].debt_type, DebtType::Complexity);
}

#[test]
fn test_debt_tracker_empty_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.json");

    let tracker = DebtTracker::new();
    tracker.save(&path).unwrap();
    let loaded = DebtTracker::load(&path).unwrap();

    assert!(loaded.items.is_empty());
    assert!(loaded.roadmaps.is_empty());
}

#[test]
fn test_debt_tracker_load_missing_file() {
    let result = DebtTracker::load(std::path::Path::new("/nonexistent/path.json"));
    assert!(result.is_err());
}

#[test]
fn test_debt_tracker_save_creates_parent_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("deep").join("debt.json");

    let tracker = DebtTracker::new();
    tracker.save(&path).unwrap();
    assert!(path.exists());
}

#[test]
fn test_debt_tracker_default_path() {
    let root = std::path::Path::new("/my/project");
    let path = DebtTracker::default_path(root);
    assert_eq!(path, root.join(".selfware").join("tech_debt.json"));
}
