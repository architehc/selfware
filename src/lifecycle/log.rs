//! The append-only lifecycle event log (formal/DESIGN.md §7).
//!
//! One JSON object per line, one line per transition:
//! `{ts, entity, id, from, to, cause}` plus optional `event`, `owner`,
//! `task_type`, `pid`, `parent` (a forked task's original), `usage`
//! (measured token/cost usage at that moment) and `agent_type` (agent
//! records). `from` is `null` when an entity (or a new segment of
//! a task) is first recorded.
//!
//! Writing is best-effort and never fails the run: an I/O error is logged
//! once as a warning and the transition proceeds. Each record is written with
//! a single `write` on an `O_APPEND` handle so concurrent selfware processes
//! do not interleave lines; only terminal records are `fsync`ed
//! ("fsync-light"). The file rotates once to `events.jsonl.1` past
//! [`MAX_LOG_BYTES`].

use super::Entity;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Environment override for the log location: a file path, or `off` to
/// disable the log.
pub const EVENT_LOG_ENV: &str = "SELFWARE_EVENT_LOG";

/// Size past which the log rotates to `<file>.1` (one generation kept).
pub const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;

/// Longest `cause` kept in a record, in characters.
pub const MAX_CAUSE_CHARS: usize = 240;

/// One recorded transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionRecord {
    /// RFC 3339 UTC timestamp with milliseconds.
    pub ts: String,
    /// Which machine.
    pub entity: Entity,
    /// The entity id (a task's checkpoint `task_id`).
    pub id: String,
    /// The state before, or `None` when the entity / segment is first recorded.
    pub from: Option<String>,
    /// The state after.
    pub to: String,
    /// Why, in words (credential-scrubbed, at most [`MAX_CAUSE_CHARS`]).
    pub cause: String,
    /// The typed event that caused the transition (`None` on creation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    /// The owning entity (a resource's task, a task's agent), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// The task type selfware classified the task as, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_type: Option<String>,
    /// The process that recorded it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    /// The task this one was forked from (an edit of a finished task
    /// creates a new task; the original's history is never rewritten).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// The task's measured usage when this record was written (terminal
    /// transitions and edits carry it). Absent means not recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<RecordedUsage>,
    /// What kind of agent (`main`, …), on agent records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
}

/// Token and cost usage as measured by the client (Rule 4): provider-reported
/// token counts, split into the main agent loop and side calls (audits,
/// summaries, classifiers), and the provider-reported cost when every call
/// reported one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RecordedUsage {
    /// All model calls of the task (main loop and side calls).
    pub total_tokens: usize,
    /// Main agent-loop calls; `None` when the split was not measured (a
    /// resumed segment carries earlier segments' totals without it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_tokens: Option<usize>,
    /// Side calls (`total_tokens - main_tokens`); `None` with `main_tokens`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side_tokens: Option<usize>,
    /// Provider-reported USD cost; `None` when no call reported one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// Every call reported its cost (otherwise `cost_usd` is a lower bound).
    #[serde(default)]
    pub cost_complete: bool,
}

impl Eq for RecordedUsage {}

impl TransitionRecord {
    /// A record stamped now, by this process. `cause` is scrubbed of
    /// credentials, reduced to its first line and capped.
    pub fn now(
        entity: Entity,
        id: &str,
        from: Option<&str>,
        to: &str,
        event: Option<&str>,
        cause: &str,
    ) -> Self {
        Self {
            ts: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            entity,
            id: id.to_string(),
            from: from.map(str::to_string),
            to: to.to_string(),
            cause: clean_cause(cause),
            event: event.map(str::to_string),
            owner: None,
            task_type: None,
            pid: Some(std::process::id()),
            parent: None,
            usage: None,
            agent_type: None,
        }
    }
}

fn clean_cause(cause: &str) -> String {
    let redacted = crate::observability::telemetry::redact_secrets(cause);
    let first = redacted.lines().next().unwrap_or("").trim();
    if first.chars().count() > MAX_CAUSE_CHARS {
        let mut s: String = first.chars().take(MAX_CAUSE_CHARS - 1).collect();
        s.push('…');
        s
    } else {
        first.to_string()
    }
}

static WRITE_WARNED: AtomicBool = AtomicBool::new(false);

/// Handle on the event log file. Cheap to clone; `disabled()` records
/// nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventLog {
    path: Option<PathBuf>,
}

impl Default for EventLog {
    fn default() -> Self {
        Self::default_location()
    }
}

impl EventLog {
    /// A log at an explicit path (tests use a temp dir).
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Some(path.into()),
        }
    }

    /// A log that records nothing and reads as empty.
    pub fn disabled() -> Self {
        Self { path: None }
    }

    /// `~/.selfware/state/events.jsonl`, or the [`EVENT_LOG_ENV`] override
    /// (`off` disables). Unit-test builds default to a per-process temp dir
    /// so tests never write the developer's real log.
    pub fn default_location() -> Self {
        match std::env::var(EVENT_LOG_ENV) {
            Ok(v) if v.eq_ignore_ascii_case("off") || v == "0" => return Self::disabled(),
            Ok(v) if !v.trim().is_empty() => return Self::at(v),
            _ => {}
        }
        #[cfg(test)]
        {
            Self::at(
                std::env::temp_dir()
                    .join(format!("selfware-test-state-{}", std::process::id()))
                    .join("events.jsonl"),
            )
        }
        #[cfg(not(test))]
        {
            match dirs::home_dir() {
                Some(home) => Self::at(home.join(".selfware").join("state").join("events.jsonl")),
                None => Self::disabled(),
            }
        }
    }

    /// The file this log writes, if enabled.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Append one record. Best-effort: failures are logged (once per
    /// process at `warn`, then at `debug`) and never propagate. `durable`
    /// asks for an `fsync` of the data (used for terminal transitions).
    pub fn append(&self, record: &TransitionRecord, durable: bool) {
        let Some(path) = &self.path else { return };
        if let Err(e) = append_line(path, record, durable) {
            if !WRITE_WARNED.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    "lifecycle event log {} not written ({e}); the run continues without it",
                    path.display()
                );
            } else {
                tracing::debug!("lifecycle event log {} not written: {e}", path.display());
            }
        }
    }

    /// Every readable record, oldest first (the rotated generation, then the
    /// current file), and the number of lines that did not parse.
    pub fn read_all(&self) -> (Vec<TransitionRecord>, usize) {
        let Some(path) = &self.path else {
            return (Vec::new(), 0);
        };
        let mut records = Vec::new();
        let mut skipped = 0;
        for p in [rotated_path(path), path.clone()] {
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                match serde_json::from_str::<TransitionRecord>(line) {
                    Ok(r) => records.push(r),
                    Err(_) => skipped += 1,
                }
            }
        }
        (records, skipped)
    }

    /// The last recorded state of `entity` `id`, if any.
    pub fn last_state(&self, entity: Entity, id: &str) -> Option<String> {
        self.read_all()
            .0
            .into_iter()
            .rev()
            .find(|r| r.entity == entity && r.id == id)
            .map(|r| r.to)
    }
}

fn rotated_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".1");
    path.with_file_name(name)
}

fn append_line(path: &Path, record: &TransitionRecord, durable: bool) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if std::fs::metadata(path).is_ok_and(|m| m.len() >= MAX_LOG_BYTES) {
        // Best-effort single-generation rotation; a concurrent writer at
        // worst appends a line to the rotated file.
        let _ = std::fs::rename(path, rotated_path(path));
    }
    let mut line = serde_json::to_vec(record).map_err(std::io::Error::other)?;
    line.push(b'\n');
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path)?;
    file.write_all(&line)?;
    if durable {
        file.sync_data()?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/lifecycle/log_test.rs"]
mod tests;
