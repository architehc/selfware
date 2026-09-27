//! The main agent run on the typed task lifecycle (formal/DESIGN.md §9
//! phase 1).
//!
//! Behaviour of the run is unchanged: this only mirrors it into
//! [`crate::lifecycle::TaskMachine`] and the event log.
//!
//! | run moment                                   | task event                 |
//! |----------------------------------------------|----------------------------|
//! | `run_task` creates the checkpoint            | (new) → queued, `start`    |
//! | loop state enters `Executing`                | `planned`                  |
//! | `continue_execution` of an interrupted task  | `resume`, `start`          |
//! | run ends, `RunEnd::Completed`                | `succeed`                  |
//! | run ends, `RunEnd::Failed` (deadline/budget clock) | `timeout`            |
//! | run ends, `RunEnd::Failed` (anything else)   | `fail`                     |
//! | run ends, `RunEnd::Interrupted`/`Terminated` | `interrupt`                |
//!
//! The terminal event is derived from the same `RunEnd::classify` the CLI
//! reports as `outcome:`, so the log and the run summary cannot disagree.
//! The task id is the checkpoint `task_id`.
//!
//! Not mapped yet (no main-loop site owns them today): `need_input` /
//! `input_arrived` (approval prompts), `verify` / `verified` / `reject`
//! (the completion gate), `pause`, `cancel`.

use super::Agent;
use crate::errors::{AgentError, RunEnd, SelfwareError};
use crate::lifecycle::{Effect, Entity, TaskEvent, TaskMachine, TaskState, Tracked};
use tracing::warn;

/// The terminal task event for a finished run, and its cause.
pub(crate) fn terminal_event_for_run(
    result: &anyhow::Result<()>,
    shutdown: Option<crate::ShutdownReason>,
) -> (TaskEvent, String) {
    let first_line = |e: &anyhow::Error| e.to_string().lines().next().unwrap_or("").to_string();
    match RunEnd::classify(result, shutdown) {
        RunEnd::Completed => (TaskEvent::Succeed, "outcome completed".to_string()),
        RunEnd::Interrupted => (
            TaskEvent::Interrupt,
            match result {
                Err(e) => format!("outcome interrupted: {}", first_line(e)),
                Ok(()) => "outcome interrupted: user interrupt".to_string(),
            },
        ),
        RunEnd::Terminated => (
            TaskEvent::Interrupt,
            "outcome terminated (SIGTERM)".to_string(),
        ),
        RunEnd::Failed => {
            let reason = match result {
                Err(e) => first_line(e),
                Ok(()) => "run timeout".to_string(),
            };
            if is_deadline_stop(result, shutdown) {
                (
                    TaskEvent::Timeout,
                    format!("outcome failed (deadline): {reason}"),
                )
            } else {
                (TaskEvent::Fail, format!("outcome failed: {reason}"))
            }
        }
    }
}

/// A failure caused by a clock running out (the run's own timeout, the wall
/// budget, or the per-call time cap), as opposed to any other failure.
/// Typed causes only.
fn is_deadline_stop(result: &anyhow::Result<()>, shutdown: Option<crate::ShutdownReason>) -> bool {
    if shutdown == Some(crate::ShutdownReason::Timeout) {
        return true;
    }
    let Err(e) = result else { return false };
    e.chain().any(|c| {
        c.is::<crate::api::client::WallClockBudgetExceeded>()
            || c.is::<crate::api::client::CallTimeBudgetExceeded>()
            || matches!(
                c.downcast_ref::<AgentError>(),
                Some(AgentError::CancelledWithReason(_))
            )
            || matches!(
                c.downcast_ref::<SelfwareError>(),
                Some(SelfwareError::Agent(AgentError::CancelledWithReason(_)))
            )
    })
}

impl Agent {
    /// Apply `event` to the current task, logging (never failing on) a
    /// refusal.
    fn lifecycle_apply(&mut self, event: TaskEvent, cause: &str) -> Vec<Effect> {
        let Some(task) = self.task_lifecycle.as_mut() else {
            return Vec::new();
        };
        match task.apply(event, cause) {
            Ok(effects) => effects,
            Err(e) => {
                warn!("task lifecycle ({}): {e}", task.id());
                Vec::new()
            }
        }
    }

    /// A tracker still live when a new one takes over belongs to a run whose
    /// future was dropped before it reported an outcome. Record that as an
    /// interruption, in those words, instead of leaving it live forever.
    fn lifecycle_supersede(&mut self, next_id: &str) {
        if let Some(prev) = self.task_lifecycle.as_ref() {
            if !prev.is_terminal() && prev.id() != next_id {
                self.lifecycle_apply(
                    TaskEvent::Interrupt,
                    "run ended without reporting an outcome (superseded by a new task)",
                );
            }
        }
    }

    /// `run_task`: the new task (its checkpoint `task_id`) is created queued
    /// and started.
    pub(super) fn lifecycle_begin_task(&mut self, task_id: &str, task: &str) {
        self.lifecycle_supersede(task_id);
        let tracked: Tracked<TaskMachine> =
            Tracked::new(task_id, TaskState::Queued, self.event_log.clone())
                .with_task_type(Self::infer_task_type(task));
        tracked.record_created("task created");
        self.task_lifecycle = Some(tracked);
        self.lifecycle_apply(TaskEvent::Start, "run started");
    }

    /// `continue_execution`: an interrupted task resumes as a new segment
    /// (`resume`, then `start`). A task whose last recorded state is not
    /// `interrupted` (a failed task resumed with `--continue`, a crashed
    /// process, no record at all) opens a new segment with `from: null` and
    /// says why — the model never lets a terminal state other than
    /// `interrupted` be left. Inside an auto-continue chain the task is
    /// still live and nothing is recorded.
    pub(super) fn lifecycle_begin_resume(&mut self) {
        let Some((task_id, task)) = self
            .current_checkpoint
            .as_ref()
            .map(|c| (c.task_id.clone(), c.task_description.clone()))
        else {
            return;
        };
        let in_memory = self
            .task_lifecycle
            .as_ref()
            .filter(|t| t.id() == task_id)
            .map(|t| *t.state());
        if in_memory.is_some_and(|s| !s.is_terminal()) {
            return; // auto-continue chain: same run, same live task
        }
        self.lifecycle_supersede(&task_id);
        let last = in_memory.or_else(|| {
            self.event_log
                .last_state(Entity::Task, &task_id)
                .and_then(|s| TaskState::from_label(&s))
        });
        let task_type = Self::infer_task_type(&task);
        let log = self.event_log.clone();
        if last == Some(TaskState::Interrupted) {
            self.task_lifecycle =
                Some(Tracked::new(task_id, TaskState::Interrupted, log).with_task_type(task_type));
            self.lifecycle_apply(TaskEvent::Resume, "resumed");
        } else {
            let tracked: Tracked<TaskMachine> =
                Tracked::new(task_id, TaskState::Queued, log).with_task_type(task_type);
            tracked.record_created(&match last {
                Some(s) => format!("new segment: resumed after last recorded state `{s}`"),
                None => "new segment: resumed with no earlier record".to_string(),
            });
            self.task_lifecycle = Some(tracked);
        }
        self.lifecycle_apply(TaskEvent::Start, "run resumed");
    }

    /// Mirror the loop's `AgentState` into the task: entering `Executing`
    /// from planning is `planned`. Idempotent; cheap enough for every
    /// iteration.
    pub(super) fn lifecycle_note_loop_state(&mut self) {
        let planning = self
            .task_lifecycle
            .as_ref()
            .is_some_and(|t| *t.state() == TaskState::Planning);
        if planning && self.loop_control.current_state_label() == "executing" {
            self.lifecycle_apply(TaskEvent::Planned, "executing");
        }
    }

    /// The run ended with `result`: record the terminal transition (once).
    /// Returns the effects of entering the terminal state, for the resource
    /// registry to carry out.
    pub(super) fn lifecycle_finish(&mut self, result: &anyhow::Result<()>) -> Vec<Effect> {
        let Some(state) = self.task_lifecycle.as_ref().map(|t| *t.state()) else {
            return Vec::new();
        };
        if state.is_terminal() {
            return Vec::new();
        }
        let (event, cause) = terminal_event_for_run(result, crate::shutdown_reason());
        if event == TaskEvent::Succeed && state == TaskState::Planning {
            // The planning turn itself produced the accepted answer.
            self.lifecycle_apply(TaskEvent::Planned, "answered in the planning turn");
        }
        self.lifecycle_apply(event, &cause)
    }

    /// The current task's lifecycle state, if a task is tracked.
    pub fn task_lifecycle_state(&self) -> Option<TaskState> {
        self.task_lifecycle.as_ref().map(|t| *t.state())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/lifecycle_wiring/lifecycle_wiring_test.rs"]
mod tests;
