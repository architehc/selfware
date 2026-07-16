# Selfware Repo Cleanup & Operationalization — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. For any test that fails unexpectedly, use superpowers:systematic-debugging before changing code.

**Goal:** Make the selfware repo fully operational (green CI gates, stable tests), remove ~650K of stale tracked content and dead modules, and surface six implemented-but-unreachable advanced features.

**Architecture:** Five sequential phases (fmt → test/CI health → deletions → feature wiring → docs), each landing one or more small commits with the test suite green before the next phase starts. Spec: `docs/superpowers/specs/2026-07-16-repo-cleanup-design.md`.

**Tech Stack:** Rust 2021 (MSRV 1.91, local toolchain 1.96.1), clap 4, tokio, GitHub Actions YAML.

## Global Constraints

- The working tree starts **clean**: the previously in-flight swebench/memory work already landed as commits `406e61e7` and `58605d65`. Do not "recommit" it.
- **Never touch** (build/test breakers, verified): `templates/**`, `workflows/*.swl`, `scripts/playwright-bridge.js`, `scripts/package.json`, `selfware-qa-schema.yaml`, `selfware.example.toml`, `scripts/swebench_pro/**`, `scripts/qa-orchestrator.py`, `scripts/report-aggregator.js`, `scripts/quant_bench/**`, `scripts/localhost_vllm_soak.sh`, `selfware-27b-concurrency16.toml`.
- Every `git rm` is preceded by the reference grep shown in its step; if the grep shows an unexpected reference, STOP and report instead of deleting.
- Minimal diffs. No unrelated refactors, no renames, no reformatting beyond Task 1.
- After every task: `cargo check` plus the task's tests must pass before its commit.
- Commit messages: conventional style (`chore:`, `fix:`, `feat:`, `docs:`, `ci:`).
- `cargo test` full runs take minutes; use the targeted commands shown in each task before running broad suites.
- Several fixes serialize tests on the shared lock in `src/test_support.rs` (`pub(crate) mod test_support`, declared `src/lib.rs:72`). Its guards (`CwdGuard`, `BudgetGuard`, `ExecGuard`) are the established repo convention — follow it.

---

## Phase 0 — Unblock

### Task 1: Format the tree

**Files:**
- Modify: the ~11 files `cargo fmt` rewrites (includes `src/bench_harness/computer_control/browser_executor.rs`, `src/bench_harness/long_running/runner.rs`; let the tool decide)

- [ ] **Step 1: Run formatter**

Run: `cargo fmt --all`

- [ ] **Step 2: Verify the gate passes**

Run: `cargo fmt --all -- --check`
Expected: exit 0, no output.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "chore: cargo fmt (un-red the CI lint gate)"
```

---

## Phase 1 — Test & CI health

### Task 2: Serialize HOME-mutating tests with a new EnvGuard

**Files:**
- Modify: `src/test_support.rs` (append new guard after `ExecGuard`, ends line 123)
- Modify: `src/cognitive/memory_system.rs` (5+ tests mutating `HOME` between lines ~417–694)

**Interfaces:**
- Produces: `crate::test_support::EnvGuard` with `fn capture(keys: &[&'static str]) -> Self` and `fn set(&self, key: &str, value: impl AsRef<std::ffi::OsStr>)`. Tasks 3 and 4 consume it.

- [ ] **Step 1: Add the guard**

Append to `src/test_support.rs`:

```rust
/// RAII guard serializing tests that mutate process environment variables
/// (`HOME`, `SELFWARE_API_KEY`, …). Takes the shared state lock — so an env
/// mutator can never run concurrently with a cwd, budget, or exec-loop test —
/// and restores each captured variable to its prior value on drop (including
/// on panic).
pub(crate) struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl EnvGuard {
    /// Acquire the state lock and capture the current values of `keys`.
    pub(crate) fn capture(keys: &[&'static str]) -> Self {
        let lock = state_lock();
        let saved = keys.iter().map(|k| (*k, std::env::var_os(k))).collect();
        Self { _lock: lock, saved }
    }

    /// Set an environment variable while holding the guard.
    pub(crate) fn set(&self, key: &str, value: impl AsRef<std::ffi::OsStr>) {
        std::env::set_var(key, value);
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.saved.drain(..) {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }
}
```

- [ ] **Step 2: Convert every HOME mutation in `src/cognitive/memory_system.rs`**

Find them all: `grep -n 'set_var("HOME"' src/cognitive/memory_system.rs` (at least 5: `test_discover_finds_files_up_to_home` ~line 417, `test_discover_workspace_guidance_finds_agents_files_up_to_home` ~485, `test_discover_workspace_guidance_truncates_large_files` ~530, `test_discover_consolidated_memory` ~593, `test_dream_integrated_memory_system_load_consolidated` ~658; convert any additional hits too).

In each test, replace this pattern:

```rust
        let original_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &home);

        // ... test body ...

        match original_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
```

with:

```rust
        let env = crate::test_support::EnvGuard::capture(&["HOME"]);
        env.set("HOME", &home);

        // ... test body unchanged, restore block deleted ...
```

Caution: `test_dream_integrated_memory_system_load_consolidated` is a `#[tokio::test]`. If it (or any converted test) is declared `#[tokio::test(flavor = "multi_thread")]`, the held `MutexGuard` makes the future `!Send` and the build fails — remedy: keep the default current-thread flavor, or capture the guard after the last `.await`. Do not weaken the guard to fix a compile error.

- [ ] **Step 3: Verify compile + targeted tests**

Run: `cargo test --lib cognitive::memory_system 2>&1 | tail -5`
Expected: all tests pass.

- [ ] **Step 4: Stress for flakiness**

Run: `for i in 1 2 3 4 5; do cargo test --lib -- --test-threads 16 cognitive::memory_system >/dev/null 2>&1 || { echo FLAKY; break; }; done; echo done`
Expected: `done` with no `FLAKY`.

- [ ] **Step 5: Commit**

```bash
git add src/test_support.rs src/cognitive/memory_system.rs
git commit -m "fix(tests): serialize HOME-mutating memory_system tests on the shared state lock"
```

### Task 3: Fix the remaining parallel-test races

**Files:**
- Modify: `src/agent/tests.rs:1136-1178` (`test_apply_recovery_action_reload_credentials_with_env`)
- Modify: `src/agent/execution.rs:4636` (`test_file_edit_clears_task_state_for_modified_file`)
- Modify: `src/agent/task_runner.rs:2157` (`test_list_tasks_multiple`)
- Modify: `src/agent/context_management.rs:1252` (`test_estimate_messages_tokens_longer_content_costs_more`)
- Modify: `src/mcp/transport.rs:539-565` (`drop_reaps_the_child_process`)

**Interfaces:**
- Consumes: `crate::test_support::EnvGuard` (Task 2), `crate::test_support::ExecGuard` (existing).

- [ ] **Step 1: Reproduce before touching anything**

Run each 10 times and note failures (they may pass — these are races):
`for i in $(seq 1 10); do cargo test --lib -- --exact agent::tests::test_apply_recovery_action_reload_credentials_with_env 2>&1 | grep -E "^test result" ; done`
Repeat for the other four test paths. Record which fail and how. Use superpowers:systematic-debugging if a failure mode differs from what the steps below assume.

- [ ] **Step 2: Credentials env test → EnvGuard**

In `src/agent/tests.rs`, replace lines 1147-1149:

```rust
    // Set a temporary env var so the reload finds a key.
    let saved = std::env::var("SELFWARE_API_KEY").ok();
    std::env::set_var("SELFWARE_API_KEY", "test-reload-credentials-directive-key");
```

with:

```rust
    // Set a temporary env var so the reload finds a key (serialized + auto-restored).
    let env = crate::test_support::EnvGuard::capture(&["SELFWARE_API_KEY"]);
    env.set("SELFWARE_API_KEY", "test-reload-credentials-directive-key");
```

and delete the manual restore block (lines ~1172-1176):

```rust
    // Restore env.
    match saved {
        Some(v) => std::env::set_var("SELFWARE_API_KEY", v),
        None => std::env::remove_var("SELFWARE_API_KEY"),
    }
```

Check the sibling test `test_apply_recovery_action_reload_credentials_no_key_returns_false` (starts line ~1185): if it also sets/removes `SELFWARE_API_KEY`, convert it the same way.

- [ ] **Step 3: Exec-loop tests → ExecGuard**

Per the `ExecGuard` doc (`src/test_support.rs:92-99`), any test that constructs an `Agent` and drives execution must hold it. Add as the first line of the test body in:
- `test_file_edit_clears_task_state_for_modified_file` (after the `use` lines, before `MockLlmServer::builder()`)
- `test_estimate_messages_tokens_longer_content_costs_more`
- `test_estimate_messages_tokens_all_roles_counted` (same file, next test — same exposure)

```rust
        let _g = crate::test_support::ExecGuard::hold();
```

For `test_list_tasks_multiple` (non-async, `src/agent/task_runner.rs:2157`): add `let _g = crate::test_support::CwdGuard::hold();` as the first line. If Step 1 showed this test's "0 vs 3" failure is NOT cwd-related (e.g. it fails with the guard held), stop and root-cause with systematic-debugging — do not guess further.

- [ ] **Step 4: MCP child-reap test → poll instead of fixed sleep**

In `src/mcp/transport.rs`, replace (lines ~550-564):

```rust
        // start_kill sends SIGKILL; give the kernel a moment to reap it.
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;

        // `kill -0 <pid>` fails once the process is gone.
        let alive = std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(
            !alive,
            "MCP child (pid {pid}) must be dead after transport drop"
        );
```

with:

```rust
        // Poll until the kernel reaps the child (5s deadline) instead of one
        // fixed sleep, which flakes on loaded machines.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let alive = loop {
            // `kill -0 <pid>` fails once the process is gone.
            let alive = std::process::Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !alive {
                break false;
            }
            if std::time::Instant::now() >= deadline {
                break true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        };
        assert!(
            !alive,
            "MCP child (pid {pid}) must be dead after transport drop"
        );
```

- [ ] **Step 5: Verify — targeted then broad**

Run: `cargo test --lib agent:: mcp:: 2>&1 | tail -5`
Expected: pass.
Then: `for i in 1 2 3; do cargo test --lib >/dev/null 2>&1 && echo "run$i OK" || echo "run$i FAIL"; done`
Expected: three `OK` (if a different pre-existing flake appears, note it for the final gate; do not expand scope).

- [ ] **Step 6: Commit**

```bash
git add src/agent/tests.rs src/agent/execution.rs src/agent/task_runner.rs src/agent/context_management.rs src/mcp/transport.rs
git commit -m "fix(tests): close remaining parallel-test races (env lock, exec guard, reap polling)"
```

### Task 4: Skip browser E2E tests cleanly when Chrome/Playwright is absent

**Files:**
- Modify: `tests/e2e_tools_test.rs:559-601` (`test_e2e_browser_pdf_round_trip`), and the same guard in `test_e2e_browser_screenshot_round_trip` (~lines 515-557)

- [ ] **Step 1: Confirm current failure**

Run: `cargo test --test e2e_tools_test test_e2e_browser_pdf_round_trip 2>&1 | tail -8`
Expected on a Chrome-less machine: FAIL (panics instead of skipping).

- [ ] **Step 2: Add the skip guard (pdf test)**

Replace lines 561-567:

```rust
    let _env_lock = BROWSER_ENV_LOCK.lock().await;
    let mut env_restore = EnvRestore::capture(BROWSER_ENV_KEYS);
    env_restore.set_var("SELFWARE_BROWSER_NO_SANDBOX", "1");
    if let Some(chrome_path) = find_chrome_executable() {
        env_restore.set_var("SELFWARE_CHROME_EXECUTABLE_PATH", &chrome_path);
        env_restore.set_var("SELFWARE_PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH", &chrome_path);
    }
```

with (mirrors the guard already used by `test_e2e_browser_eval_round_trip` at line 606-613):

```rust
    let _env_lock = BROWSER_ENV_LOCK.lock().await;
    let Some(chrome_path) = find_chrome_executable() else {
        eprintln!("skipping browser_pdf E2E test: Chrome not available");
        return;
    };
    if !playwright_runtime_ready() {
        eprintln!("skipping browser_pdf E2E test: Playwright runtime unavailable");
        return;
    }
    let mut env_restore = EnvRestore::capture(BROWSER_ENV_KEYS);
    env_restore.set_var("SELFWARE_BROWSER_NO_SANDBOX", "1");
    env_restore.set_var("SELFWARE_CHROME_EXECUTABLE_PATH", &chrome_path);
    env_restore.set_var("SELFWARE_PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH", &chrome_path);
```

- [ ] **Step 3: Apply the identical guard to the screenshot test**

In `test_e2e_browser_screenshot_round_trip` (same file, ~line 515): find its `if let Some(chrome_path) = find_chrome_executable() { ... }` block and replace with the same `let Some(chrome_path) = ... else { eprintln!(...); return; };` + `playwright_runtime_ready()` guard structure as Step 2, with "browser_screenshot" in the skip messages.

- [ ] **Step 4: Verify**

Run: `cargo test --test e2e_tools_test 2>&1 | tail -5`
Expected: pass on machines with Chrome; on Chrome-less machines the browser tests print skip messages and the suite still passes (0 failures).

- [ ] **Step 5: Commit**

```bash
git add tests/e2e_tools_test.rs
git commit -m "fix(tests): skip browser e2e tests when Chrome/Playwright is unavailable"
```

### Task 5: Repair CI jobs and .gitignore

**Files:**
- Modify: `.github/workflows/ci.yml` (remove semver job lines 258-275; add deny job; sync audit ignores line 206)
- Modify: `.github/workflows/security.yml` (sync audit ignores line 58)
- Modify: `.gitignore:40-43`

- [ ] **Step 1: Remove the structurally broken semver job**

Delete the entire `semver:` job from `.github/workflows/ci.yml` (lines 258-275, from `  semver:` through `        run: cargo semver-checks check-release`). Rationale (commit message): the crate is intentionally not crates.io-publishable (git-only `llmfit-core` dep, see Cargo.toml:124-129), so `cargo-semver-checks` has no registry baseline and the job can never work.

- [ ] **Step 2: Add a cargo-deny job**

Append to `.github/workflows/ci.yml`:

```yaml
  # Dependency policy (advisories, licenses, bans) — configured in deny.toml
  deny:
    name: Cargo Deny
    runs-on: ubuntu-24.04
    timeout-minutes: 10

    steps:
      - name: Checkout
        uses: actions/checkout@v6

      - name: Install cargo-deny
        uses: taiki-e/install-action@cargo-deny

      - name: Run cargo-deny
        run: cargo deny check
```

- [ ] **Step 3: Sync RUSTSEC ignore lists with deny.toml**

`deny.toml` ignores 7 advisories; both audit commands ignore only 6 (missing `RUSTSEC-2026-0097`). In `.github/workflows/ci.yml` line 206 and `.github/workflows/security.yml` line 58, append ` --ignore RUSTSEC-2026-0097` to the `cargo audit` command. Verify against `grep RUSTSEC deny.toml` — if deny.toml changed since this plan was written, align the workflow lists to whatever deny.toml contains instead.

- [ ] **Step 4: Fix .gitignore patterns**

Replace `.gitignore` line 41:

```
/benchmark_results_*/
```

with:

```
/benchmark_results/
/benchmark_results_*/
/models/
```

- [ ] **Step 5: Validate YAML and commit**

Run: `python3 -c "import yaml,sys; [yaml.safe_load(open(f)) for f in ['.github/workflows/ci.yml','.github/workflows/security.yml']]; print('yaml OK')"`
Expected: `yaml OK`.
Then:

```bash
git add .github/workflows/ci.yml .github/workflows/security.yml .gitignore
git commit -m "ci: drop unworkable semver job, add cargo-deny, sync audit ignores, fix gitignore patterns"
```

---

## Phase 2 — Deletions (each `git rm` preceded by a reference grep)

### Task 6: Delete stale root and docs markdown

**Files:**
- Delete: `IMPROVE_*.md` (10 files, repo root), `CODEBASE_REVIEW.md`, `BACKLOG.md`
- Delete from `docs/`: `COMPREHENSIVE_REVIEW.md`, `DEEP_DIVE_REVIEW.md`, `UX_RECOMMENDATIONS.md`, `CLAUDE_UX_RECOMMENDATIONS_SYNTHESIS.md`, `COMPARATIVE_ANALYSIS_AND_ACTION_PLAN.md`, `QWEN_CODE_CLI_UI.md`, `agent_swarm_ui_guide.md`, `AGENT_SWARM_UI_SUMMARY.md`, `LONG_RUNNING_TEST_PLAN.md`, `MEGA_TEST_PLAN_SUMMARY.md`, and all `HERMES_SETUP_*.md` (3 files)

- [ ] **Step 1: Verify nothing references them**

Run: `grep -rln "IMPROVE_0\|IMPROVE_1\|CODEBASE_REVIEW\|BACKLOG.md\|COMPREHENSIVE_REVIEW\|DEEP_DIVE_REVIEW\|UX_RECOMMENDATIONS\|CLAUDE_UX_RECOMMENDATIONS\|COMPARATIVE_ANALYSIS\|QWEN_CODE_CLI_UI\|agent_swarm_ui_guide\|AGENT_SWARM_UI_SUMMARY\|LONG_RUNNING_TEST_PLAN\|MEGA_TEST_PLAN_SUMMARY\|HERMES_SETUP" src/ tests/ system_tests/ .github/ scripts/ Makefile Cargo.toml 2>/dev/null`
Expected: no output (other markdown docs may reference each other — that's fine; only code/CI/Makefile references block deletion). If a reference appears, drop that file from the deletion list and note it.

- [ ] **Step 2: Delete**

```bash
git rm IMPROVE_*.md CODEBASE_REVIEW.md BACKLOG.md
git rm docs/COMPREHENSIVE_REVIEW.md docs/DEEP_DIVE_REVIEW.md docs/UX_RECOMMENDATIONS.md docs/CLAUDE_UX_RECOMMENDATIONS_SYNTHESIS.md docs/COMPARATIVE_ANALYSIS_AND_ACTION_PLAN.md docs/QWEN_CODE_CLI_UI.md docs/agent_swarm_ui_guide.md docs/AGENT_SWARM_UI_SUMMARY.md docs/LONG_RUNNING_TEST_PLAN.md docs/MEGA_TEST_PLAN_SUMMARY.md docs/HERMES_SETUP_*.md
```

- [ ] **Step 3: Verify nothing broke + commit**

Run: `cargo check 2>&1 | tail -2`
Expected: `Finished`.
Then:

```bash
git commit -m "chore: delete stale audit/review markdown frozen since 2026-04"
```

### Task 7: Delete stale experiments, tracked benchmark output, demos

**Files:**
- Delete: `experiments/interactive-ux-overhaul/`, `experiments/selfware-longrun-mega/`, `benchmark_results/` (tracked files only), `demos/`

- [ ] **Step 1: Verify no references**

Run: `grep -rln "interactive-ux-overhaul\|selfware-longrun-mega\|solana-counter" src/ tests/ system_tests/ .github/ scripts/ Makefile Cargo.toml 2>/dev/null; git ls-files benchmark_results/`
Expected: no source references; the `git ls-files` shows exactly which benchmark_results files are tracked (about 5, all under `benchmark_results/2026-06-24/`). If `Cargo.toml` mentions an experiments path in `[[bin]]`, STOP — only `context_select` and `tool_verify` may be referenced there, never the two being deleted.

- [ ] **Step 2: Delete**

```bash
git rm -r experiments/interactive-ux-overhaul/ experiments/selfware-longrun-mega/ demos/
git rm -r benchmark_results/
```

- [ ] **Step 3: Verify + commit**

Run: `cargo check --all-targets 2>&1 | tail -2` (the `[[bin]]` targets under `experiments/` that remain must still compile)
Expected: `Finished`.
Then:

```bash
git commit -m "chore: remove stale experiment dirs, tracked benchmark output, unreferenced demos"
```

### Task 8: Delete broken scripts and dead test binaries

**Files:**
- Delete: `scripts/verify_122b_workflows.sh`, `scripts/fuzzy_apply.py`, `scripts/dev_runner.py`, `scripts/dev-test-runner.sh`, `scripts/advanced-dev-runner.sh`, `scripts/lightweight_dev_test.sh`, `scripts/parallel-runner.sh`, `scripts/visual_feedback_loop.py`
- Delete: `system_tests/agent_regression.rs`
- Delete: `fuzz/fuzz_targets/fuzz_target_1.rs` (+ its entry in `fuzz/Cargo.toml` if present)

- [ ] **Step 1: Verify no references**

Run: `grep -rln "verify_122b_workflows\|fuzzy_apply\|dev_runner\|dev-test-runner\|advanced-dev-runner\|lightweight_dev_test\|parallel-runner\|visual_feedback_loop\|agent_regression\|fuzz_target_1" src/ tests/ system_tests/ .github/ scripts/ Makefile Cargo.toml fuzz/Cargo.toml docs/ 2>/dev/null | grep -v "^\.selfware"`
Expected: possibly `system_tests/agent_regression.rs` matching itself, `fuzz/Cargo.toml` listing `fuzz_target_1`, and stale `docs/` references (docs being updated in Phase 4 — note any doc hits for Task 17). Any reference in `src/`, CI, or Makefile outside the files being deleted blocks this task — report it.
Note: `.selfware/turns/*.json` session-log matches are local runtime logs, not references; ignore them.

- [ ] **Step 2: Delete**

```bash
git rm scripts/verify_122b_workflows.sh scripts/fuzzy_apply.py scripts/dev_runner.py scripts/dev-test-runner.sh scripts/advanced-dev-runner.sh scripts/lightweight_dev_test.sh scripts/parallel-runner.sh scripts/visual_feedback_loop.py
git rm system_tests/agent_regression.rs
git rm fuzz/fuzz_targets/fuzz_target_1.rs
```

If `fuzz/Cargo.toml` declares a `fuzz_target_1` `[[bin]]`, remove that `[[bin]]` block too (the fuzz crate is standalone — edit its manifest, then `cd fuzz && cargo metadata --no-deps >/dev/null` to validate).

- [ ] **Step 3: Verify + commit**

Run: `cargo check --all-targets 2>&1 | tail -2`
Expected: `Finished`.
Then:

```bash
git add -A
git commit -m "chore: delete broken personal dev scripts and dead test/fuzz binaries"
```

### Task 9: Remove dead modules and trim the Makefile

**Files:**
- Delete: `src/concurrent_queue/` (whole dir)
- Delete: `src/kv_store/` (whole dir)
- Modify: `src/lib.rs` (remove line 60 `pub mod concurrent_queue;` and lines 149-150 `#[cfg(feature = "tokens")]` + `pub mod kv_store;`)
- Modify: `Makefile` (rewrite, content below)

- [ ] **Step 1: Verify zero callers**

Run: `grep -rn "concurrent_queue\|kv_store" src/ tests/ system_tests/ examples/ benches/ --include="*.rs" | grep -v "^src/concurrent_queue/\|^src/kv_store/"`
Expected: only `src/lib.rs` (the two mod declarations). Any other hit blocks deletion — report it.

- [ ] **Step 2: Delete modules and their declarations**

```bash
git rm -r src/concurrent_queue/ src/kv_store/
```

In `src/lib.rs` delete line 60 (`pub mod concurrent_queue;`) and lines 149-150 (the `#[cfg(feature = "tokens")]` attribute directly above `pub mod kv_store;` — delete both lines so `pub mod llm_doctor;` follows `pub mod interview;` directly). Leave the `tokens` feature itself in Cargo.toml — other code gates on it.

- [ ] **Step 3: Rewrite the Makefile**

Replace the entire `Makefile` with:

```makefile
# Selfware development Makefile (Rust-only; QA reports via scripts/)

.PHONY: help qa test coverage bench format lint security report report-md clean

help:
	@echo "Selfware - Available Commands"
	@echo ""
	@echo "  make qa          Run full Rust QA (check, fmt, clippy, test)"
	@echo "  make test        Run the test suite"
	@echo "  make coverage    Generate coverage reports (cargo-tarpaulin)"
	@echo "  make bench       Run benchmarks"
	@echo "  make format      Format all code"
	@echo "  make lint        Run clippy"
	@echo "  make security    Run cargo audit"
	@echo "  make report      Generate unified QA report (scripts/qa-orchestrator.py)"
	@echo "  make report-md   Generate markdown QA report (scripts/report-aggregator.js)"
	@echo "  make clean       Clean generated files"

qa:
	cargo check --all-features
	cargo fmt --all -- --check
	cargo clippy --all-features -- -D warnings
	cargo test --all-features

test:
	cargo test --all-features

coverage:
	cargo tarpaulin --out Html --out Xml

bench:
	cargo bench

format:
	cargo fmt --all

lint:
	cargo clippy --all-features -- -D warnings

security:
	cargo audit

report:
	python scripts/qa-orchestrator.py \
		--action aggregate \
		--config selfware-qa-schema.yaml \
		--reports-dir reports/ \
		--output reports/unified-report.json

report-md:
	node scripts/report-aggregator.js \
		--report reports/unified-report.json \
		--format markdown \
		--output reports/qa-report.md

clean:
	rm -rf reports/
	cargo clean
```

(Tabs, not spaces, for recipe lines.)

- [ ] **Step 4: Verify + commit**

Run: `cargo check --all-targets --features extras 2>&1 | tail -2 && cargo test --lib 2>&1 | tail -2`
Expected: `Finished` and all tests pass.
Then:

```bash
git add -A
git commit -m "chore: remove caller-less concurrent_queue/kv_store modules, trim Makefile to working targets"
```

---

## Phase 3 — Surface advanced functions

### Task 10: Enable `consolidation` in default features

**Files:**
- Modify: `Cargo.toml:150`

- [ ] **Step 1: Edit the default feature list**

Change `Cargo.toml` line 150 from:

```toml
default = ["tui", "resilience", "execution-modes", "log-analysis", "tokens", "self-improvement"]
```

to:

```toml
default = ["tui", "resilience", "execution-modes", "log-analysis", "tokens", "self-improvement", "consolidation"]
```

This activates the already-written `self.consolidate_session_memory()` call in `src/agent/checkpointing.rs:372-373` (currently `#[cfg(feature = "consolidation")]`-gated, i.e. dead in normal builds).

- [ ] **Step 2: Verify the gated code compiles and its tests run**

Run: `cargo check 2>&1 | tail -2 && cargo test --lib consolidation 2>&1 | tail -3`
Expected: `Finished`, then consolidation tests pass (they now run in the default build).

- [ ] **Step 3: Verify extras still works**

Run: `cargo check --all-targets --features extras 2>&1 | tail -2`
Expected: `Finished`.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml
git commit -m "feat: ship memory consolidation (sleep cycle) in the default build"
```

### Task 11: Surface user skills in the classic REPL and headless `run`

**Files:**
- Modify: `src/skills/mod.rs` (add `wrap_task_with_skill` after `list()`, ~line 153)
- Modify: `src/agent/interactive.rs` (discover registry at REPL start ~line 424; add `/skills` and `/<name>` handling after the `/dream` block ~line 1868)
- Modify: `src/cli/args.rs:342-348` (add `--skill` to `Run`)
- Modify: `src/cli/mod.rs` (apply the skill wrap in the `Commands::Run` dispatch, ~line 1104)

**Interfaces:**
- Produces: `SkillRegistry::wrap_task_with_skill(&self, task: &str, skill_name: &str) -> Option<String>` — consumed by `src/cli/mod.rs`.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `src/skills/mod.rs`:

```rust
    #[test]
    fn test_wrap_task_with_skill() {
        let mut registry = SkillRegistry::new();
        registry.skills.insert(
            "commit".to_string(),
            Skill {
                name: "commit".to_string(),
                description: "Create a git commit".to_string(),
                tools: vec![],
                content: "Write a concise commit message.".to_string(),
                source: None,
            },
        );
        let wrapped = registry.wrap_task_with_skill("fix the bug", "commit").unwrap();
        assert!(wrapped.contains("[Skill: commit]"));
        assert!(wrapped.contains("Write a concise commit message."));
        assert!(wrapped.contains("[Task]\nfix the bug"));
        assert!(registry.wrap_task_with_skill("fix the bug", "missing").is_none());
    }
```

Run: `cargo test --lib skills::tests::test_wrap_task_with_skill 2>&1 | tail -3`
Expected: FAIL (`wrap_task_with_skill` is not a member).

- [ ] **Step 2: Implement**

In `src/skills/mod.rs`, inside `impl SkillRegistry` (after `list()`):

```rust
    /// Wrap a task string with the named skill's instructions, for headless
    /// `run --skill`. Returns `None` when the skill is unknown.
    pub fn wrap_task_with_skill(&self, task: &str, skill_name: &str) -> Option<String> {
        self.get(skill_name).map(|skill| {
            format!(
                "[Skill: {}]\n{}\n\n[Task]\n{}",
                skill.name, skill.content, task
            )
        })
    }
```

Run: `cargo test --lib skills::tests 2>&1 | tail -3`
Expected: all skills tests pass.

- [ ] **Step 3: Wire the classic REPL**

In `src/agent/interactive.rs`, after `self.show_context_stats();` (~line 425), add:

```rust
        // Discover user skills so /skills can list them and /<name> injects them.
        let skill_registry = crate::skills::SkillRegistry::discover();
        if !skill_registry.is_empty() {
            println!("  {} skill(s) available — /skills to list", skill_registry.len());
        }
```

Then, immediately after the `/dream` handler block (~line 1868, before the input falls through to being treated as a chat message), add:

```rust
            // /skills - list discovered user skills
            if input == "/skills" {
                if skill_registry.is_empty() {
                    println!("No skills found in ~/.selfware/skills or ./.selfware/skills");
                } else {
                    println!("Skills:");
                    for skill in skill_registry.list() {
                        println!("  /{:<12} - {}", skill.name, skill.description);
                    }
                }
                continue;
            }

            // /<skill-name> - inject a discovered user skill as instructions
            if let Some(name) = input.strip_prefix('/') {
                if let Some(skill) = skill_registry.get(name) {
                    self.messages.push(Message::system(format!(
                        "The user invoked the /{} skill. Follow these instructions:\n\n{}",
                        skill.name, skill.content
                    )));
                    println!("  Skill '{}' loaded — instructions added to context.", skill.name);
                    continue;
                }
            }
```

Notes: use the same `Message` type already imported in this file (it has `Message::user(...)` calls — if no `Message::system` constructor exists, construct the system-role variant the same way existing code builds system messages). If a second REPL loop exists later in the file (there is another `/help` near line 2994), declare `let skill_registry = crate::skills::SkillRegistry::discover();` there too and repeat the two blocks.

- [ ] **Step 4: Wire headless `run --skill`**

In `src/cli/args.rs`, extend the `Run` variant (lines 342-348):

```rust
    Run {
        /// What shall we tend to?
        task: String,
        /// Shortcut for --mode=yolo (skip all confirmations)
        #[arg(short = 'y', long)]
        yolo: bool,
        /// Inject a user skill's instructions (from ~/.selfware/skills or ./.selfware/skills)
        #[arg(long)]
        skill: Option<String>,
    },
```

In `src/cli/mod.rs`, find the `Commands::Run { task, yolo }` arm (grep `Commands::Run`; ~line 1104). Add `skill` to the pattern and, before `agent.run_task(...)` is called, wrap the task:

```rust
                let task = match skill.as_deref() {
                    Some(name) => crate::skills::SkillRegistry::discover()
                        .wrap_task_with_skill(&task, name)
                        .ok_or_else(|| anyhow::anyhow!(
                            "unknown skill '{name}' (looked in ~/.selfware/skills and ./.selfware/skills)"
                        ))?,
                    None => task,
                };
```

Adjust variable binding style to the surrounding code (`task` is moved into `run_task`; make the wrapped value the one passed).

- [ ] **Step 5: Verify**

Run: `cargo test --lib skills:: 2>&1 | tail -3 && cargo check --all-targets 2>&1 | tail -2`
Expected: pass + `Finished`.
Smoke: `mkdir -p /tmp/swskill/.selfware/skills && printf -- '---\nname: greet\ndescription: test\n---\nAlways greet first.\n' > /tmp/swskill/.selfware/skills/greet.md && cd /tmp/swskill && cargo run --quiet --manifest-path /home/rig/selfware/Cargo.toml -- run "say hi" --skill missing 2>&1 | head -3`
Expected: `unknown skill 'missing' ...` error (proves discovery + lookup path runs).

- [ ] **Step 6: Commit**

```bash
git add src/skills/mod.rs src/agent/interactive.rs src/cli/args.rs src/cli/mod.rs
git commit -m "feat: surface user skills in classic REPL (/skills, /<name>) and headless run --skill"
```

### Task 12: Wire log_analysis into `selfware doctor`

**Files:**
- Modify: `src/doctor.rs` (new check fn + registration; `check_msrv` at line 457 shows the `DoctorCheck` construction pattern)
- Test: in `src/doctor.rs` `#[cfg(test)]` tests

**Interfaces:**
- Consumes: `crate::observability::log_analysis::{LogAnalyzer, LogFormat}` (`LogAnalyzer::new(LogFormat)`, `process_line(&str) -> Option<LogEntry>`, `summary() -> LogAnalyzerSummary` with `.anomalies.anomalies_detected`; `LogEntry` has a `level: LogLevel` field with `LogLevel::Error`).

- [ ] **Step 1: Write the failing test**

Add to `src/doctor.rs` tests:

```rust
#[cfg(feature = "log-analysis")]
#[test]
fn analyze_log_file_counts_errors_and_anomalies() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("selfware.log");
    std::fs::write(
        &log,
        "INFO boot\nERROR failed to connect\nERROR failed to connect\nWARN retry\n",
    )
    .unwrap();
    let (errors, _anomalies) = analyze_log_file(&log, 500).unwrap();
    assert_eq!(errors, 2);
}
```

Run: `cargo test --lib doctor 2>&1 | tail -3`
Expected: FAIL (`analyze_log_file` not found).

- [ ] **Step 2: Implement the helper**

In `src/doctor.rs` (module scope, near the other `check_*` fns):

```rust
/// Count error-level entries and anomalies in a log file (last `max_lines`
/// lines). Returns `None` when the file can't be read — doctor treats that as
/// "no logs yet", not a failure.
#[cfg(feature = "log-analysis")]
fn analyze_log_file(path: &std::path::Path, max_lines: usize) -> Option<(usize, u64)> {
    use crate::observability::log_analysis::{LogAnalyzer, LogFormat, LogLevel};

    let content = std::fs::read_to_string(path).ok()?;
    let analyzer = LogAnalyzer::new(LogFormat::Plain);
    let mut errors = 0usize;
    for line in content.lines().rev().take(max_lines).collect::<Vec<_>>().into_iter().rev() {
        if let Some(entry) = analyzer.process_line(line) {
            if entry.level == LogLevel::Error {
                errors += 1;
            }
        }
    }
    let summary = analyzer.summary();
    Some((errors, summary.anomalies.anomalies_detected))
}
```

If `entry.level` is not publicly comparable that way, count via the `LogEntry` field/accessor the module actually exposes (check `pub struct LogEntry` at `src/observability/log_analysis.rs:70`).

- [ ] **Step 3: Register a doctor check**

Find the selfware log directory: `grep -rn "tracing_appender::rolling\|log_dir\|logs" src/observability/telemetry.rs | head` — use the directory the file appender actually writes to. Add a check fn modeled on `check_msrv` (`src/doctor.rs:457`) that: locates the newest `*.log` file in that directory; if none exists, returns a passing/skipped `DoctorCheck` ("no logs yet"); otherwise calls `analyze_log_file(&path, 500)` and returns a `DoctorCheck` whose message is `"{errors} error lines, {anomalies} anomalies in <file>"` — pass/warn severity matching how other checks express informational vs warning results. Register it in the same list/sequence where the other checks are collected in `run()`.

- [ ] **Step 4: Verify**

Run: `cargo test --lib doctor 2>&1 | tail -3 && cargo run --quiet -- doctor 2>&1 | grep -i log | head -3`
Expected: tests pass; doctor output includes the log-health line.

- [ ] **Step 5: Commit**

```bash
git add src/doctor.rs
git commit -m "feat: doctor check analyzing recent selfware logs (surfaces log_analysis)"
```

### Task 13: Wire the interview into `selfware init --scaffold`

**Files:**
- Modify: `src/cli/args.rs:315-319` (add `--scaffold` to `Init`)
- Modify: `src/cli/init_wizard.rs` (new `scaffold` param + `run_scaffold_interview`)
- Modify: `src/cli/mod.rs` (the `Commands::Init` dispatch — grep `run_init_wizard` for the call site)
- Test: `src/templates.rs` `#[cfg(test)]` tests

**Interfaces:**
- Consumes: `crate::interview::run_interview(task: &str, cwd: &Path) -> Result<InterviewContext>` (sync, `src/interview.rs:747`), `crate::templates::scaffold_from_context(ctx: &InterviewContext, project_dir: &Path) -> Result<Vec<String>>` (`src/templates.rs:718`).

- [ ] **Step 1: Write the failing test**

Add to `src/templates.rs` tests:

```rust
#[test]
fn scaffold_from_context_writes_rust_project() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = crate::interview::InterviewContext {
        language: Some("rust".into()),
        framework: None,
        project_type: None,
        testing_preference: Some(crate::interview::TestingPreference::Tdd),
        output_dir: None,
        scope: None,
        extra_notes: vec![],
        task: "test scaffold".into(),
    };
    let files = scaffold_from_context(&ctx, dir.path()).unwrap();
    assert!(!files.is_empty());
    assert!(files.iter().any(|f| f.ends_with("Cargo.toml")));
}
```

Run: `cargo test --lib templates 2>&1 | tail -3`
Expected: PASS if `scaffold_from_context` already works (it is implemented, just uncalled) — in that case treat this as a characterization test locking the behavior, and continue. FAIL means the function is broken — use systematic-debugging before wiring it to `init`.

- [ ] **Step 2: Add the CLI flag**

In `src/cli/args.rs`:

```rust
    /// Interactive setup wizard for first-time configuration
    Init {
        /// Use a specific template (rust, python, node, minimal)
        #[arg(long)]
        template: Option<String>,
        /// Ask what to build, then scaffold it into the current directory
        #[arg(long)]
        scaffold: bool,
    },
```

- [ ] **Step 3: Wire the wizard**

In `src/cli/init_wizard.rs`, change the signature and add the interview step:

```rust
pub(crate) fn run_init_wizard(template: Option<String>, scaffold: bool) -> Result<()> {
    use std::io::{self, BufRead, IsTerminal, Write};
    use std::path::PathBuf;

    // If a template is provided, skip the interactive wizard
    if let Some(ref tmpl) = template {
        return write_template_config(tmpl);
    }

    if scaffold {
        run_scaffold_interview()?;
    }

    // ... rest unchanged ...
```

Append to the same file:

```rust
/// Ask structured questions about the project to build, then scaffold it into
/// the current directory via the interview-driven template engine.
fn run_scaffold_interview() -> Result<()> {
    use std::io::IsTerminal;

    if !std::io::stdin().is_terminal() {
        wizard_print!(
            "  {} --scaffold needs an interactive terminal; skipping.",
            Glyphs::frost()
        );
        return Ok(());
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let ctx = crate::interview::run_interview("Scaffold a new project", &cwd)?;
    match crate::templates::scaffold_from_context(&ctx, &cwd) {
        Ok(files) => {
            wizard_print!("  {} Scaffolded {} files:", Glyphs::bloom(), files.len());
            for f in &files {
                wizard_print!("    {}", f);
            }
        }
        Err(e) => wizard_print!("  {} Could not scaffold: {}", Glyphs::frost(), e),
    }
    Ok(())
}
```

(If `IsTerminal` lands unused in `run_init_wizard`'s `use` after the edit, drop it there — the helper has its own.)

- [ ] **Step 4: Update the call site**

In `src/cli/mod.rs`, grep `run_init_wizard` and update the dispatch to destructure the new flag and pass it: `Commands::Init { template, scaffold } => run_init_wizard(template, scaffold)?` (match surrounding style).

- [ ] **Step 5: Verify**

Run: `cargo test --lib templates init_wizard 2>&1 | tail -3 && cargo check --all-targets 2>&1 | tail -2 && cargo run --quiet -- init --scaffold </dev/null 2>&1 | head -3`
Expected: tests pass, `Finished`, and the non-interactive run prints the "needs an interactive terminal; skipping" notice (then proceeds with the normal wizard — abort it with ctrl-C or let it read EOF).

- [ ] **Step 6: Commit**

```bash
git add src/cli/args.rs src/cli/init_wizard.rs src/cli/mod.rs src/templates.rs
git commit -m "feat: selfware init --scaffold runs the interview and scaffolds from it"
```

### Task 14: Add `selfware workflow codegen`

**Files:**
- Modify: `src/cli/args.rs` (new `WorkflowCommands::Codegen` variant; enum starts line 709)
- Modify: `src/cli/mod.rs` (new dispatch arm alongside `WorkflowCommands::Validate` at line 1952)
- Test: `tests/unit/cli.rs`

**Interfaces:**
- Consumes: `crate::swl::parser::parse_document(source: &str) -> Result<SwlDocument, ParseError>` (`src/swl/parser/mod.rs:20`), `crate::swl::codegen::generate_rust_stub(doc: &SwlDocument) -> String` (`src/swl/codegen/rust_gen.rs:3`, re-exported at `src/swl/mod.rs`).

- [ ] **Step 1: Write the failing test**

Add to `tests/unit/cli.rs` (assert_cmd + predicates are dev-dependencies):

```rust
#[test]
fn workflow_codegen_prints_rust_stub() {
    let mut cmd = assert_cmd::Command::cargo_bin("selfware").unwrap();
    cmd.args(["workflow", "codegen", "workflows/bug_investigation.swl"])
        .assert()
        .success()
        .stdout(predicates::str::is_empty().not());
}
```

Run: `cargo test --test unit workflow_codegen 2>&1 | tail -3`
Expected: FAIL (`codegen` isn't a valid subcommand).

- [ ] **Step 2: Add the subcommand**

In `src/cli/args.rs`, add to `enum WorkflowCommands` (after the `Run { .. }` variant):

```rust
    /// Generate a Rust stub from an SWL workflow file
    Codegen {
        /// Path to the .swl file
        file: std::path::PathBuf,
    },
```

- [ ] **Step 3: Add the dispatch arm**

In `src/cli/mod.rs`, next to `WorkflowCommands::Validate { file } => { ... }` (line 1952), add:

```rust
                WorkflowCommands::Codegen { file } => {
                    let source = std::fs::read_to_string(&file)
                        .with_context(|| format!("failed to read {}", file.display()))?;
                    let doc = crate::swl::parser::parse_document(&source)
                        .map_err(|e| anyhow::anyhow!("failed to parse {}: {e}", file.display()))?;
                    print!("{}", crate::swl::codegen::generate_rust_stub(&doc));
                }
```

Match the surrounding arms' error style (`with_context` is already imported in this file; adjust if the neighboring arm uses a different idiom).

- [ ] **Step 4: Verify**

Run: `cargo test --test unit workflow_codegen 2>&1 | tail -3 && cargo run --quiet -- workflow codegen workflows/code_review.swl | head -8`
Expected: test passes; the command prints a Rust stub naming the workflow's agents.

- [ ] **Step 5: Commit**

```bash
git add src/cli/args.rs src/cli/mod.rs tests/unit/cli.rs
git commit -m "feat: selfware workflow codegen prints a Rust stub for an SWL file"
```

### Task 15: De-hardcode the SWE-bench Pro official-eval paths

**Files:**
- Modify: `src/cli/args.rs:659-675` (three `Option<String>` args, no defaults)
- Modify: `src/cli/mod.rs:3787-3789` (use the new resolver)
- Modify: `src/cli/tests.rs:572-574` (Option assertions) + new resolver test

**Interfaces:**
- Produces: `pub(crate) fn resolve_official_eval_paths(args: &SwebenchProArgs) -> anyhow::Result<(PathBuf, PathBuf, PathBuf)>` in `src/cli/mod.rs` — consumed by the swebench-pro dispatch at line ~3787.

- [ ] **Step 1: Write the failing tests**

In `src/cli/tests.rs`, update the existing assertions (lines 572-574):

```rust
                assert_eq!(args.official_eval_script.as_deref(), Some("/tmp/eval.py"));
                assert_eq!(args.official_eval_raw_sample_path.as_deref(), Some("/tmp/sample.jsonl"));
                assert_eq!(args.official_eval_scripts_dir.as_deref(), Some("/tmp/run_scripts"));
```

Add:

```rust
#[test]
fn official_eval_requires_explicit_paths() {
    // --official-eval without the three paths must fail with a helpful error.
    let args = SwebenchProArgs {
        official_eval: true,
        ..Default::default()
    };
    let err = crate::cli::resolve_official_eval_paths(&args).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("--official-eval-script"), "got: {msg}");
    assert!(msg.contains("--official-eval-scripts-dir"), "got: {msg}");

    // Without --official-eval the paths are unused — empty triple is fine.
    let args = SwebenchProArgs::default();
    assert!(crate::cli::resolve_official_eval_paths(&args).is_ok());
}
```

Notes: `SwebenchProArgs` (`src/cli/args.rs:563`) has no `Default` yet — Step 2 adds `#[derive(Default)]`-compatible changes (`Option<String>` fields); if the struct can't derive `Default` because of other fields, construct it with the explicit values the surrounding tests use instead. The test file already imports the args types — follow its existing imports.

Run: `cargo test --lib cli 2>&1 | tail -5`
Expected: FAIL (args fields are `String`, not `Option`; resolver missing).

- [ ] **Step 2: Make the three args optional (no `/home/ivo` defaults)**

In `src/cli/args.rs`, replace lines 659-675:

```rust
    /// Path to the SWE-bench Pro evaluator script (required with --official-eval).
    #[arg(long)]
    pub official_eval_script: Option<String>,

    /// Path to the SWE-bench Pro raw sample CSV/JSONL (required with --official-eval).
    #[arg(long)]
    pub official_eval_raw_sample_path: Option<String>,

    /// Directory containing per-instance run_script.sh/parser.py files (required with --official-eval).
    #[arg(long)]
    pub official_eval_scripts_dir: Option<String>,
```

- [ ] **Step 3: Add the resolver and use it**

In `src/cli/mod.rs` (module scope):

```rust
/// Resolve the three SWE-bench Pro official-eval paths. They are required only
/// when `--official-eval` is set; there are no machine-specific defaults.
pub(crate) fn resolve_official_eval_paths(
    args: &crate::cli::args::SwebenchProArgs,
) -> anyhow::Result<(std::path::PathBuf, std::path::PathBuf, std::path::PathBuf)> {
    let missing: Vec<&str> = [
        (args.official_eval_script.is_none(), "--official-eval-script"),
        (
            args.official_eval_raw_sample_path.is_none(),
            "--official-eval-raw-sample-path",
        ),
        (
            args.official_eval_scripts_dir.is_none(),
            "--official-eval-scripts-dir",
        ),
    ]
    .into_iter()
    .filter_map(|(is_missing, flag)| is_missing.then_some(flag))
    .collect();
    if args.official_eval && !missing.is_empty() {
        anyhow::bail!(
            "--official-eval requires {}; pass the paths of your SWE-bench Pro checkout",
            missing.join(", ")
        );
    }
    Ok((
        std::path::PathBuf::from(args.official_eval_script.clone().unwrap_or_default()),
        std::path::PathBuf::from(
            args.official_eval_raw_sample_path
                .clone()
                .unwrap_or_default(),
        ),
        std::path::PathBuf::from(args.official_eval_scripts_dir.clone().unwrap_or_default()),
    ))
}
```

Replace lines 3787-3789:

```rust
        official_eval_script: PathBuf::from(args.official_eval_script),
        official_eval_raw_sample_path: PathBuf::from(args.official_eval_raw_sample_path),
        official_eval_scripts_dir: PathBuf::from(args.official_eval_scripts_dir),
```

with:

```rust
        {
            let (script, raw_sample, scripts_dir) = resolve_official_eval_paths(&args)?;
            (script, raw_sample, scripts_dir)
        }
```

then assign the three struct fields from that triple (adapt to the exact struct-literal syntax at the site).

- [ ] **Step 4: Verify**

Run: `cargo test --lib cli 2>&1 | tail -3 && cargo check --all-targets --features bench-harness 2>&1 | tail -2`
Expected: tests pass; `Finished`.

- [ ] **Step 5: Commit**

```bash
git add src/cli/args.rs src/cli/mod.rs src/cli/tests.rs
git commit -m "fix: drop machine-specific /home/ivo defaults from swebench official-eval paths"
```

---

## Phase 4 — Docs & dedup decisions

### Task 16: Write the consolidation decision document

**Files:**
- Create: `docs/CONSOLIDATION_PLAN.md`

- [ ] **Step 1: Write the document**

Create `docs/CONSOLIDATION_PLAN.md` with:

```markdown
# Consolidation Plan — Duplicated Subsystems

Status: decision records only; merging is follow-up work, out of scope for the
2026-07-16 cleanup. Each entry picks a winner and sketches the merge path.

## 1. Workflow engines: `swl/` vs `orchestration/workflows.rs`

- **Winner:** `orchestration/workflows.rs` (production-ready, powers the SWL
  runtime path in the CLI). `swl/` is self-described EXPERIMENTAL with a
  module-wide `#![allow(dead_code)]`.
- **Merge path:** keep `swl` parsing/validation as a front-end that lowers to
  the orchestration executor; delete `swl/`'s own runtime/guardrails duplicate
  logic; then remove the `#[allow(dead_code)]`.

## 2. Container/process: `devops/` vs `tools/container`, `tools/process`

- **Winner:** `tools/container` + `tools/process` (what the agent actually calls).
- **Merge path:** `devops/process_manager.rs` is wired (process tool) — keep;
  `devops/container.rs` `ContainerManager` has zero callers — port any unique
  behaviors into `tools/container/`, then delete `devops/container.rs`.

## 3. Self-improvement: `evolution/` vs `cognitive/self_improvement`

- **Winner:** both survive, but need one front door. `evolution/` is the
  daemon/fitness/tournament engine; `cognitive/self_improvement` is in-session
  learning. Document the boundary: in-session learning feeds candidates to the
  evolution daemon; no third path may appear.

## 4. Token accounting (4+ places)

- Sites: `src/tokens.rs`, `src/token_count.rs`, `src/memory/mod.rs`,
  `src/agent/{compression,context,context_management}.rs`.
- **Winner:** `tokens.rs` for budgets/costs, `token_count.rs` as the single
  tokenizer backend (tiktoken/HF/heuristic). `memory/` and `agent/*` must call
  into those two instead of re-estimating.

## 5. SWE-bench locations (3)

- Sites: `src/bench_harness/swebench_pro/` (Rust runner),
  `scripts/swebench_pro/` (Python reference/spec), `system_tests/swe_bench_pro/` (fixtures/e2e).
- **Winner:** `src/bench_harness/swebench_pro/`. Keep `scripts/swebench_pro/`
  as the documented reference implementation; `system_tests/swe_bench_pro/`
  holds fixtures only. No new code in the system_tests copy.

## 6. `lib.rs` flat re-export shims

- `src/lib.rs:109-139` re-exports `pub(crate)` domain modules under legacy flat
  paths (`crate::checkpoint`, `crate::telemetry`, …), so everything has two
  import spellings.
- **Winner:** the module paths (`crate::session::checkpoint`, …).
- **Merge path:** migrate `agent/` (the main legacy consumer) to module paths,
  then delete the shims. Mechanical, one module at a time.

## 7. God files (follow-up refactors, no winner needed)

- `src/agent/tool_dispatch.rs` (5.9k), `src/agent/execution.rs` (4.8k),
  `src/testing/verification.rs` (4.0k), `src/cli/mod.rs` (3.9k),
  `src/agent/interactive.rs` (3.9k), `src/analysis/vector_store.rs` (3.4k),
  `src/devops/process_manager.rs` (3.3k), `src/tokens.rs` (3.1k).
- Split by responsibility when next touched for a feature; no bulk rewrite.
```

- [ ] **Step 2: Commit**

```bash
git add docs/CONSOLIDATION_PLAN.md
git commit -m "docs: consolidation decision records for duplicated subsystems"
```

### Task 17: Fix stale doc references

**Files:**
- Modify: `CHANGELOG.md:10`
- Modify: `src/orchestration/ORCHESTRATION_STATUS.md` (coordinator status)
- Modify: `docs/SWE_BENCH_EVALUATION_STRATEGY.md` (dead script refs)
- Modify: any `docs/` files Task 8's grep flagged as referencing deleted scripts

- [ ] **Step 1: CHANGELOG version**

In `CHANGELOG.md` change line 10 `## [0.4.0-beta.1] - 2026-04-12` to `## [0.3.0-beta.1] - 2026-04-12` (Cargo.toml `version = "0.3.0-beta.1"` is authoritative).

- [ ] **Step 2: Coordinator status**

In `src/orchestration/ORCHESTRATION_STATUS.md`, replace line 12:

```markdown
- **Coordinator Mode** (`coordinator.rs`): ⚠️ STUBBED/SIMULATED
```

with:

```markdown
- **Coordinator Mode** (`coordinator.rs`): ✅ Functional — execution routes through `MultiAgentChat` (see `multiagent/interactive.rs`)
```

Then `grep -n "STUB\|stubbed\|SIMULATED\|simulated" src/orchestration/ORCHESTRATION_STATUS.md` and correct any other stale stub claims the same way (verify against the code before rewriting a claim — coordinator execution goes through `MultiAgentChat`).

- [ ] **Step 3: Dead script references**

Run: `grep -n "swebench_baseline\|swebench_check_regression" docs/SWE_BENCH_EVALUATION_STRATEGY.md`
Replace those references with the real entry points: `scripts/swebench_pro/run.py` (runner) and `scripts/swebench_eval.sh` / `scripts/swebench_compare.sh` (eval drivers). Keep each edit minimal — same sentence, corrected name.

Also fix any doc hits for deleted scripts recorded in Task 8 Step 1 (e.g. references to `dev-test-runner.sh`): remove the paragraph or point at `scripts/full_dev_workflow.sh` where a dev-workflow reference is genuinely needed.

- [ ] **Step 4: Commit**

```bash
git add CHANGELOG.md src/orchestration/ORCHESTRATION_STATUS.md docs/
git commit -m "docs: fix changelog version, coordinator status, dead script references"
```

### Task 18: Final verification gate

- [ ] **Step 1: Formatting + lints**

Run: `cargo fmt --all -- --check && cargo clippy --all-targets --features extras -- -D warnings 2>&1 | tail -3`
Expected: clean.

- [ ] **Step 2: Test suite, three consecutive runs**

Run: `for i in 1 2 3; do cargo test >/dev/null 2>&1 && echo "run$i OK" || { echo "run$i FAIL"; exit 1; }; done`
Expected: `run1 OK`, `run2 OK`, `run3 OK`. Any failure: systematic-debugging; do not declare done with a red suite.

- [ ] **Step 3: Extras suite**

Run: `cargo test --features extras 2>&1 | tail -3`
Expected: pass.

- [ ] **Step 4: Docs build**

Run: `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --features extras 2>&1 | tail -3`
Expected: `Finished`.

- [ ] **Step 5: cargo-deny (if installed)**

Run: `cargo deny check 2>&1 | tail -3 || echo "cargo-deny not installed locally — CI job covers it"`
Expected: either clean output matching deny.toml policy, or the not-installed note.

- [ ] **Step 6: Report**

Run `git log --oneline 39203bda..HEAD` and summarize: commits landed per phase, final tree state, any known-remaining flakes deferred during Task 3 Step 5.

---

## Self-Review Notes

- Spec coverage: Phase 0 (Task 1) ✓; Phase 1 (Tasks 2-5) ✓; Phase 2 (Tasks 6-9) ✓; Phase 3 (Tasks 10-15) ✓; Phase 4 (Tasks 16-18) ✓. All spec items mapped.
- Type consistency: `EnvGuard::capture/set` (Task 2) consumed by Tasks 2-3; `wrap_task_with_skill(&self, task: &str, skill_name: &str) -> Option<String>` (Task 11) consumed in Task 11 Step 4; `resolve_official_eval_paths(&SwebenchProArgs) -> Result<(PathBuf, PathBuf, PathBuf)>` (Task 15) consumed in Task 15 Step 3; `analyze_log_file(&Path, usize) -> Option<(usize, u64)>` (Task 12) consumed in Task 12 Steps 1/3. No cross-task name drift.
- Placeholder scan: every code step contains complete code or an exact quoted replacement; investigation steps (Task 3 Step 1, Task 12 Step 3 log dir, Task 17 Step 3) name the exact grep and the decision rule to apply.
