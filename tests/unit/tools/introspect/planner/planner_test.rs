use super::*;

#[test]
fn test_planning_strategy_detection() {
    let planner = EvolutionPlanner::new(
        "Optimize the agent loop".to_string(),
        20,
        10000,
        PathBuf::from("."),
    );

    let keywords = extract_keywords("Optimize the agent loop");
    let strategy = planner.determine_strategy(&keywords);

    // Should detect introspection-first for "optimize"
    match strategy {
        PlanningStrategy::IntrospectionFirst => (),
        _ => panic!("Expected IntrospectionFirst strategy"),
    }
}

#[test]
fn test_risk_assessment() {
    let planner = EvolutionPlanner::new("Fix bug".to_string(), 20, 10000, PathBuf::from("."));

    let phases = vec![];
    let files = vec![PathBuf::from("src/main.rs")];

    let risk = planner.assess_risk(&phases, &files);
    assert!(matches!(risk, RiskLevel::Low | RiskLevel::Medium));
}

// =========================================================================
// ActionType tests
// =========================================================================

#[test]
fn test_action_type_as_str_introspect() {
    assert_eq!(ActionType::Introspect.as_str(), "introspect");
}

#[test]
fn test_action_type_as_str_query() {
    assert_eq!(ActionType::Query.as_str(), "query");
}

#[test]
fn test_action_type_as_str_read() {
    assert_eq!(ActionType::Read.as_str(), "read");
}

#[test]
fn test_action_type_as_str_impact_analysis() {
    assert_eq!(ActionType::ImpactAnalysis.as_str(), "impact_analysis");
}

#[test]
fn test_action_type_as_str_modify() {
    assert_eq!(ActionType::Modify.as_str(), "modify");
}

#[test]
fn test_action_type_as_str_verify() {
    assert_eq!(ActionType::Verify.as_str(), "verify");
}

#[test]
fn test_action_type_as_str_test() {
    assert_eq!(ActionType::Test.as_str(), "test");
}

#[test]
fn test_action_type_as_str_complete() {
    assert_eq!(ActionType::Complete.as_str(), "complete");
}

// =========================================================================
// RiskLevel tests
// =========================================================================

#[test]
fn test_risk_level_as_str_low() {
    assert_eq!(RiskLevel::Low.as_str(), "low");
}

#[test]
fn test_risk_level_as_str_medium() {
    assert_eq!(RiskLevel::Medium.as_str(), "medium");
}

#[test]
fn test_risk_level_as_str_high() {
    assert_eq!(RiskLevel::High.as_str(), "high");
}

#[test]
fn test_risk_level_as_str_critical() {
    assert_eq!(RiskLevel::Critical.as_str(), "critical");
}

// =========================================================================
// ActionType serialization tests
// =========================================================================

#[test]
fn test_action_type_serde_roundtrip() {
    let actions = vec![
        ActionType::Introspect,
        ActionType::Query,
        ActionType::Read,
        ActionType::ImpactAnalysis,
        ActionType::Modify,
        ActionType::Verify,
        ActionType::Test,
        ActionType::Complete,
    ];
    for action in actions {
        let json = serde_json::to_string(&action).unwrap();
        let parsed: ActionType = serde_json::from_str(&json).unwrap();
        assert_eq!(action.as_str(), parsed.as_str());
    }
}

#[test]
fn test_risk_level_serde_roundtrip() {
    let levels = vec![
        RiskLevel::Low,
        RiskLevel::Medium,
        RiskLevel::High,
        RiskLevel::Critical,
    ];
    for level in levels {
        let json = serde_json::to_string(&level).unwrap();
        let parsed: RiskLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(level.as_str(), parsed.as_str());
    }
}

// =========================================================================
// determine_strategy tests
// =========================================================================

#[test]
fn test_strategy_fix_bug_is_targeted() {
    let planner = EvolutionPlanner::new(
        "Fix the failing test".to_string(),
        20,
        10000,
        PathBuf::from("."),
    );
    let keywords = extract_keywords("Fix the failing test");
    let strategy = planner.determine_strategy(&keywords);
    assert!(matches!(strategy, PlanningStrategy::TargetedChange));
}

#[test]
fn test_strategy_error_is_targeted() {
    let planner = EvolutionPlanner::new(
        "Resolve error in parser".to_string(),
        20,
        10000,
        PathBuf::from("."),
    );
    let keywords = extract_keywords("Resolve error in parser");
    let strategy = planner.determine_strategy(&keywords);
    assert!(matches!(strategy, PlanningStrategy::TargetedChange));
}

#[test]
fn test_strategy_refactor_is_introspection() {
    let planner = EvolutionPlanner::new(
        "Refactor the agent module".to_string(),
        20,
        10000,
        PathBuf::from("."),
    );
    let keywords = extract_keywords("Refactor the agent module");
    let strategy = planner.determine_strategy(&keywords);
    assert!(matches!(strategy, PlanningStrategy::IntrospectionFirst));
}

#[test]
fn test_strategy_improve_is_introspection() {
    let planner = EvolutionPlanner::new(
        "Improve error handling".to_string(),
        20,
        10000,
        PathBuf::from("."),
    );
    let keywords = extract_keywords("Improve error handling");
    let strategy = planner.determine_strategy(&keywords);
    assert!(matches!(strategy, PlanningStrategy::IntrospectionFirst));
}

#[test]
fn test_strategy_add_is_exploratory() {
    let planner = EvolutionPlanner::new(
        "Add new logging module".to_string(),
        20,
        10000,
        PathBuf::from("."),
    );
    let keywords = extract_keywords("Add new logging module");
    let strategy = planner.determine_strategy(&keywords);
    assert!(matches!(strategy, PlanningStrategy::Exploratory));
}

#[test]
fn test_strategy_implement_is_exploratory() {
    let planner = EvolutionPlanner::new(
        "Implement rate limiting".to_string(),
        20,
        10000,
        PathBuf::from("."),
    );
    let keywords = extract_keywords("Implement rate limiting");
    let strategy = planner.determine_strategy(&keywords);
    assert!(matches!(strategy, PlanningStrategy::Exploratory));
}

#[test]
fn test_strategy_understand_is_exploratory() {
    let planner = EvolutionPlanner::new(
        "understand the codebase".to_string(),
        20,
        10000,
        PathBuf::from("."),
    );
    let keywords = extract_keywords("understand the codebase");
    let strategy = planner.determine_strategy(&keywords);
    assert!(matches!(strategy, PlanningStrategy::Exploratory));
}

#[test]
fn test_strategy_default_is_introspection() {
    let planner = EvolutionPlanner::new(
        "do something general".to_string(),
        20,
        10000,
        PathBuf::from("."),
    );
    let keywords = extract_keywords("do something general");
    let strategy = planner.determine_strategy(&keywords);
    assert!(matches!(strategy, PlanningStrategy::IntrospectionFirst));
}

// =========================================================================
// assess_risk tests
// =========================================================================

#[test]
fn test_risk_safety_file_is_critical() {
    let planner = EvolutionPlanner::new("change safety".to_string(), 20, 10000, PathBuf::from("."));
    let files = vec![PathBuf::from("src/safety/checker.rs")];
    let risk = planner.assess_risk(&[], &files);
    assert!(matches!(risk, RiskLevel::Critical));
}

#[test]
fn test_risk_evolution_file_is_critical() {
    let planner = EvolutionPlanner::new("edit core".to_string(), 20, 10000, PathBuf::from("."));
    let files = vec![PathBuf::from("src/evolution/mod.rs")];
    let risk = planner.assess_risk(&[], &files);
    assert!(matches!(risk, RiskLevel::Critical));
}

#[test]
fn test_risk_many_files_is_high() {
    let planner = EvolutionPlanner::new("update docs".to_string(), 20, 10000, PathBuf::from("."));
    let files: Vec<PathBuf> = (0..15)
        .map(|i| PathBuf::from(format!("src/file{}.rs", i)))
        .collect();
    let risk = planner.assess_risk(&[], &files);
    assert!(matches!(risk, RiskLevel::High));
}

#[test]
fn test_risk_api_goal_is_high() {
    let planner = EvolutionPlanner::new(
        "change the public api".to_string(),
        20,
        10000,
        PathBuf::from("."),
    );
    let files = vec![PathBuf::from("src/lib.rs")];
    let risk = planner.assess_risk(&[], &files);
    assert!(matches!(risk, RiskLevel::High));
}

#[test]
fn test_risk_modify_is_medium() {
    let planner = EvolutionPlanner::new(
        "modify config loader".to_string(),
        20,
        10000,
        PathBuf::from("."),
    );
    let files = vec![PathBuf::from("src/config.rs")];
    let risk = planner.assess_risk(&[], &files);
    assert!(matches!(risk, RiskLevel::Medium));
}

#[test]
fn test_risk_simple_task_is_low() {
    let planner = EvolutionPlanner::new("read docs".to_string(), 20, 10000, PathBuf::from("."));
    let files = vec![PathBuf::from("README.md")];
    let risk = planner.assess_risk(&[], &files);
    assert!(matches!(risk, RiskLevel::Low));
}

// =========================================================================
// define_success_criteria tests
// =========================================================================

#[test]
fn test_success_criteria_always_has_compiles() {
    let planner = EvolutionPlanner::new("anything".to_string(), 20, 10000, PathBuf::from("."));
    let criteria = planner.define_success_criteria(&[]);
    assert!(criteria.iter().any(|c| c.contains("compiles")));
}

#[test]
fn test_success_criteria_always_has_tests_pass() {
    let planner = EvolutionPlanner::new("anything".to_string(), 20, 10000, PathBuf::from("."));
    let criteria = planner.define_success_criteria(&[]);
    assert!(criteria.iter().any(|c| c.contains("tests pass")));
}

#[test]
fn test_success_criteria_fix_adds_resolved() {
    let planner = EvolutionPlanner::new("fix bug".to_string(), 20, 10000, PathBuf::from("."));
    let criteria = planner.define_success_criteria(&["fix".to_string()]);
    assert!(criteria.iter().any(|c| c.contains("resolved")));
}

#[test]
fn test_success_criteria_optimize_adds_performance() {
    let planner = EvolutionPlanner::new("optimize".to_string(), 20, 10000, PathBuf::from("."));
    let criteria = planner.define_success_criteria(&["optimize".to_string()]);
    assert!(criteria.iter().any(|c| c.contains("Performance")));
}

#[test]
fn test_success_criteria_test_adds_coverage() {
    let planner = EvolutionPlanner::new("test".to_string(), 20, 10000, PathBuf::from("."));
    let criteria = planner.define_success_criteria(&["test".to_string()]);
    assert!(criteria.iter().any(|c| c.contains("cover")));
}

// =========================================================================
// is_source_file tests
// =========================================================================

#[test]
fn test_is_source_file_rs() {
    assert!(EvolutionPlanner::is_source_file(Path::new("src/main.rs")));
}

#[test]
fn test_is_source_file_py() {
    assert!(EvolutionPlanner::is_source_file(Path::new("script.py")));
}

#[test]
fn test_is_source_file_js() {
    assert!(EvolutionPlanner::is_source_file(Path::new("app.js")));
}

#[test]
fn test_is_source_file_ts() {
    assert!(EvolutionPlanner::is_source_file(Path::new("app.ts")));
}

#[test]
fn test_is_source_file_go() {
    assert!(EvolutionPlanner::is_source_file(Path::new("main.go")));
}

#[test]
fn test_is_source_file_java() {
    assert!(EvolutionPlanner::is_source_file(Path::new("App.java")));
}

#[test]
fn test_is_source_file_txt_not() {
    assert!(!EvolutionPlanner::is_source_file(Path::new("readme.txt")));
}

#[test]
fn test_is_source_file_toml_not() {
    assert!(!EvolutionPlanner::is_source_file(Path::new("Cargo.toml")));
}

#[test]
fn test_is_source_file_no_extension_not() {
    assert!(!EvolutionPlanner::is_source_file(Path::new("Makefile")));
}

// =========================================================================
// PlanPhase struct tests
// =========================================================================

#[test]
fn test_plan_phase_serialization() {
    let phase = PlanPhase {
        phase: 1,
        action: ActionType::Introspect,
        target: "src/main.rs".to_string(),
        params: HashMap::new(),
        reason: "Understand the codebase".to_string(),
        estimated_tokens: 1000,
        dependencies: vec![],
    };
    let json = serde_json::to_string(&phase).unwrap();
    assert!(json.contains("introspect"));
    assert!(json.contains("src/main.rs"));
}

#[test]
fn test_plan_phase_with_dependencies() {
    let phase = PlanPhase {
        phase: 3,
        action: ActionType::Modify,
        target: "src/lib.rs".to_string(),
        params: HashMap::from([("goal".to_string(), "add feature".to_string())]),
        reason: "Apply changes".to_string(),
        estimated_tokens: 2000,
        dependencies: vec![1, 2],
    };
    assert_eq!(phase.dependencies, vec![1, 2]);
}

// =========================================================================
// EvolutionPlan struct tests
// =========================================================================

#[test]
fn test_evolution_plan_serialization() {
    let plan = EvolutionPlan {
        goal: "Test goal".to_string(),
        phases: vec![],
        estimated_tokens: 0,
        token_budget: 10000,
        iteration_budget: 20,
        risk: RiskLevel::Low,
        success_criteria: vec!["Code compiles".to_string()],
    };
    let json = serde_json::to_string(&plan).unwrap();
    assert!(json.contains("Test goal"));
    assert!(json.contains("low"));
}

// =========================================================================
// ImpactAnalysis / CallerInfo struct tests
// =========================================================================

#[test]
fn test_caller_info_serialization() {
    let info = CallerInfo {
        file: "src/main.rs".to_string(),
        line: 42,
        context: "References config".to_string(),
    };
    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("src/main.rs"));
    assert!(json.contains("42"));
}

#[test]
fn test_impact_analysis_serialization() {
    let analysis = ImpactAnalysis {
        target_file: "src/lib.rs".to_string(),
        target_symbol: Some("my_fn".to_string()),
        direct_callers: vec![],
        transitive_deps: vec![],
        tests_affected: vec!["test_my_fn".to_string()],
        estimated_files_to_update: 0,
        suggested_order: vec!["Update callers".to_string()],
    };
    let json = serde_json::to_string(&analysis).unwrap();
    assert!(json.contains("src/lib.rs"));
    assert!(json.contains("my_fn"));
    assert!(json.contains("test_my_fn"));
}
