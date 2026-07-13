# Qwen Code CLI-Inspired Agent Swarm UI

## Overview

This implementation provides a **Qwen Code CLI-inspired Terminal User Interface (TUI)** for the Selfware agent swarm system. The UI offers real-time visualization of multi-agent collaboration, inspired by the interactive features of Qwen Code CLI.

## Features

### 🎨 Visual Design
- **Split-pane dashboard layout** - Monitor multiple aspects simultaneously
- **Role-based agent avatars** - Visual distinction between Architect, Coder, Tester, etc.
- **Activity indicators** - Real-time pulse animations showing agent activity
- **Color-coded status** - Warm amber palette (inspired by Selfware's garden theme)

### 🤖 Agent Swarm Visualization
- **Live agent status** - Working, Idle, Waiting, Completed, Error states
- **Trust score display** - Visual indicator of agent reliability
- **Activity level dots** - ○○○○○ to ●●●●● scale
- **Role icons** - 🏗️ Architect, 💻 Coder, 🧪 Tester, etc.

### 🧠 Shared Memory Browser
- **Key-value display** - Browse swarm's shared memory
- **Access tracking** - View read counts
- **Tag filtering** - Organize by tags
- **Modification history** - Track who changed what

### 📋 Task Queue Monitor
- **Priority visualization** - P1-P10 color-coded
- **Progress tracking** - ████████░░ style bars
- **Assignment status** - Which agents are working on what
- **Result aggregation** - Track completion status

### ⚖️ Decision & Consensus Tracking
- **Active decisions** - Questions being voted on
- **Vote counts** - Real-time vote tallying
- **Conflict detection** - Visual alerts for disagreements
- **Resolution tracking** - See outcomes

### 📜 Event Log
- **Timestamped events** - [14:32:15] style timestamps
- **Event type icons** - ▶ Started, ✓ Completed, ⚠️ Error, etc.
- **Agent attribution** - See which agent triggered events
- **Filtered view** - Color-coded by severity

## Quick Start

### Running the Swarm UI

```bash
# Run with default dev swarm (4 agents)
cargo run --example swarm_ui_demo

# Or use the library directly in your code
use selfware::ui::tui::run_tui_swarm;

fn main() -> anyhow::Result<()> {
    run_tui_swarm()
}
```

### Creating Custom Swarms

```rust
use selfware::orchestration::swarm::{Agent, AgentRole, Swarm};
use selfware::ui::tui::run_tui_swarm_with_roles;

// Define roles
let roles = vec![
    AgentRole::Architect,
    AgentRole::Coder,
    AgentRole::Tester,
    AgentRole::Security,
];

// Launch UI
run_tui_swarm_with_roles(roles)?;
```

### Advanced: Custom Swarm with Expertise

```rust
use selfware::orchestration::swarm::{Agent, AgentRole, Swarm};
use std::sync::{Arc, RwLock};

let mut swarm = Swarm::new();

swarm.add_agent(
    Agent::new("Alice", AgentRole::Architect)
        .with_expertise("Microservices")
        .with_expertise("Rust")
);

swarm.add_agent(
    Agent::new("Bob", AgentRole::Security)
        .with_expertise("Cryptography")
        .with_expertise("OWASP")
);

// Use with SwarmApp...
```

## Keyboard Controls

| Key | Action |
|-----|--------|
| `q` / `Ctrl+C` | Quit the application |
| `?` | Toggle help overlay |
| `Space` | Pause/resume swarm updates |
| `r` | Force refresh of swarm state |
| `s` | Sync swarm state |
| `t` | Add sample task to queue |
| `c` | Create sample decision |
| `v` | Cast sample vote |
| `Tab` | Cycle focus between panes |
| `z` | Toggle zoom on focused pane |
| `Esc` | Unzoom / close overlay |
| `Alt+1` | Focus layout (single pane) |
| `Alt+2` | Coding layout (chat + editor) |
| `Alt+3` | Dashboard layout (full view) |

## Layouts

### Dashboard Layout (Default)
```
┌─────────────────────────────────────────────────────────┐
│ 🤖 Swarm │ 6 Agents │ 3 Tasks │ Trust: 78%             │
├───────────────────┬─────────────────────────────────────┤
│                   │                                     │
│  Agent Swarm      │   🧠 Shared Memory                  │
│  🤖 Arch ●●●○○    │   📄 auth::design [arch] (5 reads) │
│  💻 Code ●●●●○    │   📄 impl::token [coder] (3 reads) │
│  🧪 Test ●●○○○    │                                     │
│                   ├─────────────────────────────────────┤
│                   │   📋 Task Queue                     │
│                   │   P5 ▶ Implement auth (2 results)  │
├───────────────────┴─────────────────────────────────────┤
│ 📜 Swarm Events                                         │
│ [14:32:01] ▶ Architect proposed design                  │
│ [14:32:15] ✓ Coder completed implementation             │
└─────────────────────────────────────────────────────────┘
```

### Focus Layout
Single-pane distraction-free mode for focused work.

### Coding Layout
Side-by-side chat and code editor view.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Swarm UI Architecture                │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  ┌─────────────┐     ┌─────────────┐     ┌───────────┐ │
│  │   Swarm     │◄───►│ SwarmUiState│◄───►│  TUI      │ │
│  │  (Core)     │     │  (Adapter)  │     │ Renderer  │ │
│  └─────────────┘     └─────────────┘     └───────────┘ │
│         │                   │                  │        │
│         ▼                   ▼                  ▼        │
│  ┌─────────────┐     ┌─────────────┐     ┌───────────┐ │
│  │   Agents    │     │   Events    │     │  Layout   │ │
│  │   Tasks     │     │   Memory    │     │  Engine   │ │
│  │  Decisions  │     │    Views    │     │  Widgets  │ │
│  └─────────────┘     └─────────────┘     └───────────┘ │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

## Module Structure

```
src/ui/tui/
├── mod.rs              # Main TUI module, exports, run_tui_swarm()
├── swarm_app.rs        # SwarmApp struct, event handling
├── swarm_state.rs      # SwarmUiState, AgentUiState, views
├── swarm_widgets.rs    # Widget renderers
├── animation/
│   ├── agent_avatar.rs # Animated agent avatars
│   └── message_flow.rs # Inter-agent message animation
└── layout.rs           # Pane management, layouts
```

## API Reference

### SwarmApp

```rust
pub struct SwarmApp {
    pub state: SwarmAppState,
    pub swarm_state: SwarmUiState,
    pub layout_engine: LayoutEngine,
    pub show_help: bool,
    // ...
}

impl SwarmApp {
    pub fn new() -> Self;
    pub fn with_swarm(swarm: Arc<RwLock<Swarm>>) -> Self;
    pub fn with_config(roles: Vec<AgentRole>) -> Self;
    pub fn render(&mut self, frame: &mut Frame);
    pub fn on_tick(&mut self);
    pub fn handle_event(&mut self, event: Event) -> bool;
}
```

### SwarmUiState

```rust
pub struct SwarmUiState {
    pub agents: Vec<AgentUiState>,
    pub memory_entries: Vec<MemoryEntryView>,
    pub decisions: Vec<DecisionView>,
    pub tasks: Vec<TaskView>,
    pub events: Vec<SwarmEvent>,
    pub stats: SwarmStats,
}

impl SwarmUiState {
    pub fn new(swarm: Arc<RwLock<Swarm>>) -> Self;
    pub fn sync(&mut self);
    pub fn add_event(&mut self, event_type: EventType, message: impl Into<String>, agent_id: Option<String>);
}
```

## Customization

### Themes

The UI uses the Selfware color palette:

| Color | Hex | Usage |
|-------|-----|-------|
| Amber | `#D4A373` | Primary actions |
| Garden Green | `#606C38` | Success, active |
| Soil Brown | `#BC6C25` | Warnings |
| Ink | `#283618` | Backgrounds |
| Parchment | `#FEFAE0` | Light text |

### Agent Role Colors

```rust
AgentRole::Architect => Color::Cyan,
AgentRole::Coder => Color::Blue,
AgentRole::Tester => Color::Green,
AgentRole::Reviewer => Color::Magenta,
AgentRole::Security => Color::Red,
// ... etc
```

## Future Enhancements

1. **Message Flow Animation** - Visualize messages traveling between agents
2. **3D Spatial View** - Canvas-based agent positioning
3. **Voice Notifications** - Audio cues for important events
4. **WebSocket Interface** - Remote monitoring capability
5. **Custom Themes** - User-defined color schemes
6. **Export Sessions** - Save/load swarm state
7. **Performance Metrics** - Real-time graphs

## Comparison with Qwen Code CLI

| Feature | Qwen Code CLI | Selfware Swarm UI |
|---------|---------------|-------------------|
| Multi-agent | ✅ Yes | ✅ Yes (up to 16) |
| Real-time UI | ✅ Yes | ✅ Yes |
| Agent roles | ✅ Yes | ✅ Yes (8 roles) |
| Shared memory | ❌ No | ✅ Yes |
| Consensus voting | ❌ No | ✅ Yes |
| Open source | ✅ Yes | ✅ Yes |
| Local LLMs | ✅ Yes | ✅ Yes |
| Terminal UI | ✅ Yes | ✅ Yes (ratatui) |

## Troubleshooting

### Terminal not restoring properly
If the terminal doesn't restore after quitting:
```bash
reset
# or
stty sane
```

### Performance issues
Reduce sync frequency:
```rust
app.sync_interval = Duration::from_millis(1000); // Instead of 500ms
```

### Colors not displaying
Ensure your terminal supports 256 colors:
```bash
echo $TERM
# Should show: xterm-256color or similar
```

## References

- [Qwen Code CLI Documentation](https://qwenlm.github.io/qwen-code-docs/)
- [Selfware README](../README.md)
- [ratatui Documentation](https://docs.rs/ratatui/)
- [Agent Swarm Implementation](../src/orchestration/swarm/mod.rs)
