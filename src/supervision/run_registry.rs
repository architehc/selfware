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
mod tests {
    use super::*;

    fn rec(id: &str, pid: u32, status: &str, started: i64) -> RunRecord {
        RunRecord {
            id: id.to_string(),
            pid,
            task: format!("task {id}"),
            status: status.to_string(),
            started_unix: started,
            updated_unix: started,
        }
    }

    // A pid that is essentially guaranteed not to exist (positive, > pid_max).
    const DEAD_PID: u32 = 2_147_483_646;

    #[test]
    fn write_then_get_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let reg = RunRegistry::with_dir(dir.path().to_path_buf()).unwrap();
        let r = rec("1234-1", 1234, "Running", 100);
        reg.write(&r).unwrap();
        assert_eq!(reg.get("1234-1").unwrap(), Some(r));
        assert_eq!(reg.get("nope").unwrap(), None);
    }

    #[test]
    fn list_is_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let reg = RunRegistry::with_dir(dir.path().to_path_buf()).unwrap();
        reg.write(&rec("1-1", 1, "Completed", 100)).unwrap();
        reg.write(&rec("1-2", 1, "Completed", 300)).unwrap();
        reg.write(&rec("1-3", 1, "Completed", 200)).unwrap();
        let ids: Vec<String> = reg.list().unwrap().into_iter().map(|r| r.id).collect();
        assert_eq!(ids, vec!["1-2", "1-3", "1-1"]);
    }

    #[test]
    fn update_status_changes_status() {
        let dir = tempfile::tempdir().unwrap();
        let reg = RunRegistry::with_dir(dir.path().to_path_buf()).unwrap();
        reg.write(&rec("1-1", 1, "Running", 100)).unwrap();
        reg.update_status("1-1", "Completed", 150).unwrap();
        let g = reg.get("1-1").unwrap().unwrap();
        assert_eq!(g.status, "Completed");
        assert_eq!(g.updated_unix, 150);
    }

    #[test]
    fn effective_status_reports_stale_for_dead_pid() {
        let running_dead = rec("x", DEAD_PID, "Running", 100);
        assert_eq!(running_dead.effective_status(), "Stale");
        let completed_dead = rec("y", DEAD_PID, "Completed", 100);
        assert_eq!(completed_dead.effective_status(), "Completed");
    }

    #[test]
    fn abort_unknown_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let reg = RunRegistry::with_dir(dir.path().to_path_buf()).unwrap();
        assert_eq!(reg.abort("missing", 200).unwrap(), AbortOutcome::NotFound);
    }

    #[test]
    fn abort_already_terminal_is_already_done() {
        let dir = tempfile::tempdir().unwrap();
        let reg = RunRegistry::with_dir(dir.path().to_path_buf()).unwrap();
        reg.write(&rec("1-1", 1, "Completed", 100)).unwrap();
        assert_eq!(reg.abort("1-1", 200).unwrap(), AbortOutcome::AlreadyDone);
    }

    #[test]
    fn pid_alive_detects_running_and_dead() {
        // The cross-platform liveness probe (sysinfo) that replaced the old
        // "always true on non-Unix" stub: the test process itself is alive,
        // and a pid past pid_max is not. On the old non-Unix path both would
        // have (wrongly) reported alive.
        assert!(
            pid_alive(std::process::id()),
            "current process must be alive"
        );
        assert!(!pid_alive(DEAD_PID), "a pid past pid_max must be dead");
    }

    #[test]
    fn abort_dead_pid_is_was_stale_and_marks_aborted() {
        let dir = tempfile::tempdir().unwrap();
        let reg = RunRegistry::with_dir(dir.path().to_path_buf()).unwrap();
        reg.write(&rec("1-1", DEAD_PID, "Running", 100)).unwrap();
        assert_eq!(reg.abort("1-1", 200).unwrap(), AbortOutcome::WasStale);
        assert_eq!(reg.get("1-1").unwrap().unwrap().status, "Aborted");
    }

    #[test]
    fn abort_live_non_selfware_pid_is_not_signalled() {
        let dir = tempfile::tempdir().unwrap();
        let reg = RunRegistry::with_dir(dir.path().to_path_buf()).unwrap();
        // A live process whose PID matches the record but which is NOT a
        // selfware process (i.e. a reused PID). It must NOT be signalled;
        // the run is recorded Aborted regardless.
        let mut child = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .unwrap();
        let pid = child.id();
        // Fork/exec race: right after spawn, /proc/<pid>/exe can still point
        // at the test binary (whose path contains "selfware"), so
        // pid_is_selfware would misfire and the run would be Signalled.
        // Wait until the child has actually exec'd sleep.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while pid_is_selfware(pid) {
            assert!(
                std::time::Instant::now() < deadline,
                "child {pid} never exec'd into a non-selfware process"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        reg.write(&rec("live-1", pid, "Running", 100)).unwrap();
        let outcome = reg.abort("live-1", 200).unwrap();
        assert_eq!(outcome, AbortOutcome::WasStale);
        assert_eq!(reg.get("live-1").unwrap().unwrap().status, "Aborted");
        // The unrelated process must be untouched (still alive) — reap it.
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn update_status_terminal_states_are_sticky() {
        let dir = tempfile::tempdir().unwrap();
        let reg = RunRegistry::with_dir(dir.path().to_path_buf()).unwrap();
        // Running -> Completed is allowed.
        reg.write(&rec("r1", 1, "Running", 100)).unwrap();
        reg.update_status("r1", "Completed", 110).unwrap();
        assert_eq!(reg.get("r1").unwrap().unwrap().status, "Completed");
        // Aborted must NOT be overwritten by a late Completed (the P0).
        reg.write(&rec("r2", 1, "Aborted", 100)).unwrap();
        reg.update_status("r2", "Completed", 120).unwrap();
        assert_eq!(reg.get("r2").unwrap().unwrap().status, "Aborted");
        // Completed/Failed are likewise sticky.
        reg.write(&rec("r3", 1, "Completed", 100)).unwrap();
        reg.update_status("r3", "Running", 130).unwrap();
        assert_eq!(reg.get("r3").unwrap().unwrap().status, "Completed");
    }
}
