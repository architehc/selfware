# Selfware UX Recommendations

Based on comprehensive analysis of the codebase, here are recommendations to enhance the user experience while preserving the project's unique "garden workshop" aesthetic and philosophy.

---

## 1. Onboarding & First-Time Experience

### Current State
- No guided onboarding flow
- Users must manually create `selfware.toml` or rely on defaults
- Rich help system exists (`/help`) but isn't discoverable until users start

### Recommendations

#### 1.1 Interactive First-Run Wizard
```rust
// New: src/cli/first_run.rs
pub async fn first_run_wizard() -> Result<Config> {
    // Detect available local LLM backends (ollama, vLLM, LM Studio ports)
    // Guide users through:
    // 1. Selecting endpoint (with auto-detection)
    // 2. Choosing a model from available options
    // 3. Setting up allowed_paths with examples
    // 4. Explaining execution modes with safety implications
}
```

**Why**: The config file has 20+ options; a wizard reduces cognitive load and prevents misconfiguration.

#### 1.2 Example Config Generator
```bash
selfware init --template rust    # Rust project template
selfware init --template python  # Python project template
selfware init --template minimal # Minimal config
```

**Why**: Users currently copy-paste from README; templates ensure best practices per project type.

---

## 2. Command Discoverability

### Current State
- 30+ slash commands (`/help`, `/ctx`, `/compress`, etc.)
- Commands are documented but require typing `/help` to discover
- No fuzzy command matching

### Recommendations

#### 2.1 Fuzzy Command Matching
```rust
// In src/input/completer.rs
impl Completer for SelfwareCompleter {
    fn complete(&self, line: &str, pos: usize) -> Vec<Suggestion> {
        // Allow "ctx" to match "/context", "clr" to match "/clear"
        // Typo tolerance: "contxt" → "/context"
    }
}
```

**Why**: Reduces friction for common operations; users shouldn't memorize exact command names.

#### 2.2 Command Palette (Ctrl+P)
```
┌─────────────────────────────────────────┐
│  Command Palette              12/45    │
│  > diff                                 │
│    /diff          Git diff --stat       │
│    /undo          Undo last edit        │
│    /config        Show configuration    │
└─────────────────────────────────────────┘
```

**Why**: Power users prefer keyboard navigation; fuzzy search beats hierarchical menus.

#### 2.3 Contextual Command Hints
```rust
// Show relevant commands based on state
if has_uncommitted_changes() {
    println!("{} Tip: Use {} to see changes", Glyphs::LANTERN, "/diff".emphasis());
}
if context_near_limit() {
    println!("{} Context full: Try {}", Glyphs::WILT, "/compress".emphasis());
}
```

---

## 3. Progress & Feedback

### Current State
- Garden metaphors ("tending", "blooming", "frost") are charming
- Tool execution shows: "examining (file_read)..."
- Task completion shows duration
- Limited visibility into multi-step operations

### Recommendations

#### 3.1 Visual Progress for Long Operations
```
🌱 Planning phase...
   ├─ Analyzing codebase structure ●●●○○
   ├─ Identifying key files        ●●●●○
   └─ Building execution plan      ●●●●●

🔧 Executing phase...
   ├─ file_read: src/main.rs       ✓
   ├─ file_edit: src/lib.rs        ●●●
   └─ cargo_test                   ⏳
```

**Why**: Users running 100+ iteration tasks need visibility into progress, not just "working..."

#### 3.2 Token Usage Dashboard
```
┌─ Context Window ─────────────────────┐
│  ████████████████████░░░░  78% full  │
│  45,231 / 65,536 tokens               │
│  ├── System:   2,340 tokens           │
│  ├── Context: 32,891 tokens           │
│  └── History: 10,000 tokens           │
│                                       │
│  [Compress] [Clear] [Expand]          │
└──────────────────────────────────────┘
```

**Why**: Context window exhaustion is a common failure mode; visual feedback prevents it.

#### 3.3 Streaming Response Improvements
```rust
// Currently: stream text as it arrives
// Enhanced: Parse and format tool calls in real-time
pub fn render_streaming_chunk(chunk: &str) {
    // Detect XML tool calls and render them as:
    // "🔧 Calling: file_read(src/main.rs)..."
    // Instead of raw XML appearing in output
}
```

---

## 4. Safety & Trust

### Current State
- Four execution modes: Normal, AutoEdit, YOLO, Daemon
- Confirmation prompts with risk levels (LOW/MEDIUM/HIGH)
- Path validation with allowed/denied patterns
- Audit trail in YOLO mode

### Recommendations

#### 4.1 Visual Safety Indicator
```
┌─ Selfware Workshop ──────────────────────────────┐
│  🦊  Tending: my-project    [🔒 Normal Mode]     │
└──────────────────────────────────────────────────┘
```
- 🔒 Normal (confirm all)
- ✏️ AutoEdit (confirm destructive only)
- ⚡ YOLO (auto-approve)
- 🤖 Daemon (permanent YOLO)

**Why**: Mode indication should be persistent and visually distinct; users forget which mode they're in.

#### 4.2 Dry-Run Preview
```bash
$ selfware run "Refactor auth module" --dry-run

📋 Planned Operations:
   ├─ file_read: src/auth/mod.rs
   ├─ file_read: src/auth/login.rs
   ├─ file_write: src/auth/mod.rs (modifies 3 functions)
   ├─ file_write: src/auth/login.rs (adds 45 lines, removes 12)
   └─ cargo_test

💡 Run with --confirm to execute, or --yolo to skip confirmations
```

**Why**: Users want to see what will happen before committing; this builds trust.

#### 4.3 Undo Enhancements
```bash
# Current: /undo restores last edit
# Enhanced:
$ selfware journal --undo        # Undo last journal entry
$ selfware journal --rollback 3  # Rollback to checkpoint #3
$ selfware journal --diff 3      # Show diff of checkpoint #3
```

---

## 5. Context Management

### Current State
- `/ctx`, `/ctx clear`, `/ctx load <glob>`, `/ctx reload`, `/ctx copy`
- Context compression with `/compress`
- File watching for auto-refresh

### Recommendations

#### 5.1 Smart Context Loading
```rust
// New: Auto-detect project type and load relevant files
pub fn auto_load_context(&mut self) -> Vec<PathBuf> {
    match detect_project_type() {
        Rust => vec!["Cargo.toml", "src/lib.rs", "src/main.rs"],
        Python => vec!["pyproject.toml", "requirements.txt"],
        Node => vec!["package.json", "README.md"],
    }
}
```

**Why**: Users often forget to load key files; smart defaults reduce setup friction.

#### 5.2 Context Diff View
```
$ selfware ctx diff

Changed files since last load:
  M src/main.rs      (45 lines changed)
  A src/new_module.rs (new file)
  D src/old.rs       (deleted)

[Reload] [Ignore] [Review]
```

**Why**: Auto-refresh notification exists but doesn't show *what* changed.

#### 5.3 Token Budget Warnings
```rust
// Proactive warnings at 50%, 75%, 90%
if context_usage > 0.75 {
    println!("{} Context at {}% - consider /compress", 
             Glyphs::WILT, (context_usage * 100.0) as u32);
}
```

---

## 6. Multi-Agent & Swarm UX

### Current State
- `selfware multi-chat` for concurrent agents
- `/swarm <task>` for orchestrated agents
- Roles: Architect, Coder, Tester, Reviewer, DevOps, Security

### Recommendations

#### 6.1 Swarm Visualization
```
┌─ Active Swarm ──────────────────────────────┐
│                                             │
│  🏗️  Architect   planning...    ●●●○○      │
│  💻  Coder        waiting...     ⏸         │
│  🧪  Tester       waiting...     ⏸         │
│  👁️  Reviewer     waiting...     ⏸         │
│                                             │
│  [View Architect] [Pause All] [Cancel]      │
└─────────────────────────────────────────────┘
```

**Why**: Users need visibility into distributed work; current output is interleaved and confusing.

#### 6.2 Agent Handoff Notifications
```
✋ Architect has completed planning
   Handing off to Coder...

💻 Coder is now implementing:
   ├─ src/auth/mod.rs
   └─ src/auth/middleware.rs
```

---

## 7. Error Recovery & Resilience

### Current State
- Checkpoint system with `selfware journal` and `selfware resume`
- Self-healing with exponential backoff
- 4-hour timeout for slow models

### Recommendations

#### 7.1 Crash Recovery UX
```
$ selfware chat

🌱 Welcome back! It looks like Selfware didn't shut down cleanly.
   ├─ Last task: "Refactor authentication"
   ├─ Progress: Step 12/20
   └─ Last action: file_edit(src/auth.rs)

[Resume] [View Journal] [Start Fresh]
```

**Why**: Users shouldn't need to remember `selfware resume <task-id>` after a crash.

#### 7.2 Retry with Options
```
❌ API Error: Rate limit exceeded (429)

Retry options:
  [1] Retry now
  [2] Retry in 60 seconds (recommended by API)
  [3] Switch to backup model (qwen2.5-coder)
  [4] Save state and exit
```

---

## 8. Theming & Accessibility

### Current State
- Four themes: Amber (default), Ocean, Minimal, HighContrast
- Custom color palette with warm tones
- Garden metaphors throughout

### Recommendations

#### 8.1 Auto Theme Detection
```rust
// Respect system theme on first run
if config.ui.theme == "auto" {
    match detect_system_theme() {
        Dark => ThemeId::Amber,
        Light => ThemeId::Ocean,
        HighContrast => ThemeId::HighContrast,
    }
}
```

**Why**: Users expect apps to respect system preferences.

#### 8.2 Colorblind-Friendly Mode
```rust
pub enum ThemeId {
    Amber,
    Ocean,
    Minimal,
    HighContrast,
    ColorblindSafe, // New: uses patterns + symbols instead of color alone
}
```

**Why**: ✿ (bloom), ❀ (wilt), ❄ (frost) are color-coded; colorblind users need additional distinctions.

#### 8.3 Glyph Fallback
```rust
// Current: Hardcoded emojis
// Enhanced: Detect terminal capability
pub fn status_glyph(status: &str) -> &'static str {
    if supports_unicode() && !minimal_mode() {
        "🌸" // Bloom
    } else {
        "[OK]" // ASCII fallback
    }
}
```

**Why**: Terminal support varies; provide graceful degradation.

---

## 9. Output & Logging

### Current State
- `--compact` mode reduces visual chrome
- `--verbose` shows detailed tool output
- `--show-tokens` displays token usage
- Garden metaphors for all status messages

### Recommendations

#### 9.1 Structured Output for Scripting
```bash
$ selfware status --format json | jq '.journal.in_progress'
3

$ selfware run "test" --format json
{
  "success": true,
  "duration_ms": 45230,
  "tokens_used": 15432,
  "files_modified": ["src/lib.rs"],
  "tests_passed": true
}
```

**Why**: Enables CI/CD integration and external tooling.

#### 9.2 Log Levels with Visual Hierarchy
```
DEBUG: 🔧 Tool execution details
INFO:  🌱 General progress
WARN:  ⚠️  Issues that don't block
ERROR: ❄️  Failures requiring attention
FATAL: 💥 Unrecoverable errors
```

---

## 10. Keyboard Shortcuts & Input

### Current State
- Emacs (default) and Vi modes
- Ctrl+Y for YOLO toggle, Shift+Tab for AutoEdit
- Tab completion for commands
- Ctrl+X for external editor

### Recommendations

#### 10.1 Shortcut Help Overlay
```
┌─ Keyboard Shortcuts ────────────────────────┐
│                                             │
│  General:                                   │
│    Ctrl+C      Interrupt / Double-tap exit  │
│    Ctrl+L      Clear screen                 │
│    Ctrl+P      Command palette              │
│                                             │
│  Input:                                     │
│    Tab         Autocomplete                 │
│    Ctrl+R      History search               │
│    Ctrl+X      Open $EDITOR                 │
│    Ctrl+J      Newline                      │
│                                             │
│  Modes:                                     │
│    Ctrl+Y      Toggle YOLO mode             │
│    Shift+Tab   Toggle AutoEdit mode         │
│                                             │
│  [Close]                                    │
└─────────────────────────────────────────────┘
```

**Why**: Users can't remember 10+ shortcuts; quick reference improves retention.

#### 10.2 Custom Keybindings
```toml
# selfware.toml
[input.keybindings]
"ctrl+g" = "git_status"
"ctrl+t" = "run_tests"
"ctrl+s" = "save_chat"
```

**Why**: Power users want workflow-specific shortcuts.

---

## 11. Mobile/SSH Experience

### Current State
- TUI requires `--features tui`
- Basic mode fallback when terminal detection fails
- 4-hour timeout supports slow connections

### Recommendations

#### 11.1 Responsive TUI
```rust
// Detect terminal size and adjust layout
if terminal_width < 80 {
    // Compact layout: hide side panels, stack vertically
} else if terminal_width < 120 {
    // Standard layout
} else {
    // Wide layout: show file browser side panel
}
```

**Why**: Users work over SSH on various screen sizes.

#### 11.2 Session Persistence for SSH Drops
```rust
// Automatically save checkpoint on disconnect
pub async fn handle_disconnect(&mut self) {
    self.create_checkpoint("ssh_disconnect").await;
    println!("Session saved. Resume with: selfware resume {}", 
             self.task_id);
}
```

---

## 12. Metrics & Insights

### Current State
- `--show-tokens` displays token count
- `/cost` shows estimated API cost
- Session stats with `/stats`

### Recommendations

#### 12.1 Weekly Workshop Report
```
📊 Your Selfware Week in Review

Tasks Completed: 12
Time Saved: 4.5 hours
Code Written: 2,340 lines
Tests Added: 45

Most Used Tools:
  1. file_edit (45%)
  2. cargo_test (20%)
  3. grep_search (15%)

Cost: $2.34 (local inference)
```

**Why**: Gamification and metrics encourage continued use.

#### 12.2 Performance Insights
```
⚡ Slow Operations Detected:
   cargo_test took 4m 30s (average: 45s)
   Consider: Running tests in parallel with cargo test --jobs 4
```

---

## Implementation Priority

### High Impact / Low Effort (Do First)
1. Visual safety indicator (🔒/✏️/⚡)
2. Fuzzy command matching
3. Token usage dashboard
4. Keyboard shortcut help (`/shortcuts`)
5. Crash recovery prompt

### High Impact / High Effort (Plan Carefully)
1. Interactive first-run wizard
2. Swarm visualization
3. Smart context loading
4. TUI responsive layouts

### Nice to Have
1. Weekly reports
2. Custom keybindings
3. Colorblind-safe theme
4. Structured JSON output

---

## Preserving the Soul of Selfware

While implementing these recommendations, maintain:

- **Garden metaphors**: Keep "blooming", "frost", "tending" language
- **Warm aesthetic**: Amber tones, hand-crafted feel
- **Local-first**: Emphasize "homestead" vs "remote" indicators
- **Transparency**: Show what's happening, hide complexity behind metaphors
- **User agency**: Always provide escape hatches (Ctrl+C, confirmations)

The goal is to make Selfware feel like a well-worn tool that fits the hand perfectly—not like a cold, corporate AI product.
