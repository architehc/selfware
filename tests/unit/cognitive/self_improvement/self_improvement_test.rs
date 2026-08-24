use super::*;

#[test]
fn test_outcome_score() {
    assert_eq!(Outcome::Success.score(), 1.0);
    assert_eq!(Outcome::Partial.score(), 0.5);
    assert_eq!(Outcome::Failure.score(), 0.0);
}

#[test]
fn test_outcome_is_positive() {
    assert!(Outcome::Success.is_positive());
    assert!(Outcome::Partial.is_positive());
    assert!(!Outcome::Failure.is_positive());
    assert!(!Outcome::Abandoned.is_positive());
}

#[test]
fn test_prompt_record_new() {
    let record = PromptRecord::new(
        "test prompt".to_string(),
        "code_write".to_string(),
        Outcome::Success,
    );
    assert_eq!(record.quality_score, 1.0);
    assert!(record.timestamp > 0);
}

#[test]
fn test_prompt_record_with_quality() {
    let record = PromptRecord::new("test".to_string(), "code".to_string(), Outcome::Partial)
        .with_quality(0.8);
    assert_eq!(record.quality_score, 0.8);
}

#[test]
fn test_prompt_pattern_new() {
    let pattern = PromptPattern::new("p1", "Please {action} the {target}");
    assert_eq!(pattern.id, "p1");
    assert_eq!(pattern.usage_count, 0);
}

#[test]
fn test_prompt_pattern_update() {
    let mut pattern = PromptPattern::new("p1", "template");
    pattern.update(Outcome::Success, 0.9);
    pattern.update(Outcome::Failure, 0.2);

    assert_eq!(pattern.usage_count, 2);
    assert_eq!(pattern.success_rate, 0.5);
}

#[test]
fn test_prompt_optimizer_new() {
    let optimizer = PromptOptimizer::new();
    assert_eq!(optimizer.get_stats().total_records, 0);
}

#[test]
fn test_prompt_optimizer_record() {
    let mut optimizer = PromptOptimizer::new();
    optimizer.record(PromptRecord::new(
        "test".to_string(),
        "code".to_string(),
        Outcome::Success,
    ));
    assert_eq!(optimizer.get_stats().total_records, 1);
}

#[test]
fn test_prompt_optimizer_suggest_improvements() {
    let optimizer = PromptOptimizer::new();
    let suggestions = optimizer.suggest_improvements("x", "code");
    assert!(!suggestions.is_empty()); // Should suggest adding context
}

#[test]
fn test_prompt_optimizer_evolve_prompt() {
    let mut optimizer = PromptOptimizer::new();
    for _ in 0..4 {
        optimizer.record(
            PromptRecord::new(
                "Use a step-by-step plan and verify output".to_string(),
                "system_prompt".to_string(),
                Outcome::Success,
            )
            .with_quality(0.9),
        );
    }

    let result = optimizer.evolve_prompt("system_prompt", "You are a coding agent.");
    assert!(!result.variants.is_empty());
    assert!(result.winner_score >= 0.0);
    assert!(!result.winner_prompt.is_empty());
}

#[test]
fn test_tool_usage_record_new() {
    let record = ToolUsageRecord::new(
        "file_read".to_string(),
        "reading config".to_string(),
        Outcome::Success,
    );
    assert_eq!(record.tool, "file_read");
    assert!(record.error.is_none());
}

#[test]
fn test_tool_stats_success_rate() {
    let stats = ToolStats {
        usage_count: 10,
        success_count: 8,
        ..Default::default()
    };
    assert_eq!(stats.success_rate(), 0.8);
}

#[test]
fn test_tool_selection_learner_new() {
    let learner = ToolSelectionLearner::new();
    assert_eq!(learner.get_stats().total_records, 0);
}

#[test]
fn test_tool_selection_learner_record() {
    let mut learner = ToolSelectionLearner::new();
    learner.record(ToolUsageRecord::new(
        "file_read".to_string(),
        "reading file".to_string(),
        Outcome::Success,
    ));
    assert_eq!(learner.get_stats().total_records, 1);
    assert_eq!(learner.get_stats().unique_tools, 1);
}

#[test]
fn test_tool_selection_learner_best_tools() {
    let mut learner = ToolSelectionLearner::new();
    for _ in 0..5 {
        learner.record(ToolUsageRecord::new(
            "file_read".to_string(),
            "reading".to_string(),
            Outcome::Success,
        ));
    }
    for _ in 0..3 {
        learner.record(ToolUsageRecord::new(
            "file_write".to_string(),
            "writing".to_string(),
            Outcome::Failure,
        ));
    }

    let best = learner.best_tools_for("reading");
    assert!(!best.is_empty());
}

#[test]
fn test_error_record_new() {
    let record = ErrorRecord::new(
        "file not found".to_string(),
        "io_error".to_string(),
        "loading config".to_string(),
        "file_read".to_string(),
    );
    assert!(!record.recovered);
}

#[test]
fn test_error_record_with_recovery() {
    let record = ErrorRecord::new(
        "error".to_string(),
        "type".to_string(),
        "ctx".to_string(),
        "action".to_string(),
    )
    .with_recovery("retry".to_string());
    assert!(record.recovered);
    assert_eq!(record.recovery_action, Some("retry".to_string()));
}

#[test]
fn test_error_pattern_new() {
    let pattern = ErrorPattern::new("p1", "io_error");
    assert_eq!(pattern.count, 0);
}

#[test]
fn test_error_pattern_update() {
    let mut pattern = ErrorPattern::new("p1", "io_error");
    let record = ErrorRecord::new(
        "error".to_string(),
        "io_error".to_string(),
        "context".to_string(),
        "action".to_string(),
    );
    pattern.update(&record);
    assert_eq!(pattern.count, 1);
    assert!(pattern.contexts.contains(&"context".to_string()));
}

#[test]
fn test_error_pattern_learner_new() {
    let learner = ErrorPatternLearner::new();
    assert_eq!(learner.get_stats().total_errors, 0);
}

#[test]
fn test_error_pattern_learner_record() {
    let mut learner = ErrorPatternLearner::new();
    learner.record(ErrorRecord::new(
        "error".to_string(),
        "type".to_string(),
        "ctx".to_string(),
        "action".to_string(),
    ));
    assert_eq!(learner.get_stats().total_errors, 1);
}

#[test]
fn test_error_pattern_learner_might_trigger() {
    let mut learner = ErrorPatternLearner::new();
    learner.record(ErrorRecord::new(
        "file not found".to_string(),
        "io_error".to_string(),
        "loading config".to_string(),
        "file_read".to_string(),
    ));

    let warnings = learner.might_trigger_error("file_read", "loading");
    assert!(!warnings.is_empty());
}

#[test]
fn test_usage_session_new() {
    let session = UsageSession::new("s1");
    assert_eq!(session.id, "s1");
    assert!(session.end_time.is_none());
}

#[test]
fn test_usage_session_end() {
    let mut session = UsageSession::new("s1");
    session.end();
    assert!(session.end_time.is_some());
}

#[test]
fn test_usage_session_completion_rate() {
    let mut session = UsageSession::new("s1");
    session.tasks_attempted = 10;
    session.tasks_completed = 8;
    assert_eq!(session.completion_rate(), 0.8);
}

#[test]
fn test_usage_analyzer_new() {
    let analyzer = UsageAnalyzer::new();
    assert_eq!(analyzer.get_stats().total_sessions, 0);
}

#[test]
fn test_usage_analyzer_session() {
    let mut analyzer = UsageAnalyzer::new();
    analyzer.start_session("s1");
    analyzer.record_task_attempt(true);
    analyzer.record_tool_usage("file_read");
    analyzer.end_session(Some(0.9));

    let stats = analyzer.get_stats();
    assert_eq!(stats.total_sessions, 1);
    assert_eq!(stats.completed_tasks, 1);
}

#[test]
fn test_usage_analyzer_most_used_tools() {
    let mut analyzer = UsageAnalyzer::new();
    for _ in 0..5 {
        analyzer.record_tool_usage("file_read");
    }
    for _ in 0..3 {
        analyzer.record_tool_usage("file_write");
    }

    let tools = analyzer.most_used_tools(2);
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].0, "file_read");
}

#[test]
fn test_self_improvement_engine_new() {
    let engine = SelfImprovementEngine::new();
    assert!(engine.learning_enabled);
}

#[test]
fn test_self_improvement_engine_record_prompt() {
    let engine = SelfImprovementEngine::new();
    engine.record_prompt("test prompt", "code", Outcome::Success, 0.9);

    let stats = engine.get_stats();
    assert!(stats.prompt_stats.is_some());
}

#[test]
fn test_self_improvement_engine_record_tool() {
    let engine = SelfImprovementEngine::new();
    engine.record_tool("file_read", "reading config", Outcome::Success, 100, None);

    let stats = engine.get_stats();
    assert!(stats.tool_stats.is_some());
}

#[test]
fn test_self_improvement_engine_evolve_prompt() {
    let engine = SelfImprovementEngine::new();
    let result = engine.evolve_prompt("You are Selfware.", "system_prompt");
    assert!(result.is_some());
    let result = result.unwrap();
    assert!(!result.winner_prompt.is_empty());
    assert!(!result.variants.is_empty());
}

#[test]
fn test_self_improvement_engine_record_error() {
    let engine = SelfImprovementEngine::new();
    engine.record_error(
        "error msg",
        "io_error",
        "context",
        "action",
        Some("retry".to_string()),
    );

    let stats = engine.get_stats();
    assert!(stats.error_stats.is_some());
}

#[test]
fn test_self_improvement_engine_best_tools() {
    let engine = SelfImprovementEngine::new();
    for _ in 0..5 {
        engine.record_tool("file_read", "reading", Outcome::Success, 100, None);
    }

    let best = engine.best_tools_for("reading");
    assert!(!best.is_empty());
}

#[test]
fn test_self_improvement_engine_check_errors() {
    let engine = SelfImprovementEngine::new();
    engine.record_error("file not found", "io_error", "loading", "file_read", None);

    let warnings = engine.check_for_errors("file_read", "loading");
    assert!(!warnings.is_empty());
}

#[test]
fn test_self_improvement_engine_session() {
    let engine = SelfImprovementEngine::new();
    engine.start_session("s1");
    engine.record_task(true);
    engine.end_session(Some(0.9));

    let stats = engine.get_stats();
    assert!(stats.usage_stats.is_some());
}

#[test]
fn test_self_improvement_engine_disable_learning() {
    let mut engine = SelfImprovementEngine::new();
    engine.set_learning_enabled(false);
    engine.record_prompt("test", "code", Outcome::Success, 1.0);

    let stats = engine.get_stats();
    // Stats should still be accessible but empty
    assert!(stats.prompt_stats.unwrap().total_records == 0);
}

#[test]
fn test_self_improvement_engine_save_load_roundtrip() {
    let engine = SelfImprovementEngine::new();
    engine.record_prompt("test prompt", "code", Outcome::Success, 0.9);
    engine.record_tool("file_read", "reading config", Outcome::Success, 100, None);
    engine.record_error("file not found", "io_error", "loading", "file_read", None);
    engine.start_session("s1");
    engine.record_task(true);

    let tmp = std::env::temp_dir().join("selfware_test_engine.json");
    engine.save(&tmp).unwrap();

    let loaded = SelfImprovementEngine::load(&tmp).unwrap();
    let stats = loaded.get_stats();
    assert_eq!(stats.prompt_stats.unwrap().total_records, 1);
    assert_eq!(stats.tool_stats.unwrap().total_records, 1);
    assert_eq!(stats.error_stats.unwrap().total_errors, 1);

    std::fs::remove_file(&tmp).ok();
}

#[test]
fn test_save_load_preserves_tool_stats() {
    let engine = SelfImprovementEngine::new();
    for _ in 0..5 {
        engine.record_tool("file_read", "reading", Outcome::Success, 50, None);
    }
    for _ in 0..3 {
        engine.record_tool(
            "file_write",
            "writing",
            Outcome::Failure,
            100,
            Some("permission denied".to_string()),
        );
    }

    let tmp = std::env::temp_dir().join("selfware_test_engine_tools.json");
    engine.save(&tmp).unwrap();
    let loaded = SelfImprovementEngine::load(&tmp).unwrap();

    let best = loaded.best_tools_for("reading");
    assert!(!best.is_empty());
    // file_read should rank higher than file_write
    let file_read_score = best.iter().find(|(t, _)| t == "file_read").map(|(_, s)| *s);
    assert!(file_read_score.is_some());

    std::fs::remove_file(&tmp).ok();
}

#[test]
fn test_save_load_preserves_error_patterns() {
    let engine = SelfImprovementEngine::new();
    engine.record_error("timeout waiting", "timeout", "api_call", "shell_exec", None);
    engine.record_error(
        "timeout waiting",
        "timeout",
        "api_call",
        "shell_exec",
        Some("retry".to_string()),
    );

    let tmp = std::env::temp_dir().join("selfware_test_engine_errors.json");
    engine.save(&tmp).unwrap();
    let loaded = SelfImprovementEngine::load(&tmp).unwrap();

    let warnings = loaded.check_for_errors("shell_exec", "api_call");
    assert!(!warnings.is_empty());

    std::fs::remove_file(&tmp).ok();
}

#[test]
fn test_save_load_preserves_usage_sessions() {
    let engine = SelfImprovementEngine::new();
    engine.start_session("s1");
    engine.record_task(true);
    engine.record_task(false);
    engine.end_session(Some(0.7));

    let tmp = std::env::temp_dir().join("selfware_test_engine_sessions.json");
    engine.save(&tmp).unwrap();
    let loaded = SelfImprovementEngine::load(&tmp).unwrap();

    let stats = loaded.get_stats();
    let usage = stats.usage_stats.unwrap();
    assert_eq!(usage.total_sessions, 1);
    assert_eq!(usage.total_tasks, 2);
    assert_eq!(usage.completed_tasks, 1);

    std::fs::remove_file(&tmp).ok();
}

#[test]
fn test_load_nonexistent_file_errors() {
    let result = SelfImprovementEngine::load(std::path::Path::new(
        "/tmp/selfware_nonexistent_engine_12345.json",
    ));
    assert!(result.is_err());
}

#[test]
fn test_save_creates_parent_dirs() {
    let tmp = std::env::temp_dir().join("selfware_test_nested/deep/dir/engine.json");
    // Clean up first
    std::fs::remove_dir_all(std::env::temp_dir().join("selfware_test_nested")).ok();

    let engine = SelfImprovementEngine::new();
    engine.save(&tmp).unwrap();
    assert!(tmp.exists());

    std::fs::remove_dir_all(std::env::temp_dir().join("selfware_test_nested")).ok();
}

#[test]
fn test_outcome_serialization_roundtrip() {
    for outcome in [
        Outcome::Success,
        Outcome::Partial,
        Outcome::Failure,
        Outcome::Abandoned,
    ] {
        let json = serde_json::to_string(&outcome).unwrap();
        let deserialized: Outcome = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, outcome);
    }
}

#[test]
fn test_prompt_record_serialization_roundtrip() {
    let record = PromptRecord::new(
        "test prompt".to_string(),
        "code".to_string(),
        Outcome::Success,
    )
    .with_quality(0.85)
    .with_tokens(1500)
    .with_response_time(2000);
    let json = serde_json::to_string(&record).unwrap();
    let deserialized: PromptRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.prompt, "test prompt");
    assert_eq!(deserialized.quality_score, 0.85);
    assert_eq!(deserialized.tokens_used, 1500);
    assert_eq!(deserialized.response_time_ms, 2000);
}

#[test]
fn test_tool_usage_record_serialization_roundtrip() {
    let record = ToolUsageRecord::new(
        "cargo_check".to_string(),
        "building".to_string(),
        Outcome::Failure,
    )
    .with_execution_time(5000)
    .with_error("compilation error".to_string());
    let json = serde_json::to_string(&record).unwrap();
    let deserialized: ToolUsageRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.tool, "cargo_check");
    assert_eq!(deserialized.outcome, Outcome::Failure);
    assert_eq!(deserialized.execution_time_ms, 5000);
    assert_eq!(deserialized.error, Some("compilation error".to_string()));
}

#[test]
fn test_error_record_serialization_roundtrip() {
    let record = ErrorRecord::new(
        "file not found".to_string(),
        "io_error".to_string(),
        "loading config".to_string(),
        "file_read".to_string(),
    )
    .with_recovery("use default".to_string());
    let json = serde_json::to_string(&record).unwrap();
    let deserialized: ErrorRecord = serde_json::from_str(&json).unwrap();
    assert!(deserialized.recovered);
    assert_eq!(
        deserialized.recovery_action,
        Some("use default".to_string())
    );
}

#[test]
fn test_usage_session_zero_tasks_completion_rate() {
    let session = UsageSession::new("s1");
    assert_eq!(session.completion_rate(), 0.0);
}

#[test]
fn test_usage_session_serialization_roundtrip() {
    let mut session = UsageSession::new("s1");
    session.tasks_attempted = 5;
    session.tasks_completed = 3;
    session.tools_used = vec!["file_read".to_string(), "shell_exec".to_string()];
    session.end();

    let json = serde_json::to_string(&session).unwrap();
    let deserialized: UsageSession = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, "s1");
    assert!(deserialized.end_time.is_some());
    assert_eq!(deserialized.tools_used.len(), 2);
}

#[test]
fn test_tool_stats_serialization_roundtrip() {
    let stats = ToolStats {
        usage_count: 10,
        success_count: 8,
        failure_count: 2,
        avg_execution_time_ms: 150.0,
        effective_contexts: vec!["reading files".to_string()],
        common_errors: HashMap::from([("permission denied".to_string(), 2)]),
    };
    let json = serde_json::to_string(&stats).unwrap();
    let deserialized: ToolStats = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.usage_count, 10);
    assert_eq!(deserialized.success_rate(), 0.8);
    assert_eq!(deserialized.common_errors.len(), 1);
}

#[test]
fn test_error_warning_serialization_roundtrip() {
    let warning = ErrorWarning {
        pattern_id: "p1".to_string(),
        error_type: "timeout".to_string(),
        likelihood: 0.8,
        prevention: vec!["set longer timeout".to_string()],
        recovery: vec!["retry".to_string()],
    };
    let json = serde_json::to_string(&warning).unwrap();
    let deserialized: ErrorWarning = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.pattern_id, "p1");
    assert_eq!(deserialized.likelihood, 0.8);
}

#[test]
fn test_error_pattern_serialization_roundtrip() {
    let mut pattern = ErrorPattern::new("p1", "io_error");
    let record = ErrorRecord::new(
        "not found".to_string(),
        "io_error".to_string(),
        "context".to_string(),
        "action".to_string(),
    );
    pattern.update(&record);
    pattern.add_prevention("check existence first".to_string());

    let json = serde_json::to_string(&pattern).unwrap();
    let deserialized: ErrorPattern = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.count, 1);
    assert_eq!(deserialized.prevention.len(), 1);
}

#[test]
fn test_suggest_improvements_short_prompt() {
    let engine = SelfImprovementEngine::new();
    let suggestions = engine.suggest_prompt_improvements("x", "code");
    assert!(!suggestions.is_empty());
    assert!(suggestions
        .iter()
        .any(|s| s.suggestion_type == SuggestionType::AddContext));
}

#[test]
fn test_tool_selection_learner_common_errors() {
    let mut learner = ToolSelectionLearner::new();
    learner.record(
        ToolUsageRecord::new(
            "shell_exec".to_string(),
            "running".to_string(),
            Outcome::Failure,
        )
        .with_error("permission denied".to_string()),
    );
    learner.record(
        ToolUsageRecord::new(
            "shell_exec".to_string(),
            "running".to_string(),
            Outcome::Failure,
        )
        .with_error("permission denied".to_string()),
    );
    let errors = learner.common_errors_for("shell_exec");
    assert!(!errors.is_empty());
    assert!(errors[0].1 >= 2);
}

#[test]
fn test_tool_selection_learner_no_stats() {
    let learner = ToolSelectionLearner::new();
    assert!(learner.get_tool_stats("nonexistent").is_none());
    assert!(learner.common_errors_for("nonexistent").is_empty());
}

#[test]
fn test_usage_analyzer_multiple_sessions() {
    let mut analyzer = UsageAnalyzer::new();

    analyzer.start_session("s1");
    analyzer.record_task_attempt(true);
    analyzer.record_tool_usage("file_read");
    analyzer.end_session(Some(0.8));

    analyzer.start_session("s2");
    analyzer.record_task_attempt(false);
    analyzer.record_error();
    analyzer.end_session(Some(0.5));

    let stats = analyzer.get_stats();
    assert_eq!(stats.total_sessions, 2);
    assert_eq!(stats.total_tasks, 2);
    assert_eq!(stats.completed_tasks, 1);
    assert_eq!(stats.total_errors, 1);
}

#[test]
fn test_prompt_optimizer_best_patterns() {
    let mut optimizer = PromptOptimizer::new();
    let mut pattern = PromptPattern::new("p1", "Step by step: {action}");
    pattern.effective_for = vec!["code".to_string()];
    // Need 5+ usages to be considered
    for _ in 0..6 {
        pattern.update(Outcome::Success, 0.9);
    }
    optimizer.register_pattern(pattern);

    let best = optimizer.best_patterns_for("code");
    assert_eq!(best.len(), 1);
    assert_eq!(best[0].id, "p1");
}

#[test]
#[cfg(unix)]
fn save_writes_owner_only_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("selfware_priv_test_{}", std::process::id()));
    let path = dir.join("improvement_engine.json");
    let engine = SelfImprovementEngine::new();
    engine.save(&path).unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o600,
        "learning file must be owner-only, got {:o}",
        mode
    );
    // Round-trips.
    let _loaded = SelfImprovementEngine::load(&path).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[cfg(unix)]
fn load_migrates_legacy_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("selfware_migrate_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("improvement_engine.json");
    SelfImprovementEngine::new().save(&path).unwrap();
    // Simulate a legacy world-readable file.
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o644
    );
    // Loading must migrate it to owner-only.
    let _ = SelfImprovementEngine::load(&path).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600,
        "load must tighten a legacy 0644 learning file to 0600"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// --- Honesty batch (GLM 5.3 evolution review of self_improvement, 2026-08-23) ---

#[test]
fn evolved_pattern_is_unverified_candidate_not_fabricated_evidence() {
    let mut optimizer = PromptOptimizer::new();
    let result = optimizer.evolve_prompt("system_prompt", "You are a coding agent.");

    // The tournament ranks by structural priors only — nothing is executed —
    // so the winner must be registered as an unverified candidate with zero
    // observed usage, NOT as a learned pattern with fabricated quality and
    // success_rate copied from the predicted score.
    let candidates: Vec<_> = optimizer
        .patterns
        .values()
        .filter(|p| p.id.starts_with("evo-system_prompt-"))
        .collect();
    assert_eq!(candidates.len(), 1, "winner candidate is registered once");
    let pattern = candidates[0];
    assert_eq!(
        pattern.usage_count, 0,
        "no executions happened — usage_count must be 0, got {}",
        pattern.usage_count
    );
    assert_eq!(
        pattern.success_rate, 0.0,
        "success_rate must be unknown (0), not the predicted prior"
    );
    assert_eq!(
        pattern.avg_quality, 0.0,
        "avg_quality must be unknown (0), not the predicted prior"
    );

    // And it must NOT be recommendable until real observations arrive
    // (best_patterns_for requires usage_count >= 5).
    assert!(
        optimizer.best_patterns_for("system_prompt").is_empty(),
        "an unverified candidate must not be recommended"
    );
    assert!(!result.variants.is_empty());
}

#[test]
fn pattern_update_counts_successes_exactly() {
    let mut p = PromptPattern::new("p", "t");
    p.update(Outcome::Success, 0.9);
    p.update(Outcome::Failure, 0.2);
    p.update(Outcome::Success, 0.8);
    assert_eq!(p.usage_count, 3);
    assert_eq!(
        p.successful_uses, 2,
        "exact counter, not a rate*count reconstruction with round() drift"
    );
    assert!((p.success_rate - 2.0 / 3.0).abs() < 1e-6);
    assert!((p.avg_quality - (0.9 + 0.2 + 0.8) / 3.0).abs() < 1e-4);
}

#[test]
fn suggest_improvements_does_not_claim_task_scoped_rate() {
    let mut optimizer = PromptOptimizer::new();
    let mut pattern = PromptPattern::new("p1", "Step by step: {action}");
    pattern.effective_for = vec!["code".to_string()];
    for _ in 0..6 {
        pattern.update(Outcome::Success, 0.9);
    }
    optimizer.register_pattern(pattern);

    let suggestions = optimizer.suggest_improvements("please do the thing you must", "code");
    let desc = &suggestions
        .iter()
        .find(|s| matches!(s.suggestion_type, SuggestionType::UsePattern))
        .expect("pattern suggestion present")
        .description;
    assert!(
        !desc.contains("for this task type"),
        "success_rate is the pattern's GLOBAL rate — the wording must not claim task scoping: {desc}"
    );
    assert!(
        desc.contains("recorded uses"),
        "honest wording names the observation count: {desc}"
    );
}

fn letter_salt(mut n: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'a' + (n % 26) as u8) as char);
        n /= 26;
        if n == 0 {
            break;
        }
    }
    s
}

#[test]
fn common_errors_map_is_bounded() {
    let mut learner = ToolSelectionLearner::new();
    for i in 0..600 {
        let mut rec =
            ToolUsageRecord::new("file_read".to_string(), "ctx".to_string(), Outcome::Failure);
        // Alphabetic salt survives digit normalization, giving distinct keys.
        rec.error = Some(format!("error alpha {} beta", letter_salt(i)));
        learner.record(rec);
    }
    let stats = learner.get_tool_stats("file_read").expect("stats exist");
    assert!(
        stats.common_errors.len() <= 256,
        "common_errors must stay bounded, got {}",
        stats.common_errors.len()
    );
}

#[test]
fn first_observation_shrinks_toward_prior() {
    let mut learner = ToolSelectionLearner::new();
    // Proven tool: a long track record of successes in this context.
    for _ in 0..20 {
        learner.record(ToolUsageRecord::new(
            "proven".to_string(),
            "fix the bug".to_string(),
            Outcome::Success,
        ));
    }
    // New tool: a single lucky success in the same context.
    learner.record(ToolUsageRecord::new(
        "lucky".to_string(),
        "fix the bug".to_string(),
        Outcome::Success,
    ));

    let best = learner.best_tools_for("fix the bug");
    let proven = best.iter().find(|(t, _)| t == "proven").unwrap().1;
    let lucky = best.iter().find(|(t, _)| t == "lucky").unwrap().1;
    assert!(
        lucky < proven,
        "one-shot success ({lucky}) must not outrank a proven record ({proven})"
    );
}
