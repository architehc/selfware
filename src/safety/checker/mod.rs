//! Safety Layer - Tool Call Validation
//!
//! Validates tool calls before execution to prevent dangerous operations.
//! Checks include:
//! - Path traversal prevention
//! - Protected path enforcement
//! - Command blacklisting
//! - Symlink attack prevention

pub mod types;
pub mod validation;

pub use types::SafetyChecker;

use std::path::{Path, PathBuf};

/// Normalize a filesystem path for cross-platform safety comparisons.
///
/// On Linux/macOS this canonicalizes the path (resolving symlinks and `..`/`.`)
/// when the path exists; on Windows it additionally strips the `\\?\`
/// extended-length prefix that `Path::canonicalize` produces, so the result
/// matches the form returned by `current_dir()` and user-supplied paths.
///
/// If canonicalization fails (e.g. the path does not yet exist) we fall back
/// to `p.to_path_buf()` — also with the UNC prefix stripped on Windows — so
/// the function is safe to call on prospective output paths.
///
/// This is the single source of truth used by `PathValidator` so that the
/// path being checked AND every allow-list / deny-list entry is normalized
/// the same way. macOS canonicalizes `/var/folders/...` to
/// `/private/var/folders/...`; Windows produces `\\?\C:\...`. Without
/// symmetric normalization the comparison silently fails.
pub fn normalize_path(p: &Path) -> PathBuf {
    let canonical = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    strip_unc_prefix_pathbuf(canonical)
}

/// Strip the Windows `\\?\` extended-length path prefix from a `PathBuf`.
///
/// On non-Windows targets this is a no-op.
#[cfg(target_os = "windows")]
fn strip_unc_prefix_pathbuf(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        PathBuf::from(stripped.to_string())
    } else {
        p
    }
}

#[cfg(not(target_os = "windows"))]
fn strip_unc_prefix_pathbuf(p: PathBuf) -> PathBuf {
    p
}

/// Convert OS-native path separators to forward slashes for glob matching.
///
/// The `glob` crate treats `\` as an escape character on every platform, so a
/// Windows path like `C:\foo\bar` cannot be matched by a pattern such as
/// `**/bar`. We normalize both the pattern and the input path to forward
/// slashes before invoking `glob::Pattern::matches` — this is a no-op on
/// Unix where paths already use `/`.
pub fn to_glob_form(s: &str) -> String {
    if cfg!(target_os = "windows") {
        s.replace('\\', "/")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests;
