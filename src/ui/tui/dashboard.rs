use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Row, Table},
    Frame,
};
use std::time::Duration;

use crate::orchestration::swarm::Swarm;

/// TUI Dashboard for displaying swarm and observability metrics
pub struct Dashboard {
    last_update: std::time::Instant,
}

impl Dashboard {
    pub fn new() -> Self {
        Self {
            last_update: std::time::Instant::now(),
        }
    }

    pub fn render(&mut self, frame: &mut Frame, swarm: &Swarm) {
        let area = frame.area();

        // Split area into main content and sidebar
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(area);

        // Main swarm view
        let swarm_chunk = chunks[0];
        self.render_swarm_view(frame, swarm_chunk, swarm);

        // Sidebar with metrics
        let sidebar_chunk = chunks[1];
        self.render_sidebar(frame, sidebar_chunk, swarm);
    }

    fn render_swarm_view(&self, frame: &mut Frame, area: Rect, swarm: &Swarm) {
        let agents: Vec<&crate::orchestration::swarm::Agent> = swarm.list_agents();

        // Create agent list items
        let items: Vec<ListItem> = agents
            .iter()
            .map(|agent| {
                let status = if agent.is_active() {
                    Span::styled("● Active", Style::default().fg(Color::Green))
                } else {
                    Span::styled("○ Idle", Style::default().fg(Color::Yellow))
                };

                let role = Span::styled(
                    format!("[{}]", agent.role),
                    Style::default().add_modifier(Modifier::BOLD),
                );

                let name = Span::raw(format!(" {}", agent.name));

                ListItem::new(vec![status, Span::raw(" "), role, name])
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Active Agents")
            )
            .style(Style::default().fg(Color::White));

        frame.render_widget(list, area);
    }

    fn render_sidebar(&self, frame: &mut Frame, area: Rect, swarm: &Swarm) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(30),
                Constraint::Percentage(30),
                Constraint::Percentage(40),
            ])
            .split(area);

        // Agent count
        let agent_count = swarm.list_agents().len();
        let count_text = Paragraph::new(format!("Agents: {}", agent_count))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Summary")
            );
        frame.render_widget(count_text, chunks[0]);

        // Agent roles
        let mut role_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for agent in swarm.list_agents() {
            let role = format!("{:?}", agent.role);
            *role_counts.entry(role).or_insert(0) += 1;
        }

        let role_items: Vec<ListItem> = role_counts
            .iter()
            .map(|(role, count)| {
                ListItem::new(format!("{}: {}", role, count))
            })
            .collect();

        let roles_list = List::new(role_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("By Role")
            );
        frame.render_widget(roles_list, chunks[1]);

        // Uptime
        let uptime = self.last_update.elapsed();
        let uptime_text = Paragraph::new(format!("Uptime: {:?}", uptime))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Dashboard")
            );
        frame.render_widget(uptime_text, chunks[2]);
    }

    pub fn update(&mut self) {
        self.last_update = std::time::Instant::now();
    }
}

impl Default for Dashboard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dashboard_creation() {
        let dashboard = Dashboard::new();
        assert!(dashboard.last_update.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn test_dashboard_default() {
        let dashboard = Dashboard::default();
        assert!(dashboard.last_update.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn test_dashboard_update() {
        let mut dashboard = Dashboard::new();
        std::thread::sleep(Duration::from_millis(10));
        dashboard.update();
        assert!(dashboard.last_update.elapsed() < Duration::from_millis(10));
    }
}
