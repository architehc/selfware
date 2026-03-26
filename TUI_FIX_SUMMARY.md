# TUI Garden View Fix

## What Was Broken

1. **Garden view existed but was inaccessible** - The `garden_view.rs` had full tree navigation code but was never integrated into the TUI event loop
2. **Confusing entry points** - `selfware garden` was CLI-only, `selfware --tui` had no garden, `selfware dashboard` was separate
3. **No keyboard controls** - Couldn't navigate the garden tree

## What Was Fixed

### Phase 1: Basic Integration ✅ COMPLETE

**Changes Made:**

1. **Added `GardenView` state to App** (`src/ui/tui/app.rs`)
   - Added `GardenView` variant to `AppState` enum
   - Added `garden_view: GardenView` field to `App` struct
   - Added `toggle_garden_view()` method
   - Added garden rendering in `render()` method

2. **Added garden accessor** (`src/ui/tui/garden_view.rs`)
   - Added `garden()` method to check if garden is loaded

3. **Added keyboard controls** (`src/ui/tui/mod.rs`)
   - `Ctrl+G` - Toggle garden view
   - `Esc` - Exit garden view
   - `↑/↓` - Navigate tree
   - `Enter/→` - Expand bed (directory)
   - `←` - Collapse bed
   - `r` - Refresh garden data

4. **Added escape handling** (`src/ui/tui/app.rs`)
   - `Esc` in garden view returns to chat

## How to Use

### Launch Chat TUI with Garden

```bash
# Start chat TUI
selfware

# In the TUI:
# - Press Ctrl+G to open garden view
# - Navigate with arrow keys
# - Press Enter to expand/collapse directories
# - Press r to refresh
# - Press Esc or Ctrl+G to return to chat
```

### Garden View Keys

| Key | Action |
|-----|--------|
| `Ctrl+G` | Toggle garden view |
| `↑` / `↓` | Navigate up/down |
| `Enter` / `→` | Expand selected bed |
| `←` | Collapse selected bed |
| `r` | Refresh garden data |
| `Esc` | Exit garden view |
| `q` (twice) | Quit TUI |

### What You See

```
┌─ 🌳 Garden View ☀️ ──────────────────────────────────────────┐
│                                                              │
│  ▼ 🌸 src/ (23 plants)                                       │
│      🌿 agent/                                               │
│      🌳 tools/                                               │
│      🌱 config/                                              │
│  ▶ tests/ (15 plants)                                        │
│  ▶ docs/ (8 plants)                                          │
│                                                              │
│  ┌─ Details ─────────────────────────────────────────────────┐│
│  │ Season: summer (active tending)                          ││
│  │ Plants: 127 across 12 beds                               ││
│  │ Lines: 45,231                                            ││
│  └──────────────────────────────────────────────────────────┘│
└──────────────────────────────────────────────────────────────┘
Status: Garden view: ↑↓ navigate, Enter expand, r refresh, Esc/Ctrl+G exit
```

## What's Still Missing (Future Work)

### Phase 2: Interactive Features (Next Priority)
- [ ] **Spawn agent from garden** - Press `s` on a file/dir to spawn agent
- [ ] **Show agent progress** - Highlight files being worked on
- [ ] **File preview** - Show file content in side panel
- [ ] **Quick actions** - Analyze/Refactor/Test buttons for selected item

### Phase 3: Full Integration
- [ ] **Split view** - Garden on left, chat on right
- [ ] **Drag and drop** - Drag files to agent panel
- [ ] **Real-time updates** - Garden updates as agents modify files
- [ ] **Task queue** - Visual queue of pending tasks per file

## Current Workarounds

Until Phase 2 is complete, use these workflows:

### Workflow 1: Garden + Manual Task
```bash
# Terminal 1: View garden to understand structure
selfware garden

# Terminal 2: Launch TUI and work on specific path
selfware
# Then: "Analyze src/tools/file.rs for improvements"
```

### Workflow 2: YOLO Mode with Path
```bash
# Run directly on a path
selfware -y run "Read all files in src/cognitive/, identify improvements, implement the safest one"
```

## Testing the Fix

```bash
# Build with TUI feature
cargo build --features tui --release

# Run TUI
./target/release/selfware

# In TUI:
# 1. Press Ctrl+G - should see garden view
# 2. Press ↓ to navigate
# 3. Press Enter to expand a directory
# 4. Press r to refresh
# 5. Press Esc to return to chat
```

## Related Files

- `src/ui/tui/app.rs` - Main app with garden integration
- `src/ui/tui/garden_view.rs` - Garden tree component
- `src/ui/tui/mod.rs` - Event handling for garden keys
- `src/ui/garden.rs` - Garden data model
- `docs/TUI_GARDEN_GUIDE.md` - Full user guide
