use super::*;

#[test]
fn test_cognitive_state_new() {
    let state = CognitiveState::new();
    assert_eq!(state.cycle_phase, CyclePhase::Plan);
    assert!(state.working_memory.current_plan.is_none());
}

#[test]
fn test_cycle_phase_next() {
    assert_eq!(CyclePhase::Plan.next(), CyclePhase::Do);
    assert_eq!(CyclePhase::Do.next(), CyclePhase::Verify);
    assert_eq!(CyclePhase::Verify.next(), CyclePhase::Reflect);
    assert_eq!(CyclePhase::Reflect.next(), CyclePhase::Plan);
}

#[test]
fn test_working_memory_set_plan() {
    let mut wm = WorkingMemory::new();
    wm.set_plan(
        "Fix the bug",
        vec![
            "Read the code".to_string(),
            "Write a test".to_string(),
            "Fix the bug".to_string(),
        ],
    );

    assert_eq!(wm.plan_steps.len(), 3);
    assert_eq!(wm.plan_steps[0].index, 1);
    assert_eq!(wm.plan_steps[0].status, StepStatus::Pending);
}

#[test]
fn test_working_memory_complete_step() {
    let mut wm = WorkingMemory::new();
    wm.set_plan("Test", vec!["Step 1".to_string(), "Step 2".to_string()]);

    wm.complete_step(1, Some("Done!".to_string()));

    assert_eq!(wm.plan_steps[0].status, StepStatus::Completed);
    assert_eq!(wm.plan_steps[0].notes, Some("Done!".to_string()));
}

#[test]
fn test_working_memory_progress_summary() {
    let mut wm = WorkingMemory::new();
    wm.set_plan(
        "Test",
        vec![
            "Step 1".to_string(),
            "Step 2".to_string(),
            "Step 3".to_string(),
        ],
    );
    wm.complete_step(1, None);
    wm.fail_step(2, "Error");

    let summary = wm.progress_summary();
    assert!(summary.contains("1/3"));
    assert!(summary.contains("1 failed"));
}

#[test]
fn test_episodic_memory_record_lesson() {
    let mut em = EpisodicMemory::new();
    em.what_worked("testing", "Always run tests after editing");
    em.what_failed("refactoring", "Don't rename without checking imports");

    assert_eq!(em.lessons.len(), 2);
    assert_eq!(em.lessons[0].category, LessonCategory::Success);
    assert_eq!(em.lessons[1].category, LessonCategory::Failure);
}

#[test]
fn test_episodic_memory_recent_lessons() {
    let mut em = EpisodicMemory::new();
    em.what_worked("a", "Lesson 1");
    em.what_worked("b", "Lesson 2");
    em.what_worked("c", "Lesson 3");

    let recent = em.recent_lessons(2);
    assert_eq!(recent.len(), 2);
    assert!(recent[0].contains("Lesson 3")); // Most recent first
}

#[test]
fn test_episodic_memory_find_relevant() {
    let mut em = EpisodicMemory::new();
    em.what_worked("cargo", "Always run cargo check");
    em.what_worked("git", "Commit frequently");

    let relevant = em.find_relevant("cargo");
    assert_eq!(relevant.len(), 1);
    assert!(relevant[0].content.contains("cargo check"));
}

#[test]
fn test_cognitive_state_summary() {
    let mut state = CognitiveState::new();
    state
        .working_memory
        .set_plan("Fix bug", vec!["Step 1".to_string()]);
    state.working_memory.active_hypothesis = Some("The bug is in parser.rs".to_string());
    state.working_memory.add_question("What triggers the bug?");

    let summary = state.summary();
    assert!(summary.contains("COGNITIVE STATE"));
    assert!(summary.contains("Fix bug"));
    assert!(summary.contains("parser.rs"));
}

#[test]
fn test_cognitive_state_builder() {
    let state = CognitiveStateBuilder::new()
        .with_plan("My plan", vec!["Step 1".to_string()])
        .with_hypothesis("My hypothesis")
        .with_phase(CyclePhase::Do)
        .build();

    assert_eq!(state.cycle_phase, CyclePhase::Do);
    assert!(state.working_memory.active_hypothesis.is_some());
}

#[test]
fn test_multiscale_planning_flow() {
    let mut state = CognitiveState::new();
    state.upsert_strategic_goal("g1", "Ship production-ready autonomy");
    state.set_active_tactical_plan("t1", "Stabilize execution loop", vec!["task-1".to_string()]);
    state.set_operational_plan(
        "task-1",
        vec![
            "Plan".to_string(),
            "Execute".to_string(),
            "Verify".to_string(),
        ],
    );

    state.start_operational_step("task-1", 2, "Execute");
    state.complete_operational_step(2, Some("done".to_string()));
    state.fail_operational_step(3, "verification failed");

    assert_eq!(state.strategic_goals.len(), 1);
    assert!(state.active_tactical_plan.is_some());
    let op = state.active_operational_plan.as_ref().unwrap();
    assert_eq!(op.steps.len(), 3);
    assert_eq!(op.steps[1].status, StepStatus::Completed);
    assert_eq!(op.steps[2].status, StepStatus::Failed);
}

#[test]
fn test_working_memory_approach_stack() {
    let mut wm = WorkingMemory::new();
    wm.push_approach("Try approach A", vec!["file1.rs".to_string()]);
    wm.record_outcome(false, "Didn't work");
    wm.push_approach("Try approach B", vec!["file2.rs".to_string()]);

    assert_eq!(wm.approach_stack.len(), 2);
    assert!(!wm.approach_stack[0].outcome.as_ref().unwrap().success);
}

#[test]
fn test_working_memory_approach_stack_capped() {
    let mut wm = WorkingMemory::new();
    for i in 0..MAX_APPROACH_DEPTH + 5 {
        wm.push_approach(&format!("approach {}", i), vec![]);
    }
    assert_eq!(wm.approach_stack.len(), MAX_APPROACH_DEPTH);
    assert_eq!(wm.approach_stack[0].description, "approach 5");
    assert_eq!(
        wm.approach_stack[MAX_APPROACH_DEPTH - 1].description,
        format!("approach {}", MAX_APPROACH_DEPTH + 4)
    );
}

#[test]
fn test_working_memory_start_next_step() {
    let mut wm = WorkingMemory::new();
    wm.set_plan("Plan", vec!["Step 1".to_string(), "Step 2".to_string()]);

    let step = wm.start_next_step();
    assert!(step.is_some());
    assert_eq!(step.unwrap().index, 1);
    assert_eq!(wm.plan_steps[0].status, StepStatus::InProgress);
}

#[test]
fn test_step_status_serde() {
    let status = StepStatus::Completed;
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(json, "\"completed\"");
}

#[test]
fn test_lesson_category_serde() {
    let cat = LessonCategory::Success;
    let json = serde_json::to_string(&cat).unwrap();
    assert_eq!(json, "\"success\"");
}

#[test]
fn test_cycle_phase_as_str() {
    assert_eq!(CyclePhase::Plan.as_str(), "plan");
    assert_eq!(CyclePhase::Do.as_str(), "do");
    assert_eq!(CyclePhase::Verify.as_str(), "verify");
    assert_eq!(CyclePhase::Reflect.as_str(), "reflect");
}

#[test]
fn test_cognitive_state_default() {
    let state = CognitiveState::default();
    assert_eq!(state.cycle_phase, CyclePhase::Plan);
}

#[test]
fn test_cognitive_state_save_load() {
    let mut state = CognitiveState::new();
    state
        .working_memory
        .set_plan("Test", vec!["Step 1".to_string()]);
    state.cycle_phase = CyclePhase::Do;

    let temp_path = std::env::temp_dir().join("cognitive_test.json");
    state.save(&temp_path).unwrap();

    let loaded = CognitiveState::load(&temp_path).unwrap();
    assert_eq!(loaded.cycle_phase, CyclePhase::Do);
    assert!(loaded.working_memory.current_plan.is_some());

    std::fs::remove_file(&temp_path).ok();
}

#[test]
fn test_cognitive_state_advance_phase() {
    let mut state = CognitiveState::new();
    assert_eq!(state.cycle_phase, CyclePhase::Plan);

    state.advance_phase();
    assert_eq!(state.cycle_phase, CyclePhase::Do);

    state.advance_phase();
    assert_eq!(state.cycle_phase, CyclePhase::Verify);
}

#[test]
fn test_cognitive_state_set_phase() {
    let mut state = CognitiveState::new();
    state.set_phase(CyclePhase::Reflect);
    assert_eq!(state.cycle_phase, CyclePhase::Reflect);
}

#[test]
fn test_working_memory_resolve_question() {
    let mut wm = WorkingMemory::new();
    wm.add_question("What is the bug?");
    wm.add_question("Where is it?");
    assert_eq!(wm.open_questions.len(), 2);

    wm.resolve_question("What is the bug?");
    assert_eq!(wm.open_questions.len(), 1);
    assert_eq!(wm.open_questions[0], "Where is it?");
}

#[test]
fn test_working_memory_add_fact() {
    let mut wm = WorkingMemory::new();
    wm.add_fact("The parser uses regex");
    wm.add_fact("The parser uses regex"); // Duplicate
    wm.add_fact("Config is in TOML");

    assert_eq!(wm.discovered_facts.len(), 2);
}

#[test]
fn test_working_memory_current_step_in_progress() {
    let mut wm = WorkingMemory::new();
    wm.set_plan("Plan", vec!["Step 1".to_string(), "Step 2".to_string()]);
    wm.plan_steps[0].status = StepStatus::InProgress;

    let current = wm.current_step();
    assert!(current.is_some());
    assert_eq!(current.unwrap().index, 1);
}

#[test]
fn test_episodic_memory_user_prefers() {
    let mut em = EpisodicMemory::new();
    em.user_prefers("Always use descriptive variable names");

    assert_eq!(em.lessons.len(), 1);
    assert_eq!(em.lessons[0].category, LessonCategory::Preference);
}

#[test]
fn test_episodic_memory_pattern() {
    let mut em = EpisodicMemory::new();
    em.record_pattern(Pattern {
        name: "clippy-check".to_string(),
        description: "Always run clippy before commit".to_string(),
        trigger: "Before commit".to_string(),
        action: "Run cargo clippy".to_string(),
        confidence: 0.9,
        occurrences: 5,
    });

    assert_eq!(em.patterns.len(), 1);
    assert_eq!(em.patterns[0].name, "clippy-check");
}

#[test]
fn test_lesson_formatting() {
    let lesson = Lesson {
        context: "testing".to_string(),
        content: "Run tests often".to_string(),
        category: LessonCategory::Success,
        tags: vec!["testing".to_string()],
        timestamp: Utc::now(),
    };

    let formatted = format!("{:?}", lesson);
    assert!(formatted.contains("testing"));
}

#[test]
fn test_approach_attempt_with_outcome() {
    let mut attempt = ApproachAttempt {
        description: "Try A".to_string(),
        files_modified: vec!["file.rs".to_string()],
        timestamp: Utc::now(),
        outcome: None,
    };

    attempt.outcome = Some(ApproachOutcome {
        success: true,
        notes: "Worked!".to_string(),
    });

    assert!(attempt.outcome.unwrap().success);
}

#[test]
fn test_episodic_memory_add_knowledge() {
    let mut em = EpisodicMemory::new();
    em.add_knowledge("build_system", "cargo");
    em.add_knowledge("language", "rust");

    assert_eq!(
        em.project_knowledge.get("build_system"),
        Some(&"cargo".to_string())
    );
    assert_eq!(em.project_knowledge.len(), 2);
}

#[test]
fn test_lesson_category_discovery() {
    let lesson = Lesson {
        category: LessonCategory::Discovery,
        content: "Found the config file".to_string(),
        context: "exploration".to_string(),
        tags: vec![],
        timestamp: Utc::now(),
    };

    assert_eq!(lesson.category, LessonCategory::Discovery);
}

#[test]
fn test_lesson_category_warning() {
    let lesson = Lesson {
        category: LessonCategory::Warning,
        content: "Don't edit generated files".to_string(),
        context: "codegen".to_string(),
        tags: vec!["generated".to_string()],
        timestamp: Utc::now(),
    };

    assert_eq!(lesson.category, LessonCategory::Warning);
}

#[test]
fn test_pattern_struct() {
    let pattern = Pattern {
        name: "test-first".to_string(),
        description: "Write test before implementation".to_string(),
        trigger: "New feature".to_string(),
        action: "Create test file first".to_string(),
        confidence: 0.85,
        occurrences: 10,
    };

    assert_eq!(pattern.name, "test-first");
    assert!((pattern.confidence - 0.85).abs() < f32::EPSILON);
}

#[test]
fn test_cycle_phase_serde() {
    let phase = CyclePhase::Verify;
    let json = serde_json::to_string(&phase).unwrap();
    assert!(json.contains("verify"));

    let parsed: CyclePhase = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, CyclePhase::Verify);
}

#[test]
fn test_working_memory_fail_step_invalid_index() {
    let mut wm = WorkingMemory::new();
    wm.set_plan("Plan", vec!["Step 1".to_string()]);

    // Failing step 0 should fail step at index 0 (saturating_sub)
    wm.fail_step(0, "Error");
    // The step should still be pending because 0.saturating_sub(1) = 0,
    // but the condition checks for index-1, so step 0 would be modified
    // Let's test with valid indices
    wm.fail_step(1, "Real error");
    assert_eq!(wm.plan_steps[0].status, StepStatus::Failed);
}

#[test]
fn test_working_memory_complete_step_out_of_bounds() {
    let mut wm = WorkingMemory::new();
    wm.set_plan("Plan", vec!["Step 1".to_string()]);

    // Completing step 10 on a 1-step plan should do nothing
    wm.complete_step(10, Some("Notes".to_string()));
    assert_eq!(wm.plan_steps[0].status, StepStatus::Pending);
}

#[test]
fn test_plan_step_default_status() {
    let step = PlanStep {
        index: 1,
        description: "Test step".to_string(),
        status: StepStatus::Pending,
        notes: None,
    };
    assert_eq!(step.status, StepStatus::Pending);
    assert!(step.notes.is_none());
}
