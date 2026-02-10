//! Hierarchical Planning System
//!
//! Strategic → Tactical → Operational planning layers with PDVR cycle
//! (Plan-Do-Verify-Reflect) integration for autonomous coding tasks.
//!
//! Features:
//! - Three-layer planning hierarchy
//! - Goal decomposition
//! - Plan validation and refinement
//! - Progress tracking
//! - Adaptive replanning

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Planning layer level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum PlanningLayer {
    /// High-level goals and objectives
    Strategic,
    /// Multi-step plans to achieve goals
    Tactical,
    /// Immediate actions and tool calls
    #[default]
    Operational,
}

/// PDVR cycle phase
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PdvrPhase {
    /// Planning phase - analyze and create plan
    #[default]
    Plan,
    /// Execution phase - carry out actions
    Do,
    /// Verification phase - check results
    Verify,
    /// Reflection phase - learn and adapt
    Reflect,
}

impl PdvrPhase {
    /// Get next phase in cycle
    pub fn next(&self) -> Self {
        match self {
            Self::Plan => Self::Do,
            Self::Do => Self::Verify,
            Self::Verify => Self::Reflect,
            Self::Reflect => Self::Plan,
        }
    }

    /// Get phase name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Plan => "Plan",
            Self::Do => "Do",
            Self::Verify => "Verify",
            Self::Reflect => "Reflect",
        }
    }
}

/// Goal status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum GoalStatus {
    /// Not yet started
    #[default]
    Pending,
    /// Currently being worked on
    Active,
    /// Successfully completed
    Achieved,
    /// Failed to achieve
    Failed,
    /// Abandoned (no longer relevant)
    Abandoned,
    /// Blocked by dependencies
    Blocked,
}

/// Priority level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum Priority {
    Low = 1,
    #[default]
    Medium = 2,
    High = 3,
    Critical = 4,
}

/// A strategic goal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    /// Unique identifier
    pub id: String,
    /// Goal description
    pub description: String,
    /// Success criteria
    pub success_criteria: Vec<String>,
    /// Priority
    pub priority: Priority,
    /// Status
    pub status: GoalStatus,
    /// Sub-goals (tactical level)
    pub sub_goals: Vec<String>,
    /// Dependencies (goal IDs that must complete first)
    pub dependencies: Vec<String>,
    /// Estimated complexity (1-10)
    pub complexity: u8,
    /// Progress percentage (0-100)
    pub progress: u8,
    /// Created timestamp
    pub created_at: u64,
    /// Updated timestamp
    pub updated_at: u64,
    /// Metadata
    pub metadata: HashMap<String, String>,
}

impl Goal {
    /// Create new goal
    pub fn new(id: impl Into<String>, description: impl Into<String>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            id: id.into(),
            description: description.into(),
            success_criteria: Vec::new(),
            priority: Priority::Medium,
            status: GoalStatus::Pending,
            sub_goals: Vec::new(),
            dependencies: Vec::new(),
            complexity: 5,
            progress: 0,
            created_at: now,
            updated_at: now,
            metadata: HashMap::new(),
        }
    }

    /// Add success criterion
    pub fn with_criterion(mut self, criterion: impl Into<String>) -> Self {
        self.success_criteria.push(criterion.into());
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Add dependency
    pub fn with_dependency(mut self, goal_id: impl Into<String>) -> Self {
        self.dependencies.push(goal_id.into());
        self
    }

    /// Set complexity
    pub fn with_complexity(mut self, complexity: u8) -> Self {
        self.complexity = complexity.min(10);
        self
    }

    /// Check if goal can be started (dependencies met)
    pub fn can_start(&self, completed_goals: &[String]) -> bool {
        self.dependencies
            .iter()
            .all(|dep| completed_goals.contains(dep))
    }

    /// Update progress
    pub fn set_progress(&mut self, progress: u8) {
        self.progress = progress.min(100);
        self.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Mark as active
    pub fn activate(&mut self) {
        self.status = GoalStatus::Active;
        self.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Mark as achieved
    pub fn achieve(&mut self) {
        self.status = GoalStatus::Achieved;
        self.progress = 100;
        self.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Mark as failed
    pub fn fail(&mut self, reason: impl Into<String>) {
        self.status = GoalStatus::Failed;
        self.metadata
            .insert("failure_reason".to_string(), reason.into());
        self.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }
}

/// A tactical plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    /// Unique identifier
    pub id: String,
    /// Plan name
    pub name: String,
    /// Description
    pub description: String,
    /// Parent goal ID
    pub goal_id: String,
    /// Steps in the plan
    pub steps: Vec<PlanStep>,
    /// Current step index
    pub current_step: usize,
    /// Status
    pub status: GoalStatus,
    /// Created timestamp
    pub created_at: u64,
    /// Estimated duration in seconds
    pub estimated_duration_secs: Option<u64>,
    /// Actual duration so far
    pub actual_duration_secs: u64,
}

impl Plan {
    /// Create new plan
    pub fn new(id: impl Into<String>, name: impl Into<String>, goal_id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            goal_id: goal_id.into(),
            steps: Vec::new(),
            current_step: 0,
            status: GoalStatus::Pending,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            estimated_duration_secs: None,
            actual_duration_secs: 0,
        }
    }

    /// Add step
    pub fn add_step(&mut self, step: PlanStep) {
        self.steps.push(step);
    }

    /// Get current step
    pub fn current(&self) -> Option<&PlanStep> {
        self.steps.get(self.current_step)
    }

    /// Get current step mutably
    pub fn current_mut(&mut self) -> Option<&mut PlanStep> {
        self.steps.get_mut(self.current_step)
    }

    /// Advance to next step
    pub fn advance(&mut self) -> bool {
        if self.current_step < self.steps.len() - 1 {
            self.current_step += 1;
            true
        } else {
            false
        }
    }

    /// Get progress percentage
    pub fn progress(&self) -> u8 {
        if self.steps.is_empty() {
            return 0;
        }

        let completed = self
            .steps
            .iter()
            .filter(|s| s.status == GoalStatus::Achieved)
            .count();

        ((completed * 100) / self.steps.len()) as u8
    }

    /// Check if plan is complete
    pub fn is_complete(&self) -> bool {
        self.steps.iter().all(|s| s.status == GoalStatus::Achieved)
    }

    /// Check if plan has failed
    pub fn has_failed(&self) -> bool {
        self.steps.iter().any(|s| s.status == GoalStatus::Failed)
    }
}

/// A step in a tactical plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// Step ID
    pub id: String,
    /// Step description
    pub description: String,
    /// Expected action type
    pub action_type: ActionType,
    /// Status
    pub status: GoalStatus,
    /// Verification criteria
    pub verification: Option<String>,
    /// Error message if failed
    pub error: Option<String>,
    /// Duration in milliseconds
    pub duration_ms: Option<u64>,
    /// Retry count
    pub retry_count: u32,
    /// Max retries allowed
    pub max_retries: u32,
}

impl PlanStep {
    /// Create new step
    pub fn new(id: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            action_type: ActionType::General,
            status: GoalStatus::Pending,
            verification: None,
            error: None,
            duration_ms: None,
            retry_count: 0,
            max_retries: 3,
        }
    }

    /// Set action type
    pub fn with_action(mut self, action_type: ActionType) -> Self {
        self.action_type = action_type;
        self
    }

    /// Set verification
    pub fn with_verification(mut self, verification: impl Into<String>) -> Self {
        self.verification = Some(verification.into());
        self
    }

    /// Mark as complete
    pub fn complete(&mut self, duration_ms: u64) {
        self.status = GoalStatus::Achieved;
        self.duration_ms = Some(duration_ms);
    }

    /// Mark as failed
    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = GoalStatus::Failed;
        self.error = Some(error.into());
    }

    /// Can retry
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }

    /// Increment retry
    pub fn retry(&mut self) {
        self.retry_count += 1;
        self.status = GoalStatus::Pending;
        self.error = None;
    }
}

/// Action type for operational layer
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum ActionType {
    /// General action
    #[default]
    General,
    /// File read operation
    FileRead,
    /// File write operation
    FileWrite,
    /// File edit operation
    FileEdit,
    /// Shell command
    Shell,
    /// Git operation
    Git,
    /// Search operation
    Search,
    /// LLM call
    LlmCall,
    /// User interaction
    UserInput,
    /// Verification check
    Verification,
}

/// An operational action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    /// Action ID
    pub id: String,
    /// Parent plan step ID
    pub step_id: String,
    /// Action type
    pub action_type: ActionType,
    /// Action details/parameters
    pub details: HashMap<String, String>,
    /// Status
    pub status: GoalStatus,
    /// Result
    pub result: Option<String>,
    /// Error
    pub error: Option<String>,
    /// Started at
    pub started_at: Option<u64>,
    /// Completed at
    pub completed_at: Option<u64>,
}

impl Action {
    /// Create new action
    pub fn new(id: impl Into<String>, step_id: impl Into<String>, action_type: ActionType) -> Self {
        Self {
            id: id.into(),
            step_id: step_id.into(),
            action_type,
            details: HashMap::new(),
            status: GoalStatus::Pending,
            result: None,
            error: None,
            started_at: None,
            completed_at: None,
        }
    }

    /// Add detail
    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }

    /// Start action
    pub fn start(&mut self) {
        self.status = GoalStatus::Active;
        self.started_at = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
    }

    /// Complete action
    pub fn complete(&mut self, result: impl Into<String>) {
        self.status = GoalStatus::Achieved;
        self.result = Some(result.into());
        self.completed_at = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
    }

    /// Fail action
    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = GoalStatus::Failed;
        self.error = Some(error.into());
        self.completed_at = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
    }

    /// Get duration in seconds
    pub fn duration_secs(&self) -> Option<u64> {
        match (self.started_at, self.completed_at) {
            (Some(start), Some(end)) => Some(end.saturating_sub(start)),
            _ => None,
        }
    }
}

/// PDVR cycle tracker
#[derive(Debug, Clone)]
pub struct PdvrCycle {
    /// Current phase
    pub phase: PdvrPhase,
    /// Phase history
    history: Vec<PhaseRecord>,
    /// Cycle count
    pub cycle_count: u32,
    /// Current cycle start
    cycle_start: Option<Instant>,
    /// Insights gathered during reflect phase
    pub insights: Vec<String>,
}

/// Record of a phase execution
#[derive(Debug, Clone)]
pub struct PhaseRecord {
    pub phase: PdvrPhase,
    pub cycle: u32,
    pub duration_ms: u64,
    pub success: bool,
    pub notes: String,
}

impl PdvrCycle {
    /// Create new cycle
    pub fn new() -> Self {
        Self {
            phase: PdvrPhase::Plan,
            history: Vec::new(),
            cycle_count: 0,
            cycle_start: Some(Instant::now()),
            insights: Vec::new(),
        }
    }

    /// Advance to next phase
    pub fn advance(&mut self, success: bool, notes: impl Into<String>) {
        let duration = self
            .cycle_start
            .map(|s| s.elapsed().as_millis() as u64)
            .unwrap_or(0);

        self.history.push(PhaseRecord {
            phase: self.phase,
            cycle: self.cycle_count,
            duration_ms: duration,
            success,
            notes: notes.into(),
        });

        self.phase = self.phase.next();

        if self.phase == PdvrPhase::Plan {
            self.cycle_count += 1;
        }

        self.cycle_start = Some(Instant::now());
    }

    /// Add insight from reflection
    pub fn add_insight(&mut self, insight: impl Into<String>) {
        self.insights.push(insight.into());
    }

    /// Get phase history
    pub fn history(&self) -> &[PhaseRecord] {
        &self.history
    }

    /// Get average phase duration
    pub fn average_phase_duration(&self, phase: PdvrPhase) -> Option<Duration> {
        let durations: Vec<u64> = self
            .history
            .iter()
            .filter(|r| r.phase == phase)
            .map(|r| r.duration_ms)
            .collect();

        if durations.is_empty() {
            None
        } else {
            let avg = durations.iter().sum::<u64>() / durations.len() as u64;
            Some(Duration::from_millis(avg))
        }
    }

    /// Get success rate for phase
    pub fn phase_success_rate(&self, phase: PdvrPhase) -> f32 {
        let phase_records: Vec<&PhaseRecord> =
            self.history.iter().filter(|r| r.phase == phase).collect();

        if phase_records.is_empty() {
            return 1.0;
        }

        let successes = phase_records.iter().filter(|r| r.success).count();
        successes as f32 / phase_records.len() as f32
    }
}

impl Default for PdvrCycle {
    fn default() -> Self {
        Self::new()
    }
}

/// Hierarchical planner
pub struct HierarchicalPlanner {
    /// Strategic goals
    goals: HashMap<String, Goal>,
    /// Tactical plans
    plans: HashMap<String, Plan>,
    /// Operational actions queue
    action_queue: VecDeque<Action>,
    /// Completed actions
    completed_actions: Vec<Action>,
    /// PDVR cycle
    pdvr: PdvrCycle,
    /// Planning constraints
    constraints: PlanningConstraints,
    /// Goal ID counter
    goal_counter: u32,
    /// Plan ID counter
    plan_counter: u32,
}

/// Planning constraints
#[derive(Debug, Clone)]
pub struct PlanningConstraints {
    /// Maximum concurrent goals
    pub max_concurrent_goals: usize,
    /// Maximum plan depth
    pub max_plan_depth: usize,
    /// Maximum actions in queue
    pub max_action_queue: usize,
    /// Replan threshold (failure rate)
    pub replan_threshold: f32,
    /// Maximum retries per step
    pub max_retries: u32,
}

impl Default for PlanningConstraints {
    fn default() -> Self {
        Self {
            max_concurrent_goals: 3,
            max_plan_depth: 10,
            max_action_queue: 50,
            replan_threshold: 0.3,
            max_retries: 3,
        }
    }
}

impl HierarchicalPlanner {
    /// Create new planner
    pub fn new() -> Self {
        Self {
            goals: HashMap::new(),
            plans: HashMap::new(),
            action_queue: VecDeque::new(),
            completed_actions: Vec::new(),
            pdvr: PdvrCycle::new(),
            constraints: PlanningConstraints::default(),
            goal_counter: 0,
            plan_counter: 0,
        }
    }

    /// Set constraints
    pub fn with_constraints(mut self, constraints: PlanningConstraints) -> Self {
        self.constraints = constraints;
        self
    }

    /// Add a strategic goal
    pub fn add_goal(&mut self, mut goal: Goal) -> String {
        self.goal_counter += 1;
        if goal.id.is_empty() {
            goal.id = format!("goal_{}", self.goal_counter);
        }
        let id = goal.id.clone();
        self.goals.insert(id.clone(), goal);
        id
    }

    /// Get goal by ID
    pub fn get_goal(&self, id: &str) -> Option<&Goal> {
        self.goals.get(id)
    }

    /// Get goal mutably
    pub fn get_goal_mut(&mut self, id: &str) -> Option<&mut Goal> {
        self.goals.get_mut(id)
    }

    /// List all goals
    pub fn list_goals(&self) -> Vec<&Goal> {
        self.goals.values().collect()
    }

    /// List goals by status
    pub fn goals_by_status(&self, status: GoalStatus) -> Vec<&Goal> {
        self.goals.values().filter(|g| g.status == status).collect()
    }

    /// Get next goal to work on
    pub fn next_goal(&self) -> Option<&Goal> {
        let completed: Vec<String> = self
            .goals
            .values()
            .filter(|g| g.status == GoalStatus::Achieved)
            .map(|g| g.id.clone())
            .collect();

        self.goals
            .values()
            .filter(|g| g.status == GoalStatus::Pending && g.can_start(&completed))
            .max_by_key(|g| g.priority)
    }

    /// Create a plan for a goal
    pub fn create_plan(&mut self, goal_id: &str, steps: Vec<PlanStep>) -> Result<String> {
        if !self.goals.contains_key(goal_id) {
            return Err(anyhow!("Goal not found: {}", goal_id));
        }

        if steps.len() > self.constraints.max_plan_depth {
            return Err(anyhow!(
                "Plan exceeds max depth of {}",
                self.constraints.max_plan_depth
            ));
        }

        self.plan_counter += 1;
        let plan_id = format!("plan_{}", self.plan_counter);

        let mut plan = Plan::new(&plan_id, &plan_id, goal_id);
        for step in steps {
            plan.add_step(step);
        }

        // Link plan to goal
        if let Some(goal) = self.goals.get_mut(goal_id) {
            goal.sub_goals.push(plan_id.clone());
        }

        self.plans.insert(plan_id.clone(), plan);
        Ok(plan_id)
    }

    /// Get plan by ID
    pub fn get_plan(&self, id: &str) -> Option<&Plan> {
        self.plans.get(id)
    }

    /// Get plan mutably
    pub fn get_plan_mut(&mut self, id: &str) -> Option<&mut Plan> {
        self.plans.get_mut(id)
    }

    /// Queue an action
    pub fn queue_action(&mut self, action: Action) -> Result<()> {
        if self.action_queue.len() >= self.constraints.max_action_queue {
            return Err(anyhow!("Action queue is full"));
        }
        self.action_queue.push_back(action);
        Ok(())
    }

    /// Get next action
    pub fn next_action(&mut self) -> Option<Action> {
        self.action_queue.pop_front()
    }

    /// Peek next action
    pub fn peek_action(&self) -> Option<&Action> {
        self.action_queue.front()
    }

    /// Complete an action
    pub fn complete_action(&mut self, mut action: Action, result: impl Into<String>) {
        action.complete(result);
        self.completed_actions.push(action);
    }

    /// Fail an action
    pub fn fail_action(&mut self, mut action: Action, error: impl Into<String>) {
        action.fail(error);
        self.completed_actions.push(action);
    }

    /// Get PDVR cycle
    pub fn pdvr(&self) -> &PdvrCycle {
        &self.pdvr
    }

    /// Get PDVR cycle mutably
    pub fn pdvr_mut(&mut self) -> &mut PdvrCycle {
        &mut self.pdvr
    }

    /// Advance PDVR phase
    pub fn advance_pdvr(&mut self, success: bool, notes: impl Into<String>) {
        self.pdvr.advance(success, notes);
    }

    /// Check if replanning is needed
    pub fn needs_replan(&self, plan_id: &str) -> bool {
        if let Some(plan) = self.plans.get(plan_id) {
            let failed = plan
                .steps
                .iter()
                .filter(|s| s.status == GoalStatus::Failed)
                .count();

            let total = plan.steps.len();
            if total == 0 {
                return false;
            }

            let failure_rate = failed as f32 / total as f32;
            failure_rate >= self.constraints.replan_threshold
        } else {
            false
        }
    }

    /// Decompose a goal into sub-goals
    pub fn decompose_goal(&mut self, goal_id: &str, sub_goals: Vec<Goal>) -> Result<Vec<String>> {
        if !self.goals.contains_key(goal_id) {
            return Err(anyhow!("Goal not found: {}", goal_id));
        }

        let mut sub_goal_ids = Vec::new();

        for sub_goal in sub_goals {
            let id = self.add_goal(sub_goal);
            sub_goal_ids.push(id);
        }

        // Link sub-goals to parent
        if let Some(goal) = self.goals.get_mut(goal_id) {
            goal.sub_goals.extend(sub_goal_ids.clone());
        }

        Ok(sub_goal_ids)
    }

    /// Get planning summary
    pub fn summary(&self) -> PlanningSummary {
        let goal_counts: HashMap<GoalStatus, usize> =
            self.goals.values().fold(HashMap::new(), |mut acc, g| {
                *acc.entry(g.status).or_insert(0) += 1;
                acc
            });

        let plan_progress: Vec<u8> = self.plans.values().map(|p| p.progress()).collect();
        let avg_progress = if plan_progress.is_empty() {
            0
        } else {
            plan_progress.iter().map(|&p| p as u32).sum::<u32>() / plan_progress.len() as u32
        };

        PlanningSummary {
            total_goals: self.goals.len(),
            active_goals: *goal_counts.get(&GoalStatus::Active).unwrap_or(&0),
            completed_goals: *goal_counts.get(&GoalStatus::Achieved).unwrap_or(&0),
            failed_goals: *goal_counts.get(&GoalStatus::Failed).unwrap_or(&0),
            total_plans: self.plans.len(),
            average_progress: avg_progress as u8,
            queued_actions: self.action_queue.len(),
            completed_actions: self.completed_actions.len(),
            pdvr_phase: self.pdvr.phase,
            pdvr_cycle: self.pdvr.cycle_count,
        }
    }

    /// Clear completed items
    pub fn cleanup(&mut self) {
        // Remove achieved goals
        self.goals
            .retain(|_, g| g.status != GoalStatus::Achieved && g.status != GoalStatus::Abandoned);

        // Remove completed plans
        self.plans.retain(|_, p| !p.is_complete());

        // Limit completed actions history
        if self.completed_actions.len() > 100 {
            self.completed_actions.drain(0..50);
        }
    }
}

impl Default for HierarchicalPlanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Planning summary
#[derive(Debug, Clone)]
pub struct PlanningSummary {
    pub total_goals: usize,
    pub active_goals: usize,
    pub completed_goals: usize,
    pub failed_goals: usize,
    pub total_plans: usize,
    pub average_progress: u8,
    pub queued_actions: usize,
    pub completed_actions: usize,
    pub pdvr_phase: PdvrPhase,
    pub pdvr_cycle: u32,
}

/// Goal decomposition strategies
pub struct GoalDecomposer;

impl GoalDecomposer {
    /// Decompose a coding task goal
    pub fn decompose_coding_task(description: &str) -> Vec<Goal> {
        vec![
            Goal::new("understand", "Understand the requirements and codebase")
                .with_criterion("Requirements are clear")
                .with_criterion("Relevant code is identified")
                .with_complexity(3),
            Goal::new("design", "Design the solution approach")
                .with_criterion("Approach is documented")
                .with_criterion("Edge cases are considered")
                .with_dependency("understand")
                .with_complexity(4),
            Goal::new("implement", "Implement the solution")
                .with_criterion("Code is written")
                .with_criterion("Code compiles")
                .with_dependency("design")
                .with_complexity(6),
            Goal::new("test", "Write and run tests")
                .with_criterion("Tests are written")
                .with_criterion("Tests pass")
                .with_dependency("implement")
                .with_complexity(4),
            Goal::new(
                "verify",
                format!("Verify the solution meets: {}", description),
            )
            .with_criterion("All requirements met")
            .with_criterion("No regressions")
            .with_dependency("test")
            .with_complexity(3),
        ]
    }

    /// Decompose a bug fix goal
    pub fn decompose_bug_fix(description: &str) -> Vec<Goal> {
        vec![
            Goal::new("reproduce", "Reproduce the bug")
                .with_criterion("Bug is consistently reproducible")
                .with_complexity(4),
            Goal::new("diagnose", "Identify root cause")
                .with_criterion("Root cause is identified")
                .with_dependency("reproduce")
                .with_complexity(5),
            Goal::new("fix", format!("Fix: {}", description))
                .with_criterion("Fix is implemented")
                .with_criterion("Code compiles")
                .with_dependency("diagnose")
                .with_complexity(5),
            Goal::new("verify_fix", "Verify the fix")
                .with_criterion("Bug no longer occurs")
                .with_criterion("No new bugs introduced")
                .with_dependency("fix")
                .with_complexity(3),
        ]
    }

    /// Decompose a refactoring goal
    pub fn decompose_refactor(description: &str) -> Vec<Goal> {
        vec![
            Goal::new("baseline", "Establish baseline tests")
                .with_criterion("Existing tests pass")
                .with_criterion("Coverage is documented")
                .with_complexity(3),
            Goal::new("identify", "Identify refactoring opportunities")
                .with_criterion("Changes are planned")
                .with_dependency("baseline")
                .with_complexity(4),
            Goal::new("refactor", format!("Apply refactoring: {}", description))
                .with_criterion("Refactoring is complete")
                .with_criterion("Code compiles")
                .with_dependency("identify")
                .with_complexity(6),
            Goal::new("verify_refactor", "Verify no regressions")
                .with_criterion("All tests pass")
                .with_criterion("Behavior is unchanged")
                .with_dependency("refactor")
                .with_complexity(3),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_planning_layer_default() {
        assert_eq!(PlanningLayer::default(), PlanningLayer::Operational);
    }

    #[test]
    fn test_pdvr_phase_cycle() {
        assert_eq!(PdvrPhase::Plan.next(), PdvrPhase::Do);
        assert_eq!(PdvrPhase::Do.next(), PdvrPhase::Verify);
        assert_eq!(PdvrPhase::Verify.next(), PdvrPhase::Reflect);
        assert_eq!(PdvrPhase::Reflect.next(), PdvrPhase::Plan);
    }

    #[test]
    fn test_pdvr_phase_name() {
        assert_eq!(PdvrPhase::Plan.name(), "Plan");
        assert_eq!(PdvrPhase::Do.name(), "Do");
    }

    #[test]
    fn test_goal_creation() {
        let goal = Goal::new("test", "Test goal")
            .with_criterion("Must pass")
            .with_priority(Priority::High)
            .with_complexity(7);

        assert_eq!(goal.id, "test");
        assert_eq!(goal.priority, Priority::High);
        assert_eq!(goal.complexity, 7);
        assert_eq!(goal.success_criteria.len(), 1);
    }

    #[test]
    fn test_goal_dependencies() {
        let goal = Goal::new("test", "Test").with_dependency("other");

        let completed = vec!["other".to_string()];
        assert!(goal.can_start(&completed));

        let not_completed: Vec<String> = vec![];
        assert!(!goal.can_start(&not_completed));
    }

    #[test]
    fn test_goal_progress() {
        let mut goal = Goal::new("test", "Test");
        goal.set_progress(50);
        assert_eq!(goal.progress, 50);

        goal.set_progress(150); // Should cap at 100
        assert_eq!(goal.progress, 100);
    }

    #[test]
    fn test_goal_status_transitions() {
        let mut goal = Goal::new("test", "Test");
        assert_eq!(goal.status, GoalStatus::Pending);

        goal.activate();
        assert_eq!(goal.status, GoalStatus::Active);

        goal.achieve();
        assert_eq!(goal.status, GoalStatus::Achieved);
        assert_eq!(goal.progress, 100);
    }

    #[test]
    fn test_goal_failure() {
        let mut goal = Goal::new("test", "Test");
        goal.fail("Something went wrong");

        assert_eq!(goal.status, GoalStatus::Failed);
        assert_eq!(
            goal.metadata.get("failure_reason"),
            Some(&"Something went wrong".to_string())
        );
    }

    #[test]
    fn test_plan_creation() {
        let mut plan = Plan::new("plan1", "Test Plan", "goal1");
        plan.add_step(PlanStep::new("step1", "First step"));
        plan.add_step(PlanStep::new("step2", "Second step"));

        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.current_step, 0);
    }

    #[test]
    fn test_plan_progress() {
        let mut plan = Plan::new("plan1", "Test Plan", "goal1");
        plan.add_step(PlanStep::new("step1", "First"));
        plan.add_step(PlanStep::new("step2", "Second"));

        assert_eq!(plan.progress(), 0);

        plan.steps[0].status = GoalStatus::Achieved;
        assert_eq!(plan.progress(), 50);

        plan.steps[1].status = GoalStatus::Achieved;
        assert_eq!(plan.progress(), 100);
        assert!(plan.is_complete());
    }

    #[test]
    fn test_plan_advance() {
        let mut plan = Plan::new("plan1", "Test Plan", "goal1");
        plan.add_step(PlanStep::new("step1", "First"));
        plan.add_step(PlanStep::new("step2", "Second"));

        assert_eq!(plan.current_step, 0);
        assert!(plan.advance());
        assert_eq!(plan.current_step, 1);
        assert!(!plan.advance()); // At end
    }

    #[test]
    fn test_plan_step_retry() {
        let mut step = PlanStep::new("step1", "Test step");
        step.max_retries = 2;

        assert!(step.can_retry());
        step.retry();
        assert_eq!(step.retry_count, 1);
        assert!(step.can_retry());
        step.retry();
        assert!(!step.can_retry());
    }

    #[test]
    fn test_action_lifecycle() {
        let mut action = Action::new("action1", "step1", ActionType::FileRead)
            .with_detail("path", "/tmp/test.txt");

        assert_eq!(action.status, GoalStatus::Pending);

        action.start();
        assert_eq!(action.status, GoalStatus::Active);
        assert!(action.started_at.is_some());

        action.complete("File read successfully");
        assert_eq!(action.status, GoalStatus::Achieved);
        assert!(action.completed_at.is_some());
    }

    #[test]
    fn test_action_failure() {
        let mut action = Action::new("action1", "step1", ActionType::Shell);
        action.start();
        action.fail("Command failed");

        assert_eq!(action.status, GoalStatus::Failed);
        assert_eq!(action.error, Some("Command failed".to_string()));
    }

    #[test]
    fn test_pdvr_cycle() {
        let mut cycle = PdvrCycle::new();
        assert_eq!(cycle.phase, PdvrPhase::Plan);
        assert_eq!(cycle.cycle_count, 0);

        cycle.advance(true, "Plan complete");
        assert_eq!(cycle.phase, PdvrPhase::Do);

        cycle.advance(true, "Do complete");
        cycle.advance(true, "Verify complete");
        cycle.advance(true, "Reflect complete");

        // Back to Plan, cycle incremented
        assert_eq!(cycle.phase, PdvrPhase::Plan);
        assert_eq!(cycle.cycle_count, 1);
    }

    #[test]
    fn test_pdvr_insights() {
        let mut cycle = PdvrCycle::new();
        cycle.add_insight("Learned something");
        cycle.add_insight("Another insight");

        assert_eq!(cycle.insights.len(), 2);
    }

    #[test]
    fn test_pdvr_success_rate() {
        let mut cycle = PdvrCycle::new();
        cycle.advance(true, "Success");
        cycle.advance(true, "Success");
        cycle.advance(false, "Failed");
        cycle.advance(true, "Success");

        // Plan succeeded once
        assert!((cycle.phase_success_rate(PdvrPhase::Plan) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_hierarchical_planner_creation() {
        let planner = HierarchicalPlanner::new();
        assert_eq!(planner.list_goals().len(), 0);
    }

    #[test]
    fn test_planner_add_goal() {
        let mut planner = HierarchicalPlanner::new();
        let goal = Goal::new("test", "Test goal");
        let id = planner.add_goal(goal);

        assert!(planner.get_goal(&id).is_some());
        assert_eq!(planner.list_goals().len(), 1);
    }

    #[test]
    fn test_planner_goal_auto_id() {
        let mut planner = HierarchicalPlanner::new();
        let goal = Goal::new("", "Test goal"); // Empty ID
        let id = planner.add_goal(goal);

        assert!(id.starts_with("goal_"));
    }

    #[test]
    fn test_planner_create_plan() {
        let mut planner = HierarchicalPlanner::new();
        let goal = Goal::new("goal1", "Test goal");
        planner.add_goal(goal);

        let steps = vec![
            PlanStep::new("step1", "First step"),
            PlanStep::new("step2", "Second step"),
        ];

        let plan_id = planner.create_plan("goal1", steps).unwrap();
        assert!(planner.get_plan(&plan_id).is_some());
    }

    #[test]
    fn test_planner_create_plan_missing_goal() {
        let mut planner = HierarchicalPlanner::new();
        let steps = vec![PlanStep::new("step1", "Step")];

        let result = planner.create_plan("nonexistent", steps);
        assert!(result.is_err());
    }

    #[test]
    fn test_planner_action_queue() {
        let mut planner = HierarchicalPlanner::new();
        let action = Action::new("action1", "step1", ActionType::FileRead);

        planner.queue_action(action).unwrap();
        assert_eq!(planner.action_queue.len(), 1);

        let next = planner.next_action();
        assert!(next.is_some());
        assert_eq!(planner.action_queue.len(), 0);
    }

    #[test]
    fn test_planner_next_goal() {
        let mut planner = HierarchicalPlanner::new();

        let goal1 = Goal::new("goal1", "First").with_priority(Priority::Low);
        let goal2 = Goal::new("goal2", "Second").with_priority(Priority::High);

        planner.add_goal(goal1);
        planner.add_goal(goal2);

        let next = planner.next_goal().unwrap();
        assert_eq!(next.priority, Priority::High);
    }

    #[test]
    fn test_planner_goals_by_status() {
        let mut planner = HierarchicalPlanner::new();

        let mut goal1 = Goal::new("goal1", "Active");
        goal1.status = GoalStatus::Active;

        let goal2 = Goal::new("goal2", "Pending");

        planner.add_goal(goal1);
        planner.add_goal(goal2);

        assert_eq!(planner.goals_by_status(GoalStatus::Active).len(), 1);
        assert_eq!(planner.goals_by_status(GoalStatus::Pending).len(), 1);
    }

    #[test]
    fn test_planner_decompose_goal() {
        let mut planner = HierarchicalPlanner::new();
        let goal = Goal::new("main", "Main goal");
        planner.add_goal(goal);

        let sub_goals = vec![Goal::new("sub1", "Sub 1"), Goal::new("sub2", "Sub 2")];

        let ids = planner.decompose_goal("main", sub_goals).unwrap();
        assert_eq!(ids.len(), 2);

        let main_goal = planner.get_goal("main").unwrap();
        assert_eq!(main_goal.sub_goals.len(), 2);
    }

    #[test]
    fn test_planner_needs_replan() {
        let mut planner = HierarchicalPlanner::new();
        let goal = Goal::new("goal1", "Test");
        planner.add_goal(goal);

        let mut steps = vec![
            PlanStep::new("step1", "Step 1"),
            PlanStep::new("step2", "Step 2"),
            PlanStep::new("step3", "Step 3"),
        ];
        steps[0].status = GoalStatus::Failed;

        let plan_id = planner.create_plan("goal1", steps).unwrap();

        // 1/3 = 33% failure rate, threshold is 30%
        assert!(planner.needs_replan(&plan_id));
    }

    #[test]
    fn test_planner_summary() {
        let mut planner = HierarchicalPlanner::new();
        planner.add_goal(Goal::new("goal1", "Test"));

        let summary = planner.summary();
        assert_eq!(summary.total_goals, 1);
        assert_eq!(summary.pdvr_phase, PdvrPhase::Plan);
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Critical > Priority::High);
        assert!(Priority::High > Priority::Medium);
        assert!(Priority::Medium > Priority::Low);
    }

    #[test]
    fn test_goal_decomposer_coding_task() {
        let goals = GoalDecomposer::decompose_coding_task("Add new feature");
        assert!(!goals.is_empty());
        assert!(goals.iter().any(|g| g.id == "implement"));
    }

    #[test]
    fn test_goal_decomposer_bug_fix() {
        let goals = GoalDecomposer::decompose_bug_fix("Fix null pointer");
        assert!(!goals.is_empty());
        assert!(goals.iter().any(|g| g.id == "diagnose"));
    }

    #[test]
    fn test_goal_decomposer_refactor() {
        let goals = GoalDecomposer::decompose_refactor("Extract method");
        assert!(!goals.is_empty());
        assert!(goals.iter().any(|g| g.id == "baseline"));
    }

    #[test]
    fn test_plan_step_complete() {
        let mut step = PlanStep::new("step1", "Test");
        step.complete(100);

        assert_eq!(step.status, GoalStatus::Achieved);
        assert_eq!(step.duration_ms, Some(100));
    }

    #[test]
    fn test_plan_step_fail() {
        let mut step = PlanStep::new("step1", "Test");
        step.fail("Error occurred");

        assert_eq!(step.status, GoalStatus::Failed);
        assert_eq!(step.error, Some("Error occurred".to_string()));
    }

    #[test]
    fn test_action_type_default() {
        assert_eq!(ActionType::default(), ActionType::General);
    }

    #[test]
    fn test_planning_constraints_default() {
        let constraints = PlanningConstraints::default();
        assert_eq!(constraints.max_concurrent_goals, 3);
        assert_eq!(constraints.max_retries, 3);
    }

    #[test]
    fn test_planner_cleanup() {
        let mut planner = HierarchicalPlanner::new();

        let mut goal = Goal::new("achieved", "Done");
        goal.status = GoalStatus::Achieved;
        planner.add_goal(goal);

        let pending = Goal::new("pending", "Pending");
        planner.add_goal(pending);

        planner.cleanup();

        assert_eq!(planner.list_goals().len(), 1);
        assert!(planner.get_goal("pending").is_some());
    }
}
