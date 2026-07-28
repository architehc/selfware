//! Plan Mode implementation for Selfware
//!
//! Plan Mode provides an explicit planning phase where the agent can only use
//! read-only tools (file_read, grep_search, etc.) to analyze the codebase before
//! making any modifications. This prevents premature execution and gives users
//! control over the plan before execution begins.

use serde::{Deserialize, Serialize};

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
    pub fn add_step(&mut self, description: impl Into<String>) -> Option<&mut PlanStep> {
        let id = self.steps.len() + 1;
        let step = PlanStep::new(id, description);
        self.steps.push(step);
        self.steps.last_mut()
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

    /// Get the next pending step (first step that is not done)
    pub fn next_pending_step(&self) -> Option<&PlanStep> {
        self.steps
            .iter()
            .find(|s| !matches!(s.status, StepStatus::Done))
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
            output.push_str(&format!("Estimated tokens: ~{}\n", self.estimated_tokens));
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
    READONLY_TOOLS.contains(&tool_name)
}

/// Parse a structured [`Plan`] from LLM output.
///
/// Accepts both raw JSON and JSON embedded inside markdown code fences. The
/// expected shape is:
///
/// ```json
/// {
///   "summary": "...",
///   "estimated_tokens": 1234,
///   "files_to_read": ["src/foo.rs"],
///   "steps": [
///     { "description": "...", "tool_hint": "file_read", "file_path": "...", "context": "..." }
///   ]
/// }
/// ```
pub fn parse_plan_from_llm(content: &str) -> Option<Plan> {
    let trimmed = content.trim();

    // Extract JSON from a markdown fence if present.
    let json_str = if trimmed.starts_with("```") {
        let after_open = trimmed.find('\n').map(|i| &trimmed[i + 1..]).unwrap_or("");
        let end = after_open.rfind("```").unwrap_or(after_open.len());
        after_open[..end].trim()
    } else {
        trimmed
    };

    // Locate the outermost JSON object.
    let start = json_str.find('{')?;
    let end = json_str.rfind('}')?;
    let object_str = &json_str[start..=end];

    let value: serde_json::Value = serde_json::from_str(object_str).ok()?;

    let mut plan = Plan::with_summary(
        value
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    );

    if let Some(et) = value.get("estimated_tokens").and_then(|v| v.as_u64()) {
        plan.estimated_tokens = et as usize;
    }

    if let Some(files) = value.get("files_to_read").and_then(|v| v.as_array()) {
        for file in files {
            if let Some(s) = file.as_str() {
                plan.add_file_to_read(s.to_string());
            }
        }
    }

    if let Some(steps) = value.get("steps").and_then(|v| v.as_array()) {
        for (idx, step_val) in steps.iter().enumerate() {
            let description = step_val
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if description.is_empty() {
                continue;
            }

            let mut step = PlanStep::new(idx + 1, description);
            if let Some(hint) = step_val.get("tool_hint").and_then(|v| v.as_str()) {
                step.tool_hint = Some(hint.to_string());
            }
            if let Some(path) = step_val.get("file_path").and_then(|v| v.as_str()) {
                step.file_path = Some(path.to_string());
            }
            if let Some(ctx) = step_val.get("context").and_then(|v| v.as_str()) {
                step.context = Some(ctx.to_string());
            }
            plan.steps.push(step);
        }
    }

    Some(plan)
}

#[cfg(test)]
#[path = "../../tests/unit/agent/plan_mode/plan_mode_test.rs"]
mod tests;
