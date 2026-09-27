//! The task lifecycle. Mirrors `formal/TaskFsm.lean` (`TaskState`,
//! `TaskEvent`, `step`) one-to-one; `formal/task_table.json` is that model's
//! exported table and a conformance test compares every (state, event) pair.

use super::{Effect, Entity, InvalidTransition, Label, Machine};
use serde::{Deserialize, Serialize};

/// Where a task is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    /// Created, not started.
    Queued,
    /// The first (planning) turn is running.
    Planning,
    /// The execution loop is running.
    Executing,
    /// Blocked on an approval, the user, or a resource.
    Waiting,
    /// A verification step decides whether the result stands.
    Verifying,
    /// Paused by the user.
    Paused,
    /// Finished successfully (outcome `completed`). Terminal.
    Completed,
    /// Finished unsuccessfully (outcome `failed`). Terminal.
    Failed,
    /// Stopped from outside (Ctrl-C, SIGTERM). Terminal, but resumable as a
    /// new segment of the same task.
    Interrupted,
    /// Abandoned on purpose. Terminal.
    Cancelled,
}

/// What can happen to a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskEvent {
    /// queued → planning.
    Start,
    /// planning → executing.
    Planned,
    /// executing → waiting.
    NeedInput,
    /// waiting → executing.
    InputArrived,
    /// executing → verifying.
    Verify,
    /// verifying → completed.
    Verified,
    /// verifying → executing.
    Reject,
    /// any live state except paused → paused.
    Pause,
    /// paused → executing; interrupted → queued (new segment).
    Resume,
    /// executing → completed.
    Succeed,
    /// any live state → failed.
    Fail,
    /// any live state → interrupted.
    Interrupt,
    /// any live state → cancelled.
    Cancel,
    /// any live state → failed (the state's deadline passed).
    Timeout,
    /// paused → paused: the user changed the task's description or
    /// constraints while it was paused (only a paused task is edited; a
    /// finished task is forked into a new task instead).
    Edit,
}

impl TaskState {
    /// Every state, in the Lean model's `allStates` order.
    pub const ALL: [TaskState; 10] = [
        TaskState::Queued,
        TaskState::Planning,
        TaskState::Executing,
        TaskState::Waiting,
        TaskState::Verifying,
        TaskState::Paused,
        TaskState::Completed,
        TaskState::Failed,
        TaskState::Interrupted,
        TaskState::Cancelled,
    ];

    /// Terminal states: completed, failed, interrupted, cancelled.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TaskState::Completed
                | TaskState::Failed
                | TaskState::Interrupted
                | TaskState::Cancelled
        )
    }

    /// Parse a log / table name.
    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|st| st.label() == s)
    }
}

impl TaskEvent {
    /// Every event, in the Lean model's `allEvents` order.
    pub const ALL: [TaskEvent; 15] = [
        TaskEvent::Start,
        TaskEvent::Planned,
        TaskEvent::NeedInput,
        TaskEvent::InputArrived,
        TaskEvent::Verify,
        TaskEvent::Verified,
        TaskEvent::Reject,
        TaskEvent::Pause,
        TaskEvent::Resume,
        TaskEvent::Succeed,
        TaskEvent::Fail,
        TaskEvent::Interrupt,
        TaskEvent::Cancel,
        TaskEvent::Timeout,
        TaskEvent::Edit,
    ];

    /// Parse a log / table name.
    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.label() == s)
    }
}

impl Label for TaskState {
    fn label(&self) -> &'static str {
        match self {
            TaskState::Queued => "queued",
            TaskState::Planning => "planning",
            TaskState::Executing => "executing",
            TaskState::Waiting => "waiting",
            TaskState::Verifying => "verifying",
            TaskState::Paused => "paused",
            TaskState::Completed => "completed",
            TaskState::Failed => "failed",
            TaskState::Interrupted => "interrupted",
            TaskState::Cancelled => "cancelled",
        }
    }
}

impl Label for TaskEvent {
    fn label(&self) -> &'static str {
        match self {
            TaskEvent::Start => "start",
            TaskEvent::Planned => "planned",
            TaskEvent::NeedInput => "need_input",
            TaskEvent::InputArrived => "input_arrived",
            TaskEvent::Verify => "verify",
            TaskEvent::Verified => "verified",
            TaskEvent::Reject => "reject",
            TaskEvent::Pause => "pause",
            TaskEvent::Resume => "resume",
            TaskEvent::Succeed => "succeed",
            TaskEvent::Fail => "fail",
            TaskEvent::Interrupt => "interrupt",
            TaskEvent::Cancel => "cancel",
            TaskEvent::Timeout => "timeout",
            TaskEvent::Edit => "edit",
        }
    }
}

impl std::fmt::Display for TaskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// The task machine.
#[derive(Debug, Clone, Copy, Default)]
pub struct TaskMachine;

/// The Lean `step` function, arm for arm and in the same order (Lean matches
/// the first applicable arm). `None` = refused.
fn step(s: TaskState, e: TaskEvent) -> Option<TaskState> {
    use TaskEvent as E;
    use TaskState as S;
    match (s, e) {
        (S::Queued, E::Start) => Some(S::Planning),
        (S::Planning, E::Planned) => Some(S::Executing),
        (S::Executing, E::NeedInput) => Some(S::Waiting),
        (S::Waiting, E::InputArrived) => Some(S::Executing),
        (S::Executing, E::Verify) => Some(S::Verifying),
        (S::Verifying, E::Verified) => Some(S::Completed),
        (S::Verifying, E::Reject) => Some(S::Executing),
        (S::Executing, E::Succeed) => Some(S::Completed),
        // pause / resume. Resume from pause re-enters execution (the model's
        // choice; a later phase may remember the pre-pause state).
        (s, E::Pause) => (!s.is_terminal() && s != S::Paused).then_some(S::Paused),
        (S::Paused, E::Resume) => Some(S::Executing),
        // The user edits a paused task; it stays paused until resumed.
        (S::Paused, E::Edit) => Some(S::Paused),
        // An interrupted task resumes as a new segment of the same task.
        (S::Interrupted, E::Resume) => Some(S::Queued),
        // failure / interrupt / cancel from any live state
        (s, E::Fail) => (!s.is_terminal()).then_some(S::Failed),
        (s, E::Interrupt) => (!s.is_terminal()).then_some(S::Interrupted),
        (s, E::Cancel) => (!s.is_terminal()).then_some(S::Cancelled),
        // every live state has a deadline: its timeout fails the task
        (s, E::Timeout) => (!s.is_terminal()).then_some(S::Failed),
        _ => None,
    }
}

impl Machine for TaskMachine {
    type State = TaskState;
    type Event = TaskEvent;
    const ENTITY: Entity = Entity::Task;

    fn next(state: &TaskState, event: &TaskEvent) -> Result<TaskState, InvalidTransition> {
        step(*state, *event).ok_or_else(|| Self::refuse(state, event))
    }

    fn on_enter(state: &TaskState) -> Vec<Effect> {
        if state.is_terminal() {
            vec![Effect::TeardownOwned]
        } else {
            Vec::new()
        }
    }

    fn is_terminal(state: &TaskState) -> bool {
        state.is_terminal()
    }

    /// P1 (terminal states are sticky; only `interrupted --resume--> queued`
    /// leaves one), P2 (the reached state, if live, has a timeout exit to
    /// `failed`) and P9 (an edit happens only while paused and keeps the
    /// task paused).
    fn check_step(from: &TaskState, event: &TaskEvent, to: &TaskState) -> Result<(), String> {
        if from.is_terminal() && !(*from == TaskState::Interrupted && *event == TaskEvent::Resume) {
            return Err(format!(
                "P1 violated: terminal state `{from}` left via `{}` to `{to}`",
                event.label()
            ));
        }
        if *event == TaskEvent::Edit && !(*from == TaskState::Paused && *to == TaskState::Paused) {
            return Err(format!(
                "P9 violated: edit applied in `{from}` leading to `{to}`"
            ));
        }
        if !to.is_terminal() && step(*to, TaskEvent::Timeout) != Some(TaskState::Failed) {
            return Err(format!(
                "P2 violated: live state `{to}` has no timeout exit"
            ));
        }
        Ok(())
    }
}

/// The full transition table as `(state, event, Some(next) | None)` for every
/// pair, in the Lean model's export order.
pub fn table() -> Vec<(TaskState, TaskEvent, Option<TaskState>)> {
    let mut rows = Vec::with_capacity(TaskState::ALL.len() * TaskEvent::ALL.len());
    for s in TaskState::ALL {
        for e in TaskEvent::ALL {
            rows.push((s, e, step(s, e)));
        }
    }
    rows
}

#[cfg(test)]
#[path = "../../tests/unit/lifecycle/task_test.rs"]
mod tests;
