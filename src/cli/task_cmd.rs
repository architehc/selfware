//! `selfware task edit|pause|resume|cancel <id>` (formal/DESIGN.md §8) and
//! the `run --fork-of` check.
//!
//! These read the event log only. A task running in another selfware
//! process cannot be paused, cancelled or edited from here: there is no
//! cross-process control channel yet, and the output says so and exits
//! non-zero rather than pretending. Inside a TUI session the Tasks pane
//! (Ctrl+T) controls that session's own task.

use crate::lifecycle::projection::{resolve_task_id, task_timeline, ResolveError};
use crate::lifecycle::{EventLog, TaskState, TransitionRecord};

/// What the user asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlVerb {
    Pause,
    Resume,
    Cancel,
}

impl ControlVerb {
    fn as_str(self) -> &'static str {
        match self {
            ControlVerb::Pause => "pause",
            ControlVerb::Resume => "resume",
            ControlVerb::Cancel => "cancel",
        }
    }
}

/// A task as the log last saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Recorded {
    pub id: String,
    pub state: String,
    pub pid: Option<u32>,
}

/// Resolve `query` (id or unique prefix) to the task's last record.
pub(crate) fn resolve(
    records: &[TransitionRecord],
    query: &str,
    log: &EventLog,
) -> Result<Recorded, String> {
    let id = match resolve_task_id(records, query) {
        Ok(id) => id,
        Err(ResolveError::NotFound) => {
            return Err(format!(
                "No task matching `{query}` is recorded in {}.",
                log.path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "the event log (disabled)".into())
            ))
        }
        Err(ResolveError::Ambiguous(ids)) => {
            return Err(format!(
                "`{query}` matches {} tasks: {}. Use a longer prefix.",
                ids.len(),
                ids.join(", ")
            ))
        }
    };
    let timeline = task_timeline(records, &id);
    let last = timeline
        .last()
        .ok_or_else(|| format!("task {id} has no records"))?;
    Ok(Recorded {
        id: id.clone(),
        state: last.to.clone(),
        pid: last.pid,
    })
}

fn is_terminal(state: &str) -> bool {
    TaskState::from_label(state).is_some_and(TaskState::is_terminal)
}

fn other_process_note(task: &Recorded, alive: &dyn Fn(u32) -> bool) -> String {
    match task.pid {
        Some(pid) if alive(pid) => format!(
            "Task {} is {} in selfware process {pid}. Controlling a task in another \
             process is not supported yet: use the Tasks pane (Ctrl+T) in that \
             session's TUI, or stop that process with Ctrl-C (the task is then \
             interrupted and `selfware resume {}` continues it).",
            task.id, task.state, task.id
        ),
        Some(pid) => format!(
            "Task {} was last recorded {} by selfware process {pid}, which is no longer \
             running, so nothing is running it now. `selfware resume {}` continues it.",
            task.id, task.state, task.id
        ),
        None => format!(
            "Task {} was last recorded {} by an unknown process. Controlling a task \
             in another process is not supported yet.",
            task.id, task.state
        ),
    }
}

/// `selfware task pause|resume|cancel <id>`. `Ok` is printed with exit 0;
/// `Err` is printed and exits non-zero (nothing was done).
pub(crate) fn control_output(
    records: &[TransitionRecord],
    log: &EventLog,
    query: &str,
    verb: ControlVerb,
    alive: &dyn Fn(u32) -> bool,
) -> Result<String, String> {
    let task = resolve(records, query, log)?;
    if verb == ControlVerb::Resume && task.state == TaskState::Interrupted.to_string() {
        return Ok(format!(
            "Task {} was interrupted. Resume it with:\n  selfware resume {}\n",
            task.id, task.id
        ));
    }
    if is_terminal(&task.state) {
        return Err(format!(
            "Task {} already finished ({}); there is nothing to {}. \
             `selfware task edit {}` forks it into a new task.",
            task.id,
            task.state,
            verb.as_str(),
            task.id
        ));
    }
    Err(format!("Not done: {}", other_process_note(&task, alive)))
}

/// Decide what `selfware task edit <id>` does, before any editor opens: a
/// finished task is forked (returned with its original description, when
/// the checkpoint still has it); anything else is refused with the reason.
pub(crate) fn plan_edit(
    records: &[TransitionRecord],
    log: &EventLog,
    query: &str,
    description_of: &dyn Fn(&str) -> Option<String>,
    alive: &dyn Fn(u32) -> bool,
) -> Result<(Recorded, Option<String>), String> {
    let task = resolve(records, query, log)?;
    if !is_terminal(&task.state) {
        return Err(format!("Not done: {}", other_process_note(&task, alive)));
    }
    let original = description_of(&task.id);
    Ok((task, original))
}

/// The text printed after a fork's new description is known.
pub(crate) fn fork_instructions(parent: &Recorded, description: &str) -> String {
    let quoted = shlex::try_quote(description)
        .map(|q| q.into_owned())
        .unwrap_or_else(|_| format!("{description:?}"));
    format!(
        "Task {} is {}; its history is kept as it is. The edit becomes a new task \
         forked from it (new task id, parent {}), recorded when it runs.\n\
         Run it with:\n  selfware run --fork-of {} {}\n",
        parent.id, parent.state, parent.id, parent.id, quoted
    )
}

/// `run --fork-of <query>`: the full id of a finished task, or why not.
pub(crate) fn fork_parent(
    records: &[TransitionRecord],
    log: &EventLog,
    query: &str,
) -> Result<String, String> {
    let task = resolve(records, query, log)?;
    if !is_terminal(&task.state) {
        return Err(format!(
            "task {} is {}, not finished; only a finished task is forked",
            task.id, task.state
        ));
    }
    Ok(task.id)
}

/// Everything from this line on in the edited file is ignored.
pub(crate) const EDIT_CUT_LINE: &str =
    "# ------------------------ selfware: everything below this line is ignored ------------------------";

/// Edit `original` in `$VISUAL`/`$EDITOR` (default `vi`). Returns the new
/// text above [`EDIT_CUT_LINE`], trimmed.
pub(crate) fn edit_in_editor(original: &str) -> anyhow::Result<String> {
    use std::io::Write;
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    let mut file = tempfile::Builder::new()
        .prefix("selfware-task-")
        .suffix(".md")
        .tempfile()?;
    writeln!(
        file,
        "{original}\n\n{EDIT_CUT_LINE}\nEdit the task description above this line, save and quit.\nAn empty description cancels the edit."
    )?;
    file.flush()?;
    let mut parts = shlex::split(&editor).unwrap_or_else(|| vec![editor.clone()]);
    if parts.is_empty() {
        parts.push("vi".into());
    }
    let mut cmd = std::process::Command::new(&parts[0]);
    // The editor needs the terminal and its own settings, not selfware's
    // credentials.
    crate::safety::process_env::sanitize_std_command_env_preserve(
        &mut cmd,
        &[
            "EDITOR",
            "VISUAL",
            "DISPLAY",
            "LANG",
            "LC_ALL",
            "LC_CTYPE",
            "XDG_CONFIG_HOME",
        ],
    );
    let status = cmd.args(&parts[1..]).arg(file.path()).status()?;
    anyhow::ensure!(status.success(), "editor `{editor}` exited with {status}");
    let text = std::fs::read_to_string(file.path())?;
    Ok(above_cut_line(&text))
}

/// The text above [`EDIT_CUT_LINE`] (all of it when the line was deleted),
/// trimmed.
pub(crate) fn above_cut_line(text: &str) -> String {
    text.split(EDIT_CUT_LINE)
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
}

#[cfg(test)]
#[path = "../../tests/unit/cli/task_cmd_test.rs"]
mod tests;
