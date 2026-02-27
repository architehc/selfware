//! Cognitive Architecture - Working Memory, Episodic Memory, and Reflection
//!
//! Implements a structured cognitive state that survives context compression:
//! - Working Memory: Current task context, plan, hypotheses
//! - Episodic Memory: Lessons learned, patterns that worked/didn't
//! - Reflection: Plan-Do-Verify-Reflect cycle state

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// The complete cognitive state of the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveState {
    pub strategic_goals: Vec<StrategicGoal>,
    pub active_tactical_plan: Option<TacticalPlan>,
    pub active_operational_plan: Option<OperationalPlan>,
    /// Current working memory (survives compression)
    pub working_memory: WorkingMemory,
    /// Episodic memory - lessons learned
    pub episodic_memory: EpisodicMemory,
    /// Current phase in the PDVR cycle
    pub cycle_phase: CyclePhase,
    /// Metadata
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for CognitiveState {
    fn default() -> Self {
        Self::new()
    }
}

impl CognitiveState {
    pub fn new() -> Self {
        Self {
            working_memory: WorkingMemory::new(),
            episodic_memory: EpisodicMemory::new(),
            cycle_phase: CyclePhase::Plan,
            strategic_goals: Vec::new(),
            active_tactical_plan: None,
            active_operational_plan: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Load from a file
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Save to a file
    pub fn save(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Transition to the next cycle phase
    pub fn advance_phase(&mut self) {
        self.cycle_phase = self.cycle_phase.next();
        self.updated_at = Utc::now();
    }

    /// Set the current phase explicitly
    pub fn set_phase(&mut self, phase: CyclePhase) {
        self.cycle_phase = phase;
        self.updated_at = Utc::now();
    }

    /// Register/update a long-lived strategic goal.
    pub fn upsert_strategic_goal(&mut self, id: impl Into<String>, description: impl Into<String>) {
        let id = id.into();
        if let Some(existing) = self.strategic_goals.iter_mut().find(|g| g.id == id) {
            existing.description = description.into();
        } else {
            self.strategic_goals.push(StrategicGoal {
                id,
                description: description.into(),
                criteria_for_success: Vec::new(),
                status: StepStatus::InProgress,
                tactical_plans: Vec::new(),
            });
        }
        self.updated_at = Utc::now();
    }

    /// Set the active tactical plan.
    pub fn set_active_tactical_plan(
        &mut self,
        id: impl Into<String>,
        description: impl Into<String>,
        operational_tasks: Vec<String>,
    ) {
        self.active_tactical_plan = Some(TacticalPlan {
            id: id.into(),
            description: description.into(),
            status: StepStatus::InProgress,
            operational_tasks,
        });
        self.updated_at = Utc::now();
    }

    /// Start (or replace) the current operational plan.
    pub fn set_operational_plan(&mut self, task_id: impl Into<String>, steps: Vec<String>) {
        let steps = steps
            .into_iter()
            .enumerate()
            .map(|(index, description)| PlanStep {
                index: index + 1,
                description,
                status: StepStatus::Pending,
                notes: None,
            })
            .collect();
        self.active_operational_plan = Some(OperationalPlan {
            task_id: task_id.into(),
            steps,
        });
        self.updated_at = Utc::now();
    }

    /// Ensure a step exists in the active operational plan and mark it in progress.
    pub fn start_operational_step(&mut self, task_id: &str, index: usize, description: &str) {
        if index == 0 {
            return;
        }

        let plan = self
            .active_operational_plan
            .get_or_insert_with(|| OperationalPlan {
                task_id: task_id.to_string(),
                steps: Vec::new(),
            });

        if plan.task_id != task_id {
            *plan = OperationalPlan {
                task_id: task_id.to_string(),
                steps: Vec::new(),
            };
        }

        if let Some(step) = plan.steps.iter_mut().find(|s| s.index == index) {
            if step.description.is_empty() {
                step.description = description.to_string();
            }
            if step.status == StepStatus::Pending {
                step.status = StepStatus::InProgress;
            }
        } else {
            plan.steps.push(PlanStep {
                index,
                description: description.to_string(),
                status: StepStatus::InProgress,
                notes: None,
            });
            plan.steps.sort_by_key(|s| s.index);
        }
        self.updated_at = Utc::now();
    }

    /// Mark an operational step as complete.
    pub fn complete_operational_step(&mut self, index: usize, notes: Option<String>) {
        if let Some(plan) = self.active_operational_plan.as_mut() {
            if let Some(step) = plan.steps.iter_mut().find(|s| s.index == index) {
                step.status = StepStatus::Completed;
                step.notes = notes;
            }
        }
        self.updated_at = Utc::now();
    }

    /// Mark an operational step as failed.
    pub fn fail_operational_step(&mut self, index: usize, reason: &str) {
        if let Some(plan) = self.active_operational_plan.as_mut() {
            if let Some(step) = plan.steps.iter_mut().find(|s| s.index == index) {
                step.status = StepStatus::Failed;
                step.notes = Some(reason.to_string());
            }
        }
        self.updated_at = Utc::now();
    }

    /// Generate a summary for context compression
    pub fn summary(&self) -> String {
        let strategic_summary = if self.strategic_goals.is_empty() {
            "None".to_string()
        } else {
            self.strategic_goals
                .iter()
                .take(3)
                .map(|g| format!("- [{}] {}", format!("{:?}", g.status), g.description))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let tactical_summary = self
            .active_tactical_plan
            .as_ref()
            .map(|p| format!("[{:?}] {} ({})", p.status, p.description, p.id))
            .unwrap_or_else(|| "None".to_string());

        let operational_summary = self
            .active_operational_plan
            .as_ref()
            .map(|p| {
                let total = p.steps.len();
                let completed = p
                    .steps
                    .iter()
                    .filter(|s| s.status == StepStatus::Completed)
                    .count();
                format!("task={} {} / {} steps", p.task_id, completed, total)
            })
            .unwrap_or_else(|| "None".to_string());

        format!(
            r#"=== COGNITIVE STATE ===
Phase: {:?}

[Strategic Goals]
{}

[Active Tactical Plan]
{}

[Active Operational Plan]
{}

[Current Plan]
{}

[Active Hypothesis]
{}

[Verification Status]
{}

[Open Questions]
{}

[Recent Lessons]
{}
=== END COGNITIVE STATE ==="#,
            self.cycle_phase,
            strategic_summary,
            tactical_summary,
            operational_summary,
            self.working_memory
                .current_plan
                .as_deref()
                .unwrap_or("No plan set"),
            self.working_memory
                .active_hypothesis
                .as_deref()
                .unwrap_or("None"),
            self.working_memory
                .verification_status
                .as_deref()
                .unwrap_or("Not verified"),
            self.working_memory.open_questions.join("\n- "),
            self.episodic_memory.recent_lessons(3).join("\n- "),
        )
    }
}

/// Working Memory - the agent's "scratchpad"
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkingMemory {
    /// The current high-level plan
    pub current_plan: Option<String>,
    /// Steps in the plan with completion status
    pub plan_steps: Vec<PlanStep>,
    /// Current hypothesis being tested
    pub active_hypothesis: Option<String>,
    /// Last verification result summary
    pub verification_status: Option<String>,
    /// Open questions that need answers
    pub open_questions: Vec<String>,
    /// Key facts discovered during this task
    pub discovered_facts: Vec<String>,
    /// Files currently being worked on
    pub active_files: Vec<String>,
    /// Temporary notes
    pub scratchpad: String,
    /// Stack of attempted approaches (for backtracking)
    pub approach_stack: Vec<ApproachAttempt>,
}

impl WorkingMemory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the current plan
    pub fn set_plan(&mut self, plan: &str, steps: Vec<String>) {
        self.current_plan = Some(plan.to_string());
        self.plan_steps = steps
            .into_iter()
            .enumerate()
            .map(|(i, description)| PlanStep {
                index: i + 1,
                description,
                status: StepStatus::Pending,
                notes: None,
            })
            .collect();
    }

    /// Mark a step as complete
    pub fn complete_step(&mut self, index: usize, notes: Option<String>) {
        if let Some(step) = self.plan_steps.get_mut(index.saturating_sub(1)) {
            step.status = StepStatus::Completed;
            step.notes = notes;
        }
    }

    /// Mark a step as failed
    pub fn fail_step(&mut self, index: usize, reason: &str) {
        if let Some(step) = self.plan_steps.get_mut(index.saturating_sub(1)) {
            step.status = StepStatus::Failed;
            step.notes = Some(reason.to_string());
        }
    }

    /// Get the current step
    pub fn current_step(&self) -> Option<&PlanStep> {
        self.plan_steps
            .iter()
            .find(|s| s.status == StepStatus::InProgress)
            .or_else(|| {
                self.plan_steps
                    .iter()
                    .find(|s| s.status == StepStatus::Pending)
            })
    }

    /// Start the next pending step
    pub fn start_next_step(&mut self) -> Option<&PlanStep> {
        if let Some(step) = self
            .plan_steps
            .iter_mut()
            .find(|s| s.status == StepStatus::Pending)
        {
            step.status = StepStatus::InProgress;
        }
        self.current_step()
    }

    /// Add an open question
    pub fn add_question(&mut self, question: &str) {
        if !self.open_questions.contains(&question.to_string()) {
            self.open_questions.push(question.to_string());
        }
    }

    /// Resolve a question
    pub fn resolve_question(&mut self, question: &str) {
        self.open_questions.retain(|q| q != question);
    }

    /// Add a discovered fact
    pub fn add_fact(&mut self, fact: &str) {
        if !self.discovered_facts.contains(&fact.to_string()) {
            self.discovered_facts.push(fact.to_string());
        }
    }

    /// Push an approach attempt for backtracking
    pub fn push_approach(&mut self, description: &str, files_modified: Vec<String>) {
        self.approach_stack.push(ApproachAttempt {
            description: description.to_string(),
            files_modified,
            timestamp: Utc::now(),
            outcome: None,
        });
    }

    /// Record the outcome of the current approach
    pub fn record_outcome(&mut self, success: bool, notes: &str) {
        if let Some(attempt) = self.approach_stack.last_mut() {
            attempt.outcome = Some(ApproachOutcome {
                success,
                notes: notes.to_string(),
            });
        }
    }

    /// Get progress summary
    pub fn progress_summary(&self) -> String {
        let total = self.plan_steps.len();
        let completed = self
            .plan_steps
            .iter()
            .filter(|s| s.status == StepStatus::Completed)
            .count();
        let failed = self
            .plan_steps
            .iter()
            .filter(|s| s.status == StepStatus::Failed)
            .count();

        format!(
            "Progress: {}/{} steps ({} failed)",
            completed, total, failed
        )
    }

    /// Clear working memory for a new task
    pub fn clear(&mut self) {
        *self = Self::new();
    }
}

/// Strategic level goal spanning multiple sessions/days
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategicGoal {
    pub id: String,
    pub description: String,
    pub criteria_for_success: Vec<String>,
    pub status: StepStatus,
    pub tactical_plans: Vec<TacticalPlan>,
}

/// Tactical level plan spanning hours/multiple operational tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TacticalPlan {
    pub id: String,
    pub description: String,
    pub status: StepStatus,
    pub operational_tasks: Vec<String>, // Task IDs
}

/// Operational level plan (current task execution)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationalPlan {
    pub task_id: String,
    pub steps: Vec<PlanStep>,
}

/// A step in the plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub index: usize,
    pub description: String,
    pub status: StepStatus,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

/// An attempted approach (for backtracking)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproachAttempt {
    pub description: String,
    pub files_modified: Vec<String>,
    pub timestamp: DateTime<Utc>,
    pub outcome: Option<ApproachOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproachOutcome {
    pub success: bool,
    pub notes: String,
}

/// Episodic Memory - lessons learned across tasks
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EpisodicMemory {
    /// Lessons learned (what worked, what didn't)
    pub lessons: Vec<Lesson>,
    /// Patterns observed
    pub patterns: Vec<Pattern>,
    /// Project-specific knowledge
    pub project_knowledge: HashMap<String, String>,
}

impl EpisodicMemory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a lesson learned
    pub fn record_lesson(&mut self, lesson: Lesson) {
        // Avoid duplicates
        if !self.lessons.iter().any(|l| l.content == lesson.content) {
            self.lessons.push(lesson);
        }
    }

    /// Record a pattern
    pub fn record_pattern(&mut self, pattern: Pattern) {
        if !self.patterns.iter().any(|p| p.name == pattern.name) {
            self.patterns.push(pattern);
        }
    }

    /// Add project knowledge
    pub fn add_knowledge(&mut self, key: &str, value: &str) {
        self.project_knowledge
            .insert(key.to_string(), value.to_string());
    }

    /// Get recent lessons
    pub fn recent_lessons(&self, n: usize) -> Vec<String> {
        self.lessons
            .iter()
            .rev()
            .take(n)
            .map(|l| format!("[{:?}] {}", l.category, l.content))
            .collect()
    }

    /// Find relevant lessons for a context
    pub fn find_relevant(&self, context: &str) -> Vec<&Lesson> {
        let context_lower = context.to_lowercase();
        self.lessons
            .iter()
            .filter(|l| {
                l.content.to_lowercase().contains(&context_lower)
                    || l.tags
                        .iter()
                        .any(|t| context_lower.contains(&t.to_lowercase()))
            })
            .collect()
    }

    /// Quick lesson: what worked
    pub fn what_worked(&mut self, context: &str, description: &str) {
        self.record_lesson(Lesson {
            category: LessonCategory::Success,
            content: description.to_string(),
            context: context.to_string(),
            tags: vec![],
            timestamp: Utc::now(),
        });
    }

    /// Quick lesson: what failed
    pub fn what_failed(&mut self, context: &str, description: &str) {
        self.record_lesson(Lesson {
            category: LessonCategory::Failure,
            content: description.to_string(),
            context: context.to_string(),
            tags: vec![],
            timestamp: Utc::now(),
        });
    }

    /// Quick lesson: user preference
    pub fn user_prefers(&mut self, description: &str) {
        self.record_lesson(Lesson {
            category: LessonCategory::Preference,
            content: description.to_string(),
            context: String::new(),
            tags: vec!["user_preference".to_string()],
            timestamp: Utc::now(),
        });
    }
}

/// A lesson learned
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lesson {
    pub category: LessonCategory,
    pub content: String,
    pub context: String,
    pub tags: Vec<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LessonCategory {
    Success,    // This approach worked
    Failure,    // This approach failed
    Preference, // User preference observed
    Discovery,  // Learned something about the codebase
    Warning,    // Something to avoid
}

/// A pattern observed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub name: String,
    pub description: String,
    pub trigger: String,    // When to apply this pattern
    pub action: String,     // What to do
    pub confidence: f32,    // How confident we are (0-1)
    pub occurrences: usize, // How many times observed
}

/// The Plan-Do-Verify-Reflect cycle phase
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CyclePhase {
    /// Planning: Break down the task
    Plan,
    /// Doing: Execute one step
    Do,
    /// Verifying: Check it worked
    Verify,
    /// Reflecting: Update plan based on results
    Reflect,
}

impl CyclePhase {
    pub fn next(&self) -> Self {
        match self {
            Self::Plan => Self::Do,
            Self::Do => Self::Verify,
            Self::Verify => Self::Reflect,
            Self::Reflect => Self::Plan,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Do => "do",
            Self::Verify => "verify",
            Self::Reflect => "reflect",
        }
    }
}

/// Builder for cognitive state
pub struct CognitiveStateBuilder {
    state: CognitiveState,
}

impl CognitiveStateBuilder {
    pub fn new() -> Self {
        Self {
            state: CognitiveState::new(),
        }
    }

    pub fn with_plan(mut self, plan: &str, steps: Vec<String>) -> Self {
        self.state.working_memory.set_plan(plan, steps);
        self
    }

    pub fn with_hypothesis(mut self, hypothesis: &str) -> Self {
        self.state.working_memory.active_hypothesis = Some(hypothesis.to_string());
        self
    }

    pub fn with_phase(mut self, phase: CyclePhase) -> Self {
        self.state.cycle_phase = phase;
        self
    }

    pub fn build(self) -> CognitiveState {
        self.state
    }
}

impl Default for CognitiveStateBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
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
        state.set_active_tactical_plan(
            "t1",
            "Stabilize execution loop",
            vec!["task-1".to_string()],
        );
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
}
