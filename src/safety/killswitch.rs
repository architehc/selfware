//! Fail-closed killswitch for autonomous loops, evolution, and skill activation.
//!
//! When the killswitch is active, any evolution loop, self-improvement run,
//! candidate mutation, or candidate skill activation halts immediately.
//!
//! Tripping mechanisms:
//! 1. Environment variable: `SELFWARE_KILLSWITCH=1` (or any value other than "0", "false", "no", "")
//! 2. Killswitch file: `.selfware/KILLSWITCH` in project root, current directory, or user home
//! 3. In-process atomic flag: [`trip_in_process`]

use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub const KILLSWITCH_ENV_VAR: &str = "SELFWARE_KILLSWITCH";
pub const KILLSWITCH_FILE_NAME: &str = "KILLSWITCH";

static IN_PROCESS_KILLSWITCH: AtomicBool = AtomicBool::new(false);
static IN_PROCESS_REASON: RwLock<Option<String>> = RwLock::new(None);

/// Source and details of an active killswitch.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum KillswitchError {
    #[error("Killswitch active from environment variable {KILLSWITCH_ENV_VAR}: {reason}")]
    Environment { reason: String },
    #[error("Killswitch active from file '{}': {reason}", path.display())]
    File { path: PathBuf, reason: String },
    #[error("Killswitch active in-process: {reason}")]
    InProcess { reason: String },
}

/// Status summary of the killswitch check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KillswitchStatus {
    pub is_active: bool,
    pub error: Option<KillswitchError>,
}

impl KillswitchStatus {
    pub fn inactive() -> Self {
        Self {
            is_active: false,
            error: None,
        }
    }

    pub fn active(error: KillswitchError) -> Self {
        Self {
            is_active: true,
            error: Some(error),
        }
    }
}

/// Trip the in-process killswitch with an explanatory reason.
pub fn trip_in_process(reason: impl Into<String>) {
    let reason_str = reason.into();
    *IN_PROCESS_REASON.write() = Some(reason_str);
    IN_PROCESS_KILLSWITCH.store(true, Ordering::SeqCst);
}

/// Reset the in-process killswitch flag (primarily for tests).
pub fn reset_in_process() {
    IN_PROCESS_KILLSWITCH.store(false, Ordering::SeqCst);
    *IN_PROCESS_REASON.write() = None;
}

/// Check if the killswitch is currently active anywhere (in-process, env, or default paths).
pub fn is_killswitch_active() -> bool {
    check_killswitch(None).is_err()
}

/// Detailed killswitch check against a specific or default project root.
/// Returns `Ok(())` if safe to proceed, or `Err(KillswitchError)` if tripped.
pub fn check_killswitch(project_root: Option<&Path>) -> Result<(), KillswitchError> {
    // 1. In-process atomic check (fastest, zero allocation)
    if IN_PROCESS_KILLSWITCH.load(Ordering::SeqCst) {
        let reason = IN_PROCESS_REASON
            .read()
            .clone()
            .unwrap_or_else(|| "In-process killswitch triggered".to_string());
        return Err(KillswitchError::InProcess { reason });
    }

    // 2. Environment variable check
    if let Ok(val) = std::env::var(KILLSWITCH_ENV_VAR) {
        let trimmed = val.trim();
        let lower = trimmed.to_ascii_lowercase();
        if !lower.is_empty() && lower != "0" && lower != "false" && lower != "no" && lower != "off"
        {
            return Err(KillswitchError::Environment {
                reason: if trimmed.is_empty() {
                    "env var present".to_string()
                } else {
                    trimmed.to_string()
                },
            });
        }
    }

    // 3. File existence checks
    let mut check_paths = Vec::new();

    // Specific project root if provided, otherwise check current working directory
    if let Some(root) = project_root {
        check_paths.push(root.join(".selfware").join(KILLSWITCH_FILE_NAME));
    } else if let Ok(cwd) = std::env::current_dir() {
        let cwd_ks = cwd.join(".selfware").join(KILLSWITCH_FILE_NAME);
        if !check_paths.contains(&cwd_ks) {
            check_paths.push(cwd_ks);
        }
    }

    // User home directory
    if let Some(home) = dirs::home_dir() {
        let home_ks = home.join(".selfware").join(KILLSWITCH_FILE_NAME);
        if !check_paths.contains(&home_ks) {
            check_paths.push(home_ks);
        }
    }

    for path in check_paths {
        if path.exists() && path.is_file() {
            let reason = std::fs::read_to_string(&path)
                .map(|s| {
                    let trimmed = s.trim();
                    if trimmed.is_empty() {
                        "Killswitch file present".to_string()
                    } else {
                        trimmed.to_string()
                    }
                })
                .unwrap_or_else(|_| "Killswitch file present (unreadable)".to_string());
            return Err(KillswitchError::File { path, reason });
        }
    }

    Ok(())
}

/// Retrieve the current status of the killswitch.
pub fn get_killswitch_status(project_root: Option<&Path>) -> KillswitchStatus {
    match check_killswitch(project_root) {
        Ok(()) => KillswitchStatus::inactive(),
        Err(e) => KillswitchStatus::active(e),
    }
}

/// Create a `.selfware/KILLSWITCH` file in the given directory with the provided reason.
pub fn trip_file_killswitch(project_root: &Path, reason: &str) -> std::io::Result<PathBuf> {
    let selfware_dir = project_root.join(".selfware");
    std::fs::create_dir_all(&selfware_dir)?;
    let killswitch_path = selfware_dir.join(KILLSWITCH_FILE_NAME);
    let content = if reason.trim().is_empty() {
        format!("Tripped at {}\n", chrono::Utc::now().to_rfc3339())
    } else {
        format!(
            "Tripped at {}: {}\n",
            chrono::Utc::now().to_rfc3339(),
            reason.trim()
        )
    };
    std::fs::write(&killswitch_path, content)?;
    Ok(killswitch_path)
}

/// Remove a `.selfware/KILLSWITCH` file in the given directory if it exists.
/// Returns `Ok(true)` if a file was removed, `Ok(false)` if none existed.
pub fn remove_file_killswitch(project_root: &Path) -> std::io::Result<bool> {
    let killswitch_path = project_root.join(".selfware").join(KILLSWITCH_FILE_NAME);
    if killswitch_path.exists() {
        std::fs::remove_file(killswitch_path)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/safety/killswitch_test.rs"]
mod tests;
