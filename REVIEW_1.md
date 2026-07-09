# Selfware Harness Audit — REVIEW_1

**Date:** 2026-07-09  
**Scope:** `src/bench_harness`, benchmark examples, dev/test scripts, workflows, CI, and relevant test suites.  
**Goal:** Identify what is missing, what is broken, and what the fastest fixes are to get the selfware harness working end-to-end. Security is explicitly out of scope for this pass.

## How this audit was produced

Four parallel exploration agents inspected:

1. `src/bench_harness` core + examples (`bench_32_streams`, `multilang_bench`, `quant_benchmark`, `swebench_eval`, `browser_bench`, `full_flow_test`).
2. `tests/unit`, `tests/integration`, `system_tests`, and their current pass/fail state.
3. Dev/test scripts (`scripts/dev_runner.py`, `dev-test-runner.sh`, `advanced-dev-runner.sh`, `full_dev_workflow.sh`, `parallel-runner.sh`, `verify_122b_workflows.sh`) and workflow files (`workflows/product_*.yml`, `.github/workflows/*.yml`).
4. Documentation (`README.md`, `docs/`, architecture/harness docs) and `src/cli/{args,mod}.rs`.

Key findings were cross-checked against the current source tree. Exact file/line references are included where useful.

---

## Executive Summary

The **core benchmark harness is solid**: it compiles, has 121 passing unit tests, supports concurrent OpenAI-compatible requests, retries, evaluators, and writes JSON/Markdown reports. The biggest obstacles to "it just works" are **plumbing and portability**, not the engine:

- Modern CLI subcommands (`selfware bench throughput`, `multilang`, `browser`, `long-run`) are **stubbed** and redirect users to legacy flags.
- Dev scripts have real bugs: wrong directory plumbing, dead screenshot workers, non-interactive hangs, and concurrent workdir collisions.
- A handful of unit/integration tests fail because of contract mismatches (`directory_tree` returns `tree` vs expected `entries`, iteration-count semantics, stale error messages).
- SWE-bench Pro is hard-wired to one developer's machine (`/home/ivo/...` paths, `~/models/qwen36-quants`, `tensor-split 24,24`).
- The browser/computer-control harness is an HTTP fetcher, not a real browser.
- Long-running tests only run a hardcoded Rust calculator task and ignore the available template catalog.
- Documentation oversells `bench` capabilities and references a missing `docs/benchmarks.md`.

**Bottom line:** this is a "fix the wrappers" problem, not a "rebuild the engine" problem. A focused sprint on the items below would make the harness usable for most people.

---

## Component Status

| Component | Status | Notes |
|---|---|---|
| Core `HarnessRunner` (`src/bench_harness/runner.rs`) | ✅ Working | Semaphore-bounded concurrency, retries, timeout, OpenAI response parsing. |
| `HarnessConfig` / `HarnessReport` | ✅ Working | Reports latency p50/p95/p99, tok/s, pass rate, JSON + Markdown output. |
| Evaluators | ✅ Working | Keyword, JSON, Noop evaluators all tested. |
| Legacy `selfware bench --suite throughput\|multilang\|all` | ✅ Working | Feature-gated behind `bench-harness`. |
| Modern `selfware bench throughput\|multilang\|browser\|long-run` | ❌ Stubbed | `src/cli/mod.rs:3274-3298` bails with "not yet implemented". |
| `selfware bench swebench-pro` | ⚠️ Works locally only | Complete implementation, but hardcoded paths and kills any running `llama-server`. |
| `selfware long-test` | ⚠️ Partial | Library works; CLI only runs a hardcoded calculator loop. |
| Browser/computer-control harness | ⚠️ Scaffolding | HTTP-only executor; real browser actions are no-ops. |
| Dev scripts (`dev_runner.py`, `full_dev_workflow.sh`, etc.) | ❌ Multiple bugs | Directory plumbing, dead screenshot worker, non-interactive hangs, collisions. |
| Product workflows (`workflows/product_*.yml`) | ✅ Engine works | Not exercised in CI; only validated manually. |
| Unit tests (`cargo test --features bench-harness --lib`) | ✅ 9090 passed | Harness-specific tests all pass. |
| `tests/unit` | ❌ 10 failures | Mostly contract/assertion mismatches, not core logic bugs. |
| `tests/integration` | ❌ 3 failures | `directory_tree` shape mismatch + missing `xdotool`. |

---

## Detailed Findings

### 1. Modern `bench` CLI subcommands are dead ends

**Location:** `src/cli/mod.rs:3274-3298` (`dispatch_bench_subcommand`), `src/cli/args.rs` (`BenchCommand` enum).

What a user sees today:

```text
$ selfware bench throughput
`bench throughput` subcommand is not yet implemented; use `selfware bench --suite throughput` for the legacy path.
```

The legacy `Commands::Bench` handler already implements throughput and multilang. The modern subcommands simply were never wired to that code. This is the single most visible "broken" surface.

**Impact:** High — the CLI help advertises commands that do not work.

### 2. Feature-gating surprise

**Location:** `Cargo.toml:133` (default features), `Cargo.toml:162` (`bench-harness` feature).

`bench-harness` is **not** in the default feature set. A plain `cargo build --release` produces a binary where `selfware bench` prints a red "✗ Benchmark requires --features bench-harness". The README quick-start does not mention this.

**Impact:** Medium — every new user hits this wall.

### 3. Dev/test scripts are unreliable

**Locations:** `scripts/dev_runner.py`, `scripts/dev-test-runner.sh`, `scripts/full_dev_workflow.sh`, `scripts/parallel-runner.sh`.

Observed issues:

- `full_dev_workflow.sh` computes `TEST_DIR` but then calls `dev_runner.py --run`, which creates its own `parallel_dev_tests/run_<timestamp>`. The monitoring loop watches the wrong directory.
- `dev_runner.py` starts a screenshot worker as a daemon thread right before `run()` returns and the process exits, so screenshots are rarely/never captured.
- `dev-test-runner.sh` defines `screenshot_and_review` but never invokes it.
- `parallel-runner.sh` points all 16 instances at `--workdir "$PROJECT_DIR"`, causing concurrent mutations of the repo.
- `parallel-runner.sh` `wait.sh` reads per-instance `exit_code` files that are never written.
- `dev_runner.py` and `full_dev_workflow.sh` call `input()` when no endpoint is reachable, hanging in non-interactive/CI environments.
- Hardcoded model (`qwen3.5-27b`) and endpoint (`http://localhost:8000/v1`) require file edits to use other backends.

**Impact:** High for anyone trying to run the parallel dev harness.

### 4. Test failures are mostly contract mismatches

**Unit failures (`cargo test --features bench-harness --test unit`):**

- `tests/unit/test_file_extended.rs` — 5 `directory_tree` tests expect `result.entries`; implementation returns `result.tree`.
- `tests/unit/test_agent.rs::test_iteration_tracking` / `test_zero_max_iterations` — `AgentLoop::next_state()` treats `Planning` as a free iteration, so tests expecting early failure do not match.
- `tests/unit/test_error_paths.rs::test_file_read_nonexistent_file` — error message no longer contains "Failed to read file".
- `tests/unit/test_safety.rs::test_safety_allows_git_push_force_false` — `git_push` is in `always_confirm`, so even `force: false` is rejected.

**Integration failures (`cargo test --features "integration bench-harness" --test integration`):**

- `tests/integration/tool_tests.rs::test_directory_tree_execution` — same `entries` vs `tree` mismatch.
- `tests/integration/vision_computer_tests.rs::test_computer_control_chain` and `test_control_then_analyze_chain` — require `xdotool` (not installed here).

**Impact:** Medium — 13 failing tests create noise and reduce confidence in the test suite.

### 5. SWE-bench Pro is not portable

**Locations:** `src/bench_harness/swebench_pro/harness.rs:62`, `src/cli/args.rs:665-678`.

Hardcoded defaults:

- `llama-server` binary: `/home/ivo/llama.cpp/build/bin/llama-server`
- Models dir: `~/models/qwen36-quants`
- Official eval script: `/home/ivo/SWE-bench_Pro-os/swe_bench_pro_eval.py`
- Eval raw sample: `/home/ivo/SWE-bench_Pro-os/helper_code/sweap_eval_full_v2.jsonl`
- Eval scripts dir: `/home/ivo/SWE-bench_Pro-os/run_scripts`
- Default `--tensor-split 24,24` (assumes 2× RTX 4090)
- Quant catalog is Qwen3.6 HauhauCS-only.
- Harness boots its own `llama-server` and calls `pkill -u <uid> -f llama-server`, killing any other `llama-server` the user has running.
- No "use existing endpoint" mode.

**Impact:** High for portability; the feature is effectively tied to one machine.

### 6. Long-running harness ignores its own catalog

**Location:** `src/cli/mod.rs` (long-test dispatch), `src/bench_harness/long_running/runner.rs`, `system_tests/projecte2e/templates/`.

The `LongRunningRunner` library can scaffold Rust/Python/Go/template projects and validate them. The CLI, however, only runs a hardcoded Rust calculator task per round and does not use the `--templates` directory flag. `max_concurrent` is accepted but not used for fan-out across projects.

**Impact:** Medium — the long-running feature is much less capable than the library behind it.

### 7. Browser/computer-control harness is HTTP-only

**Locations:** `src/bench_harness/computer_control/executor.rs`, `src/bench_harness/computer_control/tasks.rs`.

`WebTaskExecutor` uses `reqwest`, not a real browser. Actions like `Click`, `Fill`, `Scroll`, `Press`, `Hover`, `WaitFor` are simulated or no-ops. `ElementVisible` and `VisualSimilarity` cannot be verified. `Cargo.toml` marks the `browser` feature as a "stub implementation".

**Impact:** Medium — any benchmark that needs real DOM interaction is invalid.

### 8. Missing docs and README drift

**Locations:** `README.md`, `docs/`.

- `README.md` references `docs/benchmarks.md`, which does not exist.
- README CLI reference table does not list `bench`.
- README implies `--trials N` exists on `selfware bench`, but `--trials` only exists on `bench swebench-pro`.
- `docs/superpowers/plans/2026-06-24-review-and-run-benchmarks.md` already notes the missing API-key support in the harness.

**Impact:** Low-to-medium — confusion for new users.

### 9. CI does not exercise harness examples or workflows

**Location:** `.github/workflows/ci.yml`.

CI builds/tests the library but does not:

- validate `workflows/product_*.yml`
- dry-run `product_build.yml`
- run `cargo check --examples --features bench-harness`
- run the 121 harness unit tests as a dedicated job

**Impact:** Medium — harness regressions are caught late or not at all.

### 10. API-key / auth header missing from core harness

**Location:** `src/bench_harness/config.rs`, `src/bench_harness/runner.rs`.

`HarnessConfig` has no field for an API key or auth header. This blocks cloud/OpenRouter endpoints even though the README advertises OpenRouter support.

**Impact:** Medium — limits where the harness can run.

---

## Early-Failure Detection / Monitoring Gaps

Because the user asked to "detect failure early," the following gaps are worth calling out explicitly:

| What we want to catch early | Why it fails silently today |
|---|---|
| Harness examples no longer compile | CI does not `cargo check --examples --features bench-harness`. |
| Modern `bench` subcommands break | They are stubs; no runtime path exercises them. |
| Dev scripts hang in CI | `input()` prompts block forever without a TTY. |
| Concurrent dev runs corrupt the repo | `parallel-runner.sh` reuses `$PROJECT_DIR` for all instances. |
| SWE-bench Pro kills user's `llama-server` | `pkill` is unconditional. |
| Tests fail because of contract drift | No CI job isolates `tests/unit` or `tests/integration`. |
| Long-running task ignores templates | CLI path never reads the `--templates` directory. |

A minimal monitoring layer would be: a single CI job or local script that runs `cargo test --features bench-harness --lib`, `cargo check --examples --features bench-harness`, `cargo test --features "integration bench-harness" --test integration`, and validates the product workflows. That would catch most of the regressions above on every commit.

---

## Prioritized Action List

### P0 — "It compiles and the advertised CLI works"

1. **Wire modern `bench` subcommands** — delegate `Throughput`, `Multilang`, and `LongRun` to the existing legacy/example implementations (`src/cli/mod.rs:3274-3298`).
2. **Fix `directory_tree` contract** — align tool output (`tree`) and tests (`entries`) so 6 tests pass. Pick one shape and update the other.
3. **Fix dev script directory plumbing** — make `full_dev_workflow.sh` and `dev_runner.py` agree on the run directory; write per-instance `exit_code` files for `parallel-runner.sh`.
4. **Add non-interactive flags** — add `--yes`/`-n` to `dev_runner.py` and `full_dev_workflow.sh` to skip `input()` prompts.
5. **Run a targeted test pass** — ensure `cargo test --features bench-harness --lib`, `cargo check --examples --features bench-harness`, and the fixed unit/integration tests pass.

### P1 — "It works on more than one machine"

6. **Add API-key / auth header support to `HarnessConfig`** so OpenRouter/cloud endpoints work.
7. **Replace `/home/ivo` hardcodes with env-driven defaults** (`LLAMA_SERVER_BIN`, `SWEBENCH_MODELS_DIR`, `SWE_BENCH_PRO_EVAL`, etc.) and document them.
8. **Add an "use existing endpoint" mode for SWE-bench Pro** and remove or gate the unconditional `pkill`.
9. **Make `selfware long-test` use the `--templates` directory** and cycle through templates instead of the hardcoded calculator.
10. **Decouple quant catalog from Qwen-only defaults** or document how to override it.

### P2 — "It is trustworthy and well documented"

11. **Add CI jobs** for:
    - `cargo check --examples --features bench-harness`
    - harness unit tests
    - `selfware workflow validate workflows/product_*.yml`
    - dry-run `product_build.yml`
12. **Create `docs/benchmarks.md`** and fix README drift (`--trials`, missing `bench` reference, feature-gate note).
13. **Decide browser harness fate** — either wire a real browser driver (Playwright/CDP) behind the `browser` feature or remove the stub from CLI help.
14. **Clean up dead code** (`TokenMetricsTracker`) or wire it into `LongRunningRunner`.
15. **Reduce `unwrap()`/`expect()` in `swebench_pro/runner.rs`** to prevent panics on I/O errors.

---

## What is intentionally not in this audit

- Security review of dependencies, credentials, or `hot-reload` feature.
- Performance optimization of the harness engine.
- New features beyond getting the existing harness to work end-to-end.

---

## Recommended Next Step

Start the P0 items. They are small, isolated changes that move the project from "harness looks broken" to "harness runs out of the box." After P0 is green, the P1 portability work makes it usable for contributors on different machines, and P2 turns it into a maintainable, documented subsystem.
