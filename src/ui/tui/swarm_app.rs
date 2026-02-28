//! Swarm TUI Application
//!
//! Main application for the agent swarm TUI mode.

use crate::orchestration::swarm::{create_dev_swarm, AgentRole, Swarm, SwarmTask};
use crate::ui::tui::layout::{LayoutEngine, LayoutPreset, PaneType};
use crate::ui::tui::swarm_state::{EventType, SwarmEvent, SwarmUiState};
use crate::ui::tui::swarm_widgets::*;
use crate::ui::tui::TuiPalette;
use crossterm::event::{Event, KeyCode, KeyModifiers};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    widgets::Paragraph,
    Frame,
};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Application state for swarm UI
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwarmAppState {
    Running,
    Paused,
    Help,
    CreatingDecision,
    Voting,
}

/// Swarm UI application
pub struct SwarmApp {
    pub state: SwarmAppState,
    pub swarm_state: SwarmUiState,
    pub layout_engine: LayoutEngine,
    pub show_help: bool,
    pub last_sync: Instant,
    pub sync_interval: Duration,
    pub selected_decision: usize,
    pub input_buffer: String,
    /// Tracks the last time 'q' was pressed for double-tap quit
    pub last_q_press: Option<Instant>,
    /// Currently focused agent role name. None = show all agents, Some(name) = show only this agent.
    pub focused_agent: Option<String>,
    /// Index of the currently selected agent in the agent list (for keyboard navigation).
    pub selected_agent: usize,
}

impl Default for SwarmApp {
    fn default() -> Self {
        Self::new()
    }
}

impl SwarmApp {
    /// Create new swarm app with default dev swarm
    pub fn new() -> Self {
        let swarm = Arc::new(RwLock::new(create_dev_swarm()));
        Self::with_swarm(swarm)
    }

    /// Create swarm app with custom swarm
    pub fn with_swarm(swarm: Arc<RwLock<Swarm>>) -> Self {
        let mut layout_engine = LayoutEngine::new();
        layout_engine.apply_preset(LayoutPreset::Dashboard);

        let mut app = Self {
            state: SwarmAppState::Running,
            swarm_state: SwarmUiState::new(swarm),
            layout_engine,
            show_help: false,
            last_sync: Instant::now(),
            sync_interval: Duration::from_millis(500),
            selected_decision: 0,
            input_buffer: String::new(),
            last_q_press: None,
            focused_agent: None,
            selected_agent: 0,
        };

        // Initial sync
        app.swarm_state.sync();
        app.swarm_state
            .add_event(EventType::AgentStarted, "Swarm UI initialized", None);

        app
    }

    /// Create swarm app with custom configuration
    pub fn with_config(roles: Vec<AgentRole>) -> Self {
        let mut swarm = Swarm::new();

        for (i, role) in roles.iter().enumerate() {
            let name = format!("{}-{}", role.name(), i + 1);
            swarm.add_agent(crate::orchestration::swarm::Agent::new(name, *role));
        }

        let swarm = Arc::new(RwLock::new(swarm));
        Self::with_swarm(swarm)
    }

    /// Render the application
    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // Calculate layouts
        let layouts = self.layout_engine.calculate_layout(area);

        // Render each pane based on its type
        for (pane_id, pane_area) in &layouts {
            if let Some(pane) = self.layout_engine.get_pane(*pane_id) {
                match pane.pane_type {
                    PaneType::StatusBar => {
                        render_swarm_status_bar(frame, *pane_area, &self.swarm_state.stats);
                    }
                    PaneType::Chat => {
                        render_agent_swarm(
                            frame,
                            *pane_area,
                            &self.swarm_state.agents,
                            self.selected_agent,
                            self.focused_agent.as_deref(),
                        );
                    }
                    PaneType::GardenView => {
                        render_shared_memory(frame, *pane_area, &self.swarm_state.memory_entries);
                    }
                    PaneType::ActiveTools => {
                        render_task_queue(frame, *pane_area, &self.swarm_state.tasks);
                    }
                    PaneType::Logs => {
                        let events_to_show: Vec<&SwarmEvent> =
                            if let Some(ref role) = self.focused_agent {
                                self.swarm_state
                                    .events
                                    .iter()
                                    .filter(|e| e.agent_id.as_deref() == Some(role.as_str()))
                                    .collect()
                            } else {
                                self.swarm_state.events.iter().collect()
                            };
                        let title = if let Some(ref role) = self.focused_agent {
                            format!(" Events [{}] (Esc to unfocus) ", role)
                        } else {
                            " Swarm Events ".to_string()
                        };
                        render_swarm_events_filtered(frame, *pane_area, &events_to_show, &title);
                    }
                    PaneType::GardenHealth => {
                        render_swarm_health(frame, *pane_area, &self.swarm_state.stats);
                    }
                    PaneType::Diff => {
                        render_decisions(frame, *pane_area, &self.swarm_state.decisions);
                    }
                    _ => {
                        // Render placeholder for other pane types
                        let block = ratatui::widgets::Block::default()
                            .borders(ratatui::widgets::Borders::ALL)
                            .title(pane.pane_type.title());
                        frame.render_widget(block, *pane_area);
                    }
                }
            }
        }

        // Render help overlay if active
        if self.show_help {
            render_swarm_help(frame, area);
        }

        // Render pause indicator if paused
        if self.state == SwarmAppState::Paused {
            render_pause_indicator(frame, area);
        }
    }

    /// Handle tick/update
    pub fn on_tick(&mut self) {
        if self.state != SwarmAppState::Paused {
            // Sync with swarm at intervals
            if self.last_sync.elapsed() >= self.sync_interval {
                self.swarm_state.sync();
                self.last_sync = Instant::now();

                // Check for new events in swarm
                self.detect_swarm_events();
            }
        }
    }

    /// Detect and log new swarm events
    fn detect_swarm_events(&mut self) {
        // This would check the swarm for new events and add them to the log
        // For now, we'll just ensure sync happened
    }

    /// Handle keyboard events
    pub fn handle_event(&mut self, event: Event) -> bool {
        if let Event::Key(key) = event {
            // Handle help mode
            if self.show_help {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('?') => {
                        self.show_help = false;
                        return true;
                    }
                    _ => return true, // Consume all input while help is shown
                }
            }

            match key.code {
                // Quit immediately on Ctrl+C
                KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                    return false;
                }
                // Double-tap q to quit (prevents accidental quits)
                KeyCode::Char('q') => {
                    let now = Instant::now();
                    let timeout = Duration::from_secs(2);
                    if let Some(last) = self.last_q_press {
                        if now.duration_since(last) <= timeout {
                            self.last_q_press = None;
                            return false;
                        }
                    }
                    self.last_q_press = Some(now);
                    self.swarm_state.add_event(
                        EventType::AgentStarted,
                        "Press q again within 2s to quit",
                        None,
                    );
                    return true;
                }

                // Help
                KeyCode::Char('?') => {
                    self.show_help = !self.show_help;
                }

                // Pause/Resume
                KeyCode::Char(' ') => {
                    self.toggle_pause();
                }

                // Refresh
                KeyCode::Char('r') => {
                    self.swarm_state.sync();
                    self.swarm_state
                        .add_event(EventType::AgentStarted, "Manual refresh", None);
                }

                // Layout presets
                KeyCode::Char('1') if key.modifiers == KeyModifiers::ALT => {
                    self.layout_engine.apply_preset(LayoutPreset::Focus);
                    self.swarm_state
                        .add_event(EventType::AgentStarted, "Layout: Focus", None);
                }
                KeyCode::Char('2') if key.modifiers == KeyModifiers::ALT => {
                    self.layout_engine.apply_preset(LayoutPreset::Coding);
                    self.swarm_state
                        .add_event(EventType::AgentStarted, "Layout: Coding", None);
                }
                KeyCode::Char('3') if key.modifiers == KeyModifiers::ALT => {
                    self.layout_engine.apply_preset(LayoutPreset::Dashboard);
                    self.swarm_state
                        .add_event(EventType::AgentStarted, "Layout: Dashboard", None);
                }

                // Focus cycling
                KeyCode::Tab => {
                    self.layout_engine.focus_next();
                }
                KeyCode::BackTab => {
                    self.layout_engine.focus_prev();
                }

                // Zoom
                KeyCode::Char('z') => {
                    self.layout_engine.toggle_zoom();
                }

                // Unzoom or unfocus agent
                KeyCode::Esc => {
                    if self.focused_agent.is_some() {
                        self.focused_agent = None;
                        self.swarm_state.add_event(
                            EventType::AgentStarted,
                            "Unfocused agent (showing all)",
                            None,
                        );
                    } else if self.layout_engine.is_zoomed() {
                        self.layout_engine.toggle_zoom();
                    }
                }

                // Focus on selected agent
                KeyCode::Enter => {
                    if let Some(agent) = self.swarm_state.agents.get(self.selected_agent) {
                        let name = agent.name.clone();
                        self.focused_agent = Some(name.clone());
                        self.swarm_state.add_event(
                            EventType::AgentStarted,
                            format!("Focused on agent: {}", name),
                            None,
                        );
                    }
                }

                // Navigate agent list
                KeyCode::Up => {
                    if !self.swarm_state.agents.is_empty() {
                        if self.selected_agent > 0 {
                            self.selected_agent -= 1;
                        } else {
                            self.selected_agent = self.swarm_state.agents.len() - 1;
                        }
                    }
                }
                KeyCode::Down => {
                    if !self.swarm_state.agents.is_empty() {
                        if self.selected_agent < self.swarm_state.agents.len() - 1 {
                            self.selected_agent += 1;
                        } else {
                            self.selected_agent = 0;
                        }
                    }
                }

                // Quick-focus agent by index (1-9)
                KeyCode::Char(c)
                    if c.is_ascii_digit() && c != '0' && key.modifiers == KeyModifiers::NONE =>
                {
                    let idx = (c as usize) - ('1' as usize);
                    if let Some(agent) = self.swarm_state.agents.get(idx) {
                        let name = agent.name.clone();
                        self.selected_agent = idx;
                        self.focused_agent = Some(name.clone());
                        self.swarm_state.add_event(
                            EventType::AgentStarted,
                            format!("Quick-focused on agent: {}", name),
                            None,
                        );
                    }
                }

                // Add task
                KeyCode::Char('t') => {
                    self.add_sample_task();
                }

                // Create decision
                KeyCode::Char('c') => {
                    self.create_sample_decision();
                }

                // Vote
                KeyCode::Char('v') => {
                    self.cast_sample_vote();
                }

                // Sync
                KeyCode::Char('s') => {
                    self.swarm_state.sync();
                }

                _ => {}
            }
        }

        true
    }

    /// Toggle pause state
    fn toggle_pause(&mut self) {
        if self.state == SwarmAppState::Paused {
            self.state = SwarmAppState::Running;
            self.swarm_state
                .add_event(EventType::AgentStarted, "Swarm resumed", None);
        } else {
            self.state = SwarmAppState::Paused;
            self.swarm_state
                .add_event(EventType::AgentStarted, "Swarm paused", None);
        }
    }

    /// Add a sample task (for demonstration)
    fn add_sample_task(&mut self) {
        if let Ok(mut swarm) = self.swarm_state.swarm().write() {
            let task = SwarmTask::new("Sample task from UI")
                .with_role(AgentRole::Coder)
                .with_priority(5);
            swarm.queue_task(task);

            self.swarm_state
                .add_event(EventType::TaskCreated, "New task added to queue", None);
        }
    }

    /// Create a sample decision (for demonstration)
    fn create_sample_decision(&mut self) {
        if let Ok(mut swarm) = self.swarm_state.swarm().write() {
            let decision_id = swarm.create_decision(
                "Should we use async/await?",
                vec!["Yes".to_string(), "No".to_string()],
            );

            self.swarm_state.add_event(
                EventType::DecisionCreated,
                format!("Created decision: {}", decision_id),
                None,
            );
        }
    }

    /// Cast a sample vote (for demonstration)
    fn cast_sample_vote(&mut self) {
        if let Ok(mut swarm) = self.swarm_state.swarm().write() {
            // Get first pending decision and first agent
            let decisions: Vec<_> = swarm
                .list_decisions()
                .into_iter()
                .filter(|d| d.is_pending())
                .collect();

            if let Some(decision) = decisions.first() {
                let agents: Vec<_> = swarm.list_agents();
                if let Some(agent) = agents.first() {
                    let decision_id = decision.id.clone();
                    let agent_id = agent.id.clone();
                    let agent_name = agent.name.clone();
                    let question = decision.question.clone();

                    let _ = swarm.vote(&decision_id, &agent_id, "Yes", 0.8, "Good approach");

                    self.swarm_state.add_event(
                        EventType::VoteCast,
                        format!("{} voted on {}", agent_name, question),
                        Some(agent_id),
                    );
                }
            }
        }
    }

    /// Check if app should continue running
    pub fn is_running(&self) -> bool {
        true // Can be extended for shutdown logic
    }
}

/// Render pause indicator overlay
fn render_pause_indicator(frame: &mut Frame, area: Rect) {
    let text = " ⏸ PAUSED ";
    let width = text.len() as u16 + 4;
    let height = 3;
    let x = (area.width - width) / 2;
    let y = (area.height - height) / 2;

    let pause_area = Rect::new(x, y, width, height);

    let block = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(Style::default().fg(TuiPalette::warning()))
        .style(Style::default().bg(TuiPalette::INK));

    frame.render_widget(block, pause_area);

    let text_widget = Paragraph::new(text)
        .style(
            Style::default()
                .fg(TuiPalette::warning())
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center);

    let inner = Rect::new(pause_area.x + 1, pause_area.y + 1, pause_area.width - 2, 1);
    frame.render_widget(text_widget, inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::swarm::create_dev_swarm;

    #[test]
    fn test_swarm_app_creation() {
        let app = SwarmApp::new();
        assert_eq!(app.state, SwarmAppState::Running);
        assert!(!app.show_help);
    }

    #[test]
    fn test_swarm_app_with_swarm() {
        let swarm = Arc::new(RwLock::new(create_dev_swarm()));
        let app = SwarmApp::with_swarm(swarm);
        assert_eq!(app.state, SwarmAppState::Running);
    }

    #[test]
    fn test_swarm_app_with_config() {
        let roles = vec![AgentRole::Coder, AgentRole::Tester];
        let app = SwarmApp::with_config(roles);
        assert_eq!(app.state, SwarmAppState::Running);
    }

    #[test]
    fn test_toggle_pause() {
        let mut app = SwarmApp::new();
        assert_eq!(app.state, SwarmAppState::Running);

        app.toggle_pause();
        assert_eq!(app.state, SwarmAppState::Paused);

        app.toggle_pause();
        assert_eq!(app.state, SwarmAppState::Running);
    }

    #[test]
    fn test_app_state_equality() {
        assert_eq!(SwarmAppState::Running, SwarmAppState::Running);
        assert_eq!(SwarmAppState::Paused, SwarmAppState::Paused);
        assert_ne!(SwarmAppState::Running, SwarmAppState::Paused);
    }

    #[test]
    fn test_handle_event_single_q_does_not_quit() {
        let mut app = SwarmApp::new();
        let event = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
        ));
        // Single q should arm quit but not actually quit
        let should_continue = app.handle_event(event);
        assert!(should_continue, "single q should not quit");
        assert!(app.last_q_press.is_some(), "q press should be recorded");
    }

    #[test]
    fn test_handle_event_double_q_quits() {
        let mut app = SwarmApp::new();
        let event = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
        ));
        // First q arms
        let should_continue = app.handle_event(event.clone());
        assert!(should_continue, "first q should not quit");
        // Second q within timeout quits
        let should_continue = app.handle_event(event);
        assert!(!should_continue, "second q should quit");
    }

    #[test]
    fn test_handle_event_quit_ctrl_c() {
        let mut app = SwarmApp::new();
        let event = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        ));
        let should_continue = app.handle_event(event);
        assert!(!should_continue);
    }

    #[test]
    fn test_handle_event_help_toggle() {
        let mut app = SwarmApp::new();
        assert!(!app.show_help);

        let event = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('?'),
            KeyModifiers::NONE,
        ));
        app.handle_event(event);
        assert!(app.show_help);

        // Esc while in help
        let esc_event = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE,
        ));
        app.handle_event(esc_event);
        assert!(!app.show_help);
    }

    #[test]
    fn test_handle_event_space_pause() {
        let mut app = SwarmApp::new();
        assert_eq!(app.state, SwarmAppState::Running);

        let event = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char(' '),
            KeyModifiers::NONE,
        ));
        app.handle_event(event);
        assert_eq!(app.state, SwarmAppState::Paused);
    }

    #[test]
    fn test_handle_event_tab_focus() {
        let mut app = SwarmApp::new();
        let tab = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Tab,
            KeyModifiers::NONE,
        ));
        // Just make sure it doesn't panic
        app.handle_event(tab);
    }

    #[test]
    fn test_handle_event_backtab() {
        let mut app = SwarmApp::new();
        let backtab = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::BackTab,
            KeyModifiers::SHIFT,
        ));
        app.handle_event(backtab);
    }

    #[test]
    fn test_is_running() {
        let app = SwarmApp::new();
        assert!(app.is_running());
    }

    #[test]
    fn test_default_impl() {
        let app = SwarmApp::default();
        assert_eq!(app.state, SwarmAppState::Running);
    }

    #[test]
    fn test_with_config_agent_count() {
        let roles = vec![AgentRole::Coder, AgentRole::Tester, AgentRole::Architect];
        let app = SwarmApp::with_config(roles);
        // After initial sync in constructor, agents should be populated
        assert_eq!(app.swarm_state.agents.len(), 3);
    }

    #[test]
    fn test_handle_event_refresh() {
        let mut app = SwarmApp::new();
        let initial_event_count = app.swarm_state.events.len();

        let event = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('r'),
            KeyModifiers::NONE,
        ));
        let cont = app.handle_event(event);
        assert!(cont);
        // Refresh adds a "Manual refresh" event
        assert!(app.swarm_state.events.len() > initial_event_count);
        let last_event = app.swarm_state.events.last().unwrap();
        assert_eq!(last_event.message, "Manual refresh");
    }

    #[test]
    fn test_handle_event_zoom_toggle() {
        let mut app = SwarmApp::new();
        let was_zoomed = app.layout_engine.is_zoomed();

        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('z'),
            KeyModifiers::NONE,
        )));
        assert_ne!(app.layout_engine.is_zoomed(), was_zoomed);

        // Press z again to toggle back
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('z'),
            KeyModifiers::NONE,
        )));
        assert_eq!(app.layout_engine.is_zoomed(), was_zoomed);
    }

    #[test]
    fn test_handle_event_esc_unzooms() {
        let mut app = SwarmApp::new();

        // Zoom first
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('z'),
            KeyModifiers::NONE,
        )));
        assert!(app.layout_engine.is_zoomed());

        // Now Esc should unzoom
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE,
        )));
        assert!(!app.layout_engine.is_zoomed());
    }

    #[test]
    fn test_handle_event_esc_noop_when_not_zoomed() {
        let mut app = SwarmApp::new();
        assert!(!app.layout_engine.is_zoomed());

        let esc = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE,
        ));
        let cont = app.handle_event(esc);
        assert!(cont); // Should not quit
        assert!(!app.layout_engine.is_zoomed()); // Still not zoomed
    }

    #[test]
    fn test_help_mode_consumes_all_input() {
        let mut app = SwarmApp::new();

        // Enter help mode
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('?'),
            KeyModifiers::NONE,
        )));
        assert!(app.show_help);

        // 'q' while in help should NOT quit - it should be consumed
        let q_event = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
        ));
        let cont = app.handle_event(q_event);
        assert!(cont); // Should continue, not quit
        assert!(app.show_help); // Still in help
    }

    #[test]
    fn test_help_mode_exit_with_question_mark() {
        let mut app = SwarmApp::new();

        // Enter help mode
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('?'),
            KeyModifiers::NONE,
        )));
        assert!(app.show_help);

        // '?' while in help should dismiss it
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('?'),
            KeyModifiers::NONE,
        )));
        assert!(!app.show_help);
    }

    #[test]
    fn test_on_tick_when_paused() {
        let mut app = SwarmApp::new();
        app.toggle_pause();
        assert_eq!(app.state, SwarmAppState::Paused);

        // Force sync interval to be exceeded
        app.last_sync = Instant::now() - Duration::from_secs(10);
        let stale_sync = app.last_sync;

        app.on_tick();

        // When paused, last_sync should NOT be updated
        assert_eq!(app.last_sync, stale_sync);
    }

    #[test]
    fn test_on_tick_when_running_no_sync_needed() {
        let mut app = SwarmApp::new();
        assert_eq!(app.state, SwarmAppState::Running);

        // last_sync was just set in constructor, so interval not exceeded
        let sync_before = app.last_sync;
        app.on_tick();

        // With default 500ms interval and immediate tick, last_sync may or may not update
        // but it should not panic
        assert!(app.last_sync >= sync_before);
    }

    #[test]
    fn test_add_sample_task() {
        let mut app = SwarmApp::new();
        let events_before = app.swarm_state.events.len();

        // Press 't' to add a sample task
        let event = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('t'),
            KeyModifiers::NONE,
        ));
        app.handle_event(event);

        // Should have added an event about the new task
        assert!(app.swarm_state.events.len() > events_before);
        let last = app.swarm_state.events.last().unwrap();
        assert_eq!(last.message, "New task added to queue");
    }

    #[test]
    fn test_create_sample_decision() {
        let mut app = SwarmApp::new();
        let events_before = app.swarm_state.events.len();

        let event = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::NONE,
        ));
        app.handle_event(event);

        assert!(app.swarm_state.events.len() > events_before);
        let last = app.swarm_state.events.last().unwrap();
        assert!(last.message.starts_with("Created decision:"));
    }

    #[test]
    fn test_cast_sample_vote_after_decision() {
        let mut app = SwarmApp::new();

        // First create a decision
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::NONE,
        )));

        let events_before = app.swarm_state.events.len();

        // Now vote on it
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('v'),
            KeyModifiers::NONE,
        )));

        // Should have added a vote event
        assert!(app.swarm_state.events.len() > events_before);
        let last = app.swarm_state.events.last().unwrap();
        assert!(last.message.contains("voted on"));
    }

    #[test]
    fn test_cast_vote_no_decisions() {
        let mut app = SwarmApp::new();
        let events_before = app.swarm_state.events.len();

        // Vote with no decisions - should silently do nothing
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('v'),
            KeyModifiers::NONE,
        )));

        // No new event should be added
        assert_eq!(app.swarm_state.events.len(), events_before);
    }

    #[test]
    fn test_sync_event() {
        let mut app = SwarmApp::new();

        let event = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('s'),
            KeyModifiers::NONE,
        ));
        // Should not panic
        let cont = app.handle_event(event);
        assert!(cont);
    }

    #[test]
    fn test_pause_adds_events() {
        let mut app = SwarmApp::new();
        let events_before = app.swarm_state.events.len();

        app.toggle_pause();
        assert_eq!(app.state, SwarmAppState::Paused);
        // Should have added "Swarm paused" event
        let pause_event = &app.swarm_state.events[events_before];
        assert_eq!(pause_event.message, "Swarm paused");

        app.toggle_pause();
        assert_eq!(app.state, SwarmAppState::Running);
        // Should have added "Swarm resumed" event
        let resume_event = &app.swarm_state.events[events_before + 1];
        assert_eq!(resume_event.message, "Swarm resumed");
    }

    #[test]
    fn test_swarm_app_state_debug() {
        // Ensure Debug is implemented
        let state = SwarmAppState::Running;
        let debug_str = format!("{:?}", state);
        assert_eq!(debug_str, "Running");
    }

    #[test]
    fn test_swarm_app_state_clone_copy() {
        let state = SwarmAppState::Help;
        let cloned = state;
        assert_eq!(cloned, SwarmAppState::Help);
    }

    #[test]
    fn test_initial_sync_on_creation() {
        let app = SwarmApp::new();
        // Constructor calls sync and adds an initialization event
        assert!(!app.swarm_state.events.is_empty());
        assert_eq!(app.swarm_state.events[0].message, "Swarm UI initialized");
        // dev swarm has 4 agents
        assert_eq!(app.swarm_state.agents.len(), 4);
    }

    // ── Agent focus/unfocus tests ───────────────────────────────────

    #[test]
    fn test_focused_agent_initially_none() {
        let app = SwarmApp::new();
        assert!(app.focused_agent.is_none());
        assert_eq!(app.selected_agent, 0);
    }

    #[test]
    fn test_enter_focuses_selected_agent() {
        let mut app = SwarmApp::new();
        // dev swarm has 4 agents; selected_agent starts at 0
        assert!(app.focused_agent.is_none());

        let enter = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ));
        app.handle_event(enter);

        // Should now be focused on the first agent
        assert!(app.focused_agent.is_some());
        let first_agent_name = app.swarm_state.agents[0].name.clone();
        assert_eq!(
            app.focused_agent.as_deref(),
            Some(first_agent_name.as_str())
        );
    }

    #[test]
    fn test_esc_unfocuses_agent() {
        let mut app = SwarmApp::new();

        // Focus first
        let enter = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ));
        app.handle_event(enter);
        assert!(app.focused_agent.is_some());

        // Now Esc should unfocus
        let esc = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE,
        ));
        app.handle_event(esc);
        assert!(app.focused_agent.is_none());
    }

    #[test]
    fn test_esc_unfocuses_before_unzoom() {
        let mut app = SwarmApp::new();

        // Zoom first
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('z'),
            KeyModifiers::NONE,
        )));
        assert!(app.layout_engine.is_zoomed());

        // Focus an agent
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
        assert!(app.focused_agent.is_some());

        // First Esc should unfocus the agent, NOT unzoom
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE,
        )));
        assert!(app.focused_agent.is_none());
        assert!(app.layout_engine.is_zoomed()); // Still zoomed

        // Second Esc should unzoom
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE,
        )));
        assert!(!app.layout_engine.is_zoomed());
    }

    #[test]
    fn test_quick_focus_with_number_keys() {
        let mut app = SwarmApp::new();
        // dev swarm has 4 agents (indices 0-3)

        // Press '2' to quick-focus the second agent
        let key2 = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('2'),
            KeyModifiers::NONE,
        ));
        app.handle_event(key2);

        assert_eq!(app.selected_agent, 1);
        let second_agent_name = app.swarm_state.agents[1].name.clone();
        assert_eq!(
            app.focused_agent.as_deref(),
            Some(second_agent_name.as_str())
        );
    }

    #[test]
    fn test_quick_focus_out_of_range_does_nothing() {
        let mut app = SwarmApp::new();
        // dev swarm has 4 agents, so '9' is out of range

        let key9 = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('9'),
            KeyModifiers::NONE,
        ));
        app.handle_event(key9);

        assert!(app.focused_agent.is_none());
        assert_eq!(app.selected_agent, 0);
    }

    #[test]
    fn test_quick_focus_zero_does_nothing() {
        let mut app = SwarmApp::new();

        // '0' should not trigger quick-focus
        let key0 = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('0'),
            KeyModifiers::NONE,
        ));
        app.handle_event(key0);

        assert!(app.focused_agent.is_none());
    }

    #[test]
    fn test_arrow_down_navigates_agent_list() {
        let mut app = SwarmApp::new();
        assert_eq!(app.selected_agent, 0);

        let down = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        ));
        app.handle_event(down);
        assert_eq!(app.selected_agent, 1);

        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        )));
        assert_eq!(app.selected_agent, 2);
    }

    #[test]
    fn test_arrow_up_navigates_agent_list() {
        let mut app = SwarmApp::new();
        // Move down first
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        )));
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        )));
        assert_eq!(app.selected_agent, 2);

        // Now up
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Up,
            KeyModifiers::NONE,
        )));
        assert_eq!(app.selected_agent, 1);
    }

    #[test]
    fn test_arrow_down_wraps_around() {
        let mut app = SwarmApp::new();
        let agent_count = app.swarm_state.agents.len();
        assert!(agent_count > 0);

        // Move to last agent
        for _ in 0..agent_count - 1 {
            app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
                KeyCode::Down,
                KeyModifiers::NONE,
            )));
        }
        assert_eq!(app.selected_agent, agent_count - 1);

        // One more down should wrap to 0
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        )));
        assert_eq!(app.selected_agent, 0);
    }

    #[test]
    fn test_arrow_up_wraps_around() {
        let mut app = SwarmApp::new();
        let agent_count = app.swarm_state.agents.len();
        assert!(agent_count > 0);
        assert_eq!(app.selected_agent, 0);

        // Up from 0 should wrap to last
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Up,
            KeyModifiers::NONE,
        )));
        assert_eq!(app.selected_agent, agent_count - 1);
    }

    #[test]
    fn test_focus_then_navigate_then_refocus() {
        let mut app = SwarmApp::new();

        // Focus agent 0
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
        let first_name = app.swarm_state.agents[0].name.clone();
        assert_eq!(app.focused_agent.as_deref(), Some(first_name.as_str()));

        // Unfocus
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE,
        )));
        assert!(app.focused_agent.is_none());

        // Navigate to agent 2
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        )));
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        )));
        assert_eq!(app.selected_agent, 2);

        // Focus agent 2
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
        let third_name = app.swarm_state.agents[2].name.clone();
        assert_eq!(app.focused_agent.as_deref(), Some(third_name.as_str()));
    }

    #[test]
    fn test_focus_adds_event_to_log() {
        let mut app = SwarmApp::new();
        let events_before = app.swarm_state.events.len();

        // Focus via Enter
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));

        assert!(app.swarm_state.events.len() > events_before);
        let last = app.swarm_state.events.last().unwrap();
        assert!(last.message.starts_with("Focused on agent:"));
    }

    #[test]
    fn test_unfocus_adds_event_to_log() {
        let mut app = SwarmApp::new();

        // Focus first
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));

        let events_before = app.swarm_state.events.len();

        // Unfocus
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE,
        )));

        assert!(app.swarm_state.events.len() > events_before);
        let last = app.swarm_state.events.last().unwrap();
        assert_eq!(last.message, "Unfocused agent (showing all)");
    }

    #[test]
    fn test_quick_focus_adds_event_to_log() {
        let mut app = SwarmApp::new();
        let events_before = app.swarm_state.events.len();

        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('1'),
            KeyModifiers::NONE,
        )));

        assert!(app.swarm_state.events.len() > events_before);
        let last = app.swarm_state.events.last().unwrap();
        assert!(last.message.starts_with("Quick-focused on agent:"));
    }

    #[test]
    fn test_alt_number_keys_still_change_layout() {
        let mut app = SwarmApp::new();
        // Alt+1 should still change layout, not focus an agent
        app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('1'),
            KeyModifiers::ALT,
        )));
        // Should not have focused any agent
        assert!(app.focused_agent.is_none());
    }

    #[test]
    fn test_enter_with_no_agents_does_nothing() {
        let swarm = crate::orchestration::swarm::Swarm::new();
        let swarm = Arc::new(RwLock::new(swarm));
        let mut app = SwarmApp::with_swarm(swarm);
        // No agents after sync
        assert!(app.swarm_state.agents.is_empty());

        let enter = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ));
        app.handle_event(enter);
        assert!(app.focused_agent.is_none());
    }
}
