use super::*;

/// Unix seconds, so a record can be stamped as a *recent* failure. The cooldown
/// ages on the wall clock, so a hard-coded timestamp like `100` reads as decades
/// old and is correctly treated as already expired.
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock must be after the epoch")
        .as_secs()
}

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

    // All judges, evaluators, orchestrators, mutators, and test paths must be denied
    let denied_paths = [
        "src/safety/checker.rs",
        "src/agent/verification.rs",
        "src/agent/verification_scope.rs",
        "src/agent/checkpointing.rs",
        "src/agent/tool_dispatch/mod.rs",
        "src/evolution/fitness.rs",
        "src/evolution/daemon.rs",
        "src/cognitive/rsi_orchestrator.rs",
        "src/testing/verification.rs",
        "src/cognitive/self_edit.rs",
        "src/cognitive/compilation_manager.rs",
        "system_tests/run_projecte2e.sh",
        "tests/unit/mod.rs",
        "Cargo.toml",
        "Cargo.lock",
        ".github/workflows/ci.yml",
        "src/main.rs",
        ".selfware/KILLSWITCH",
        ".selfware/skills/sop.md",
        ".selfware/commands/cmd.md",
        ".selfware/skill-candidates/candidate.md",
        ".admitted_ledger.json",
    ];

    for path in denied_paths {
        let target = ImprovementTarget::new(
            ImprovementCategory::CodeQuality,
            format!("edit {}", path),
            "reason",
            ImprovementSource::CodeSmell,
        )
        .with_file(path);
        assert!(
            orchestrator.is_denied(&target),
            "Path '{}' must be denied by safety gates",
            path
        );
    }

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
fn test_recently_failed_categories_ignores_skipped_trivial() {
    let mut orchestrator = SelfEditOrchestrator::new(PathBuf::from("/tmp/selfware_test"));
    orchestrator.history.clear();
    let trivial_record = ImprovementRecord {
        target_id: "imp-1".to_string(),
        category: ImprovementCategory::CodeQuality,
        description: "trivial".to_string(),
        before_metrics: None,
        after_metrics: None,
        git_commits: vec![],
        verified: false,
        rolled_back: true,
        effectiveness_score: -1.0,
        completed_at: now_secs(),
        status: ProposalStatus::SkippedTrivial,
    };
    orchestrator.history.push(trivial_record);
    let failed = orchestrator.recently_failed_categories(FAILURE_COOLDOWN_SECS);
    assert!(
        failed.is_empty(),
        "SkippedTrivial must be neutral and never penalized as a failed category"
    );

    let regressed_record = ImprovementRecord {
        target_id: "imp-2".to_string(),
        category: ImprovementCategory::ErrorHandling,
        description: "regression".to_string(),
        before_metrics: None,
        after_metrics: None,
        git_commits: vec![],
        verified: false,
        rolled_back: true,
        effectiveness_score: -0.5,
        completed_at: now_secs(),
        status: ProposalStatus::EvaluatedRegression,
    };
    orchestrator.history.push(regressed_record);
    let failed = orchestrator.recently_failed_categories(FAILURE_COOLDOWN_SECS);
    assert_eq!(failed, vec![ImprovementCategory::ErrorHandling]);
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

// ── Inline comment stripper (shared with the trivial-mutation gate) ──

#[test]
fn test_strip_inline_comment_basic() {
    assert_eq!(
        strip_inline_comment("    let x = 5; // TODO: fix"),
        "    let x = 5;"
    );
    // A whole-line comment strips to nothing (the gate then filters it out).
    assert_eq!(strip_inline_comment("// whole line comment"), "");
    // No comment anywhere: the line comes back unchanged.
    assert_eq!(strip_inline_comment("let x = 5;"), "let x = 5;");
    assert_eq!(strip_inline_comment(""), "");
}

#[test]
fn test_strip_inline_comment_ignores_slashes_inside_strings() {
    // `//` inside a string literal is code, not a comment — a URL must
    // survive intact so a change inside it never compares equal.
    assert_eq!(
        strip_inline_comment(r#"let u = "http://example.com/a/b"; // note"#),
        r#"let u = "http://example.com/a/b";"#
    );
    // Char literals are shielded the same way.
    assert_eq!(
        strip_inline_comment("let c = '/'; // separator"),
        "let c = '/';"
    );
}

#[test]
fn test_strip_inline_comment_escaped_quotes_stay_inside_string() {
    assert_eq!(
        strip_inline_comment(r#"let s = "a \"quoted\" bit"; // note"#),
        r#"let s = "a \"quoted\" bit";"#
    );
}

#[test]
fn test_strip_inline_comment_lifetime_erreds_to_not_stripping() {
    // A Rust lifetime leaves the char-literal state open until the next `'`,
    // so the trailing comment is (conservatively) NOT stripped: this can only
    // cost one cycle's evaluation, never silently skip a real change.
    let line = "fn f<'a>(x: &'a str) -> &'a str { x } // TODO";
    assert_eq!(strip_inline_comment(line), line);
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
        "fn main() {\n    let port = 8080; // TODO: fix this\n}\n",
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
    // The marker shares its line with code — a whole-line comment marker is
    // deliberately not a target (its rewrite is a comment-only diff, which the
    // trivial-mutation gate discards before evaluation).
    std::fs::write(
        src.join("a.rs"),
        "const LIMIT: usize = 3; // FIXME: broken\n",
    )
    .unwrap();

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
        status: ProposalStatus::EvaluatedSuccess,
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
        completed_at: now_secs(),
        status: ProposalStatus::VerificationFailed,
    };
    orchestrator.record_result(record).unwrap();

    let failed = orchestrator.recently_failed_categories(FAILURE_COOLDOWN_SECS);
    assert!(failed.contains(&ImprovementCategory::PromptTemplate));

    std::fs::remove_dir_all(&tmp).ok();
}

/// The cooldown must expire on the wall clock, not on the arrival of new
/// records.
///
/// A record-count window can never age out its own blocker: a cycle that finds
/// nothing eligible appends no record, so the window never advances and the
/// failed category stays excluded — and because the history is persisted, that
/// block survived restarts. This is the starvation path that made RSI cycles
/// spin on a category that could never become eligible again.
#[test]
fn test_cooldown_expires_by_age_without_new_records() {
    let mut orchestrator = SelfEditOrchestrator::new(PathBuf::from("/tmp/selfware_test"));
    orchestrator.history.clear();

    let stale = ImprovementRecord {
        target_id: "imp-stale".to_string(),
        category: ImprovementCategory::CodeQuality,
        description: "failed long ago".to_string(),
        before_metrics: None,
        after_metrics: None,
        git_commits: vec![],
        verified: false,
        rolled_back: true,
        effectiveness_score: -1.0,
        completed_at: 1,
        status: ProposalStatus::VerificationFailed,
    };
    orchestrator.history.push(stale.clone());

    // No record is appended between these calls: the window must still advance.
    let failed = orchestrator.recently_failed_categories(FAILURE_COOLDOWN_SECS);
    assert!(
        failed.is_empty(),
        "an ancient failure must not block its category forever"
    );

    // The same record stamped now is inside the cooldown.
    let mut fresh = stale;
    fresh.completed_at = now_secs();
    orchestrator.history.clear();
    orchestrator.history.push(fresh);
    let failed = orchestrator.recently_failed_categories(FAILURE_COOLDOWN_SECS);
    assert!(failed.contains(&ImprovementCategory::CodeQuality));
}

/// A TODO/FIXME on a whole-line comment can only be rewritten as comment text,
/// which the trivial-mutation gate discards before evaluation — so proposing it
/// spends a cycle to learn nothing and leaves the target eligible again. The
/// scanner must only propose markers that share their line with code, which is
/// exactly the mutation the trivial gate keeps.
#[test]
fn test_scan_code_quality_skips_comment_only_markers() {
    let tmp = tempfile::tempdir().unwrap();
    let project_root = tmp.path().to_path_buf();
    let src_dir = project_root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        src_dir.join("pipeline.rs"),
        "pub fn run() -> usize {\n    // TODO: whole-line comment marker\n    let n = 42; // FIXME: inline marker shares its line with code\n    n\n}\n",
    )
    .unwrap();

    let orchestrator = SelfEditOrchestrator::new(project_root);
    let targets = orchestrator.scan_code_quality();

    assert_eq!(
        targets.len(),
        1,
        "only a code-line marker can reach evaluation; got: {:?}",
        targets
            .iter()
            .map(|t| t.description.as_str())
            .collect::<Vec<_>>()
    );
    let target = &targets[0];
    assert!(
        target.description.contains("inline marker"),
        "the surviving target must be the code-line marker, got: {}",
        target.description
    );
    // The invariant the scanner upholds: what it proposes is something the
    // trivial gate will not throw away.
    assert!(orchestrator.supports_target(target));
}

/// The line hint exists to absorb drift between scan and apply, but its
/// fallback must land on a code-line marker — never on a whole-line comment,
/// whose comment-only diff `mutation_is_trivial` discards before evaluation.
/// Without this the scanner's honesty could be undone at apply time.
#[test]
fn test_rewrite_fallback_skips_comment_only_markers() {
    // The target named line 2, which no longer holds a marker. The markers that
    // remain are a whole-line comment (line 3) and an inline one (line 4).
    let input = "fn demo() -> usize {\n    let first = 1;\n    // TODO: stale comment marker\n    let second = 2; // FIXME: real target\n    first + second\n}\n";
    let (updated, line_number) =
        rewrite_todo_fixme_marker(input, Some(2)).expect("a code-line marker exists to rewrite");

    assert_eq!(
        line_number, 4,
        "must rewrite the code-line marker, not the comment"
    );
    assert!(
        updated.contains("// TODO: stale comment marker"),
        "the comment-only marker must be left alone"
    );
    assert!(
        !updated.contains("FIXME"),
        "the code-line marker must be rewritten"
    );
    // The hinted line held no marker at all, so it must come back
    // byte-identical: the old guard compared the *post-whitespace-collapse*
    // text, so `replaced != original` was true for any indented line and the
    // drift case silently de-indented an unrelated line instead of falling
    // through.
    assert!(
        updated.contains("\n    let first = 1;\n"),
        "a marker-less hinted line must not be rewritten, got: {updated:?}"
    );
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
        status: ProposalStatus::EvaluatedSuccess,
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

#[test]
fn test_supports_target_rejects_unsupported_categories() {
    let orch = SelfEditOrchestrator::new(PathBuf::from("/tmp/test"));

    let t_prompt = ImprovementTarget::new(
        ImprovementCategory::PromptTemplate,
        "Refine prompt guidance for reasoning",
        "Improve reasoning depth",
        ImprovementSource::MetricsRegression,
    )
    .with_file("src/agent/mod.rs");
    assert!(!orch.supports_target(&t_prompt));

    let t_tool = ImprovementTarget::new(
        ImprovementCategory::ToolPipeline,
        "Tune tool batch limit policy",
        "Reduce tool-call churn",
        ImprovementSource::MetricsRegression,
    )
    .with_file("src/agent/execution.rs");
    assert!(!orch.supports_target(&t_tool));

    let t_err = ImprovementTarget::new(
        ImprovementCategory::ErrorHandling,
        "Configure retry backoff policy",
        "Handle transient provider rate limits",
        ImprovementSource::ErrorPattern,
    )
    .with_file("src/agent/retry.rs");
    assert!(!orch.supports_target(&t_err));

    let t_ctx = ImprovementTarget::new(
        ImprovementCategory::ContextManagement,
        "Adjust context compaction window budget",
        "Prevent context exhaustion",
        ImprovementSource::MetricsRegression,
    )
    .with_file("src/agent/context.rs");
    assert!(!orch.supports_target(&t_ctx));

    let t_verif = ImprovementTarget::new(
        ImprovementCategory::VerificationLogic,
        "Harden verification contract assertions",
        "Detect test regressions earlier",
        ImprovementSource::MetricsRegression,
    )
    .with_file("src/agent/verification.rs");
    assert!(!orch.supports_target(&t_verif));

    let t_cq_err = ImprovementTarget::new(
        ImprovementCategory::CodeQuality,
        "Fix repeated compilation errors",
        "Compile pass rate drop",
        ImprovementSource::ErrorPattern,
    )
    .with_file("src/cognitive/self_edit.rs");
    // Without TODO/FIXME in description, code quality cannot be mechanically mutated
    assert!(!orch.supports_target(&t_cq_err));

    let t_cq_todo = ImprovementTarget::new(
        ImprovementCategory::CodeQuality,
        "Address TODO at src/lib.rs:5: // TODO: fix",
        "Known debt",
        ImprovementSource::TechDebt,
    )
    .with_file("src/lib.rs");
    assert!(orch.supports_target(&t_cq_todo));

    let t_cap = ImprovementTarget::new(
        ImprovementCategory::NewCapability,
        "Implement autonomous planning engine",
        "Missing capability",
        ImprovementSource::TechDebt,
    )
    .with_file("src/cognitive/planner.rs");
    assert!(!orch.supports_target(&t_cap));

    let t_no_file = ImprovementTarget::new(
        ImprovementCategory::PromptTemplate,
        "Refine prompt guidance without file",
        "Prompt tuning",
        ImprovementSource::MetricsRegression,
    );
    assert!(!orch.supports_target(&t_no_file));
}

#[test]
fn test_apply_target_in_sandbox_containment_and_deny_list() {
    let _state = crate::test_support::CwdGuard::hold();
    let tmp = tempfile::tempdir().unwrap();
    let project_root = tmp.path().to_path_buf();
    let src_dir = project_root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let file_path = src_dir.join("pipeline.rs");
    std::fs::write(
        &file_path,
        "pub fn run_pipeline() -> bool {\n    // TODO: implement\n    true\n}\n",
    )
    .unwrap();
    let main_path = src_dir.join("main.rs");
    std::fs::write(&main_path, "fn main() {}\n").unwrap();

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

    // 1. Rejects path traversal attack
    let traversal_target = ImprovementTarget::new(
        ImprovementCategory::CodeQuality,
        "Address TODO in escaped path",
        "Attacking path containment",
        ImprovementSource::TechDebt,
    )
    .with_file("../src/main.rs");
    let err = orchestrator
        .apply_target_in_sandbox(&traversal_target, &sandbox)
        .unwrap_err();
    assert!(
        err.to_string().contains("Path traversal")
            || err.to_string().contains("No concrete mutation")
            || err.to_string().contains("deny list")
    );

    // 2. Rejects denied file even with valid TODO description
    let denied_target = ImprovementTarget::new(
        ImprovementCategory::CodeQuality,
        "Address TODO at src/main.rs:1: // TODO: denied file",
        "Attacking deny list",
        ImprovementSource::TechDebt,
    )
    .with_file("src/main.rs");
    let err = orchestrator
        .apply_target_in_sandbox(&denied_target, &sandbox)
        .unwrap_err();
    assert!(err.to_string().contains("deny list"));

    // 3. Rejects absolute path
    let absolute_target = ImprovementTarget::new(
        ImprovementCategory::CodeQuality,
        "Address TODO at /etc/passwd:1: // TODO: absolute path",
        "Absolute path attack",
        ImprovementSource::TechDebt,
    )
    .with_file("/etc/passwd");
    let err = orchestrator
        .apply_target_in_sandbox(&absolute_target, &sandbox)
        .unwrap_err();
    assert!(
        err.to_string().contains("Path traversal") || err.to_string().contains("absolute path")
    );

    // 4. Valid mutation in sandbox succeeds and stays contained
    let valid_target = ImprovementTarget::new(
        ImprovementCategory::CodeQuality,
        "Address TODO at src/pipeline.rs:2: // TODO: implement",
        "Resolve TODO",
        ImprovementSource::TechDebt,
    )
    .with_file("src/pipeline.rs");
    let applied = orchestrator
        .apply_target_in_sandbox(&valid_target, &sandbox)
        .unwrap();
    assert_eq!(applied.edited_files, vec!["src/pipeline.rs".to_string()]);
    assert!(applied.summary.contains("Rewrote TODO/FIXME marker"));

    let content = std::fs::read_to_string(sandbox.work_dir().join("src/pipeline.rs")).unwrap();
    assert!(content.contains("Resolved: implement"));
    assert!(!content.contains("TODO"));
}

#[test]
fn test_killswitch_blocks_self_edit_operations() {
    let _lock = crate::safety::killswitch::KILLSWITCH_TEST_LOCK.lock();
    crate::safety::killswitch::reset_in_process();

    let temp = tempfile::tempdir().unwrap();
    let project_root = temp.path().to_path_buf();
    let src_dir = project_root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(src_dir.join("pipeline.rs"), "// TODO: implement\n").unwrap();

    let run_git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(&project_root)
            .status()
            .unwrap();
        assert!(status.success(), "git {:?} should succeed", args);
    };
    run_git(&["init"]);
    run_git(&["config", "user.email", "test@example.com"]);
    run_git(&["config", "user.name", "Test"]);
    run_git(&["add", "."]);
    run_git(&["commit", "-m", "initial"]);

    let sandbox = CompilationSandbox::new(&project_root).unwrap();
    let orchestrator = SelfEditOrchestrator::new(project_root);

    let target = ImprovementTarget::new(
        ImprovementCategory::CodeQuality,
        "Address TODO at src/pipeline.rs:1: // TODO: implement",
        "Resolve TODO",
        ImprovementSource::TechDebt,
    )
    .with_file("src/pipeline.rs");

    // Before trip: is_denied is false
    assert!(!orchestrator.is_denied(&target));

    // Trip killswitch
    crate::safety::killswitch::trip_in_process("Self-edit killswitch test");

    // All operations must be blocked/denied
    assert!(orchestrator.is_denied(&target));
    assert!(orchestrator.analyze_self().is_empty());
    assert!(orchestrator
        .select_target(std::slice::from_ref(&target))
        .is_none());
    let err = orchestrator
        .apply_target_in_sandbox(&target, &sandbox)
        .unwrap_err();
    assert!(err.to_string().contains("Killswitch is active"));

    // Reset cleanly
    crate::safety::killswitch::reset_in_process();
    assert!(!orchestrator.is_denied(&target));
}
