//! Best-snapshot restore: submit the best state, not the last state.
//!
//! Opus 5 consult (2026-08-25, six-model panel on the TB 3.0 evidence): a
//! task that was 80% green at minute 30 submits a broken edit at minute 60.
//! The harness snapshots the agent's written/edited files whenever its own
//! verification passes, and restores that last-green state when the run fails
//! (abort, stall, budget stop). Read-only tasks and cancellations are never
//! touched.
//!
//! Snapshot scope and restore semantics:
//! - Each instance owns a unique temp directory (PID + uuid), so concurrent
//!   in-process workers never share state and `clear()` only affects the
//!   owning instance.
//! - Captured files are mirrored by their full relative path under the
//!   snapshot dir (`files/`), so distinct source paths never collide.
//! - A manifest records exactly which paths existed at capture time.
//!   `restore_written` restores those, and removes any file in the caller's
//!   written set that the manifest proves was absent at capture (created
//!   after the last-green state). Files outside the caller-provided written
//!   set are never touched.

use std::path::{Component, Path, PathBuf};

/// File-backed snapshot of the agent's written deliverables, kept outside the
/// workspace (temp dir) so a broken end state can't corrupt it.
pub(crate) struct AgentSnapshot {
    dir: PathBuf,
    taken: bool,
}

impl Default for AgentSnapshot {
    fn default() -> Self {
        Self {
            // PID for debuggability, uuid so concurrent in-process instances
            // (and PID reuse across runs) can never share a directory.
            dir: std::env::temp_dir().join(format!(
                "selfware-snapshot-{}-{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            )),
            taken: false,
        }
    }
}

impl AgentSnapshot {
    /// Root under which captured file contents are mirrored. Kept separate
    /// from the manifest so a captured file can never shadow it.
    fn files_dir(&self) -> PathBuf {
        self.dir.join("files")
    }

    fn manifest_path(&self) -> PathBuf {
        self.dir.join("manifest.json")
    }

    /// Component-wise normalization: collapses `.` and repeated separators so
    /// `a.py` and `./a.py` are the same snapshot target.
    fn normalize(path: &Path) -> PathBuf {
        path.components().collect()
    }

    fn slot_for(&self, path: &Path) -> PathBuf {
        // Mirror the source path's structure under `files/` instead of
        // flattening, so `a/b.py` and `a_b.py` can never collide. `..`
        // segments map to a fixed marker that real components escape away
        // from (any component starting with `_` gets one more `_`), keeping
        // the mapping injective and inside the snapshot dir.
        let mut slot = self.files_dir();
        for component in Self::normalize(path).components() {
            match component {
                Component::Prefix(_) | Component::RootDir | Component::CurDir => {}
                Component::ParentDir => slot.push("_parent"),
                Component::Normal(part) => {
                    let part = part.to_string_lossy();
                    if part.starts_with('_') {
                        slot.push(format!("_{part}"));
                    } else {
                        slot.push(part.as_ref());
                    }
                }
            }
        }
        slot
    }

    fn write_manifest(&self, captured: &[PathBuf]) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let body = serde_json::json!({ "captured": captured }).to_string();
        let tmp = self.dir.join("manifest.json.tmp");
        std::fs::write(&tmp, body)?;
        std::fs::rename(tmp, self.manifest_path())?;
        Ok(())
    }

    fn read_manifest(&self) -> std::io::Result<Vec<PathBuf>> {
        let body = match std::fs::read_to_string(self.manifest_path()) {
            Ok(body) => body,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let value: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(value
            .get("captured")
            .and_then(|c| c.as_array())
            .into_iter()
            .flatten()
            .filter_map(|p| p.as_str().map(PathBuf::from))
            .collect())
    }

    /// Copy each existing file into the snapshot (last-green capture).
    /// Missing files are skipped — a deleted file is not a snapshot target.
    /// The manifest is rewritten to exactly this call's captured set, so it
    /// always describes the most recent last-green state.
    pub(crate) fn snapshot_written(&mut self, paths: &[PathBuf]) -> std::io::Result<()> {
        std::fs::create_dir_all(self.files_dir())?;
        let mut captured = Vec::new();
        for path in paths {
            if path.is_file() {
                let slot = self.slot_for(path);
                if let Some(parent) = slot.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(path, &slot)?;
                captured.push(Self::normalize(path));
            }
        }
        self.write_manifest(&captured)?;
        self.taken = self.taken || !captured.is_empty();
        Ok(())
    }

    /// True when at least one file has been snapshotted.
    pub(crate) fn has_snapshot(&self) -> bool {
        self.taken
    }

    /// Drop any snapshotted state (fresh task). Only removes this instance's
    /// own directory — other in-process snapshots are untouched.
    pub(crate) fn clear(&mut self) {
        self.taken = false;
        if self.dir.exists() {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// Restore snapshotted files to their original paths, and remove files in
    /// `paths` that were created after the last-green capture (absent from
    /// the manifest). Paths outside `paths` — unrelated user work — are never
    /// touched. No-op when no snapshot was taken.
    pub(crate) fn restore_written(&self, paths: &[PathBuf]) -> std::io::Result<()> {
        if !self.taken {
            return Ok(());
        }
        let captured: std::collections::BTreeSet<PathBuf> =
            self.read_manifest()?.into_iter().collect();
        for path in &captured {
            let slot = self.slot_for(path);
            if slot.is_file() {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&slot, path)?;
            }
        }
        // Roll back post-capture creations: the caller's written set covers
        // every file the agent wrote this run, so anything in it that the
        // manifest proves absent at capture did not exist in the last-green
        // state and must not survive the restore.
        for path in paths {
            if !captured.contains(&Self::normalize(path)) && path.is_file() {
                std::fs::remove_file(path)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/best_snapshot_test.rs"]
mod best_snapshot_test;
