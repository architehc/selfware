# Codebase Review: Actionable Architectural & Quality Issues

After performing an independent static analysis of the `src/` directory, ignoring prior documentation, I have identified several critical, tangible code-quality issues, bugs, and architectural flaws that affect performance, stability, and maintainability.

## 1. Blocking I/O in Async Contexts (Performance / Deadlock Risk)
**Locations:** 
- `src/tools/file.rs` (e.g., `FileRead`, `FileWrite`, `FileEdit`)
- `src/tools/search.rs`

**Issue:** 
Multiple tools implement an `async fn execute(&self, args: Value)` but internally use synchronous `std::fs` calls (e.g., `std::fs::read_to_string`, `std::fs::write`, `std::fs::metadata`). Because Selfware relies on the `tokio` async runtime, these blocking operations freeze the executor threads. If multiple file operations run concurrently, it could lead to executor starvation and application deadlocks.

**Recommendation:**
Replace all instances of `std::fs` in async functions with their `tokio::fs` equivalents. Alternatively, wrap the synchronous operations in `tokio::task::spawn_blocking`.

## 2. Hard Panics on UTF-8 Slicing Boundaries (Stability Risk)
**Locations:**
- `src/agent/interactive.rs` (e.g., lines 889, 918, 1599, 1992, 2011)

**Issue:**
There are multiple instances of unsafe string byte-slicing for UI previews, such as:
```rust
let preview = if msg.len() > 60 { &msg[..60] } else { msg };
```
If the 60th byte falls in the middle of a multi-byte UTF-8 character (like an emoji, which are heavily used in this codebase), the application will immediately crash with a panic (`byte index 60 is not a char boundary`).

**Recommendation:**
Iterate over characters instead of bytes:
```rust
let preview: String = msg.chars().take(60).collect();
```
Or at the very least use `.is_char_boundary(60)` before slicing.

## 3. Synchronous Process Execution on UI Thread (UX Degradation)
**Locations:**
- `src/ui/tui/mod.rs` (e.g., handling `/diff` command)

**Issue:**
The TUI event loop processes user input and renders frames. Inside the command handling logic for `/diff`, the code uses:
```rust
let output = std::process::Command::new("git").args(["diff", "--stat"]).output();
```
This is a synchronous, blocking call. Running this on a large repository will freeze the entire terminal UI, making the application appear unresponsive until the `git` process finishes.

**Recommendation:**
Dispatch long-running shell commands to a background `tokio::spawn` task and communicate the result back to the TUI via an `mpsc` channel.

## 4. `RwLock`/`Mutex` Poisoning Propagation (Resilience Risk)
**Locations:**
- `src/session/cache.rs` (e.g., `cache.embeddings.read().unwrap()`)

**Issue:**
The codebase frequently uses `.unwrap()` on `MutexGuard` and `RwLockReadGuard` acquisitions. If a thread panics while holding a write lock, the lock becomes "poisoned." Any subsequent reads (even in unrelated parts of the app) that use `.unwrap()` will also panic. This defeats the purpose of the `resilience` and `self-healing` features, as a minor thread panic will cascade into a total application crash.

**Recommendation:**
Handle lock poisoning gracefully.
```rust
let lock = cache.embeddings.read().unwrap_or_else(|poisoned| poisoned.into_inner());
```

## 5. Fragile Loop Detection (Algorithmic Flaw)
**Locations:**
- `src/agent/execution.rs` (`detect_repetition` function)

**Issue:**
To prevent the LLM from getting stuck in an infinite loop (e.g., repeatedly calling `file_read`), the agent tracks the history of tool calls. However, it hashes the raw `args_str`:
```rust
let mut hasher = std::collections::hash_map::DefaultHasher::new();
args_str.hash(&mut hasher);
```
Because it hashes the raw string, small formatting variations (e.g., `{"path":"src"}` vs `{"path": "src"}`) result in different hashes. The LLM can trivially bypass the anti-loop mechanism by slightly altering whitespace in its JSON.

**Recommendation:**
Parse the JSON into a `serde_json::Value`, normalize it (e.g., stringify it using a deterministic/sorted formatter), and hash the normalized string instead of the raw LLM output.

## 6. The `Agent` God Object (Architectural Debt)
**Locations:**
- `src/agent/mod.rs`

**Issue:**
The `Agent` struct violates the Single Responsibility Principle. It currently owns over 20 fields and directly manages API communication, cognitive loop state, persistence, memory bounds, safety checks, and TUI event emission. This makes writing isolated unit tests incredibly difficult and causes high coupling.

**Recommendation:**
Refactor the agent into distinct services:
- `CognitiveEngine`: Manages the PDVR loop, learning, and episodic memory.
- `ContextWindow`: Exclusively handles token limits, compression, and message history.
- `ToolExecutor`: Handles safety checks, argument parsing, and tool execution.