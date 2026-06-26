# SWE-bench Pro Agent Loop P0 Fixes

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the agent loop for non-Rust SWE-bench tasks by treating any successful test/build command as completion verification, auto-running the official `fail_to_pass` test command after edits, and applying the official `test_patch` to the host repo before generation.

**Architecture:** Reuse the existing `tool_call_is_verification` abstraction in `src/agent/tool_dispatch.rs` to make the completion gate language-agnostic. Add an optional `post_edit_test_command` to the agent config; when set, `VerificationGate::verify_change` will run it after each `file_edit`/`file_write` and surface failures. The harness will apply `instance["test_patch"]` after resetting the repo and pass the official test command via `SELFWARE_POST_EDIT_TEST_COMMAND`.

**Tech Stack:** Rust (tokio, serde, regex), Python 3 (harness scripts).

---

## Task 1: Generic verification completion gate

**Files:**
- Modify: `src/agent/verification.rs:559-652`
- Modify: `src/agent/verification.rs:538-543` (message only)
- Test: `src/agent/verification.rs` tests

- [ ] **Step 1: Add helper `has_successful_verification_tool_call`**

Add a method on `Agent` in `src/agent/verification.rs`:

```rust
fn has_successful_verification_tool_call(&self) -> bool {
    self.current_checkpoint
        .as_ref()
        .map(|cp| {
            cp.tool_calls.iter().any(|tc| {
                tc.success && super::tool_dispatch::tool_call_is_verification(&tc.tool_name, &tc.arguments)
            })
        })
        .unwrap_or(false)
}
```

- [ ] **Step 2: Replace the two cargo-only verification blocks**

Replace lines 559-580 (`has_written_any_file` block) with:

```rust
if self.has_written_any_file {
    let has_verification = self.has_successful_verification_tool_call();
    if !has_verification {
        return Some(
            "You have written code, but you have not verified it. \
             Run a verification command (e.g. cargo_check, cargo_test, pytest, npm test, go test, mvn test, dotnet test) \
             successfully before completing."
                .to_string(),
        );
    }
}
```

Replace lines 621-652 (`require_verification_before_completion` block) with the same generic check, but only when `self.config.agent.require_verification_before_completion` is true and the task is not read-only-only. Remove the `should_skip_cargo_verification()` bypass so non-Rust SWE tasks are no longer allowed to complete without verification.

- [ ] **Step 3: Update the unwritten-code hint**

Change the `contains_unwritten_code` message at lines 538-543 to say:

```rust
"Your response contains code that was NOT written to any file. \
 Use file_write to save it to a file, then verify with a relevant test/build command. \
 Do NOT output code as text — use tools."
```

- [ ] **Step 4: Add/update unit tests**

Add tests in `src/agent/verification.rs` that:
1. Accept completion when a successful `shell_exec` with `pytest` is in the checkpoint.
2. Reject completion when only a failing `shell_exec` with `pytest` is present.
3. Reject completion when files were written but no verification tool succeeded.

Run: `cargo test --lib --all-features agent::verification::tests -- --test-threads=4`
Expected: all tests pass.

---

## Task 2: Auto-run official test command after edits

**Files:**
- Modify: `src/config/agent.rs:99-106`
- Modify: `src/config/loader.rs:363-370`
- Modify: `src/testing/verification.rs:236-275`
- Modify: `src/testing/verification.rs:454-470`
- Modify: `src/agent/mod.rs:888-891`
- Test: `src/testing/verification.rs` tests

- [ ] **Step 1: Add `post_edit_test_command` to `AgentConfig`**

In `src/config/agent.rs` add:

```rust
/// Optional command to run automatically after every file_edit/file_write.
/// Used by SWE-bench Pro to run the official fail_to_pass tests.
#[serde(default)]
pub post_edit_test_command: Option<String>,
```

Add `post_edit_test_command: None` to `AgentConfig::default()`.

- [ ] **Step 2: Read `SELFWARE_POST_EDIT_TEST_COMMAND` from the environment**

In `src/config/loader.rs` after the `SELFWARE_TIMEOUT` block add:

```rust
if let Ok(cmd) = std::env::var("SELFWARE_POST_EDIT_TEST_COMMAND") {
    if !cmd.trim().is_empty() {
        config.agent.post_edit_test_command = Some(cmd);
        sources.set(
            "agent.post_edit_test_command",
            ConfigSource::EnvVar("SELFWARE_POST_EDIT_TEST_COMMAND".into()),
        );
    }
}
```

- [ ] **Step 3: Add `post_edit_test_command` to `VerificationConfig`**

In `src/testing/verification.rs` add to `VerificationConfig`:

```rust
/// Optional SWE-bench official test command. When set, it is run after every
/// file edit/write in addition to the normal per-language checks.
#[serde(default)]
pub post_edit_test_command: Option<String>,
```

Update `Default`, `fast()`, and `thorough()` to include `post_edit_test_command: None`.

Add a setter on `VerificationGate`:

```rust
pub fn set_post_edit_test_command(&mut self, command: Option<String>) {
    self.config.post_edit_test_command = command;
}
```

- [ ] **Step 4: Run the command in `verify_change`**

After the cheap syntax checks (around line 454-470), if `self.config.post_edit_test_command` is `Some(cmd)`:
1. Parse the command string with `shlex` (or split on whitespace) into program and args.
2. Run it with `tokio::process::Command` from `self.project_root`.
3. Timeout after `self.config.check_timeout_secs.max(60)` seconds.
4. Produce a `CheckResult` with `check_type = CheckType::Test`, `passed = output.status.success()`, and output = combined stdout/stderr (truncated to ~4KB).
5. Append it to `checks`.

If the command fails, set `overall_passed = false` and add a suggested next step like:

```rust
suggested_next_steps.push(format!(
    "The post-edit test command failed: {}. Fix the failing test before completing.",
    cmd
));
```

- [ ] **Step 5: Wire the command from `Agent::new` into `VerificationGate`**

In `src/agent/mod.rs:888-891`:

```rust
let mut verification_gate = VerificationGate::new(&project_root, VerificationConfig::fast());
if let Some(ref cmd) = config.agent.post_edit_test_command {
    verification_gate.set_post_edit_test_command(Some(cmd.clone()));
}
```

- [ ] **Step 6: Add/update unit tests**

Add a test in `src/testing/verification.rs` that sets `post_edit_test_command` to a shell command that fails, calls `verify_change`, and asserts the report is `overall_passed == false` with the failure output present.

Run: `cargo test --lib --all-features testing::verification::tests -- --test-threads=4`
Expected: all tests pass.

---

## Task 3: Harness applies test_patch and passes command

**Files:**
- Modify: `system_tests/swe_bench_pro/run_selfware.py:1128-1138`
- Modify: `system_tests/swe_bench_pro/run_selfware.py:2300-2322`
- Modify: `system_tests/swe_bench_pro/run_selfware.py:1966-1972`
- Test: `python -m pytest system_tests/swe_bench_pro/tests/...` if available, otherwise run a dry check.

- [ ] **Step 1: Add `_apply_test_patch` helper**

After `_reset_repo` in `run_selfware.py` add:

```python
def _apply_test_patch(host_repo_dir: Path, test_patch: str, logger: logging.Logger) -> bool:
    """Apply the official test patch to the host repo so the agent can run failing tests."""
    if not test_patch or not test_patch.strip():
        return True
    patch_path = host_repo_dir / ".selfware_test_patch.diff"
    patch_path.write_text(test_patch, encoding="utf-8")
    try:
        proc = run_cmd(
            ["git", "-C", str(host_repo_dir), "apply", str(patch_path)],
            logger=logger,
        )
        if proc.returncode != 0:
            logger.warning("git apply test_patch failed: %s", proc.stderr.strip())
            # Fallback to patch -p1 --no-backup-if-mismatch
            proc = run_cmd(
                ["patch", "-p1", "--no-backup-if-mismatch", "-i", str(patch_path)],
                cwd=host_repo_dir,
                logger=logger,
            )
            if proc.returncode != 0:
                logger.error("patch fallback for test_patch failed: %s", proc.stderr.strip())
                return False
        return True
    finally:
        patch_path.unlink(missing_ok=True)
```

- [ ] **Step 2: Call `_apply_test_patch` after reset/clean**

In `process_instance` after the `git clean` block (around line 2322) add:

```python
if not _apply_test_patch(
    host_repo_dir,
    instance.get("test_patch", "") or "",
    logger,
):
    logger.error("Failed to apply test_patch; aborting instance %s", instance_id)
    return False
```

- [ ] **Step 3: Compute and export the official test command**

In `process_instance`, after applying the test patch, compute:

```python
language = instance.get("repo_language", "")
selected_tests = instance.get("selected_test_files_to_run", []) or instance.get("fail_to_pass", []) or []
test_cmd = _format_test_command(language, selected_tests)
```

Pass `test_cmd` into `run_selfware_on_host` as a new parameter `post_edit_test_command: str | None = None`.

In `run_selfware_on_host`, if `post_edit_test_command` is not None, set:

```python
env["SELFWARE_POST_EDIT_TEST_COMMAND"] = post_edit_test_command
```

- [ ] **Step 4: Update callers of `run_selfware_on_host`**

Find all call sites of `run_selfware_on_host` in `run_selfware.py` and pass `post_edit_test_command=test_cmd`. The call site inside `process_instance` already has `test_cmd`. Other call sites (if any) can pass `None`.

- [ ] **Step 5: Verify harness syntax**

Run: `python -m py_compile system_tests/swe_bench_pro/run_selfware.py`
Expected: no errors.

---

## Verification

After all tasks:

1. `cargo check --release --all-features -q` — must pass.
2. `cargo test --lib --all-features agent::verification::tests testing::verification::tests -- --test-threads=4` — must pass.
3. `python -m py_compile system_tests/swe_bench_pro/run_selfware.py` — must pass.
4. `git diff --stat` — should show changes only in the files above.
