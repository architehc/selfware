//! Test-only helpers shared across unit tests.
//!
//! The process current directory is a single global, but Rust runs unit tests on
//! many threads by default. A test that `set_current_dir`s into a tempdir therefore
//! races any concurrent test that resolves paths against the cwd
//! (`current_project_root`, codemap cost estimation, …), producing
//! non-deterministic failures. Every cwd-sensitive test takes [`CwdGuard`], which
//! serializes them on one lock and restores the original cwd on drop.
#![cfg(test)]

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

// A SINGLE lock serializes every test that touches process-global state — the
// current directory AND the codemap budget atomics. These are entangled: the
// production context-map code mutates the budget atomics as a side effect of the
// agent execute loop, so an execute-loop test is implicitly a budget mutator, and
// the completion gate resolves `current_project_root()` from the cwd. Using one
// lock (rather than separate cwd/budget locks) ensures a cwd test, a budget test,
// and an execute-loop test can never run concurrently and corrupt each other.
static STATE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn state_lock() -> MutexGuard<'static, ()> {
    // Recover from a poisoned lock: a panicking test still restores state on drop,
    // so the guarded invariant (globals only mutated while the lock is held) holds.
    STATE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// RAII guard that serializes cwd-sensitive tests and restores the original
/// working directory when dropped (including on panic).
pub(crate) struct CwdGuard {
    _lock: MutexGuard<'static, ()>,
    saved: PathBuf,
}

impl CwdGuard {
    /// Acquire the cwd lock and change the current directory to `dir`.
    pub(crate) fn enter(dir: &Path) -> Self {
        let guard = Self::hold();
        std::env::set_current_dir(dir).expect("set current dir");
        guard
    }

    /// Acquire the cwd lock WITHOUT changing directory — for tests that only
    /// *read* the cwd and must not run while another test has changed it.
    pub(crate) fn hold() -> Self {
        let lock = state_lock();
        let saved = std::env::current_dir().expect("read current dir");
        Self { _lock: lock, saved }
    }

    /// Change to another directory while already holding the lock (for tests
    /// that walk through several directories in one body).
    pub(crate) fn switch_to(&self, dir: &Path) {
        std::env::set_current_dir(dir).expect("set current dir");
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.saved);
    }
}

/// RAII guard serializing tests that read or write the process-global codemap
/// budget atomics (`TOTAL_BUDGET`, `*_USED_TOKENS`, `FILES_IN_CONTEXT`). Those
/// are shared mutable state; a budget-reading assertion (`fits_in_budget`) flakes
/// when a concurrent test stores different values. The guard resets the budget to
/// a large known default on acquire and on drop, so guarded tests see a
/// deterministic starting point.
pub(crate) struct BudgetGuard {
    _lock: MutexGuard<'static, ()>,
}

impl BudgetGuard {
    pub(crate) fn hold() -> Self {
        let lock = state_lock();
        crate::tools::codemap::update_budget(0, 1_000_000, 0);
        Self { _lock: lock }
    }
}

impl Drop for BudgetGuard {
    fn drop(&mut self) {
        crate::tools::codemap::update_budget(0, 1_000_000, 0);
    }
}

/// Guard for tests that drive the agent execute loop. Such a test BOTH reads the
/// cwd (`current_project_root` in the completion gate) AND mutates the codemap
/// budget atomics (as a side effect of execution), so it needs all of: hold the
/// shared lock (serialize), reset the budget to a large known value on
/// acquire/drop (so a prior execute-loop test that left the budget low can't
/// make this one spuriously trip "budget exceeded"), and restore the cwd on
/// drop. `CwdGuard` alone left the budget inherited — the cause of flaky
/// `continue_execution`/`run_task` failures under parallel test execution.
pub(crate) struct ExecGuard {
    _lock: MutexGuard<'static, ()>,
    saved: PathBuf,
}

impl ExecGuard {
    pub(crate) fn hold() -> Self {
        let lock = state_lock();
        let saved = std::env::current_dir().expect("read current dir");
        crate::tools::codemap::update_budget(0, 1_000_000, 0);
        // Start from a non-cancelled state: a prior test may have exercised
        // `request_shutdown`, and `is_cancelled()` observes that global latch.
        crate::reset_shutdown_for_test();
        Self { _lock: lock, saved }
    }
}

impl Drop for ExecGuard {
    fn drop(&mut self) {
        crate::tools::codemap::update_budget(0, 1_000_000, 0);
        crate::reset_shutdown_for_test();
        let _ = std::env::set_current_dir(&self.saved);
    }
}

/// RAII guard serializing tests that mutate process environment variables
/// (`HOME`, `SELFWARE_API_KEY`, …). Takes the shared state lock — so an env
/// mutator can never run concurrently with a cwd, budget, or exec-loop test —
/// and restores each captured variable to its prior value on drop (including
/// on panic).
pub(crate) struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
    clear_selfware_on_drop: bool,
}

/// `SELFWARE_*` (and provider key) env vars cleared by
/// [`EnvGuard::clear_selfware_env`].
const SELFWARE_ENV_VARS: &[&str] = &[
    "SELFWARE_CONFIG",
    "SELFWARE_ENDPOINT",
    "SELFWARE_MODEL",
    "SELFWARE_API_KEY",
    "OPENROUTER_API_KEY",
    "SELFWARE_MAX_TOKENS",
    "SELFWARE_TEMPERATURE",
    "SELFWARE_TIMEOUT",
    "SELFWARE_THEME",
    "SELFWARE_LOG_LEVEL",
    "SELFWARE_MODE",
    "SELFWARE_STRICT_PERMISSIONS",
];

impl EnvGuard {
    /// Acquire the state lock and capture the current values of `keys`.
    pub(crate) fn capture(keys: &[&'static str]) -> Self {
        let lock = state_lock();
        let saved = keys.iter().map(|k| (*k, std::env::var_os(k))).collect();
        Self {
            _lock: lock,
            saved,
            clear_selfware_on_drop: false,
        }
    }

    /// Acquire the state lock, clear every `SELFWARE_*` env var, and return a
    /// guard that re-clears them on drop (including on panic). This uses the
    /// SAME `STATE_LOCK` as every other guard — the old
    /// `config::test_helpers::ENV_MUTEX` was a second, independent lock, which
    /// let a `SELFWARE_*` mutator race a `HOME`/cwd mutator.
    pub(crate) fn clear_selfware_env() -> Self {
        let lock = state_lock();
        for var in SELFWARE_ENV_VARS {
            std::env::remove_var(var);
        }
        Self {
            _lock: lock,
            saved: Vec::new(),
            clear_selfware_on_drop: true,
        }
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
        if self.clear_selfware_on_drop {
            for var in SELFWARE_ENV_VARS {
                std::env::remove_var(var);
            }
        }
    }
}

/// Permissive agent config for tests that drive the agent against a mock LLM
/// endpoint (Yolo mode, all paths allowed, no streaming, generous iteration
/// budget). Shared by the tool_dispatch / execution / failure_mode suites.
pub(crate) fn mock_agent_config(endpoint: &str) -> crate::config::Config {
    crate::config::Config {
        endpoint: endpoint.to_string(),
        model: "mock-model".to_string(),
        agent: crate::config::AgentConfig {
            max_iterations: 50,
            step_timeout_secs: 10,
            stream_stall_timeout_secs: None,
            streaming: false,
            native_function_calling: false,
            min_completion_steps: 0,
            require_verification_before_completion: false,
            ..Default::default()
        },
        safety: crate::config::SafetyConfig {
            allowed_paths: vec!["./**".to_string(), "/**".to_string()],
            ..Default::default()
        },
        execution_mode: crate::config::ExecutionMode::Yolo,
        ..Default::default()
    }
}

/// [`mock_agent_config`] with explicit context/token/iteration limits, for
/// tests that exercise budget or failure-mode thresholds.
pub(crate) fn mock_agent_config_with_limits(
    endpoint: &str,
    context_length: usize,
    max_tokens: usize,
    max_iterations: usize,
    step_timeout_secs: u64,
) -> crate::config::Config {
    let mut config = mock_agent_config(endpoint);
    config.context_length = context_length;
    config.max_tokens = max_tokens;
    config.agent.max_iterations = max_iterations;
    config.agent.step_timeout_secs = step_timeout_secs;
    config
}
