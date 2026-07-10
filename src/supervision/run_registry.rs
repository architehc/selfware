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
    /// Process-unique id: "<pid>-<local_run_id>".
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
pub fn pid_alive(pid: u32) -> bool {
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    // Signal `None` (0) performs error checking without sending a signal.
    kill(Pid::from_raw(pid as i32), None).is_ok()
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
        std::fs::rename(&tmp, &path)
            .with_context(|| format!("rename into {}", path.display()))?;
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
        let rec = serde_json::from_str(&data)
            .with_context(|| format!("parse {}", path.display()))?;
        Ok(Some(rec))
    }

    /// Update status + `updated_unix` of an existing record. No-op if absent.
    pub fn update_status(&self, id: &str, status: &str, now_unix: i64) -> Result<()> {
        if let Some(mut rec) = self.get(id)? {
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
        out.sort_by(|a, b| b.started_unix.cmp(&a.started_unix));
        Ok(out)
    }

    /// Remove a record file. Ok even if already gone.
    pub fn remove(&self, id: &str) -> Result<()> {
        let path = self.record_path(id);
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("remove {}", path.display()))?;
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
        let outcome = if pid_alive(rec.pid) {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            let _ = kill(Pid::from_raw(rec.pid as i32), Signal::SIGTERM);
            AbortOutcome::Signalled(rec.pid)
        } else {
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
    fn abort_dead_pid_is_was_stale_and_marks_aborted() {
        let dir = tempfile::tempdir().unwrap();
        let reg = RunRegistry::with_dir(dir.path().to_path_buf()).unwrap();
        reg.write(&rec("1-1", DEAD_PID, "Running", 100)).unwrap();
        assert_eq!(reg.abort("1-1", 200).unwrap(), AbortOutcome::WasStale);
        assert_eq!(reg.get("1-1").unwrap().unwrap().status, "Aborted");
    }

    #[test]
    fn abort_live_process_signals_and_marks_aborted() {
        let dir = tempfile::tempdir().unwrap();
        let reg = RunRegistry::with_dir(dir.path().to_path_buf()).unwrap();
        // Spawn a real, long-lived child to signal.
        let mut child = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .unwrap();
        let pid = child.id();
        reg.write(&rec("live-1", pid, "Running", 100)).unwrap();
        let outcome = reg.abort("live-1", 200).unwrap();
        assert_eq!(outcome, AbortOutcome::Signalled(pid));
        assert_eq!(reg.get("live-1").unwrap().unwrap().status, "Aborted");
        // Reap the child so it doesn't linger (SIGTERM should have ended it).
        let _ = child.wait();
    }
}
