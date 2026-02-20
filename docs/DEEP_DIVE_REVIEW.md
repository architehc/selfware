# Deep Dive Code Review

**Review Type**: Line-by-line analysis  
**Scope**: Agent Swarm UI + Mega Test Infrastructure  
**Method**: Static analysis, security audit, performance review  

---

## Part 1: Swarm State (`swarm_state.rs`)

### 1.1 Line-by-Line Analysis

#### Line 9: Unused Import
```rust
use crate::ui::tui::animation::agent_avatar::{ActivityLevel, AgentRole as AvatarRole};
```
**Issue**: `AvatarRole` is imported but only used in one method (`avatar_role()` at line 57).
**Impact**: Minimal, but creates confusion between `AgentRole` (domain) and `AvatarRole` (UI).
**Recommendation**: Consider unifying these types or clearly documenting the mapping.

#### Lines 13-25: AgentUiState Struct
```rust
pub struct AgentUiState {
    pub id: String,
    pub name: String,
    pub role: AgentRole,
    pub status: AgentStatus,
    pub activity: ActivityLevel,
    pub trust_score: f32,
    pub tokens_processed: u64,  // ALWAYS 0 (line 49)
    pub current_task: Option<String>,  // ALWAYS None (line 50)
    pub position: (u16, u16),
    pub success_rate: f32,
}
```
**Critical Issues**:
- `tokens_processed` (line 21): Always initialized to 0, never updated
- `current_task` (line 22): Always None, never populated

**Security**: No input validation on `name` field - could contain terminal escape sequences.

**Performance**: All fields are public - breaks encapsulation. Consider getter methods.

#### Lines 29-38: status_to_activity()
```rust
fn status_to_activity(status: AgentStatus) -> ActivityLevel {
    match status {
        AgentStatus::Idle => ActivityLevel::Idle,
        AgentStatus::Working => ActivityLevel::High,
        AgentStatus::Waiting => ActivityLevel::Medium,
        AgentStatus::Completed => ActivityLevel::Complete,
        AgentStatus::Error => ActivityLevel::Error,
        AgentStatus::Paused => ActivityLevel::Idle,  // QUESTIONABLE
    }
}
```
**Logic Issue**: `Paused` maps to `Idle` activity level. This might confuse users - a paused agent isn't idle, it's intentionally stopped.
**Recommendation**: Add `ActivityLevel::Paused` variant or document this mapping.

#### Lines 41-54: from_agent()
```rust
pub fn from_agent(agent: &Agent) -> Self {
    Self {
        id: agent.id.clone(),          // String clone - OK
        name: agent.name.clone(),      // String clone - OK
        role: agent.role,              // Copy - OK
        status: agent.status,          // Copy - OK
        activity: Self::status_to_activity(agent.status),
        trust_score: agent.trust_score,  // Copy - OK
        tokens_processed: 0,           // MISSING: actual token count
        current_task: None,            // MISSING: task lookup
        position: (0, 0),              // Calculated later
        success_rate: agent.success_rate(),  // Method call
    }
}
```
**Performance**: 2 heap allocations (String clones) per agent per sync cycle. With 16 agents syncing every 500ms = 64 allocations/second.

**Correctness**: `tokens_processed` and `current_task` are never populated from the source `Agent`.

#### Lines 57-70: avatar_role()
```rust
pub fn avatar_role(&self) -> Option<AvatarRole> {
    match self.role {
        AgentRole::Architect => Some(AvatarRole::Architect),
        // ... 7 more variants
        _ => None,  // General variant
    }
}
```
**API Design**: Returns `Option` but caller at line 59 in `swarm_widgets.rs` doesn't handle `None` case.
**Risk**: If role is `General`, avatar rendering might fail silently.

#### Lines 85-90: MemoryEntryView::from_entry()
```rust
let preview = if entry.value.len() > 50 {
    format!("{}...", &entry.value[..50])  // UTF-8 BUG!
} else {
    entry.value.clone()
};
```
**CRITICAL BUG**: Byte-indexing at 50 can panic on multi-byte UTF-8 characters.

**Exploit Scenario**:
```rust
let entry = MemoryEntry {
    value: "🎉".repeat(20),  // Each emoji is 4 bytes
    // ...
};
// entry.value.len() == 80 bytes
// &entry.value[..50] splits in the MIDDLE of an emoji
// PANIC: byte index 50 is not a char boundary
```

**Fix**:
```rust
let preview: String = entry.value.chars().take(50).collect();
if entry.value.chars().count() > 50 {
    format!("{}...", preview)
} else {
    preview
}
```

#### Lines 116-125: DecisionView::from_decision()
```rust
pub fn from_decision(decision: &Decision) -> Self {
    Self {
        id: decision.id.clone(),
        question: decision.question.clone(),
        options: decision.options.clone(),
        vote_count: decision.votes.len(),  // CORRECT
        status: decision.status,
        outcome: decision.outcome.clone(),
    }
}
```
**OK**: Correctly calculates vote count from decision votes.

#### Lines 141-150: TaskView::from_task()
```rust
pub fn from_task(task: &SwarmTask) -> Self {
    Self {
        id: task.id.clone(),
        description: task.description.clone(),
        priority: task.priority,
        status: task.status,
        assigned_agents: task.assigned_agents.clone(),
        result_count: task.results.len(),  // CORRECT
    }
}
```
**OK**: Properly counts results.

#### Lines 179-194: EventType::icon()
```rust
pub fn icon(&self) -> &'static str {
    match self {
        EventType::AgentStarted => "▶",
        // ...
        EventType::ConsensusReached => "🤝",  // 4-byte UTF-8
    }
}
```
**Compatibility Issue**: Emojis may not render in all terminals.
**Width Issue**: Emojis are typically 2 columns wide, but code assumes 1 column in some places.

#### Lines 236-297: sync() - CRITICAL SECTION
```rust
pub fn sync(&mut self) {
    let (agents_data, swarm_stats_opt, memory_entries_opt, decisions_data, tasks_data) = {
        if let Ok(swarm) = self.swarm.read() {  // LINE 239 - SILENT POISONING
```

**CRITICAL**: Line 239 silently ignores poisoned locks:
```rust
if let Ok(swarm) = self.swarm.read() {
    // If lock is poisoned, this branch is skipped
    // UI shows EMPTY data without any warning!
}
```

**Attack Scenario**:
1. Thread A panics while holding write lock
2. Lock becomes poisoned
3. sync() is called
4. `self.swarm.read()` returns `Err(PoisonError)`
5. `if let Ok(...)` skips the block
6. All data vectors are set to empty
7. UI shows "No agents active" - misleading!

**Fix**:
```rust
let swarm = match self.swarm.read() {
    Ok(s) => s,
    Err(e) => {
        tracing::error!("Lock poisoned, recovering: {}", e);
        e.into_inner()  // Recover the guard
    }
};
```

**Performance Analysis**:
- Lines 240-270: Collects all data in a closure - GOOD (minimizes lock time)
- Lines 279-296: Updates self outside lock - GOOD
- But: 4 separate data structures rebuilt on every sync (could be optimized with diffing)

#### Lines 300-307: calculate_agent_positions()
```rust
fn calculate_agent_positions(&mut self) {
    let cols = 2u16;  // HARDCODED
    for (i, agent) in self.agents.iter_mut().enumerate() {
        let col = (i as u16) % cols;
        let row = (i as u16) / cols;
        agent.position = (col * 15, row * 5);  // Magic numbers
    }
}
```
**Issues**:
- Hardcoded 2 columns - not responsive to terminal size
- Magic numbers 15 and 5 - no explanation
- Doesn't handle overflow (what if row * 5 > u16::MAX?)

#### Lines 310-335: update_stats()
```rust
fn update_stats(&mut self, swarm_stats: &crate::orchestration::swarm::SwarmStats) {
    self.stats = SwarmStats {
        total_agents: swarm_stats.total_agents,
        // ...
        completed_tasks: swarm_stats
            .agents_by_status
            .get(&AgentStatus::Completed)
            .copied()
            .unwrap_or(0),  // BUG: This counts AGENTS, not tasks!
```

**BUG**: Line 326-330: `completed_tasks` is populated from `agents_by_status`, which counts **completed agents**, not completed tasks.

**Correct logic should be**:
```rust
completed_tasks: self.tasks.iter().filter(|t| 
    matches!(t.status, TaskStatus::Completed)
).count(),
```

#### Lines 337-355: add_event()
```rust
pub fn add_event(&mut self, ...) {
    self.events.push(SwarmEvent { ... });
    
    if self.events.len() > 100 {
        self.events.remove(0);  // O(n) - expensive!
    }
}
```

**Performance Issue**: `Vec::remove(0)` is O(n) - shifts all elements.

**Worst case**: 100 events, remove(0) called repeatedly = O(n²) = 10,000 operations.

**Fix**: Use `VecDeque` for O(1) pop_front():
```rust
pub events: VecDeque<SwarmEvent>,
// ...
if self.events.len() > 100 {
    self.events.pop_front();
}
```

#### Lines 357-369: Accessor Methods
```rust
pub fn get_agent(&self, id: &str) -> Option<&AgentUiState> {
    self.agents.iter().find(|a| a.id == id)  // O(n) lookup
}

pub fn swarm(&self) -> Arc<RwLock<Swarm>> {
    Arc::clone(&self.swarm)  // Increments ref count
}
```

**Performance**: O(n) agent lookup. Should use `HashMap<String, AgentUiState>` for O(1).

**Safety**: `swarm()` returns cloned Arc - caller could hold lock indefinitely, blocking sync().

---

## Part 2: Swarm App (`swarm_app.rs`)

### 2.1 Line-by-Line Analysis

#### Lines 21-28: SwarmAppState Enum
```rust
pub enum SwarmAppState {
    Running,
    Paused,
    Help,
    CreatingDecision,  // UNUSED
    Voting,            // UNUSED
}
```

**Dead Code**: `CreatingDecision` and `Voting` are never used. The app uses `show_help: bool` instead of `SwarmAppState::Help`.

#### Lines 31-40: SwarmApp Struct
```rust
pub struct SwarmApp {
    pub state: SwarmAppState,
    pub swarm_state: SwarmUiState,
    pub layout_engine: LayoutEngine,
    pub show_help: bool,           // Redundant with SwarmAppState::Help
    pub last_sync: Instant,
    pub sync_interval: Duration,
    pub selected_decision: usize,  // UNUSED - always 0
    pub input_buffer: String,      // UNUSED - always empty
}
```

**Dead Fields**:
- `selected_decision`: Never modified after initialization
- `input_buffer`: Never used (no text input mode implemented)

#### Lines 44-71: new() and with_swarm()
```rust
pub fn new() -> Self {
    let swarm = Arc::new(RwLock::new(create_dev_swarm()));
    Self::with_swarm(swarm)
}
```

**Issue**: `new()` is infallible but could panic if `create_dev_swarm()` fails (though unlikely).

**Initialization Order**:
1. Line 52: Creates LayoutEngine
2. Line 54-62: Initializes all fields
3. Line 66: Calls `sync()` - potential panic if swarm lock poisoned
4. Line 67-68: Adds event

**Risk**: If sync() fails due to lock poisoning, app starts with empty state and no error message.

#### Lines 73-84: with_config()
```rust
pub fn with_config(roles: Vec<AgentRole>) -> Self {
    let mut swarm = Swarm::new();
    
    for (i, role) in roles.iter().enumerate() {
        let name = format!("{}-{}", role.name(), i + 1);
        swarm.add_agent(crate::orchestration::swarm::Agent::new(name, *role));
    }
    // ...
}
```

**Issue**: No validation on `roles` vector. Empty vector creates empty swarm (valid but possibly unintended).

**Performance**: Creates agents sequentially - could be parallelized with `rayon`.

#### Lines 86-138: render()
```rust
pub fn render(&mut self, frame: &mut Frame) {
    let area = frame.size();
    let layouts = self.layout_engine.calculate_layout(area);
    
    for (pane_id, pane_area) in &layouts {
        if let Some(pane) = self.layout_engine.get_pane(*pane_id) {
            match pane.pane_type {
                PaneType::StatusBar => { ... }
                PaneType::Chat => { ... }
                // ...
                _ => {
                    // Render placeholder
                    let block = ratatui::widgets::Block::default()
                        .borders(ratatui::widgets::Borders::ALL)
                        .title(pane.pane_type.title());
                    frame.render_widget(block, *pane_area);
                }
            }
        }
    }
    // ...
}
```

**Performance**: Recreates all widgets every frame - no caching. With 60 FPS and 10 panes = 600 widget creations/second.

**Correctness**: Line 118-124 placeholder rendering will overwrite any content in unhandled pane types.

#### Lines 160-262: handle_event()

**Key Binding Analysis**:

| Key | Action | Conflict? |
|-----|--------|-----------|
| `q` | Quit | No |
| `Ctrl+C` | Quit | Duplicate with 'q' |
| `?` | Toggle help | No |
| `Space` | Pause | No |
| `r` | Refresh | No |
| `Alt+1/2/3` | Layouts | No |
| `Tab` | Focus next | No |
| `z` | Zoom | No |
| `c` | Create decision | **Conflicts with Ctrl+C help text** |
| `v` | Vote | No |
| `t` | Add task | No |
| `s` | Sync | No |

**Line 176**: `Ctrl+C` handling
```rust
KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
    return false;  // Quits
}
```

**Line 243**: 'c' handling
```rust
KeyCode::Char('c') => {
    self.create_sample_decision();  // Creates decision
}
```

**Documentation Bug**: Help text says "q / Ctrl+C" for quit, but 'c' alone creates a decision. Users might accidentally create decisions trying to quit.

**Line 179**: 'q' quit
```rust
KeyCode::Char('q') => {
    return false;
}
```

**Missing**: "Press q twice" logic mentioned in help text. Should track time between presses.

#### Lines 264-275: toggle_pause()
```rust
fn toggle_pause(&mut self) {
    if self.state == SwarmAppState::Paused {
        self.state = SwarmAppState::Running;
        self.swarm_state
            .add_event(EventType::AgentStarted, "Swarm resumed", None);
    } else {
        self.state = SwarmAppState::Paused;
        self.swarm_state
            .add_event(EventType::AgentStarted, "Swarm paused", None);  // WRONG EVENT TYPE
    }
}
```

**Bug**: Both branches use `EventType::AgentStarted`. Pause should use a different event type or add `EventType::SwarmPaused`.

#### Lines 277-288: add_sample_task()
```rust
fn add_sample_task(&mut self) {
    if let Ok(mut swarm) = self.swarm_state.swarm().write() {
        let task = SwarmTask::new("Sample task from UI")
            .with_role(AgentRole::Coder)
            .with_priority(5);
        swarm.queue_task(task);
        // ...
    }  // Lock dropped here
}
```

**Issue**: Silent failure if lock is poisoned. No user feedback that task wasn't added.

**Suggestion**: Return `bool` and show error in UI.

#### Lines 290-304: create_sample_decision()
```rust
fn create_sample_decision(&mut self) {
    if let Ok(mut swarm) = self.swarm_state.swarm().write() {
        let decision_id = swarm.create_decision(
            "Should we use async/await?",  // HARDCODED
            vec!["Yes".to_string(), "No".to_string()],
        );
        // ...
    }
}
```

**Hardcoded Values**: Question and options are hardcoded. Should be configurable or interactive.

#### Lines 306-334: cast_sample_vote()
```rust
fn cast_sample_vote(&mut self) {
    if let Ok(mut swarm) = self.swarm_state.swarm().write() {
        let decisions: Vec<_> = swarm
            .list_decisions()
            .into_iter()
            .filter(|d| d.is_pending())
            .collect();
        
        if let Some(decision) = decisions.first() {
            let agents: Vec<_> = swarm.list_agents();
            if let Some(agent) = agents.first() {
                let _ = swarm.vote(&decision_id, &agent_id, "Yes", 0.8, "Good approach");
                // Ignores vote result!
            }
        }
    }
}
```

**Issue**: `let _ =` ignores vote result. If vote fails (e.g., agent already voted), no feedback.

**Logic Issue**: Always votes "Yes" with first agent. No way to vote "No" or choose different agent.

#### Lines 336-339: is_running()
```rust
pub fn is_running(&self) -> bool {
    true // Can be extended for shutdown logic
}
```

**Dead Code**: Always returns true. The `state` field already tracks this.

---

## Part 3: Swarm Widgets (`swarm_widgets.rs`)

### 3.1 Rendering Functions

#### Lines 20-37: render_swarm_status_bar()
```rust
pub fn render_swarm_status_bar(frame: &mut Frame, area: Rect, stats: &SwarmStats) {
    let status_text = format!(
        " 🤖 Swarm │ {} Agents ({} active, {} idle) │ {} Tasks │ {} Decisions │ Trust: {:.0}% ",
        stats.total_agents,
        stats.active_agents,
        stats.idle_agents,
        stats.pending_tasks,      // Shows pending, not total
        stats.pending_decisions,  // Shows pending, not total
        stats.average_trust * 100.0
    );
```

**UI Issue**: Shows "pending_tasks" and "pending_decisions" which might confuse users expecting totals.

**Overflow Risk**: `stats.average_trust * 100.0` could overflow if trust is NaN or infinity (though unlikely with valid data).

#### Lines 55-131: render_agent_swarm()

**Emoji Handling**:
```rust
let (icon, name) = match agent.role {
    AgentRole::Architect => ("🏗️", "Architect"),  // Multi-byte emoji
```

**Width Calculation Bug**: The emoji "🏗️" is typically 2 columns wide in terminals, but line 112 assumes fixed width:
```rust
Span::styled(format!("{:12}", agent.name), ...)  // Assumes 12 chars
```

If agent.name is 10 chars + 2-char emoji = 12 display columns. But if terminal doesn't support emoji, it might show as "?" (1 column), breaking alignment.

**Activity Dots**:
```rust
let activity_dots = match agent.activity {
    ActivityLevel::Idle => "○○○○○",
    ActivityLevel::Complete => "●●●●● ✓",  // 8 chars vs 5!
```

**Alignment Issue**: "●●●●● ✓" is 8 characters, breaking column alignment with other variants.

#### Lines 164-167: render_shared_memory()
```rust
let items: Vec<ListItem> = entries
    .iter()
    .take(chunks[1].height as usize)  // Cast from u16
```

**Cast Safety**: `chunks[1].height` is `u16`, cast to `usize`. On 32-bit systems this is fine, but explicit `min()` would be safer.

#### Lines 392-398: render_swarm_health()
```rust
let gauge = Gauge::default()
    .gauge_style(Style::default().fg(health_color))
    .ratio(health)  // f64
    .label(format!("{} {} ({:.0}%)", icon, stage, health * 100.0));
```

**Precision Loss**: `{:.0%}` rounds to integer, losing precision. Health of 0.749 shows as 75% (rounded up).

**Icon Width**: Emoji icons have varying widths. "🥀" vs "🌸" might cause label misalignment.

---

## Part 4: Test Infrastructure (`test_runner.py`)

### 4.1 Architecture Analysis

#### Lines 34-73: TestConfig
```python
@dataclass
class TestConfig:
    project_specs: Dict = None  # Mutable default!
    
    def __post_init__(self):
        if self.project_specs is None:
            self.project_specs = self._default_specs()
```

**Bug Pattern**: While `None` is used here (safe), the pattern is risky. If someone changes to `project_specs: Dict = {}`, it becomes a shared mutable default.

**Recommendation**: Use `field(default_factory=dict)`.

#### Lines 92-144: CheckpointManager

**Race Condition**:
```python
def create_checkpoint(self, phase: str, metrics: SessionMetrics) -> Path:
    checkpoint_id = f"checkpoint_{int(time.time())}"  # Not unique enough!
    checkpoint_path = self.checkpoint_dir / f"{checkpoint_id}.json"
```

If two checkpoints created in same second: **COLLISION**.

**Fix**: Use UUID or nanosecond timestamp.

**Stub Implementation** (Line 139-143):
```python
def restore_checkpoint(self, checkpoint_path: Path) -> bool:
    logger.info(f"Restoring from checkpoint: {checkpoint_path}")
    # Implementation would restore agent states, etc.
    return True  # ALWAYS RETURNS TRUE!
```

**CRITICAL**: This is a stub that always succeeds. No actual restoration happens.

**Correct Implementation**:
```python
def restore_checkpoint(self, checkpoint_path: Path) -> bool:
    try:
        with open(checkpoint_path) as f:
            data = json.load(f)
        
        # Validate checkpoint
        required_keys = ['id', 'timestamp', 'phase', 'metrics']
        if not all(k in data for k in required_keys):
            raise ValueError("Invalid checkpoint format")
        
        # Restore git state
        if data.get('git_commit'):
            subprocess.run(['git', 'checkout', data['git_commit']], 
                         check=True, cwd=self.session_dir)
        
        # Restore would continue here...
        return True
    except Exception as e:
        logger.exception("Restoration failed")
        return False
```

#### Lines 188-349: MegaTestRunner

**Signal Handler Race** (Lines 204-211):
```python
def _signal_handler(self, signum, frame):
    logger.info(f"Received signal {signum}, initiating graceful shutdown...")
    self.running = False
```

**Problem**: Python signal handlers run on the main thread. If the main thread is in `time.sleep(30)` (line 280), the signal won't be processed until sleep completes.

**Fix**: Use `threading.Event` for interruptible sleep.

**Simulated Metrics** (Lines 285-300):
```python
def _update_metrics(self):
    # TODO: Query actual agent metrics
    # For now, use simulated growth
    self.metrics.lines_of_code = int(1000 + elapsed * 0.5)  # FAKE!
```

**Issue**: Metrics are completely simulated. No actual data collection from agents.

**Health Check Stub** (Lines 311-318):
```python
def _health_check(self) -> bool:
    # TODO: Implement actual health checks
    return True
```

**CRITICAL**: Always returns True. Won't detect actual failures.

---

## Part 5: Security Audit

### 5.1 Injection Vulnerabilities

#### Command Injection in Bash Script (Lines 336-338)
```bash
if timeout "${DURATION_HOURS}h" ./target/release/selfware run "$PROJECT_PROMPT" \
```

**Risk**: `PROJECT_PROMPT` contains user-controlled content (from case statement, but could be modified).

**Exploit**:
```bash
PROJECT_TYPE="task_queue; rm -rf /"
# Results in: selfware run "...; rm -rf /"
```

**Fix**: Quote properly and validate inputs:
```bash
if [[ "$PROJECT_TYPE" =~ ^[a-z_]+$ ]]; then
    # Safe
fi
```

### 5.2 Path Traversal

#### Session Directory Creation (Line 193, test_runner.py)
```python
self.session_dir = Path("test_runs") / config.session_id
```

**Risk**: If `session_id` contains `../`, could write outside intended directory.

**Exploit**:
```python
config = TestConfig(session_id="../../../etc/cron.d/malicious")
# Creates: test_runs/../../../etc/cron.d/malicious/
```

**Fix**: Validate session_id format:
```python
if not re.match(r'^[a-zA-Z0-9_-]+$', config.session_id):
    raise ValueError("Invalid session_id")
```

### 5.3 Log Injection

#### Event Logging (swarm_state.rs:337-355)
```rust
pub fn add_event(&mut self, event_type: EventType, message: impl Into<String>, ...) {
    self.events.push(SwarmEvent {
        message: message.into(),  // No sanitization!
        // ...
    });
}
```

**Risk**: If message contains terminal escape sequences, could hijack terminal.

**Exploit**:
```rust
add_event(EventType::AgentError, "\x1B[2J\x1B[HYou've been hacked!", None);
// Clears screen and moves cursor
```

**Fix**: Sanitize or use a safe logging library.

---

## Part 6: Performance Analysis

### 6.1 Time Complexities

| Operation | Current | Optimized | Notes |
|-----------|---------|-----------|-------|
| Agent lookup | O(n) | O(1) | Use HashMap |
| Event removal | O(n) | O(1) | Use VecDeque |
| Sync | O(n) | O(∆n) | Use diffing |
| Render | O(n) | O(1) | Cache widgets |

### 6.2 Memory Usage

**Per-Agent Overhead**:
- `AgentUiState`: ~200 bytes (with String heap allocations)
- 16 agents: ~3.2 KB
- Sync every 500ms: 6.4 KB/s allocation rate

**Event Log**:
- 100 events × ~150 bytes = 15 KB max
- But O(n) removal causes temporary 2× memory during shift

### 6.3 Allocation Hotspots

**swarm_state.rs:244**:
```rust
.map(|a| AgentUiState::from_agent(a))  // Allocates per agent
```

Allocates new String for every agent name on every sync. With 16 agents at 2Hz = 32 String allocations/second.

**Fix**: Use `Arc<str>` for immutable names:
```rust
pub name: Arc<str>,  // Shared, immutable
```

---

## Part 7: Integration Issues

### 7.1 API Inconsistencies

| Location | Issue |
|----------|-------|
| `swarm_state.rs:9` | `AgentRole` vs `AvatarRole` - two enums for same concept |
| `swarm_app.rs:26-27` | Unused state variants |
| `swarm_widgets.rs:59-69` | Hardcoded emoji mapping instead of using `avatar_role()` |
| `test_runner.py` | Ignores `mega_test_config.toml` completely |

### 7.2 Configuration Drift

**TOML vs Python vs Bash**:

| Setting | TOML | Python Default | Bash Default |
|---------|------|----------------|--------------|
| Duration | 6h | 6h | 6h |
| Agents | 6 | 6 | 6 |
| Recovery attempts | 5 | ❌ Not used | ❌ Not used |
| Token budget | 5M | ❌ Not used | ❌ Not used |

**Result**: TOML configuration is decorative - Python and Bash use hard-coded values.

---

## Part 8: Summary of Critical Issues

### 🔴 MUST FIX (Production Blockers)

1. **UTF-8 Truncation Panic** (`swarm_state.rs:86`)
2. **Lock Poisoning Ignored** (`swarm_state.rs:239`)
3. **Checkpoint Restore Stub** (`test_runner.py:139`)
4. **Health Check Stub** (`test_runner.py:311`)
5. **Path Traversal Risk** (`test_runner.py:193`)

### 🟡 SHOULD FIX (High Priority)

6. **O(n) Event Removal** (`swarm_state.rs:353`)
7. **Simulated Metrics** (`test_runner.py:299`)
8. **Key Binding Conflict** (`swarm_app.rs:176 vs 243`)
9. **Configuration Ignored** (`test_runner.py` ignores TOML)
10. **Dead Code** (Multiple unused fields/variants)

### 🟢 NICE TO HAVE (Low Priority)

11. **Performance**: Use HashMap for agent lookup
12. **Performance**: Cache rendered widgets
13. **UX**: Add ASCII mode for emoji-free terminals
14. **Docs**: Fix `--features tui` in examples
15. **UX**: Implement "q twice" quit logic

---

## Part 9: Estimated Fix Effort

| Category | Issues | Hours |
|----------|--------|-------|
| Critical | 5 | 8 |
| High | 5 | 12 |
| Low | 5 | 6 |
| **Total** | **15** | **26** |

**Recommendation**: 4-5 days of focused work to reach production quality.
