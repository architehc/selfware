//! Harness-owned scratch under `<workspace>/.selfware/`.
//!
//! The checkout is untrusted: a repository can ship a symlink at
//! `.selfware/tool_results`, `.selfware/.gitignore` or
//! `.selfware/turns/evidence.jsonl`, and `git clone` recreates it. A
//! path-based `create_dir_all` / `write` / `append` then lands wherever the
//! link points (`~/.gitconfig`, `~/.selfware/KILLSWITCH`) with no tool call
//! and no approval involved — harness writes never pass the path validator.
//!
//! So every harness write below `.selfware/` goes through a pinned directory
//! descriptor: the workspace root is opened once, each component below it is
//! created with `mkdirat` and entered with `openat(O_DIRECTORY|O_NOFOLLOW)`,
//! and the leaf is opened `O_NOFOLLOW` relative to that descriptor. A symlink
//! at any level is an error, not a destination — the treatment
//! `crate::phi::activity` gives its receipts. Only the workspace root itself
//! may be a symlink (macOS `/tmp -> /private/tmp`). Non-Unix platforms keep
//! path-based I/O but refuse symlinked components the same way.

use std::io;
use std::path::{Path, PathBuf};

/// The harness's project-local scratch directory, relative to the workspace.
pub(crate) const SELFWARE_DIR: &str = ".selfware";

/// A pinned directory at or below `<workdir>/.selfware/`.
#[derive(Debug)]
pub(crate) struct ScratchDir {
    #[cfg(unix)]
    dir: std::fs::File,
    /// Where the directory was opened, for messages (and for the non-Unix
    /// path-based fallback).
    path: PathBuf,
}

impl ScratchDir {
    /// Open `<workdir>/.selfware/<components...>`. With `create`, missing
    /// directories are made on the way; without it a missing one is
    /// `NotFound`. A symlink at `.selfware` or below is refused.
    pub(crate) fn open(workdir: &Path, components: &[&str], create: bool) -> io::Result<Self> {
        let mut path = workdir.join(SELFWARE_DIR);
        for component in components {
            single_component(component)?;
            path.push(component);
        }
        #[cfg(unix)]
        {
            let mut dir = open_workdir(workdir)?;
            dir = open_child_dir(&dir, SELFWARE_DIR, create)?;
            for component in components {
                dir = open_child_dir(&dir, component, create)?;
            }
            Ok(Self { dir, path })
        }
        #[cfg(not(unix))]
        {
            let mut cur = workdir.to_path_buf();
            for component in std::iter::once(&SELFWARE_DIR).chain(components) {
                cur.push(component);
                match std::fs::symlink_metadata(&cur) {
                    Ok(meta) if meta.is_dir() => {}
                    Ok(_) => return Err(not_a_directory(&cur)),
                    Err(e) if create && e.kind() == io::ErrorKind::NotFound => {
                        std::fs::create_dir(&cur)?;
                    }
                    Err(e) => return Err(e),
                }
            }
            Ok(Self { path })
        }
    }

    /// The path the directory was opened at.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Whether `name` exists in this directory. A dangling symlink counts as
    /// present: it is a slot the harness must not write into.
    pub(crate) fn exists(&self, name: &str) -> bool {
        if single_component(name).is_err() {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let Ok(c_name) = std::ffi::CString::new(name) else {
                return false;
            };
            // SAFETY: zeroed stat buffer of the right size, NUL-terminated
            // single component, live directory descriptor.
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            let rc = unsafe {
                libc::fstatat(
                    self.dir.as_raw_fd(),
                    c_name.as_ptr(),
                    &mut st,
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            };
            rc == 0
        }
        #[cfg(not(unix))]
        {
            std::fs::symlink_metadata(self.path.join(name)).is_ok()
        }
    }

    /// Create or truncate `name` and write `bytes` to it.
    pub(crate) fn write(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
        use std::io::Write;
        let mut file = self.open_leaf(name, OpenMode::Truncate)?;
        file.write_all(bytes)
    }

    /// Create `name` with `bytes`. Whatever is already there — a file, a
    /// symlink, anything — is left alone; `Ok(false)` says so.
    pub(crate) fn write_new(&self, name: &str, bytes: &[u8]) -> io::Result<bool> {
        use std::io::Write;
        let mut file = match self.open_leaf(name, OpenMode::CreateNew) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Ok(false),
            Err(e) => return Err(e),
        };
        file.write_all(bytes)?;
        Ok(true)
    }

    /// Append `bytes` to `name`, creating it when missing.
    pub(crate) fn append(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
        use std::io::Write;
        let mut file = self.open_leaf(name, OpenMode::Append)?;
        file.write_all(bytes)
    }

    /// Open `name` for writing relative to the pinned directory, never
    /// through a symlink, and only if it is (or becomes) a regular file.
    #[cfg(unix)]
    fn open_leaf(&self, name: &str, mode: OpenMode) -> io::Result<std::fs::File> {
        use std::os::unix::io::{AsRawFd, FromRawFd};
        single_component(name)?;
        let c_name = std::ffi::CString::new(name)?;
        let mode_flags = match mode {
            OpenMode::Truncate => libc::O_CREAT | libc::O_TRUNC,
            OpenMode::CreateNew => libc::O_CREAT | libc::O_EXCL,
            OpenMode::Append => libc::O_CREAT | libc::O_APPEND,
        };
        // O_NONBLOCK: never hang on a FIFO planted at the leaf.
        let flags =
            libc::O_WRONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC | mode_flags;
        // SAFETY: NUL-terminated single component, live directory descriptor.
        let fd = unsafe {
            libc::openat(
                self.dir.as_raw_fd(),
                c_name.as_ptr(),
                flags,
                0o644 as libc::c_uint,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the new descriptor is owned exactly once by the File.
        let file = unsafe { std::fs::File::from_raw_fd(fd) };
        if !file.metadata()?.is_file() {
            return Err(not_a_file(&self.path.join(name)));
        }
        Ok(file)
    }

    #[cfg(not(unix))]
    fn open_leaf(&self, name: &str, mode: OpenMode) -> io::Result<std::fs::File> {
        single_component(name)?;
        let path = self.path.join(name);
        if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(not_a_file(&path));
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true);
        match mode {
            OpenMode::Truncate => options.create(true).truncate(true),
            OpenMode::CreateNew => options.create_new(true),
            OpenMode::Append => options.create(true).append(true),
        };
        let file = options.open(&path)?;
        if !file.metadata()?.is_file() {
            return Err(not_a_file(&path));
        }
        Ok(file)
    }
}

/// How a leaf is opened for writing; the file is created when missing.
#[derive(Clone, Copy)]
enum OpenMode {
    Truncate,
    CreateNew,
    Append,
}

/// Every name the harness resolves here is one path component: no
/// separators, no `.`/`..`, no NUL.
fn single_component(name: &str) -> io::Result<()> {
    let bad = name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', '\0'])
        || (cfg!(windows) && name.contains(':'));
    if bad {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("scratch path component {name:?} is not a single name"),
        ));
    }
    Ok(())
}

fn not_a_file(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{} is not a regular file", path.display()),
    )
}

#[cfg(not(unix))]
fn not_a_directory(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{} is not a directory", path.display()),
    )
}

/// Open the workspace root as a directory descriptor. This one follows
/// symlinks: the root is the operator's, and legitimately symlinked
/// workspaces exist.
#[cfg(unix)]
fn open_workdir(workdir: &Path) -> io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(workdir)
}

/// Descend one component relative to a pinned directory (`mkdirat` first when
/// `create`), refusing a symlink: `openat(O_DIRECTORY|O_NOFOLLOW)`.
#[cfg(unix)]
fn open_child_dir(parent: &std::fs::File, name: &str, create: bool) -> io::Result<std::fs::File> {
    use std::os::unix::io::{AsRawFd, FromRawFd};
    let c_name = std::ffi::CString::new(name)?;
    if create {
        // SAFETY: NUL-terminated single component, live directory descriptor.
        if unsafe { libc::mkdirat(parent.as_raw_fd(), c_name.as_ptr(), 0o777) } != 0 {
            let err = io::Error::last_os_error();
            if err.kind() != io::ErrorKind::AlreadyExists {
                return Err(err);
            }
        }
    }
    // SAFETY: `c_name` is a valid NUL-terminated single component and
    // `parent` owns a live descriptor for the duration of the call.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            c_name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the new descriptor is owned exactly once by the File.
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

#[cfg(test)]
#[path = "../../tests/unit/agent/scratch_dir_test.rs"]
mod tests;
