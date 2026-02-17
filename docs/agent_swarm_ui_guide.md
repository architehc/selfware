# Agent Swarm UI Implementation Guide

## Overview

This guide describes how to create a Qwen Code CLI-inspired UI for the Selfware agent swarm system. The UI provides:

- **Multi-agent visualization** with animated avatars
- **Real-time message flow** between agents
- **Shared memory dashboard** showing swarm coordination
- **Task queue and progress** tracking
- **Consensus voting visualization**

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  Agent Swarm UI Architecture                                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐          │
│  │  Swarm      │◄──►│  UI State   │◄──►│   TUI       │          │
│  │  Controller │    │  Manager    │    │  Renderer   │          │
│  └─────────────┘    └─────────────┘    └─────────────┘          │
│         │                  │                  │                  │
│         ▼                  ▼                  ▼                  │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐          │
│  │   Agents    │    │  Message    │    │  Layout     │          │
│  │  (async)    │    │   Queue     │    │  Engine     │          │
│  └─────────────┘    └─────────────┘    └─────────────┘          │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Key Components

### 1. Swarm UI State Manager

Manages the shared state between the swarm system and the TUI:

```rust
pub struct SwarmUiState {
    /// Active agents with their status
    pub agents: Vec<AgentUiState>,
    /// Message flows between agents
    pub message_flows: Vec<MessageFlow>,
    /// Shared memory view
    pub shared_memory: Vec<MemoryEntryView>,
    /// Active decisions being voted on
    pub active_decisions: Vec<DecisionView>,
    /// Task queue status
    pub task_queue: Vec<TaskView>,
    /// System events log
    pub events: Vec<SwarmEvent>,
}
```

### 2. Agent Avatar Widget

Already implemented in `src/ui/tui/animation/agent_avatar.rs`:

- Role-specific icons (🏗️ Architect, 💻 Coder, 🧪 Tester, etc.)
- Pulsing border based on activity level
- Token count display
- Activity indicators (●●●○○)

### 3. Message Flow Animation

Already implemented in `src/ui/tui/animation/message_flow.rs`:

- Visualizes messages between agents
- Trail effects
- Different symbols for message types

### 4. Swarm Dashboard Layout

```
┌─────────────────────────────────────────────────────────────────┐
│ 🦊 Selfware Swarm │ 6 Agents │ 3 Tasks │ Consensus: Active     │
├─────────────────┬───────────────────────────┬───────────────────┤
│                 │                           │  🌱 Swarm Health  │
│  Agent Swarm    │    Message Flow           │  ████████░░ 82%   │
│  ┌─────────┐    │                           │                   │
│  │ 🏗️ Arch │    │   ●────────►◆             │  Active Tasks:    │
│  │ ●●●○○   │    │             │             │  🔧 Code Review   │
│  └────┬────┘    │   ◄─────────●             │  ⏱️ 2m 34s        │
│       │         │                           │                   │
│  ┌────┴────┐    │  Consensus Vote:          │  Shared Memory:   │
│  │ 💻 Code │    │  ┌─────────────────┐      │  24 entries       │
│  │ ●●●●○   │◄──►│  │ Security issue  │      │  3 tagged         │
│  └─────────┘    │  │ [Yes] [No] [?]  │      │                   │
│                 │  └─────────────────┘      │  Trust Score:     │
│  🧪 Tessa       │                           │  Avg: 0.78        │
│  ●●○○○          │                           │                   │
├─────────────────┴───────────────────────────┴───────────────────┤
│  📜 Swarm Events                                                │
│  [14:32:01] Architect proposed design for auth module           │
│  [14:32:15] Coder started implementation                        │
│  [14:33:02] Tester found 2 issues                               │
│  [14:33:30] Security flagged potential vulnerability            │
└─────────────────────────────────────────────────────────────────┘
```

## Implementation

### Step 1: Create Swarm UI State Module

Create `src/ui/tui/swarm_state.rs`:

```rust
use crate::orchestration::swarm::{Agent, AgentRole, AgentStatus, Swarm, SwarmTask, Decision};
use crate::ui::tui::animation::{AgentAvatar, ActivityLevel, MessageFlow, MessageType};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// UI-friendly representation of an agent
#[derive(Debug, Clone)]
pub struct AgentUiState {
    pub id: String,
    pub name: String,
    pub role: AgentRole,
    pub status: AgentStatus,
    pub activity: ActivityLevel,
    pub trust_score: f32,
    pub tokens_processed: u64,
    pub current_task: Option<String>,
    pub position: (u16, u16), // For swarm visualization layout
}

/// View of a memory entry
#[derive(Debug, Clone)]
pub struct MemoryEntryView {
    pub key: String,
    pub value_preview: String,
    pub created_by: String,
    pub tags: Vec<String>,
    pub access_count: u32,
}

/// View of an active decision
#[derive(Debug, Clone)]
pub struct DecisionView {
    pub id: String,
    pub question: String,
    pub options: Vec<String>,
    pub vote_count: usize,
    pub status: DecisionStatus,
}

/// View of a task
#[derive(Debug, Clone)]
pub struct TaskView {
    pub id: String,
    pub description: String,
    pub priority: u8,
    pub status: TaskStatus,
    pub assigned_agents: Vec<String>,
    pub progress: f32,
}

/// Swarm event for the event log
#[derive(Debug, Clone)]
pub struct SwarmEvent {
    pub timestamp: String,
    pub event_type: EventType,
    pub message: String,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    AgentStarted,
    AgentCompleted,
    AgentError,
    TaskCreated,
    TaskCompleted,
    DecisionCreated,
    DecisionResolved,
    MemoryUpdated,
    ConsensusReached,
    ConflictDetected,
}

/// Main swarm UI state
pub struct SwarmUiState {
    agents: Vec<AgentUiState>,
    message_flows: Vec<MessageFlow>,
    memory_entries: Vec<MemoryEntryView>,
    decisions: Vec<DecisionView>,
    tasks: Vec<TaskView>,
    events: Vec<SwarmEvent>,
    swarm: Arc<RwLock<Swarm>>,
}

impl SwarmUiState {
    pub fn new(swarm: Arc<RwLock<Swarm>>) -> Self {
        Self {
            agents: Vec::new(),
            message_flows: Vec::new(),
            memory_entries: Vec::new(),
            decisions: Vec::new(),
            tasks: Vec::new(),
            events: Vec::new(),
            swarm,
        }
    }

    /// Sync state from the underlying swarm
    pub fn sync(&mut self) {
        if let Ok(swarm) = self.swarm.read() {
            // Sync agents
            self.agents = swarm.list_agents().iter().map(|a| AgentUiState {
                id: a.id.clone(),
                name: a.name.clone(),
                role: a.role,
                status: a.status,
                activity: Self::status_to_activity(a.status),
                trust_score: a.trust_score,
                tokens_processed: 0, // Would come from actual token tracking
                current_task: None,
                position: (0, 0), // Calculated by layout
            }).collect();

            // Sync other state...
        }
    }

    fn status_to_activity(status: AgentStatus) -> ActivityLevel {
        match status {
            AgentStatus::Idle => ActivityLevel::Idle,
            AgentStatus::Working => ActivityLevel::High,
            AgentStatus::Waiting => ActivityLevel::Medium,
            AgentStatus::Completed => ActivityLevel::Complete,
            AgentStatus::Error => ActivityLevel::Error,
            AgentStatus::Paused => ActivityLevel::Idle,
        }
    }

    pub fn add_event(&mut self, event_type: EventType, message: impl Into<String>, agent_id: Option<String>) {
        self.events.push(SwarmEvent {
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            event_type,
            message: message.into(),
            agent_id,
        });
        // Keep only last 100 events
        if self.events.len() > 100 {
            self.events.remove(0);
        }
    }
}
```

### Step 2: Create Swarm Dashboard Widgets

Create `src/ui/tui/swarm_widgets.rs`:

```rust
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Widget},
    Frame,
};
use crate::ui::tui::TuiPalette;
use crate::ui::tui::swarm_state::{AgentUiState, SwarmEvent, EventType, TaskView};

/// Render the swarm status bar
pub fn render_swarm_status_bar(
    frame: &mut Frame,
    area: Rect,
    agent_count: usize,
    task_count: usize,
    decision_count: usize,
) {
    let status_text = format!(
        " 🦊 Selfware Swarm │ {} Agents │ {} Tasks │ {} Decisions ",
        agent_count, task_count, decision_count
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(TuiPalette::border_style())
        .title(Span::styled(status_text, TuiPalette::title_style()));

    frame.render_widget(block, area);
}

/// Render agent swarm visualization
pub fn render_agent_swarm(
    frame: &mut Frame,
    area: Rect,
    agents: &[AgentUiState],
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(TuiPalette::border_style())
        .title(Span::styled(" 🤖 Agent Swarm ", TuiPalette::title_style()));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Layout agents in a grid
    let cols = 2u16;
    let rows = ((agents.len() as u16 + cols - 1) / cols).max(1);
    
    let col_width = inner.width / cols;
    let row_height = inner.height / rows;

    for (i, agent) in agents.iter().enumerate() {
        let col = (i as u16) % cols;
        let row = (i as u16) / cols;
        
        let x = inner.x + col * col_width;
        let y = inner.y + row * row_height;
        
        let agent_area = Rect::new(x, y, col_width, row_height.min(5));
        render_agent_card(frame, agent_area, agent);
    }
}

fn render_agent_card(frame: &mut Frame, area: Rect, agent: &AgentUiState) {
    let role_icon = agent.role.icon();
    let role_name = agent.role.name();
    
    // Activity dots
    let activity_dots = match agent.activity {
        ActivityLevel::Idle => "○○○○○",
        ActivityLevel::Low => "●○○○○",
        ActivityLevel::Medium => "●●○○○",
        ActivityLevel::High => "●●●○○",
        ActivityLevel::Max => "●●●●○",
        ActivityLevel::Complete => "●●●●●",
        ActivityLevel::Error => "⚠",
    };

    let lines = vec![
        Line::from(vec![
            Span::styled(format!("{} ", role_icon), Style::default().fg(agent.role.color())),
            Span::styled(&agent.name, Style::default().add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled(activity_dots, Style::default().fg(agent.role.color())),
            Span::raw(" "),
            Span::styled(format!("{:.0}%", agent.trust_score * 100.0), TuiPalette::muted_style()),
        ]),
    ];

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

/// Render shared memory view
pub fn render_shared_memory(
    frame: &mut Frame,
    area: Rect,
    entries: &[MemoryEntryView],
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(TuiPalette::border_style())
        .title(Span::styled(" 🧠 Shared Memory ", TuiPalette::title_style()));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let items: Vec<ListItem> = entries
        .iter()
        .take(inner.height as usize)
        .map(|entry| {
            let tags = if entry.tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", entry.tags.join(", "))
            };
            
            ListItem::new(Line::from(vec![
                Span::styled(format!("📄 ", ), TuiPalette::muted_style()),
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
    frame.render_widget(list, inner);
}

/// Render task queue
pub fn render_task_queue(
    frame: &mut Frame,
    area: Rect,
    tasks: &[TaskView],
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(TuiPalette::border_style())
        .title(Span::styled(" 📋 Task Queue ", TuiPalette::title_style()));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let items: Vec<ListItem> = tasks
        .iter()
        .take(inner.height as usize)
        .map(|task| {
            let priority_color = match task.priority {
                8..=10 => Color::Red,
                5..=7 => Color::Yellow,
                _ => Color::Green,
            };
            
            let progress_bar = format!(
                "[{}{}]",
                "█".repeat((task.progress * 10.0) as usize),
                "░".repeat(10 - (task.progress * 10.0) as usize)
            );
            
            ListItem::new(Line::from(vec![
                Span::styled(format!("P{} ", task.priority), Style::default().fg(priority_color)),
                Span::styled(&task.description, Style::default()),
                Span::raw(" "),
                Span::styled(progress_bar, Style::default().fg(TuiPalette::GARDEN_GREEN)),
            ]))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

/// Render swarm event log
pub fn render_swarm_events(
    frame: &mut Frame,
    area: Rect,
    events: &[SwarmEvent],
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(TuiPalette::border_style())
        .title(Span::styled(" 📜 Swarm Events ", TuiPalette::title_style()));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let items: Vec<ListItem> = events
        .iter()
        .rev()
        .take(inner.height as usize)
        .map(|event| {
            let icon = match event.event_type {
                EventType::AgentStarted => "▶",
                EventType::AgentCompleted => "✓",
                EventType::AgentError => "✗",
                EventType::TaskCreated => "📝",
                EventType::TaskCompleted => "✓",
                EventType::DecisionCreated => "⚖️",
                EventType::DecisionResolved => "✓",
                EventType::MemoryUpdated => "💾",
                EventType::ConsensusReached => "🤝",
                EventType::ConflictDetected => "⚠️",
            };
            
            let style = match event.event_type {
                EventType::AgentError | EventType::ConflictDetected => TuiPalette::error_style(),
                EventType::AgentCompleted | EventType::TaskCompleted | EventType::ConsensusReached => {
                    TuiPalette::success_style()
                }
                _ => Style::default(),
            };
            
            ListItem::new(Line::from(vec![
                Span::styled(format!("[{}] ", event.timestamp), TuiPalette::muted_style()),
                Span::styled(format!("{} ", icon), style),
                Span::styled(&event.message, style),
            ]))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}
```

### Step 3: Create Swarm UI App

Create `src/ui/tui/swarm_app.rs`:

```rust
use crate::orchestration::swarm::{Swarm, AgentRole, create_dev_swarm};
use crate::ui::tui::swarm_state::SwarmUiState;
use crate::ui::tui::swarm_widgets::*;
use crate::ui::tui::layout::{LayoutEngine, LayoutPreset, PaneType};
use ratatui::{Frame, layout::Rect};
use std::sync::{Arc, RwLock};

/// Swarm UI application
pub struct SwarmApp {
    state: SwarmUiState,
    layout_engine: LayoutEngine,
    show_help: bool,
    paused: bool,
}

impl SwarmApp {
    pub fn new() -> Self {
        let swarm = Arc::new(RwLock::new(create_dev_swarm()));
        let mut layout_engine = LayoutEngine::new();
        
        // Apply swarm-specific layout
        layout_engine.apply_preset(LayoutPreset::Dashboard);
        
        Self {
            state: SwarmUiState::new(swarm),
            layout_engine,
            show_help: false,
            paused: false,
        }
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.size();
        
        // Sync state before rendering
        self.state.sync();
        
        // Calculate layouts
        let layouts = self.layout_engine.calculate_layout(area);
        
        // Render each pane
        for (pane_id, pane_area) in &layouts {
            if let Some(pane) = self.layout_engine.get_pane(*pane_id) {
                match pane.pane_type {
                    PaneType::StatusBar => {
                        render_swarm_status_bar(
                            frame,
                            *pane_area,
                            self.state.agents.len(),
                            self.state.tasks.len(),
                            self.state.decisions.len(),
                        );
                    }
                    PaneType::Chat => {
                        render_agent_swarm(frame, *pane_area, &self.state.agents);
                    }
                    PaneType::GardenView => {
                        render_shared_memory(frame, *pane_area, &self.state.memory_entries);
                    }
                    PaneType::ActiveTools => {
                        render_task_queue(frame, *pane_area, &self.state.tasks);
                    }
                    PaneType::Logs => {
                        render_swarm_events(frame, *pane_area, &self.state.events);
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn on_tick(&mut self) {
        if !self.paused {
            self.state.sync();
        }
    }
}
```

### Step 4: Integration with Existing TUI

Modify `src/ui/tui/mod.rs` to add swarm UI mode:

```rust
/// Run the TUI in swarm mode
pub fn run_tui_swarm() -> Result<()> {
    let mut terminal = TuiTerminal::new()?;
    let mut app = SwarmApp::new();
    
    loop {
        // Update animations
        app.on_tick();
        
        // Render
        terminal.terminal().draw(|frame| {
            app.render(frame);
        })?;
        
        // Handle events
        if let Some(event) = read_event(100)? {
            if is_quit(&event) {
                break;
            }
            
            // Handle other events...
        }
    }
    
    terminal.restore()?;
    Ok(())
}
```

## Usage Examples

### Starting the Swarm UI

```rust
use selfware::ui::tui::run_tui_swarm;

fn main() -> Result<()> {
    run_tui_swarm()
}
```

### Adding Custom Swarm Configurations

```rust
use selfware::orchestration::swarm::{Swarm, Agent, AgentRole};

let mut swarm = Swarm::new();

// Add specialized agents
swarm.add_agent(Agent::new("Alice", AgentRole::Architect)
    .with_expertise("Microservices")
    .with_expertise("Rust"));

swarm.add_agent(Agent::new("Bob", AgentRole::Coder)
    .with_expertise("Async Programming"));

swarm.add_agent(Agent::new("Carol", AgentRole::Tester)
    .with_expertise("Property-based Testing"));

swarm.add_agent(Agent::new("Dave", AgentRole::Security)
    .with_expertise("Cryptography"));
```

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `q` / `Ctrl+C` | Quit (press q twice) |
| `?` | Toggle help overlay |
| `Tab` | Cycle focus between panes |
| `Space` | Pause/resume animations |
| `z` | Toggle zoom on focused pane |
| `r` | Refresh swarm state |
| `Alt+1-6` | Switch layout presets |
| `c` | Create new decision |
| `v` | Vote on current decision |
| `t` | Add task to queue |

## Features

### Real-time Visualization

- Agent activity levels shown with pulsing borders
- Message flows animated between communicating agents
- Token counts updating in real-time
- Trust scores reflecting agent performance

### Consensus Visualization

- Active decisions displayed with vote counts
- Visual indicators for conflicts
- Resolution progress tracking

### Shared Memory Browser

- Browse memory entries by key
- Filter by tags
- View access statistics
- Track modifications

### Task Management

- Queue visualization with priorities
- Progress bars for active tasks
- Assignment tracking
- Completion notifications

## Performance Considerations

- Use `Arc<RwLock<Swarm>>` for thread-safe shared state
- Sync state only on UI ticks (not every frame)
- Limit event log to last 100 entries
- Use bounded channels for message passing
- Cache agent avatars to avoid re-rendering

## Future Enhancements

1. **3D Swarm Visualization**: Use tui-rs canvas for spatial agent layout
2. **Voice Notifications**: Audio cues for important events
3. **Export/Import**: Save and load swarm sessions
4. **Remote Monitoring**: WebSocket interface for remote observation
5. **Custom Themes**: User-defined color schemes
