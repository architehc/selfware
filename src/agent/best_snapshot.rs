//! Restore verified state from explicit preimages, never inferred absence.
//!
//! Dispatch records files before mutation and observes them afterwards,
//! including partial failures. A passing verifier advances the baseline.
//! Recovery only touches known paths still matching the agent's last write.

use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Clone, PartialEq, Eq)]
enum FileState {
    Missing,
    Present([u8; 32]),
}

struct SnapshotEntry {
    // None is an explicitly observed absence, not missing coverage.
    saved: Option<PathBuf>,
    observed: Option<FileState>,
}

pub(crate) struct AgentSnapshot {
    dir: PathBuf,
    taken: bool,
    entries: BTreeMap<PathBuf, SnapshotEntry>,
}

impl Default for AgentSnapshot {
    fn default() -> Self {
        Self {
            dir: std::env::temp_dir().join(format!(
                "selfware-snapshot-{}-{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            )),
            taken: false,
            entries: BTreeMap::new(),
        }
    }
}

impl AgentSnapshot {
    /// Resolve existing ancestors too, giving missing files stable identities
    /// before creation, after deletion, and through relative aliases.
    pub(crate) fn identity(path: &Path) -> std::io::Result<PathBuf> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        match std::fs::canonicalize(&absolute) {
            Ok(path) => Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let parent = absolute.parent().ok_or(e)?;
                let name = absolute.file_name().ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid snapshot path")
                })?;
                Ok(Self::identity(parent)?.join(name))
            }
            Err(e) => Err(e),
        }
    }

    fn read_file(path: &Path) -> std::io::Result<Option<(Vec<u8>, std::fs::Permissions)>> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_file() => {
                Ok(Some((std::fs::read(path)?, metadata.permissions())))
            }
            Ok(_) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("snapshot target is not a regular file: {}", path.display()),
            )),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn state(path: &Path) -> std::io::Result<FileState> {
        Ok(match Self::read_file(path)? {
            Some((bytes, _)) => FileState::Present(Sha256::digest(&bytes).into()),
            None => FileState::Missing,
        })
    }

    fn capture(dir: &Path, path: &Path) -> std::io::Result<SnapshotEntry> {
        match Self::read_file(path)? {
            Some((bytes, permissions)) => {
                std::fs::create_dir_all(dir)?;
                let slot = dir.join(uuid::Uuid::new_v4().to_string());
                std::fs::write(&slot, &bytes)?;
                std::fs::set_permissions(&slot, permissions)?;
                Ok(SnapshotEntry {
                    saved: Some(slot),
                    observed: Some(FileState::Present(Sha256::digest(&bytes).into())),
                })
            }
            None => Ok(SnapshotEntry {
                saved: None,
                observed: Some(FileState::Missing),
            }),
        }
    }

    pub(crate) fn before_mutation(&mut self, paths: &[PathBuf]) -> std::io::Result<()> {
        let identities = paths
            .iter()
            .map(|path| Self::identity(path))
            .collect::<std::io::Result<BTreeSet<_>>>()?;
        // Validate the entire call before invalidating observations. Repeated
        // aliases in a multi-edit are one identity, and a later failed preflight
        // must not destroy recovery coverage for earlier targets.
        for path in &identities {
            if let Some(entry) = self.entries.get(path) {
                if entry.observed.as_ref() != Some(&Self::state(path)?) {
                    return Err(std::io::Error::other(format!(
                        "refusing to overwrite externally changed snapshot target: {}",
                        path.display()
                    )));
                }
            } else {
                let entry = Self::capture(&self.dir, path)?;
                self.entries.insert(path.clone(), entry);
            }
        }
        for path in identities {
            // An interrupted call must not leave an older observation usable.
            self.entries.get_mut(&path).expect("just captured").observed = None;
        }
        Ok(())
    }

    pub(crate) fn after_mutation(&mut self, paths: &[PathBuf]) -> std::io::Result<()> {
        let mut errors = Vec::new();
        for path in paths {
            let result = (|| -> std::io::Result<()> {
                let path = Self::identity(path)?;
                if let Some(entry) = self.entries.get_mut(&path) {
                    entry.observed = None;
                    entry.observed = Some(Self::state(&path)?);
                }
                Ok(())
            })();
            if let Err(error) = result {
                errors.push(format!("{}: {error}", path.display()));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(std::io::Error::other(errors.join("; ")))
        }
    }

    /// Finish a new generation before replacing the last good one.
    pub(crate) fn snapshot_written(&mut self, paths: &[PathBuf]) -> std::io::Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let generation = self.dir.join(uuid::Uuid::new_v4().to_string());
        let mut entries = BTreeMap::new();
        let result = (|| {
            for path in paths {
                let path = Self::identity(path)?;
                entries.insert(path.clone(), Self::capture(&generation, &path)?);
            }
            Ok::<_, std::io::Error>(())
        })();
        if let Err(e) = result {
            let _ = std::fs::remove_dir_all(&generation);
            return Err(e);
        }
        for entry in self.entries.values() {
            if let Some(slot) = &entry.saved {
                let _ = std::fs::remove_file(slot);
            }
        }
        self.entries = entries;
        self.taken = true;
        Ok(())
    }

    pub(crate) fn has_snapshot(&self) -> bool {
        self.taken
    }

    /// Shell, formatter, and git tools can modify previously tracked files
    /// without naming each target. Observe those identities around the call;
    /// new unknown targets still carry no implicit rollback authority.
    pub(crate) fn tracked_paths(&self) -> Vec<PathBuf> {
        self.entries.keys().cloned().collect()
    }

    pub(crate) fn clear(&mut self) {
        self.taken = false;
        self.entries.clear();
        let _ = std::fs::remove_dir_all(&self.dir);
    }

    pub(crate) fn restore_written(&mut self, paths: &[PathBuf]) -> std::io::Result<()> {
        if !self.taken {
            return Ok(());
        }
        let mut identities = BTreeSet::new();
        let mut errors = Vec::new();
        for path in paths {
            match Self::identity(path) {
                Ok(path) => {
                    identities.insert(path);
                }
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            }
        }
        for path in identities {
            let Some(entry) = self.entries.get_mut(&path) else {
                // No preimage means no authority to remove or overwrite it.
                continue;
            };
            let restore = || -> std::io::Result<FileState> {
                if entry.observed.as_ref() != Some(&Self::state(&path)?) {
                    return Err(std::io::Error::other(
                        "file changed after the agent's last observation; left untouched",
                    ));
                }
                if let Some(slot) = &entry.saved {
                    let restored = Self::state(slot)?;
                    let parent = path
                        .parent()
                        .ok_or_else(|| std::io::Error::other("snapshot path has no parent"))?;
                    std::fs::create_dir_all(parent)?;
                    let temporary =
                        parent.join(format!(".selfware-restore-{}", uuid::Uuid::new_v4()));
                    let result = std::fs::copy(slot, &temporary)
                        .and_then(|_| std::fs::rename(&temporary, &path));
                    if result.is_err() {
                        let _ = std::fs::remove_file(temporary);
                    }
                    result?;
                    Ok(restored)
                } else {
                    if path.exists() {
                        std::fs::remove_file(&path)?;
                    }
                    Ok(FileState::Missing)
                }
            };
            match restore() {
                Ok(restored) => entry.observed = Some(restored),
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(std::io::Error::other(errors.join("; ")))
        }
    }
}

impl Drop for AgentSnapshot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/best_snapshot_test.rs"]
mod best_snapshot_test;
