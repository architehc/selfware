//! Generate VLM benchmark fixture PNGs.
//!
//! Usage: cargo run --features vlm-bench --bin vlm_gen_fixtures

use selfware::vlm_bench::fixtures::{ensure_fixture_dir, text_to_png};
use std::path::Path;

fn main() {
    let base = Path::new("vlm_fixtures");

    println!("Generating VLM benchmark fixtures...\n");

    gen_l1(base);
    gen_l2(base);
    gen_l3(base);
    gen_l4(base);
    gen_l5(base);
    gen_mega(base);

    println!("\nDone! All fixtures generated.");
}

// ── L1: TUI State (Easy) — 4 PNGs ──────────────────────────────────────────

fn gen_l1(base: &Path) {
    let dir = ensure_fixture_dir(base, "l1_tui_state").expect("create l1 dir");

    // dashboard_normal.png
    let text = r#"
┌─ Selfware Dashboard ─────────────────────────────────────────────────────────┐
│ [Status: OK]                                        Theme: Dark    12:34:56  │
├──────────────────────────────────┬────────────────────────────────────────────┤
│  Agent Activity                  │  System Health                             │
│  ─────────────                   │  ─────────────                             │
│  ● Task: code review     [DONE]  │  CPU:  ████████░░  78%                     │
│  ● Task: refactor api    [RUN]   │  MEM:  ██████░░░░  62%                     │
│  ● Task: write tests     [WAIT]  │  DISK: ███░░░░░░░  31%                     │
│  ● Task: deploy staging  [WAIT]  │  NET:  █████████░  91%                     │
│                                  │                                            │
├──────────────────────────────────┼────────────────────────────────────────────┤
│  Context Window                  │  Knowledge Graph                           │
│  ─────────────                   │  ───────────────                            │
│  Tokens: 42,310 / 128,000       │  Nodes: 1,247                              │
│  Files:  12 loaded               │  Edges: 3,891                              │
│  Depth:  3 turns                 │  Last update: 2m ago                       │
│                                  │                                            │
├──────────────────────────────────┴────────────────────────────────────────────┤
│  Press q to quit | h for help | Tab to switch panels | j/k to scroll         │
└──────────────────────────────────────────────────────────────────────────────┘
"#;
    write_png(&dir, "dashboard_normal.png", text);

    // dashboard_error.png
    let text = r#"
┌─ Selfware Dashboard ─────────────────────────────────────────────────────────┐
│ [Status: ERROR]  ⚠ Connection lost                  Theme: Dark    12:35:01  │
├──────────────────────────────────┬────────────────────────────────────────────┤
│  Agent Activity                  │  System Health                             │
│  ─────────────                   │  ─────────────                             │
│  ✗ Task: code review    [FAIL]   │  CPU:  ██████████  99%  ⚠ HIGH            │
│  ✗ Task: refactor api   [FAIL]   │  MEM:  █████████░  92%  ⚠ HIGH            │
│  ○ Task: write tests    [SKIP]   │  DISK: ███░░░░░░░  31%                     │
│  ○ Task: deploy staging [SKIP]   │  NET:  ░░░░░░░░░░   0%  ✗ DOWN            │
│                                  │                                            │
├──────────────────────────────────┼────────────────────────────────────────────┤
│  ┌─ Error Details ────────────┐  │  Knowledge Graph                           │
│  │ ConnectionRefused:          │  │  ───────────────                            │
│  │ LM Studio at 192.168.1.99  │  │  Nodes: 1,247                              │
│  │ Port 1234 unreachable      │  │  Edges: 3,891                              │
│  │ Retry in 30s...            │  │  Status: STALE                             │
│  └────────────────────────────┘  │                                            │
├──────────────────────────────────┴────────────────────────────────────────────┤
│  ⚠ ERROR: LLM endpoint unreachable — check network | Press r to retry       │
└──────────────────────────────────────────────────────────────────────────────┘
"#;
    write_png(&dir, "dashboard_error.png", text);

    // help_panel.png
    let text = r#"
┌─ Help ───────────────────────────────────────────────────────────────────────┐
│                                                                              │
│  Keyboard Shortcuts                                                          │
│  ──────────────────                                                          │
│                                                                              │
│  Navigation                        Actions                                   │
│  ──────────                        ───────                                   │
│  q        Quit application         Enter   Execute command                   │
│  h        Show this help           Space   Toggle selection                  │
│  Tab      Switch panel             /       Search                            │
│  j / ↓    Move down                Ctrl+S  Save session                      │
│  k / ↑    Move up                  Ctrl+R  Reload config                     │
│  g        Go to top                Ctrl+C  Cancel operation                  │
│  G        Go to bottom                                                       │
│                                    Agent Commands                            │
│  Panels                           ───────────────                            │
│  ──────                            :run     Run agent task                   │
│  1        Dashboard                :stop    Stop current task                │
│  2        Agent Log                :status  Show agent status                │
│  3        Context                  :clear   Clear context                    │
│  4        Knowledge Graph          :export  Export results                   │
│                                                                              │
│  Press q or Esc to close help                                                │
└──────────────────────────────────────────────────────────────────────────────┘
"#;
    write_png(&dir, "help_panel.png", text);

    // loading_state.png
    let text = r#"
┌─ Selfware Dashboard ─────────────────────────────────────────────────────────┐
│ [Status: LOADING]                                   Theme: Dark    12:34:00  │
├──────────────────────────────────┬────────────────────────────────────────────┤
│  Agent Activity                  │  System Health                             │
│  ─────────────                   │  ─────────────                             │
│                                  │  CPU:  ████░░░░░░  42%                     │
│    ◐ Initializing agent...       │  MEM:  ███░░░░░░░  35%                     │
│                                  │  DISK: ███░░░░░░░  31%                     │
│    Loading model weights         │  NET:  ██████░░░░  58%                     │
│    [████████████░░░░░░░░]  62%   │                                            │
│                                  │                                            │
│    ETA: ~45s remaining           │                                            │
├──────────────────────────────────┼────────────────────────────────────────────┤
│  Context Window                  │  Knowledge Graph                           │
│  ─────────────                   │  ───────────────                            │
│  ◐ Loading context...            │  ◐ Building index...                       │
│                                  │                                            │
│                                  │                                            │
│                                  │                                            │
├──────────────────────────────────┴────────────────────────────────────────────┤
│  ◐ Starting up... please wait                                                │
└──────────────────────────────────────────────────────────────────────────────┘
"#;
    write_png(&dir, "loading_state.png", text);

    println!("  L1 TUI State:     4 PNGs generated");
}

// ── L2: Diagnostics (Medium) — 3 PNGs ──────────────────────────────────────

fn gen_l2(base: &Path) {
    let dir = ensure_fixture_dir(base, "l2_diagnostics").expect("create l2 dir");

    // lifetime_error.png
    let text = r#"
error[E0106]: missing lifetime specifier
  --> src/main.rs:42:33
   |
42 | fn get_name(data: &DataStore) -> &str {
   |                   ----------     ^ expected named lifetime parameter
   |
   = help: this function's return type contains a borrowed value,
           but the signature does not say which one of `data`'s
           lifetimes it is borrowed from
help: consider introducing a named lifetime parameter
   |
42 | fn get_name<'a>(data: &'a DataStore) -> &'a str {
   |            ++++        ++                ++

error: aborting due to 1 previous error

For more information about this error, try `rustc --explain E0106`.
"#;
    write_png(&dir, "lifetime_error.png", text);

    // type_mismatch.png
    let text = r#"
error[E0308]: mismatched types
  --> src/lib.rs:15:20
   |
15 |     let count: u32 = get_name();
   |                ---   ^^^^^^^^^^ expected `u32`, found `String`
   |                |
   |                expected due to this
   |
   = note: expected type `u32`
              found type `String`

help: you can convert a `String` to a `u32` using `parse`
   |
15 |     let count: u32 = get_name().parse().unwrap();
   |                                ^^^^^^^^^^^^^^^^

error: aborting due to 1 previous error

For more information about this error, try `rustc --explain E0308`.
"#;
    write_png(&dir, "type_mismatch.png", text);

    // trait_bound.png
    let text = r#"
error[E0277]: `Rc<RefCell<State>>` cannot be sent between threads safely
  --> src/agent/worker.rs:28:18
   |
28 |     tokio::spawn(async move {
   |                  ^^^^^^^^^^ `Rc<RefCell<State>>` cannot be sent
   |                              between threads safely
   |
   = help: within `impl Future<Output = ()>`, the trait `Send`
           is not implemented for `Rc<RefCell<State>>`
   = note: required for `impl Future<Output = ()>` to implement `Send`
note: required by a bound in `tokio::spawn`
  --> /tokio-1.43/src/task/spawn.rs:166:21
   |
166 |     T: Future + Send + 'static,
   |                  ^^^^ required by this bound in `spawn`

help: consider using `Arc<Mutex<State>>` instead of `Rc<RefCell<State>>`

error: aborting due to 1 previous error

For more information about this error, try `rustc --explain E0277`.
"#;
    write_png(&dir, "trait_bound.png", text);

    println!("  L2 Diagnostics:   3 PNGs generated");
}

// ── L3: Architecture (Hard) — 3 PNGs ───────────────────────────────────────

fn gen_l3(base: &Path) {
    let dir = ensure_fixture_dir(base, "l3_architecture").expect("create l3 dir");

    // evolution_diagram.png
    let text = r#"
    ┌─────────────────── Evolution Engine Architecture ───────────────────┐
    │                                                                      │
    │   ┌──────────┐        ┌───────────┐        ┌──────────────┐         │
    │   │  Daemon   │───────>│ Scheduler  │───────>│   Sandbox     │        │
    │   │  Loop     │        │            │        │   Executor    │        │
    │   └─────┬────┘        └──────┬────┘        └──────┬───────┘        │
    │         │                     │                     │                │
    │         │ trigger             │ dispatch             │ results       │
    │         v                     v                     v                │
    │   ┌──────────┐        ┌───────────┐        ┌──────────────┐         │
    │   │ Tournament│<───────│  Fitness   │<───────│   Mutation    │        │
    │   │ Selection │        │  Evaluator │        │   Engine      │        │
    │   └──────────┘        └───────────┘        └──────────────┘         │
    │         │                     ^                     ^                │
    │         │ survivors           │ scores              │ candidates    │
    │         v                     │                     │                │
    │   ┌──────────────────────────┴─────────────────────┘               │
    │   │              Generation Pool                                    │
    │   │  candidates: Vec<Candidate>                                     │
    │   │  history:    Vec<GenerationRecord>                              │
    │   └─────────────────────────────────────────────────┘               │
    │                                                                      │
    └──────────────────────────────────────────────────────────────────────┘
"#;
    write_png(&dir, "evolution_diagram.png", text);

    // agent_pipeline.png
    let text = r#"
    ┌────────────────── Agent Execution Pipeline ──────────────────────────┐
    │                                                                       │
    │   User Input                                                          │
    │       │                                                               │
    │       v                                                               │
    │   ┌─────────┐     ┌──────────┐     ┌───────────┐     ┌─────────┐   │
    │   │  Parser  │────>│  Context  │────>│  Planner   │────>│ Executor│   │
    │   │          │     │  Manager  │     │            │     │         │   │
    │   └─────────┘     └──────────┘     └───────────┘     └────┬────┘   │
    │                                                            │         │
    │                                          ┌─────────────────┤         │
    │                                          v                 v         │
    │                                    ┌──────────┐     ┌──────────┐    │
    │                                    │  Safety   │     │  Tool     │    │
    │                                    │  Checker  │     │  Registry │    │
    │                                    └────┬─────┘     └─────┬────┘    │
    │                                         │ approve         │ invoke  │
    │                                         v                 v         │
    │                                    ┌──────────┐     ┌──────────┐    │
    │                                    │  Audit    │     │  Shell    │    │
    │                                    │  Log      │     │  HTTP     │    │
    │                                    └──────────┘     │  File     │    │
    │                                                      │  Search   │    │
    │                                                      └──────────┘    │
    │                                                                       │
    └───────────────────────────────────────────────────────────────────────┘
"#;
    write_png(&dir, "agent_pipeline.png", text);

    // safety_layers.png
    let text = r#"
    ┌──────────────── Multi-Layer Safety Architecture ────────────────────┐
    │                                                                      │
    │  ┌────────────────────────────────────────────────────────────┐      │
    │  │  Layer 1: Input Validation                                  │      │
    │  │  Module: src/safety/checker.rs                              │      │
    │  │  Validates: command syntax, argument bounds, encoding       │      │
    │  │                                                             │      │
    │  │  ┌──────────────────────────────────────────────────┐      │      │
    │  │  │  Layer 2: Path Validation                         │      │      │
    │  │  │  Module: src/safety/path_validator.rs              │      │      │
    │  │  │  Validates: path traversal, symlink resolution     │      │      │
    │  │  │                                                    │      │      │
    │  │  │  ┌────────────────────────────────────────┐       │      │      │
    │  │  │  │  Layer 3: Command Filtering              │       │      │      │
    │  │  │  │  Module: src/safety/scanner.rs            │       │      │      │
    │  │  │  │  Validates: blocklist, injection patterns  │       │      │      │
    │  │  │  │                                           │       │      │      │
    │  │  │  │  ┌────────────────────────────┐          │       │      │      │
    │  │  │  │  │  Layer 4: Sandbox Execution  │          │       │      │      │
    │  │  │  │  │  Module: src/tools/shell.rs   │          │       │      │      │
    │  │  │  │  │  Validates: timeouts, limits   │          │       │      │      │
    │  │  │  │  └────────────────────────────┘          │       │      │      │
    │  │  │  └────────────────────────────────────────┘       │      │      │
    │  │  └──────────────────────────────────────────────────┘      │      │
    │  └────────────────────────────────────────────────────────────┘      │
    │                                                                      │
    └──────────────────────────────────────────────────────────────────────┘
"#;
    write_png(&dir, "safety_layers.png", text);

    println!("  L3 Architecture:  3 PNGs generated");
}

// ── L4: Profiling (VeryHard) — 3 PNGs ──────────────────────────────────────

fn gen_l4(base: &Path) {
    let dir = ensure_fixture_dir(base, "l4_profiling").expect("create l4 dir");

    // simple_flamegraph.png
    let text = r#"
  Flamegraph — selfware CPU profile (10s sample)
  ═══════════════════════════════════════════════════════════════════════

  main                                                          [100%]
  ├── agent::run_loop                                           [ 82%]
  │   ├── agent::context::build_context                         [ 35%]
  │   │   ├── memory::vector_search                             [ 22%]
  │   │   │   └── hnsw::search_layer          ← HOT             [ 18%]
  │   │   └── memory::load_embeddings                           [ 12%]
  │   ├── tools::shell::execute                                 [ 28%]
  │   │   ├── tokio::process::spawn                             [ 15%]
  │   │   └── safety::checker::validate                         [  8%]
  │   └── cognitive::reasoning::plan                            [ 14%]
  │       └── reqwest::Client::post                             [ 11%]
  ├── tui::render_frame                                         [ 12%]
  │   ├── ratatui::Terminal::draw                               [  8%]
  │   └── tui::widgets::dashboard                               [  3%]
  └── config::reload                                            [  4%]

  Total samples: 10,432    Duration: 10.0s    Threads: 4
  Hottest function: hnsw::search_layer (18% of total CPU)
"#;
    write_png(&dir, "simple_flamegraph.png", text);

    // multithread_profile.png
    let text = r#"
  Thread Profile — selfware (4 threads, 10s window)
  ═══════════════════════════════════════════════════════════════════════

  Thread 0 (main)        [CPU: 45%]  ████████████████████░░░░░░░░░░
    agent::run_loop                   ██████████████░░░░░░░░░░░░░░░
    tui::render_frame                 ██████░░░░░░░░░░░░░░░░░░░░░░░

  Thread 1 (tokio-rt-1)  [CPU: 30%]  █████████████░░░░░░░░░░░░░░░░
    tools::shell::execute             ████████░░░░░░░░░░░░░░░░░░░░░
    tools::http::request              █████░░░░░░░░░░░░░░░░░░░░░░░░

  Thread 2 (tokio-rt-2)  [CPU: 20%]  █████████░░░░░░░░░░░░░░░░░░░░
    memory::vector_search             ███████░░░░░░░░░░░░░░░░░░░░░░
    cognitive::embed                  ██░░░░░░░░░░░░░░░░░░░░░░░░░░░

  Thread 3 (blocking)    [CPU:  5%]  ██░░░░░░░░░░░░░░░░░░░░░░░░░░░
    config::watch_files               █░░░░░░░░░░░░░░░░░░░░░░░░░░░░

  Contention: Mutex wait detected on context::SharedState (Thread 0 <-> 2)
  Busiest thread: Thread 0 (main) at 45%
  Hottest function: agent::run_loop
"#;
    write_png(&dir, "multithread_profile.png", text);

    // memory_profile.png
    let text = r#"
  Memory Allocation Profile — selfware (peak: 847 MB)
  ═══════════════════════════════════════════════════════════════════════

  Top Allocators:
  ────────────────────────────────────────────────────────────────
  1. memory::vector_store::index    312 MB  (36.8%)  ████████████
  2. tokenizers::encode             198 MB  (23.4%)  ████████
  3. agent::context::buffer          87 MB  (10.3%)  ████
  4. reqwest::response::body         65 MB   (7.7%)  ███
  5. tui::terminal_buffer            42 MB   (5.0%)  ██
  6. Other (38 allocators)          143 MB  (16.8%)  ██████

  Allocation Pattern: bursty (spikes during agent::run_loop)

  Timeline:
  0s     2s     4s     6s     8s     10s
  ─┬──────┬──────┬──────┬──────┬──────┬─
   │ 200M │ 450M │ 847M │ 620M │ 310M │  ← peak at 4s (context build)
   └──────┴──────┴──────┴──────┴──────┘

  Suggestions:
  - Pool vector_store allocations (reuse buffers)
  - Stream tokenizer output instead of full materialization
  - Cap context buffer at 64MB with LRU eviction
"#;
    write_png(&dir, "memory_profile.png", text);

    println!("  L4 Profiling:     3 PNGs generated");
}

// ── L5: Layout (Extreme) — 3 PNGs ──────────────────────────────────────────

fn gen_l5(base: &Path) {
    let dir = ensure_fixture_dir(base, "l5_layout").expect("create l5 dir");

    // simple_split.png
    let text = r#"
    ┌─────────────────── TUI Layout: Horizontal Split ──────────────────┐
    │                                                                    │
    │   ┌──────────────────────┐  ┌──────────────────────────────────┐  │
    │   │                      │  │                                  │  │
    │   │   Left Panel         │  │   Right Panel                    │  │
    │   │   (Sidebar)          │  │   (Main Content)                 │  │
    │   │                      │  │                                  │  │
    │   │   Widget: List       │  │   Widget: Paragraph              │  │
    │   │                      │  │                                  │  │
    │   │   Constraint:        │  │   Constraint:                    │  │
    │   │   Percentage(30)     │  │   Percentage(70)                 │  │
    │   │                      │  │                                  │  │
    │   │                      │  │                                  │  │
    │   │                      │  │                                  │  │
    │   │                      │  │                                  │  │
    │   └──────────────────────┘  └──────────────────────────────────┘  │
    │                                                                    │
    │   Direction: Horizontal                                            │
    │   Layout::horizontal([Constraint::Percentage(30),                  │
    │                        Constraint::Percentage(70)])                │
    └────────────────────────────────────────────────────────────────────┘
"#;
    write_png(&dir, "simple_split.png", text);

    // dashboard_grid.png
    let text = r#"
    ┌─────────────────── TUI Layout: Dashboard Grid ────────────────────┐
    │                                                                    │
    │   ┌──────────────────────────────────────────────────────────┐    │
    │   │  Header Bar  (Constraint: Length(3))                      │    │
    │   └──────────────────────────────────────────────────────────┘    │
    │   ┌──────────────┐  ┌────────────────────────────────────────┐    │
    │   │  Sidebar      │  │  Main Content Area                     │    │
    │   │  (List)       │  │  (Paragraph)                           │    │
    │   │               │  │                                        │    │
    │   │  Constraint:  │  │  Constraint: Percentage(75)            │    │
    │   │  Percentage   │  │                                        │    │
    │   │  (25)         │  │                                        │    │
    │   │               │  │                                        │    │
    │   └──────────────┘  └────────────────────────────────────────┘    │
    │   ┌──────────────────────────────────────────────────────────┐    │
    │   │  Status Bar  (Constraint: Length(1))                      │    │
    │   └──────────────────────────────────────────────────────────┘    │
    │                                                                    │
    │   Outer: Vertical [Length(3), Min(10), Length(1)]                  │
    │   Inner: Horizontal [Percentage(25), Percentage(75)]              │
    └────────────────────────────────────────────────────────────────────┘
"#;
    write_png(&dir, "dashboard_grid.png", text);

    // complex_nested.png
    let text = r#"
    ┌─────────── TUI Layout: Complex Nested with Tabs & Popup ─────────┐
    │                                                                    │
    │   ┌──────────────────────────────────────────────────────────┐    │
    │   │ [Tab1: Dashboard] [Tab2: Logs] [Tab3: Graph]              │    │
    │   └──────────────────────────────────────────────────────────┘    │
    │   ┌────────────────────────┐  ┌──────────────────────────────┐   │
    │   │  Left Pane             │  │  Right Pane                   │   │
    │   │  ┌──────────────────┐  │  │  ┌────────────────────────┐  │   │
    │   │  │ Widget: List      │  │  │  │ Widget: Chart           │  │   │
    │   │  │ (scrollable)      │  │  │  │ (bar chart)             │  │   │
    │   │  └──────────────────┘  │  │  └────────────────────────┘  │   │
    │   │  ┌──────────────────┐  │  │  ┌────────────────────────┐  │   │
    │   │  │ Widget: Text      │  │  │  │ Widget: Table           │  │   │
    │   │  │ (status info)     │  │  │  │ (data view)             │  │   │
    │   │  └──────────────────┘  │  │  └────────────────────────┘  │   │
    │   └────────────────────────┘  └──────────────────────────────┘   │
    │   ┌──────────────────────┐                                       │
    │   │ ┌─ Popup ──────────┐ │  Nesting depth: 4                     │
    │   │ │ Confirm action?  │ │  Has popup: true                      │
    │   │ │ [Yes]    [No]    │ │  Widgets: tab, list, chart, text,     │
    │   │ └──────────────────┘ │           table, popup                │
    │   └──────────────────────┘                                       │
    └────────────────────────────────────────────────────────────────────┘
"#;
    write_png(&dir, "complex_nested.png", text);

    println!("  L5 Layout:        3 PNGs generated");
}

// ── Mega: Visual Evolution — 3 PNGs ─────────────────────────────────────────

fn gen_mega(base: &Path) {
    let dir = ensure_fixture_dir(base, "mega_evolution").expect("create mega dir");

    // iteration_01.png — basic first iteration
    let text = r#"
┌─ Selfware v0.1 ──────────────────────────────────────────────────────────────┐
│                                                                              │
│  Output:                                                                     │
│  ────────                                                                    │
│  > Running task: analyze code                                                │
│  > Processing file: src/main.rs                                              │
│  > Found 3 issues                                                            │
│  > Done.                                                                     │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│  Status: idle                                                                │
├──────────────────────────────────────────────────────────────────────────────┤
│  q: quit | h: help                                                           │
└──────────────────────────────────────────────────────────────────────────────┘
"#;
    write_png(&dir, "iteration_01.png", text);

    // progression_pair.png — before/after side by side
    let text = r#"
┌─ BEFORE (v0.1) ──────────────────┐  ┌─ AFTER (v0.3) ───────────────────────┐
│                                   │  │ [Status: OK]        Theme: Dark       │
│  Output:                          │  ├─────────────────┬─────────────────────┤
│  > Running task: analyze code     │  │ Agent Activity   │ System Health       │
│  > Processing...                  │  │ ─────────────    │ ─────────────       │
│  > Done.                          │  │ ● review [DONE]  │ CPU: ████░░  42%   │
│                                   │  │ ● refact [RUN]   │ MEM: ███░░░  35%   │
│                                   │  │ ● tests [WAIT]   │ NET: █████░  58%   │
│                                   │  │                  │                     │
│                                   │  ├─────────────────┼─────────────────────┤
│                                   │  │ Context          │ Knowledge           │
│                                   │  │ ─────────        │ ──────────          │
│                                   │  │ Tokens: 42K/128K │ Nodes: 1,247       │
│                                   │  │ Files: 12        │ Edges: 3,891       │
│                                   │  │                  │                     │
│  Status: idle                     │  ├─────────────────┴─────────────────────┤
│  q: quit | h: help                │  │ Tab: switch | h: help | j/k: scroll   │
└───────────────────────────────────┘  └───────────────────────────────────────┘

  Improvements: layout hierarchy, widget density, status visibility, color use
  Trajectory: IMPROVING
"#;
    write_png(&dir, "progression_pair.png", text);

    // iteration_03.png — improved third iteration
    let text = r#"
┌─ Selfware v0.3 ─────────────────────────────────────────────────────────────┐
│ [●] Agent: active    Tokens: 42K/128K    Model: qwen3.5-9b    12:34:56      │
├──────────────────────────────┬───────────────────────────────────────────────┤
│  Agent Activity               │  System Metrics                              │
│  ──────────────                │  ──────────────                              │
│  ● code review       [DONE]   │  CPU:  ████████░░  78%                       │
│  ● refactor api      [RUN]    │  MEM:  ██████░░░░  62%                       │
│  ● write tests       [WAIT]   │  DISK: ███░░░░░░░  31%                       │
│  ● deploy staging    [WAIT]   │  GPU:  █████████░  91%                       │
│                               │                                              │
│  Current: Refactoring         │  Latency: 1.2s avg                           │
│  src/agent/context.rs         │  Throughput: 42 tok/s                        │
│                               │                                              │
├──────────────────────────────┼───────────────────────────────────────────────┤
│  Context Window               │  Knowledge Graph                             │
│  ──────────────                │  ───────────────                              │
│  Tokens: 42,310 / 128,000    │  Nodes: 1,247  Edges: 3,891                  │
│  Files: 12 loaded             │  Clusters: 8   Orphans: 23                   │
│  Depth: 3 conversation turns  │  Freshness: 2m ago                           │
│                               │  ┌─ Graph Preview ──┐                        │
│  Recent files:                │  │  agent ── tools   │                        │
│  • src/agent/context.rs       │  │    |       |      │                        │
│  • src/tools/shell.rs         │  │  safety── config  │                        │
│  • src/config.rs              │  └──────────────────┘                        │
├──────────────────────────────┴───────────────────────────────────────────────┤
│  q: quit  h: help  Tab: switch  /: search  1-4: panels  Ctrl+S: save        │
└──────────────────────────────────────────────────────────────────────────────┘
"#;
    write_png(&dir, "iteration_03.png", text);

    println!("  Mega Evolution:   3 PNGs generated");
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn write_png(dir: &Path, filename: &str, text: &str) {
    // 8x16 pixels per character gives ~640x400 for 80x25 grid
    let png_data = text_to_png(text, 8, 16);
    let path = dir.join(filename);
    std::fs::write(&path, &png_data).unwrap_or_else(|e| {
        panic!("Failed to write {}: {}", path.display(), e);
    });
}
