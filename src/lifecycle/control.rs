//! In-process control of the running task (formal/DESIGN.md §8, phase 5).
//!
//! A [`TaskControl`] is shared between the agent that runs a task and a UI
//! in the same process (the TUI Tasks pane). The UI *requests* — pause,
//! resume, cancel, edit — and the agent *acts* on them only at a safe point:
//! the top of its loop, between steps, never in the middle of a model call
//! or a tool call. Every action the agent takes goes through the task
//! machine ([`super::TaskMachine`]) and is recorded in the event log, so the
//! log, not this handle, is the history.
//!
//! The agent also publishes a [`LiveTask`] snapshot here (state, step,
//! constraints, measured usage), which is what the pane shows for the task
//! running in this process.
//!
//! Tasks of other processes cannot be controlled: there is no cross-process
//! channel yet, and the CLI says so instead of pretending.

use super::{RecordedUsage, TaskState};
use chrono::{DateTime, Utc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// The agent id of the REPL/`run` agent (DESIGN §3: agent type `Main`).
pub const MAIN_AGENT: &str = "main";

/// The constraints a task runs under that an edit can change.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TaskConstraints {
    /// The iteration cap (`--max-turns` / `[agent] max_iterations`),
    /// including any adaptive extension already granted.
    pub max_turns: usize,
    /// Iterations used so far.
    pub turns_used: usize,
    /// The token budget (`[agent] max_budget_tokens`), `None` = unbounded.
    pub token_budget: Option<usize>,
    /// `[safety] allowed_paths`. Shown, not editable mid-task: the safety
    /// checker reads it once when the agent is built.
    pub allowed_paths: Vec<String>,
}

/// What the running task looks like right now, as published by its agent.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveTask {
    /// Task id (the checkpoint `task_id`, as in the event log).
    pub id: String,
    /// The owning agent's id.
    pub agent: String,
    /// The task description as the agent currently holds it (edits included).
    pub description: String,
    /// The description the task was started with, when it was edited.
    pub original_description: Option<String>,
    /// The classified task type, when known.
    pub task_type: Option<String>,
    /// Current lifecycle state.
    pub state: TaskState,
    /// When this process observed the task enter `state`.
    pub state_since: DateTime<Utc>,
    /// The loop's step counter.
    pub step: usize,
    /// Current constraints.
    pub constraints: TaskConstraints,
    /// Measured usage so far (`None` before the first model call reports).
    pub usage: Option<RecordedUsage>,
    /// The task this one was forked from.
    pub parent: Option<String>,
    /// A pause was requested and the agent has not reached its safe point.
    pub pause_pending: bool,
    /// Measured time this run segment spent paused (closed pauses; the
    /// current pause shows as time in state). Not counted against the
    /// wall-clock budget.
    pub paused: std::time::Duration,
}

/// A task description the user edited mid-run, with the one the task was
/// started with. Reported wherever the task's description is (run summary,
/// structured result, journal, `task show`, the Tasks pane) so the edited
/// text is never shown as if it had been the task from the start.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EditedDescription {
    /// The description the task runs under now.
    pub description: String,
    /// The description the task was started with.
    pub original: String,
}

/// A user edit of a live task: the full new description and constraints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskEdit {
    /// New task description.
    pub description: String,
    /// New iteration cap.
    pub max_turns: usize,
    /// New token budget (`None` = unbounded).
    pub token_budget: Option<usize>,
}

impl TaskEdit {
    /// The edit that changes nothing: the task as it is now.
    pub fn from_live(live: &LiveTask) -> Self {
        Self {
            description: live.description.clone(),
            max_turns: live.constraints.max_turns,
            token_budget: live.constraints.token_budget,
        }
    }

    /// Check the edit against the task's current progress. An edit that
    /// would end the task on the spot (a cap at or below what is already
    /// used) is refused with the reason, not applied.
    pub fn validate(&self, current: &LiveTask) -> Result<(), String> {
        if self.description.trim().is_empty() {
            return Err("the task description cannot be empty".into());
        }
        if self.max_turns <= current.constraints.turns_used {
            return Err(format!(
                "max turns must be above the {} already used",
                current.constraints.turns_used
            ));
        }
        if let Some(budget) = self.token_budget {
            let used = current.usage.map(|u| u.total_tokens).unwrap_or(0);
            if budget <= used {
                return Err(format!(
                    "token budget must be above the {used} tokens already used"
                ));
            }
        }
        Ok(())
    }

    /// What changed relative to `before`, as one line per change, or `None`
    /// when nothing changed. This is the text the agent receives ("Task
    /// updated: …") and the edit's recorded cause.
    pub fn changes(&self, before: &LiveTask) -> Option<String> {
        let mut parts = Vec::new();
        if self.description.trim() != before.description.trim() {
            parts.push(format!("description is now: {}", self.description.trim()));
        }
        if self.max_turns != before.constraints.max_turns {
            parts.push(format!(
                "max turns {} → {}",
                before.constraints.max_turns, self.max_turns
            ));
        }
        if self.token_budget != before.constraints.token_budget {
            let fmt = |b: Option<usize>| b.map_or("unbounded".to_string(), |b| b.to_string());
            parts.push(format!(
                "token budget {} → {}",
                fmt(before.constraints.token_budget),
                fmt(self.token_budget)
            ));
        }
        (!parts.is_empty()).then(|| parts.join("; "))
    }
}

/// Why a request was not accepted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ControlError {
    /// No task is running in this process.
    #[error("no task is running in this process")]
    NoLiveTask,
    /// The named task is not the one running in this process.
    #[error(
        "task {0} is not running in this process (cross-process control is not supported yet)"
    )]
    NotInThisProcess(String),
    /// The task already finished.
    #[error("task {0} already finished; edit it to fork a new task")]
    Finished(String),
    /// The request needs a paused task.
    #[error("task {0} is not paused")]
    NotPaused(String),
    /// The edit was refused.
    #[error("edit refused: {0}")]
    InvalidEdit(String),
}

#[derive(Debug, Default)]
struct Inner {
    live: Option<LiveTask>,
    pause: bool,
    resume: bool,
    cancel: bool,
    edit: Option<TaskEdit>,
    /// (parent, description) of forks queued to run next.
    forks: Vec<(String, String)>,
}

/// Shared handle; cheap to clone.
#[derive(Debug, Clone)]
pub struct TaskControl {
    inner: Arc<Mutex<Inner>>,
    cancel_token: Arc<AtomicBool>,
}

impl TaskControl {
    /// A control whose cancel requests latch `cancel_token` (the agent's
    /// cancellation flag, checked between steps and inside model calls).
    pub fn new(cancel_token: Arc<AtomicBool>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
            cancel_token,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ---- UI side -------------------------------------------------------

    /// The task running in this process, or the last one it ran.
    pub fn snapshot(&self) -> Option<LiveTask> {
        let inner = self.lock();
        inner.live.clone().map(|mut l| {
            l.pause_pending = inner.pause && l.state != TaskState::Paused;
            l
        })
    }

    fn live_for(&self, inner: &Inner, id: &str) -> Result<LiveTask, ControlError> {
        let live = inner.live.as_ref().ok_or(ControlError::NoLiveTask)?;
        if live.id != id {
            return Err(ControlError::NotInThisProcess(id.to_string()));
        }
        if live.state.is_terminal() {
            return Err(ControlError::Finished(id.to_string()));
        }
        Ok(live.clone())
    }

    /// Ask task `id` to pause at its next safe point (between steps).
    pub fn request_pause(&self, id: &str) -> Result<(), ControlError> {
        let mut inner = self.lock();
        self.live_for(&inner, id)?;
        inner.pause = true;
        inner.resume = false;
        Ok(())
    }

    /// Ask a paused task (or one with a pause pending) to continue.
    pub fn request_resume(&self, id: &str) -> Result<(), ControlError> {
        let mut inner = self.lock();
        let live = self.live_for(&inner, id)?;
        if live.state != TaskState::Paused && !inner.pause {
            return Err(ControlError::NotPaused(id.to_string()));
        }
        if live.state == TaskState::Paused {
            inner.resume = true;
        }
        // A pause that has not been reached yet is simply withdrawn.
        inner.pause = false;
        Ok(())
    }

    /// Cancel task `id`: the agent stops at its next cancellation check (a
    /// model call is aborted; a tool call finishes first) and the task ends
    /// `cancelled`.
    pub fn request_cancel(&self, id: &str) -> Result<(), ControlError> {
        let mut inner = self.lock();
        self.live_for(&inner, id)?;
        inner.cancel = true;
        self.cancel_token.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Edit task `id`: pause at the next safe point (if not paused already),
    /// apply the edit, resume. The edit is validated against the task's
    /// current progress first.
    pub fn submit_edit(&self, id: &str, edit: TaskEdit) -> Result<(), ControlError> {
        let mut inner = self.lock();
        let live = self.live_for(&inner, id)?;
        edit.validate(&live).map_err(ControlError::InvalidEdit)?;
        inner.edit = Some(edit);
        inner.pause = true;
        Ok(())
    }

    /// Queue a fork of finished task `parent`: when this process next runs a
    /// task whose text is exactly `description`, it is recorded with
    /// `parent` as its origin.
    pub fn queue_fork(&self, parent: &str, description: &str) {
        self.lock()
            .forks
            .push((parent.to_string(), description.to_string()));
    }

    // ---- agent side ----------------------------------------------------

    /// Publish (or replace) the running task's snapshot. The time in state
    /// restarts only when the state changes.
    pub fn publish(&self, mut live: LiveTask) {
        let mut inner = self.lock();
        if let Some(prev) = inner.live.as_ref() {
            if prev.id == live.id && prev.state == live.state {
                live.state_since = prev.state_since;
            }
        }
        inner.live = Some(live);
    }

    /// A new task starts: requests addressed to the previous one are
    /// dropped.
    pub fn begin_task(&self) {
        let mut inner = self.lock();
        inner.pause = false;
        inner.resume = false;
        inner.cancel = false;
        inner.edit = None;
    }

    /// Whether a pause (or an edit, which pauses) is requested.
    pub fn pause_requested(&self) -> bool {
        self.lock().pause
    }

    /// Take the pending edit, if any.
    pub fn take_edit(&self) -> Option<TaskEdit> {
        self.lock().edit.take()
    }

    /// Take a resume request (also clears the pause request).
    pub fn take_resume(&self) -> bool {
        let mut inner = self.lock();
        let r = inner.resume || !inner.pause;
        if r {
            inner.resume = false;
            inner.pause = false;
        }
        r
    }

    /// Mark the pause as reached and handled by the agent.
    pub fn clear_pause(&self) {
        let mut inner = self.lock();
        inner.pause = false;
        inner.resume = false;
    }

    /// Whether the user cancelled the current task through this control.
    pub fn cancel_requested(&self) -> bool {
        self.lock().cancel
    }

    /// The fork parent queued for a task with exactly this text, if any
    /// (consumed).
    pub fn take_fork_for(&self, description: &str) -> Option<String> {
        let mut inner = self.lock();
        let pos = inner.forks.iter().position(|(_, d)| d == description)?;
        Some(inner.forks.remove(pos).0)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/lifecycle/control_test.rs"]
mod tests;
