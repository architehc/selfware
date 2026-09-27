//! Typed lifecycles for tasks, agents and resources (formal/DESIGN.md §4).
//!
//! Three small state machines share one shape, the [`Machine`] trait:
//!
//! - [`TaskMachine`]: queued → planning → executing ⇄ waiting/verifying →
//!   completed | failed | interrupted | cancelled. Its transition table is
//!   the one proved in `formal/TaskFsm.lean` and is checked against the
//!   exported `formal/task_table.json` by a conformance test.
//! - [`AgentMachine`]: idle → working ⇄ blocked, crash/restart, stop.
//! - [`ResourceMachine`]: requested → starting → live → draining →
//!   released | leaked, plus orphan reconciliation. Its table is the one
//!   proved in `formal/ResourceFsm.lean`, checked against the exported
//!   `formal/resource_table.json` the same way.
//!
//! A transition is computed by [`Machine::next`]; an event the current state
//! refuses is a typed [`InvalidTransition`], never a panic. [`Tracked`] holds
//! one entity's current state, applies events, runs the proved invariants as
//! runtime oracles (`debug_assert!` in debug/test builds, a warning in
//! release builds) and appends one [`TransitionRecord`] per transition to the
//! append-only [`EventLog`] (`~/.selfware/state/events.jsonl`).
//!
//! Entering a state can request [`Effect`]s (e.g. a task entering a terminal
//! state asks for [`Effect::TeardownOwned`]). This module only reports them;
//! the resource registry executes them.

mod agent;
pub mod control;
mod log;
pub mod projection;
mod resource;
mod task;
mod tracked;

pub use agent::{AgentEvent, AgentMachine, AgentState};
pub use log::{
    EventLog, RecordedUsage, TransitionRecord, EVENT_LOG_ENV, MAX_CAUSE_CHARS, MAX_LOG_BYTES,
};
pub use resource::{
    table as resource_table, ResourceEvent, ResourceKind, ResourceMachine, ResourceState,
};
pub use task::{table as task_table, TaskEvent, TaskMachine, TaskState};
pub use tracked::Tracked;

use serde::{Deserialize, Serialize};

/// Which kind of entity a transition belongs to (the `entity` field of a
/// [`TransitionRecord`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Entity {
    /// A unit of work (one `run_task` / resumed segment).
    Task,
    /// An agent that works on at most one task at a time.
    Agent,
    /// Something a task started that can outlive a model call.
    Resource,
}

impl Entity {
    /// The label used in the event log.
    pub fn as_str(self) -> &'static str {
        match self {
            Entity::Task => "task",
            Entity::Agent => "agent",
            Entity::Resource => "resource",
        }
    }
}

impl std::fmt::Display for Entity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A stable snake_case name for a state or an event, as written to the event
/// log and to `formal/task_table.json`.
pub trait Label {
    /// The name.
    fn label(&self) -> &'static str;
}

/// An event the current state refuses. Returned by [`Machine::next`] and
/// [`Tracked::apply`]; the state is left unchanged.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid {entity} transition: event `{event}` is refused in state `{from}`")]
pub struct InvalidTransition {
    /// The machine that refused the event.
    pub entity: Entity,
    /// The state the event was applied to.
    pub from: &'static str,
    /// The refused event.
    pub event: &'static str,
}

/// A side effect requested by entering a state. Reported by
/// [`Machine::on_enter`]; carried out by whoever owns the entity (the agent
/// drains the task's resources through the resource registry on
/// `TeardownOwned`; the registry surfaces `LeakAlarm`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// The task reached a terminal state: every resource it owns must be
    /// drained (reverse creation order, within the teardown deadline).
    TeardownOwned,
    /// A resource reached `leaked`: show it loudly and let the reaper retry.
    LeakAlarm,
}

/// One typed lifecycle (formal/DESIGN.md §4.4).
pub trait Machine {
    /// The machine's state.
    type State: Clone + PartialEq + std::fmt::Debug + Label;
    /// The events it reacts to.
    type Event: Clone + PartialEq + std::fmt::Debug + Label;
    /// The entity kind written to the event log.
    const ENTITY: Entity;

    /// The state `event` leads to from `state`, or the typed refusal.
    fn next(state: &Self::State, event: &Self::Event) -> Result<Self::State, InvalidTransition>;

    /// The effects entering `state` requests.
    fn on_enter(state: &Self::State) -> Vec<Effect>;

    /// Whether `state` is terminal.
    fn is_terminal(state: &Self::State) -> bool;

    /// Check the machine's proved invariants on one accepted transition
    /// `from --event--> to`. Returns a description of the violated invariant.
    /// Used as a runtime oracle by [`Tracked::apply`].
    fn check_step(from: &Self::State, event: &Self::Event, to: &Self::State) -> Result<(), String>;

    /// Build the refusal for `event` in `state`.
    fn refuse(state: &Self::State, event: &Self::Event) -> InvalidTransition {
        InvalidTransition {
            entity: Self::ENTITY,
            from: state.label(),
            event: event.label(),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/lifecycle/mod_test.rs"]
mod tests;
