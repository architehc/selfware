//! ActionEngine: builds non-mutating evolution action proposals.
//!
//! Actions either change the active context (handled elsewhere) or produce a
//! branch proposal. Branch creation itself is owned by the guarded Git engine.

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
    /// Suggested branch name only; no Git branch is created by `propose`.
    pub branch: Option<String>,
    pub message: String,
}

pub struct ActionEngine {
    // Stateless by design. Guarded mutations live in the dedicated engines.
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
            Action::Extend { component } => {
                format!("evolve/extend-{}-{}", branch_segment(component), ts)
            }
            Action::Connect { from, to } => format!(
                "evolve/connect-{}-{}-{}",
                branch_segment(from),
                branch_segment(to),
                ts
            ),
            Action::BlockEvolution { component } => {
                format!("evolve/block-{}-{}", branch_segment(component), ts)
            }
            Action::Notify { component } => {
                format!("evolve/notify-{}-{}", branch_segment(component), ts)
            }
        }
    }

    /// Build an action proposal.
    ///
    /// This does not mutate Git or source. It returns a proposal that must be
    /// previewed and confirmed through the guarded Git engine.
    pub fn propose(&self, action: &Action) -> Result<ActionResult> {
        match action {
            Action::Extend { component } => Ok(ActionResult {
                branch: Some(Self::branch_name(action)),
                message: format!("Proposed an extension branch for {component}"),
            }),
            _ => Ok(ActionResult {
                branch: None,
                message: "No mutation was performed; this action has no executor".to_string(),
            }),
        }
    }
}

fn branch_segment(value: &str) -> String {
    let mut result = String::new();
    let mut previous_separator = false;
    for character in value.chars() {
        let allowed = character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-');
        if allowed {
            result.push(character.to_ascii_lowercase());
            previous_separator = false;
        } else if !previous_separator && !result.is_empty() {
            result.push('-');
            previous_separator = true;
        }
    }
    let trimmed = result.trim_matches(['.', '-']).to_string();
    if trimmed.is_empty() {
        "component".to_string()
    } else {
        trimmed
    }
}
