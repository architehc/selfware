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
        let content = crate::tools::file::read_to_string_capped(
            path.as_ref(),
            crate::tools::file::MAX_SOURCE_READ_SIZE,
        )?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Save to a file
    pub fn save(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let path_ref = path.as_ref();
        if let Some(parent) = path_ref.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let content = serde_json::to_string_pretty(self)?;
        crate::session::checkpoint::write_bytes_atomically(path_ref, content.as_bytes(), 0o600)?;
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
                .map(|g| format!("- [{:?}] {}", g.status, g.description))
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
            self.episodic_memory
                .sanitized_recent_lessons_for_workspace(
                    3,
                    &crate::tools::workspace_root::current_path(),
                    true,
                )
                .join("\n- "),
        )
    }
}

/// Maximum number of approach attempts retained in `WorkingMemory`.
const MAX_APPROACH_DEPTH: usize = 20;

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

    /// Add an open question (capped at `MAX_OPEN_QUESTIONS`, evicting oldest).
    pub fn add_question(&mut self, question: &str) {
        const MAX_OPEN_QUESTIONS: usize = 50;
        if !self.open_questions.contains(&question.to_string()) {
            if self.open_questions.len() >= MAX_OPEN_QUESTIONS {
                self.open_questions.remove(0);
            }
            self.open_questions.push(question.to_string());
        }
    }

    /// Resolve a question
    pub fn resolve_question(&mut self, question: &str) {
        self.open_questions.retain(|q| q != question);
    }

    /// Add a discovered fact (capped at `MAX_DISCOVERED_FACTS`, evicting oldest).
    pub fn add_fact(&mut self, fact: &str) {
        const MAX_DISCOVERED_FACTS: usize = 100;
        if !self.discovered_facts.contains(&fact.to_string()) {
            if self.discovered_facts.len() >= MAX_DISCOVERED_FACTS {
                self.discovered_facts.remove(0);
            }
            self.discovered_facts.push(fact.to_string());
        }
    }

    /// Push an approach attempt for backtracking.
    ///
    /// The stack is capped at `MAX_APPROACH_DEPTH`; when the cap is reached,
    /// the oldest attempt is discarded so the structure cannot grow without
    /// bound during long retry loops.
    pub fn push_approach(&mut self, description: &str, files_modified: Vec<String>) {
        if self.approach_stack.len() >= MAX_APPROACH_DEPTH {
            self.approach_stack.remove(0);
        }
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

    /// Record a lesson learned (capped at `MAX_EPISODIC_LESSONS` with FIFO eviction of oldest).
    pub fn record_lesson(&mut self, lesson: Lesson) {
        const MAX_EPISODIC_LESSONS: usize = 500;
        // Avoid duplicates
        if !self.lessons.iter().any(|l| l.content == lesson.content) {
            if self.lessons.len() >= MAX_EPISODIC_LESSONS {
                self.lessons.remove(0);
            }
            self.lessons.push(lesson);
        }
    }

    /// Record a pattern (capped at `MAX_EPISODIC_PATTERNS` with occurrences-based eviction).
    pub fn record_pattern(&mut self, pattern: Pattern) {
        const MAX_EPISODIC_PATTERNS: usize = 200;
        if !self.patterns.iter().any(|p| p.name == pattern.name) {
            if self.patterns.len() >= MAX_EPISODIC_PATTERNS {
                if let Some((min_idx, _)) = self
                    .patterns
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, p)| p.occurrences)
                {
                    self.patterns.remove(min_idx);
                }
            }
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

    /// Recent lessons scoped to `workspace` (or untagged global lessons) and
    /// sanitized through redaction, trust-gating, and special-token stripping.
    pub fn sanitized_recent_lessons_for_workspace(
        &self,
        n: usize,
        workspace: &std::path::Path,
        trust_gate_enabled: bool,
    ) -> Vec<String> {
        let ws_tag = format!("workspace:{}", workspace.display());
        self.lessons
            .iter()
            .rev()
            .filter(|l| {
                let mut has_ws_tag = false;
                let mut matches_ws = false;
                for t in &l.tags {
                    if t.starts_with("workspace:") {
                        has_ws_tag = true;
                        if t == &ws_tag {
                            matches_ws = true;
                        }
                    }
                }
                !has_ws_tag || matches_ws
            })
            .filter_map(|l| {
                let formatted = format!("[{:?}] {}", l.category, l.content);
                let redacted = crate::safety::redact::redact_for_model(
                    &formatted,
                    crate::safety::redact::RedactionContext::Generic,
                )
                .content;
                let gate = crate::safety::source_context::trust_gate_tool_result(
                    "episodic_lesson",
                    "{}",
                    &redacted,
                    trust_gate_enabled,
                );
                if gate.sanitized > 0 {
                    tracing::warn!(
                        "Dropped a global lesson from prompt/summary: trust gate found {:?}",
                        gate.kinds
                    );
                    return None;
                }
                Some(
                    crate::agent::result_envelope::strip_special_token_openers(&gate.content)
                        .chars()
                        .map(|c| if c.is_control() { ' ' } else { c })
                        .take(200)
                        .collect(),
                )
            })
            .take(n)
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
        let ws = crate::tools::workspace_root::current_path();
        self.record_lesson(Lesson {
            category: LessonCategory::Success,
            content: description.to_string(),
            context: context.to_string(),
            tags: vec![format!("workspace:{}", ws.display())],
            timestamp: Utc::now(),
        });
    }

    /// Quick lesson: what failed
    pub fn what_failed(&mut self, context: &str, description: &str) {
        let ws = crate::tools::workspace_root::current_path();
        self.record_lesson(Lesson {
            category: LessonCategory::Failure,
            content: description.to_string(),
            context: context.to_string(),
            tags: vec![format!("workspace:{}", ws.display())],
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
#[path = "../../tests/unit/cognitive/state/state_test.rs"]
mod tests;
