//! Plan Mode implementation for Selfware
//!
//! Plan Mode provides an explicit planning phase where the agent can only use
//! read-only tools (file_read, grep_search, etc.) to analyze the codebase before
//! making any modifications. This prevents premature execution and gives users
//! control over the plan before execution begins.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

/// Status of an individual plan step
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepStatus {
    /// Step is pending execution
    Pending,
    /// Step is currently being executed
    InProgress,
    /// Step has been completed
    Done,
    /// Step failed during execution
    Failed(String),
}

impl std::fmt::Display for StepStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StepStatus::Pending => write!(f, "pending"),
            StepStatus::InProgress => write!(f, "in_progress"),
            StepStatus::Done => write!(f, "done"),
            StepStatus::Failed(reason) => write!(f, "failed: {}", reason),
        }
    }
}

/// A single step in the execution plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// Unique identifier for the step
    pub id: usize,
    /// Human-readable description of the step
    pub description: String,
    /// Hint for which tool should be used (e.g., "file_read", "file_edit")
    pub tool_hint: Option<String>,
    /// Current status of the step
    pub status: StepStatus,
    /// Optional file path associated with this step
    pub file_path: Option<String>,
    /// Optional additional context for the step
    pub context: Option<String>,
}

impl PlanStep {
    /// Create a new plan step
    pub fn new(id: usize, description: impl Into<String>) -> Self {
        Self {
            id,
            description: description.into(),
            tool_hint: None,
            status: StepStatus::Pending,
            file_path: None,
            context: None,
        }
    }

    /// Set the tool hint for this step
    pub fn with_tool_hint(mut self, hint: impl Into<String>) -> Self {
        self.tool_hint = Some(hint.into());
        self
    }

    /// Set the file path for this step
    pub fn with_file_path(mut self, path: impl Into<String>) -> Self {
        self.file_path = Some(path.into());
        self
    }

    /// Set additional context for this step
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Mark the step as in progress
    pub fn mark_in_progress(&mut self) {
        self.status = StepStatus::InProgress;
    }

    /// Mark the step as done
    pub fn mark_done(&mut self) {
        self.status = StepStatus::Done;
    }

    /// Mark the step as failed
    pub fn mark_failed(&mut self, reason: impl Into<String>) {
        self.status = StepStatus::Failed(reason.into());
    }
}

/// A structured execution plan
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Plan {
    /// The steps in the plan
    pub steps: Vec<PlanStep>,
    /// Estimated tokens needed for the plan
    pub estimated_tokens: usize,
    /// Files that should be read as part of the plan
    pub files_to_read: Vec<String>,
    /// Brief summary of the plan
    pub summary: String,
    /// When the plan was created
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Plan {
    /// Create a new empty plan
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            estimated_tokens: 0,
            files_to_read: Vec::new(),
            summary: String::new(),
            created_at: Some(chrono::Utc::now()),
        }
    }

    /// Create a plan with a summary
    pub fn with_summary(summary: impl Into<String>) -> Self {
        Self {
            steps: Vec::new(),
            estimated_tokens: 0,
            files_to_read: Vec::new(),
            summary: summary.into(),
            created_at: Some(chrono::Utc::now()),
        }
    }

    /// Add a step to the plan
    pub fn add_step(&mut self, description: impl Into<String>) -> &mut PlanStep {
        let id = self.steps.len() + 1;
        let step = PlanStep::new(id, description);
        self.steps.push(step);
        self.steps.last_mut().unwrap()
    }

    /// Add a file to read
    pub fn add_file_to_read(&mut self, path: impl Into<String>) {
        let path = path.into();
        if !self.files_to_read.contains(&path) {
            self.files_to_read.push(path);
        }
    }

    /// Get the number of steps
    pub fn step_count(&self) -> usize {
        self.steps.len()
    }

    /// Get the number of completed steps
    pub fn completed_count(&self) -> usize {
        self.steps
            .iter()
            .filter(|s| matches!(s.status, StepStatus::Done))
            .count()
    }

    /// Check if all steps are done
    pub fn is_complete(&self) -> bool {
        !self.steps.is_empty() && self.completed_count() == self.steps.len()
    }

    /// Get the next pending step
    pub fn next_pending_step(&self) -> Option<&PlanStep> {
        self.steps.iter().find(|s| matches!(s.status, StepStatus::Pending))
    }

    /// Get the next pending step mutably
    pub fn next_pending_step_mut(&mut self) -> Option<&mut PlanStep> {
        self.steps.iter_mut().find(|s| matches!(s.status, StepStatus::Pending))
    }

    /// Format the plan as a human-readable string
    pub fn format(&self) -> String {
        let mut output = String::new();

        if !self.summary.is_empty() {
            output.push_str(&format!("# Plan Summary\n{}\n\n", self.summary));
        }

        if !self.files_to_read.is_empty() {
            output.push_str("## Files to Analyze\n");
            for file in &self.files_to_read {
                output.push_str(&format!("- {}\n", file));
            }
            output.push('\n');
        }

        if !self.steps.is_empty() {
            output.push_str("## Execution Steps\n");
            for step in &self.steps {
                let status_icon = match step.status {
                    StepStatus::Pending => "⏳",
                    StepStatus::InProgress => "▶️",
                    StepStatus::Done => "✅",
                    StepStatus::Failed(_) => "❌",
                };
                output.push_str(&format!(
                    "{}. {} {}\n",
                    step.id, status_icon, step.description
                ));
                if let Some(ref tool_hint) = step.tool_hint {
                    output.push_str(&format!("   Tool: `{}`\n", tool_hint));
                }
                if let Some(ref path) = step.file_path {
                    output.push_str(&format!("   File: `{}`\n", path));
                }
            }
            output.push('\n');
        }

        if self.estimated_tokens > 0 {
            output.push_str(&format!(
                "Estimated tokens: ~{}\n",
                self.estimated_tokens
            ));
        }

        output
    }
}

/// The state of plan mode
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanModeState {
    /// Plan mode is inactive - normal execution
    Inactive,
    /// Currently in planning phase - only read-only tools allowed
    Planning { plan_text: String },
    /// Plan is approved and executing
    Executing,
}

impl PlanModeState {
    /// Check if currently in planning mode
    pub fn is_planning(&self) -> bool {
        matches!(self, PlanModeState::Planning { .. })
    }

    /// Check if plan mode is active (either planning or executing)
    pub fn is_active(&self) -> bool {
        !matches!(self, PlanModeState::Inactive)
    }
}

/// Manager for plan mode state and operations
#[derive(Debug, Clone)]
pub struct PlanModeManager {
    /// Current state of plan mode
    state: PlanModeState,
    /// The stored plan
    plan: Option<Plan>,
    /// Whether the current plan has been approved for execution
    approved: bool,
}

impl Default for PlanModeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PlanModeManager {
    /// Create a new plan mode manager
    pub fn new() -> Self {
        Self {
            state: PlanModeState::Inactive,
            plan: None,
            approved: false,
        }
    }

    /// Enter plan mode - switch to read-only planning phase
    pub fn enter_plan_mode(&mut self) {
        self.state = PlanModeState::Planning {
            plan_text: String::new(),
        };
        self.approved = false;
    }

    /// Exit plan mode - return to normal execution
    pub fn exit_plan_mode(&mut self) {
        self.state = PlanModeState::Inactive;
        self.approved = false;
    }

    /// Check if currently in plan mode (planning or executing)
    pub fn is_in_plan_mode(&self) -> bool {
        self.state.is_active()
    }

    /// Check if currently in the planning phase (before approval)
    pub fn is_planning(&self) -> bool {
        self.state.is_planning()
    }

    /// Get the current plan mode state
    pub fn state(&self) -> &PlanModeState {
        &self.state
    }

    /// Store a generated plan
    pub fn store_plan(&mut self, plan: Plan) {
        self.plan = Some(plan);
    }

    /// Get the stored plan
    pub fn get_plan(&self) -> Option<&Plan> {
        self.plan.as_ref()
    }

    /// Get the stored plan mutably
    pub fn get_plan_mut(&mut self) -> Option<&mut Plan> {
        self.plan.as_mut()
    }

    /// Store plan text during the planning phase
    pub fn store_plan_text(&mut self, text: impl Into<String>) {
        if let PlanModeState::Planning { plan_text } = &mut self.state {
            *plan_text = text.into();
        }
    }

    /// Get the current plan text
    pub fn get_plan_text(&self) -> Option<&str> {
        if let PlanModeState::Planning { plan_text } = &self.state {
            Some(plan_text)
        } else {
            None
        }
    }

    /// Approve the plan for execution
    pub fn approve_plan(&mut self) {
        self.approved = true;
        self.state = PlanModeState::Executing;
    }

    /// Check if the plan has been approved
    pub fn is_approved(&self) -> bool {
        self.approved
    }

    /// Clear the stored plan and reset state
    pub fn clear_plan(&mut self) {
        self.plan = None;
        self.state = PlanModeState::Inactive;
        self.approved = false;
    }

    /// Check if a tool is allowed in the current state
    ///
    /// In planning mode, only read-only tools are allowed.
    pub fn is_tool_allowed(&self, _tool_name: &str, is_readonly: bool) -> bool {
        match self.state {
            PlanModeState::Inactive => true,
            PlanModeState::Planning { .. } => is_readonly,
            PlanModeState::Executing => true,
        }
    }
}

/// Thread-safe shared plan mode manager
pub type SharedPlanModeManager = Arc<RwLock<PlanModeManager>>;

/// Create a new shared plan mode manager
pub fn create_shared_plan_manager() -> SharedPlanModeManager {
    Arc::new(RwLock::new(PlanModeManager::new()))
}

/// List of tool names that are read-only and safe to use in plan mode
pub const READONLY_TOOLS: &[&str] = &[
    "file_read",
    "grep_search",
    "glob_find",
    "directory_tree",
    "symbol_search",
    "tool_search",
    "context_bulk_read",
];

/// Check if a tool name is in the read-only list
pub fn is_readonly_tool(tool_name: &str) -> bool {
    READONLY_TOOLS.iter().any(|&name| name == tool_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_step_creation() {
        let step = PlanStep::new(1, "Read the main file");
        assert_eq!(step.id, 1);
        assert_eq!(step.description, "Read the main file");
        assert!(matches!(step.status, StepStatus::Pending));
    }

    #[test]
    fn test_plan_step_with_tool_hint() {
        let step = PlanStep::new(1, "Read file").with_tool_hint("file_read");
        assert_eq!(step.tool_hint, Some("file_read".to_string()));
    }

    #[test]
    fn test_plan_step_status_transitions() {
        let mut step = PlanStep::new(1, "Test step");
        assert!(matches!(step.status, StepStatus::Pending));

        step.mark_in_progress();
        assert!(matches!(step.status, StepStatus::InProgress));

        step.mark_done();
        assert!(matches!(step.status, StepStatus::Done));
    }

    #[test]
    fn test_plan_step_failed() {
        let mut step = PlanStep::new(1, "Test step");
        step.mark_failed("File not found");
        assert!(matches!(step.status, StepStatus::Failed(ref r) if r == "File not found"));
    }

    #[test]
    fn test_plan_creation() {
        let plan = Plan::with_summary("Fix the bug");
        assert_eq!(plan.summary, "Fix the bug");
        assert!(plan.steps.is_empty());
        assert!(plan.created_at.is_some());
    }

    #[test]
    fn test_plan_add_step() {
        let mut plan = Plan::new();
        {
            let step = plan.add_step("Step 1");
            step.tool_hint = Some("file_read".to_string());
        }
        plan.add_step("Step 2");
        assert_eq!(plan.step_count(), 2);
        assert_eq!(plan.steps[0].id, 1);
        assert_eq!(plan.steps[0].tool_hint, Some("file_read".to_string()));
        assert_eq!(plan.steps[1].id, 2);
    }

    #[test]
    fn test_plan_add_file() {
        let mut plan = Plan::new();
        plan.add_file_to_read("src/main.rs");
        plan.add_file_to_read("src/main.rs"); // Duplicate should be ignored
        assert_eq!(plan.files_to_read.len(), 1);
        assert_eq!(plan.files_to_read[0], "src/main.rs");
    }

    #[test]
    fn test_plan_completion() {
        let mut plan = Plan::new();
        plan.add_step("Step 1");
        plan.add_step("Step 2");

        assert!(!plan.is_complete());
        assert_eq!(plan.completed_count(), 0);

        plan.steps[0].mark_done();
        assert!(!plan.is_complete());
        assert_eq!(plan.completed_count(), 1);

        plan.steps[1].mark_done();
        assert!(plan.is_complete());
        assert_eq!(plan.completed_count(), 2);
    }

    #[test]
    fn test_plan_format() {
        let mut plan = Plan::with_summary("Test plan");
        {
            let step = plan.add_step("Step 1");
            step.tool_hint = Some("file_read".to_string());
        }
        plan.add_file_to_read("src/main.rs");
        plan.estimated_tokens = 1000;

        let formatted = plan.format();
        assert!(formatted.contains("Test plan"));
        assert!(formatted.contains("Step 1"));
        assert!(formatted.contains("file_read"));
        assert!(formatted.contains("src/main.rs"));
        assert!(formatted.contains("1000"));
    }

    #[test]
    fn test_plan_mode_state_transitions() {
        let mut manager = PlanModeManager::new();
        assert!(!manager.is_in_plan_mode());
        assert!(!manager.is_planning());

        manager.enter_plan_mode();
        assert!(manager.is_in_plan_mode());
        assert!(manager.is_planning());

        manager.approve_plan();
        assert!(manager.is_in_plan_mode());
        assert!(!manager.is_planning());
        assert!(manager.is_approved());

        manager.exit_plan_mode();
        assert!(!manager.is_in_plan_mode());
        assert!(!manager.is_approved());
    }

    #[test]
    fn test_store_and_get_plan() {
        let mut manager = PlanModeManager::new();
        let plan = Plan::with_summary("Test plan");

        manager.store_plan(plan.clone());
        assert!(manager.get_plan().is_some());
        assert_eq!(manager.get_plan().unwrap().summary, "Test plan");
    }

    #[test]
    fn test_plan_text_storage() {
        let mut manager = PlanModeManager::new();
        manager.enter_plan_mode();

        manager.store_plan_text("1. Do this\n2. Do that");
        assert_eq!(manager.get_plan_text(), Some("1. Do this\n2. Do that"));
    }

    #[test]
    fn test_is_tool_allowed() {
        let mut manager = PlanModeManager::new();

        // Inactive mode - all tools allowed
        assert!(manager.is_tool_allowed("file_edit", false));
        assert!(manager.is_tool_allowed("file_read", true));

        // Planning mode - only read-only tools allowed
        manager.enter_plan_mode();
        assert!(!manager.is_tool_allowed("file_edit", false));
        assert!(manager.is_tool_allowed("file_read", true));

        // Executing mode - all tools allowed
        manager.approve_plan();
        assert!(manager.is_tool_allowed("file_edit", false));
        assert!(manager.is_tool_allowed("file_read", true));
    }

    #[test]
    fn test_readonly_tool_list() {
        assert!(is_readonly_tool("file_read"));
        assert!(is_readonly_tool("grep_search"));
        assert!(is_readonly_tool("glob_find"));
        assert!(is_readonly_tool("directory_tree"));
        assert!(is_readonly_tool("symbol_search"));
        assert!(!is_readonly_tool("file_edit"));
        assert!(!is_readonly_tool("file_write"));
        assert!(!is_readonly_tool("shell_exec"));
    }

    #[test]
    fn test_step_status_display() {
        assert_eq!(StepStatus::Pending.to_string(), "pending");
        assert_eq!(StepStatus::InProgress.to_string(), "in_progress");
        assert_eq!(StepStatus::Done.to_string(), "done");
        assert_eq!(
            StepStatus::Failed("error".to_string()).to_string(),
            "failed: error"
        );
    }

    #[test]
    fn test_plan_next_pending_step() {
        let mut plan = Plan::new();
        plan.add_step("Step 1");
        plan.add_step("Step 2");
        plan.add_step("Step 3");

        assert_eq!(plan.next_pending_step().unwrap().id, 1);

        plan.steps[0].mark_done();
        assert_eq!(plan.next_pending_step().unwrap().id, 2);

        plan.steps[1].mark_in_progress();
        assert_eq!(plan.next_pending_step().unwrap().id, 2); // Still the same
    }

    #[test]
    fn test_clear_plan() {
        let mut manager = PlanModeManager::new();
        manager.enter_plan_mode();
        manager.store_plan(Plan::with_summary("Test"));
        manager.approve_plan();

        manager.clear_plan();
        assert!(!manager.is_in_plan_mode());
        assert!(!manager.is_approved());
        assert!(manager.get_plan().is_none());
    }
}
