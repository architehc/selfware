//! Read-side projections of the event log (`selfware tasks`,
//! `selfware task show`, `selfware agents`). Everything shown is recorded
//! data: states and timestamps come from the log, durations are differences
//! of recorded timestamps, a task type appears only when one was recorded,
//! and token totals are the measured `usage` written on a task's terminal
//! record — anything not recorded is shown as `not recorded`.

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
    /// The task this one was forked from, if recorded.
    pub parent: Option<String>,
    /// The owning agent, if recorded.
    pub owner: Option<String>,
    /// The last recorded usage, if any record carried one.
    pub usage: Option<super::RecordedUsage>,
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
            parent: None,
            owner: None,
            usage: None,
        });
        // Records are in append order; the last one wins.
        entry.state = r.to.clone();
        entry.last_ts = r.ts.clone();
        entry.records += 1;
        if r.task_type.is_some() {
            entry.task_type = r.task_type.clone();
        }
        if r.parent.is_some() {
            entry.parent = r.parent.clone();
        }
        if r.owner.is_some() {
            entry.owner = r.owner.clone();
        }
        if r.usage.is_some() {
            entry.usage = r.usage;
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

/// `selfware tasks --tree`: forks under the task they were forked from
/// (most recently active roots first). A fork whose parent is not in the
/// log is shown as a root.
pub fn render_task_tree(summaries: &[TaskSummary], limit: usize) -> String {
    let ids: std::collections::HashSet<&str> = summaries.iter().map(|s| s.id.as_str()).collect();
    let is_root = |s: &TaskSummary| s.parent.as_deref().is_none_or(|p| !ids.contains(p));
    let mut out = String::new();
    fn walk(
        out: &mut String,
        all: &[TaskSummary],
        s: &TaskSummary,
        depth: usize,
        seen: &mut std::collections::HashSet<String>,
    ) {
        if !seen.insert(s.id.clone()) {
            return; // a malformed log with a parent cycle
        }
        out.push_str(&format!(
            "{}{}{}  {}  {}\n",
            "  ".repeat(depth),
            if depth > 0 { "└─ " } else { "" },
            s.id,
            s.state,
            s.task_type.as_deref().unwrap_or("-")
        ));
        for child in all.iter().filter(|c| c.parent.as_deref() == Some(&s.id)) {
            walk(out, all, child, depth + 1, seen);
        }
    }
    let mut seen = std::collections::HashSet::new();
    let roots: Vec<&TaskSummary> = summaries.iter().filter(|s| is_root(s)).collect();
    for root in roots.iter().take(limit) {
        walk(&mut out, summaries, root, 0, &mut seen);
    }
    if roots.len() > limit {
        out.push_str(&format!(
            "({} more; use --limit to show them)\n",
            roots.len() - limit
        ));
    }
    out
}

/// One line of measured usage: `212000 tokens (main 177000 · side 35000)`,
/// and the provider-reported cost or `cost not reported`.
pub fn usage_line(usage: Option<&super::RecordedUsage>) -> (String, String) {
    let Some(u) = usage else {
        return ("not recorded".into(), "not reported".into());
    };
    let tokens = match (u.main_tokens, u.side_tokens) {
        (Some(m), Some(sd)) => format!("{} (main {} · side {})", u.total_tokens, m, sd),
        _ => format!("{} (main/side split not measured)", u.total_tokens),
    };
    let cost = match (u.cost_usd, u.cost_complete) {
        (Some(c), true) => format!("${c:.4}"),
        (Some(c), false) => format!("≥ ${c:.4} (some calls reported no cost)"),
        (None, _) => "not reported".into(),
    };
    (tokens, cost)
}

fn parse_ts(ts: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_rfc3339(ts).ok()
}

/// Total time the task spent paused, measured from the recorded
/// timestamps: each `pause` record up to the next record that leaves
/// `paused` (resume, cancel, timeout, …). An edit keeps the task paused and
/// does not close the interval; a pause still open is not counted (it shows
/// as the current state's time). `None` when no pause was closed.
pub fn paused_total_ms(timeline: &[&TransitionRecord]) -> Option<i64> {
    let paused = "paused";
    let mut since: Option<chrono::DateTime<chrono::FixedOffset>> = None;
    let mut total: i64 = 0;
    let mut closed = false;
    for r in timeline {
        let ts = parse_ts(&r.ts);
        if r.to == paused && r.from.as_deref() != Some(paused) {
            since = ts;
        } else if r.from.as_deref() == Some(paused) && r.to != paused {
            if let (Some(start), Some(end)) = (since.take(), ts) {
                total += (end - start).num_milliseconds().max(0);
                closed = true;
            }
        }
    }
    closed.then_some(total)
}

/// `12s`, `3m41s`, `1h02m` for a measured duration in milliseconds.
pub fn fmt_paused(ms: i64) -> String {
    let secs = (ms.max(0) + 500) / 1000;
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m{:02}s", secs / 60, secs % 60)
    } else {
        format!("{}h{:02}m", secs / 3600, (secs % 3600) / 60)
    }
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
    if let Some(p) = timeline.iter().find_map(|r| r.parent.as_deref()) {
        out.push_str(&format!("  forked from: {p}\n"));
    }
    if let Some(u) = timeline.iter().rev().find_map(|r| r.usage.as_ref()) {
        let (tokens, cost) = usage_line(Some(u));
        out.push_str(&format!("  tokens: {tokens}\n  cost: {cost}\n"));
    }
    if let Some(ms) = paused_total_ms(timeline).filter(|&ms| ms > 0) {
        out.push_str(&format!(
            "  paused: {} (not counted against the wall-clock budget)\n",
            fmt_paused(ms)
        ));
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

/// The resource transitions recorded with `owner` = task `id` (what the
/// task spawned and how its teardown went), or an empty string.
pub fn render_task_resources(records: &[TransitionRecord], id: &str) -> String {
    let owned: Vec<&TransitionRecord> = records
        .iter()
        .filter(|r| r.entity == Entity::Resource && r.owner.as_deref() == Some(id))
        .collect();
    if owned.is_empty() {
        return String::new();
    }
    let mut out = String::from("\nResources owned\n");
    for r in owned {
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
            "  {:<24} {:<18} {:<22} {}{}\n",
            r.ts, r.id, transition, event, r.cause
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

/// Output of `selfware tasks [--limit N] [--tree]`.
pub fn tasks_command_output(log: &super::EventLog, limit: usize, tree: bool) -> String {
    let (records, skipped) = log.read_all();
    let summaries = task_summaries(&records);
    let mut out = String::new();
    if summaries.is_empty() {
        out.push_str(&format!(
            "No task transitions recorded in {}.\n",
            log_location(log)
        ));
    } else {
        if tree {
            out.push_str(&render_task_tree(&summaries, limit));
        } else {
            out.push_str(&render_task_list(&summaries, limit));
        }
        out.push_str(&format!("(from {})\n", log_location(log)));
    }
    out.push_str(&skipped_note(skipped));
    out
}

/// Output of `selfware task show <id>` (a full id or a unique prefix), or
/// the message to print when it does not resolve.
pub fn task_show_output(log: &super::EventLog, query: &str) -> Result<String, String> {
    task_show_output_with(log, query, &|_| None)
}

/// [`task_show_output`] with the task's description from the checkpoint
/// journal: `description_of(id)` returns the description the task holds
/// now and, when it was edited mid-run, the one it was started with. The
/// event log itself records no description (only an edit's cause).
pub fn task_show_output_with(
    log: &super::EventLog,
    query: &str,
    description_of: &dyn Fn(&str) -> Option<(String, Option<String>)>,
) -> Result<String, String> {
    let (records, skipped) = log.read_all();
    match resolve_task_id(&records, query) {
        Ok(id) => {
            let timeline = task_timeline(&records, &id);
            let mut out = render_timeline(&id, &timeline);
            if let Some((description, original)) = description_of(&id) {
                let note = original
                    .filter(|o| o.trim() != description.trim())
                    .map(|o| format!("\n  (edited mid-run; started as: {o})"))
                    .unwrap_or_default();
                // After the header, before the transitions.
                let at = out.find("\n\n").map_or(out.len(), |i| i + 1);
                out.insert_str(at, &format!("  description: {description}{note}\n"));
            }
            let forks: Vec<String> = task_summaries(&records)
                .into_iter()
                .filter(|s| s.parent.as_deref() == Some(id.as_str()))
                .map(|s| s.id)
                .collect();
            if !forks.is_empty() {
                out.push_str(&format!("  forks: {}\n", forks.join(", ")));
            }
            out.push_str(&render_task_resources(&records, &id));
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

// ---------------------------------------------------------------------------
// Agents (`selfware agents`)
// ---------------------------------------------------------------------------

/// One agent, as the log last saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSummary {
    /// The agent id (`main-<8 hex>`).
    pub id: String,
    /// The recorded agent type, if any.
    pub agent_type: Option<String>,
    /// The last recorded agent state (`not recorded` when only its tasks
    /// name it).
    pub state: String,
    /// When that state was entered (the last agent record), if recorded.
    pub since: Option<String>,
    /// The process that recorded the last agent record.
    pub pid: Option<u32>,
    /// Tasks whose last segment by this agent ended `completed`.
    pub tasks_completed: usize,
    /// Tasks whose last segment by this agent ended `failed`.
    pub tasks_failed: usize,
    /// Tasks that ended `interrupted` or `cancelled`.
    pub tasks_stopped: usize,
    /// Sum of the measured `usage.total_tokens` on those tasks' last
    /// terminal records; `None` when none carried usage.
    pub tokens: Option<u64>,
    /// Ended tasks whose terminal record carries no token total.
    pub tasks_without_tokens: usize,
    /// The task this agent most recently worked on.
    pub last_task: Option<String>,
    /// That task's recorded type.
    pub last_task_type: Option<String>,
    /// That task's last recorded state.
    pub last_task_state: Option<String>,
    /// Every task recorded with this agent as owner, sorted.
    pub task_ids: Vec<String>,
}

#[derive(Default)]
struct AgentTask {
    last_state: String,
    last_ts: String,
    /// `Some(usage total)` once a terminal record was seen: the inner
    /// `None` = that record carried no usage.
    terminal_tokens: Option<Option<u64>>,
    task_type: Option<String>,
}

/// Every agent in `records` (agent records, and the owners of task
/// records), most recently active first.
pub fn agent_summaries(records: &[TransitionRecord]) -> Vec<AgentSummary> {
    let mut agents: HashMap<String, AgentSummary> = HashMap::new();
    let blank = |id: &str| AgentSummary {
        id: id.to_string(),
        agent_type: None,
        state: "not recorded".to_string(),
        since: None,
        pid: None,
        tasks_completed: 0,
        tasks_failed: 0,
        tasks_stopped: 0,
        tokens: None,
        tasks_without_tokens: 0,
        last_task: None,
        last_task_type: None,
        last_task_state: None,
        task_ids: Vec::new(),
    };
    // (agent, task) -> what the agent's last segment of that task recorded.
    let mut tasks: HashMap<(String, String), AgentTask> = HashMap::new();
    let mut task_types: HashMap<&str, String> = HashMap::new();
    for r in records {
        match r.entity {
            Entity::Agent => {
                let a = agents.entry(r.id.clone()).or_insert_with(|| blank(&r.id));
                a.state = r.to.clone();
                a.since = Some(r.ts.clone());
                a.pid = r.pid;
                if r.agent_type.is_some() {
                    a.agent_type = r.agent_type.clone();
                }
            }
            Entity::Task => {
                if let Some(t) = &r.task_type {
                    task_types.insert(&r.id, t.clone());
                }
                // Tasks recorded before agents were (no owner) belong to the
                // main agent of their process, listed as `main`.
                let agent = r
                    .owner
                    .clone()
                    .unwrap_or_else(|| super::control::MAIN_AGENT.to_string());
                let agent = &agent;
                agents.entry(agent.clone()).or_insert_with(|| blank(agent));
                let t = tasks.entry((agent.clone(), r.id.clone())).or_default();
                t.last_state = r.to.clone();
                t.last_ts = r.ts.clone();
                if r.task_type.is_some() {
                    t.task_type = r.task_type.clone();
                }
                if super::TaskState::from_label(&r.to).is_some_and(|s| s.is_terminal()) {
                    t.terminal_tokens = Some(r.usage.map(|u| u.total_tokens as u64));
                }
            }
            Entity::Resource => {}
        }
    }
    let mut latest: HashMap<String, (String, String)> = HashMap::new();
    for ((agent, task), t) in &tasks {
        let a = agents.get_mut(agent).expect("inserted above");
        a.task_ids.push(task.clone());
        match t.last_state.as_str() {
            "completed" => a.tasks_completed += 1,
            "failed" => a.tasks_failed += 1,
            "interrupted" | "cancelled" => a.tasks_stopped += 1,
            _ => {}
        }
        if let Some(tokens) = t.terminal_tokens {
            match tokens {
                Some(n) => a.tokens = Some(a.tokens.unwrap_or(0) + n),
                None => a.tasks_without_tokens += 1,
            }
        }
        let newer = latest
            .get(agent)
            .is_none_or(|(ts, id)| (&t.last_ts, task) > (ts, id));
        if newer {
            latest.insert(agent.clone(), (t.last_ts.clone(), task.clone()));
        }
    }
    for (agent, (_, task)) in latest {
        let a = agents.get_mut(&agent).expect("inserted above");
        let t = tasks.get(&(agent.clone(), task.clone()));
        a.last_task_type = t
            .and_then(|t| t.task_type.clone())
            .or_else(|| task_types.get(task.as_str()).cloned());
        a.last_task_state = t.map(|t| t.last_state.clone());
        a.last_task = Some(task);
    }
    let mut out: Vec<AgentSummary> = agents.into_values().collect();
    for a in &mut out {
        a.task_ids.sort();
    }
    let activity = |a: &AgentSummary| -> String {
        let task_ts = tasks
            .iter()
            .filter(|((agent, _), _)| *agent == a.id)
            .map(|(_, t)| t.last_ts.clone())
            .max()
            .unwrap_or_default();
        a.since.clone().unwrap_or_default().max(task_ts)
    };
    out.sort_by(|a, b| activity(b).cmp(&activity(a)).then_with(|| a.id.cmp(&b.id)));
    out
}

fn fmt_age(ms: i64) -> String {
    let secs = ms.max(0) / 1000;
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m{:02}s", secs / 60, secs % 60)
    } else if secs < 86_400 {
        format!("{}h{:02}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d{:02}h", secs / 86_400, (secs % 86_400) / 3600)
    }
}

/// How one agent reads in a listing (the CLI table and the TUI Tasks pane
/// share it): measured values only, `not recorded` otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDisplay {
    /// The recorded state, with `(process ended)` when the recording process
    /// is gone while the state is still a live one.
    pub state: String,
    /// Time since the state was entered (`now` − the recorded timestamp),
    /// `-` when not applicable.
    pub in_state: String,
    /// Recorded token total, with how many ended tasks carried none.
    pub tokens: String,
    /// `<last task> (<type>)`.
    pub last: String,
}

/// Format `a` for display at `now`; `process_alive` says whether a recording
/// pid still runs.
pub fn agent_display(
    a: &AgentSummary,
    now: chrono::DateTime<chrono::Utc>,
    process_alive: &dyn Fn(u32) -> bool,
) -> AgentDisplay {
    let live = !matches!(a.state.as_str(), "stopped" | "not recorded");
    let ended = live && a.pid.is_some_and(|pid| !process_alive(pid));
    let state = if ended {
        format!("{} (process ended)", a.state)
    } else {
        a.state.clone()
    };
    let in_state = match (&a.since, ended) {
        (Some(ts), false) => parse_ts(ts).map_or_else(
            || "not recorded".to_string(),
            |t| fmt_age((now - t.with_timezone(&chrono::Utc)).num_milliseconds()),
        ),
        _ => "-".to_string(),
    };
    let tokens = match (a.tokens, a.tasks_without_tokens) {
        (Some(n), 0) => n.to_string(),
        (Some(n), k) => format!("{n} (+{k} not recorded)"),
        (None, _) => "not recorded".to_string(),
    };
    let last = match (&a.last_task, &a.last_task_type) {
        (Some(t), Some(ty)) => format!("{t} ({ty})"),
        (Some(t), None) => format!("{t} (type not recorded)"),
        (None, _) => "-".to_string(),
    };
    AgentDisplay {
        state,
        in_state,
        tokens,
        last,
    }
}

/// How many of `unreleased` — `(owner task, owner agent)` of each
/// unreleased registry entry — agent `a` holds: attributed to it directly,
/// or owned by one of its tasks.
pub fn resources_held<'a>(
    a: &AgentSummary,
    unreleased: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
) -> usize {
    unreleased
        .into_iter()
        .filter(|(task, agent)| {
            *agent == Some(a.id.as_str()) || a.task_ids.iter().any(|t| t == task)
        })
        .count()
}

/// The `selfware agents` table for the `limit` most recently active agents.
/// `held` maps an agent id to the unreleased resources the registry
/// attributes to it; `process_alive` says whether a recording pid still
/// runs (a live state recorded by an ended process is not shown as live);
/// time in state is `now` minus the recorded timestamp.
pub fn render_agent_list(
    summaries: &[AgentSummary],
    unreleased: &[(String, Option<String>)],
    now: chrono::DateTime<chrono::Utc>,
    process_alive: &dyn Fn(u32) -> bool,
    limit: usize,
) -> String {
    let mut out = format!(
        "{:<14}  {:<6}  {:<22}  {:<9}  {:>4}  {:>6}  {:>7}  {:<22}  {:<48}  {}\n",
        "ID",
        "TYPE",
        "STATE",
        "IN STATE",
        "DONE",
        "FAILED",
        "STOPPED",
        "TOKENS",
        "LAST TASK (TYPE)",
        "RESOURCES"
    );
    for a in summaries.iter().take(limit) {
        let d = agent_display(a, now, process_alive);
        let held = resources_held(
            a,
            unreleased.iter().map(|(t, ag)| (t.as_str(), ag.as_deref())),
        );
        out.push_str(&format!(
            "{:<14}  {:<6}  {:<22}  {:<9}  {:>4}  {:>6}  {:>7}  {:<22}  {:<48}  {}\n",
            a.id,
            a.agent_type.as_deref().unwrap_or("not recorded"),
            d.state,
            d.in_state,
            a.tasks_completed,
            a.tasks_failed,
            a.tasks_stopped,
            d.tokens,
            d.last,
            held
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

/// Output of `selfware agents [--limit N]`. `unreleased` is `(owner task,
/// owner agent)` of every unreleased resource registry entry.
pub fn agents_command_output(
    log: &super::EventLog,
    unreleased: &[(String, Option<String>)],
    process_alive: &dyn Fn(u32) -> bool,
    limit: usize,
) -> String {
    let (records, skipped) = log.read_all();
    let summaries = agent_summaries(&records);
    let mut out = String::new();
    if summaries.is_empty() {
        out.push_str(&format!(
            "No agent transitions recorded in {}.\n",
            log_location(log)
        ));
    } else {
        out.push_str(&render_agent_list(
            &summaries,
            unreleased,
            chrono::Utc::now(),
            process_alive,
            limit,
        ));
        out.push_str(&format!(
            "(from {}; tokens = measured usage of ended tasks; resources = unreleased entries in the resource registry)\n",
            log_location(log)
        ));
    }
    out.push_str(&skipped_note(skipped));
    out
}

#[cfg(test)]
#[path = "../../tests/unit/lifecycle/projection_test.rs"]
mod tests;
