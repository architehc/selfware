# TUI & Garden Workflow Guide

## Current State (What's Actually Working)

### 1. Available TUI Modes

| Command | What It Does | Status |
|---------|--------------|--------|
| `selfware --tui` or just `selfware` | Chat TUI with agent | ✅ Working |
| `selfware dashboard` | Swarm metrics dashboard | ✅ Working |
| `selfware garden` | **CLI-only** garden visualization | ⚠️ Static output |
| `selfware demo` | Animated demo scenarios | ✅ Working |

### 2. The Problem

**There is NO integrated "Garden TUI" that combines:**
- Interactive garden tree navigation
- Agent spawning from selected files
- Real-time swarm progress
- Combined view

The `garden_view.rs` exists but is **not connected** to any TUI event loop.

## How to Use What's Available Now

### Chat TUI (Default)

```bash
# Launch interactive chat TUI
selfware

# Or explicitly
selfware --tui
```

**Keys:**
- `Enter` - Send message
- `Ctrl+P` - Command palette
- `Tab` - Cycle panes
- `q` (twice) or `Ctrl+C` - Quit
- `?` - Help (if available)

### Garden CLI (Static)

```bash
# View codebase as garden (one-time output)
selfware garden

# Output example:
# 🌳 Your Digital Garden: selfware
# ☀️ Season: summer (active tending)
#
# Garden Summary:
#     🌱 127 plants across 12 beds
#     🌾 45,231 lines of carefully tended code
#     🌸 45 healthy, 0 need attention
#
# Growth Stages:
#     🌱 Seedlings (new code)  ████░░░░░░░░░░░░░░░░ 12
#     🌿 Sprouts (growing)    ████████░░░░░░░░░░░░ 23
#     🌿 Established          ████████████░░░░░░░░ 45
#     🌳 Mature               ██████░░░░░░░░░░░░░░ 38
#     🍂 Need attention       ░░░░░░░░░░░░░░░░░░░░ 0
#
# Garden Beds:
#     🌸 src/ — 23 plants, 12,456 lines
#     🌿 tests/ — 15 plants, 5,234 lines
#     ...
```

**Limitation:** This is a static snapshot, not interactive.

### Swarm Dashboard

```bash
# View swarm metrics (for active swarms)
selfware dashboard
```

Shows:
- Agent list and status
- Token usage
- Task queue
- Shared memory

### Demo Scenarios

```bash
# Run animated demos
selfware demo archaeology    # Codebase exploration
selfware demo feature        # Feature implementation
selfware demo bug-hunt       # Bug finding
```

## The Missing Integration

### What Users Expect (But Doesn't Exist)

```
┌─ Interactive Garden TUI ─────────────────────────────────────┐
│                                                              │
│  ┌─ Garden Tree ──────────────┐  ┌─ Agent Panel ───────────┐ │
│  │ ▼ 🌸 src/                  │  │ Active: 3 agents        │ │
│  │   🌿 agent/       [Select]│  │ Queue: 5 tasks          │ │
│  │   🌳 tools/       [Spawn]  │  │                         │ │
│  │   🌱 config/               │  │ Selected: src/tools/    │ │
│  │ ▶ tests/                   │  │                         │ │
│  │   🌿 unit/                 │  │ [Analyze] [Refactor]    │ │
│  │   🌿 integration/          │  │ [Test]    [Document]    │ │
│  └─────────────────────────────┘  └─────────────────────────┘ │
│                                                              │
│  ┌─ Output ─────────────────────────────────────────────────┐│
│  │ Agent-3: Analyzing src/tools/file.rs...                  ││
│  │ Agent-3: Found 2 potential improvements                  ││
│  │ Agent-7: Refactoring complete                            ││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  [?] Help  [q] Quit  [Tab] Focus  [Enter] Select/Spawn       │
└──────────────────────────────────────────────────────────────┘
```

### Current Workaround

Since the integrated view doesn't exist, use this workflow:

```bash
# 1. View garden to understand structure
selfware garden

# 2. In another terminal, spawn swarm on specific folder
selfware run "Read all files in src/tools/, analyze for refactoring opportunities, implement the safest improvement. Run tests after."

# 3. Or use multi-chat for parallel analysis
selfware multi-chat -n 4
# Then: "Each agent analyze one submodule in src/"
```

## Quick Fixes Needed

### 1. Add Garden to Chat TUI (`--tui`)

**File:** `src/ui/tui/app.rs`

Add garden view as a toggleable pane:

```rust
// In AppState, add:
pub enum AppState {
    Chatting,
    RunningTask,
    Palette,
    FileBrowser,
    GardenView,  // NEW
    Help,
}

// In handle_event, add key binding:
KeyCode::Char('g') if key.modifiers == KeyModifiers::CONTROL => {
    app.toggle_garden_view();
}
```

### 2. Wire Up Garden View Navigation

**File:** `src/ui/tui/garden_view.rs`

Add public methods for event handling:

```rust
impl GardenView {
    pub fn handle_key(&mut self, key: KeyCode) -> bool {
        match key {
            KeyCode::Up => self.select_prev(),
            KeyCode::Down => self.select_next(),
            KeyCode::Enter | KeyCode::Right => self.toggle_expand(),
            KeyCode::Left => self.collapse_if_expanded(),
            _ => return false,
        }
        true
    }
    
    pub fn get_selected_path(&self) -> Option<String> {
        self.selected_item().map(|item| match item {
            GardenItem::Bed { path, .. } => path.clone(),
            GardenItem::Plant { plant, .. } => plant.path.clone(),
        })
    }
}
```

### 3. Add Agent Spawn from Garden

**File:** `src/ui/tui/app.rs`

When user presses `s` in garden view:

```rust
KeyCode::Char('s') if app.state == AppState::GardenView => {
    if let Some(path) = app.garden_view.get_selected_path() {
        let task = format!("Analyze and improve {}", path);
        app.spawn_agent_on_path(&path);
    }
}
```

## Implementation Roadmap

### Phase 1: Basic Integration (1-2 hours)
- [ ] Add `Ctrl+G` to toggle garden view in chat TUI
- [ ] Wire up arrow keys for garden navigation
- [ ] Show selected path in status bar

### Phase 2: Interactive Features (2-4 hours)
- [ ] Press `s` to spawn agent on selected path
- [ ] Show agent progress in garden view
- [ ] Highlight files being worked on

### Phase 3: Full Integration (4-8 hours)
- [ ] Drag-and-drop tasks to agents
- [ ] Real-time garden updates as agents modify files
- [ ] Combined dashboard + garden view

## Workflows Using Current Tools

### Workflow 1: Explore Then Act

```bash
# 1. Understand codebase structure
selfware garden

# 2. Pick a "bed" (directory) to work on
# Let's say src/cognitive/ looks interesting

# 3. Run focused analysis
selfware run "Read all files in src/cognitive/, identify the 3 most impactful improvements for reliability, then implement the easiest one. Run cargo check and cargo test after."
```

### Workflow 2: Parallel Exploration

```bash
# 1. Start multi-agent chat
selfware multi-chat -n 4

# 2. Assign different areas to different agents:
# Agent 1: "Analyze src/agent/ for error handling issues"
# Agent 2: "Check src/tools/ for performance optimizations"  
# Agent 3: "Review src/cognitive/ for documentation gaps"
# Agent 4: "Examine tests/ for coverage improvements"

# 3. Review results and pick best recommendations
```

### Workflow 3: Evolution + Monitoring

```bash
# Terminal 1: Run evolution
selfware evolve --generations 20

# Terminal 2: Monitor swarm
selfware dashboard

# Terminal 3: Watch garden changes
watch -n 5 'selfware garden'
```

## Common Issues

### "Garden workflow seems broken"

**Reality:** There's no interactive garden workflow yet. The `selfware garden` command outputs static text and exits.

**Workaround:** Use the CLI workflow shown above.

### "How do I select files in TUI?"

**Reality:** The chat TUI doesn't have a file browser. The garden view exists in code but isn't integrated.

**Workaround:** Use `selfware run` with explicit paths.

### "Dashboard shows nothing"

**Reality:** Dashboard shows swarm metrics. If no swarm is active, it's empty.

**Fix:** Start a swarm first:
```bash
selfware evolve  # or
selfware multi-chat
```

## Debug Commands

```bash
# Check TUI feature is enabled
selfware --version  # Should show "tui" in features

# Test garden rendering
selfware garden --path src/

# Check swarm status
selfware status

# View logs
selfware journal
```

## Next Steps

1. **For Users:** Use the CLI workflows above until TUI integration is complete
2. **For Developers:** Pick a Phase 1 task from the roadmap
3. **For Contributors:** Help wire up garden_view.rs to the TUI event loop

---

**Related Files:**
- `src/ui/tui/app.rs` - Main TUI app
- `src/ui/tui/garden_view.rs` - Garden view component (not integrated)
- `src/ui/garden.rs` - Garden data model
- `src/cli.rs` - CLI command handling
