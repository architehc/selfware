use super::*;

#[test]
fn test_improvement_target_new() {
    let target = ImprovementTarget::new(
        ImprovementCategory::ErrorHandling,
        "Add retry logic to API calls",
        "API calls sometimes fail transiently",
        ImprovementSource::ErrorPattern,
    );
    assert_eq!(target.category, ImprovementCategory::ErrorHandling);
    assert_eq!(target.status, ImprovementStatus::Proposed);
    assert!(target.id.starts_with("imp-"));
}

#[test]
fn test_improvement_target_with_scores() {
    let target = ImprovementTarget::new(
        ImprovementCategory::CodeQuality,
        "desc",
        "rationale",
        ImprovementSource::TechDebt,
    )
    .with_scores(0.8, 0.9);
    assert!((target.priority - 0.72).abs() < 0.001);
}

#[test]
fn test_deny_list() {
    let orchestrator = SelfEditOrchestrator::new(PathBuf::from("/tmp/selfware_test"));
    let target = ImprovementTarget::new(
        ImprovementCategory::CodeQuality,
        "edit safety",
        "reason",
        ImprovementSource::CodeSmell,
    )
    .with_file("src/safety/checker.rs");
    assert!(orchestrator.is_denied(&target));

    let safe_target = ImprovementTarget::new(
        ImprovementCategory::CodeQuality,
        "edit tools",
        "reason",
        ImprovementSource::CodeSmell,
    )
    .with_file("src/tools/file_ops.rs");
    assert!(!orchestrator.is_denied(&safe_target));
}

#[test]
fn test_build_improvement_prompt() {
    let orchestrator = SelfEditOrchestrator::new(PathBuf::from("/tmp/selfware_test"));
    let target = ImprovementTarget::new(
        ImprovementCategory::ErrorHandling,
        "Add retry logic",
        "Transient failures",
        ImprovementSource::ErrorPattern,
    )
    .with_file("src/api/client.rs");
    let prompt = orchestrator.build_improvement_prompt(&target);
    assert!(prompt.contains("Add retry logic"));
    assert!(prompt.contains("cargo check"));
}

#[test]
fn test_evaluate_effectiveness() {
    let before = PerformanceSnapshot::from_checkpoint_data(10, 20, 5, 2, false, 10000, false);
    let after = PerformanceSnapshot::from_checkpoint_data(5, 10, 2, 2, true, 5000, true);
    let score = SelfEditOrchestrator::evaluate(&before, &after);
    assert!(score > 0.0);
}

#[test]
fn test_improvement_target_with_file() {
    let target = ImprovementTarget::new(
        ImprovementCategory::ToolPipeline,
        "desc",
        "rationale",
        ImprovementSource::CodeSmell,
    )
    .with_file("src/tools/registry.rs");
    assert_eq!(target.file, Some("src/tools/registry.rs".to_string()));
}

#[test]
fn test_improvement_target_scores_clamped() {
    let target = ImprovementTarget::new(
        ImprovementCategory::CodeQuality,
        "d",
        "r",
        ImprovementSource::TechDebt,
    )
    .with_scores(1.5, -0.2); // out of range
    assert_eq!(target.impact, 1.0);
    assert_eq!(target.confidence, 0.0);
    assert_eq!(target.priority, 0.0); // 1.0 * 0.0
}

#[test]
fn test_improvement_category_display() {
    assert_eq!(
        format!("{}", ImprovementCategory::PromptTemplate),
        "prompt_template"
    );
    assert_eq!(
        format!("{}", ImprovementCategory::ErrorHandling),
        "error_handling"
    );
    assert_eq!(
        format!("{}", ImprovementCategory::NewCapability),
        "new_capability"
    );
}

#[test]
fn test_deny_list_all_patterns() {
    let orchestrator = SelfEditOrchestrator::new(PathBuf::from("/tmp/selfware_test"));

    let make_target = |file: &str| {
        ImprovementTarget::new(
            ImprovementCategory::CodeQuality,
            "d",
            "r",
            ImprovementSource::CodeSmell,
        )
        .with_file(file)
    };

    // All denied patterns
    assert!(orchestrator.is_denied(&make_target("src/safety/checker.rs")));
    assert!(orchestrator.is_denied(&make_target("src/safety/path_validator.rs")));
    assert!(orchestrator.is_denied(&make_target("Cargo.toml")));
    assert!(orchestrator.is_denied(&make_target(".github/workflows/ci.yml")));
    assert!(orchestrator.is_denied(&make_target("src/main.rs")));

    // Not denied
    assert!(!orchestrator.is_denied(&make_target("src/agent/mod.rs")));
    assert!(!orchestrator.is_denied(&make_target("src/cognitive/metrics.rs")));

    // No file — not denied
    let no_file = ImprovementTarget::new(
        ImprovementCategory::CodeQuality,
        "d",
        "r",
        ImprovementSource::CodeSmell,
    );
    assert!(!orchestrator.is_denied(&no_file));
}

#[test]
fn test_select_target_returns_first() {
    let orchestrator = SelfEditOrchestrator::new(PathBuf::from("/tmp/selfware_test"));
    let targets = vec![
        ImprovementTarget::new(
            ImprovementCategory::CodeQuality,
            "Address TODO at src/lib.rs:2: // TODO: first",
            "r",
            ImprovementSource::TechDebt,
        )
        .with_file("src/lib.rs")
        .with_scores(0.9, 0.9),
        ImprovementTarget::new(
            ImprovementCategory::CodeQuality,
            "Address TODO at src/main.rs:4: // TODO: second",
            "r",
            ImprovementSource::TechDebt,
        )
        .with_file("src/main.rs")
        .with_scores(0.5, 0.5),
    ];
    let selected = orchestrator.select_target(&targets).unwrap();
    assert!(selected.description.contains("first"));
}

#[test]
fn test_select_target_empty_returns_none() {
    let orchestrator = SelfEditOrchestrator::new(PathBuf::from("/tmp/selfware_test"));
    assert!(orchestrator.select_target(&[]).is_none());
}

#[test]
fn test_select_target_skips_unsupported_targets() {
    let orchestrator = SelfEditOrchestrator::new(PathBuf::from("/tmp/selfware_test"));
    let targets = vec![
        ImprovementTarget::new(
            ImprovementCategory::ToolPipeline,
            "Reduce tool-call churn",
            "metrics rationale",
            ImprovementSource::MetricsRegression,
        )
        .with_file("src/agent/execution.rs")
        .with_scores(0.9, 0.9),
        ImprovementTarget::new(
            ImprovementCategory::CodeQuality,
            "Address TODO at src/lib.rs:3: // TODO: tighten this path",
            "TODO/FIXME markers indicate known issues or missing features",
            ImprovementSource::TechDebt,
        )
        .with_file("src/lib.rs")
        .with_scores(0.6, 0.8),
    ];

    let selected = orchestrator.select_target(&targets).unwrap();
    assert!(selected.description.contains("Address TODO"));
}

#[test]
fn test_parse_line_hint_extracts_line_number() {
    let line = parse_line_hint("Address TODO at src/lib.rs:42: // TODO: tighten path");
    assert_eq!(line, Some(42));
}

#[test]
fn test_rewrite_todo_fixme_marker_rewrites_preferred_line() {
    let input = "fn demo() {\n    // TODO: clean this up\n}\n";
    let (updated, line_number) = rewrite_todo_fixme_marker(input, Some(2)).unwrap();
    assert_eq!(line_number, 2);
    assert!(updated.contains("Resolved: clean this up"));
    assert!(!updated.contains("TODO"));
}

#[test]
fn test_apply_target_in_sandbox_updates_file() {
    // Serialize against tests that mutate process-global state (cwd, HOME):
    // the sandbox git-clone and the fixture's git commands inherit both.
    let _state = crate::test_support::CwdGuard::hold();
    let tmp = tempfile::tempdir().unwrap();
    let project_root = tmp.path().to_path_buf();
    let src_dir = project_root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let file_path = src_dir.join("lib.rs");
    std::fs::write(
        &file_path,
        "pub fn demo() {\n    // TODO: clean this up\n}\n",
    )
    .unwrap();

    let run_git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(&project_root)
            .status()
            .unwrap();
        assert!(status.success(), "git {:?} should succeed", args);
    };
    run_git(&["init"]);
    run_git(&["config", "user.email", "codex@openai.com"]);
    run_git(&["config", "user.name", "Codex"]);
    run_git(&["add", "."]);
    run_git(&["commit", "-m", "initial"]);

    let sandbox = CompilationSandbox::new(&project_root).unwrap();
    let orchestrator = SelfEditOrchestrator::new(project_root);
    let target = ImprovementTarget::new(
        ImprovementCategory::CodeQuality,
        "Address TODO at src/lib.rs:2: // TODO: clean this up",
        "TODO/FIXME markers indicate known issues or missing features",
        ImprovementSource::TechDebt,
    )
    .with_file("src/lib.rs")
    .with_scores(0.6, 0.8);

    let applied = orchestrator
        .apply_target_in_sandbox(&target, &sandbox)
        .unwrap();
    let updated = std::fs::read_to_string(sandbox.work_dir().join("src/lib.rs")).unwrap();

    assert_eq!(applied.edited_files, vec!["src/lib.rs".to_string()]);
    assert!(applied.summary.contains("src/lib.rs:2"));
    assert!(updated.contains("Resolved: clean this up"));
    assert!(!updated.contains("TODO"));
}

#[test]
fn test_analyze_self_on_temp_dir_with_todo() {
    // Create a temp dir with a fake .rs file containing a TODO
    let tmp = std::env::temp_dir().join("selfware_test_analyze");
    let src = tmp.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("example.rs"),
        "fn main() {\n    // TODO: fix this\n}\n",
    )
    .unwrap();

    let orchestrator = SelfEditOrchestrator::new(tmp.clone());
    let targets = orchestrator.analyze_self();

    // Should find at least the TODO
    assert!(!targets.is_empty(), "Should find TODO target in test dir");
    assert!(targets.iter().any(|t| t.description.contains("TODO")));
    assert!(targets
        .iter()
        .any(|t| t.source == ImprovementSource::TechDebt));
    assert!(targets
        .iter()
        .any(|t| t.category == ImprovementCategory::CodeQuality));

    // Cleanup
    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn test_analyze_self_filters_low_confidence() {
    // analyze_self filters confidence <= 0.5
    // Our scan_code_quality sets confidence to 0.6, so they should pass
    let tmp = std::env::temp_dir().join("selfware_test_analyze_conf");
    let src = tmp.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("a.rs"), "// FIXME: broken\n").unwrap();

    let orchestrator = SelfEditOrchestrator::new(tmp.clone());
    let targets = orchestrator.analyze_self();
    assert!(!targets.is_empty());
    // All returned targets should have confidence > 0.5
    for t in &targets {
        assert!(
            t.confidence > 0.5,
            "confidence {} should be > 0.5",
            t.confidence
        );
    }

    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn test_analyze_self_no_src_dir() {
    let tmp = std::env::temp_dir().join("selfware_test_no_src");
    std::fs::create_dir_all(&tmp).unwrap();
    // No src/ subdirectory

    let orchestrator = SelfEditOrchestrator::new(tmp.clone());
    let targets = orchestrator.analyze_self();
    assert!(targets.is_empty());

    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn test_record_result_and_history() {
    let tmp = std::env::temp_dir().join("selfware_test_history");
    std::fs::create_dir_all(&tmp).ok();
    let history_path = tmp.join("history.json");
    std::fs::remove_file(&history_path).ok();

    let mut orchestrator =
        SelfEditOrchestrator::with_history_path(tmp.clone(), history_path.clone());
    assert!(orchestrator.history().is_empty());

    let record = ImprovementRecord {
        target_id: "imp-1".to_string(),
        category: ImprovementCategory::ErrorHandling,
        description: "Added retry".to_string(),
        before_metrics: None,
        after_metrics: None,
        git_commits: vec!["abc123".to_string()],
        verified: true,
        rolled_back: false,
        effectiveness_score: 0.5,
        completed_at: 12345,
    };

    orchestrator.record_result(record).unwrap();
    assert_eq!(orchestrator.history().len(), 1);
    assert_eq!(orchestrator.history()[0].target_id, "imp-1");

    // Verify persistence — create new orchestrator from same path
    let orchestrator2 = SelfEditOrchestrator::with_history_path(tmp.clone(), history_path);
    assert_eq!(orchestrator2.history().len(), 1);
    assert_eq!(orchestrator2.history()[0].description, "Added retry");

    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn test_recently_failed_categories_cooldown() {
    let tmp = std::env::temp_dir().join("selfware_test_cooldown");
    std::fs::create_dir_all(&tmp).ok();
    let history_path = tmp.join("history.json");
    std::fs::remove_file(&history_path).ok();

    let mut orchestrator = SelfEditOrchestrator::with_history_path(tmp.clone(), history_path);

    // Record a rolled-back attempt
    let record = ImprovementRecord {
        target_id: "imp-fail".to_string(),
        category: ImprovementCategory::PromptTemplate,
        description: "bad change".to_string(),
        before_metrics: None,
        after_metrics: None,
        git_commits: vec![],
        verified: false,
        rolled_back: true,
        effectiveness_score: -0.3,
        completed_at: 0,
    };
    orchestrator.record_result(record).unwrap();

    let failed = orchestrator.recently_failed_categories(5);
    assert!(failed.contains(&ImprovementCategory::PromptTemplate));

    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn test_introspect_performance_from_snapshots_detects_regression() {
    let orchestrator = SelfEditOrchestrator::new(PathBuf::from("/tmp/selfware_test"));
    let mut snapshots = Vec::new();
    for _ in 0..5 {
        snapshots.push(PerformanceSnapshot {
            timestamp: 1,
            task_success_rate: 0.95,
            avg_iterations: 5.0,
            avg_tool_calls: 8.0,
            error_recovery_rate: 0.9,
            first_try_verification_rate: 0.85,
            avg_tokens: 5000.0,
            test_pass_rate: 0.95,
            compilation_errors_per_task: 0.1,
            label: None,
        });
    }
    for _ in 0..5 {
        snapshots.push(PerformanceSnapshot {
            timestamp: 2,
            task_success_rate: 0.7,
            avg_iterations: 8.0,
            avg_tool_calls: 18.0,
            error_recovery_rate: 0.5,
            first_try_verification_rate: 0.35,
            avg_tokens: 9000.0,
            test_pass_rate: 0.7,
            compilation_errors_per_task: 2.1,
            label: None,
        });
    }

    let targets = orchestrator.introspect_performance_from_snapshots(&snapshots);
    assert!(targets
        .iter()
        .any(|t| t.category == ImprovementCategory::VerificationLogic));
    assert!(targets
        .iter()
        .any(|t| t.category == ImprovementCategory::ToolPipeline));
    assert!(targets
        .iter()
        .any(|t| t.category == ImprovementCategory::CodeQuality));
}

#[test]
fn test_build_improvement_prompt_no_file() {
    let orchestrator = SelfEditOrchestrator::new(PathBuf::from("/tmp/selfware_test"));
    let target = ImprovementTarget::new(
        ImprovementCategory::ContextManagement,
        "Reduce context window usage",
        "Too many tokens wasted",
        ImprovementSource::MetricsRegression,
    );
    let prompt = orchestrator.build_improvement_prompt(&target);
    assert!(prompt.contains("Reduce context window usage"));
    assert!(prompt.contains("context_management"));
    // Should not contain "File:" line since no file set
    assert!(!prompt.contains("**File**:"));
}

#[test]
fn test_improvement_target_serialization_roundtrip() {
    let target = ImprovementTarget::new(
        ImprovementCategory::VerificationLogic,
        "desc",
        "rationale",
        ImprovementSource::LLMReflection,
    )
    .with_file("src/verification.rs")
    .with_scores(0.7, 0.8);

    let json = serde_json::to_string(&target).unwrap();
    let deserialized: ImprovementTarget = serde_json::from_str(&json).unwrap();
    assert_eq!(
        deserialized.category,
        ImprovementCategory::VerificationLogic
    );
    assert_eq!(deserialized.source, ImprovementSource::LLMReflection);
    assert!((deserialized.priority - 0.56).abs() < 0.001);
}

#[test]
fn test_improvement_record_serialization_roundtrip() {
    let record = ImprovementRecord {
        target_id: "imp-42".to_string(),
        category: ImprovementCategory::ToolPipeline,
        description: "test record".to_string(),
        before_metrics: Some(PerformanceSnapshot::from_checkpoint_data(
            5, 10, 1, 1, true, 5000, true,
        )),
        after_metrics: Some(PerformanceSnapshot::from_checkpoint_data(
            3, 6, 0, 0, true, 3000, true,
        )),
        git_commits: vec!["abc".to_string(), "def".to_string()],
        verified: true,
        rolled_back: false,
        effectiveness_score: 0.75,
        completed_at: 99999,
    };

    let json = serde_json::to_string(&record).unwrap();
    let deserialized: ImprovementRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.target_id, "imp-42");
    assert!(deserialized.before_metrics.is_some());
    assert_eq!(deserialized.git_commits.len(), 2);
}

#[test]
fn test_glob_rs_files() {
    let tmp = std::env::temp_dir().join("selfware_test_glob");
    let sub = tmp.join("subdir");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(tmp.join("a.rs"), "").unwrap();
    std::fs::write(tmp.join("b.txt"), "").unwrap(); // not .rs
    std::fs::write(sub.join("c.rs"), "").unwrap();

    let files = glob_rs_files(&tmp).unwrap();
    assert_eq!(files.len(), 2);
    let names: Vec<_> = files
        .iter()
        .map(|f| f.file_name().unwrap().to_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"a.rs".to_string()));
    assert!(names.contains(&"c.rs".to_string()));

    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn test_glob_rs_files_nonexistent_dir() {
    let result = glob_rs_files(Path::new("/tmp/selfware_nonexistent_dir_123456"));
    assert!(result.unwrap().is_empty());
}

#[test]
fn test_deny_list_broken_symlink_fails_closed() {
    // A broken symlink should be denied (fail-closed) because we
    // can't verify it doesn't resolve to a protected file.
    let tmp = std::env::temp_dir().join("selfware_test_symlink_deny");
    std::fs::create_dir_all(&tmp).unwrap();

    // Create a broken symlink
    let link_path = tmp.join("sneaky.rs");
    let _ = std::fs::remove_file(&link_path);
    #[cfg(unix)]
    std::os::unix::fs::symlink("/nonexistent/target", &link_path).unwrap();

    #[cfg(unix)]
    {
        let orchestrator = SelfEditOrchestrator::new(tmp.clone());
        let target = ImprovementTarget::new(
            ImprovementCategory::CodeQuality,
            "edit sneaky file",
            "reason",
            ImprovementSource::CodeSmell,
        )
        .with_file("sneaky.rs");

        // Broken symlink exists but can't be canonicalized → denied
        assert!(
            orchestrator.is_denied(&target),
            "broken symlink should be denied (fail-closed)"
        );
    }

    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn test_deny_list_traversal_denied() {
    let orchestrator = SelfEditOrchestrator::new(PathBuf::from("/tmp/selfware_test"));
    let target = ImprovementTarget::new(
        ImprovementCategory::CodeQuality,
        "traverse",
        "reason",
        ImprovementSource::CodeSmell,
    )
    .with_file("../../etc/safety/checker.rs");
    assert!(
        orchestrator.is_denied(&target),
        "path traversal to denied file should be caught"
    );
}
