//! Read-side projections of the event log (`selfware tasks`,
//! `selfware task show`). Everything shown is recorded data: states and
//! timestamps come from the log, durations are differences of recorded
//! timestamps, and a task type appears only when one was recorded.

use super::{Entity, TransitionRecord};
use std::collections::HashMap;

/// One task, as the log last saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSummary {
    /// The task id.
    pub id: String,
    /// The last recorded state.
    pub state: String,
    /// When the task was first recorded.
    pub first_ts: String,
    /// When its last transition was recorded.
    pub last_ts: String,
    /// The last recorded task type, if any.
    pub task_type: Option<String>,
    /// Number of records for this task.
    pub records: usize,
}

/// Every task in `records`, most recently active first.
pub fn task_summaries(records: &[TransitionRecord]) -> Vec<TaskSummary> {
    let mut by_id: HashMap<&str, TaskSummary> = HashMap::new();
    for r in records.iter().filter(|r| r.entity == Entity::Task) {
        let entry = by_id.entry(&r.id).or_insert_with(|| TaskSummary {
            id: r.id.clone(),
            state: r.to.clone(),
            first_ts: r.ts.clone(),
            last_ts: r.ts.clone(),
            task_type: None,
            records: 0,
        });
        // Records are in append order; the last one wins.
        entry.state = r.to.clone();
        entry.last_ts = r.ts.clone();
        entry.records += 1;
        if r.task_type.is_some() {
            entry.task_type = r.task_type.clone();
        }
    }
    let mut out: Vec<TaskSummary> = by_id.into_values().collect();
    // RFC 3339 UTC with fixed millisecond precision sorts lexicographically.
    out.sort_by(|a, b| b.last_ts.cmp(&a.last_ts).then_with(|| a.id.cmp(&b.id)));
    out
}

/// The records of task `id`, in append order.
pub fn task_timeline<'a>(records: &'a [TransitionRecord], id: &str) -> Vec<&'a TransitionRecord> {
    records
        .iter()
        .filter(|r| r.entity == Entity::Task && r.id == id)
        .collect()
}

/// Why a task id did not resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    /// No recorded task has this id or id prefix.
    NotFound,
    /// Several recorded tasks share this prefix.
    Ambiguous(Vec<String>),
}

/// Resolve a full task id or a unique prefix of one.
pub fn resolve_task_id(records: &[TransitionRecord], query: &str) -> Result<String, ResolveError> {
    let mut ids: Vec<&str> = records
        .iter()
        .filter(|r| r.entity == Entity::Task)
        .map(|r| r.id.as_str())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.contains(&query) {
        return Ok(query.to_string());
    }
    let matches: Vec<String> = ids
        .into_iter()
        .filter(|id| !query.is_empty() && id.starts_with(query))
        .map(str::to_string)
        .collect();
    match matches.len() {
        0 => Err(ResolveError::NotFound),
        1 => Ok(matches.into_iter().next().unwrap_or_default()),
        _ => Err(ResolveError::Ambiguous(matches)),
    }
}

/// The `selfware tasks` table for the `limit` most recent tasks.
pub fn render_task_list(summaries: &[TaskSummary], limit: usize) -> String {
    let mut out = format!(
        "{:<36}  {:<11}  {:<24}  {}\n",
        "ID", "STATE", "LAST TRANSITION", "TYPE"
    );
    for s in summaries.iter().take(limit) {
        out.push_str(&format!(
            "{:<36}  {:<11}  {:<24}  {}\n",
            s.id,
            s.state,
            s.last_ts,
            s.task_type.as_deref().unwrap_or("-")
        ));
    }
    if summaries.len() > limit {
        out.push_str(&format!(
            "({} more; use --limit to show them)\n",
            summaries.len() - limit
        ));
    }
    out
}

fn parse_ts(ts: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_rfc3339(ts).ok()
}

fn fmt_delta(ms: i64) -> String {
    if ms < 1000 {
        format!("+{ms}ms")
    } else if ms < 60_000 {
        format!("+{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("+{}m{:02}s", ms / 60_000, (ms % 60_000) / 1000)
    }
}

/// The `selfware task show` timeline: one line per recorded transition,
/// with the time since the previous record (measured from the recorded
/// timestamps). A record with no `from` opens a new segment.
pub fn render_timeline(id: &str, timeline: &[&TransitionRecord]) -> String {
    let mut out = format!("Task {id}\n");
    if let Some(t) = timeline.iter().rev().find_map(|r| r.task_type.as_deref()) {
        out.push_str(&format!("  type: {t}\n"));
    }
    if let Some(last) = timeline.last() {
        out.push_str(&format!("  state: {} (at {})\n", last.to, last.ts));
    }
    out.push('\n');
    let mut prev: Option<chrono::DateTime<chrono::FixedOffset>> = None;
    for r in timeline {
        let now = parse_ts(&r.ts);
        let delta = match (prev, now) {
            (Some(p), Some(n)) => fmt_delta((n - p).num_milliseconds().max(0)),
            _ => String::new(),
        };
        prev = now.or(prev);
        let transition = match &r.from {
            Some(from) => format!("{from} → {}", r.to),
            None => format!("(new) → {}", r.to),
        };
        let event = r
            .event
            .as_deref()
            .map(|e| format!("[{e}] "))
            .unwrap_or_default();
        out.push_str(&format!(
            "  {:<24} {:>9}  {:<26} {}{}\n",
            r.ts, delta, transition, event, r.cause
        ));
    }
    out
}

fn log_location(log: &super::EventLog) -> String {
    log.path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| format!("(disabled via {})", super::EVENT_LOG_ENV))
}

fn skipped_note(skipped: usize) -> String {
    if skipped == 0 {
        String::new()
    } else {
        format!("({skipped} unreadable log line(s) skipped)\n")
    }
}

/// Output of `selfware tasks [--limit N]`.
pub fn tasks_command_output(log: &super::EventLog, limit: usize) -> String {
    let (records, skipped) = log.read_all();
    let summaries = task_summaries(&records);
    let mut out = String::new();
    if summaries.is_empty() {
        out.push_str(&format!(
            "No task transitions recorded in {}.\n",
            log_location(log)
        ));
    } else {
        out.push_str(&render_task_list(&summaries, limit));
        out.push_str(&format!("(from {})\n", log_location(log)));
    }
    out.push_str(&skipped_note(skipped));
    out
}

/// Output of `selfware task show <id>` (a full id or a unique prefix), or
/// the message to print when it does not resolve.
pub fn task_show_output(log: &super::EventLog, query: &str) -> Result<String, String> {
    let (records, skipped) = log.read_all();
    match resolve_task_id(&records, query) {
        Ok(id) => {
            let timeline = task_timeline(&records, &id);
            let mut out = render_timeline(&id, &timeline);
            out.push_str(&skipped_note(skipped));
            Ok(out)
        }
        Err(ResolveError::NotFound) => Err(format!(
            "No task matching `{query}` is recorded in {}.",
            log_location(log)
        )),
        Err(ResolveError::Ambiguous(ids)) => Err(format!(
            "`{query}` matches {} tasks: {}. Use a longer prefix.",
            ids.len(),
            ids.join(", ")
        )),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/lifecycle/projection_test.rs"]
mod tests;
