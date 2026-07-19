//! ActionEngine: executes evolution actions.
//!
//! Actions either change the active context (handled elsewhere) or are
//! represented as git branches (`evolve/<kind>-<target>-<timestamp>`).

use anyhow::Result;
use chrono::Utc;

#[derive(Debug, Clone)]
pub enum Action {
    Extend { component: String },
    Connect { from: String, to: String },
    BlockEvolution { component: String },
    Notify { component: String },
}

pub struct ActionResult {
    /// Suggested branch name only — no git branch is created (see `execute`).
    pub branch: Option<String>,
    pub message: String,
}

pub struct ActionEngine {
    // Git operations will use git2 or shell
}

impl Default for ActionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ActionEngine {
    pub fn new() -> Self {
        Self {}
    }

    pub fn branch_name(action: &Action) -> String {
        let ts = Utc::now().format("%Y%m%d-%H%M%S");
        match action {
            Action::Extend { component } => format!("evolve/extend-{}-{}", component, ts),
            Action::Connect { from, to } => format!("evolve/connect-{}-{}-{}", from, to, ts),
            Action::BlockEvolution { component } => format!("evolve/block-{}-{}", component, ts),
            Action::Notify { component } => format!("evolve/notify-{}-{}", component, ts),
        }
    }

    /// Execute an action.
    ///
    /// STUB: this does **not** create a real git branch yet. `ActionResult::branch`
    /// is only a *suggested* branch name for the action; wiring it to actual
    /// branch creation (git2 or shell) is planned follow-up work.
    pub fn execute(&self, action: &Action) -> Result<ActionResult> {
        match action {
            Action::Extend { component } => Ok(ActionResult {
                branch: Some(Self::branch_name(action)),
                message: format!("Extended {}", component),
            }),
            _ => Ok(ActionResult {
                branch: None,
                message: "Action not implemented yet".to_string(),
            }),
        }
    }
}
