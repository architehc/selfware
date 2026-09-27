//! Per-agent view of a multi-chat run.
//!
//! A fan-out runs several agents at once. Printing their events as they
//! arrive interleaves them into one hard-to-follow stream, so this module
//! keeps a separate, chronological log per agent and derives everything the
//! front ends show from it:
//!
//! - the plain CLI prefixes every line with a stable short agent tag
//!   (`[coder·2] …`) and emits only complete lines ([`AgentLineBuffer`]), so
//!   two agents can never be spliced together mid-line;
//! - the TUI shows one tab per agent plus an "All" overview tab
//!   ([`MultiChatView`], [`ViewTab`]);
//! - both end with a per-agent summary table ([`summary_table_rows`]).
//!
//! Everything here is pure (no terminal I/O) so it can be unit tested.
//!
//! Honesty rules (AGENTS.md §3/§4): token counts come only from the
//! provider-reported `usage` on [`AgentResult`]. When an agent has no usage
//! (failed before a response, or skipped by the budget gate) the view shows
//! `—`, never an estimate. Durations are the measured `AgentResult::duration`,
//! or, while an agent is still working, the wall-clock time since its
//! `AgentStarted` event was received.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::swarm::AgentRole;

use super::types::{AgentResult, AgentStatus, MultiAgentEvent};

/// Number of distinct agent colours; agent ids cycle through them.
pub const AGENT_COLOR_COUNT: usize = 8;

/// Placeholder shown when a value was not measured (no provider usage).
pub const NOT_MEASURED: &str = "—";

/// Stable short tag for an agent: lowercase role name plus the 1-based agent
/// number, e.g. `coder·2`. The number matches the TUI tab key for the agent.
pub fn agent_tag(agent_id: usize, role: AgentRole) -> String {
    format!("{}·{}", role.name().to_lowercase(), agent_id + 1)
}

/// Stable colour slot for an agent (index into a front end's palette).
pub fn agent_color_index(agent_id: usize) -> usize {
    agent_id % AGENT_COLOR_COUNT
}

/// Stable terminal colour for an agent in the plain CLI.
pub fn agent_color(agent_id: usize) -> colored::Color {
    use colored::Color;
    const PALETTE: [Color; AGENT_COLOR_COUNT] = [
        Color::BrightCyan,
        Color::BrightMagenta,
        Color::BrightYellow,
        Color::BrightBlue,
        Color::Cyan,
        Color::Magenta,
        Color::Yellow,
        Color::Blue,
    ];
    PALETTE[agent_color_index(agent_id)]
}

/// Per-agent line assembler.
///
/// Text for an agent may arrive in arbitrary chunks. Only complete lines are
/// ever returned; a trailing partial line is held back per agent until its
/// newline arrives (or [`AgentLineBuffer::flush`] is called), so output from
/// different agents can interleave only at line boundaries, never mid-line.
#[derive(Debug, Default, Clone)]
pub struct AgentLineBuffer {
    pending: HashMap<usize, String>,
}

impl AgentLineBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a chunk for `agent_id`; return the lines it completed (without
    /// their trailing newline, `\r\n` normalised).
    pub fn push(&mut self, agent_id: usize, chunk: &str) -> Vec<String> {
        let pending = self.pending.entry(agent_id).or_default();
        pending.push_str(chunk);
        let mut lines = Vec::new();
        while let Some(pos) = pending.find('\n') {
            let mut line: String = pending.drain(..=pos).collect();
            line.pop(); // '\n'
            if line.ends_with('\r') {
                line.pop();
            }
            lines.push(line);
        }
        lines
    }

    /// Release the held-back partial line for `agent_id`, if any.
    pub fn flush(&mut self, agent_id: usize) -> Option<String> {
        self.pending
            .remove(&agent_id)
            .filter(|rest| !rest.is_empty())
    }

    /// Whether `agent_id` has a held-back partial line.
    pub fn has_pending(&self, agent_id: usize) -> bool {
        self.pending.get(&agent_id).is_some_and(|p| !p.is_empty())
    }
}

/// One complete line of an agent's log, ready to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLine {
    pub agent_id: usize,
    pub text: String,
}

/// Everything the view knows about one agent.
#[derive(Debug, Clone)]
pub struct AgentPanel {
    pub agent_id: usize,
    pub role: AgentRole,
    pub name: String,
    pub status: AgentStatus,
    /// When the `AgentStarted` event for the current run was received.
    pub started_at: Option<Instant>,
    /// Measured duration from the agent's `AgentResult`.
    pub duration: Option<Duration>,
    /// Provider-reported `usage.total_tokens`; `None` = not reported.
    pub tokens: Option<usize>,
    /// Short description of the most recent event.
    pub last_activity: String,
    /// Chronological log (complete lines only).
    pub lines: Vec<String>,
}

impl AgentPanel {
    pub fn new(agent_id: usize, role: AgentRole, name: impl Into<String>) -> Self {
        Self {
            agent_id,
            role,
            name: name.into(),
            status: AgentStatus::Idle,
            started_at: None,
            duration: None,
            tokens: None,
            last_activity: "idle".to_string(),
            lines: Vec::new(),
        }
    }

    pub fn tag(&self) -> String {
        agent_tag(self.agent_id, self.role)
    }

    /// Elapsed time: the measured result duration once finished, otherwise
    /// wall-clock time since the start event (`None` before it started).
    pub fn elapsed(&self, now: Instant) -> Option<Duration> {
        self.duration
            .or_else(|| self.started_at.map(|s| now.saturating_duration_since(s)))
    }
}

/// Which tab the TUI shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewTab {
    /// Overview: one status line per agent.
    All,
    /// A single agent's stream (index into the panel list).
    Agent(usize),
}

/// Human label for an [`AgentStatus`].
pub fn status_label(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::Idle => "idle",
        AgentStatus::Working => "working",
        AgentStatus::Completed => "done",
        AgentStatus::Failed => "failed",
    }
}

/// Format a measured duration compactly (`3.21s`, `2m05s`).
pub fn format_duration(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs < 60.0 {
        format!("{secs:.2}s")
    } else {
        let whole = d.as_secs();
        format!("{}m{:02}s", whole / 60, whole % 60)
    }
}

/// Format a provider-reported token count, or [`NOT_MEASURED`].
pub fn format_tokens(tokens: Option<usize>) -> String {
    tokens.map_or_else(|| NOT_MEASURED.to_string(), |t| t.to_string())
}

/// Truncate to `max` chars (UTF-8 safe), appending `…` when cut.
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

/// Per-agent state of a multi-chat session plus the selected TUI tab.
#[derive(Debug, Clone)]
pub struct MultiChatView {
    panels: Vec<AgentPanel>,
    tab: ViewTab,
    buffer: AgentLineBuffer,
}

impl MultiChatView {
    /// Build a view for agents given as `(agent_id, role, name)`, in id order.
    pub fn new<I, S>(agents: I) -> Self
    where
        I: IntoIterator<Item = (usize, AgentRole, S)>,
        S: Into<String>,
    {
        let mut view = Self {
            panels: Vec::new(),
            tab: ViewTab::All,
            buffer: AgentLineBuffer::new(),
        };
        for (id, role, name) in agents {
            view.ensure_panel(id, Some(role), Some(name.into()));
        }
        view
    }

    /// Build a view for the agents a fan-out over `roles` will create
    /// (agent `i` has role `roles[i]`, named like `initialize_agents`).
    pub fn from_roles(roles: &[AgentRole]) -> Self {
        Self::new(
            roles
                .iter()
                .enumerate()
                .map(|(i, r)| (i, *r, format!("Agent-{}-{}", i, r.name()))),
        )
    }

    pub fn panels(&self) -> &[AgentPanel] {
        &self.panels
    }

    pub fn agent_count(&self) -> usize {
        self.panels.len()
    }

    pub fn tab(&self) -> ViewTab {
        self.tab
    }

    /// Select tab by its number key: `0` is "All", `1..=N` the agents.
    /// Out-of-range numbers leave the selection unchanged; returns whether
    /// the key selected a tab.
    pub fn select_number(&mut self, n: usize) -> bool {
        if n == 0 {
            self.tab = ViewTab::All;
            true
        } else if n <= self.panels.len() {
            self.tab = ViewTab::Agent(n - 1);
            true
        } else {
            false
        }
    }

    /// Cycle forward: All → 1 → … → N → All.
    pub fn next_tab(&mut self) {
        let n = self.panels.len();
        self.tab = match self.tab {
            _ if n == 0 => ViewTab::All,
            ViewTab::All => ViewTab::Agent(0),
            ViewTab::Agent(i) if i + 1 < n => ViewTab::Agent(i + 1),
            ViewTab::Agent(_) => ViewTab::All,
        };
    }

    /// Cycle backward: All → N → … → 1 → All.
    pub fn prev_tab(&mut self) {
        let n = self.panels.len();
        self.tab = match self.tab {
            _ if n == 0 => ViewTab::All,
            ViewTab::All => ViewTab::Agent(n - 1),
            ViewTab::Agent(0) => ViewTab::All,
            ViewTab::Agent(i) => ViewTab::Agent(i.min(n) - 1),
        };
    }

    /// Tab titles in order: `All`, then `1 coder·1`, `2 tester·2`, …
    pub fn tab_titles(&self) -> Vec<String> {
        std::iter::once("0 All".to_string())
            .chain(
                self.panels
                    .iter()
                    .enumerate()
                    .map(|(i, p)| format!("{} {}", i + 1, p.tag())),
            )
            .collect()
    }

    /// Index of the selected tab in [`Self::tab_titles`].
    pub fn tab_index(&self) -> usize {
        match self.tab {
            ViewTab::All => 0,
            ViewTab::Agent(i) => i + 1,
        }
    }

    /// Overview status line for one agent: tag, role, state, elapsed,
    /// tokens, last activity.
    pub fn status_line(panel: &AgentPanel, now: Instant) -> String {
        format!(
            "{:<16} {:<12} {:<8} {:>9}  {:>7} tok  {}",
            panel.tag(),
            panel.role.name(),
            status_label(panel.status),
            panel
                .elapsed(now)
                .map_or_else(|| NOT_MEASURED.to_string(), format_duration),
            format_tokens(panel.tokens),
            truncate_chars(&panel.last_activity, 60),
        )
    }

    /// Mark the start of a new task for every agent (a separator line in
    /// each agent's log) and reset per-run measurements.
    pub fn begin_task(&mut self, task: &str) -> Vec<AgentLine> {
        let header = format!("── task: {}", truncate_chars(task.trim(), 72));
        let mut out = Vec::new();
        for panel in &mut self.panels {
            panel.status = AgentStatus::Idle;
            panel.started_at = None;
            panel.duration = None;
            panel.tokens = None;
            panel.last_activity = "queued".to_string();
            panel.lines.push(header.clone());
            out.push(AgentLine {
                agent_id: panel.agent_id,
                text: header.clone(),
            });
        }
        out
    }

    fn ensure_panel(
        &mut self,
        agent_id: usize,
        role: Option<AgentRole>,
        name: Option<String>,
    ) -> &mut AgentPanel {
        let idx = match self.panels.iter().position(|p| p.agent_id == agent_id) {
            Some(idx) => idx,
            None => {
                let role = role.unwrap_or(AgentRole::General);
                let name = name.unwrap_or_else(|| format!("Agent-{}-{}", agent_id, role.name()));
                self.panels.push(AgentPanel::new(agent_id, role, name));
                self.panels.sort_by_key(|p| p.agent_id);
                self.panels
                    .iter()
                    .position(|p| p.agent_id == agent_id)
                    .expect("panel just inserted")
            }
        };
        &mut self.panels[idx]
    }

    /// Append raw text to an agent's log; only complete lines are recorded
    /// and returned. Call [`Self::end_output`] to release a trailing partial
    /// line.
    pub fn push_output(&mut self, agent_id: usize, chunk: &str) -> Vec<AgentLine> {
        let lines = self.buffer.push(agent_id, chunk);
        self.record(agent_id, lines)
    }

    /// Release an agent's held-back partial line, if any.
    pub fn end_output(&mut self, agent_id: usize) -> Vec<AgentLine> {
        let rest: Vec<String> = self.buffer.flush(agent_id).into_iter().collect();
        self.record(agent_id, rest)
    }

    fn record(&mut self, agent_id: usize, lines: Vec<String>) -> Vec<AgentLine> {
        let panel = self.ensure_panel(agent_id, None, None);
        panel.lines.extend(lines.iter().cloned());
        lines
            .into_iter()
            .map(|text| AgentLine { agent_id, text })
            .collect()
    }

    fn note(&mut self, agent_id: usize, text: String) -> AgentLine {
        let panel = self.ensure_panel(agent_id, None, None);
        panel.lines.push(text.clone());
        AgentLine { agent_id, text }
    }

    /// Apply one orchestration event at time `now`; returns the new log lines
    /// it produced, in order (each line belongs to exactly one agent).
    pub fn apply(&mut self, event: &MultiAgentEvent, now: Instant) -> Vec<AgentLine> {
        match event {
            MultiAgentEvent::AgentStarted { agent_id, name, .. } => {
                let panel = self.ensure_panel(*agent_id, None, Some(name.clone()));
                panel.status = AgentStatus::Working;
                panel.started_at = Some(now);
                panel.duration = None;
                panel.tokens = None;
                panel.last_activity = "waiting for response".to_string();
                vec![self.note(*agent_id, "▶ started".to_string())]
            }
            MultiAgentEvent::AgentFailed { agent_id, error } => {
                let panel = self.ensure_panel(*agent_id, None, None);
                panel.status = AgentStatus::Failed;
                panel.last_activity = format!("failed: {error}");
                let mut out = self.end_output(*agent_id);
                out.push(self.note(*agent_id, format!("✗ failed: {error}")));
                out
            }
            MultiAgentEvent::AgentCompleted { agent_id, result } => {
                self.apply_result(*agent_id, result)
            }
            MultiAgentEvent::AllCompleted { .. } => Vec::new(),
        }
    }

    fn apply_result(&mut self, agent_id: usize, result: &AgentResult) -> Vec<AgentLine> {
        let already_failed = {
            let panel =
                self.ensure_panel(agent_id, Some(result.role), Some(result.agent_name.clone()));
            panel.duration = Some(result.duration);
            panel.tokens = result.usage.as_ref().map(|u| u.total_tokens);
            panel.status == AgentStatus::Failed
        };

        let mut out = Vec::new();
        if result.success {
            if !result.content.is_empty() {
                out.extend(self.push_output(agent_id, &result.content));
            }
            out.extend(self.end_output(agent_id));
            let panel = self.ensure_panel(agent_id, None, None);
            panel.status = AgentStatus::Completed;
            let tokens = panel.tokens;
            let reply_lines = result.content.lines().count();
            // Elapsed and tokens have their own columns; say what arrived.
            panel.last_activity = format!(
                "replied ({reply_lines} line{})",
                if reply_lines == 1 { "" } else { "s" }
            );
            let line = format!(
                "✓ done in {} · {} tokens",
                format_duration(result.duration),
                format_tokens(tokens)
            );
            out.push(self.note(agent_id, line));
        } else {
            out.extend(self.end_output(agent_id));
            let panel = self.ensure_panel(agent_id, None, None);
            panel.status = AgentStatus::Failed;
            if !already_failed {
                let error = result.error.as_deref().unwrap_or("unknown error");
                panel.last_activity = format!("failed: {error}");
                let line = format!("✗ failed: {error}");
                out.push(self.note(agent_id, line));
            }
        }
        out
    }
}

/// Header of the per-agent summary table.
pub const SUMMARY_HEADER: [&str; 5] = ["agent", "role", "outcome", "duration", "tokens"];

/// Summary table cells for each result, in agent-id order: tag, role,
/// outcome (`ok` / `failed` / `skipped`), measured duration, and
/// provider-reported tokens (`—` when none were reported).
pub fn summary_table_rows(results: &[AgentResult]) -> Vec<[String; 5]> {
    let mut sorted: Vec<&AgentResult> = results.iter().collect();
    sorted.sort_by_key(|r| r.agent_id);
    sorted
        .into_iter()
        .map(|r| {
            let outcome = if r.success {
                "ok"
            } else if r.error.as_deref().is_some_and(|e| e.starts_with("skipped")) {
                // Budget-gate skips make no call: say so rather than "failed".
                "skipped"
            } else {
                "failed"
            };
            let tokens = match &r.usage {
                Some(u) => format!(
                    "{} ({}+{})",
                    u.total_tokens, u.prompt_tokens, u.completion_tokens
                ),
                None => NOT_MEASURED.to_string(),
            };
            [
                agent_tag(r.agent_id, r.role),
                r.role.name().to_string(),
                outcome.to_string(),
                format_duration(r.duration),
                tokens,
            ]
        })
        .collect()
}

/// Render the summary table as aligned plain-text lines (header, rule, one
/// row per agent). Column widths are measured in chars, so the table lines
/// up with the non-ASCII `·` in tags and `—` placeholders.
pub fn render_summary_table(results: &[AgentResult]) -> Vec<String> {
    let rows = summary_table_rows(results);
    let mut widths = SUMMARY_HEADER.map(|h| h.chars().count());
    for row in &rows {
        for (w, cell) in widths.iter_mut().zip(row.iter()) {
            *w = (*w).max(cell.chars().count());
        }
    }
    let fmt_row = |cells: [&str; 5]| -> String {
        let mut line = String::new();
        for (i, (cell, w)) in cells.iter().zip(widths.iter()).enumerate() {
            let pad = w - cell.chars().count();
            // Numbers (duration, tokens) right-aligned; text left-aligned.
            if i >= 3 {
                line.push_str(&" ".repeat(pad));
                line.push_str(cell);
            } else {
                line.push_str(cell);
                line.push_str(&" ".repeat(pad));
            }
            if i + 1 < cells.len() {
                line.push_str("  ");
            }
        }
        line.trim_end().to_string()
    };
    let mut out = vec![fmt_row(SUMMARY_HEADER)];
    out.push(
        widths
            .iter()
            .map(|w| "─".repeat(*w))
            .collect::<Vec<_>>()
            .join("  "),
    );
    for row in &rows {
        out.push(fmt_row([&row[0], &row[1], &row[2], &row[3], &row[4]]));
    }
    out
}

#[cfg(test)]
#[path = "../../../tests/unit/orchestration/multiagent/view/view_test.rs"]
mod tests;
