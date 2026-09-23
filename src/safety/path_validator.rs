//! Shared path validation logic for safety checks.

use crate::config::SafetyConfig;
use crate::errors::{Result, SafetyError, SelfwareError};
use crate::safety::checker::{normalize_path, to_glob_form};
use std::path::{Path, PathBuf};

/// ELOOP errno value (symlink encountered with O_NOFOLLOW).
#[cfg(target_os = "linux")]
const ELOOP: i32 = 40;
#[cfg(target_os = "macos")]
const ELOOP: i32 = 62;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
const ELOOP: i32 = -1;

/// O_NOFOLLOW flag value for OpenOptions::custom_flags.
#[cfg(target_os = "linux")]
const O_NOFOLLOW: i32 = 0o0400000;
#[cfg(target_os = "macos")]
const O_NOFOLLOW: i32 = 0x0100;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
const O_NOFOLLOW: i32 = 0;

/// Atomically open a path with O_NOFOLLOW to prevent TOCTOU symlink races.
/// Returns the real path of the opened file descriptor.
///
/// The open is `O_NONBLOCK`: validation must never hang on a FIFO (an
/// `O_RDONLY` open of a FIFO with no writer blocks forever). The descriptor
/// is only used to resolve the real path and is dropped immediately; the
/// I/O layer re-opens through [`PathValidator::open_regular_file`] /
/// [`open_parent_dir_fd`] and rejects non-regular files there.
#[cfg(unix)]
fn open_nofollow_and_resolve(path: &Path) -> std::io::Result<PathBuf> {
    use std::os::unix::fs::OpenOptionsExt;

    let fd = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)?;
    fd_real_path(&fd, path)
}

#[cfg(not(unix))]
fn open_nofollow_and_resolve(path: &Path) -> std::io::Result<PathBuf> {
    path.canonicalize()
}

/// Resolve the real (symlink-free) path of an open descriptor.
///
/// Linux: `/proc/self/fd/N`; macOS: `F_GETPATH`. Both describe the object the
/// descriptor actually refers to, so a path swapped after the open cannot
/// change the answer. Other Unix platforms without either fall back to
/// canonicalizing `fallback` (the path that was opened).
#[cfg(unix)]
pub(crate) fn fd_real_path(file: &std::fs::File, fallback: &Path) -> std::io::Result<PathBuf> {
    use std::os::unix::io::AsRawFd;
    let raw_fd = file.as_raw_fd();

    // Linux: resolve via /proc/self/fd which is atomic
    let fd_path = format!("/proc/self/fd/{}", raw_fd);
    let proc_path = Path::new(&fd_path);
    if proc_path.exists() {
        return std::fs::read_link(proc_path);
    }

    // macOS: use F_GETPATH to resolve fd to path atomically (no TOCTOU)
    #[cfg(target_os = "macos")]
    {
        let mut buf = vec![0u8; libc::PATH_MAX as usize];
        // SAFETY: F_GETPATH writes at most MAXPATHLEN (== PATH_MAX) bytes
        // into `buf`, which is exactly that long; `raw_fd` is live for the
        // borrow of `file`.
        let ret = unsafe { libc::fcntl(raw_fd, libc::F_GETPATH, buf.as_mut_ptr()) };
        if ret != -1 {
            let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            buf.truncate(len);
            use std::os::unix::ffi::OsStringExt;
            return Ok(PathBuf::from(std::ffi::OsString::from_vec(buf)));
        }
    }

    // Final fallback for other Unix platforms without /proc
    fallback.canonicalize()
}

/// Refusal for a path that exists but is not a regular file (FIFO, socket,
/// device, directory). Reading a FIFO blocks forever; writing through a
/// device node is never what a file tool means.
pub(crate) fn not_regular_error(path: &Path, meta: &std::fs::Metadata) -> SelfwareError {
    let ft = meta.file_type();
    let kind = if ft.is_dir() {
        "a directory"
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileTypeExt;
            if ft.is_fifo() {
                "a FIFO (named pipe)"
            } else if ft.is_socket() {
                "a socket"
            } else if ft.is_block_device() || ft.is_char_device() {
                "a device node"
            } else {
                "not a regular file"
            }
        }
        #[cfg(not(unix))]
        {
            "not a regular file"
        }
    };
    SelfwareError::Safety(SafetyError::BlockedPath {
        path: format!(
            "{} is {} — file tools only read and write regular files",
            path.display(),
            kind
        ),
    })
}

#[cfg(test)]
thread_local! {
    /// Test hook fired between lexical validation and the descriptor open,
    /// i.e. inside the race window the descriptor re-check exists to close.
    static AFTER_LEXICAL_VALIDATE: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        std::cell::RefCell::new(None);
}

#[cfg(test)]
pub(crate) fn set_after_lexical_validate_hook(hook: Box<dyn FnOnce()>) {
    AFTER_LEXICAL_VALIDATE.with(|h| *h.borrow_mut() = Some(hook));
}

#[cfg(test)]
fn run_after_lexical_validate_hook() {
    if let Some(hook) = AFTER_LEXICAL_VALIDATE.with(|h| h.borrow_mut().take()) {
        hook();
    }
}

/// A regular file opened for reading whose descriptor's real path passed
/// the same policy check as the lexical path. Read from `file`, never by
/// re-opening `real_path`: the path is informational only.
#[derive(Debug)]
pub struct ValidatedFile {
    pub file: std::fs::File,
    pub real_path: PathBuf,
}

/// Open `dir` as a directory descriptor (following symlinks; the caller
/// validates the descriptor's real path).
#[cfg(unix)]
fn open_dir(dir: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(dir)
}

/// `openat(dirfd, name, O_DIRECTORY|O_NOFOLLOW)`: descend exactly one
/// component relative to a pinned directory, refusing a symlink.
#[cfg(unix)]
fn openat_dir_nofollow(
    dir: &std::fs::File,
    name: &std::ffi::OsStr,
) -> std::io::Result<std::fs::File> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::io::{AsRawFd, FromRawFd};
    let c_name = std::ffi::CString::new(name.as_bytes())?;
    // SAFETY: `c_name` is a valid NUL-terminated single component and `dir`
    // owns a live descriptor for the duration of the call.
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            c_name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: the new descriptor is owned exactly once by the File.
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

/// Open (optionally creating) the parent directory of `target` as a pinned
/// descriptor for fd-relative I/O, and prove it is still an allowed location.
///
/// * The deepest existing ancestor is opened and its real path — together
///   with the not-yet-existing tail — is passed to `check` BEFORE any
///   directory is created, so a swapped ancestor never gets directories
///   created under it.
/// * Missing components are created with `mkdirat` and entered with
///   `openat(O_DIRECTORY|O_NOFOLLOW)` relative to the previous descriptor,
///   so a symlink planted mid-walk is refused rather than followed.
/// * The final directory's real path (from the descriptor, not the path)
///   joined with the file name is passed to `check` again.
///
/// This is the portable equivalent of Linux `openat2(RESOLVE_BENEATH)` /
/// macOS `O_NOFOLLOW_ANY`: instead of forbidding every symlink on the way
/// (which would break legitimately symlinked workspaces such as macOS
/// `/tmp -> /private/tmp`), it validates where the pinned descriptor really
/// is. Returns the directory descriptor and the target's final component;
/// every later `openat`/`renameat`/`unlinkat` against this descriptor lands
/// in the checked directory even if the path is swapped afterwards.
#[cfg(unix)]
pub(crate) fn open_parent_dir_fd(
    target: &Path,
    create: bool,
    check: &dyn Fn(&Path) -> Result<()>,
) -> Result<(std::fs::File, std::ffi::OsString)> {
    use std::path::Component;
    let name = match target.components().next_back() {
        Some(Component::Normal(n)) => n.to_os_string(),
        _ => {
            return Err(SelfwareError::Safety(
                SafetyError::PathCanonicalizationFailed {
                    path: target.display().to_string(),
                },
            ))
        }
    };
    let parent = match target.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };

    // Find the deepest existing ancestor; collect the missing tail.
    const MAX_MISSING: usize = 128;
    let mut missing: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = parent.clone();
    let anchor = loop {
        match open_dir(&cur) {
            Ok(f) => break f,
            Err(e) if create && e.kind() == std::io::ErrorKind::NotFound => {
                if missing.len() >= MAX_MISSING {
                    return Err(e.into());
                }
                match cur.components().next_back() {
                    Some(Component::Normal(n)) => missing.push(n.to_os_string()),
                    // `..`, `.` or a root in the missing tail cannot be
                    // created component-wise; refuse rather than guess.
                    _ => return Err(e.into()),
                }
                cur = match cur.parent() {
                    Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
                    _ => PathBuf::from("."),
                };
            }
            Err(e) => return Err(e.into()),
        }
    };
    missing.reverse();

    let anchor_real = fd_real_path(&anchor, &cur)?;
    let mut prospective = anchor_real;
    for comp in &missing {
        prospective.push(comp);
    }
    prospective.push(&name);
    check(&prospective)?;

    let mut dir = anchor;
    for comp in &missing {
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::io::AsRawFd;
        let c_comp = std::ffi::CString::new(comp.as_bytes()).map_err(std::io::Error::from)?;
        // SAFETY: single NUL-terminated component, live directory descriptor.
        let rc = unsafe { libc::mkdirat(dir.as_raw_fd(), c_comp.as_ptr(), 0o777) };
        if rc != 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(err.into());
            }
        }
        dir = openat_dir_nofollow(&dir, comp)?;
    }

    let real_dir = fd_real_path(&dir, &parent)?;
    check(&real_dir.join(&name))?;
    Ok((dir, name))
}

/// Canonicalize a path and fail closed if the filesystem cannot resolve it.
fn canonicalize_or_fail(path: &Path) -> Result<PathBuf> {
    path.canonicalize().map_err(|_| {
        SelfwareError::Safety(SafetyError::PathCanonicalizationFailed {
            path: path.display().to_string(),
        })
    })
}

/// Resolve a path that does not yet exist by canonicalizing its deepest
/// existing ancestor and appending the missing components. This allows safe
/// validation of paths that will be created by tools like `file_write`.
///
/// Lexically collapse `.`/`..` components. `Path::components()` keeps
/// `ParentDir` entries and `resolve_missing_path` joins a lexical suffix onto
/// a canonical ancestor, so without this a path containing `..` escapes the
/// workspace while still string-matching the working-dir prefix.
fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::CurDir => {}
            // pop() at the root returns false — `..` above root clamps at /.
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// One round of percent-decoding. `%` not followed by two hex digits is kept
/// literally (legit filenames like `100%.txt`). Escapes decoding to invalid
/// UTF-8 (overlong forms like `%c0%af`, a classic `/` smuggle) are rejected.
fn percent_decode_once(input: &str) -> Result<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let mut changed = false;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                changed = true;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    if !changed {
        return Ok(input.to_string());
    }
    String::from_utf8(out).map_err(|_| {
        SelfwareError::Safety(SafetyError::PathInvalidEncoding {
            reason: "percent-escapes decode to invalid UTF-8 (overlong encoding?)".to_string(),
        })
    })
}

/// Encoding-evasion normalization, run BEFORE any other path check:
/// iterative percent-decode (cap 4 rounds so `%252f` → `%2f` → `/`),
/// backslash → slash, and rejection of all-dot components longer than two
/// (`....//` collapses to `..//` on PHP/Windows parsers). The validator then
/// sees the decoded form, so encoded traversal cannot bypass literal checks.
fn decode_path_escapes(path: &str) -> Result<String> {
    let mut current = path.to_string();
    for _ in 0..4 {
        let next = percent_decode_once(&current)?;
        if next == current {
            break;
        }
        current = next;
    }
    let normalized = current.replace('\\', "/");
    for component in normalized.split('/') {
        if component.len() > 2 && component.bytes().all(|b| b == b'.') {
            return Err(SelfwareError::Safety(SafetyError::PathInvalidEncoding {
                reason: format!("dot-overflow component '{component}'"),
            }));
        }
    }
    Ok(normalized)
}

fn resolve_missing_path(path: &Path) -> Result<PathBuf> {
    // Bound only unresolved ancestry work. Existing deep paths still use the
    // direct descriptor resolution; a missing thousand-component path must not
    // cause a platform-dependent unbounded sequence of filesystem probes.
    const MAX_UNRESOLVED_ANCESTORS: usize = 128;
    for ancestor in path.ancestors().take(MAX_UNRESOLVED_ANCESTORS) {
        match open_nofollow_and_resolve(ancestor) {
            Ok(real) => {
                let suffix = path
                    .strip_prefix(ancestor)
                    .map_err(|_| {
                        SelfwareError::Safety(SafetyError::PathCanonicalizationFailed {
                            path: path.display().to_string(),
                        })
                    })?
                    .iter()
                    .collect::<PathBuf>();
                // The suffix is joined LEXICALLY — `..` components in it are
                // never collapsed by the filesystem open above (only the
                // existing ancestor was opened). Without normalization,
                // `node_modules/.bin/../../../.aws/credentials` keeps its
                // `..`, passes the starts-with/glob allowed checks against
                // the working-dir prefix, and escapes the workspace.
                return Ok(normalize_lexical(&real.join(suffix)));
            }
            Err(e) if e.raw_os_error() == Some(ELOOP) => {
                // Symlink in the existing portion — let the caller resolve it
                // with the dedicated symlink safety check.
                return Err(SelfwareError::Safety(
                    SafetyError::PathCanonicalizationFailed {
                        path: path.display().to_string(),
                    },
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Keep walking up until we find an existing ancestor.
                continue;
            }
            Err(_) => {
                return Err(SelfwareError::Safety(
                    SafetyError::PathCanonicalizationFailed {
                        path: path.display().to_string(),
                    },
                ))
            }
        }
    }
    Err(SelfwareError::Safety(
        SafetyError::PathCanonicalizationFailed {
            path: path.display().to_string(),
        },
    ))
}

#[derive(Clone)]
pub struct PathValidator {
    config: SafetyConfig,
    working_dir: PathBuf,
}

impl PathValidator {
    pub fn new(config: &SafetyConfig, working_dir: PathBuf) -> Self {
        Self {
            config: config.clone(),
            working_dir,
        }
    }

    /// Canonicalize and check a file path for safety.
    pub fn validate(&self, path: &str) -> Result<()> {
        // SECURITY: Reject null bytes early — they can truncate paths at the OS/C-library
        // boundary, allowing an attacker to bypass later validation checks.
        // Example: "/safe/path\0/../../../etc/passwd" could be interpreted as just
        // "/safe/path" by some APIs but open "/etc/passwd" when passed to C functions.
        if path.contains('\0') {
            return Err(SelfwareError::Safety(SafetyError::PathNullBytes));
        }

        // SECURITY: encoding-evasion normalization (see decode_path_escapes).
        // `%2e%2e%2f`, double-encoded `%252f`, backslash separators, `....//`
        // dot-overflow, and overlong UTF-8 (`%c0%af`) execute as traversal on
        // some parsers while matching no literal `..` check. Re-check nulls
        // afterwards: `%00` decodes to a real NUL.
        let decoded_path = decode_path_escapes(path)?;
        if decoded_path.contains('\0') {
            return Err(SelfwareError::Safety(SafetyError::PathNullBytes));
        }
        let path = decoded_path.as_str();

        // SECURITY: Unicode normalization bypass prevention.
        // Reject paths with characters that look like ASCII but are not.
        // These "homoglyph" characters could fool visual inspection and bypass
        // filters that only check for standard ASCII path separators.
        let suspicious_unicode: &[(char, &str)] = &[
            ('\u{FF0E}', "fullwidth full stop (.)"),
            ('\u{FF0F}', "fullwidth solidus (/)"),
            ('\u{FF3C}', "fullwidth reverse solidus (\\)"),
            ('\u{2024}', "one dot leader (.)"),
            ('\u{FE52}', "small full stop (.)"),
            ('\u{2025}', "two dot leader (..)"),
            ('\u{2026}', "horizontal ellipsis (...)"),
            ('\u{29F8}', "big solidus (/)"),
            ('\u{2044}', "fraction slash (/)"),
            ('\u{2215}', "division slash (/)"),
            ('\u{FE68}', "small reverse solidus (\\)"),
        ];
        for (ch, description) in suspicious_unicode {
            if path.contains(*ch) {
                return Err(SelfwareError::Safety(SafetyError::PathSuspiciousUnicode {
                    character: (*description).to_string(),
                    codepoint: *ch as u32,
                }));
            }
        }

        // Reject short path components mixing ASCII dots with non-ASCII chars.
        // Split on both '/' and '\' to cover Unix and Windows path separators.
        for component in path.split(&['/', '\\'][..]) {
            if component.is_empty() {
                continue;
            }
            let has_non_ascii = !component.is_ascii();
            let has_dots = component.contains('.');
            if has_non_ascii && has_dots && component.len() <= 10 {
                return Err(SelfwareError::Safety(SafetyError::PathSuspiciousMix {
                    component: component.to_string(),
                }));
            }
        }
        let path_buf = Path::new(path);
        let resolved = if path_buf.is_absolute() {
            path_buf.to_path_buf()
        } else {
            self.working_dir.join(path_buf)
        };

        // Check the lexical input BEFORE following magic links. /proc/self/cwd
        // and /proc/self/fd/N can canonicalize into an allowed workspace, which
        // must not erase the caller's use of a protected system namespace.
        let lexical = normalize_lexical(&resolved);
        if [
            "/proc",
            "/sys",
            "/dev/fd",
            "/dev/stdin",
            "/dev/stdout",
            "/dev/stderr",
        ]
        .iter()
        .any(|protected| lexical.starts_with(protected))
        {
            return Err(SelfwareError::Safety(SafetyError::PathProtectedSystem {
                path: lexical.display().to_string(),
            }));
        }

        // SECURITY: Use O_NOFOLLOW atomic open to eliminate TOCTOU symlink races.
        // Try to open with O_NOFOLLOW and resolve from the fd directly.
        // This prevents attackers from swapping a file with a symlink between
        // the time we check the path and the time we open it.
        let canonical = match open_nofollow_and_resolve(&resolved) {
            Ok(real_path) => real_path,
            Err(e) if e.raw_os_error() == Some(ELOOP) => {
                // O_NOFOLLOW returns ELOOP for symlinks
                let safe_target = self.check_symlink_safety(&resolved)?;
                canonicalize_or_fail(&safe_target)?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // If it doesn't exist, check parent atomically
                if let Some(parent) = resolved.parent() {
                    match open_nofollow_and_resolve(parent) {
                        Ok(real_parent) => {
                            real_parent.join(resolved.file_name().unwrap_or_default())
                        }
                        Err(e) if e.raw_os_error() == Some(ELOOP) => {
                            let safe_parent = self.check_symlink_safety(parent)?;
                            canonicalize_or_fail(&safe_parent)?
                                .join(resolved.file_name().unwrap_or_default())
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                            // Parent (and possibly higher ancestors) don't exist yet.
                            // Resolve from the deepest existing ancestor so tools that
                            // create nested directories can still be validated safely.
                            resolve_missing_path(&resolved)?
                        }
                        Err(_) => {
                            return Err(SelfwareError::Safety(
                                SafetyError::PathCanonicalizationFailed {
                                    path: resolved.display().to_string(),
                                },
                            ))
                        }
                    }
                } else {
                    return Err(SelfwareError::Safety(
                        SafetyError::PathCanonicalizationFailed {
                            path: resolved.display().to_string(),
                        },
                    ));
                }
            }
            Err(_) => canonicalize_or_fail(&resolved)?,
        };
        let canonical_str = strip_unc_prefix(&canonical.to_string_lossy());

        // Strict path traversal check - ALWAYS check, not just for ".."
        // Absolute paths can bypass ".." checks, so we validate all paths
        let original_parent = self
            .working_dir
            .canonicalize()
            .unwrap_or_else(|_| self.working_dir.clone());

        // The resolved path must be within allowed boundaries
        let is_within_working_dir = canonical.starts_with(&original_parent);
        let is_explicitly_allowed = self
            .is_path_in_allowed_list(&canonical_str, path)
            .unwrap_or(false);

        // Check allowed_paths configuration
        if !self.config.allowed_paths.is_empty() {
            // allowed_paths is configured: path must be in the allowed list
            if !is_explicitly_allowed {
                return Err(SelfwareError::Safety(SafetyError::PathNotAllowed {
                    path: canonical_str.to_string(),
                }));
            }
            // Even if in allowed list, still check denied patterns below
        } else {
            // No allowed_paths configured: restrict to working directory
            // to prevent unrestricted filesystem access.
            if !is_within_working_dir {
                tracing::warn!(
                    "No allowed_paths configured — restricting to working directory. \
                     Path '{}' is outside '{}'",
                    canonical_str,
                    original_parent.display()
                );
                // Distinguish between traversal attack vs absolute path access
                if path.contains("..") {
                    return Err(SelfwareError::Safety(SafetyError::PathTraversal {
                        path: format!("{} resolves to {}", path, canonical_str),
                    }));
                } else {
                    return Err(SelfwareError::Safety(SafetyError::PathOutsideWorkspace {
                        path: canonical_str.to_string(),
                    }));
                }
            }
        }

        // Always validate denied patterns, even for paths in allowed_paths
        // This prevents accidentally allowing dangerous paths via overly broad allowed_paths
        // Check against denied patterns using both original and canonical paths.
        //
        // Cross-platform: convert `\` to `/` in BOTH the pattern and the input
        // before glob matching, since the `glob` crate treats `\` as an escape
        // character. This is a no-op on Unix.
        let canonical_glob = to_glob_form(&canonical_str);
        let path_glob = to_glob_form(path);
        for pattern in &self.config.denied_paths {
            let pattern_glob = to_glob_form(pattern);
            let glob_pattern = glob::Pattern::new(&pattern_glob)?;

            if glob_pattern.matches(&canonical_glob) {
                return Err(SelfwareError::Safety(SafetyError::PathDeniedPattern {
                    pattern: pattern.clone(),
                }));
            }
            if glob_pattern.matches(&path_glob) {
                return Err(SelfwareError::Safety(SafetyError::PathDeniedPattern {
                    pattern: pattern.clone(),
                }));
            }

            // Also check components for filename-only patterns like ".env".
            for component in canonical.components() {
                if let std::path::Component::Normal(name) = component {
                    let name_str = name.to_string_lossy();
                    if !pattern.contains('/')
                        && !pattern.contains('\\')
                        && glob_pattern.matches(&name_str)
                    {
                        return Err(SelfwareError::Safety(SafetyError::PathDeniedPattern {
                            pattern: pattern.clone(),
                        }));
                    }
                }
            }
        }

        // Check for dangerous absolute paths that should never be allowed
        // even if they match allowed_paths (defense in depth)
        let dangerous_system_paths = [
            "/etc/passwd",
            "/etc/shadow",
            "/etc/sudoers",
            "/etc/ssh/",
            "/root/",
            "/proc/",
            "/sys/",
            "/boot/",
            "/var/log/",
        ];
        for dangerous in &dangerous_system_paths {
            if canonical_str.starts_with(dangerous) {
                return Err(SelfwareError::Safety(SafetyError::PathProtectedSystem {
                    path: canonical_str.to_string(),
                }));
            }
        }

        Ok(())
    }

    /// Check if a path is in the allowed list.
    ///
    /// IMPORTANT: We only check the canonical path, not the original path.
    ///
    /// Cross-platform handling:
    /// - The input `canonical_str` is already canonicalized by the caller.
    /// - On macOS, allow-list entries that point at temp paths
    ///   (`/var/folders/...`) must be canonicalized to `/private/var/folders/...`
    ///   to match the canonical input. We do that here by passing each pattern
    ///   through [`crate::safety::checker::normalize_path`].
    /// - On Windows, paths use `\` as a separator while glob patterns use `/`,
    ///   so we convert via [`to_glob_form`] before invoking `glob::Pattern`.
    pub fn is_path_in_allowed_list(
        &self,
        canonical_str: &str,
        _original_path: &str,
    ) -> Result<bool> {
        let working_dir_canonical_pb = normalize_path(&self.working_dir);
        let working_dir_canonical = working_dir_canonical_pb.to_string_lossy();

        // Normalize to forward slashes for glob matching on all platforms.
        // The `glob` crate treats backslash as an escape character, so
        // Windows paths like `C:\foo\bar` must be converted to `C:/foo/bar`.
        let canonical_normalized = to_glob_form(canonical_str);
        let working_dir_normalized = to_glob_form(&working_dir_canonical);

        for pattern in &self.config.allowed_paths {
            // Expand a leading "~" to the user's home directory so that
            // patterns like "~/project" match the real canonical path.
            let home_expanded: String = if let Some(rest) = pattern.strip_prefix("~/") {
                if let Some(home) = std::env::var_os("HOME") {
                    format!("{}/{}", home.to_string_lossy(), rest)
                } else if let Some(home) = dirs::home_dir() {
                    format!("{}/{}", home.display(), rest)
                } else {
                    pattern.clone()
                }
            } else if pattern == "~" {
                if let Some(home) = std::env::var_os("HOME") {
                    home.to_string_lossy().to_string()
                } else if let Some(home) = dirs::home_dir() {
                    home.display().to_string()
                } else {
                    pattern.clone()
                }
            } else {
                pattern.clone()
            };

            // For relative patterns, expand using the working directory
            let expanded_pattern = if home_expanded.starts_with("./") || home_expanded == "." {
                let suffix = home_expanded.strip_prefix("./").unwrap_or("");
                format!("{}/{}", working_dir_normalized, suffix)
            } else {
                to_glob_form(&home_expanded)
            };

            let pattern_normalized = to_glob_form(&home_expanded);

            if glob::Pattern::new(&expanded_pattern)?.matches(&canonical_normalized)
                || glob::Pattern::new(&pattern_normalized)?.matches(&canonical_normalized)
            {
                return Ok(true);
            }

            // Symmetric canonicalization: the allow-list pattern may be a
            // pre-canonical path the caller passed in literally — e.g. on
            // macOS callers commonly use `TempDir::path()` (`/var/folders/...`)
            // verbatim while the canonical form is `/private/var/folders/...`.
            // Run the literal pattern through the same `normalize_path`
            // pipeline so prefixes line up. We only attempt this for patterns
            // that look like concrete paths (no glob metacharacters), since
            // canonicalizing a pattern like `**/*.rs` is meaningless.
            let looks_like_concrete_path = !pattern_normalized.contains(['*', '?', '[']);
            if looks_like_concrete_path {
                let canonical_pattern = normalize_path(Path::new(pattern));
                let canonical_pattern_str = to_glob_form(&canonical_pattern.to_string_lossy());
                if !canonical_pattern_str.is_empty()
                    && canonical_pattern_str != pattern_normalized
                    && glob::Pattern::new(&canonical_pattern_str)?.matches(&canonical_normalized)
                {
                    return Ok(true);
                }
            }

            // Some allow-list entries are written as `<path>/**` directly —
            // canonicalize the parent (when IT is concrete) and re-append the
            // suffix, so short-name/symlinked parents (Windows 8.3 TEMP dirs,
            // macOS /var symlinks) still match the canonical input. This must
            // live OUTSIDE the concrete-pattern gate above: a `/**` suffix is
            // itself a metacharacter, so gating on it makes the branch
            // unreachable for exactly the patterns that need it.
            if let Some(stripped) = pattern_normalized.strip_suffix("/**") {
                if !stripped.is_empty() && !stripped.contains(['*', '?', '[']) {
                    let canon_parent = normalize_path(Path::new(stripped));
                    let canon_parent_glob = to_glob_form(&canon_parent.to_string_lossy());
                    if !canon_parent_glob.is_empty() {
                        let combined = format!("{}/**", canon_parent_glob);
                        if glob::Pattern::new(&combined)?.matches(&canonical_normalized) {
                            return Ok(true);
                        }
                        // Also accept the parent itself, since `<dir>/**` does
                        // not match `<dir>` exactly.
                        if canonical_normalized == canon_parent_glob {
                            return Ok(true);
                        }
                    }
                }
            }

            // On Windows, absolute Unix-style patterns like "/**" won't match
            // drive-letter paths like "C:/Users/...". Try prepending the drive letter.
            if cfg!(target_os = "windows") && pattern_normalized.starts_with('/') {
                if let Some(drive_prefix) = canonical_normalized.get(..2) {
                    if drive_prefix.ends_with(':') {
                        let win_pattern = format!("{}{}", drive_prefix, pattern_normalized);
                        if glob::Pattern::new(&win_pattern)?.matches(&canonical_normalized) {
                            return Ok(true);
                        }
                    }
                }
            }

            // Fallback: for "./**" pattern, allow the working dir itself and any
            // path strictly BELOW it. A raw string starts_with would also match a
            // SIBLING that merely shares the name prefix (e.g. `<wd>-evil`), so
            // require an exact match or a path-separator boundary.
            if pattern == "./**" {
                let wd = working_dir_normalized.trim_end_matches('/');
                if canonical_normalized == wd
                    || canonical_normalized.starts_with(&format!("{}/", wd))
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Open `path` as a regular file for reading and prove the object actually
    /// opened is allowed.
    ///
    /// Closes the validate-then-reopen TOCTOU: the lexical path is validated
    /// first (as [`Self::validate`]), then opened `O_NONBLOCK` (a FIFO cannot
    /// hang the open), `fstat`ed (non-regular files are refused with a clear
    /// error), and the DESCRIPTOR's real path is validated again. Whatever
    /// the filesystem did between the two steps — an intermediate directory
    /// swapped for a symlink, a file renamed in — the returned handle refers
    /// to an object whose real location passed policy. Read from
    /// [`ValidatedFile::file`]; never re-open by path.
    pub fn open_regular_file(&self, path: &str) -> Result<ValidatedFile> {
        self.validate(path)?;
        #[cfg(test)]
        run_after_lexical_validate_hook();
        let target = self.absolute(path);

        #[cfg(unix)]
        let file = {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
                .open(&target)?
        };
        #[cfg(not(unix))]
        let file = std::fs::File::open(&target)?;

        let meta = file.metadata()?;
        if !meta.is_file() {
            return Err(not_regular_error(&target, &meta));
        }

        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            // Regular file confirmed: restore blocking mode for the reader.
            // SAFETY: plain fcntl flag get/set on a live descriptor.
            unsafe {
                let fd = file.as_raw_fd();
                let flags = libc::fcntl(fd, libc::F_GETFL);
                if flags != -1 {
                    libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
                }
            }
        }

        #[cfg(unix)]
        let real_path = fd_real_path(&file, &target)?;
        #[cfg(not(unix))]
        let real_path = canonicalize_or_fail(&target)?;

        self.validate(&real_path.to_string_lossy())?;
        Ok(ValidatedFile { file, real_path })
    }

    /// Open the parent directory of `path` as a pinned descriptor for
    /// fd-relative writes (see [`open_parent_dir_fd`]). The lexical path is
    /// validated first; then both the prospective and the final real
    /// directory locations are re-validated against this validator's policy.
    #[cfg(unix)]
    pub fn open_parent_dir(
        &self,
        path: &str,
        create: bool,
    ) -> Result<(std::fs::File, std::ffi::OsString)> {
        self.validate(path)?;
        #[cfg(test)]
        run_after_lexical_validate_hook();
        open_parent_dir_fd(&self.absolute(path), create, &|real: &Path| {
            self.validate(&real.to_string_lossy())
        })
    }

    /// `path` as given, anchored at the working directory when relative.
    fn absolute(&self, path: &str) -> PathBuf {
        let raw = Path::new(path);
        if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            self.working_dir.join(raw)
        }
    }

    /// Check for symlink-based attacks.
    pub fn check_symlink_safety(&self, path: &Path) -> Result<PathBuf> {
        let mut current = path.to_path_buf();
        let mut visited = std::collections::HashSet::new();
        let max_depth = 40; // Linux default MAXSYMLINKS

        for _ in 0..max_depth {
            if !current.is_symlink() {
                break;
            }

            let current_str = current.to_string_lossy().to_string();
            if visited.contains(&current_str) {
                return Err(SelfwareError::Safety(SafetyError::SymlinkLoop {
                    path: path.display().to_string(),
                }));
            }
            visited.insert(current_str);

            let target = std::fs::read_link(&current)?;
            let resolved_target = if target.is_absolute() {
                target
            } else {
                current.parent().unwrap_or(Path::new("/")).join(&target)
            };

            let target_str = resolved_target.to_string_lossy();
            let dangerous_targets = [
                "/etc/passwd",
                "/etc/shadow",
                "/etc/sudoers",
                "/root/",
                "/proc/",
                "/sys/",
            ];

            for dangerous in &dangerous_targets {
                if target_str.starts_with(dangerous) {
                    return Err(SelfwareError::Safety(SafetyError::SymlinkProtectedTarget {
                        symlink: path.display().to_string(),
                        target: target_str.to_string(),
                    }));
                }
            }

            current = resolved_target;
        }

        if visited.len() >= max_depth {
            return Err(SelfwareError::Safety(SafetyError::SymlinkChainTooDeep {
                path: path.display().to_string(),
            }));
        }

        Ok(current)
    }
}

/// Strip the Windows `\\?\` extended-length path prefix.
///
/// On Windows, `canonicalize()` returns paths like `\\?\C:\Users\...`
/// but `current_dir()` returns `C:\Users\...` without the prefix.
/// This causes `starts_with` comparisons to fail.
fn strip_unc_prefix(path: &str) -> String {
    if cfg!(target_os = "windows") {
        path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
    } else {
        path.to_string()
    }
}

/// Normalize a path lexically (without touching the filesystem) by resolving
/// `.` and `..` components.
///
/// Use this only when the path may not exist on disk; otherwise prefer
/// [`crate::safety::checker::normalize_path`], which canonicalizes through
/// the filesystem and strips the Windows `\\?\` UNC prefix.
pub fn lexical_normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                if !components.is_empty() {
                    components.pop();
                }
            }
            std::path::Component::CurDir => {}
            c => components.push(c),
        }
    }

    components.iter().collect()
}

#[cfg(test)]
#[path = "../../tests/unit/safety/path_validator/path_validator_test.rs"]
mod tests;
