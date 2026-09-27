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
//! | pause requested, loop reaches its safe point | `pause`                   |
//! | edit applied while paused                    | `edit`                     |
//! | resume requested (or after an edit)          | `resume`                   |
//! | run ends after a cancel request (Tasks pane) | `cancel` (not `interrupt`) |
//! | `run --fork-of` / fork from the Tasks pane   | (new) → queued, `parent`   |
//!
//! Pause, edit, resume and cancel come from the shared
//! [`TaskControl`](crate::lifecycle::control::TaskControl) and are acted on
//! only at the loop's safe point — the top of an iteration, between steps —
//! so a pause never interrupts a model call or a tool call.
//!
//! Not mapped yet (no main-loop site owns them today): `need_input` /
//! `input_arrived` (approval prompts), `verify` / `verified` / `reject`
//! (the completion gate).
//!
//! The agent itself is mirrored onto [`crate::lifecycle::AgentMachine`]
//! (type `main`, id `main-<8 hex>`): created idle with its first task,
//! `assign` when a task (or a resumed segment) starts, `block` / `unblock`
//! while its task is paused, `done` when the task ends, `stop` when the
//! agent is dropped. Its tasks carry it as `owner` and their terminal
//! records carry the measured `usage` — what `selfware agents` and the
//! Tasks pane's agent list project.

use super::Agent;
use crate::errors::{AgentError, RunEnd, SelfwareError};
use crate::lifecycle::control::{LiveTask, TaskConstraints, TaskControl, TaskEdit, MAIN_AGENT};
use crate::lifecycle::{
    AgentEvent, AgentMachine, AgentState, Effect, Entity, RecordedUsage, TaskEvent, TaskMachine,
    TaskState, Tracked,
};
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

/// Why a finished run's resource teardown is not driven by
/// [`Effect::TeardownOwned`], or `None` when it is. `tracked` is the task's
/// lifecycle state after `lifecycle_finish`. Teardown still runs in every
/// case (a drain is never lost); this names why it had to run without the
/// effect, for the warning.
pub(crate) fn teardown_fallback_reason(
    effects: &[Effect],
    tracked: Option<TaskState>,
) -> Option<String> {
    if effects.contains(&Effect::TeardownOwned) {
        return None;
    }
    Some(match tracked {
        None => "no task lifecycle tracker for this run".to_string(),
        Some(s) if s.is_terminal() => format!(
            "the task was already `{s}` when the run ended, so no terminal transition was recorded"
        ),
        Some(s) => format!("the terminal transition from `{s}` was refused"),
    })
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

    /// Apply `event` to this agent, logging (never failing on) a refusal.
    fn agent_apply(&mut self, event: AgentEvent, cause: &str) {
        let Some(agent) = self.agent_lifecycle.as_mut() else {
            return;
        };
        if let Err(e) = agent.apply(event, cause) {
            warn!("agent lifecycle ({}): {e}", agent.id());
        }
    }

    /// The agent takes `task_id`: recorded idle on its first task, then
    /// `assign`. A task it still holds that never reported an outcome is
    /// closed with `done` first, in those words.
    fn lifecycle_agent_assign(&mut self, task_id: &str) {
        if self.agent_lifecycle.is_none() {
            let tracked: Tracked<AgentMachine> = Tracked::new(
                self.agent_id.clone(),
                AgentState::Idle,
                self.event_log.clone(),
            )
            .with_agent_type(MAIN_AGENT);
            tracked.record_created("agent started");
            self.agent_lifecycle = Some(tracked);
        }
        let held = self
            .agent_lifecycle
            .as_ref()
            .and_then(|a| a.state().task().map(str::to_string));
        if let Some(prev) = held {
            if prev == task_id {
                return;
            }
            self.agent_apply(
                AgentEvent::Done,
                &format!("task {prev} ended without reporting an outcome"),
            );
        }
        self.agent_apply(
            AgentEvent::Assign {
                task: task_id.to_string(),
            },
            &format!("assigned task {task_id}"),
        );
    }

    /// The agent is going away (dropped at session / process end).
    pub(super) fn lifecycle_stop_agent(&mut self) {
        if self
            .agent_lifecycle
            .as_ref()
            .is_some_and(|a| !a.is_terminal())
        {
            self.agent_apply(AgentEvent::Stop, "agent ended (session or process end)");
        }
    }

    /// This agent's lifecycle state, if it has worked on a task.
    pub fn agent_lifecycle_state(&self) -> Option<&AgentState> {
        self.agent_lifecycle.as_ref().map(|a| a.state())
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
        self.task_control.begin_task();
        self.task_main_loop_base = Some(self.session_main_loop_tokens());
        let mut tracked: Tracked<TaskMachine> =
            Tracked::new(task_id, TaskState::Queued, self.event_log.clone())
                .with_task_type(Self::infer_task_type(task))
                .with_owner(self.agent_id.clone());
        let cause = match self.pending_fork_parent.take() {
            Some(parent) => {
                let cause = format!("forked from task {parent}");
                tracked = tracked.with_parent(parent);
                cause
            }
            None => "task created".to_string(),
        };
        tracked.record_created(&cause);
        self.task_lifecycle = Some(tracked);
        self.lifecycle_apply(TaskEvent::Start, "run started");
        self.lifecycle_agent_assign(task_id);
        self.publish_live_task();
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
        self.task_control.begin_task();
        // The restored usage includes earlier segments whose main/side split
        // was not recorded: report the total only.
        self.task_main_loop_base = None;
        let last = in_memory.or_else(|| {
            self.event_log
                .last_state(Entity::Task, &task_id)
                .and_then(|s| TaskState::from_label(&s))
        });
        let task_type = Self::infer_task_type(&task);
        let log = self.event_log.clone();
        let agent = self.agent_id.clone();
        let assigned = task_id.clone();
        if last == Some(TaskState::Interrupted) {
            self.task_lifecycle = Some(
                Tracked::new(task_id, TaskState::Interrupted, log)
                    .with_task_type(task_type)
                    .with_owner(agent),
            );
            self.lifecycle_apply(TaskEvent::Resume, "resumed");
        } else {
            let tracked: Tracked<TaskMachine> = Tracked::new(task_id, TaskState::Queued, log)
                .with_task_type(task_type)
                .with_owner(agent);
            tracked.record_created(&match last {
                Some(s) => format!("new segment: resumed after last recorded state `{s}`"),
                None => "new segment: resumed with no earlier record".to_string(),
            });
            self.task_lifecycle = Some(tracked);
        }
        self.lifecycle_apply(TaskEvent::Start, "run resumed");
        self.lifecycle_agent_assign(&assigned);
        self.publish_live_task();
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
        let (mut event, mut cause) = terminal_event_for_run(result, crate::shutdown_reason());
        if event == TaskEvent::Interrupt && self.task_control.cancel_requested() {
            // The stop came from a cancel request, not Ctrl-C / SIGTERM: the
            // task was abandoned on purpose and is not resumable.
            event = TaskEvent::Cancel;
            cause = "cancelled by the user".to_string();
        }
        // Say in the task's own record what the teardown effect is about to
        // drain (its outcome is recorded per resource, owner = this task).
        if let Some(id) = self.task_lifecycle.as_ref().map(|t| t.id().to_string()) {
            let owned = crate::resources::ResourceRegistry::global()
                .owned_by(&id)
                .len();
            if owned > 0 {
                cause.push_str(&format!(
                    "; {owned} owned resource{} to drain",
                    if owned == 1 { "" } else { "s" }
                ));
            }
        }
        if event == TaskEvent::Succeed && state == TaskState::Planning {
            // The planning turn itself produced the accepted answer.
            self.lifecycle_apply(TaskEvent::Planned, "answered in the planning turn");
        }
        let usage = self.measured_task_usage();
        if let Some(task) = self.task_lifecycle.as_mut() {
            task.attach_usage(usage);
        }
        let effects = self.lifecycle_apply(event, &cause);
        if let Some((id, end)) = self
            .task_lifecycle
            .as_ref()
            .filter(|t| t.is_terminal())
            .map(|t| (t.id().to_string(), *t.state()))
        {
            self.agent_apply(AgentEvent::Done, &format!("task {id} {end}"));
        }
        self.publish_live_task();
        effects
    }

    /// The current task's lifecycle state, if a task is tracked.
    pub fn task_lifecycle_state(&self) -> Option<TaskState> {
        self.task_lifecycle.as_ref().map(|t| *t.state())
    }

    /// The control handle an in-process UI uses to pause, resume, cancel
    /// and edit this agent's task (see [`TaskControl`]).
    pub fn task_control(&self) -> TaskControl {
        self.task_control.clone()
    }

    /// The next `run_task` is a fork of finished task `parent`: it gets a new
    /// task id recorded with `parent` as its origin.
    pub fn set_fork_parent(&mut self, parent: impl Into<String>) {
        self.pending_fork_parent = Some(parent.into());
    }

    /// The current task's usage as measured by the client (Rule 4). The
    /// main/side split is the session main-loop counter's growth since the
    /// task started; it is left out when that start was not observed.
    pub(crate) fn measured_task_usage(&self) -> RecordedUsage {
        let usage = self.current_task_usage();
        let main = self.task_main_loop_base.map(|base| {
            let grown = self.session_main_loop_tokens().saturating_sub(base);
            usize::try_from(grown)
                .unwrap_or(usize::MAX)
                .min(usage.total_tokens)
        });
        RecordedUsage {
            total_tokens: usage.total_tokens,
            main_tokens: main,
            side_tokens: main.map(|m| usage.total_tokens - m),
            cost_usd: usage.cost_usd,
            cost_complete: usage.cost_complete,
        }
    }

    fn task_constraints(&self) -> TaskConstraints {
        TaskConstraints {
            max_turns: self.loop_control.max_iterations(),
            turns_used: self.loop_control.current_iteration(),
            token_budget: self.config.agent.max_budget_tokens.filter(|&b| b > 0),
            allowed_paths: self.config.safety.allowed_paths.clone(),
        }
    }

    /// The snapshot of the current task the Tasks pane shows.
    pub(crate) fn live_task_snapshot(&self) -> Option<LiveTask> {
        let task = self.task_lifecycle.as_ref()?;
        let description = self
            .current_checkpoint
            .as_ref()
            .filter(|c| c.task_id == task.id())
            .map(|c| c.task_description.clone())
            .unwrap_or_default();
        let usage = self.measured_task_usage();
        Some(LiveTask {
            id: task.id().to_string(),
            agent: self.agent_id.clone(),
            task_type: Some(Self::infer_task_type(&description).to_string()),
            description,
            state: *task.state(),
            state_since: chrono::Utc::now(),
            step: self.loop_control.current_step(),
            constraints: self.task_constraints(),
            usage: (usage.total_tokens > 0 || usage.cost_usd.is_some()).then_some(usage),
            parent: task.parent().map(str::to_string),
            pause_pending: false,
            paused: self.task_paused(),
        })
    }

    /// Publish the current task's snapshot to the shared control.
    pub(super) fn publish_live_task(&self) {
        if let Some(live) = self.live_task_snapshot() {
            self.task_control.publish(live);
        }
    }

    /// The loop's safe point (top of an iteration, between steps): publish
    /// the snapshot, and if a pause or an edit was requested, pause here —
    /// recording `pause`, then `edit` for an applied edit, then `resume` —
    /// until the user resumes or cancels. Nothing is in flight while
    /// paused: the previous step's model and tool calls have completed.
    ///
    /// Time spent paused is measured and taken out of every wall clock the
    /// run is held to (`max_wall_secs` on both the agent and the API client
    /// side, and the deadline-based lifecycle `timeout` that follows from
    /// them); see `Agent::exclude_paused_time`.
    pub(super) async fn task_control_safe_point(&mut self) {
        self.publish_live_task();
        if !self.task_control.pause_requested() {
            return;
        }
        let pausable = self
            .task_lifecycle
            .as_ref()
            .is_some_and(|t| !t.is_terminal() && *t.state() != TaskState::Paused);
        if !pausable {
            self.task_control.clear_pause();
            return;
        }
        self.lifecycle_apply(TaskEvent::Pause, "paused by the user between steps");
        self.agent_apply(
            AgentEvent::Block {
                reason: "task paused".to_string(),
            },
            "its task was paused",
        );
        let paused_at = std::time::Instant::now();
        self.publish_live_task();
        self.emit_event(super::AgentEvent::Status {
            message: "Task paused between steps — resume, edit or cancel it in the Tasks pane"
                .to_string(),
        });
        let mut edited = false;
        loop {
            if self.is_cancelled() {
                self.exclude_paused_time(paused_at.elapsed());
                // The loop-top cancellation check ends the run next.
                return;
            }
            if let Some(edit) = self.task_control.take_edit() {
                self.apply_task_edit(edit);
                edited = true;
                // Pause → edit → resume.
                self.task_control.clear_pause();
                break;
            }
            if self.task_control.take_resume() {
                break;
            }
            crate::supervision::health::record_heartbeat();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        self.exclude_paused_time(paused_at.elapsed());
        let cause = if edited {
            "resumed after an edit"
        } else {
            "resumed by the user"
        };
        self.lifecycle_apply(TaskEvent::Resume, cause);
        self.agent_apply(AgentEvent::Unblock, cause);
        self.publish_live_task();
        self.emit_event(super::AgentEvent::Status {
            message: format!("Task {cause}"),
        });
    }

    /// Apply a user edit to the paused task: new constraints take effect
    /// for the rest of the run, the change is recorded as an `edit` event
    /// (with the measured usage at that moment) and delivered to the model
    /// as a user message, so the conversation says what changed. An edit
    /// that changes nothing records nothing; one that no longer validates
    /// (progress moved on since it was submitted) is reported, not applied.
    fn apply_task_edit(&mut self, mut edit: TaskEdit) {
        let Some(before) = self.live_task_snapshot() else {
            return;
        };
        if let Err(reason) = edit.validate(&before) {
            self.emit_event(super::AgentEvent::Status {
                message: format!("Edit not applied: {reason}"),
            });
            return;
        }
        if edit.token_budget != before.constraints.token_budget {
            let mut config = self.config.clone();
            config.agent.max_budget_tokens = edit.token_budget;
            if let Err(e) = self.install_config(config) {
                // Record and announce only what actually took effect.
                warn!("task edit: token budget not applied: {e}");
                self.emit_event(super::AgentEvent::Status {
                    message: format!("Edit: token budget not applied ({e})"),
                });
                edit.token_budget = before.constraints.token_budget;
            }
        }
        let Some(changes) = edit.changes(&before) else {
            return;
        };
        if edit.max_turns != before.constraints.max_turns {
            self.loop_control.set_max_iterations(edit.max_turns);
        }
        if edit.description.trim() != before.description.trim() {
            if let Some(cp) = self.current_checkpoint.as_mut() {
                cp.task_description = edit.description.trim().to_string();
            }
        }
        self.messages.push(crate::api::types::Message::user(format!(
            "Task updated: {changes}"
        )));
        let usage = self.measured_task_usage();
        if let Some(task) = self.task_lifecycle.as_mut() {
            task.attach_usage(usage);
        }
        self.lifecycle_apply(TaskEvent::Edit, &format!("edited: {changes}"));
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/lifecycle_wiring/lifecycle_wiring_test.rs"]
mod tests;
