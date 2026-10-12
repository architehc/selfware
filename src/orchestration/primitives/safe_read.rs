use std::path::{Component, Path};

#[cfg(unix)]
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::config::SafetyConfig;

/// Read one regular file below an already-canonical workspace root.
///
/// Artifact authority is deliberately narrower than the normal file tool:
/// every component below `root` must be a real directory or file, never a
/// symlink. On Unix the walk is descriptor-relative, so renaming or replacing
/// an earlier component cannot redirect a later open outside the pinned tree.
pub(super) async fn read_relative_regular(
    root: &Path,
    relative: &Path,
    safety: &SafetyConfig,
    limit: u64,
) -> Result<(String, Vec<u8>)> {
    let root = root.to_path_buf();
    let relative = relative.to_path_buf();
    let safety = safety.clone();
    crate::tools::workspace_root::spawn_blocking(move || {
        read_relative_regular_sync(&root, &relative, &safety, limit)
    })
    .await
    .context("artifact file read task panicked")?
}

fn relative_components(relative: &Path) -> Result<Vec<std::ffi::OsString>> {
    anyhow::ensure!(
        !relative.as_os_str().is_empty(),
        "artifact read path is empty"
    );
    let mut components = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(value) => components.push(value.to_os_string()),
            _ => anyhow::bail!(
                "artifact read path must contain only relative normal components: {}",
                relative.display()
            ),
        }
    }
    anyhow::ensure!(!components.is_empty(), "artifact read path is empty");
    Ok(components)
}

#[cfg(unix)]
fn open_root(root: &Path) -> Result<(std::fs::File, PathBuf)> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(root)
        .with_context(|| format!("cannot pin artifact harness root: {}", root.display()))?;
    let real = crate::safety::path_validator::fd_real_path(&file, root)
        .context("cannot resolve pinned artifact harness root")?;
    anyhow::ensure!(
        real == root,
        "artifact harness root changed while it was being opened"
    );
    Ok((file, real))
}

#[cfg(unix)]
fn openat_component(
    parent: &std::fs::File,
    name: &std::ffi::OsStr,
    directory: bool,
) -> std::io::Result<std::fs::File> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::io::{AsRawFd, FromRawFd};

    let name = std::ffi::CString::new(name.as_bytes())?;
    let flags = libc::O_RDONLY
        | libc::O_NOFOLLOW
        | libc::O_CLOEXEC
        | if directory {
            libc::O_DIRECTORY
        } else {
            libc::O_NONBLOCK
        };
    // SAFETY: `parent` owns a live directory descriptor, `name` is one
    // NUL-terminated component, and a successful descriptor is owned once by
    // the returned File.
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn refuse_crossed_device(file: &std::fs::File, root_dev: u64, relative: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    // Mount points are not symlinks, so O_NOFOLLOW still crosses onto another st_dev.
    let dev = file
        .metadata()
        .context("cannot stat pinned artifact descriptor")?
        .dev();
    anyhow::ensure!(
        dev == root_dev,
        "artifact read refused {}: the artifact crossed onto another device / mount",
        relative.display()
    );
    Ok(())
}

#[cfg(unix)]
fn read_relative_regular_sync(
    root: &Path,
    relative: &Path,
    safety: &SafetyConfig,
    limit: u64,
) -> Result<(String, Vec<u8>)> {
    use std::io::Read;
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::io::AsRawFd;

    let components = relative_components(relative)?;
    let lexical = root.join(relative);
    crate::tools::file::validate_tool_path(&lexical.to_string_lossy(), safety)
        .context("artifact read path is outside path policy")?;

    let (mut directory, pinned_root) = open_root(root)?;
    let root_dev = directory
        .metadata()
        .context("cannot stat pinned artifact harness root")?
        .dev();
    for component in &components[..components.len() - 1] {
        directory = openat_component(&directory, component, true).with_context(|| {
            format!(
                "artifact read refused directory component in {}",
                relative.display()
            )
        })?;
        refuse_crossed_device(&directory, root_dev, relative)?;
    }
    let name = components
        .last()
        .expect("relative_components always returns a non-empty vector");
    let mut file = openat_component(&directory, name, false)
        .with_context(|| format!("artifact read refused {}", relative.display()))?;
    refuse_crossed_device(&file, root_dev, relative)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(anyhow::anyhow!(
            crate::safety::path_validator::not_regular_error(&lexical, &metadata)
        ));
    }
    anyhow::ensure!(
        metadata.nlink() == 1,
        "artifact read refuses files with multiple hard links"
    );

    // O_NONBLOCK keeps an unexpected special file from hanging the open. The
    // descriptor is regular now, so restore its ordinary blocking mode.
    // SAFETY: F_GETFL/F_SETFL operate on the live descriptor owned by `file`.
    unsafe {
        let fd = file.as_raw_fd();
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags != -1 {
            libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
        }
    }

    let real = crate::safety::path_validator::fd_real_path(&file, &lexical)
        .context("cannot resolve pinned artifact file")?;
    anyhow::ensure!(
        real.starts_with(&pinned_root) && real != pinned_root,
        "artifact file resolved outside the pinned harness root"
    );
    crate::tools::file::validate_tool_path(&real.to_string_lossy(), safety)
        .context("opened artifact file is outside path policy")?;

    anyhow::ensure!(
        metadata.len() <= limit,
        "artifact file is too large: {} bytes exceeds {limit}",
        metadata.len()
    );
    let mut bytes = Vec::with_capacity(metadata.len().min(limit) as usize);
    file.by_ref()
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 <= limit,
        "artifact file grew beyond the {limit}-byte limit while being read"
    );
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Ok((text, bytes))
}

#[cfg(not(unix))]
fn read_relative_regular_sync(
    _root: &Path,
    relative: &Path,
    _safety: &SafetyConfig,
    _limit: u64,
) -> Result<(String, Vec<u8>)> {
    relative_components(relative)?;
    anyhow::bail!(
        "artifact harness secure component-relative reads are unavailable on this platform"
    )
}

#[cfg(test)]
#[path = "../../../tests/unit/orchestration/primitives/safe_read_test.rs"]
mod tests;
