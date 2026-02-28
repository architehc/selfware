# Selfware Evaluation Report

**Date:** 2026-02-28
**Endpoint:** `https://crazyshit.ngrok.io/v1`
**Model:** Qwen/Qwen3-Coder-Next-FP8 (1,010,000 token context)
**Config:** `selfware-eval.toml` — YOLO mode, 2000 max iterations, 900K token budget, 3h max

---

## Executive Summary

Five concurrent long-running tasks were executed against the selfware codebase to evaluate the agent framework's performance with an open-weight model (Qwen3-Coder-Next-FP8). Tasks were designed to run ~2 hours each but completed in 2–25 minutes, revealing significant gaps in task persistence, verification discipline, and tool format handling.

**Key finding:** The framework infrastructure (YOLO mode, checkpointing, retry, streaming) works correctly. The bottleneck is model-level behavior — early self-termination, tool syntax confusion, and lack of iterative verification.

---

## Task Results

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

---

## Framework Assessment

### What Worked Well

| Feature | Evidence |
|---------|----------|
| **YOLO mode** | All 5 tasks ran unattended without confirmation prompts |
| **Streaming** | Token-by-token streaming worked reliably across all tasks |
| **Retry logic** | No API failures observed; retries handled transparently |
| **Checkpoint system** | Task 2 successfully created a git checkpoint mid-run |
| **Concurrent execution** | 5 tasks ran simultaneously without framework crashes |
| **Audit logging** | All tool calls logged to eval-audit.log |
| **Config system** | External TOML config worked correctly for endpoint/model override |

### What Needs Improvement

| Issue | Severity | Details |
|-------|----------|---------|
| **Early self-termination** | Critical | All tasks finished in 2–25 min vs 2h target. The model decides it's "done" far too early. |
| **No verification loop** | Critical | Zero tasks ran `cargo check`, `cargo test`, or `cargo clippy` to verify their output. |
| **Tool format confusion** | High | Task 2 got stuck calling tools with OpenAI function-calling syntax instead of selfware XML format for 10 consecutive steps. |
| **Concurrent workspace conflicts** | High | Task 1 failed because Task 4 modified shared files. No workspace isolation between concurrent tasks. |
| **No iterative deepening** | Medium | Tasks read code and wrote code but never iterated (write → test → fix → test). |
| **Single-shot file writes** | Medium | Task 5 tried to write a huge file in one call instead of building incrementally. |
| **No progress self-assessment** | Low | Tasks never evaluated their own progress against the original prompt. |

---

## Root Cause Analysis

### 1. Early Termination (Critical)

The model treats each task as a single-pass operation: read context, write output, stop. The framework provides 2000 iterations and 3 hours, but the model self-terminates after 7–145 steps. This is a model-level behavior — the framework cannot force the model to continue working.

**Potential mitigations:**
- Add a "minimum steps" or "minimum duration" threshold before allowing termination
- Inject periodic "you are X% through your budget, continue working" system messages
- Use a meta-prompt that explicitly instructs the model to verify, iterate, and expand
- Implement a "task completion criteria" check that validates output before accepting termination

### 2. No Verification (Critical)

Despite having `cargo_test`, `cargo_check`, and `cargo_clippy` tools available, no task ever used them. The model wrote code and assumed it was correct.

**Potential mitigations:**
- Add mandatory verification steps in the PDVR cognitive cycle
- Auto-run `cargo check` after any file write and feed errors back to the model
- Include "you MUST run cargo check after writing code" in the system prompt

### 3. Tool Format Confusion (High)

The Qwen3-Coder model reverted to OpenAI function-calling XML format (`<function=name>`) in Task 2, rather than selfware's expected format. The framework logged these as errors but couldn't recover.

**Potential mitigations:**
- Add tool format correction: detect malformed calls and re-prompt with correct syntax
- Include tool format examples in every system message, not just the initial prompt
- Implement fuzzy tool call parsing that accepts multiple formats

### 4. Workspace Isolation (High)

Running 5 tasks against the same working directory caused file conflicts. Task 1's edits failed because Task 4 had modified the same files.

**Potential mitigations:**
- Use git worktrees for concurrent task isolation (already supported but not used here)
- Lock files being edited so concurrent tasks don't conflict
- Run each eval task in a separate clone of the repository

---

## Quantitative Summary

| Metric | Value |
|--------|-------|
| Total tasks | 5 |
| Tasks with usable output | 2 (Task 3, Task 4) |
| Tasks that failed | 2 (Task 2, Task 5) |
| Tasks blocked by external factors | 1 (Task 1) |
| Total steps across all tasks | 250 |
| Total duration | ~70 minutes |
| Expected duration | ~10 hours (5 × 2h) |
| Duration efficiency | 12% of target |
| Total lines of code written | ~2,600 |
| Lines that compiled | 0 verified |
| Verification tool calls | 0 |
| Framework crashes | 0 |
| API failures | 0 |

---

## Recommendations

### Short-Term (Framework Changes)

1. **Auto-verify after writes** — Run `cargo check` automatically after any Rust file is written and inject errors into the conversation
2. **Tool format recovery** — Detect malformed tool calls and re-prompt with correct syntax examples
3. **Minimum work threshold** — Don't accept task completion until minimum steps/duration/verification criteria are met
4. **Workspace isolation** — Use git worktrees or separate clones for concurrent tasks

### Medium-Term (Prompt Engineering)

5. **Verification-first prompting** — Restructure system prompt to mandate write-test-fix cycles
6. **Progress injection** — Periodically inject "progress report" system messages showing budget usage
7. **Task decomposition** — Break 2-hour tasks into explicit sub-tasks with verification gates

### Long-Term (Architecture)

8. **Supervisor agent** — A meta-agent that monitors task progress and can redirect/extend worker agents
9. **Compilation oracle** — A background process that continuously compiles and feeds errors back
10. **Task completion scoring** — ML-based assessment of whether a task's output meets its prompt criteria before allowing termination

---

## Conclusion

The selfware framework infrastructure is production-ready: streaming, retry, YOLO mode, checkpointing, and concurrent execution all work correctly. The evaluation bottleneck is at the model behavior level — Qwen3-Coder-Next-FP8 treats complex tasks as single-pass operations rather than iterative engineering work. The highest-impact improvements are auto-verification after file writes, tool format recovery for non-native models, and minimum work thresholds before task completion.
