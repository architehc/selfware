//! Persistent, cross-invocation registry of supervised runs.
//!
//! [`crate::supervision::run_supervisor::RunSupervisor`] tracks runs in memory,
//! so `selfware runs list`/`abort` in one process cannot see runs started by
//! another. This registry persists a small JSON record per run under
//! `~/.selfware/runs/`, keyed by a process-unique id (`<pid>-<local_id>`), so
//! listing and aborting work across separate CLI invocations. Liveness is
//! derived from whether the owning pid is still alive.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A persisted record of a single supervised run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunRecord {
    /// Process-unique id: `<pid>-<local_run_id>`.
    pub id: String,
    /// OS process id that started (owns) the run.
    pub pid: u32,
    /// Human-readable task description.
    pub task: String,
    /// Last persisted status: "Running" | "Completed" | "Failed" | "Aborted".
    pub status: String,
    /// Unix seconds when the run was first recorded.
    pub started_unix: i64,
    /// Unix seconds of the last status update.
    pub updated_unix: i64,
}

impl RunRecord {
    /// Effective status accounting for liveness: a record still marked
    /// "Running" whose owning process is gone is reported as "Stale".
    pub fn effective_status(&self) -> &str {
        if self.status == "Running" && !pid_alive(self.pid) {
            "Stale"
        } else {
            self.status.as_str()
        }
    }
}

/// Returns true if a process with `pid` currently exists.
///
/// Cross-platform via `sysinfo` (already a dependency). The previous version
/// probed liveness with `nix` on Unix and unconditionally returned `true` on
/// every other platform — so on Windows a finished run was reported alive
/// forever and never reclaimed (its status stuck at "Running").
pub fn pid_alive(pid: u32) -> bool {
    use sysinfo::{Pid, ProcessesToUpdate, System};
    let target = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::Some(&[target]), true);
    sys.process(target).is_some()
}

/// Whether a status string is a terminal (final) run state. Terminal
/// states are sticky — see [`RunRegistry::update_status`].
pub fn is_terminal_status(status: &str) -> bool {
    matches!(status, "Completed" | "Failed" | "Aborted")
}

/// Best-effort check that `pid` still belongs to a *selfware* process, to
/// guard against PID reuse: after the original owner exits, the OS may
/// reassign its PID to an unrelated process, and we must never signal that.
///
/// Cross-platform via `sysinfo`. The previous version only inspected `/proc`
/// on Linux and returned `true` unconditionally on macOS/Windows — so on those
/// platforms a REUSED pid was assumed to still be selfware and could be
/// SIGTERM'd even though it now belonged to an unrelated program.
pub fn pid_is_selfware(pid: u32) -> bool {
    use sysinfo::{Pid, ProcessesToUpdate, System};
    let target = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::Some(&[target]), true);
    match sys.process(target) {
        Some(proc_) => {
            proc_.name().to_string_lossy().contains("selfware")
                || proc_
                    .exe()
                    .map(|e| e.to_string_lossy().contains("selfware"))
                    .unwrap_or(false)
        }
        None => false,
    }
}

/// Result of a cross-process abort attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbortOutcome {
    /// No record with that id.
    NotFound,
    /// The run was already in a terminal state.
    AlreadyDone,
    /// The owning process was already gone; record marked Aborted.
    WasStale,
    /// SIGTERM was sent to the owning pid; record marked Aborted.
    Signalled(u32),
    /// The owning process is alive but this platform has no supported way to
    /// signal it (non-Unix; `nix` is Unix-only). The run is NOT marked Aborted
    /// — it is still running — and the caller must report that honestly.
    SignalUnsupported(u32),
    /// The owning process is alive and ours, but the abort signal itself failed
    /// (e.g. EPERM — we lack permission to signal it). The run is NOT marked
    /// Aborted; it is still running and the caller must report that honestly.
    SignalFailed(u32, String),
}

/// A filesystem-backed registry of run records under a directory.
pub struct RunRegistry {
    dir: PathBuf,
}

impl RunRegistry {
    /// Registry rooted at the default location (`~/.selfware/runs/`).
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().context("cannot resolve home directory")?;
        Self::with_dir(home.join(".selfware").join("runs"))
    }

    /// Registry rooted at an explicit directory (used by tests).
    pub fn with_dir(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("create run registry dir {}", dir.display()))?;
        Ok(Self { dir })
    }

    fn record_path(&self, id: &str) -> PathBuf {
        // ids are "<pid>-<local>"; sanitize to safe filename chars regardless.
        let safe: String = id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.dir.join(format!("{safe}.json"))
    }

    /// Write (or overwrite) a run record atomically (temp file + rename).
    pub fn write(&self, record: &RunRecord) -> Result<()> {
        let path = self.record_path(&record.id);
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(record).context("serialize run record")?;
        std::fs::write(&tmp, json).with_context(|| format!("write {}", tmp.display()))?;
        std::fs::rename(&tmp, &path).with_context(|| format!("rename into {}", path.display()))?;
        Ok(())
    }

    /// Read a single record by id, or `None` if absent.
    pub fn get(&self, id: &str) -> Result<Option<RunRecord>> {
        let path = self.record_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let data =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let rec =
            serde_json::from_str(&data).with_context(|| format!("parse {}", path.display()))?;
        Ok(Some(rec))
    }

    /// Update status + `updated_unix` of an existing record. No-op if absent.
    ///
    /// Terminal states are STICKY: once a run is `Aborted`, `Completed`, or
    /// `Failed`, a later `update_status` call must not move it away. This is
    /// the fix for an aborted run being re-recorded as `Completed` by a
    /// SIGTERM'd owner that returns `Ok` and reports its final in-process
    /// status.
    pub fn update_status(&self, id: &str, status: &str, now_unix: i64) -> Result<()> {
        if let Some(mut rec) = self.get(id)? {
            // Terminal states are STICKY: once a run is
            // Aborted/Completed/Failed, a later update must not move it away.
            // This is the fix for an aborted run being re-recorded as
            // Completed by a SIGTERM'd owner that returns Ok and reports its
            // final in-process status.
            if is_terminal_status(&rec.status) {
                return Ok(());
            }
            rec.status = status.to_string();
            rec.updated_unix = now_unix;
            self.write(&rec)?;
        }
        Ok(())
    }

    /// All records, newest first (by `started_unix`). Malformed files skipped.
    pub fn list(&self) -> Result<Vec<RunRecord>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&self.dir)
            .with_context(|| format!("read dir {}", self.dir.display()))?
        {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(data) = std::fs::read_to_string(&path) {
                if let Ok(rec) = serde_json::from_str::<RunRecord>(&data) {
                    out.push(rec);
                }
            }
        }
        out.sort_by_key(|b| std::cmp::Reverse(b.started_unix));
        Ok(out)
    }

    /// Remove a record file. Ok even if already gone.
    pub fn remove(&self, id: &str) -> Result<()> {
        let path = self.record_path(id);
        if path.exists() {
            std::fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
        }
        Ok(())
    }

    /// Abort a run by id, across processes.
    ///
    /// NOTE: SIGTERM terminates the whole owning process, which may host other
    /// runs — the CLI surfaces this caveat to the user.
    pub fn abort(&self, id: &str, now_unix: i64) -> Result<AbortOutcome> {
        let Some(mut rec) = self.get(id)? else {
            return Ok(AbortOutcome::NotFound);
        };
        if matches!(rec.status.as_str(), "Completed" | "Failed" | "Aborted") {
            return Ok(AbortOutcome::AlreadyDone);
        }
        // Only signal if the PID is still alive AND still a selfware
        // process — never SIGTERM a reused PID belonging to an unrelated
        // program.
        let outcome = if pid_alive(rec.pid) && pid_is_selfware(rec.pid) {
            #[cfg(unix)]
            {
                use nix::errno::Errno;
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;
                match kill(Pid::from_raw(rec.pid as i32), Signal::SIGTERM) {
                    Ok(()) => AbortOutcome::Signalled(rec.pid),
                    // Raced: the process vanished between the liveness check and
                    // the signal. Effectively dead → record it stale/aborted.
                    Err(Errno::ESRCH) => AbortOutcome::WasStale,
                    // e.g. EPERM: alive but we may not signal it. Do NOT mark
                    // the run Aborted; report the failure honestly so the caller
                    // knows it is still running.
                    Err(e) => return Ok(AbortOutcome::SignalFailed(rec.pid, e.to_string())),
                }
            }
            #[cfg(not(unix))]
            {
                // No portable way to signal without nix (Unix-only). The process
                // is still alive — return early WITHOUT marking the run Aborted,
                // so we don't falsely report a successful termination.
                return Ok(AbortOutcome::SignalUnsupported(rec.pid));
            }
        } else {
            // Owner gone, or its PID was reused by an unrelated process — do
            // not signal; the run is effectively dead, so record it Aborted.
            AbortOutcome::WasStale
        };
        rec.status = "Aborted".to_string();
        rec.updated_unix = now_unix;
        self.write(&rec)?;
        Ok(outcome)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/supervision/run_registry/run_registry_test.rs"]
mod tests;
