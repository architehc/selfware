//! Multi-chat TUI: one tab per agent plus an "All" overview.
//!
//! `selfware --tui multi-chat` runs the same fan-out as the plain REPL, but
//! instead of one interleaved stream each agent gets its own tab with its own
//! chronological log, and the "All" tab shows one status line per agent
//! (role, state, elapsed, tokens, last activity) and, after a run, the
//! per-agent summary table. All state lives in
//! [`crate::orchestration::multiagent::MultiChatView`]; this
//! module only maps keys and draws it.
//!
//! Keys: `1..N` select an agent tab and `0` the overview (while the input
//! line is empty; `Alt+digit` always switches), `Tab`/`Shift-Tab` cycle,
//! `PgUp`/`PgDn`/`↑`/`↓` scroll an agent tab, `Enter` sends the typed task to
//! every agent, `Esc` clears the input (or quits when it is empty),
//! `Ctrl-C` quits.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
    Frame,
};
use tokio::sync::mpsc;

use crate::orchestration::multiagent::view::{
    agent_color_index, render_summary_table, status_label, AGENT_COLOR_COUNT,
};
use crate::orchestration::multiagent::{
    AgentResult, AgentStatus, MultiAgentChat, MultiAgentEvent, MultiChatView, ViewTab,
};
use crate::swarm::AgentRole;

use super::{signal_received, TuiPalette, TuiTerminal};

/// Agent colours in the TUI, indexed by [`agent_color_index`] (mirrors the
/// plain CLI palette).
const AGENT_COLORS: [Color; AGENT_COLOR_COUNT] = [
    Color::LightCyan,
    Color::LightMagenta,
    Color::LightYellow,
    Color::LightBlue,
    Color::Cyan,
    Color::Magenta,
    Color::Yellow,
    Color::Blue,
];

fn agent_style(agent_id: usize) -> Style {
    Style::default().fg(AGENT_COLORS[agent_color_index(agent_id)])
}

/// What a key press asks the event loop to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultiChatKeyAction {
    /// Handled locally (tab switch, typing, scrolling) or ignored.
    None,
    /// Send this task to every agent.
    Submit(String),
    /// Leave the TUI.
    Quit,
}

/// Mutable UI state of the multi-chat TUI.
#[derive(Debug, Clone)]
pub struct MultiChatTuiState {
    pub view: MultiChatView,
    /// Task being typed.
    pub input: String,
    /// Whether a fan-out is in flight (a new task is refused until it ends).
    pub running: bool,
    /// Lines scrolled up from the bottom of the selected agent tab.
    pub scroll: usize,
    /// One-line status / outcome message.
    pub status: String,
    /// Results of the last finished run (for the summary table).
    pub last_results: Vec<AgentResult>,
}

impl MultiChatTuiState {
    pub fn new(roles: &[AgentRole]) -> Self {
        Self {
            view: MultiChatView::from_roles(roles),
            input: String::new(),
            running: false,
            scroll: 0,
            status: "type a task and press Enter · 0/1..N or Tab to switch tabs · Ctrl-C quits"
                .to_string(),
            last_results: Vec::new(),
        }
    }

    fn select_tab(&mut self, f: impl FnOnce(&mut MultiChatView)) {
        let before = self.view.tab();
        f(&mut self.view);
        if self.view.tab() != before {
            self.scroll = 0;
        }
    }
}

/// Map one key press onto the state; returns what the loop must do.
pub fn handle_key(state: &mut MultiChatTuiState, key: KeyEvent) -> MultiChatKeyAction {
    if key.kind == KeyEventKind::Release {
        return MultiChatKeyAction::None;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    match key.code {
        KeyCode::Char('c') | KeyCode::Char('d') if ctrl => MultiChatKeyAction::Quit,
        KeyCode::Tab => {
            state.select_tab(MultiChatView::next_tab);
            MultiChatKeyAction::None
        }
        KeyCode::BackTab => {
            state.select_tab(MultiChatView::prev_tab);
            MultiChatKeyAction::None
        }
        KeyCode::Char(c) if c.is_ascii_digit() && (alt || state.input.is_empty()) => {
            let n = c.to_digit(10).unwrap_or(0) as usize;
            let mut selected = false;
            state.select_tab(|v| selected = v.select_number(n));
            if !selected && !alt {
                // Not a tab number: it is the start of a task.
                state.input.push(c);
            }
            MultiChatKeyAction::None
        }
        KeyCode::Char(c) if !ctrl && !alt => {
            state.input.push(c);
            MultiChatKeyAction::None
        }
        KeyCode::Backspace => {
            state.input.pop();
            MultiChatKeyAction::None
        }
        KeyCode::Esc => {
            if state.input.is_empty() {
                MultiChatKeyAction::Quit
            } else {
                state.input.clear();
                MultiChatKeyAction::None
            }
        }
        KeyCode::Up => {
            state.scroll = state.scroll.saturating_add(1);
            MultiChatKeyAction::None
        }
        KeyCode::Down => {
            state.scroll = state.scroll.saturating_sub(1);
            MultiChatKeyAction::None
        }
        KeyCode::PageUp => {
            state.scroll = state.scroll.saturating_add(10);
            MultiChatKeyAction::None
        }
        KeyCode::PageDown => {
            state.scroll = state.scroll.saturating_sub(10);
            MultiChatKeyAction::None
        }
        KeyCode::End => {
            state.scroll = 0;
            MultiChatKeyAction::None
        }
        KeyCode::Enter => {
            let task = state.input.trim().to_string();
            if task.is_empty() {
                return MultiChatKeyAction::None;
            }
            let lower = task.to_ascii_lowercase();
            if crate::input::command_registry::is_exit_command(&lower) || lower == "q" {
                return MultiChatKeyAction::Quit;
            }
            if state.running {
                state.status = "a run is in progress — wait for it to finish".to_string();
                return MultiChatKeyAction::None;
            }
            state.input.clear();
            MultiChatKeyAction::Submit(task)
        }
        _ => MultiChatKeyAction::None,
    }
}

/// Hard-wrap lines to `width` chars (UTF-8 safe); empty lines are kept.
pub fn wrap_lines(lines: &[String], width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for line in lines {
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            out.push(String::new());
            continue;
        }
        for chunk in chars.chunks(width) {
            out.push(chunk.iter().collect());
        }
    }
    out
}

/// The window of `total` wrapped lines to show in `height` rows when
/// scrolled `scroll` lines up from the bottom: `(start, end)`; the scroll is
/// clamped so the top line can't scroll past the start.
pub fn visible_window(total: usize, height: usize, scroll: usize) -> (usize, usize) {
    let max_scroll = total.saturating_sub(height);
    let scroll = scroll.min(max_scroll);
    let end = total - scroll;
    (end.saturating_sub(height), end)
}

fn status_style(status: AgentStatus) -> Style {
    match status {
        AgentStatus::Idle => TuiPalette::muted_style(),
        AgentStatus::Working => Style::default().fg(TuiPalette::AMBER),
        AgentStatus::Completed => Style::default().fg(TuiPalette::GARDEN_GREEN),
        AgentStatus::Failed => Style::default().fg(Color::LightRed),
    }
}

/// Draw the whole multi-chat screen.
pub fn render(frame: &mut Frame, state: &MultiChatTuiState, now: Instant) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(frame.area());

    render_tabs(frame, chunks[0], state);
    match state.view.tab() {
        ViewTab::All => render_overview(frame, chunks[1], state, now),
        ViewTab::Agent(i) => render_agent(frame, chunks[1], state, i),
    }
    frame.render_widget(
        Paragraph::new(Span::styled(
            state.status.clone(),
            TuiPalette::muted_style(),
        )),
        chunks[2],
    );
    let input_title = if state.running {
        " Task (running…) "
    } else {
        " Task "
    };
    frame.render_widget(
        Paragraph::new(format!("❯ {}", state.input)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(TuiPalette::border_style())
                .title(Span::styled(input_title, TuiPalette::title_style())),
        ),
        chunks[3],
    );
}

fn render_tabs(frame: &mut Frame, area: Rect, state: &MultiChatTuiState) {
    let titles: Vec<Line> = state
        .view
        .tab_titles()
        .into_iter()
        .enumerate()
        .map(|(i, title)| {
            let style = match i.checked_sub(1).and_then(|a| state.view.panels().get(a)) {
                Some(panel) => agent_style(panel.agent_id),
                None => Style::default(),
            };
            Line::from(Span::styled(title, style))
        })
        .collect();
    let tabs = Tabs::new(titles)
        .select(state.view.tab_index())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(TuiPalette::border_style())
                .title(Span::styled(" 🤖 Multi-Agent ", TuiPalette::title_style())),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD));
    frame.render_widget(tabs, area);
}

fn render_overview(frame: &mut Frame, area: Rect, state: &MultiChatTuiState, now: Instant) {
    let mut lines: Vec<Line> = Vec::new();
    for panel in state.view.panels() {
        lines.push(Line::from(vec![
            Span::styled("● ", status_style(panel.status)),
            Span::styled(
                MultiChatView::status_line(panel, now),
                agent_style(panel.agent_id),
            ),
        ]));
    }
    if !state.last_results.is_empty() && !state.running {
        lines.push(Line::from(""));
        for (i, row) in render_summary_table(&state.last_results)
            .into_iter()
            .enumerate()
        {
            let style = if i < 2 {
                TuiPalette::muted_style()
            } else {
                Style::default()
            };
            lines.push(Line::from(Span::styled(row, style)));
        }
    }
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(TuiPalette::border_style())
                .title(Span::styled(" All agents ", TuiPalette::title_style())),
        ),
        area,
    );
}

fn render_agent(frame: &mut Frame, area: Rect, state: &MultiChatTuiState, index: usize) {
    let Some(panel) = state.view.panels().get(index) else {
        return;
    };
    let inner_width = area.width.saturating_sub(2) as usize;
    let inner_height = area.height.saturating_sub(2) as usize;
    let wrapped = wrap_lines(&panel.lines, inner_width);
    let (start, end) = visible_window(wrapped.len(), inner_height, state.scroll);
    let text: Vec<Line> = wrapped[start..end]
        .iter()
        .map(|l| Line::from(l.clone()))
        .collect();
    let title = format!(
        " {} · {} · {} ",
        panel.tag(),
        panel.role.name(),
        status_label(panel.status)
    );
    frame.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(agent_style(panel.agent_id))
                .title(Span::styled(title, agent_style(panel.agent_id))),
        ),
        area,
    );
}

type RunHandle = tokio::task::JoinHandle<Result<Vec<AgentResult>>>;

fn start_run(chat: &Arc<MultiAgentChat>, state: &mut MultiChatTuiState, task: String) -> RunHandle {
    state.view.begin_task(&task);
    state.running = true;
    state.scroll = 0;
    state.status = format!("running: {task}");
    let chat = Arc::clone(chat);
    tokio::spawn(async move { chat.run_task(&task).await })
}

fn finish_run(
    state: &mut MultiChatTuiState,
    outcome: Result<Result<Vec<AgentResult>>, tokio::task::JoinError>,
) {
    state.running = false;
    match outcome {
        Ok(Ok(results)) => {
            let ok = results.iter().filter(|r| r.success).count();
            let tokens: Option<usize> = results
                .iter()
                .filter_map(|r| r.usage.as_ref().map(|u| u.total_tokens))
                .reduce(|a, b| a + b);
            state.status = match tokens {
                Some(t) => format!(
                    "{ok}/{} agents ok · {t} tokens (provider-reported)",
                    results.len()
                ),
                None => format!("{ok}/{} agents ok · no token usage reported", results.len()),
            };
            state.last_results = results;
        }
        Ok(Err(e)) => state.status = format!("not run: {e}"),
        Err(e) => state.status = format!("run aborted: {e}"),
    }
}

/// Run the multi-chat TUI until the user quits.
///
/// `chat` must not have an event sender yet; one is installed here. When
/// `initial_task` is given it is submitted immediately (one-shot form); the
/// TUI stays open afterwards so the per-agent tabs can be read. The last
/// run's results are returned so the caller can print the summary table
/// after the alternate screen is gone.
pub async fn run_multichat_tui(
    chat: MultiAgentChat,
    roles: Vec<AgentRole>,
    initial_task: Option<String>,
) -> Result<Vec<AgentResult>> {
    let (tx, mut rx) = mpsc::channel::<MultiAgentEvent>(1000);
    let chat = Arc::new(chat.with_events(tx));
    let mut state = MultiChatTuiState::new(&roles);
    let mut run: Option<RunHandle> = None;

    let mut terminal = TuiTerminal::new()?;
    crate::output::set_tui_active(true);

    if let Some(task) = initial_task {
        run = Some(start_run(&chat, &mut state, task));
    }

    let loop_result: Result<()> = async {
        loop {
            let now = Instant::now();
            while let Ok(event) = rx.try_recv() {
                state.view.apply(&event, now);
            }
            if run.as_ref().is_some_and(|h| h.is_finished()) {
                if let Some(handle) = run.take() {
                    finish_run(&mut state, handle.await);
                }
            }

            terminal
                .terminal()
                .draw(|frame| render(frame, &state, Instant::now()))?;

            if signal_received() {
                break;
            }
            // Non-blocking poll; the sleep below paces the loop (~20 fps).
            while event::poll(Duration::ZERO)? {
                if let Event::Key(key) = event::read()? {
                    match handle_key(&mut state, key) {
                        MultiChatKeyAction::None => {}
                        MultiChatKeyAction::Quit => return Ok(()),
                        MultiChatKeyAction::Submit(task) => {
                            run = Some(start_run(&chat, &mut state, task));
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Ok(())
    }
    .await;

    if let Some(handle) = run.take() {
        handle.abort();
    }
    crate::output::set_tui_active(false);
    terminal.restore()?;
    drop(terminal);
    loop_result?;
    Ok(state.last_results)
}

#[cfg(test)]
#[path = "../../../tests/unit/ui/tui/multichat/multichat_test.rs"]
mod tests;
