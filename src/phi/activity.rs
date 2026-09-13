//! Atomic, opt-in runtime receipts for the local Phi workspace.
//!
//! These receipts describe observed activity, not successful verification.
//! Each task has its own file so concurrent agents cannot replace one another.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io;
#[cfg(unix)]
use std::io::Write;
use std::path::{Path, PathBuf};

const MAX_CAPTURE_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityPhase {
    Running,
    Completed,
    Failed,
    Partial,
    Abandoned,
}

pub struct ActivityCapture {
    workspace_root: PathBuf,
    session_id: String,
    agent_id: String,
    #[cfg(unix)]
    root_directory: std::fs::File,
}

impl ActivityCapture {
    /// Called only when the existing diagnostic capture switch is enabled.
    /// A supervisor may group isolated agents under one explicitly owned root.
    pub fn from_environment(session_id: &str) -> io::Result<Self> {
        let root = match std::env::var_os("SELFWARE_PHI_WORKSPACE") {
            Some(root) => PathBuf::from(root),
            None => std::env::current_dir()?,
        };
        let agent_id =
            std::env::var("SELFWARE_PHI_AGENT_ID").unwrap_or_else(|_| session_id.to_string());
        Self::new(&root, session_id, &agent_id)
    }

    pub fn new(root: &Path, session_id: &str, agent_id: &str) -> io::Result<Self> {
        let workspace_root = root.canonicalize()?;
        if !workspace_root.is_dir() || session_id.is_empty() || agent_id.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid Phi capture identity",
            ));
        }
        if session_id.len() > 128 || agent_id.len() > 128 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Phi capture identity too long",
            ));
        }
        Ok(Self {
            #[cfg(unix)]
            root_directory: open_root(&workspace_root)?,
            workspace_root,
            session_id: session_id.into(),
            agent_id: agent_id.into(),
        })
    }

    pub fn write(
        &self,
        task_id: &str,
        phase: ActivityPhase,
        evidence: serde_json::Value,
        observations_truncated: bool,
    ) -> io::Result<()> {
        if task_id.is_empty() || task_id.len() > 128 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid Phi task identity",
            ));
        }
        let payload = serde_json::json!({
            "schema_version": 1,
            "agent_id": self.agent_id,
            "session_id": self.session_id,
            "task_id": task_id,
            "workspace_root": self.workspace_root,
            "recorded_at_ms": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64,
            "phase": phase,
            "observation_truncated": observations_truncated,
            "evidence": evidence,
        });
        let bytes = serde_json::to_vec(&payload)?;
        if bytes.len() > MAX_CAPTURE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Phi receipt exceeds size limit",
            ));
        }
        let digest = Sha256::digest(format!("{}\0{}", self.session_id, task_id).as_bytes());
        self.write_receipt(&format!("{digest:x}.json"), &bytes)
    }

    #[cfg(unix)]
    fn activity_directory(&self) -> io::Result<std::fs::File> {
        let mut directory = self.root_directory.try_clone()?;
        for component in [".selfware", "phi", "activity"] {
            directory = open_directory(&directory, component.as_ref(), true)?;
        }
        Ok(directory)
    }

    #[cfg(unix)]
    fn write_receipt(&self, destination: &str, bytes: &[u8]) -> io::Result<()> {
        let dir = self.activity_directory()?;
        write_atomic(&dir, destination, bytes)?;
        prune_retention(&self.workspace_root.join(".selfware/phi/activity"));
        Ok(())
    }

    #[cfg(not(unix))]
    fn write_receipt(&self, _destination: &str, _bytes: &[u8]) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "safe Phi activity capture unavailable on this platform",
        ))
    }
}

/// Traverse canonical root components without following replacement symlinks.
/// Keep the final handle so later root/ancestor renames cannot redirect writes.
#[cfg(unix)]
fn open_root(root: &Path) -> io::Result<std::fs::File> {
    let mut directory = std::fs::File::open("/")?;
    for component in root.components() {
        match component {
            std::path::Component::RootDir => (),
            std::path::Component::Normal(name) => {
                directory = open_directory(&directory, name, false)?;
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid Phi root",
                ))
            }
        }
    }
    Ok(directory)
}

#[cfg(unix)]
fn open_directory(
    parent: &std::fs::File,
    name: &std::ffi::OsStr,
    create: bool,
) -> io::Result<std::fs::File> {
    use nix::libc;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    let name = std::ffi::CString::new(name.as_bytes())?;
    if create {
        // SAFETY: parent owns a live directory fd and name is NUL terminated.
        if unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::AlreadyExists {
                return Err(error);
            }
        }
    }
    // SAFETY: no borrowed fd is consumed; a successful new fd is owned below.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: openat returned a new descriptor; File becomes its sole owner.
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn write_atomic(directory: &std::fs::File, destination: &str, bytes: &[u8]) -> io::Result<()> {
    use nix::libc;
    use std::os::fd::{AsRawFd, FromRawFd};
    let destination = std::ffi::CString::new(destination)?;
    let temporary = std::ffi::CString::new(format!(".tmp-{}", uuid::Uuid::new_v4()))?;
    // SAFETY: names are bounded, generated components; directory owns a live fd.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            temporary.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the newly created descriptor transfers once to File ownership.
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        // SAFETY: both names resolve against the same pinned directory. renameat
        // replaces a destination symlink itself, never its target.
        if unsafe {
            libc::renameat(
                directory.as_raw_fd(),
                temporary.as_ptr(),
                directory.as_raw_fd(),
                destination.as_ptr(),
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    })();
    if result.is_err() {
        // SAFETY: cleanup uses that same directory handle even if its old path
        // was replaced while the receipt was being written.
        unsafe {
            libc::unlinkat(directory.as_raw_fd(), temporary.as_ptr(), 0);
        }
    }
    result
}

#[cfg(unix)]
fn prune_retention(activity_path: &Path) {
    let Ok(entries) = std::fs::read_dir(activity_path) else {
        return;
    };
    let now_time = std::time::SystemTime::now();
    const RETENTION_SECS: u64 = 24 * 60 * 60;
    const MAX_SCAN: usize = 256;

    let mut json_files = Vec::new();

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();

        // Clean up orphaned temporary files older than 5 minutes
        if name_str.starts_with(".tmp-") {
            if let Ok(meta) = entry.metadata() {
                if let Ok(mtime) = meta.modified() {
                    if let Ok(age) = now_time.duration_since(mtime) {
                        if age.as_secs() > 300 {
                            let _ = std::fs::remove_file(entry.path());
                        }
                    }
                }
            }
            continue;
        }

        if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let mtime = entry.metadata().ok().and_then(|m| m.modified().ok());
        json_files.push((mtime, entry.path()));
    }

    // Two-pass: sort all json receipts descending by modified time
    json_files.sort_by_key(|b| std::cmp::Reverse(b.0));

    // Bounded receipt GC: prune excess oldest files past MAX_SCAN
    if json_files.len() > MAX_SCAN {
        for (_, old_file) in json_files.iter().skip(MAX_SCAN) {
            let _ = std::fs::remove_file(old_file);
        }
        json_files.truncate(MAX_SCAN);
    }

    // Retention policy: prune expired receipts older than 24 hours
    for (mtime, file_path) in json_files {
        if let Some(mt) = mtime {
            if let Ok(age) = now_time.duration_since(mt) {
                if age.as_secs() > RETENTION_SECS {
                    let _ = std::fs::remove_file(file_path);
                }
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn concurrent_agents_have_distinct_atomic_task_receipts() {
        let root = tempfile::tempdir().unwrap();
        std::thread::scope(|scope| {
            for index in 0..16 {
                let root = root.path();
                scope.spawn(move || {
                    let capture = ActivityCapture::new(
                        root,
                        &format!("session-{index}"),
                        &format!("agent-{index}"),
                    )
                    .unwrap();
                    capture
                        .write(
                            "task",
                            ActivityPhase::Running,
                            serde_json::json!({"outstanding": 1}),
                            false,
                        )
                        .unwrap();
                    capture
                        .write(
                            "task",
                            ActivityPhase::Partial,
                            serde_json::json!({"outstanding": 2}),
                            false,
                        )
                        .unwrap();
                });
            }
        });
        let entries: Vec<_> = std::fs::read_dir(root.path().join(".selfware/phi/activity"))
            .unwrap()
            .collect();
        assert_eq!(entries.len(), 16);
        for entry in entries {
            let value: serde_json::Value =
                serde_json::from_slice(&std::fs::read(entry.unwrap().path()).unwrap()).unwrap();
            assert_eq!(value["phase"], "partial");
            assert_eq!(value["evidence"]["outstanding"], 2);
        }
    }

    #[test]
    fn oversized_receipt_preserves_previous_capture() {
        let root = tempfile::tempdir().unwrap();
        let capture = ActivityCapture::new(root.path(), "session", "agent").unwrap();
        capture
            .write("task", ActivityPhase::Failed, serde_json::json!({}), false)
            .unwrap();
        assert!(capture
            .write(
                "task",
                ActivityPhase::Completed,
                serde_json::json!({"content": "x".repeat(MAX_CAPTURE_BYTES)}),
                false
            )
            .is_err());
        let entry = std::fs::read_dir(root.path().join(".selfware/phi/activity"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(entry.path()).unwrap()).unwrap();
        assert_eq!(value["phase"], "failed");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_capture_directory_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join(".selfware")).unwrap();
        let capture = ActivityCapture::new(root.path(), "session", "agent").unwrap();
        assert!(capture
            .write("task", ActivityPhase::Running, serde_json::json!({}), false)
            .is_err());
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    }

    #[test]
    fn replacing_root_after_capture_creation_cannot_redirect_writes() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("project");
        let moved = base.path().join("original-project");
        let outside = base.path().join("outside");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&outside).unwrap();
        let capture = ActivityCapture::new(&root, "session", "agent").unwrap();
        std::fs::rename(&root, &moved).unwrap();
        std::os::unix::fs::symlink(&outside, &root).unwrap();
        capture
            .write("task", ActivityPhase::Partial, serde_json::json!({}), false)
            .unwrap();
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
        let receipts: Vec<_> = std::fs::read_dir(moved.join(".selfware/phi/activity"))
            .unwrap()
            .collect();
        assert_eq!(receipts.len(), 1);
        let receipt: serde_json::Value =
            serde_json::from_slice(&std::fs::read(receipts[0].as_ref().unwrap().path()).unwrap())
                .unwrap();
        assert_eq!(receipt["phase"], "partial");
    }

    #[test]
    fn replacing_any_capture_ancestor_after_open_cannot_redirect_atomic_publish() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        for component in [".selfware", ".selfware/phi", ".selfware/phi/activity"] {
            let base = tempfile::tempdir().unwrap();
            let root = base.path().join("project");
            let outside = base.path().join("outside");
            std::fs::create_dir(&root).unwrap();
            std::fs::create_dir(&outside).unwrap();
            let capture = ActivityCapture::new(&root, "session", "agent").unwrap();
            let directory = capture.activity_directory().unwrap();
            // Pause at the old check/open race boundary, replace an ancestor,
            // then execute the real production atomic-write helper.
            let moved = base.path().join("original");
            std::fs::rename(root.join(component), &moved).unwrap();
            symlink(&outside, root.join(component)).unwrap();
            std::fs::write(outside.join("receipt.json"), b"protected").unwrap();
            write_atomic(&directory, "receipt.json", b"new capture").unwrap();
            assert_eq!(
                std::fs::read(outside.join("receipt.json")).unwrap(),
                b"protected"
            );
            assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 1);
            let suffix = match component {
                ".selfware" => "phi/activity",
                ".selfware/phi" => "activity",
                _ => "",
            };
            let actual = moved.join(suffix).join("receipt.json");
            assert_eq!(std::fs::read(&actual).unwrap(), b"new capture");
            assert_eq!(
                std::fs::metadata(actual).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(std::fs::read_dir(moved.join(suffix)).unwrap().count(), 1);
        }
    }

    #[test]
    fn destination_symlink_is_replaced_without_modifying_target() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(outside.path(), b"protected").unwrap();
        let capture = ActivityCapture::new(root.path(), "session", "agent").unwrap();
        let directory = capture.activity_directory().unwrap();
        let path = root.path().join(".selfware/phi/activity/receipt.json");
        std::os::unix::fs::symlink(outside.path(), &path).unwrap();
        write_atomic(&directory, "receipt.json", b"capture").unwrap();
        assert_eq!(std::fs::read(outside.path()).unwrap(), b"protected");
        assert!(!std::fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(std::fs::read(path).unwrap(), b"capture");
    }

    #[test]
    fn failed_publish_cleans_temporary_without_removing_existing_destination() {
        let root = tempfile::tempdir().unwrap();
        let capture = ActivityCapture::new(root.path(), "session", "agent").unwrap();
        let directory = capture.activity_directory().unwrap();
        let path = root.path().join(".selfware/phi/activity");
        std::fs::create_dir(path.join("receipt.json")).unwrap();
        assert!(write_atomic(&directory, "receipt.json", b"capture").is_err());
        assert!(path.join("receipt.json").is_dir());
        assert_eq!(std::fs::read_dir(path).unwrap().count(), 1);
    }

    #[test]
    fn write_prunes_excess_receipts_beyond_max_scan() {
        let root = tempfile::tempdir().unwrap();
        let act_dir = root.path().join(".selfware/phi/activity");
        std::fs::create_dir_all(&act_dir).unwrap();

        // Create 260 files directly in the directory
        for id in 0..260 {
            let p = act_dir.join(format!("dummy-{:04}.json", id));
            std::fs::write(&p, b"{}").unwrap();
        }
        assert_eq!(std::fs::read_dir(&act_dir).unwrap().count(), 260);

        // Writing a new receipt through ActivityCapture triggers write-time pruning
        let capture = ActivityCapture::new(root.path(), "session-gc", "agent-gc").unwrap();
        capture
            .write(
                "task-gc",
                ActivityPhase::Completed,
                serde_json::json!({"outstanding": 0}),
                false,
            )
            .unwrap();

        let count = std::fs::read_dir(&act_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
            .count();
        assert_eq!(count, 256);
    }
}

#[cfg(all(test, not(unix)))]
mod unsupported_platform_tests {
    #[test]
    fn capture_reports_unsupported_without_creating_activity_files() {
        let root = tempfile::tempdir().unwrap();
        let capture = super::ActivityCapture::new(root.path(), "session", "agent").unwrap();
        let error = capture
            .write(
                "task",
                super::ActivityPhase::Running,
                serde_json::json!({}),
                false,
            )
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
        assert!(!root.path().join(".selfware").exists());
    }
}
