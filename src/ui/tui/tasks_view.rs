//! The Tasks pane (formal/DESIGN.md §8, phase 5): navigate from the
//! session to an agent, to a task, to a resource or a recorded transition,
//! and back; pause, edit, cancel, fork and reap from there.
//!
//! ```text
//! Session › Agents › main › Task 01J9…  "add max_words to slugify()"
//! ─────────────────────────────────────────────────────────────────
//!  state      executing (step 12, 3m41s)      type   mutation
//!  tokens     212000 (main 177000 · side 35000)  cost  not reported
//!  …
//! [Enter] open  [e] edit  [p] pause  [x] cancel  [r] reap  [Esc] back
//! ```
//!
//! Split for testing: [`TasksInputs`] is everything the pane shows (event
//! log records, resource registry rows, the in-process live task, journal
//! descriptions, the clock), [`build_view`] is a pure function from inputs
//! and navigation state to a [`TasksView`], and [`render`] draws a view.
//! [`TasksPane::on_key`] handles keys; the only side effects it performs are
//! requests on the in-process [`TaskControl`] — anything else (sending a
//! fork to the agent, reaping) is returned as a [`PaneAction`] for the event
//! loop to carry out.

use crate::lifecycle::control::{LiveTask, TaskControl, TaskEdit, MAIN_AGENT};
use crate::lifecycle::projection::{
    agent_display, agent_summaries, resources_held, task_summaries, task_timeline, usage_line,
    TaskSummary,
};
use crate::lifecycle::{Entity, TaskState, TransitionRecord};
use chrono::{DateTime, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use std::collections::HashMap;

use super::TuiPalette;

// ---- inputs ---------------------------------------------------------------

/// One registered resource, as the pane shows it. Built from the resource
/// registry with read-only calls only ([`resource_rows_from_registry`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRow {
    pub id: String,
    pub kind: String,
    pub state: String,
    pub handle: String,
    pub port: Option<u16>,
    pub owner_task: String,
    /// The agent the resource was spawned under, when recorded.
    pub owner_agent: Option<String>,
    pub label: String,
    pub note: Option<String>,
    /// selfware can stop this kind itself (container, process, pty, browser).
    pub drainable: bool,
}

impl ResourceRow {
    /// Left behind: teardown could not confirm release, or the owning
    /// session ended without teardown. The pane reaps only these.
    pub fn is_reapable(&self) -> bool {
        self.drainable && matches!(self.state.as_str(), "leaked" | "orphaned")
    }
}

/// Everything the pane shows.
#[derive(Debug, Clone, Default)]
pub struct TasksInputs {
    /// The lifecycle event log, oldest first.
    pub records: Vec<TransitionRecord>,
    /// Registered resources (all sessions).
    pub resources: Vec<ResourceRow>,
    /// The task running (or last run) in this process.
    pub live: Option<LiveTask>,
    /// Task descriptions from the checkpoint journal, by task id.
    pub descriptions: HashMap<String, String>,
    /// This process's pid (a non-terminal task recorded by another pid is
    /// not controllable here).
    pub this_pid: u32,
    /// Pids (other than this one) that recorded agents and still run; an
    /// agent's live state recorded by any other pid reads "(process ended)".
    pub live_pids: std::collections::HashSet<u32>,
    /// Now.
    pub now: DateTime<Utc>,
}

/// Registry rows for the pane: read-only listing only.
pub fn resource_rows_from_registry(
    registry: &crate::resources::ResourceRegistry,
) -> Vec<ResourceRow> {
    registry
        .snapshot()
        .resources
        .into_iter()
        .map(|r| ResourceRow {
            port: match &r.handle {
                crate::resources::ResourceHandle::Port { port } => Some(*port),
                _ => None,
            },
            handle: r.handle.short(),
            id: r.id,
            kind: r.kind.as_str().to_string(),
            state: r.state.as_str().to_string(),
            owner_task: r.owner_task,
            owner_agent: r.owner_agent,
            label: r.label,
            note: r.note,
            drainable: r.kind.is_drainable(),
        })
        .collect()
}

/// One agent row: the `selfware agents` projection
/// ([`agent_summaries`]) of the event log, formatted by [`agent_display`],
/// with the resources the registry attributes to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRow {
    pub id: String,
    /// Recorded agent type, or `not recorded`.
    pub agent_type: String,
    /// Recorded state (`(process ended)` when its process is gone).
    pub state: String,
    /// Time in that state, from the recorded timestamp.
    pub in_state: String,
    /// Tasks recorded for this agent.
    pub tasks: usize,
    pub completed: usize,
    pub failed: usize,
    /// Interrupted or cancelled.
    pub stopped: usize,
    /// Measured token total of its ended tasks, or `not recorded`.
    pub tokens: String,
    /// The most recently active task.
    pub last_task: Option<String>,
    /// Its state.
    pub last_state: Option<String>,
    /// Its task type.
    pub last_task_type: Option<String>,
    /// Unreleased resources it (or one of its tasks) owns.
    pub resources: usize,
}

/// The session's agents: the agent projection of the event log, plus the
/// live task's agent before the log shows it.
pub fn agents_from(inputs: &TasksInputs) -> Vec<AgentRow> {
    let alive = |pid: u32| pid == inputs.this_pid || inputs.live_pids.contains(&pid);
    let unreleased: Vec<(&str, Option<&str>)> = inputs
        .resources
        .iter()
        .filter(|r| r.state != "released")
        .map(|r| (r.owner_task.as_str(), r.owner_agent.as_deref()))
        .collect();
    let mut rows: Vec<AgentRow> = agent_summaries(&inputs.records)
        .iter()
        .map(|a| {
            let d = agent_display(a, inputs.now, &alive);
            AgentRow {
                id: a.id.clone(),
                agent_type: a
                    .agent_type
                    .clone()
                    .unwrap_or_else(|| "not recorded".into()),
                state: d.state,
                in_state: d.in_state,
                tasks: a.task_ids.len(),
                completed: a.tasks_completed,
                failed: a.tasks_failed,
                stopped: a.tasks_stopped,
                tokens: d.tokens,
                last_task: a.last_task.clone(),
                last_state: a.last_task_state.clone(),
                last_task_type: a.last_task_type.clone(),
                resources: resources_held(a, unreleased.iter().copied()),
            }
        })
        .collect();
    if let Some(live) = inputs.live.as_ref() {
        match rows.iter_mut().find(|r| r.id == live.agent) {
            Some(row) => {
                // The live task is this agent's current one, and its
                // in-process state is at least as fresh as the log's.
                let logged = inputs
                    .records
                    .iter()
                    .any(|r| r.entity == Entity::Task && r.id == live.id);
                if !logged {
                    row.tasks += 1;
                }
                row.last_task = Some(live.id.clone());
                row.last_state = Some(live.state.to_string());
                row.last_task_type = live.task_type.clone();
            }
            None => rows.push(AgentRow {
                id: live.agent.clone(),
                agent_type: "not recorded".into(),
                state: "not recorded".into(),
                in_state: "-".into(),
                tasks: 1,
                completed: 0,
                failed: 0,
                stopped: 0,
                tokens: "not recorded".into(),
                last_task: Some(live.id.clone()),
                last_state: Some(live.state.to_string()),
                last_task_type: live.task_type.clone(),
                resources: unreleased
                    .iter()
                    .filter(|(t, a)| *t == live.id || *a == Some(live.agent.as_str()))
                    .count(),
            }),
        }
    }
    if rows.is_empty() {
        rows.push(AgentRow {
            id: MAIN_AGENT.to_string(),
            agent_type: MAIN_AGENT.to_string(),
            state: "not recorded".into(),
            in_state: "-".into(),
            tasks: 0,
            completed: 0,
            failed: 0,
            stopped: 0,
            tokens: "not recorded".into(),
            last_task: None,
            last_state: None,
            last_task_type: None,
            resources: 0,
        });
    }
    rows
}

// ---- navigation -----------------------------------------------------------

/// Where the pane is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Level {
    /// The session's agents.
    Agents,
    /// One agent's tasks.
    Tasks { agent: String },
    /// One task.
    Task { agent: String, id: String },
    /// One resource of a task.
    Resource {
        agent: String,
        task: String,
        id: String,
    },
    /// One recorded transition of a task (index into its timeline).
    Record {
        agent: String,
        task: String,
        index: usize,
    },
}

/// A level plus the selection and scroll it had.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavFrame {
    pub level: Level,
    pub selected: usize,
    pub scroll: usize,
}

/// The back stack: the current frame plus every frame Enter left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nav {
    pub current: NavFrame,
    pub back: Vec<NavFrame>,
}

impl Default for Nav {
    fn default() -> Self {
        Self {
            current: NavFrame {
                level: Level::Agents,
                selected: 0,
                scroll: 0,
            },
            back: Vec::new(),
        }
    }
}

impl Nav {
    /// Drill into `level`, remembering where we were.
    pub fn push(&mut self, level: Level) {
        let prev = std::mem::replace(
            &mut self.current,
            NavFrame {
                level,
                selected: 0,
                scroll: 0,
            },
        );
        self.back.push(prev);
    }

    /// Go back, restoring the previous selection and scroll. `false` at the
    /// root.
    pub fn pop(&mut self) -> bool {
        match self.back.pop() {
            Some(frame) => {
                self.current = frame;
                true
            }
            None => false,
        }
    }
}

/// One selectable row and where Enter takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub target: Option<Level>,
}

// ---- view model -------------------------------------------------------------

/// What the pane draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TasksView {
    /// `Session › Agents › main › Task 01J9… › …`
    pub breadcrumb: Vec<String>,
    /// Title of the body.
    pub title: String,
    /// Label/value pairs above the list (detail levels).
    pub fields: Vec<(String, String)>,
    /// Selectable rows.
    pub items: Vec<Item>,
    pub selected: usize,
    pub scroll: usize,
    /// Key hints.
    pub keys: String,
}

const KEYS: &str = "[Enter] open  [e] edit  [p] pause/resume  [x] cancel  [r] reap  [Esc] back";

fn short_id(id: &str) -> String {
    if id.chars().count() > 13 {
        format!("{}…", id.chars().take(12).collect::<String>())
    } else {
        id.to_string()
    }
}

fn clip(s: &str, max: usize) -> String {
    let first = s.lines().next().unwrap_or("");
    if first.chars().count() > max {
        format!(
            "{}…",
            first
                .chars()
                .take(max.saturating_sub(1))
                .collect::<String>()
        )
    } else {
        first.to_string()
    }
}

/// `3m41s`, `12s`, `1h02m`.
pub fn fmt_duration(secs: i64) -> String {
    let secs = secs.max(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m{:02}s", secs / 60, secs % 60)
    } else {
        format!("{}h{:02}m", secs / 3600, (secs % 3600) / 60)
    }
}

fn parse_ts(ts: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

fn is_terminal(state: &str) -> bool {
    TaskState::from_label(state).is_some_and(TaskState::is_terminal)
}

fn task_owner(s: &TaskSummary) -> String {
    s.owner.clone().unwrap_or_else(|| MAIN_AGENT.to_string())
}

/// The tasks of `agent`, most recently active first, with the live task
/// included even before the log shows it.
fn agent_tasks(inputs: &TasksInputs, agent: &str) -> Vec<TaskSummary> {
    let mut tasks: Vec<TaskSummary> = task_summaries(&inputs.records)
        .into_iter()
        .filter(|s| task_owner(s) == agent)
        .collect();
    if let Some(live) = inputs.live.as_ref().filter(|l| l.agent == agent) {
        if !tasks.iter().any(|t| t.id == live.id) {
            tasks.insert(
                0,
                TaskSummary {
                    id: live.id.clone(),
                    state: live.state.to_string(),
                    first_ts: String::new(),
                    last_ts: String::new(),
                    task_type: live.task_type.clone(),
                    records: 0,
                    parent: live.parent.clone(),
                    owner: Some(live.agent.clone()),
                    usage: live.usage,
                },
            );
        }
    }
    tasks
}

fn description_of(inputs: &TasksInputs, id: &str) -> Option<String> {
    inputs
        .live
        .as_ref()
        .filter(|l| l.id == id && !l.description.is_empty())
        .map(|l| l.description.clone())
        .or_else(|| inputs.descriptions.get(id).cloned())
}

/// `Queued 0s → Planning 3s → Executing …` from the recorded timestamps.
pub fn compact_timeline(timeline: &[&TransitionRecord]) -> String {
    let mut parts = Vec::new();
    for (i, r) in timeline.iter().enumerate() {
        let mut name = r.to.clone();
        if let Some(first) = name.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        match timeline.get(i + 1) {
            Some(next) => {
                let spent = match (parse_ts(&r.ts), parse_ts(&next.ts)) {
                    (Some(a), Some(b)) => fmt_duration((b - a).num_seconds()),
                    _ => "?".into(),
                };
                if next.event.as_deref() == Some("edit") && r.to == next.to {
                    parts.push(format!("{name} {spent} (edited)"));
                } else if next.from.is_none() {
                    parts.push(format!("{name} {spent} | new segment"));
                } else {
                    parts.push(format!("{name} {spent}"));
                }
            }
            None if is_terminal(&r.to) => parts.push(name),
            None => parts.push(format!("{name} …")),
        }
    }
    // Consecutive "Paused … (edited)" entries collapse visually; keep it
    // readable by dropping repeats of the same label.
    parts.dedup();
    parts.join(" → ")
}

fn state_field(inputs: &TasksInputs, id: &str, summary: Option<&TaskSummary>) -> String {
    if let Some(live) = inputs.live.as_ref().filter(|l| l.id == id) {
        let since = fmt_duration((inputs.now - live.state_since).num_seconds());
        let pending = if live.pause_pending {
            " — pause requested, waiting for the step to finish"
        } else {
            ""
        };
        return if live.state.is_terminal() {
            format!("{}", live.state)
        } else {
            format!("{} (step {}, {since}){pending}", live.state, live.step)
        };
    }
    let Some(s) = summary else {
        return "not recorded".into();
    };
    if is_terminal(&s.state) {
        return format!("{} (at {})", s.state, s.last_ts);
    }
    let since = parse_ts(&s.last_ts)
        .map(|t| fmt_duration((inputs.now - t).num_seconds()))
        .unwrap_or_else(|| "?".into());
    let pid = inputs
        .records
        .iter()
        .rev()
        .find(|r| r.entity == Entity::Task && r.id == id)
        .and_then(|r| r.pid);
    match pid {
        Some(p) if p != inputs.this_pid => {
            format!(
                "{} for {since} (recorded by selfware pid {p}, not this session)",
                s.state
            )
        }
        _ => format!("{} for {since}", s.state),
    }
}

fn resource_label(r: &ResourceRow) -> String {
    let mut out = format!("● {} {} ({}", r.kind, r.handle, r.state);
    if let Some(p) = r.port {
        out.push_str(&format!(", :{p}"));
    }
    out.push(')');
    if !r.label.is_empty() {
        out.push_str(&format!(" {}", clip(&r.label, 40)));
    }
    if r.is_reapable() {
        out.push_str("  [r] reap");
    }
    out
}

/// The rows at `level` and where Enter goes from each.
pub fn items_at(inputs: &TasksInputs, level: &Level) -> Vec<Item> {
    match level {
        Level::Agents => agents_from(inputs)
            .into_iter()
            .map(|a| Item {
                label: format!(
                    "{:<14} {:<5} {} {}  {} task(s): {} done, {} failed, {} stopped  tokens {}  last: {} {} {}  resources {}",
                    a.id,
                    a.agent_type,
                    a.state,
                    a.in_state,
                    a.tasks,
                    a.completed,
                    a.failed,
                    a.stopped,
                    a.tokens,
                    a.last_task
                        .as_deref()
                        .map(short_id)
                        .unwrap_or_else(|| "-".into()),
                    a.last_state.as_deref().unwrap_or("-"),
                    a.last_task_type.as_deref().unwrap_or(""),
                    a.resources
                ),
                target: Some(Level::Tasks { agent: a.id }),
            })
            .collect(),
        Level::Tasks { agent } => agent_tasks(inputs, agent)
            .into_iter()
            .map(|t| {
                let live = inputs.live.as_ref().is_some_and(|l| l.id == t.id);
                let desc = description_of(inputs, &t.id)
                    .map(|d| format!("  \"{}\"", clip(&d, 48)))
                    .unwrap_or_default();
                let fork = t
                    .parent
                    .as_deref()
                    .map(|p| format!("  ↳ fork of {}", short_id(p)))
                    .unwrap_or_default();
                Item {
                    label: format!(
                        "{} {:<13} {:<11} {:<10}{desc}{fork}",
                        if live { "▶" } else { " " },
                        short_id(&t.id),
                        t.state,
                        t.task_type.as_deref().unwrap_or("-"),
                    ),
                    target: Some(Level::Task {
                        agent: agent.clone(),
                        id: t.id,
                    }),
                }
            })
            .collect(),
        Level::Task { agent, id } => {
            let mut items = Vec::new();
            for fork in task_summaries(&inputs.records)
                .into_iter()
                .filter(|s| s.parent.as_deref() == Some(id.as_str()))
            {
                items.push(Item {
                    label: format!("↳ fork {} ({})", short_id(&fork.id), fork.state),
                    target: Some(Level::Task {
                        agent: task_owner(&fork),
                        id: fork.id,
                    }),
                });
            }
            for r in inputs.resources.iter().filter(|r| &r.owner_task == id) {
                items.push(Item {
                    label: resource_label(r),
                    target: Some(Level::Resource {
                        agent: agent.clone(),
                        task: id.clone(),
                        id: r.id.clone(),
                    }),
                });
            }
            for (index, r) in task_timeline(&inputs.records, id).into_iter().enumerate() {
                let transition = match &r.from {
                    Some(from) => format!("{from} → {}", r.to),
                    None => format!("(new) → {}", r.to),
                };
                items.push(Item {
                    label: format!(
                        "{} {:<24} {}",
                        r.ts.get(11..19).unwrap_or(&r.ts),
                        transition,
                        clip(&r.cause, 60)
                    ),
                    target: Some(Level::Record {
                        agent: agent.clone(),
                        task: id.clone(),
                        index,
                    }),
                });
            }
            items
        }
        Level::Resource { .. } | Level::Record { .. } => Vec::new(),
    }
}

fn task_fields(inputs: &TasksInputs, id: &str) -> Vec<(String, String)> {
    let summaries = task_summaries(&inputs.records);
    let summary = summaries.iter().find(|s| s.id == id);
    let live = inputs.live.as_ref().filter(|l| l.id == id);
    let mut fields = Vec::new();
    fields.push(("state".into(), state_field(inputs, id, summary)));
    let task_type = live
        .and_then(|l| l.task_type.clone())
        .or_else(|| summary.and_then(|s| s.task_type.clone()))
        .unwrap_or_else(|| "not recorded".into());
    fields.push(("type".into(), task_type));
    let usage = live.and_then(|l| l.usage).or(summary.and_then(|s| s.usage));
    let (tokens, cost) = usage_line(usage.as_ref());
    fields.push(("tokens".into(), tokens));
    fields.push(("cost".into(), cost));
    if let Some(parent) = live
        .and_then(|l| l.parent.clone())
        .or_else(|| summary.and_then(|s| s.parent.clone()))
    {
        fields.push(("forked from".into(), parent));
    }
    if let Some(l) = live {
        let c = &l.constraints;
        fields.push((
            "limits".into(),
            format!(
                "turns {}/{} · token budget {} · allowed paths {} (not editable mid-task)",
                c.turns_used,
                c.max_turns,
                c.token_budget.map_or("unbounded".into(), |b| b.to_string()),
                if c.allowed_paths.is_empty() {
                    "none".into()
                } else {
                    c.allowed_paths.join(", ")
                }
            ),
        ));
    }
    let owned: Vec<&ResourceRow> = inputs
        .resources
        .iter()
        .filter(|r| r.owner_task == id)
        .collect();
    let unreleased = owned.iter().filter(|r| r.state != "released").count();
    fields.push((
        "resources".into(),
        if owned.is_empty() {
            "none recorded".into()
        } else {
            format!("{} recorded, {unreleased} not released", owned.len())
        },
    ));
    let timeline = task_timeline(&inputs.records, id);
    fields.push((
        "timeline".into(),
        if timeline.is_empty() {
            "no transitions recorded".into()
        } else {
            compact_timeline(&timeline)
        },
    ));
    fields.push((
        "description".into(),
        description_of(inputs, id).unwrap_or_else(|| "not recorded".into()),
    ));
    fields
}

/// The pure view: breadcrumb, fields and rows for the current frame.
pub fn build_view(inputs: &TasksInputs, nav: &Nav) -> TasksView {
    let level = &nav.current.level;
    let mut breadcrumb = vec!["Session".to_string(), "Agents".to_string()];
    let (title, fields) = match level {
        Level::Agents => ("Agents".to_string(), Vec::new()),
        Level::Tasks { agent } => {
            breadcrumb.push(agent.clone());
            (format!("Tasks of {agent}"), Vec::new())
        }
        Level::Task { agent, id } => {
            breadcrumb.push(agent.clone());
            breadcrumb.push(format!("Task {}", short_id(id)));
            let desc = description_of(inputs, id)
                .map(|d| format!("  \"{}\"", clip(&d, 60)))
                .unwrap_or_default();
            (format!("Task {id}{desc}"), task_fields(inputs, id))
        }
        Level::Resource { agent, task, id } => {
            breadcrumb.push(agent.clone());
            breadcrumb.push(format!("Task {}", short_id(task)));
            let row = inputs.resources.iter().find(|r| &r.id == id);
            breadcrumb.push(
                row.map(|r| format!("{} {}", r.kind, r.handle))
                    .unwrap_or_else(|| id.clone()),
            );
            let fields = match row {
                Some(r) => vec![
                    ("id".into(), r.id.clone()),
                    ("kind".into(), r.kind.clone()),
                    ("state".into(), r.state.clone()),
                    ("handle".into(), r.handle.clone()),
                    (
                        "port".into(),
                        r.port.map_or("none".into(), |p| p.to_string()),
                    ),
                    ("owner task".into(), r.owner_task.clone()),
                    (
                        "label".into(),
                        if r.label.is_empty() {
                            "-".into()
                        } else {
                            r.label.clone()
                        },
                    ),
                    ("note".into(), r.note.clone().unwrap_or_else(|| "-".into())),
                    (
                        "reap".into(),
                        if r.is_reapable() {
                            "left behind — [r] drains it".into()
                        } else if !r.drainable {
                            format!("{} resources are not reaped automatically", r.kind)
                        } else {
                            "not a reap candidate (owned by a running task or released)".into()
                        },
                    ),
                ],
                None => vec![("state".into(), "no longer in the registry".into())],
            };
            ("Resource".to_string(), fields)
        }
        Level::Record { agent, task, index } => {
            breadcrumb.push(agent.clone());
            breadcrumb.push(format!("Task {}", short_id(task)));
            breadcrumb.push(format!("transition {}", index + 1));
            let timeline = task_timeline(&inputs.records, task);
            let fields = match timeline.get(*index) {
                Some(r) => {
                    let mut f = vec![
                        ("time".into(), r.ts.clone()),
                        (
                            "transition".into(),
                            format!("{} → {}", r.from.as_deref().unwrap_or("(new)"), r.to),
                        ),
                        (
                            "event".into(),
                            r.event.clone().unwrap_or_else(|| "(created)".into()),
                        ),
                        ("cause".into(), r.cause.clone()),
                        (
                            "process".into(),
                            r.pid.map_or("not recorded".into(), |p| format!("pid {p}")),
                        ),
                    ];
                    if let Some(u) = &r.usage {
                        let (tokens, cost) = usage_line(Some(u));
                        f.push(("tokens".into(), tokens));
                        f.push(("cost".into(), cost));
                    }
                    f
                }
                None => vec![("record".into(), "no longer in the log".into())],
            };
            ("Recorded transition".to_string(), fields)
        }
    };
    let items = items_at(inputs, level);
    let selected = nav.current.selected.min(items.len().saturating_sub(1));
    TasksView {
        breadcrumb,
        title,
        fields,
        items,
        selected,
        scroll: nav.current.scroll,
        keys: KEYS.to_string(),
    }
}

// ---- editor -----------------------------------------------------------------

/// What an edit applies to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditTarget {
    /// A task running in this process: pause → edit → resume.
    Live { id: String },
    /// A finished task: the edit becomes a new task forked from it.
    Fork { parent: String },
}

/// The inline editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskEditor {
    pub target: EditTarget,
    /// (label, value, editable)
    pub fields: Vec<(String, String, bool)>,
    pub focus: usize,
    /// Why the last save was refused.
    pub error: Option<String>,
}

impl TaskEditor {
    fn for_live(live: &LiveTask) -> Self {
        Self {
            target: EditTarget::Live {
                id: live.id.clone(),
            },
            fields: vec![
                ("description".into(), live.description.clone(), true),
                (
                    "max turns".into(),
                    live.constraints.max_turns.to_string(),
                    true,
                ),
                (
                    "token budget".into(),
                    live.constraints
                        .token_budget
                        .map(|b| b.to_string())
                        .unwrap_or_default(),
                    true,
                ),
                (
                    "allowed paths".into(),
                    live.constraints.allowed_paths.join(", "),
                    false,
                ),
            ],
            focus: 0,
            error: None,
        }
    }

    fn for_fork(parent: &str, description: String) -> Self {
        Self {
            target: EditTarget::Fork {
                parent: parent.to_string(),
            },
            fields: vec![("description".into(), description, true)],
            focus: 0,
            error: None,
        }
    }

    fn value(&self, label: &str) -> &str {
        self.fields
            .iter()
            .find(|(l, _, _)| l == label)
            .map(|(_, v, _)| v.as_str())
            .unwrap_or("")
    }

    /// The edit the fields describe, or why they do not parse.
    pub fn to_edit(&self) -> Result<TaskEdit, String> {
        let max_turns = self
            .value("max turns")
            .trim()
            .parse::<usize>()
            .map_err(|_| "max turns must be a whole number".to_string())?;
        let budget = self.value("token budget").trim();
        let token_budget = if budget.is_empty() {
            None
        } else {
            Some(
                budget
                    .parse::<usize>()
                    .map_err(|_| "token budget must be a whole number or empty".to_string())?,
            )
        };
        Ok(TaskEdit {
            description: self.value("description").trim().to_string(),
            max_turns,
            token_budget,
        })
    }

    fn move_focus(&mut self, forward: bool) {
        let n = self.fields.len();
        for step in 1..=n {
            let i = if forward {
                (self.focus + step) % n
            } else {
                (self.focus + n - step) % n
            };
            if self.fields[i].2 {
                self.focus = i;
                return;
            }
        }
    }
}

// ---- the pane ---------------------------------------------------------------

/// Work the event loop does on the pane's behalf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaneAction {
    /// Send this text to the agent as the next task (a queued fork).
    SubmitTask(String),
    /// Drain these resources (ids) through the teardown engine.
    Reap(Vec<String>),
    /// Close the pane.
    Close,
}

/// Navigation, editor and status of the pane.
#[derive(Debug, Clone, Default)]
pub struct TasksPane {
    pub nav: Nav,
    pub editor: Option<TaskEditor>,
    /// One-line outcome of the last action.
    pub status: Option<String>,
    /// Task id armed for cancel (a second `x` confirms).
    cancel_armed: Option<String>,
}

impl TasksPane {
    /// The task the current frame is about: the task level itself, or the
    /// selected row in a task list.
    fn focused_task(&self, inputs: &TasksInputs) -> Option<String> {
        match &self.nav.current.level {
            Level::Task { id, .. } => Some(id.clone()),
            Level::Resource { task, .. } | Level::Record { task, .. } => Some(task.clone()),
            Level::Tasks { .. } => items_at(inputs, &self.nav.current.level)
                .get(self.nav.current.selected)
                .and_then(|i| match &i.target {
                    Some(Level::Task { id, .. }) => Some(id.clone()),
                    _ => None,
                }),
            Level::Agents => None,
        }
    }

    fn task_state(inputs: &TasksInputs, id: &str) -> Option<String> {
        if let Some(l) = inputs.live.as_ref().filter(|l| l.id == id) {
            return Some(l.state.to_string());
        }
        task_summaries(&inputs.records)
            .into_iter()
            .find(|s| s.id == id)
            .map(|s| s.state)
    }

    fn not_here(id: &str) -> String {
        format!(
            "task {} is not running in this session — controlling another process's task \
             is not supported yet",
            short_id(id)
        )
    }

    /// Handle one key. Returns an action for the event loop, if any.
    pub fn on_key(
        &mut self,
        key: KeyEvent,
        inputs: &TasksInputs,
        control: &TaskControl,
    ) -> Option<PaneAction> {
        if self.editor.is_some() {
            return self.on_editor_key(key, control);
        }
        if key.code != KeyCode::Char('x') {
            self.cancel_armed = None;
        }
        let count = items_at(inputs, &self.nav.current.level).len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.nav.current.selected = self.nav.current.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.nav.current.selected + 1 < count {
                    self.nav.current.selected += 1;
                }
            }
            KeyCode::PageUp => {
                self.nav.current.scroll = self.nav.current.scroll.saturating_sub(5);
            }
            KeyCode::PageDown => self.nav.current.scroll += 5,
            KeyCode::Enter => {
                let items = items_at(inputs, &self.nav.current.level);
                if let Some(target) = items
                    .get(self.nav.current.selected)
                    .and_then(|i| i.target.clone())
                {
                    self.nav.push(target);
                    self.status = None;
                }
            }
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Left => {
                if !self.nav.pop() {
                    return Some(PaneAction::Close);
                }
                self.status = None;
            }
            KeyCode::Char('e') => self.start_edit(inputs, control),
            KeyCode::Char('p') => self.toggle_pause(inputs, control),
            KeyCode::Char('x') => self.cancel(inputs, control),
            KeyCode::Char('r') => return self.reap(inputs),
            _ => {}
        }
        None
    }

    fn start_edit(&mut self, inputs: &TasksInputs, control: &TaskControl) {
        let Some(id) = self.focused_task(inputs) else {
            self.status = Some("select a task to edit".into());
            return;
        };
        let state = Self::task_state(inputs, &id).unwrap_or_default();
        if is_terminal(&state) {
            let description = description_of(inputs, &id).unwrap_or_default();
            self.editor = Some(TaskEditor::for_fork(&id, description));
            self.status = Some(format!(
                "task {} is {state}: saving creates a new task forked from it (history is kept)",
                short_id(&id)
            ));
            return;
        }
        match inputs.live.as_ref().filter(|l| l.id == id) {
            Some(live) => match control.request_pause(&id) {
                Ok(()) => {
                    self.editor = Some(TaskEditor::for_live(live));
                    self.status = Some(
                        "pausing at the next step boundary while you edit — Enter saves and \
                         resumes, Esc discards and resumes"
                            .into(),
                    );
                }
                Err(e) => self.status = Some(e.to_string()),
            },
            None => self.status = Some(Self::not_here(&id)),
        }
    }

    fn on_editor_key(&mut self, key: KeyEvent, control: &TaskControl) -> Option<PaneAction> {
        let editor = self.editor.as_mut()?;
        match key.code {
            KeyCode::Esc => {
                let target = editor.target.clone();
                self.editor = None;
                if let EditTarget::Live { id } = target {
                    // Nothing changed: withdraw the pause (or resume).
                    let _ = control.request_resume(&id);
                    self.status = Some("edit discarded; task resumes".into());
                } else {
                    self.status = Some("fork discarded".into());
                }
            }
            KeyCode::Tab | KeyCode::Down => editor.move_focus(true),
            KeyCode::BackTab | KeyCode::Up => editor.move_focus(false),
            KeyCode::Backspace => {
                let f = editor.focus;
                editor.fields[f].1.pop();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let f = editor.focus;
                if editor.fields[f].2 {
                    editor.fields[f].1.push(c);
                }
            }
            KeyCode::Enter => return self.save_edit(control),
            _ => {}
        }
        None
    }

    fn save_edit(&mut self, control: &TaskControl) -> Option<PaneAction> {
        let editor = self.editor.as_mut()?;
        match editor.target.clone() {
            EditTarget::Live { id } => {
                let edit = match editor.to_edit() {
                    Ok(e) => e,
                    Err(e) => {
                        editor.error = Some(e);
                        return None;
                    }
                };
                match control.submit_edit(&id, edit) {
                    Ok(()) => {
                        self.editor = None;
                        self.status = Some(
                            "edit submitted: applied at the step boundary, sent to the agent \
                             as \"Task updated: …\", then the task resumes"
                                .into(),
                        );
                    }
                    Err(e) => editor.error = Some(e.to_string()),
                }
                None
            }
            EditTarget::Fork { parent } => {
                let description = editor.value("description").trim().to_string();
                if description.is_empty() {
                    editor.error = Some("the task description cannot be empty".into());
                    return None;
                }
                control.queue_fork(&parent, &description);
                self.editor = None;
                self.status = Some(format!(
                    "fork of {} sent to the agent; it runs as a new task after any running one",
                    short_id(&parent)
                ));
                Some(PaneAction::SubmitTask(description))
            }
        }
    }

    fn toggle_pause(&mut self, inputs: &TasksInputs, control: &TaskControl) {
        let Some(id) = self.focused_task(inputs) else {
            self.status = Some("select a task to pause".into());
            return;
        };
        let Some(live) = inputs.live.as_ref().filter(|l| l.id == id) else {
            let state = Self::task_state(inputs, &id).unwrap_or_default();
            self.status = Some(if is_terminal(&state) {
                format!("task {} already finished ({state})", short_id(&id))
            } else {
                Self::not_here(&id)
            });
            return;
        };
        let result = if live.state == TaskState::Paused || live.pause_pending {
            control
                .request_resume(&id)
                .map(|()| "resume requested".to_string())
        } else {
            control
                .request_pause(&id)
                .map(|()| "pause requested: the task stops at the next step boundary".to_string())
        };
        self.status = Some(result.unwrap_or_else(|e| e.to_string()));
    }

    fn cancel(&mut self, inputs: &TasksInputs, control: &TaskControl) {
        let Some(id) = self.focused_task(inputs) else {
            self.status = Some("select a task to cancel".into());
            return;
        };
        if inputs.live.as_ref().filter(|l| l.id == id).is_none() {
            let state = Self::task_state(inputs, &id).unwrap_or_default();
            self.status = Some(if is_terminal(&state) {
                format!("task {} already finished ({state})", short_id(&id))
            } else {
                Self::not_here(&id)
            });
            return;
        }
        if self.cancel_armed.as_deref() != Some(id.as_str()) {
            self.cancel_armed = Some(id.clone());
            self.status = Some(format!(
                "press x again to cancel task {} (it ends cancelled and is not resumable)",
                short_id(&id)
            ));
            return;
        }
        self.cancel_armed = None;
        self.status = Some(match control.request_cancel(&id) {
            Ok(()) => "cancel requested: a running model call is aborted, a running tool call \
                       finishes first"
                .into(),
            Err(e) => e.to_string(),
        });
    }

    fn reap(&mut self, inputs: &TasksInputs) -> Option<PaneAction> {
        let ids: Vec<String> = match &self.nav.current.level {
            Level::Resource { id, .. } => inputs
                .resources
                .iter()
                .filter(|r| &r.id == id && r.is_reapable())
                .map(|r| r.id.clone())
                .collect(),
            Level::Task { id, .. } => inputs
                .resources
                .iter()
                .filter(|r| &r.owner_task == id && r.is_reapable())
                .map(|r| r.id.clone())
                .collect(),
            _ => {
                self.status = Some("open a task or a resource to reap what it left behind".into());
                return None;
            }
        };
        if ids.is_empty() {
            self.status = Some(
                "nothing to reap here (only leaked or orphaned containers, processes, ptys and \
                 browsers are); `selfware resources reap` reconciles everything"
                    .into(),
            );
            return None;
        }
        self.status = Some(format!("reaping {} resource(s)…", ids.len()));
        Some(PaneAction::Reap(ids))
    }
}

// ---- rendering ----------------------------------------------------------------

/// Draw `view` (and the editor, if open) into `area`.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    view: &TasksView,
    editor: Option<&TaskEditor>,
    status: Option<&str>,
) {
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(TuiPalette::border_style())
        .title(Span::styled(" Tasks ", TuiPalette::title_style()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let field_rows = if view.fields.is_empty() {
        0
    } else {
        view.fields.len() as u16 + 1
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),          // breadcrumb
            Constraint::Length(1),          // title
            Constraint::Length(field_rows), // fields
            Constraint::Min(1),             // items
            Constraint::Length(1),          // status
            Constraint::Length(1),          // keys
        ])
        .split(inner);

    let crumb = view.breadcrumb.join(" › ");
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            crumb,
            Style::default()
                .fg(TuiPalette::AMBER)
                .add_modifier(Modifier::BOLD),
        ))),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            view.title.clone(),
            TuiPalette::title_style(),
        ))),
        chunks[1],
    );
    if !view.fields.is_empty() {
        let lines: Vec<Line> = view
            .fields
            .iter()
            .map(|(k, v)| {
                Line::from(vec![
                    Span::styled(format!(" {k:<12}"), TuiPalette::muted_style()),
                    Span::styled(v.clone(), Style::default().fg(TuiPalette::PARCHMENT)),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), chunks[2]);
    }

    let items: Vec<ListItem> = if view.items.is_empty() {
        vec![ListItem::new(Span::styled(
            match view.fields.is_empty() {
                true => " (nothing recorded yet)",
                false => "",
            },
            TuiPalette::muted_style(),
        ))]
    } else {
        view.items
            .iter()
            .map(|i| ListItem::new(Span::raw(i.label.clone())))
            .collect()
    };
    let list = List::new(items).highlight_style(
        Style::default()
            .fg(TuiPalette::INK)
            .bg(TuiPalette::AMBER)
            .add_modifier(Modifier::BOLD),
    );
    let mut state = ListState::default().with_offset(view.scroll);
    if !view.items.is_empty() {
        state.select(Some(view.selected));
    }
    frame.render_stateful_widget(list, chunks[3], &mut state);

    frame.render_widget(
        Paragraph::new(Span::styled(
            status.unwrap_or("").to_string(),
            Style::default().fg(TuiPalette::COPPER),
        )),
        chunks[4],
    );
    frame.render_widget(
        Paragraph::new(Span::styled(view.keys.clone(), TuiPalette::muted_style())),
        chunks[5],
    );

    if let Some(editor) = editor {
        render_editor(frame, inner, editor);
    }
}

fn render_editor(frame: &mut Frame, area: Rect, editor: &TaskEditor) {
    let width = area.width.saturating_sub(4).min(100);
    let height = (editor.fields.len() as u16 * 2 + 5).min(area.height);
    let rect = Rect::new(
        area.x + (area.width.saturating_sub(width)) / 2,
        area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rect);
    let title = match &editor.target {
        EditTarget::Live { id } => format!(" Edit task {} ", short_id(id)),
        EditTarget::Fork { parent } => format!(" Fork task {} ", short_id(parent)),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(TuiPalette::AMBER))
        .title(Span::styled(title, TuiPalette::title_style()));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let mut lines = Vec::new();
    for (i, (label, value, editable)) in editor.fields.iter().enumerate() {
        let focused = i == editor.focus;
        let marker = if focused { "▶ " } else { "  " };
        let suffix = if *editable { "" } else { "  (read-only)" };
        lines.push(Line::from(Span::styled(
            format!("{marker}{label}{suffix}"),
            if focused {
                Style::default()
                    .fg(TuiPalette::AMBER)
                    .add_modifier(Modifier::BOLD)
            } else {
                TuiPalette::muted_style()
            },
        )));
        let shown = if value.is_empty() && label == "token budget" {
            "(empty = unbounded)".to_string()
        } else {
            value.clone()
        };
        lines.push(Line::from(Span::styled(
            format!("    {shown}{}", if focused { "▏" } else { "" }),
            Style::default().fg(TuiPalette::PARCHMENT),
        )));
    }
    lines.push(Line::from(Span::styled(
        editor
            .error
            .clone()
            .unwrap_or_else(|| "[Enter] save  [Tab] next field  [Esc] discard".into()),
        if editor.error.is_some() {
            Style::default().fg(TuiPalette::RUST)
        } else {
            TuiPalette::muted_style()
        },
    )));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

#[cfg(test)]
#[path = "../../../tests/unit/ui/tui/tasks_view/tasks_view_test.rs"]
mod tests;
