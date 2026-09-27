//! The agent lifecycle (formal/DESIGN.md §4.2). An agent works on at most
//! one task at a time.
//!
//! ```text
//!  Idle ─assign─▶ Working{task} ─block─▶ Blocked{task} ─unblock─▶ Working{task}
//!    ▲                 │   (Blocked ─done─▶ Idle too)
//!    └──── done ───────┘          any live ─crash─▶ Crashed ─restart─▶ Idle
//!                                 any live ─stop──▶ Stopped (terminal)
//! ```

use super::{Effect, Entity, InvalidTransition, Label, Machine};
use serde::{Deserialize, Serialize};

/// Where an agent is in its life. The task ids are the owning task's id
/// (the checkpoint `task_id` for the main agent).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum AgentState {
    /// No task assigned.
    Idle,
    /// Working on `task`.
    Working {
        /// The task being worked on.
        task: String,
    },
    /// Working on `task` but blocked (approval, user, resource).
    Blocked {
        /// The task being worked on.
        task: String,
        /// Why it is blocked.
        reason: String,
    },
    /// The agent died unexpectedly; `restart` brings it back to idle.
    Crashed,
    /// Stopped for good. Terminal.
    Stopped,
}

/// What can happen to an agent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "event")]
pub enum AgentEvent {
    /// idle → working on `task`.
    Assign {
        /// The assigned task.
        task: String,
    },
    /// working → blocked.
    Block {
        /// Why.
        reason: String,
    },
    /// blocked → working.
    Unblock,
    /// working | blocked → idle (the task ended).
    Done,
    /// any live state except crashed → crashed.
    Crash,
    /// crashed → idle.
    Restart,
    /// any live state → stopped.
    Stop,
}

impl Label for AgentState {
    fn label(&self) -> &'static str {
        match self {
            AgentState::Idle => "idle",
            AgentState::Working { .. } => "working",
            AgentState::Blocked { .. } => "blocked",
            AgentState::Crashed => "crashed",
            AgentState::Stopped => "stopped",
        }
    }
}

impl Label for AgentEvent {
    fn label(&self) -> &'static str {
        match self {
            AgentEvent::Assign { .. } => "assign",
            AgentEvent::Block { .. } => "block",
            AgentEvent::Unblock => "unblock",
            AgentEvent::Done => "done",
            AgentEvent::Crash => "crash",
            AgentEvent::Restart => "restart",
            AgentEvent::Stop => "stop",
        }
    }
}

impl AgentState {
    /// The task this agent currently holds, if any.
    pub fn task(&self) -> Option<&str> {
        match self {
            AgentState::Working { task } | AgentState::Blocked { task, .. } => Some(task),
            _ => None,
        }
    }
}

/// The agent machine.
#[derive(Debug, Clone, Copy, Default)]
pub struct AgentMachine;

impl Machine for AgentMachine {
    type State = AgentState;
    type Event = AgentEvent;
    const ENTITY: Entity = Entity::Agent;

    fn next(state: &AgentState, event: &AgentEvent) -> Result<AgentState, InvalidTransition> {
        use AgentEvent as E;
        use AgentState as S;
        let next = match (state, event) {
            (S::Idle, E::Assign { task }) => Some(S::Working { task: task.clone() }),
            (S::Working { task }, E::Block { reason }) => Some(S::Blocked {
                task: task.clone(),
                reason: reason.clone(),
            }),
            (S::Blocked { task, .. }, E::Unblock) => Some(S::Working { task: task.clone() }),
            (S::Working { .. } | S::Blocked { .. }, E::Done) => Some(S::Idle),
            (S::Idle | S::Working { .. } | S::Blocked { .. }, E::Crash) => Some(S::Crashed),
            (S::Crashed, E::Restart) => Some(S::Idle),
            (S::Stopped, E::Stop) => None,
            (_, E::Stop) => Some(S::Stopped),
            _ => None,
        };
        next.ok_or_else(|| Self::refuse(state, event))
    }

    fn on_enter(_state: &AgentState) -> Vec<Effect> {
        Vec::new()
    }

    fn is_terminal(state: &AgentState) -> bool {
        matches!(state, AgentState::Stopped)
    }

    /// `stopped` is sticky, and every live state can be stopped.
    fn check_step(from: &AgentState, event: &AgentEvent, to: &AgentState) -> Result<(), String> {
        if matches!(from, AgentState::Stopped) {
            return Err(format!(
                "agent left terminal `stopped` via `{}`",
                event.label()
            ));
        }
        if !Self::is_terminal(to) && Self::next(to, &AgentEvent::Stop) != Ok(AgentState::Stopped) {
            return Err(format!("agent state `{}` cannot be stopped", to.label()));
        }
        Ok(())
    }
}
