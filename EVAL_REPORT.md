# Selfware Evaluation Report

**Date:** 2026-02-28
**Endpoint:** `https://crazyshit.ngrok.io/v1`
**Model:** Qwen/Qwen3-Coder-Next-FP8 (1,010,000 token context)

---

## Eval Round 5 — Post-Loop Detection (2026-02-28)

**Config:** `system_tests/projecte2e/config/crazyshit_model.toml` — YOLO mode, 500 max iterations, streaming
**Harness:** `system_tests/projecte2e/run_projecte2e.sh` — 6 coding scenarios + 1 swarm scenario
**Binary:** `target/release/selfware` built with `--all-features`

### Results: 5/6 Coding Pass, 85.7/100, Rating: Excellent

| Scenario | Type | Difficulty | Baseline | Post | Agent Exit | Timeout | Duration (s) | Score | Notes |
|---|---|---|---:|---:|---:|---:|---:|---:|---|
| `easy_calculator` | coding | easy | 101 | 0 | 0 | 0 | 27 | 100 | |
| `easy_string_ops` | coding | easy | 101 | 0 | 0 | 0 | 51 | 100 | |
| `medium_json_merge` | coding | medium | 101 | 0 | 0 | 0 | 74 | 100 | |
| `medium_bitset` | coding | medium | 101 | 0 | 0 | 0 | 37 | 100 | |
| `hard_scheduler` | coding | hard | 101 | 0 | 0 | 0 | 26 | 100 | |
| `hard_event_bus` | coding | hard | 101 | 101 | 124 | 1 | 420 | 0 | no-op write loop (model-level) |
| `swarm_session` | swarm | n/a | n/a | n/a | 0 | 0 | 6 | 100 | spawned=3 |

### What Changed Since Round 2

Three framework improvements were applied after observing stuck loops in Rounds 3–4:

1. **No-op `file_edit` detection** — Rejects edits where `old_str == new_str` with an error message telling the agent to provide a different `new_str`
2. **No-op `file_write` detection** — Rejects writes where file content already matches, preventing idempotent write loops
3. **General repetition detector** — Tracks recent tool calls by `(name, args_hash)` in a sliding window of 10. After 3 identical calls, injects a correction message with tool-specific guidance (re-read file, check test expectations, try a different strategy)

### Analysis

**5 scenarios at 100/100** (all except hard_event_bus): All passed with clean exits and fast durations (26–74s). The repetition detector specifically rescued `medium_bitset` (stale old_str loop in Round 4) and `hard_scheduler` (repeated file_read loop in Round 4).

**hard_event_bus at 0/100**: The model consistently generates the wrong Display format string (`seq: {}` instead of `seq={}`) and cannot self-correct. The no-op `file_write` detection fires correctly, and the repetition detector injects correction messages, but the model cycles: 3 identical writes → correction → "let me try a different approach" → 3 more identical writes. This is a model capability gap, not a framework issue.

### Round Progression

| Round | Pass | Score | Rating | Key Changes |
|---|---|---|---|---|
| 1 | 0/5 | N/A | N/A | Baseline — early termination, no verification, tool format confusion |
| 2 | 6/6 | 97.1 | Excellent | +completion gate, +verification prompt, +malformed tool re-prompt |
| 3 | 5/6 | 85.7 | Excellent | No framework changes; hard_event_bus no-op edit loop |
| 4 | 3/6 | 57.1 | Fair | No framework changes; 3 stuck loops exposed |
| **5** | **5/6** | **85.7** | **Excellent** | +no-op detection, +repetition detector; rescued bitset+scheduler |

### Key Observations

- **Variance is model-level**: The same scenario can score 100 or 0 across runs depending on whether the model generates the right fix on the first attempt. Framework guards reduce but cannot eliminate this variance.
- **Repetition detector works**: Rounds 4→5 show it rescuing two scenarios that were previously stuck. The detector fires, clears the window, and the model recovers with a different approach.
- **hard_event_bus is the ceiling**: The Display format bug requires the model to understand test assertions and generate the exact expected format. Qwen3-Coder consistently misreads `seq=N` as `seq: N` — a subtle but fatal error that the framework cannot fix.

---

## Eval Round 2 — Post-Framework Improvements (2026-02-28)

**Config:** `system_tests/projecte2e/config/crazyshit_model.toml` — YOLO mode, 500 max iterations, streaming
**Harness:** `system_tests/projecte2e/run_projecte2e.sh` — 6 coding scenarios + 1 swarm scenario
**Binary:** `target/release/selfware` built with `--all-features`

### Results: 6/6 Coding Pass, 97.1/100, Rating: Excellent

| Scenario | Type | Difficulty | Baseline | Post | Agent Exit | Timeout | Duration (s) | Score | Changed Files | Error Hits | Notes |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| `easy_calculator` | coding | easy | 101 | 0 | 124 | 1 | 240 | 90 | 3 | 48 | agent_timeout |
| `easy_string_ops` | coding | easy | 101 | 0 | 0 | 0 | 33 | 100 | 3 | 2 | |
| `medium_json_merge` | coding | medium | 101 | 0 | 0 | 0 | 19 | 100 | 3 | 0 | |
| `medium_bitset` | coding | medium | 101 | 0 | 0 | 0 | 38 | 100 | 3 | 1 | |
| `hard_scheduler` | coding | hard | 101 | 0 | 124 | 1 | 360 | 90 | 6 | 31 | agent_timeout |
| `hard_event_bus` | coding | hard | 101 | 0 | 0 | 0 | 83 | 100 | 5 | 2 | |
| `swarm_session` | swarm | n/a | n/a | n/a | 0 | 0 | 8 | 100 | 3 | 4 | spawned=3,status_mentions=2 |

### What the Fixes Changed

Seven framework improvements were applied between Round 1 and Round 2:

1. **Auto-verify after `file_write`** — Verification gate now triggers on both `file_edit` and `file_write`, not just `file_edit`
2. **OpenAI function format parser** — Added `<function=name>{"json"}</function>` format to the tool parser cascade (the exact format that caused Task 2's stuck loop in Round 1)
3. **Malformed tool call re-prompt** — Instead of silently logging malformed tool calls, the framework now detects them, injects a correction message with the exact format, and continues the loop
4. **Completion gate** — Requires minimum 3 steps + at least one successful verification tool (cargo_check/cargo_test/cargo_clippy) before accepting task completion
5. **Progress injection** — Every 5 steps, injects a system message with step count, budget %, verification status, and adaptive guidance
6. **Verification-first system prompt** — Both native FC and XML prompts now include a mandatory 5-step workflow (Plan → Implement → Verify → Fix → Test) and critical rules
7. **Enhanced operational plan** — Default operational plan expanded from 3 to 5 steps with explicit verification gates

### Analysis

**Scores of 100/100** (easy_string_ops, medium_json_merge, medium_bitset, hard_event_bus): Agent fixed all bugs, ran verification, and exited cleanly within the timeout. The completion gate ensured the agent ran cargo_check/cargo_test before declaring done.

**Scores of 90/100** (easy_calculator, hard_scheduler): Agent fixed all bugs and tests pass (post-validation = 0), but timed out instead of exiting cleanly. The completion gate kept the agent working — it had verified its code but kept trying to re-summarize. This is the correct tradeoff: persisting too long is much better than self-terminating too early. The 10-point deduction is for the non-clean exit only.

**Swarm (100/100):** Spawned 3 agents and completed in 8 seconds with status reports.

### Key Metrics Comparison

| Metric | Round 1 | Round 2 | Change |
|--------|---------|---------|--------|
| Tasks with passing tests | 0/5 | **6/6** | +6 |
| Verification tool calls | 0 | **Multiple per task** | Fixed |
| Tool format stuck loops | 1 (10 steps) | **0** | Fixed |
| Overall score | N/A (different harness) | **97.1/100** | — |
| Rating | N/A | **Excellent** | — |
| Framework crashes | 0 | 0 | Stable |
| Early self-termination | 5/5 tasks | **0/6 tasks** | Fixed |

---

## Eval Round 1 — Baseline (2026-02-28)

**Config:** `selfware-eval.toml` — YOLO mode, 2000 max iterations, 900K token budget, 3h max

### Executive Summary

Five concurrent long-running tasks were executed against the selfware codebase to evaluate the agent framework's performance with an open-weight model (Qwen3-Coder-Next-FP8). Tasks were designed to run ~2 hours each but completed in 2–25 minutes, revealing significant gaps in task persistence, verification discipline, and tool format handling.

**Key finding:** The framework infrastructure (YOLO mode, checkpointing, retry, streaming) works correctly. The bottleneck is model-level behavior — early self-termination, tool syntax confusion, and lack of iterative verification.

### Task Results

| # | Task | Steps | Duration | Lines Written | Outcome |
|---|------|-------|----------|---------------|---------|
| 1 | Security Audit & Hardening | 33 | 13m 44s | 0 (read-only) | Partial — read 60+ files, attempted fix, blocked by concurrent edits |
| 2 | God Object Refactor | 40 | 14m 00s | 0 (net) | Failed — created then deleted display.rs, stuck in tool syntax loop |
| 3 | Performance Benchmarking | 25 | 14m 46s | ~500 | Partial — wrote perf_analyzer.rs, bench file, shell script; never ran them |
| 4 | Plugin System | 145 | 24m 43s | 1,697 | Best — created 5-module plugin system (mod/manifest/discovery/lifecycle/registry) |
| 5 | Integration Test Suite | 7 | 2m 30s | ~400 (single file) | Failed — wrote mock_server.rs content then immediately terminated |

### Task 1: Security Audit & Hardening

**Prompt:** Comprehensive security audit of the selfware codebase — identify vulnerabilities, fix unsafe patterns, harden input validation.

**What happened:**
- Successfully read ~60 source files across all major modules
- Identified a missing `FileFimEdit` tool registration in `src/tools/mod.rs`
- Attempted to fix it but hit compilation errors because Task 4 had concurrently created `src/plugins/` modules that `src/lib.rs` referenced
- Could not resolve the cross-task interference and self-terminated

**Observations:**
- Good at systematic code reading and identifying real issues
- The concurrent task interference was a framework-level problem, not a model problem
- Never produced a security report or findings document despite reading extensively

### Task 2: God Object Refactor (streaming.rs)

**Prompt:** Refactor `src/agent/streaming.rs` (1,900+ lines) by extracting display logic into a separate module.

**What happened:**
- Read streaming.rs and identified display-related functions
- Created `src/agent/display.rs` with extracted functions
- Discovered the code was tightly coupled and deleted display.rs
- Got stuck in a 10-step loop (steps 19–28) calling `file_read` with incorrect XML syntax: `<function=file_read>{"path":"..."}` instead of the correct `<tool><name>file_read</name>` format
- Never recovered from the syntax loop; self-terminated

**Observations:**
- The model reverted to OpenAI function-calling XML format rather than selfware's tool format
- The framework did not detect or recover from the repeated malformed tool calls
- The refactoring approach was sound (identifying display logic to extract) but execution failed

### Task 3: Performance Benchmarking

**Prompt:** Profile the agent framework, identify bottlenecks, create benchmarks, and optimize hot paths.

**What happened:**
- Created `src/tools/perf_analyzer.rs` (~400 lines) — a comprehensive performance analysis tool
- Created `benches/api_bench.rs` — Criterion benchmark suite
- Created `run_benchmarks.sh` — automated benchmark runner
- Never ran `cargo test` or `cargo bench` to verify any of the code compiles
- Self-terminated after writing the files

**Observations:**
- Generated reasonable benchmark code structure
- Zero verification — no compilation check, no test run, no profiling
- Wrote code that references internal types without verifying import paths

### Task 4: Plugin System (Best Result)

**Prompt:** Design and implement a plugin system with manifest, discovery, lifecycle management, and hot-reloading.

**What happened:**
- Sustained 145 steps over 24 minutes (3–6x longer than other tasks)
- Created a well-structured `src/plugins/` directory:
  - `mod.rs` (311 lines) — public API, re-exports, PluginManager
  - `manifest.rs` (223 lines) — TOML-based plugin manifests with validation
  - `discovery.rs` (282 lines) — filesystem and registry-based plugin discovery
  - `lifecycle.rs` (571 lines) — state machine (Discovered → Loaded → Initialized → Running → Stopped → Error)
  - `registry.rs` (310 lines) — thread-safe plugin registry with dependency resolution
- Made iterative improvements and cross-referenced between modules
- Still never ran `cargo check` or `cargo test`

**Observations:**
- Most successful task by every metric (steps, duration, output quality)
- The plugin system design was architecturally sound with proper state machines
- Demonstrated that the model CAN sustain long-running work when the task is well-scoped
- Even the best task ran only 25 minutes vs the 2-hour target

### Task 5: Integration Test Suite

**Prompt:** Build a comprehensive integration test suite with mock servers, end-to-end scenarios, and CI integration.

**What happened:**
- Attempted to write a massive `tests/mock_server.rs` file in a single tool call
- The file content was extremely large (estimated 400+ lines)
- Self-terminated after only 7 steps in 2 minutes 30 seconds
- No explanation for early termination

**Observations:**
- The model tried to do everything in one shot rather than incrementally
- Shortest task by far — barely started before quitting
- Possibly hit an output token limit on the single large file write

### Framework Assessment

#### What Worked Well

| Feature | Evidence |
|---------|----------|
| **YOLO mode** | All 5 tasks ran unattended without confirmation prompts |
| **Streaming** | Token-by-token streaming worked reliably across all tasks |
| **Retry logic** | No API failures observed; retries handled transparently |
| **Checkpoint system** | Task 2 successfully created a git checkpoint mid-run |
| **Concurrent execution** | 5 tasks ran simultaneously without framework crashes |
| **Audit logging** | All tool calls logged to eval-audit.log |
| **Config system** | External TOML config worked correctly for endpoint/model override |

#### What Needed Improvement (addressed in Round 2)

| Issue | Severity | Details | Fix Applied |
|-------|----------|---------|-------------|
| **Early self-termination** | Critical | All tasks finished in 2–25 min vs 2h target | Completion gate (min steps + verification required) |
| **No verification loop** | Critical | Zero tasks ran cargo check/test/clippy | Auto-verify after file_write, verification-first prompt |
| **Tool format confusion** | High | Task 2 stuck calling tools with OpenAI syntax for 10 steps | OpenAI format parser + malformed tool re-prompt |
| **Concurrent workspace conflicts** | High | Task 1 failed because Task 4 modified shared files | (Future: git worktrees) |
| **No iterative deepening** | Medium | Tasks read/wrote code but never iterated | Progress injection + enhanced operational plan |
| **No progress self-assessment** | Low | Tasks never evaluated their own progress | Progress injection every 5 steps |

### Quantitative Summary (Round 1)

| Metric | Value |
|--------|-------|
| Total tasks | 5 |
| Tasks with usable output | 2 (Task 3, Task 4) |
| Tasks that failed | 2 (Task 2, Task 5) |
| Tasks blocked by external factors | 1 (Task 1) |
| Total steps across all tasks | 250 |
| Total duration | ~70 minutes |
| Expected duration | ~10 hours (5 x 2h) |
| Duration efficiency | 12% of target |
| Total lines of code written | ~2,600 |
| Lines that compiled | 0 verified |
| Verification tool calls | 0 |
| Framework crashes | 0 |
| API failures | 0 |

---

## Future Work

- **Workspace isolation**: Use git worktrees for concurrent eval tasks to prevent file conflicts
- **Compilation oracle**: Background `cargo check --message-format=json` watcher injecting results into context
- **Supervisor agent**: Meta-agent monitoring worker progress, built on existing `src/orchestration/multiagent.rs`
- **Task completion scoring**: ML-based assessment of whether output meets prompt criteria
