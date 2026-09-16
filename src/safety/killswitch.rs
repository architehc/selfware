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

#[cfg(test)]
pub(crate) struct KillswitchTestLock;

#[cfg(test)]
pub(crate) struct KillswitchTestGuard {
    _lock: parking_lot::ReentrantMutexGuard<'static, ()>,
}

#[cfg(test)]
impl KillswitchTestLock {
    pub(crate) fn lock(&self) -> KillswitchTestGuard {
        let lock = crate::test_support::state_lock();
        std::env::remove_var(KILLSWITCH_ENV_VAR);
        std::env::remove_var("SELFWARE_KILLSWITCH_IGNORE_HOME");
        reset_in_process();
        KillswitchTestGuard { _lock: lock }
    }
}

#[cfg(test)]
impl Drop for KillswitchTestGuard {
    fn drop(&mut self) {
        std::env::remove_var(KILLSWITCH_ENV_VAR);
        std::env::remove_var("SELFWARE_KILLSWITCH_IGNORE_HOME");
        reset_in_process();
    }
}

#[cfg(test)]
pub(crate) static KILLSWITCH_TEST_LOCK: KillswitchTestLock = KillswitchTestLock;

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

/// Parse an environment variable value to determine if it activates the killswitch.
/// Returns Some(reason) if active, or None if inactive/falsy.
pub fn parse_env_killswitch_value(val: &str) -> Option<String> {
    let trimmed = val.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.is_empty() && lower != "0" && lower != "false" && lower != "no" && lower != "off" {
        Some(if trimmed.is_empty() {
            "env var present".to_string()
        } else {
            trimmed.to_string()
        })
    } else {
        None
    }
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
        if let Some(reason) = parse_env_killswitch_value(&val) {
            return Err(KillswitchError::Environment { reason });
        }
    }

    // 3. File existence checks (fail-closed for project root / cwd)
    let mut check_paths = Vec::new();

    // Specific project root if provided, otherwise check current working directory
    if let Some(root) = project_root {
        check_paths.push(root.join(".selfware").join(KILLSWITCH_FILE_NAME));
    } else if let Ok(cwd) = std::env::current_dir() {
        let cwd_ks = cwd.join(".selfware").join(KILLSWITCH_FILE_NAME);
        check_paths.push(cwd_ks);
    }

    // User home directory (strictly test-only bypass for test suite isolation)
    #[cfg(test)]
    let ignore_home = std::env::var("SELFWARE_KILLSWITCH_IGNORE_HOME")
        .or_else(|_| std::env::var("SELFWARE_NO_HOME_KILLSWITCH"))
        .or_else(|_| std::env::var("SELFWARE_DISABLE_HOME_KILLSWITCH"))
        .map(|v| {
            let lower = v.trim().to_ascii_lowercase();
            !lower.is_empty() && lower != "0" && lower != "false" && lower != "no" && lower != "off"
        })
        .unwrap_or(false);
    #[cfg(not(test))]
    let ignore_home = false;

    if !ignore_home {
        if let Some(home) = dirs::home_dir() {
            let home_selfware = home.join(".selfware");
            // Inspect home/.selfware directory: use std::fs::metadata to resolve symlinks
            match std::fs::metadata(&home_selfware) {
                Ok(meta) => {
                    if meta.is_dir() {
                        let home_ks = home_selfware.join(KILLSWITCH_FILE_NAME);
                        match home_ks.symlink_metadata() {
                            Ok(_) => {
                                // Sentinel exists at home killswitch path; inspect it
                                check_paths.push(home_ks);
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                                // Genuinely absent
                            }
                            Err(e) => {
                                // Inspection error (e.g. EACCES, PermissionDenied, I/O error): fail closed!
                                return Err(KillswitchError::File {
                                    path: home_ks,
                                    reason: format!(
                                        "Cannot verify home killswitch path ({e}): failing closed"
                                    ),
                                });
                            }
                        }
                    } else {
                        // .selfware exists in home but is not a directory: fail closed!
                        return Err(KillswitchError::File {
                            path: home_selfware,
                            reason: "Home .selfware path is not a directory: failing closed"
                                .to_string(),
                        });
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // ~/.selfware does not exist; no global killswitch present
                }
                Err(e) => {
                    // Inspection error reading ~/.selfware (e.g. EACCES, PermissionDenied, I/O error): fail closed!
                    return Err(KillswitchError::File {
                        path: home_selfware,
                        reason: format!(
                            "Cannot verify home killswitch directory ({e}): failing closed"
                        ),
                    });
                }
            }
        }
    }

    for path in check_paths {
        match path.symlink_metadata() {
            Ok(meta) => {
                let reason = if meta.file_type().is_symlink() {
                    "Killswitch symlink present".to_string()
                } else if meta.is_dir() {
                    "Killswitch directory present".to_string()
                } else if !meta.file_type().is_file() {
                    // FIFO, socket, char/block device: fail closed immediately WITHOUT opening or reading!
                    format!("Killswitch special file present ({:?})", meta.file_type())
                } else {
                    // Regular file: safe bounded read with O_NONBLOCK to prevent FIFO/device open hangs
                    use std::io::Read;
                    let mut open_opts = std::fs::OpenOptions::new();
                    open_opts.read(true);
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::OpenOptionsExt;
                        open_opts.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
                    }

                    match open_opts.open(&path) {
                        Ok(file) => {
                            // Verify opened descriptor is indeed a regular file
                            if let Ok(stat) = file.metadata() {
                                if !stat.file_type().is_file() {
                                    return Err(KillswitchError::File {
                                        path: path.clone(),
                                        reason: format!(
                                            "Killswitch special file present ({:?})",
                                            stat.file_type()
                                        ),
                                    });
                                }
                            }
                            let mut buf = String::new();
                            match file.take(4096).read_to_string(&mut buf) {
                                Ok(_) => {
                                    let trimmed = buf.trim();
                                    if trimmed.is_empty() {
                                        "Killswitch file present".to_string()
                                    } else {
                                        trimmed.to_string()
                                    }
                                }
                                Err(_) => "Killswitch file present (unreadable)".to_string(),
                            }
                        }
                        Err(_) => "Killswitch file present (unreadable)".to_string(),
                    }
                };
                return Err(KillswitchError::File { path, reason });
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Genuinely absent, continue to next path
            }
            Err(e) => {
                // Project root / cwd / home unreadable ancestor or permission denied: FAIL CLOSED
                return Err(KillswitchError::File {
                    path: path.clone(),
                    reason: format!("Cannot verify killswitch path ({e}): failing closed"),
                });
            }
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

/// Outcome of tripping a file-based killswitch sentinel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TripFileOutcome {
    /// Sentinel file was created or updated with the specified reason.
    Written { path: PathBuf },
    /// Sentinel already existed as a special file (symlink, FIFO, socket, device, directory)
    /// and is already active; the sentinel was preserved without opening, and the reason was
    /// not written to disk.
    PreservedExisting { path: PathBuf, description: String },
}

impl TripFileOutcome {
    pub fn path(&self) -> &Path {
        match self {
            Self::Written { path } => path,
            Self::PreservedExisting { path, .. } => path,
        }
    }

    pub fn into_path(self) -> PathBuf {
        match self {
            Self::Written { path } => path,
            Self::PreservedExisting { path, .. } => path,
        }
    }
}

impl std::ops::Deref for TripFileOutcome {
    type Target = Path;
    fn deref(&self) -> &Self::Target {
        self.path()
    }
}

impl AsRef<Path> for TripFileOutcome {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

/// Create a `.selfware/KILLSWITCH` file in the given directory with the provided reason.
pub fn trip_file_killswitch(project_root: &Path, reason: &str) -> std::io::Result<TripFileOutcome> {
    let selfware_dir = project_root.join(".selfware");
    std::fs::create_dir_all(&selfware_dir)?;
    let killswitch_path = selfware_dir.join(KILLSWITCH_FILE_NAME);

    // Inspect destination before opening or writing
    match killswitch_path.symlink_metadata() {
        Ok(meta) => {
            if meta.file_type().is_symlink() || !meta.file_type().is_file() {
                // If a symlink, FIFO, socket, device, or directory already exists at the
                // killswitch path, detection ALREADY treats it as active (fail-closed).
                // Do NOT open, write, or replace it — doing so could block indefinitely on
                // a FIFO or overwrite a symlink target outside the workspace.
                // Preserve the existing sentinel node and return immediately.
                let description = if meta.file_type().is_symlink() {
                    "symlink present".to_string()
                } else if meta.is_dir() {
                    "directory present".to_string()
                } else {
                    format!("special file present ({:?})", meta.file_type())
                };
                tracing::info!(
                    "Killswitch sentinel already exists ({description}) at {:?}; treating as active without opening",
                    killswitch_path
                );
                return Ok(TripFileOutcome::PreservedExisting {
                    path: killswitch_path,
                    description,
                });
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Not present, safe to create
        }
        Err(e) => {
            return Err(e);
        }
    }

    let content = if reason.trim().is_empty() {
        format!("Tripped at {}\n", chrono::Utc::now().to_rfc3339())
    } else {
        format!(
            "Tripped at {}: {}\n",
            chrono::Utc::now().to_rfc3339(),
            reason.trim()
        )
    };

    let mut open_opts = std::fs::OpenOptions::new();
    open_opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        open_opts.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    use std::io::Write;
    let mut file = open_opts.open(&killswitch_path)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    Ok(TripFileOutcome::Written {
        path: killswitch_path,
    })
}

/// Remove a `.selfware/KILLSWITCH` file in the given directory if it exists.
/// Returns `Ok(true)` if a file was removed, `Ok(false)` if none existed.
pub fn remove_file_killswitch(project_root: &Path) -> std::io::Result<bool> {
    let killswitch_path = project_root.join(".selfware").join(KILLSWITCH_FILE_NAME);
    match killswitch_path.symlink_metadata() {
        Ok(meta) => {
            if meta.is_dir() {
                // Verify directory is empty to ensure user/operator contents survive
                let mut entries = std::fs::read_dir(&killswitch_path)?;
                if entries.next().transpose()?.is_some() {
                    return Err(std::io::Error::other(format!(
                        "Killswitch directory '{}' is not empty; refusing to recursively delete contents",
                        killswitch_path.display()
                    )));
                }
                std::fs::remove_dir(&killswitch_path)?;
            } else {
                std::fs::remove_file(&killswitch_path)?;
            }
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/safety/killswitch_test.rs"]
mod tests;
