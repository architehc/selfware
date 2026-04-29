# Agent Execution Loop — Code Review Report

**Scope:** `src/agent/{mod,execution,loop_control,streaming,tool_dispatch,tool_validator,tool_collect,tool_execution,recovery,failure_mode,last_tool}.rs`
**Date:** 2026-04-28
**Focus:** Bugs, logic errors, TODO/FIXME, design smells, performance bottlenecks, missing error handling, SWE-bench reliability.

---

## 🔴 CRITICAL: Compilation Errors (Duplicate Inherent Methods on `Agent`)

Rust does not allow two inherent `impl Agent` methods with the same name, even if signatures differ. The codebase currently defines the following duplicates across `tool_execution.rs` and `tool_dispatch.rs`:

| Method | `tool_execution.rs` | `tool_dispatch.rs` | Status |
|--------|---------------------|---------------------|--------|
| `record_failed_tool_attempt` | line 48 | line 963 | **Identical signature → hard compile error** |
| `clear_failed_tool_attempts` | line 72 | line 989 | **Identical signature → hard compile error** |
| `build_tool_call_context` | line 77 | line 2266 | Different signature (still illegal) |
| `parse_tool_args` | line 96 | line 2363 | Different signature (still illegal) |
| `validate_tool_args` | line 123 | line 2410 | Different signature (still illegal) |
| `execute_single_tool` | line 174 | line 2459 | Different signature (still illegal) |

**Fix:**
- Delete `tool_execution.rs` entirely (or remove its `impl Agent` block). It appears to be stale code that was superseded by `tool_dispatch.rs`. All call sites in `tool_dispatch.rs` resolve to the `tool_dispatch.rs` versions (e.g., `execute_single_tool` is called at `tool_dispatch.rs:1923` with 4 trailing args, matching the `tool_dispatch.rs` signature, not the 6-arg `tool_execution.rs` version).
- If `tool_execution.rs` is meant to be kept, rename every method in it with a `_legacy` suffix and audit call sites.

---

## `src/agent/mod.rs`

### 1. Synchronous I/O Inside `Agent::new` (Async Context)
**Lines:** 510–521, 524–545, 709–711, 1108–1175  
`Agent::new` is `async`, yet it calls `dirs::data_local_dir()`, `tokio::fs::read_to_string` (correct), but then `current_project_root()` (line 709) calls `std::env::current_dir()` and `ancestor.join(marker).exists()` — all blocking syscalls on the async thread. `collect_direct_project_context` (line 1108) uses `walkdir::WalkDir` and `std::fs::read_to_string` synchronously.

**Fix:**
```rust
// Replace std::env::current_dir() + Path::exists() with tokio::fs::metadata in async paths.
// For walkdir usage, spawn_blocking or use async-recursion with tokio::fs::read_dir.
```

### 2. `extract_candidate_paths_from_text` Recompiles Regex on Every Call
**Lines:** 154–171  
```rust
let path_re = regex::Regex::new(...).expect("...");
```
This allocates and compiles a regex every invocation. If called inside `collect_direct_project_context` for every message, it is O(messages × regex_compile).

**Fix:** Use `std::sync::OnceLock` (or `lazy_static`):
```rust
static PATH_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
let path_re = PATH_RE.get_or_init(|| regex::Regex::new(...).unwrap());
```

### 3. `detect_project_type` Re-does File System Probing on Every Call
**Lines:** 180–196  
Called from `Agent::new` and potentially elsewhere. Each call does 5 `Path::exists()` checks.

**Fix:** Compute once in `Agent::new` and store in a `ProjectType` field on `Agent`.

### 4. `FileTracker::mark_stale` Silently Drops Entries After 500
**Lines:** 300–304  
```rust
if self.stale_files.len() < 500 {
    self.stale_files.insert(path.to_string());
}
```
No warning, no eviction policy. In a long SWE-bench task with >500 modified files, stale tracking silently breaks.

**Fix:**
```rust
if self.stale_files.len() >= 500 {
    warn!("Stale file tracker overflow — evicting oldest");
    // Or use an LRU instead of HashSet
}
```

### 5. `max_context_tokens` Calculation Uses Integer Division, Can Yield Zero
**Lines:** 877–881  
```rust
let safety_margin = model_context_limit / 5; // 20%
let max_context_tokens = model_context_limit
    .saturating_sub(config.max_tokens)
    .saturating_sub(safety_margin);
```
If `model_context_limit` is small (e.g., 8192) and `max_tokens` is 4096, `max_context_tokens` becomes ~2457. If `model_context_limit` is 4096 and `max_tokens` is 4096, result is 0. The warning at line 883 is emitted, but the agent continues with a useless context budget.

**Fix:** Return `Err` from `Agent::new` when `max_context_tokens < 2048` (or some floor) so the caller knows the config is incompatible.

### 6. `Agent` Struct Has 77 Fields — Single-Responsibility Violation
**Lines:** 326–480  
The struct mixes: API client, tool registry, memory, safety, loop control, compression, checkpointing, cognitive state, self-improvement, file tracking, TUI events, progress emitter, edit history, chat store, cancellation, caching, concurrency governor, plan mode, audit logging, session logging, RAG, visual state, context map, compression orchestrator, and failure-mode counters.

**Design smell:** This makes testing, reasoning about borrows, and refactoring extremely difficult.

**Fix:** Extract cohesive groups into sub-structs:
```rust
struct Agent {
    core: AgentCore,        // client, tools, config, messages
    execution: ExecutionState, // loop_control, counters, failed_attempts
    context: ContextManager,   // compressor, context_map, file_tracker
    ui: UiState,               // events, progress_emitter, cancellation
    telemetry: TelemetryState, // audit, session, checkpoint
}
```

### 7. `prompt_for_permission_cli` Uses `tokio::task::block_in_place`
**Lines:** 1727–1730  
```rust
let input = tokio::task::block_in_place(|| {
    let mut buf = String::new();
    io::stdin().read_line(&mut buf).map(|_| buf)
})?;
```
This blocks the current thread pool thread. If the Tokio runtime is single-threaded (common in tests or constrained environments), this deadlocks.

**Fix:** Use `tokio::io::AsyncBufReadExt::read_line` on `tokio::io::stdin()`, or spawn a dedicated blocking thread with `std::thread::spawn` + `tokio::sync::oneshot`.

### 8. `is_interactive` Re-checks `stdin().is_terminal()` on Every Call
**Lines:** 1438–1441  
```rust
pub fn is_interactive(&self) -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}
```
Terminal state does not change during the agent lifetime. Repeated syscalls are wasteful.

**Fix:** Cache the result in `Agent::new` and store a `bool` field.

### 9. `last_assistant_response_has_final_answer` Allocates on Every Call
**Lines:** 1846–1849  
```rust
let lower = self.last_assistant_response.to_lowercase();
lower.contains("final answer")
```
Called by `FailureMode::classify` at run end. For a 47KB response (common in `NontermProse`), this allocates 47KB just for a substring check.

**Fix:** Use `str::to_ascii_lowercase` if the marker is ASCII, or use `memmem` case-insensitive search without allocation.

### 10. `permanently_blocked_tool_calls_len` Deduplicates on Read, Not on Write
**Lines:** 1800–1806  
The vec stores duplicates; deduplication happens only when the classifier reads it. If the vec grows to 64 (the write cap), it could be 64 copies of the same tool name.

**Fix:** Use a `HashSet<String>` for storage, or deduplicate in `note_permanently_blocked` before pushing.

---

## `src/agent/execution.rs`

### 11. `count_source_files_in_workdir` Does Blocking I/O in Async Path
**Lines:** 54–124  
Uses `walkdir::WalkDir` (synchronous) and `std::env::current_dir()`. Called from the terminal progress guard (`execute_step_internal_inner`), which is async.

**Fix:** Wrap in `tokio::task::spawn_blocking` or replace with `tokio::fs` recursive walk.

### 12. Auto-Write Code Paths Have No Rollback on Partial Failure
**Lines:** 336–362, 516–564  
If `contains_unwritten_code` detects code, `execute_tool_batch` is called with a synthetic `file_write`. If the batch fails partway through (e.g., second tool in batch errors), the auto-write has already happened but the function may return `Err`, leaving the conversation in an inconsistent state.

**Fix:** Perform auto-writes in a separate, isolated batch before the main batch, or snapshot files before auto-write and rollback on failure.

### 13. Completion Gate Checked *After* Auto-Write — Model Can Pass with Unverified Code
**Lines:** 567–584  
The completion gate runs after the auto-write injection. This means a model that outputs code as text, gets it auto-written, and then produces no further tool calls will pass the gate even though it never ran `cargo check`.

**Fix:** Move the auto-write block **after** the completion gate check, or add a rule to the gate that requires at least one verification tool call when `has_written_any_file` is true.

### 14. Repeated Identical Response Detection Has Counter-Reset Bug
**Lines:** 434–465  
```rust
if content == self.last_assistant_response && !content.is_empty() {
    self.consecutive_no_action_prompts += 1;
    if self.consecutive_no_action_prompts >= 5 {
        if self.check_completion_gate().is_none() {
            // accept completion
            return Ok(true);
        }
        // Gate rejected — reset counter to 0
        self.consecutive_no_action_prompts = 0;
        ...
    }
}
```
If the model repeats the same response 5 times, the gate rejects, and the counter resets to 0. The model can then repeat 4 more times without triggering this branch again. For SWE-bench, this means a stuck model can burn 9 iterations instead of being caught at 5.

**Fix:** Do not reset `consecutive_no_action_prompts` to 0 on gate rejection. Cap it at the threshold or increment a separate "rejection counter."

### 15. `should_prompt_for_action` Test Helper Duplicates Production Logic
**Lines:** 1146–1175  
A standalone `should_prompt_for_action` function is copied into the test module. When the production logic changes (e.g., intent phrases are updated), tests can pass while production behaves differently.

**Fix:** Make the production `should_prompt_for_action` on `Agent` `pub(super)` and call it directly from tests.

### 16. `contains_unwritten_code` Miscounts Fenced Code Blocks
**Lines:** 1007–1080  
```rust
let code_block_count = stripped.matches("```").count() / 2;
```
If a line contains an unmatched `` ` `` or a language specifier line like `` ```rust ``, the division by 2 yields the wrong block count. More importantly, the subsequent line-by-line parser toggles `in_code_block` on every `` ``` ``, so a single stray backtick line (e.g., `` `foo` `` inside a paragraph) will flip the state incorrectly.

**Fix:** Only toggle `in_code_block` when a line *starts with* `` ``` `` (after trimming), not when it merely contains the substring.

### 17. `extract_code_and_path` Default Path Logic Is SWE-bench Unfriendly
**Lines:** 917–1000  
When no path is found, it defaults to `src/lib.rs` if the code lacks `fn main(`, otherwise `src/main.rs`. On SWE-bench tasks that are Python or JavaScript projects, this writes Rust code into the wrong file.

**Fix:** Use `detect_project_type()` to choose a sensible default (e.g., `src/lib.rs` for Rust, `main.py` for Python, etc.), or refuse to auto-write when the path cannot be determined.

---

## `src/agent/loop_control.rs`

### 18. Missing State Transitions
**Lines:** 94–113  
`is_valid_transition` rejects:
- `Planning -> Completed` (an exact-response task can finish without executing)
- `ErrorRecovery -> Completed` (recovery might conclude the task is done)
- `Failed -> *` (no recovery from failed state)

**Fix:** Add `(Planning, Completed)` and `(ErrorRecovery, Completed)` to the match, or document why they are intentionally forbidden.

### 19. `set_state` Panics Instead of Returning Error
**Lines:** 129–132  
```rust
pub fn set_state(&mut self, state: AgentState) {
    self.transition_to(state)
        .expect("invalid agent state transition");
}
```
In production (especially SWE-bench headless runs), a panic aborts the entire process. A bad plugin or hook could trigger an invalid transition and kill the runner.

**Fix:** Return `Result<(), InvalidStateTransition>` and let the caller decide whether to log and continue.

---

## `src/agent/streaming.rs`

### 20. `chat_streaming` Is 382 Lines — Does Too Much
**Lines:** 182–564  
Responsibilities: cache lookup, spinner management, TUI event emission, tag suppression, reasoning extraction, usage tracking, finish-reason capture, progress emission, and cache storage. This makes unit testing nearly impossible.

**Fix:** Decompose into:
- `stream_chunks()` → yields `StreamChunk`s
- `render_stream()` → handles TUI/CLI output
- `cache_streaming_response()` → handles cache get/put

### 21. `display_buf` Can Grow Unbounded on Partial Tags
**Lines:** 250, 342–426  
If a model emits an opening `<tool>` that never closes (e.g., truncated stream), `display_buf` accumulates indefinitely because `has_partial_tag_at_end` returns true and the outer loop buffers everything.

**Fix:** Cap `display_buf` at ~4KB and force-flush if the cap is hit, or time out partial tags after N chunks.

### 22. LLM Cache Key Is Never Used
**Lines:** 101–102, 136  
```rust
let _key = format!("{}:{:?}:{:?}", prompt, tools, thinking);
```
The key is formatted but discarded (`_key`). The actual lookup uses an embedding vector, which is expensive and may not be deterministic across runs.

**Fix:** If the cache is meant to be a fast hash lookup, use the key. If it's meant to be semantic, document why embeddings are preferred and remove the dead key computation.

### 23. `messages_to_prompt` Uses Naive Join — Cache Collisions Possible
**Lines:** 114–120  
```rust
messages.iter().map(|m| format!("[{}]: {}", m.role, m.content)).collect::<Vec<_>>().join("\n")
```
Two different message sequences can produce the same prompt string if roles/content line up ambiguously (e.g., `user: foo\nassistant: bar` vs a single message with content `[user]: foo\n[assistant]: bar`).

**Fix:** Use a structured separator that cannot appear in content, or hash the JSON serialization of the message array.

---

## `src/agent/tool_dispatch.rs`

### 24. `summarize_and_spill` Does Blocking File I/O in Async Context
**Lines:** 32–71  
```rust
let _ = std::fs::create_dir_all(spill_dir);
if let Err(e) = std::fs::write(&spill_file, raw) { ... }
```
Called from `push_tool_result_message` (line 2840), which is called from `execute_parallel_tools` and `execute_single_tool_in_batch` — both async.

**Fix:** Use `tokio::fs::create_dir_all` and `tokio::fs::write`, or spawn_blocking.

### 25. `truncate_chars` Computes `chars().count()` Twice
**Lines:** 382–389  
```rust
let collected: String = s.chars().take(max_chars).collect();
if s.chars().count() > max_chars { ... }
```
The second call is O(N) for long strings (e.g., 50K tool results).

**Fix:**
```rust
let mut iter = s.chars();
let collected: String = iter.by_ref().take(max_chars).collect();
if iter.next().is_some() { format!("{}...", collected) } else { collected }
```

### 26. `extract_explicit_requested_tools` Compiles Regexes on Every Call
**Lines:** 483–512  
```rust
let patterns = [
    format!(r"(?i)\b(?:use|call|invoke|run)\s+(?:the\s+)?`?{}`?(?:\s+tool)?\b", escaped),
    ...
];
if patterns.iter().filter_map(|p| regex::Regex::new(p).ok()).any(|re| re.is_match(task_context)) { ... }
```
For a registry with 50 tools, this compiles 100 regexes every time the function is called.

**Fix:** Pre-compile the pattern templates with `format!("...", "{}")` and use `regex::Regex::new` once per tool at startup, storing in a `Vec<Regex>`.

### 27. `task_requires_mutation` Has Fragile Keyword Matching
**Lines:** 532–555  
```rust
let lower = task_context.to_lowercase();
[
    "fix", "implement", "edit", ...
    "add ",   // <-- trailing space
]
.iter().any(|needle| lower.contains(needle))
```
`"add "` with a trailing space means `"addition"` does not match, but `"add "` does. This is inconsistent with `"fix"` (which matches `"prefix"`). Also, `to_lowercase()` allocates the entire task string.

**Fix:** Use a `HashSet<&str>` of whole-word matchers and `regex::Regex::new(r"(?i)\b(fix|implement|edit|...)\b").unwrap().is_match(task_context)`.

### 28. `shell_command_is_observational` Does `to_lowercase()` on Every Call
**Lines:** 557–637  
Called for every `shell_exec` tool call to decide if it counts as mutating.

**Fix:** Normalize once and match against lowercase patterns, or use `eq_ignore_ascii_case` / `starts_with` on the original string.

### 29. `tool_call_is_observational` and `tool_call_counts_as_state_change` Are Inverse but Not Mirror Images
**Lines:** 639–685  
`tool_call_counts_as_state_change` special-cases `cargo_check/cargo_test/cargo_clippy` to `false`, but `tool_call_is_observational` returns `true` for them. If a new tool like `cargo_doc` is added, it must be updated in both places or the counters will be wrong.

**Fix:** Define one canonical function (e.g., `tool_side_effect_level() -> SideEffectLevel`) and derive the boolean checks from it.

### 30. `maybe_block_redundant_reread` Has Off-by-One in Message vs. Counter
**Lines:** 994–1069  
The check allows 3 unchanged rereads (`state.unchanged_read_count < 3`), but the warning message at line 1036 says "already been read unchanged {} times", where `read_count = state.unchanged_read_count + 1`. So on the 4th read (when count is 3), the message says "4 times", which is technically correct but the threshold name "up to 3" is confusing.

More importantly, after blocking at line 1036, the code increments the counter at line 1033, but the `push_tool_result_message` at line 1045 reports the failure to the model. The model sees "blocked after 4 times" but the threshold was 3.

**Fix:** Unify the threshold constant and use it in both the check and the error message.

### 31. `execute_parallel_tools` Clears Failed Attempts on Any Success in Batch
**Lines:** 1697–1701  
```rust
if success {
    self.clear_failed_tool_attempts();
} else { ... }
```
If a parallel batch contains `[file_read A, file_write B]`, and `file_write B` succeeds while `file_read A` fails, the failed attempt for `file_read A` is cleared. On the next turn the model might retry the exact same failing `file_read A` and pay the latency again.

**Fix:** Only clear failed attempts for the specific tool that succeeded, or clear only when ALL tools in the batch succeed.

### 32. `execute_parallel_tools` and `execute_single_tool_in_batch` Are Near-Identical
**Lines:** 1400–1770 vs. 1772–2047  
Both do: self-improvement warning → inject defaults → build context → suppress retry → policy check → safety check → schema validation → parse args → validate args → block reread → hook → emit started → execute → emit completed → store output → self-improvement record → track state → push message → recovery hint → post hook → audit log.

This ~300-line duplication is a maintenance nightmare. Any fix (e.g., adding a new pre-execution check) must be applied in two places.

**Fix:** Extract a `prepare_tool_execution()` method that returns a `PreparedTool` struct, and a `finalize_tool_execution()` method that handles post-processing. Both parallel and sequential paths should call them.

### 33. `execute_single_tool_inner` Success Path Is 120+ Lines
**Lines:** 2633–2756  
After a tool succeeds, the function handles: caching, diff display, mutating counter, self-improvement, file verification, visual verification, result enhancement, and hard-failure error construction. This is far too much for one branch.

**Fix:** Extract `post_process_successful_tool()`.

### 34. `confirm_tool_execution` Auto-Approves in TUI Mode
**Lines:** 2297–2299  
```rust
if self.has_tui_renderer() {
    return Ok(true);
}
```
This silently bypasses confirmation when the TUI is active. A user running in TUI with Normal mode might not expect destructive tools to run without asking.

**Fix:** Emit a `PermissionRequested` TUI event and await the response via the event channel, rather than unconditionally returning `true`.

### 35. `build_tool_call_context` Inconsistent Native FC Logic vs. `tool_execution.rs`
**Lines:** 2266–2283  
`tool_dispatch.rs`: `use_native_fc = self.config.agent.native_function_calling && tool_call_id.is_some();`  
`tool_execution.rs`: `use_native_fc = self.config.agent.native_function_calling;`

If `tool_execution.rs` were ever called (it can't due to the duplicate method error), it would use native FC even without an ID, producing malformed messages. After removing `tool_execution.rs`, this inconsistency is moot, but the fact that it existed shows the danger of duplication.

---

## `src/agent/tool_execution.rs`

### 36. Entire File Is Either Dead Code or Causes Compile Errors
As documented in the 🔴 CRITICAL section, this file duplicates methods from `tool_dispatch.rs`. Even if the duplicates were renamed, many methods here are never called because `tool_dispatch.rs` superseded them.

**Fix:** Remove the file. If any helpers are still needed, move them to `tool_dispatch.rs` or a shared `tool_utils.rs`.

---

## `src/agent/tool_validator.rs`

### 37. `value_matches_type` Does Not Handle JSON Schema Union Types
**Lines:** 93–104  
```rust
"string" => value.is_string(),
"integer" => value.is_i64() || value.is_u64(),
...
_ => true, // Unknown types pass through
```
If a schema declares `"type": ["string", "null"]`, `prop_type` will be parsed as `"string"` (because `as_str()` on an array returns `None`), and the check will be skipped entirely. This allows passing `null` to a required string field.

**Fix:** Handle `type` being an array:
```rust
if let Some(types) = prop_schema.get("type").and_then(|t| t.as_array()) {
    if !types.iter().any(|t| value_matches_type(value, t.as_str().unwrap_or(""))) { ... }
}
```

### 38. No `enum` Validation
**Lines:** 69–87  
The validator checks required fields and scalar types, but does not validate `enum` constraints. A model could pass `"severity": "critical"` when the schema only allows `["low", "medium", "high"]`.

**Fix:** Add an `enum` check after type validation.

---

## `src/agent/tool_collect.rs`

### 39. Native Tool Call Validation Logs but Does Not Reject
**Lines:** 40–55  
```rust
if let Err(e) = crate::agent::tool_validator::validate_tool_calls(&extracted, &defs) {
    warn!("Native tool call batch validation failed: {}", e);
}
extracted.retain(|tc| match tc.validate_structure() { ... });
```
A batch validation failure is only logged. Individual malformed calls are filtered, but a batch with wrong argument types might still proceed.

**Fix:** Either reject the entire batch on validation failure, or log at `error!` level and emit a metric/event so the harness can detect systemic schema issues.

---

## `src/agent/recovery.rs`

### 40. `strip_think_blocks` Leaks Unclosed Thinking Content
**Lines:** 99–105, 107–121  
If the model emits `<think>` without `</think>`, the explicit tag handler (line 110) finds `<think>`, pushes everything before it into `result`, then tries to find `</think>`. If none is found, it sets `rest = ""` and breaks. This means everything *after* the unclosed `<think>` is **discarded**. For SWE-bench, this could drop the actual answer that follows an unclosed think tag.

**Fix:** If `</think>` is missing, preserve the content after `<think>` rather than discarding it.

### 41. `normalize_no_action_content` Is Expensive for Long Content
**Lines:** 34–40  
```rust
content.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
```
This allocates a Vec of all words and a new String. For a 10K-char response, this is wasteful when the caller only needs a hash and a few `contains` checks.

**Fix:** Lowercase in-place and use a streaming hasher, or at least avoid the `split_whitespace().collect().join()` dance — just lowercase and hash the original string.

### 42. `maybe_prompt_for_action` Aborts with Generic Error Message
**Lines:** 556–571  
```rust
return Err(error_msg);
```
The error is a plain string. In SWE-bench, the harness cannot programmatically distinguish "model refused to use tools" from "API key missing" or "network timeout".

**Fix:** Return a typed error like `AgentError::NonTerminatingProse { consecutive: usize, total: usize }` so the harness can classify it without string matching.

### 43. `detect_repetition` Can Trigger Immediately on Multi-Call Batches
**Lines:** 695–720  
`recent_tool_calls` is a flat deque of individual calls. If a batch contains 3 identical `file_read` calls, pushing them increments the count to 3. The next batch with the same call pushes it to 4, and `repeat_count >= MAX_REPEATS` (3) triggers immediately. This means a model that issues 3 `file_read`s in one turn gets flagged on the very next turn.

**Fix:** Count per *batch*, not per *individual call*, or increase `MAX_REPEATS` proportionally to batch size.

### 44. `build_error_recovery_hint` Does `error.to_lowercase()` + Many Contains Checks
**Lines:** 363–491  
Every failed tool call triggers this function, which allocates a lowercase copy of the error and does ~20 substring searches.

**Fix:** Use `regex::RegexSet` pre-compiled at startup, or at least do case-insensitive matching without allocation.

---

## `src/agent/failure_mode.rs`

### 45. `NontermProse` Threshold Mismatch with Abort Logic
**Lines:** 264  
```rust
if no_action_consecutive >= super::recovery::FORCE_FALLBACK_AFTER && mutating == 0 {
    return FailureMode { kind: FailureKind::NontermProse, ... };
}
```
`FORCE_FALLBACK_AFTER = 3`, but the actual abort in `recovery.rs` happens at `MAX_NO_ACTION_PROMPTS = 20`. A run with 4–19 consecutive prose-only turns will be classified as `MaxIterations` instead of `NontermProse`, making SWE-bench post-mortems misleading.

**Fix:** Change the threshold to `MAX_NO_ACTION_PROMPTS` (or define a shared constant).

### 46. `truncate()` Uses Byte Slices, Not Char Boundaries
**Lines:** 324–330  
```rust
format!("{}…", &s[..max])
```
If `s` contains multi-byte UTF-8 (e.g., emoji, CJK) and `max` falls in the middle of a character, this panics.

**Fix:**
```rust
s.char_indices().nth(max).map(|(i, _)| &s[..i]).unwrap_or(s).to_string()
```

---

## `src/agent/last_tool.rs`

### 47. `LastToolOutput` Stores Unbounded Strings in Memory
**Lines:** 12–25  
`full_output: String` has no size limit. A `shell_exec` of `find . -type f -exec cat {} \;` could produce hundreds of megabytes, all retained for the lifetime of the `Agent`.

**Fix:** Store only the first N bytes (e.g., 64KB) and a flag `truncated: bool`.

---

## Cross-Cutting Concerns

### A. Magic Numbers Everywhere
Thresholds are scattered with no single source of truth:
- `3` — unchanged reread block (`tool_dispatch.rs:1016`), force fallback (`recovery.rs:15`), prefill breaker (`mod.rs:1911`)
- `5` — repetition detection (`recovery.rs:696`), suppression nudge (`execution.rs:705`)
- `8/15/20` — terminal progress guard thresholds (`execution.rs:732–733`)
- `10` — suppression escalation (`execution.rs:684`)
- `16` — progress guard block threshold (`tool_dispatch.rs:812`)
- `20` — max no-action prompts (`recovery.rs:12`)
- `500` — total no-action ceiling (`recovery.rs:20`), file tracker cap (`mod.rs:301`)

**Fix:** Create a `Thresholds` struct (or `AgentConfig` defaults) and reference it everywhere.

### B. Synchronous I/O in Async Functions
At least 15 call sites use `std::fs::*`, `walkdir`, or `std::env::current_dir` inside async `Agent` methods. In a high-concurrency SWE-bench runner, this blocks the Tokio runtime and reduces throughput.

**Fix:** Audit all agent modules for `std::fs` and `std::env` usage inside `async fn`; replace with `tokio::fs` or `spawn_blocking`.

### C. No TODO/FIXME Comments Found
None of the 10 files contain `TODO` or `FIXME` comments. This is suspicious — either the code is perfect (unlikely) or unfinished work is not marked. The `#[cfg(target_os = "windows", ignore = "...")]` attributes in tests (`execution.rs:3281`, `3909`) indicate known issues that should be marked with `// FIXME(windows): ...`.

---

## Priority Matrix for SWE-bench Reliability

| Priority | Issue | File(s) | Impact |
|----------|-------|---------|--------|
| **P0** | Duplicate methods → code does not compile | `tool_execution.rs`, `tool_dispatch.rs` | Agent cannot start |
| **P0** | `strip_think_blocks` discards unclosed think content | `recovery.rs:110` | Answers lost |
| **P1** | Completion gate runs after auto-write | `execution.rs:567` | False completions |
| **P1** | Repeated response counter resets on gate rejection | `execution.rs:456` | Wasted iterations |
| **P1** | `execute_parallel_tools` clears all failures on any success | `tool_dispatch.rs:1697` | Retry loops |
| **P1** | `max_context_tokens` can be zero with no hard error | `mod.rs:877` | Context collapse |
| **P2** | `NontermProse` threshold mismatch | `failure_mode.rs:264` | Misclassified runs |
| **P2** | `truncate()` panic on UTF-8 boundaries | `failure_mode.rs:324` | Crash on non-ASCII |
| **P2** | Synchronous I/O in async paths | `mod.rs`, `tool_dispatch.rs`, `execution.rs` | Poor throughput |
| **P3** | `Agent` struct bloat (77 fields) | `mod.rs:326` | Maintainability |
| **P3** | `chat_streaming` 382-line monster | `streaming.rs:182` | Untestable |
| **P3** | `execute_single_tool_inner` 309 lines | `tool_dispatch.rs:2493` | Untestable |
