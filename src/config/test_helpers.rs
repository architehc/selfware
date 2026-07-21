//! Shared test helpers for env-variable isolation.
//!
//! Both `config::tests` and `cli::tests` mutate `SELFWARE_*` env vars.  When
//! the lib test suite runs with multiple threads (the default), those
//! mutations race.  The `test_env_guard()` helper acquires a global mutex and
//! returns an RAII guard that **clears** every `SELFWARE_*` variable on drop,
//! so even panicking tests don't leave pollution behind.

use std::sync::Mutex;

pub static ENV_MUTEX: Mutex<()> = Mutex::new(());

pub struct EnvGuard(#[allow(dead_code)] std::sync::MutexGuard<'static, ()>);

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for var in &[
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
        ] {
            std::env::remove_var(var);
        }
    }
}

/// Lock the global env mutex, clear all `SELFWARE_*` vars, and return a
/// guard that re-clears them on drop.
pub fn clear_env() -> EnvGuard {
    let guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    for var in &[
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
    ] {
        std::env::remove_var(var);
    }
    EnvGuard(guard)
}
