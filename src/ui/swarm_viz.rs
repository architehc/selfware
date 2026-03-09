#![allow(dead_code, unused_imports, unused_variables)]
//! Swarm Visualization System
//!
//! Terminal-based real-time visualization for multi-agent coordination.
//! Displays agent status panels, consensus history, and activity timelines
//! with animated progress bars and color-coded state indicators.

use std::fmt;
use std::time::{Duration, Instant};

use super::style::{Glyphs, SelfwareStyle};
use super::task_display::format_duration;

// ============================================================================
// Spinner Frames
// ============================================================================

/// Braille spinner frames for working agents.
const SPINNER_FRAMES: &[char] = &[
    '\u{280B}', // ⠋
    '\u{2819}', // ⠙
    '\u{2839}', // ⠹
    '\u{2838}', // ⠸
    '\u{283C}', // ⠼
    '\u{2834}', // ⠴
    '\u{2826}', // ⠦
    '\u{2827}', // ⠧
    '\u{2807}', // ⠇
    '\u{280F}', // ⠏
];

/// Progress bar fill character.
const BAR_FILL: char = '\u{2588}'; // █
/// Progress bar empty character.
const BAR_EMPTY: char = '\u{2591}'; // ░

// ============================================================================
// AgentState
// ============================================================================

/// The current operational state of a swarm agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    /// Agent is registered but has no active task.
    Idle,
    /// Agent is actively processing its task.
    Working,
    /// Agent is waiting for a consensus decision.
    Waiting,
    /// Agent has completed its task successfully.
    Done,
    /// Agent encountered an unrecoverable error.
    Failed,
}

impl fmt::Display for AgentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            Self::Working => write!(f, "Working"),
            Self::Waiting => write!(f, "Waiting"),
            Self::Done => write!(f, "Done"),
            Self::Failed => write!(f, "Failed"),
        }
    }
}

// ============================================================================
// AgentStatus
// ============================================================================

/// Live status of a single agent in the swarm.
pub struct AgentStatus {
    /// Role label, e.g. "Architect", "Coder", "Tester".
    pub role: String,
    /// Current operational state.
    pub state: AgentState,
    /// Description of the task the agent is working on, if any.
    pub current_task: Option<String>,
    /// Progress from 0.0 (not started) to 1.0 (complete).
    pub progress: f32,
    /// Total tool invocations by this agent.
    pub tool_calls: u32,
    /// Total tokens consumed by this agent.
    pub tokens_used: u64,
    /// Timestamp of the most recent status update.
    pub last_update: Instant,
}

impl AgentStatus {
    /// Create a new agent status in the Idle state.
    fn new(role: &str) -> Self {
        Self {
            role: role.to_string(),
            state: AgentState::Idle,
            current_task: None,
            progress: 0.0,
            tool_calls: 0,
            tokens_used: 0,
            last_update: Instant::now(),
        }
    }

    /// Return the emoji glyph for this agent's role.
    fn role_icon(&self) -> &'static str {
        match self.role.as_str() {
            "Architect" => "\u{1F3D7}\u{FE0F} ",    // 🏗️
            "Coder" => "\u{1F4BB}",                 // 💻
            "Tester" => "\u{1F9EA}",                // 🧪
            "Reviewer" => "\u{1F4CB}",              // 📋
            "DevOps" => "\u{2699}\u{FE0F} ",        // ⚙️
            "Security" => "\u{1F512}",              // 🔒
            "Documenter" => "\u{1F4DD}",            // 📝
            "Performance" => "\u{26A1}",            // ⚡
            "VisualCritic" => "\u{1F441}\u{FE0F} ", // 👁️
            _ => "\u{1F916}",                       // 🤖
        }
    }

    /// Return the state indicator suffix.
    fn state_suffix(&self) -> &'static str {
        match self.state {
            AgentState::Done => " \u{2713}",   // ✓
            AgentState::Failed => " \u{2717}", // ✗
            _ => "",
        }
    }
}

// ============================================================================
// ConsensusEvent
// ============================================================================

/// A recorded consensus decision among agents.
pub struct ConsensusEvent {
    /// When the consensus occurred.
    pub timestamp: Instant,
    /// The topic being decided.
    pub topic: String,
    /// The conflict resolution strategy used.
    pub strategy: String,
    /// Votes cast by each agent: (role, approved).
    pub votes: Vec<(String, bool)>,
    /// Whether the proposal was accepted.
    pub outcome: bool,
}

// ============================================================================
// TimelineEntry
// ============================================================================

/// A single entry in the activity timeline.
struct TimelineEntry {
    /// When this event occurred.
    timestamp: Instant,
    /// Human-readable description of the event.
    description: String,
}

// ============================================================================
// SwarmVisualization
// ============================================================================

/// Terminal-based real-time visualization for multi-agent swarm coordination.
///
/// Tracks agent statuses, consensus history, and an activity timeline,
/// then renders them as bordered terminal panels with animated progress
/// bars and color-coded state indicators.
pub struct SwarmVisualization {
    /// Status of each agent in the swarm, keyed by role name.
    agents: Vec<AgentStatus>,
    /// Historical record of consensus decisions.
    consensus_history: Vec<ConsensusEvent>,
    /// Activity timeline entries.
    timeline: Vec<TimelineEntry>,
    /// When the swarm was started.
    start_time: Instant,
    /// Monotonically increasing tick for spinner animation.
    tick: u64,
}

impl SwarmVisualization {
    /// Create a new, empty swarm visualization.
    pub fn new() -> Self {
        Self {
            agents: Vec::new(),
            consensus_history: Vec::new(),
            timeline: Vec::new(),
            start_time: Instant::now(),
            tick: 0,
        }
    }

    // ────────────────────────────────────────────────────────────────
    // Mutation helpers
    // ────────────────────────────────────────────────────────────────

    /// Register a new agent by role and return a mutable reference to it.
    pub fn add_agent(&mut self, role: &str) -> &mut AgentStatus {
        self.timeline.push(TimelineEntry {
            timestamp: Instant::now(),
            description: format!("{} joined swarm", role),
        });
        self.agents.push(AgentStatus::new(role));
        self.agents.last_mut().unwrap()
    }

    /// Update an existing agent's state, task, and progress.
    pub fn update_agent(
        &mut self,
        role: &str,
        state: AgentState,
        task: Option<&str>,
        progress: f32,
    ) {
        if let Some(agent) = self.agents.iter_mut().find(|a| a.role == role) {
            let old_state = agent.state;
            agent.state = state;
            agent.current_task = task.map(|t| t.to_string());
            agent.progress = progress.clamp(0.0, 1.0);
            agent.last_update = Instant::now();

            // Record notable state transitions in the timeline.
            if old_state != state {
                let desc = match state {
                    AgentState::Working => {
                        if let Some(t) = task {
                            format!("{} started {}", role, t)
                        } else {
                            format!("{} started working", role)
                        }
                    }
                    AgentState::Done => format!("{} completed", role),
                    AgentState::Failed => format!("{} failed", role),
                    AgentState::Waiting => format!("{} waiting for consensus", role),
                    AgentState::Idle => format!("{} went idle", role),
                };
                self.timeline.push(TimelineEntry {
                    timestamp: Instant::now(),
                    description: desc,
                });
            }
        }
    }

    /// Record a tool call by an agent and add it to the timeline.
    pub fn record_tool_call(&mut self, role: &str, tool: &str) {
        if let Some(agent) = self.agents.iter_mut().find(|a| a.role == role) {
            agent.tool_calls += 1;
            agent.last_update = Instant::now();
        }
        self.timeline.push(TimelineEntry {
            timestamp: Instant::now(),
            description: format!("{} \u{2192} {}", role, tool), // →
        });
    }

    /// Record a consensus event and add it to the timeline.
    pub fn record_consensus(&mut self, event: ConsensusEvent) {
        let summary = format!("Consensus: {}", event.topic);
        self.timeline.push(TimelineEntry {
            timestamp: event.timestamp,
            description: summary,
        });
        self.consensus_history.push(event);
    }

    /// Advance the animation tick (call periodically from a timer).
    pub fn advance_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    // ────────────────────────────────────────────────────────────────
    // Rendering
    // ────────────────────────────────────────────────────────────────

    /// Render all panels: agent status, consensus log, and timeline.
    pub fn render_full(&self) -> String {
        let mut output = String::new();
        output.push_str(&self.render_agent_panel());
        output.push('\n');
        output.push_str(&self.render_consensus_panel());
        output.push('\n');
        output.push_str(&self.render_timeline_panel());
        output
    }

    /// Render a compact one-line summary of swarm status.
    ///
    /// Example: `Swarm: 3/5 active | Architect:80% Coder:done Tester:40%`
    pub fn render_compact(&self) -> String {
        let total = self.agents.len();
        let active = self
            .agents
            .iter()
            .filter(|a| a.state == AgentState::Working || a.state == AgentState::Waiting)
            .count();

        let mut parts: Vec<String> = Vec::new();
        for agent in &self.agents {
            let status = match agent.state {
                AgentState::Done => "done".to_string(),
                AgentState::Failed => "FAIL".to_string(),
                AgentState::Idle => "idle".to_string(),
                AgentState::Waiting => "wait".to_string(),
                AgentState::Working => format!("{}%", (agent.progress * 100.0) as u32),
            };
            parts.push(format!("{}:{}", agent.role, status));
        }

        format!("Swarm: {}/{} active | {}", active, total, parts.join(" "))
    }

    /// Render the progress bar for a single agent identified by role.
    ///
    /// Returns an empty string if the role is not found.
    pub fn render_agent_progress(&self, role: &str) -> String {
        if let Some(agent) = self.agents.iter().find(|a| a.role == role) {
            let bar = render_progress_bar(agent.progress, 10);
            let pct = format!("{}%", (agent.progress * 100.0) as u32);
            format!(
                "{} {} {} {} {}{}",
                agent.role_icon(),
                pad_right(&agent.role, 12),
                bar,
                pad_right(&pct, 4),
                agent.state,
                agent.state_suffix(),
            )
        } else {
            String::new()
        }
    }

    // ────────────────────────────────────────────────────────────────
    // Panel renderers (private)
    // ────────────────────────────────────────────────────────────────

    /// Render the agent status panel.
    fn render_agent_panel(&self) -> String {
        let inner_width: usize = 48;
        let h = Glyphs::horiz();
        let v = Glyphs::vert();

        let mut lines: Vec<String> = Vec::new();

        // Top border
        let title = " Swarm Status ";
        let title_len = title.chars().count();
        let remaining = inner_width.saturating_sub(title_len);
        lines.push(format!(
            "{}{}{}{}{}",
            Glyphs::corner_tl(),
            h,
            title,
            h.repeat(remaining.saturating_sub(1)),
            Glyphs::corner_tr(),
        ));

        // Empty line
        lines.push(format!("{} {:width$} {}", v, "", v, width = inner_width));

        // Agent lines
        for agent in &self.agents {
            let spinner = self.spinner_for(agent);
            let bar = render_progress_bar(agent.progress, 10);
            let pct = format!("{}%", (agent.progress * 100.0) as u32);
            let state_str = format!("{}{}", agent.state, agent.state_suffix());

            let content = format!(
                "{}  {} {} {} {}",
                spinner,
                pad_right(&agent.role, 12),
                bar,
                pad_right(&pct, 4),
                state_str,
            );
            // Pad to inner width
            let content_display_len = display_width(&content);
            let padding = inner_width.saturating_sub(content_display_len);
            lines.push(format!("{} {}{} {}", v, content, " ".repeat(padding), v));
        }

        // Empty line
        lines.push(format!("{} {:width$} {}", v, "", v, width = inner_width));

        // Consensus summary
        let decisions_made = self.consensus_history.len();
        let total_decisions = decisions_made + self.pending_consensus_count();
        let strategy = self.dominant_strategy();
        let consensus_line = format!(
            "Consensus: {}/{} decisions made",
            decisions_made, total_decisions
        );
        let strategy_line = format!("Strategy: {}", strategy);

        let c_pad = inner_width.saturating_sub(consensus_line.chars().count());
        lines.push(format!(
            "{} {}{} {}",
            v,
            consensus_line,
            " ".repeat(c_pad),
            v
        ));
        let s_pad = inner_width.saturating_sub(strategy_line.chars().count());
        lines.push(format!(
            "{} {}{} {}",
            v,
            strategy_line,
            " ".repeat(s_pad),
            v
        ));

        // Bottom border
        lines.push(format!(
            "{}{}{}",
            Glyphs::corner_bl(),
            h.repeat(inner_width + 2),
            Glyphs::corner_br(),
        ));

        lines.join("\n")
    }

    /// Render the consensus history panel.
    fn render_consensus_panel(&self) -> String {
        let inner_width: usize = 48;
        let h = Glyphs::horiz();
        let v = Glyphs::vert();

        let mut lines: Vec<String> = Vec::new();

        // Top border
        let title = " Consensus Log ";
        let title_len = title.chars().count();
        let remaining = inner_width.saturating_sub(title_len);
        lines.push(format!(
            "{}{}{}{}{}",
            Glyphs::corner_tl(),
            h,
            title,
            h.repeat(remaining.saturating_sub(1)),
            Glyphs::corner_tr(),
        ));

        if self.consensus_history.is_empty() {
            let msg = "No consensus events yet.";
            let pad = inner_width.saturating_sub(msg.len());
            lines.push(format!("{} {}{} {}", v, msg, " ".repeat(pad), v));
        } else {
            // Show last 5 consensus events
            let start = self.consensus_history.len().saturating_sub(5);
            for event in &self.consensus_history[start..] {
                let elapsed = event.timestamp.duration_since(self.start_time);
                let time_tag = format_elapsed_compact(elapsed);

                let topic_line = format!("[{}] {}", time_tag, event.topic);
                let t_pad = inner_width.saturating_sub(topic_line.chars().count());
                lines.push(format!("{} {}{} {}", v, topic_line, " ".repeat(t_pad), v,));

                // Votes line
                let votes_str: Vec<String> = event
                    .votes
                    .iter()
                    .map(|(role, approved)| {
                        let mark = if *approved { "\u{2713}" } else { "\u{2717}" };
                        format!("{}:{}", role, mark)
                    })
                    .collect();
                let outcome_mark = if event.outcome {
                    "\u{2192} approved"
                } else {
                    "\u{2192} rejected"
                };
                let votes_line = format!("       {} {}", votes_str.join(" "), outcome_mark);
                let vl_chars = votes_line.chars().count();
                let v_pad = inner_width.saturating_sub(vl_chars);
                lines.push(format!("{} {}{} {}", v, votes_line, " ".repeat(v_pad), v,));
            }
        }

        // Bottom border
        lines.push(format!(
            "{}{}{}",
            Glyphs::corner_bl(),
            h.repeat(inner_width + 2),
            Glyphs::corner_br(),
        ));

        lines.join("\n")
    }

    /// Render the activity timeline panel.
    fn render_timeline_panel(&self) -> String {
        let inner_width: usize = 48;
        let h = Glyphs::horiz();
        let v = Glyphs::vert();

        let mut lines: Vec<String> = Vec::new();

        // Top border
        let title = " Timeline ";
        let title_len = title.chars().count();
        let remaining = inner_width.saturating_sub(title_len);
        lines.push(format!(
            "{}{}{}{}{}",
            Glyphs::corner_tl(),
            h,
            title,
            h.repeat(remaining.saturating_sub(1)),
            Glyphs::corner_tr(),
        ));

        if self.timeline.is_empty() {
            let msg = "No activity yet.";
            let pad = inner_width.saturating_sub(msg.len());
            lines.push(format!("{} {}{} {}", v, msg, " ".repeat(pad), v));
        } else {
            // Show last 10 timeline entries
            let start = self.timeline.len().saturating_sub(10);
            for entry in &self.timeline[start..] {
                let elapsed = entry.timestamp.duration_since(self.start_time);
                let time_tag = format_elapsed_compact(elapsed);

                let line_content = format!("\u{25B8} {}  {}", time_tag, entry.description,);
                let lc_chars = line_content.chars().count();
                // Truncate if too long
                let display = if lc_chars > inner_width {
                    let truncated: String = line_content.chars().take(inner_width - 3).collect();
                    format!("{}...", truncated)
                } else {
                    line_content
                };
                let d_chars = display.chars().count();
                let pad = inner_width.saturating_sub(d_chars);
                lines.push(format!("{} {}{} {}", v, display, " ".repeat(pad), v));
            }
        }

        // Bottom border
        lines.push(format!(
            "{}{}{}",
            Glyphs::corner_bl(),
            h.repeat(inner_width + 2),
            Glyphs::corner_br(),
        ));

        lines.join("\n")
    }

    // ────────────────────────────────────────────────────────────────
    // Internal helpers
    // ────────────────────────────────────────────────────────────────

    /// Get the spinner character for an agent based on its state and the
    /// current tick.
    fn spinner_for(&self, agent: &AgentStatus) -> String {
        match agent.state {
            AgentState::Working => {
                let idx = (self.tick as usize) % SPINNER_FRAMES.len();
                format!("{}", SPINNER_FRAMES[idx])
            }
            _ => agent.role_icon().to_string(),
        }
    }

    /// Count agents currently waiting (a rough proxy for pending consensus).
    fn pending_consensus_count(&self) -> usize {
        self.agents
            .iter()
            .filter(|a| a.state == AgentState::Waiting)
            .count()
    }

    /// Determine the dominant conflict resolution strategy from history.
    fn dominant_strategy(&self) -> String {
        if self.consensus_history.is_empty() {
            return "none".to_string();
        }
        let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for event in &self.consensus_history {
            *counts.entry(event.strategy.as_str()).or_insert(0) += 1;
        }
        counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(strategy, _)| strategy.to_string())
            .unwrap_or_else(|| "none".to_string())
    }
}

impl Default for SwarmVisualization {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Free functions
// ============================================================================

/// Render a progress bar of the given width.
///
/// Example: `[████████░░] 80%`
pub fn render_progress_bar(progress: f32, width: usize) -> String {
    let clamped = progress.clamp(0.0, 1.0);
    let filled = (clamped * width as f32).round() as usize;
    let empty = width.saturating_sub(filled);

    format!(
        "[{}{}]",
        BAR_FILL.to_string().repeat(filled),
        BAR_EMPTY.to_string().repeat(empty),
    )
}

/// Format an elapsed duration as compact `M:SS`.
fn format_elapsed_compact(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{}:{:02}", mins, secs)
}

/// Pad a string on the right to the given width.
fn pad_right(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - len))
    }
}

/// Estimate the display width of a string (each char counts as 1 for
/// simplicity; emoji may be wider in some terminals but this keeps
/// alignment predictable).
fn display_width(s: &str) -> usize {
    s.chars().count()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── Construction ────────────────────────────────────────────────

    #[test]
    fn test_new_visualization() {
        let viz = SwarmVisualization::new();
        assert!(viz.agents.is_empty());
        assert!(viz.consensus_history.is_empty());
        assert!(viz.timeline.is_empty());
        assert_eq!(viz.tick, 0);
    }

    #[test]
    fn test_default_impl() {
        let viz = SwarmVisualization::default();
        assert!(viz.agents.is_empty());
    }

    // ── Agent management ────────────────────────────────────────────

    #[test]
    fn test_add_agent() {
        let mut viz = SwarmVisualization::new();
        let agent = viz.add_agent("Architect");
        assert_eq!(agent.role, "Architect");
        assert_eq!(agent.state, AgentState::Idle);
        assert_eq!(agent.progress, 0.0);
        assert_eq!(agent.tool_calls, 0);
        assert_eq!(agent.tokens_used, 0);
        assert!(agent.current_task.is_none());
        assert_eq!(viz.agents.len(), 1);
        // Adding an agent creates a timeline entry
        assert_eq!(viz.timeline.len(), 1);
        assert!(viz.timeline[0].description.contains("Architect"));
    }

    #[test]
    fn test_add_multiple_agents() {
        let mut viz = SwarmVisualization::new();
        viz.add_agent("Architect");
        viz.add_agent("Coder");
        viz.add_agent("Tester");
        assert_eq!(viz.agents.len(), 3);
        assert_eq!(viz.agents[0].role, "Architect");
        assert_eq!(viz.agents[1].role, "Coder");
        assert_eq!(viz.agents[2].role, "Tester");
    }

    // ── Agent updates ───────────────────────────────────────────────

    #[test]
    fn test_update_agent_state() {
        let mut viz = SwarmVisualization::new();
        viz.add_agent("Coder");
        viz.update_agent("Coder", AgentState::Working, Some("implementing API"), 0.5);

        let agent = &viz.agents[0];
        assert_eq!(agent.state, AgentState::Working);
        assert_eq!(agent.current_task.as_deref(), Some("implementing API"));
        assert!((agent.progress - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_update_agent_progress_clamped() {
        let mut viz = SwarmVisualization::new();
        viz.add_agent("Tester");

        viz.update_agent("Tester", AgentState::Working, None, 1.5);
        assert!((viz.agents[0].progress - 1.0).abs() < f32::EPSILON);

        viz.update_agent("Tester", AgentState::Working, None, -0.5);
        assert!((viz.agents[0].progress - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_update_nonexistent_agent_is_noop() {
        let mut viz = SwarmVisualization::new();
        viz.add_agent("Coder");
        // Should not panic
        viz.update_agent("Ghost", AgentState::Working, None, 0.5);
        assert_eq!(viz.agents.len(), 1);
    }

    #[test]
    fn test_update_agent_creates_timeline_entry_on_state_change() {
        let mut viz = SwarmVisualization::new();
        viz.add_agent("Architect");
        let initial_timeline_len = viz.timeline.len();

        viz.update_agent("Architect", AgentState::Working, Some("planning"), 0.1);
        assert_eq!(viz.timeline.len(), initial_timeline_len + 1);
        assert!(viz.timeline.last().unwrap().description.contains("started"));

        // Same state again should NOT add a timeline entry
        let len_before = viz.timeline.len();
        viz.update_agent(
            "Architect",
            AgentState::Working,
            Some("still planning"),
            0.3,
        );
        assert_eq!(viz.timeline.len(), len_before);

        // Transition to Done
        viz.update_agent("Architect", AgentState::Done, None, 1.0);
        assert!(viz
            .timeline
            .last()
            .unwrap()
            .description
            .contains("completed"));
    }

    #[test]
    fn test_update_agent_timeline_descriptions() {
        let mut viz = SwarmVisualization::new();
        viz.add_agent("Security");

        viz.update_agent("Security", AgentState::Waiting, None, 0.0);
        assert!(viz.timeline.last().unwrap().description.contains("waiting"));

        viz.update_agent("Security", AgentState::Failed, None, 0.0);
        assert!(viz.timeline.last().unwrap().description.contains("failed"));

        viz.update_agent("Security", AgentState::Idle, None, 0.0);
        assert!(viz.timeline.last().unwrap().description.contains("idle"));
    }

    // ── Tool call recording ─────────────────────────────────────────

    #[test]
    fn test_record_tool_call() {
        let mut viz = SwarmVisualization::new();
        viz.add_agent("Coder");
        viz.record_tool_call("Coder", "file_write");
        viz.record_tool_call("Coder", "shell_exec");

        assert_eq!(viz.agents[0].tool_calls, 2);
        // Each tool call adds a timeline entry (plus 1 for add_agent)
        assert!(viz.timeline.len() >= 3);
        assert!(viz
            .timeline
            .iter()
            .any(|e| e.description.contains("file_write")));
    }

    #[test]
    fn test_record_tool_call_unknown_agent() {
        let mut viz = SwarmVisualization::new();
        viz.add_agent("Coder");
        // Should not panic; timeline still gets an entry
        let before = viz.timeline.len();
        viz.record_tool_call("Ghost", "file_read");
        assert_eq!(viz.timeline.len(), before + 1);
        // Agent tool count should not change
        assert_eq!(viz.agents[0].tool_calls, 0);
    }

    // ── Consensus recording ─────────────────────────────────────────

    #[test]
    fn test_record_consensus() {
        let mut viz = SwarmVisualization::new();
        let event = ConsensusEvent {
            timestamp: Instant::now(),
            topic: "REST vs GraphQL".to_string(),
            strategy: "majority".to_string(),
            votes: vec![
                ("Architect".to_string(), true),
                ("Coder".to_string(), true),
                ("Reviewer".to_string(), false),
            ],
            outcome: true,
        };
        viz.record_consensus(event);

        assert_eq!(viz.consensus_history.len(), 1);
        assert_eq!(viz.consensus_history[0].topic, "REST vs GraphQL");
        assert!(viz.consensus_history[0].outcome);
        // Also creates a timeline entry
        assert!(viz
            .timeline
            .iter()
            .any(|e| e.description.contains("Consensus")));
    }

    // ── Animation ───────────────────────────────────────────────────

    #[test]
    fn test_advance_tick() {
        let mut viz = SwarmVisualization::new();
        assert_eq!(viz.tick, 0);
        viz.advance_tick();
        assert_eq!(viz.tick, 1);
        viz.advance_tick();
        viz.advance_tick();
        assert_eq!(viz.tick, 3);
    }

    #[test]
    fn test_spinner_cycles() {
        let mut viz = SwarmVisualization::new();
        viz.add_agent("Coder");
        viz.update_agent("Coder", AgentState::Working, None, 0.5);

        let mut seen = std::collections::HashSet::new();
        for _ in 0..SPINNER_FRAMES.len() {
            let s = viz.spinner_for(&viz.agents[0]);
            seen.insert(s);
            viz.advance_tick();
        }
        assert_eq!(seen.len(), SPINNER_FRAMES.len());
    }

    #[test]
    fn test_spinner_idle_uses_role_icon() {
        let viz = SwarmVisualization::new();
        let agent = AgentStatus::new("Architect");
        let s = viz.spinner_for(&agent);
        assert_eq!(s, agent.role_icon());
    }

    // ── Progress bar ────────────────────────────────────────────────

    #[test]
    fn test_render_progress_bar_zero() {
        let bar = render_progress_bar(0.0, 10);
        assert!(bar.starts_with('['));
        assert!(bar.ends_with(']'));
        assert!(!bar.contains(BAR_FILL));
        // All empty chars
        let count = bar.chars().filter(|c| *c == BAR_EMPTY).count();
        assert_eq!(count, 10);
    }

    #[test]
    fn test_render_progress_bar_full() {
        let bar = render_progress_bar(1.0, 10);
        let count = bar.chars().filter(|c| *c == BAR_FILL).count();
        assert_eq!(count, 10);
        assert!(!bar.contains(BAR_EMPTY));
    }

    #[test]
    fn test_render_progress_bar_half() {
        let bar = render_progress_bar(0.5, 10);
        let filled: usize = bar.chars().filter(|c| *c == BAR_FILL).count();
        let empty: usize = bar.chars().filter(|c| *c == BAR_EMPTY).count();
        assert_eq!(filled, 5);
        assert_eq!(empty, 5);
    }

    #[test]
    fn test_render_progress_bar_clamped() {
        let bar_over = render_progress_bar(1.5, 10);
        let bar_full = render_progress_bar(1.0, 10);
        assert_eq!(bar_over, bar_full);

        let bar_under = render_progress_bar(-0.5, 10);
        let bar_zero = render_progress_bar(0.0, 10);
        assert_eq!(bar_under, bar_zero);
    }

    #[test]
    fn test_render_progress_bar_various_widths() {
        for width in [1, 5, 20, 50] {
            let bar = render_progress_bar(0.5, width);
            // Total chars between brackets should equal width
            let inner_len = bar.len() - "[".len() - "]".len();
            // Note: multi-byte chars, so count chars not bytes
            let inner_chars: usize = bar
                .chars()
                .filter(|c| *c == BAR_FILL || *c == BAR_EMPTY)
                .count();
            assert_eq!(inner_chars, width);
        }
    }

    // ── Compact rendering ───────────────────────────────────────────

    #[test]
    fn test_render_compact_empty() {
        let viz = SwarmVisualization::new();
        let compact = viz.render_compact();
        assert!(compact.starts_with("Swarm: 0/0 active"));
    }

    #[test]
    fn test_render_compact_with_agents() {
        let mut viz = SwarmVisualization::new();
        viz.add_agent("Architect");
        viz.add_agent("Coder");
        viz.add_agent("Tester");

        viz.update_agent("Architect", AgentState::Working, None, 0.8);
        viz.update_agent("Coder", AgentState::Done, None, 1.0);
        viz.update_agent("Tester", AgentState::Working, None, 0.4);

        let compact = viz.render_compact();
        assert!(compact.contains("2/3 active"));
        assert!(compact.contains("Architect:80%"));
        assert!(compact.contains("Coder:done"));
        assert!(compact.contains("Tester:40%"));
    }

    #[test]
    fn test_render_compact_all_states() {
        let mut viz = SwarmVisualization::new();
        viz.add_agent("A");
        viz.add_agent("B");
        viz.add_agent("C");
        viz.add_agent("D");
        viz.add_agent("E");

        viz.update_agent("A", AgentState::Idle, None, 0.0);
        viz.update_agent("B", AgentState::Working, None, 0.5);
        viz.update_agent("C", AgentState::Waiting, None, 0.0);
        viz.update_agent("D", AgentState::Done, None, 1.0);
        viz.update_agent("E", AgentState::Failed, None, 0.0);

        let compact = viz.render_compact();
        assert!(compact.contains("A:idle"));
        assert!(compact.contains("B:50%"));
        assert!(compact.contains("C:wait"));
        assert!(compact.contains("D:done"));
        assert!(compact.contains("E:FAIL"));
        // B (Working) + C (Waiting) = 2 active
        assert!(compact.contains("2/5 active"));
    }

    // ── Single agent progress ───────────────────────────────────────

    #[test]
    fn test_render_agent_progress_found() {
        let mut viz = SwarmVisualization::new();
        viz.add_agent("Tester");
        viz.update_agent("Tester", AgentState::Working, None, 0.4);

        let line = viz.render_agent_progress("Tester");
        assert!(line.contains("Tester"));
        assert!(line.contains("40%"));
        assert!(line.contains("Working"));
        assert!(line.contains('['));
    }

    #[test]
    fn test_render_agent_progress_not_found() {
        let viz = SwarmVisualization::new();
        let line = viz.render_agent_progress("Ghost");
        assert!(line.is_empty());
    }

    #[test]
    fn test_render_agent_progress_done() {
        let mut viz = SwarmVisualization::new();
        viz.add_agent("Coder");
        viz.update_agent("Coder", AgentState::Done, None, 1.0);

        let line = viz.render_agent_progress("Coder");
        assert!(line.contains("Done"));
        assert!(line.contains("\u{2713}")); // ✓
    }

    #[test]
    fn test_render_agent_progress_failed() {
        let mut viz = SwarmVisualization::new();
        viz.add_agent("Security");
        viz.update_agent("Security", AgentState::Failed, None, 0.3);

        let line = viz.render_agent_progress("Security");
        assert!(line.contains("Failed"));
        assert!(line.contains("\u{2717}")); // ✗
    }

    // ── Full rendering ──────────────────────────────────────────────

    #[test]
    fn test_render_full_contains_all_panels() {
        let mut viz = SwarmVisualization::new();
        viz.add_agent("Architect");
        viz.add_agent("Coder");
        viz.update_agent("Architect", AgentState::Working, Some("planning"), 0.8);
        viz.update_agent("Coder", AgentState::Done, None, 1.0);

        viz.record_consensus(ConsensusEvent {
            timestamp: Instant::now(),
            topic: "REST vs GraphQL".to_string(),
            strategy: "majority".to_string(),
            votes: vec![("Architect".to_string(), true), ("Coder".to_string(), true)],
            outcome: true,
        });

        let full = viz.render_full();
        assert!(full.contains("Swarm Status"));
        assert!(full.contains("Consensus Log"));
        assert!(full.contains("Timeline"));
        assert!(full.contains("Architect"));
        assert!(full.contains("Coder"));
        assert!(full.contains("REST vs GraphQL"));
    }

    #[test]
    fn test_render_full_empty_swarm() {
        let viz = SwarmVisualization::new();
        let full = viz.render_full();
        assert!(full.contains("Swarm Status"));
        assert!(full.contains("Consensus Log"));
        assert!(full.contains("Timeline"));
        assert!(full.contains("No consensus events yet"));
        assert!(full.contains("No activity yet"));
    }

    // ── Panel borders ───────────────────────────────────────────────

    #[test]
    fn test_agent_panel_has_borders() {
        let mut viz = SwarmVisualization::new();
        viz.add_agent("Coder");

        let panel = viz.render_agent_panel();
        assert!(panel.contains(Glyphs::corner_tl()));
        assert!(panel.contains(Glyphs::corner_tr()));
        assert!(panel.contains(Glyphs::corner_bl()));
        assert!(panel.contains(Glyphs::corner_br()));
    }

    #[test]
    fn test_consensus_panel_has_borders() {
        let viz = SwarmVisualization::new();
        let panel = viz.render_consensus_panel();
        assert!(panel.contains(Glyphs::corner_tl()));
        assert!(panel.contains(Glyphs::corner_br()));
    }

    #[test]
    fn test_timeline_panel_has_borders() {
        let viz = SwarmVisualization::new();
        let panel = viz.render_timeline_panel();
        assert!(panel.contains(Glyphs::corner_tl()));
        assert!(panel.contains(Glyphs::corner_br()));
    }

    // ── Consensus panel rendering ───────────────────────────────────

    #[test]
    fn test_consensus_panel_shows_votes() {
        let mut viz = SwarmVisualization::new();
        viz.record_consensus(ConsensusEvent {
            timestamp: viz.start_time,
            topic: "Architecture choice".to_string(),
            strategy: "unanimous".to_string(),
            votes: vec![
                ("Architect".to_string(), true),
                ("Coder".to_string(), false),
            ],
            outcome: false,
        });

        let panel = viz.render_consensus_panel();
        assert!(panel.contains("Architecture choice"));
        assert!(panel.contains("Architect:\u{2713}"));
        assert!(panel.contains("Coder:\u{2717}"));
        assert!(panel.contains("rejected"));
    }

    #[test]
    fn test_consensus_panel_approved() {
        let mut viz = SwarmVisualization::new();
        viz.record_consensus(ConsensusEvent {
            timestamp: viz.start_time,
            topic: "Use REST".to_string(),
            strategy: "majority".to_string(),
            votes: vec![("Architect".to_string(), true), ("Coder".to_string(), true)],
            outcome: true,
        });

        let panel = viz.render_consensus_panel();
        assert!(panel.contains("approved"));
    }

    // ── Timeline rendering ──────────────────────────────────────────

    #[test]
    fn test_timeline_shows_entries() {
        let mut viz = SwarmVisualization::new();
        viz.add_agent("Architect");
        viz.record_tool_call("Architect", "file_read src/main.rs");

        let panel = viz.render_timeline_panel();
        assert!(panel.contains("Architect"));
        assert!(panel.contains("file_read"));
        // Should contain the bullet marker
        assert!(panel.contains("\u{25B8}")); // ▸
    }

    #[test]
    fn test_timeline_limits_to_10_entries() {
        let mut viz = SwarmVisualization::new();
        viz.add_agent("Coder");
        for i in 0..15 {
            viz.record_tool_call("Coder", &format!("tool_{}", i));
        }

        let panel = viz.render_timeline_panel();
        // Should contain the most recent entries
        assert!(panel.contains("tool_14"));
        // The earliest entries should be truncated from view
        // (though they're still in the vec)
        assert!(viz.timeline.len() > 10);
    }

    // ── Helpers ─────────────────────────────────────────────────────

    #[test]
    fn test_format_elapsed_compact() {
        assert_eq!(format_elapsed_compact(Duration::from_secs(0)), "0:00");
        assert_eq!(format_elapsed_compact(Duration::from_secs(5)), "0:05");
        assert_eq!(format_elapsed_compact(Duration::from_secs(23)), "0:23");
        assert_eq!(format_elapsed_compact(Duration::from_secs(60)), "1:00");
        assert_eq!(format_elapsed_compact(Duration::from_secs(105)), "1:45");
        assert_eq!(format_elapsed_compact(Duration::from_secs(3661)), "61:01");
    }

    #[test]
    fn test_pad_right() {
        assert_eq!(pad_right("foo", 6), "foo   ");
        assert_eq!(pad_right("hello", 5), "hello");
        assert_eq!(pad_right("longer", 3), "longer");
        assert_eq!(pad_right("", 4), "    ");
    }

    #[test]
    fn test_display_width() {
        assert_eq!(display_width("hello"), 5);
        assert_eq!(display_width(""), 0);
        assert_eq!(display_width("abc"), 3);
    }

    // ── AgentState Display ──────────────────────────────────────────

    #[test]
    fn test_agent_state_display() {
        assert_eq!(format!("{}", AgentState::Idle), "Idle");
        assert_eq!(format!("{}", AgentState::Working), "Working");
        assert_eq!(format!("{}", AgentState::Waiting), "Waiting");
        assert_eq!(format!("{}", AgentState::Done), "Done");
        assert_eq!(format!("{}", AgentState::Failed), "Failed");
    }

    // ── Role icons ──────────────────────────────────────────────────

    #[test]
    fn test_role_icons() {
        let roles = [
            "Architect",
            "Coder",
            "Tester",
            "Reviewer",
            "DevOps",
            "Security",
            "Documenter",
            "Performance",
            "VisualCritic",
            "Unknown",
        ];
        for role in roles {
            let agent = AgentStatus::new(role);
            assert!(!agent.role_icon().is_empty());
        }
    }

    // ── Dominant strategy ───────────────────────────────────────────

    #[test]
    fn test_dominant_strategy_empty() {
        let viz = SwarmVisualization::new();
        assert_eq!(viz.dominant_strategy(), "none");
    }

    #[test]
    fn test_dominant_strategy_single() {
        let mut viz = SwarmVisualization::new();
        viz.record_consensus(ConsensusEvent {
            timestamp: Instant::now(),
            topic: "test".to_string(),
            strategy: "weighted".to_string(),
            votes: vec![],
            outcome: true,
        });
        assert_eq!(viz.dominant_strategy(), "weighted");
    }

    #[test]
    fn test_dominant_strategy_majority_wins() {
        let mut viz = SwarmVisualization::new();
        for strategy in &["majority", "majority", "unanimous"] {
            viz.record_consensus(ConsensusEvent {
                timestamp: Instant::now(),
                topic: "test".to_string(),
                strategy: strategy.to_string(),
                votes: vec![],
                outcome: true,
            });
        }
        assert_eq!(viz.dominant_strategy(), "majority");
    }

    // ── State suffix ────────────────────────────────────────────────

    #[test]
    fn test_state_suffix() {
        let mut agent = AgentStatus::new("Test");

        agent.state = AgentState::Done;
        assert_eq!(agent.state_suffix(), " \u{2713}");

        agent.state = AgentState::Failed;
        assert_eq!(agent.state_suffix(), " \u{2717}");

        agent.state = AgentState::Working;
        assert_eq!(agent.state_suffix(), "");

        agent.state = AgentState::Idle;
        assert_eq!(agent.state_suffix(), "");

        agent.state = AgentState::Waiting;
        assert_eq!(agent.state_suffix(), "");
    }

    // ── Integration: full workflow ──────────────────────────────────

    #[test]
    fn test_full_workflow() {
        let mut viz = SwarmVisualization::new();

        // Add agents
        viz.add_agent("Architect");
        viz.add_agent("Coder");
        viz.add_agent("Tester");
        viz.add_agent("Reviewer");
        viz.add_agent("Security");

        // Architect starts planning
        viz.update_agent("Architect", AgentState::Working, Some("planning"), 0.2);
        viz.record_tool_call("Architect", "file_read src/main.rs");
        viz.advance_tick();

        // Progress
        viz.update_agent("Architect", AgentState::Working, Some("planning"), 0.8);

        // Consensus
        viz.record_consensus(ConsensusEvent {
            timestamp: Instant::now(),
            topic: "REST vs GraphQL".to_string(),
            strategy: "majority".to_string(),
            votes: vec![
                ("Architect".to_string(), true),
                ("Coder".to_string(), true),
                ("Reviewer".to_string(), false),
            ],
            outcome: true,
        });

        // Architect done, Coder starts
        viz.update_agent("Architect", AgentState::Done, None, 1.0);
        viz.update_agent("Coder", AgentState::Working, Some("implementing"), 0.3);
        viz.record_tool_call("Coder", "file_write src/api.rs");
        viz.advance_tick();

        // Tester starts
        viz.update_agent("Tester", AgentState::Working, Some("test generation"), 0.1);

        // Render everything
        let full = viz.render_full();
        assert!(full.contains("Swarm Status"));
        assert!(full.contains("Architect"));
        assert!(full.contains("Done"));
        assert!(full.contains("Coder"));
        assert!(full.contains("Working"));
        assert!(full.contains("REST vs GraphQL"));
        assert!(full.contains("Timeline"));

        let compact = viz.render_compact();
        assert!(compact.contains("Swarm:"));
        assert!(compact.contains("Architect:done"));

        // Verify agent progress rendering
        let arch_progress = viz.render_agent_progress("Architect");
        assert!(arch_progress.contains("100%"));
        assert!(arch_progress.contains("\u{2713}")); // ✓
    }
}
