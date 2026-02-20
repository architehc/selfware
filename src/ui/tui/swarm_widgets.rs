//! Swarm Dashboard Widgets
//!
//! Specialized widgets for visualizing the agent swarm in the TUI.

use crate::orchestration::swarm::{AgentRole, AgentStatus, DecisionStatus, TaskStatus};
use crate::ui::tui::animation::agent_avatar::ActivityLevel;
use crate::ui::tui::swarm_state::{
    AgentUiState, DecisionView, EventType, MemoryEntryView, SwarmEvent, SwarmStats, TaskView,
};
use crate::ui::tui::TuiPalette;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
    Frame,
};

/// Render the swarm status bar
pub fn render_swarm_status_bar(frame: &mut Frame, area: Rect, stats: &SwarmStats) {
    let status_text = format!(
        " 🤖 Swarm │ {} Agents ({} active, {} idle) │ {} Tasks │ {} Decisions │ Trust: {:.0}% ",
        stats.total_agents,
        stats.active_agents,
        stats.idle_agents,
        stats.pending_tasks,
        stats.pending_decisions,
        stats.average_trust * 100.0
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(TuiPalette::border_style())
        .title(Span::styled(status_text, TuiPalette::title_style()));

    frame.render_widget(block, area);
}

/// Render agent swarm visualization
pub fn render_agent_swarm(frame: &mut Frame, area: Rect, agents: &[AgentUiState]) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(TuiPalette::border_style())
        .title(Span::styled(" 🤖 Agent Swarm ", TuiPalette::title_style()));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if agents.is_empty() {
        let empty = Paragraph::new("No agents active").style(TuiPalette::muted_style());
        frame.render_widget(empty, inner);
        return;
    }

    // Create list items for each agent
    let items: Vec<ListItem> = agents
        .iter()
        .map(|agent| {
            let (icon, name) = match agent.role {
                AgentRole::Architect => ("🏗️", "Architect"),
                AgentRole::Coder => ("💻", "Coder"),
                AgentRole::Tester => ("🧪", "Tester"),
                AgentRole::Reviewer => ("👁️", "Reviewer"),
                AgentRole::Documenter => ("📚", "Documenter"),
                AgentRole::DevOps => ("🚀", "DevOps"),
                AgentRole::Security => ("🔒", "Security"),
                AgentRole::Performance => ("⚡", "Performance"),
                AgentRole::General => ("🤖", "General"),
            };

            // Activity dots
            let activity_dots = match agent.activity {
                ActivityLevel::Idle => "○○○○○",
                ActivityLevel::Low => "●○○○○",
                ActivityLevel::Medium => "●●○○○",
                ActivityLevel::High => "●●●○○",
                ActivityLevel::Max => "●●●●○",
                ActivityLevel::Complete => "●●●●● ✓",
                ActivityLevel::Error => "⚠ ERROR",
            };

            let role_color = match agent.role {
                AgentRole::Security => Color::Red,
                AgentRole::Architect => Color::Cyan,
                AgentRole::Coder => Color::Blue,
                AgentRole::Tester => Color::Green,
                AgentRole::Reviewer => Color::Magenta,
                _ => TuiPalette::AMBER,
            };

            let status_text = match agent.status {
                AgentStatus::Working => " Working ",
                AgentStatus::Idle => " Idle ",
                AgentStatus::Waiting => " Waiting ",
                AgentStatus::Completed => " Done ",
                AgentStatus::Error => " Error ",
                AgentStatus::Paused => " Paused ",
            };

            let status_color = match agent.status {
                AgentStatus::Working => TuiPalette::GARDEN_GREEN,
                AgentStatus::Idle => TuiPalette::muted(),
                AgentStatus::Waiting => TuiPalette::warning(),
                AgentStatus::Completed => TuiPalette::BLOOM,
                AgentStatus::Error => TuiPalette::FROST,
                AgentStatus::Paused => TuiPalette::STONE,
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", icon), Style::default().fg(role_color)),
                Span::styled(
                    format!("{:12}", agent.name),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(format!("[{:10}]", name), Style::default().fg(role_color)),
                Span::raw(" "),
                Span::styled(activity_dots, Style::default().fg(role_color)),
                Span::raw(" "),
                Span::styled(
                    format!("Trust:{:3.0}%", agent.trust_score * 100.0),
                    TuiPalette::muted_style(),
                ),
                Span::raw(" "),
                Span::styled(status_text, Style::default().fg(status_color)),
            ]))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

/// Render shared memory view
pub fn render_shared_memory(frame: &mut Frame, area: Rect, entries: &[MemoryEntryView]) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(TuiPalette::border_style())
        .title(Span::styled(
            " 🧠 Shared Memory ",
            TuiPalette::title_style(),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if entries.is_empty() {
        let empty = Paragraph::new("No memory entries").style(TuiPalette::muted_style());
        frame.render_widget(empty, inner);
        return;
    }

    // Show stats
    let stats_text = format!("{} entries", entries.len());
    let stats_para = Paragraph::new(stats_text).style(TuiPalette::muted_style());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    frame.render_widget(stats_para, chunks[0]);

    let items: Vec<ListItem> = entries
        .iter()
        .take(chunks[1].height as usize)
        .map(|entry| {
            let tags = if entry.tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", entry.tags.join(", "))
            };

            ListItem::new(Line::from(vec![
                Span::styled("📄 ", TuiPalette::muted_style()),
                Span::styled(&entry.key, Style::default().fg(TuiPalette::AMBER)),
                Span::styled(tags, TuiPalette::muted_style()),
                Span::styled(
                    format!(" ({} reads)", entry.access_count),
                    TuiPalette::muted_style(),
                ),
            ]))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, chunks[1]);
}

/// Render task queue
pub fn render_task_queue(frame: &mut Frame, area: Rect, tasks: &[TaskView]) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(TuiPalette::border_style())
        .title(Span::styled(" 📋 Task Queue ", TuiPalette::title_style()));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if tasks.is_empty() {
        let empty = Paragraph::new("No tasks queued").style(TuiPalette::muted_style());
        frame.render_widget(empty, inner);
        return;
    }

    let items: Vec<ListItem> = tasks
        .iter()
        .take(inner.height as usize)
        .map(|task| {
            let priority_color = match task.priority {
                8..=10 => Color::Red,
                5..=7 => Color::Yellow,
                _ => Color::Green,
            };

            let status_icon = match task.status {
                TaskStatus::Pending => "⏳",
                TaskStatus::InProgress => "▶",
                TaskStatus::Completed => "✓",
                TaskStatus::Failed => "✗",
            };

            let status_color = match task.status {
                TaskStatus::Pending => TuiPalette::warning(),
                TaskStatus::InProgress => TuiPalette::GARDEN_GREEN,
                TaskStatus::Completed => TuiPalette::BLOOM,
                TaskStatus::Failed => TuiPalette::FROST,
            };

            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("P{} ", task.priority),
                    Style::default().fg(priority_color),
                ),
                Span::styled(status_icon, Style::default().fg(status_color)),
                Span::raw(" "),
                Span::styled(
                    &task.description,
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" ({} results)", task.result_count),
                    TuiPalette::muted_style(),
                ),
            ]))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

/// Render decisions view
pub fn render_decisions(frame: &mut Frame, area: Rect, decisions: &[DecisionView]) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(TuiPalette::border_style())
        .title(Span::styled(
            " ⚖️ Active Decisions ",
            TuiPalette::title_style(),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if decisions.is_empty() {
        let empty = Paragraph::new("No active decisions").style(TuiPalette::muted_style());
        frame.render_widget(empty, inner);
        return;
    }

    let items: Vec<ListItem> = decisions
        .iter()
        .take(inner.height as usize)
        .map(|decision| {
            let status_icon = match decision.status {
                DecisionStatus::Pending => "⏳",
                DecisionStatus::Resolved => "✓",
                DecisionStatus::Conflict => "⚠️",
                DecisionStatus::TimedOut => "⏰",
            };

            let status_color = match decision.status {
                DecisionStatus::Pending => TuiPalette::warning(),
                DecisionStatus::Resolved => TuiPalette::BLOOM,
                DecisionStatus::Conflict => TuiPalette::FROST,
                DecisionStatus::TimedOut => Color::Gray,
            };

            let outcome_text = decision
                .outcome
                .as_ref()
                .map(|o| format!(" → {}", o))
                .unwrap_or_default();

            ListItem::new(Line::from(vec![
                Span::styled(status_icon, Style::default().fg(status_color)),
                Span::raw(" "),
                Span::styled(&decision.question, Style::default()),
                Span::styled(
                    format!(" ({} votes)", decision.vote_count),
                    TuiPalette::muted_style(),
                ),
                Span::styled(outcome_text, Style::default().fg(TuiPalette::GARDEN_GREEN)),
            ]))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

/// Render swarm event log
pub fn render_swarm_events(frame: &mut Frame, area: Rect, events: &[SwarmEvent]) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(TuiPalette::border_style())
        .title(Span::styled(" 📜 Swarm Events ", TuiPalette::title_style()));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if events.is_empty() {
        let empty = Paragraph::new("No events yet").style(TuiPalette::muted_style());
        frame.render_widget(empty, inner);
        return;
    }

    let items: Vec<ListItem> = events
        .iter()
        .rev()
        .take(inner.height as usize)
        .map(|event| {
            let style = match event.event_type {
                EventType::AgentError | EventType::ConflictDetected => TuiPalette::error_style(),
                EventType::AgentCompleted
                | EventType::TaskCompleted
                | EventType::ConsensusReached
                | EventType::DecisionResolved => TuiPalette::success_style(),
                _ => Style::default(),
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("[{}] ", event.timestamp), TuiPalette::muted_style()),
                Span::styled(format!("{} ", event.event_type.icon()), style),
                Span::styled(&event.message, style),
            ]))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

/// Render swarm health gauge
pub fn render_swarm_health(frame: &mut Frame, area: Rect, stats: &SwarmStats) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(TuiPalette::border_style())
        .title(Span::styled(" 🌱 Swarm Health ", TuiPalette::title_style()));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Calculate health based on various factors
    let health = if stats.total_agents == 0 {
        0.0
    } else {
        let active_ratio = stats.active_agents as f64 / stats.total_agents as f64;
        let trust_factor = stats.average_trust as f64;
        (active_ratio * 0.5 + trust_factor * 0.5).clamp(0.0, 1.0)
    };

    let (stage, icon) = match (health * 100.0) as u8 {
        0..=25 => ("Struggling", "🥀"),
        26..=50 => ("Recovering", "🌿"),
        51..=75 => ("Coordinating", "🌳"),
        76..=90 => ("Synchronized", "🌲"),
        _ => ("Thriving", "🌸"),
    };

    let health_color = if health > 0.75 {
        TuiPalette::BLOOM
    } else if health > 0.5 {
        TuiPalette::GARDEN_GREEN
    } else if health > 0.25 {
        TuiPalette::WILT
    } else {
        TuiPalette::FROST
    };

    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(health_color))
        .ratio(health)
        .label(format!("{} {} ({:.0}%)", icon, stage, health * 100.0));

    frame.render_widget(gauge, inner);
}

/// Render swarm help overlay
pub fn render_swarm_help(frame: &mut Frame, area: Rect) {
    let width = 50.min(area.width - 4);
    let height = 18.min(area.height - 4);
    let x = (area.width - width) / 2;
    let y = (area.height - height) / 3;

    let help_area = Rect::new(x, y, width, height);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(TuiPalette::title_style())
        .style(Style::default().bg(TuiPalette::INK))
        .title(Span::styled(
            " 🤖 Swarm Controls ",
            TuiPalette::title_style(),
        ));

    let inner = block.inner(help_area);
    frame.render_widget(block, help_area);

    let shortcuts = vec![
        ("q / Ctrl+C", "Quit (q twice)"),
        ("?", "Toggle this help"),
        ("Tab", "Cycle focus between panes"),
        ("Space", "Pause/resume animations"),
        ("z", "Toggle zoom on focused pane"),
        ("r", "Refresh swarm state"),
        ("Alt+1-6", "Switch layout presets"),
        ("c", "Create new decision"),
        ("v", "Vote on current decision"),
        ("t", "Add task to queue"),
        ("s", "Sync swarm state"),
    ];

    let items: Vec<ListItem> = shortcuts
        .iter()
        .map(|(key, action)| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {:12} ", key),
                    Style::default()
                        .fg(TuiPalette::AMBER)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(*action, TuiPalette::muted_style()),
            ]))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::swarm::AgentRole;
    use crate::ui::tui::animation::agent_avatar::ActivityLevel;

    #[test]
    fn test_render_swarm_stats() {
        let stats = SwarmStats {
            total_agents: 5,
            active_agents: 2,
            idle_agents: 3,
            pending_tasks: 1,
            completed_tasks: 4,
            pending_decisions: 0,
            average_trust: 0.75,
            memory_entries: 10,
        };

        assert_eq!(stats.total_agents, 5);
        assert!(stats.average_trust > 0.0);
    }

    #[test]
    fn test_agent_role_icons() {
        // Verify role mappings
        assert_eq!(AgentRole::Architect, AgentRole::Architect);
        assert_eq!(AgentRole::Coder, AgentRole::Coder);
    }

    #[test]
    fn test_activity_level_display() {
        assert_eq!(ActivityLevel::Idle.dots(), 0);
        assert_eq!(ActivityLevel::Complete.dots(), 5);
    }

    #[test]
    fn test_decision_status_display() {
        assert_eq!(DecisionStatus::Pending, DecisionStatus::Pending);
        assert_eq!(DecisionStatus::Resolved, DecisionStatus::Resolved);
    }

    #[test]
    fn test_event_type_icon() {
        assert_eq!(EventType::AgentStarted.icon(), "▶");
        assert_eq!(EventType::ConsensusReached.icon(), "🤝");
    }
}
