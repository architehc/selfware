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
#[cfg(unix)]
fn open_nofollow_and_resolve(path: &Path) -> std::io::Result<PathBuf> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::AsRawFd;

    let fd = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW)
        .open(path)?;

    let raw_fd = fd.as_raw_fd();

    // Linux: resolve via /proc/self/fd which is atomic
    let fd_path = format!("/proc/self/fd/{}", raw_fd);
    let proc_path = Path::new(&fd_path);
    if proc_path.exists() {
        return std::fs::read_link(proc_path);
    }

    // macOS: use F_GETPATH to resolve fd to path atomically (no TOCTOU)
    #[cfg(target_os = "macos")]
    {
        const F_GETPATH: i32 = 50;
        const MAXPATHLEN: usize = 1024;
        let mut buf = vec![0u8; MAXPATHLEN];
        let ret = unsafe {
            extern "C" {
                fn fcntl(fd: i32, cmd: i32, ...) -> i32;
            }
            fcntl(raw_fd, F_GETPATH, buf.as_mut_ptr())
        };
        if ret != -1 {
            let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            buf.truncate(len);
            return Ok(PathBuf::from(std::ffi::OsString::from(
                String::from_utf8_lossy(&buf).into_owned(),
            )));
        }
    }

    // Final fallback for other Unix platforms without /proc
    path.canonicalize()
}

#[cfg(not(unix))]
fn open_nofollow_and_resolve(path: &Path) -> std::io::Result<PathBuf> {
    path.canonicalize()
}

/// Canonicalize a path and fail closed if the filesystem cannot resolve it.
fn canonicalize_or_fail(path: &Path) -> Result<PathBuf> {
    path.canonicalize().map_err(|_| {
        SelfwareError::Safety(SafetyError::PathCanonicalizationFailed {
            path: path.display().to_string(),
        })
    })
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
            // For relative patterns, expand using the working directory
            let expanded_pattern = if pattern.starts_with("./") || pattern == "." {
                let suffix = pattern.strip_prefix("./").unwrap_or("");
                format!("{}/{}", working_dir_normalized, suffix)
            } else {
                to_glob_form(pattern)
            };

            let pattern_normalized = to_glob_form(pattern);

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
                // Some allow-list entries are written as `<path>/**` directly —
                // try canonicalizing the parent and re-appending the suffix.
                if let Some(stripped) = pattern_normalized.strip_suffix("/**") {
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

            // Fallback: for "./**" pattern, do a simple prefix check
            if pattern == "./**" && canonical_normalized.starts_with(&*working_dir_normalized) {
                return Ok(true);
            }
        }
        Ok(false)
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
mod tests {
    use super::*;

    fn make_config(allowed: Vec<&str>, denied: Vec<&str>) -> SafetyConfig {
        SafetyConfig {
            allowed_paths: allowed.into_iter().map(|s| s.to_string()).collect(),
            denied_paths: denied.into_iter().map(|s| s.to_string()).collect(),
            protected_branches: vec![],
            require_confirmation: vec![],
            strict_permissions: false,
            permissions: vec![],
        }
    }

    // ===== lexical_normalize_path tests =====

    #[cfg(unix)]
    #[test]
    fn test_normalize_simple_absolute() {
        let path = lexical_normalize_path(Path::new("/foo/bar/baz"));
        assert_eq!(path, PathBuf::from("/foo/bar/baz"));
    }

    #[cfg(unix)]
    #[test]
    fn test_normalize_with_dot() {
        let path = lexical_normalize_path(Path::new("/foo/./bar"));
        assert_eq!(path, PathBuf::from("/foo/bar"));
    }

    #[cfg(unix)]
    #[test]
    fn test_normalize_with_dotdot() {
        let path = lexical_normalize_path(Path::new("/foo/bar/../baz"));
        assert_eq!(path, PathBuf::from("/foo/baz"));
    }

    #[cfg(unix)]
    #[test]
    fn test_normalize_multiple_dotdot() {
        let path = lexical_normalize_path(Path::new("/foo/bar/baz/../../qux"));
        assert_eq!(path, PathBuf::from("/foo/qux"));
    }

    #[cfg(unix)]
    #[test]
    fn test_normalize_dotdot_at_root() {
        // When all components are popped, the result is an empty path
        let path = lexical_normalize_path(Path::new("/foo/../.."));
        assert_eq!(path, PathBuf::from(""));
    }

    #[test]
    fn test_normalize_relative() {
        let path = lexical_normalize_path(Path::new("foo/./bar/../baz"));
        assert_eq!(path, PathBuf::from("foo/baz"));
    }

    // ===== strip_unc_prefix tests =====

    #[test]
    fn test_strip_unc_prefix_normal_path() {
        assert_eq!(strip_unc_prefix("/foo/bar"), "/foo/bar");
    }

    #[test]
    fn test_strip_unc_prefix_empty() {
        assert_eq!(strip_unc_prefix(""), "");
    }

    // ===== is_path_in_allowed_list tests =====

    #[test]
    fn test_allowed_list_empty() {
        let config = make_config(vec![], vec![]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        // Empty allowed list => nothing matches
        assert!(!validator
            .is_path_in_allowed_list("/some/path", "/some/path")
            .unwrap());
    }

    #[test]
    fn test_allowed_list_absolute_glob() {
        let config = make_config(vec!["/tmp/**"], vec![]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        assert!(validator
            .is_path_in_allowed_list("/tmp/foo/bar", "/tmp/foo/bar")
            .unwrap());
        assert!(!validator
            .is_path_in_allowed_list("/etc/passwd", "/etc/passwd")
            .unwrap());
    }

    #[test]
    fn test_allowed_list_relative_glob() {
        let config = make_config(vec!["./**"], vec![]);
        let cwd = std::env::current_dir().unwrap();
        let cwd_str = cwd.to_string_lossy();
        let validator = PathValidator::new(&config, cwd.clone());
        let test_path = format!("{}/src/main.rs", cwd_str);
        assert!(validator
            .is_path_in_allowed_list(&test_path, "./src/main.rs")
            .unwrap());
    }

    // ===== validate tests =====

    #[test]
    fn test_validate_denied_env_file() {
        let config = make_config(vec![], vec!["**/.env"]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        let result = validator.validate(".env");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("denied pattern"));
    }

    #[test]
    fn test_validate_denied_ssh() {
        let config = make_config(vec![], vec!["**/.ssh/**"]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        let result = validator.validate("/home/user/.ssh/id_rsa");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_allowed_path() {
        let config = make_config(vec![], vec![]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd.clone());
        // A path within the working dir with no denied patterns should be OK
        let result = validator.validate("src/main.rs");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_denied_secrets_dir() {
        let config = make_config(vec![], vec!["**/secrets/**"]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        let result = validator.validate("config/secrets/api_key.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_path_traversal_detected() {
        let config = make_config(vec![], vec![]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        // Traversal that goes outside working dir
        let result = validator.validate("../../../../etc/passwd");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Path traversal") || err_msg.contains("denied"),
            "Expected traversal or denied error, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_validate_not_in_allowed_list() {
        let config = make_config(vec!["/allowed/**"], vec![]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        let result = validator.validate("/not-allowed/file.txt");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not in allowed list") || err.contains("Failed to canonicalize"),
            "Expected not-in-allowed-list or canonicalization error, got: {}",
            err
        );
    }

    #[test]
    fn test_validate_env_local_denied() {
        let config = make_config(vec![], vec!["**/.env.local"]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        let result = validator.validate(".env.local");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_null_byte_rejected() {
        let config = make_config(vec![], vec![]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        let result = validator.validate("safe_path\0/etc/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("null bytes"));
    }

    #[test]
    fn test_validate_null_byte_at_end_rejected() {
        let config = make_config(vec![], vec![]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        let result = validator.validate("some/file.txt\0");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("null bytes"));
    }

    /// Verify the SELFWARE_TEST_MODE env-var bypass is gone (issue #59).
    /// Setting the env var must NOT cause denied paths to be allowed.
    #[test]
    fn test_no_test_mode_bypass() {
        // SAFETY: This test is single-threaded w.r.t. this env var. We set
        // and remove it within the same test to avoid leaking state.
        unsafe {
            std::env::set_var("SELFWARE_TEST_MODE", "1");
        }

        let config = make_config(vec![], vec!["**/.env"]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);

        // Even with SELFWARE_TEST_MODE set, denied paths must still be denied.
        let result = validator.validate(".env");
        assert!(
            result.is_err(),
            "SELFWARE_TEST_MODE must not bypass path validation"
        );
        assert!(
            result.unwrap_err().to_string().contains("denied pattern"),
            "Expected 'denied pattern' error"
        );

        unsafe {
            std::env::remove_var("SELFWARE_TEST_MODE");
        }
    }

    /// Test that encoding-based path bypasses are blocked.
    /// These tests verify defense against path traversal using Unicode homoglyphs.
    #[test]
    fn test_validate_rejects_fullwidth_dot() {
        let config = make_config(vec![], vec![]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        // Fullwidth full stop (U+FF0E) looks like a period
        let result = validator.validate("src\u{FF0E}./etc/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unicode"));
    }

    #[test]
    fn test_validate_rejects_fullwidth_slash() {
        let config = make_config(vec![], vec![]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        // Fullwidth solidus (U+FF0F) looks like a forward slash
        let result = validator.validate(".\u{FF0F}..\u{FF0F}etc\u{FF0F}passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_rejects_two_dot_leader() {
        let config = make_config(vec![], vec![]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        // Two dot leader (U+2025) looks like two periods
        let result = validator.validate("src\u{2025}\u{2025}/etc/passwd");
        assert!(result.is_err());
    }

    /// Test for path traversal with mixed ASCII and non-ASCII characters
    #[test]
    fn test_validate_rejects_suspicious_mix() {
        let config = make_config(vec![], vec![]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        // Mixing dots with non-ASCII characters in a short component
        let result = validator.validate("foo./\u{FF0E}/etc/passwd");
        assert!(result.is_err());
    }

    /// Test for absolute path access outside workspace
    #[test]
    fn test_validate_rejects_absolute_system_path() {
        let config = make_config(vec![], vec![]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        // Direct access to system files should be blocked
        let result = validator.validate("/etc/passwd");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("outside") || err.contains("traversal") || err.contains("system"),
            "Expected outside/traversal/system error, got: {}",
            err
        );
    }

    #[test]
    fn test_validate_rejects_absolute_root() {
        let config = make_config(vec![], vec![]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        let result = validator.validate("/");
        assert!(result.is_err());
    }

    /// Test for dangerous system paths
    #[test]
    fn test_validate_rejects_etc_passwd() {
        let config = make_config(vec!["./**"], vec![]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        let result = validator.validate("/etc/passwd");
        assert!(result.is_err());
        // Path is outside working dir (either "system" or "outside" error)
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("system") || err.contains("outside") || err.contains("allowed"),
            "Expected security error, got: {}",
            err
        );
    }

    #[test]
    fn test_validate_rejects_etc_shadow() {
        let config = make_config(vec!["./**"], vec![]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        let result = validator.validate("/etc/shadow");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("system") || err.contains("outside") || err.contains("allowed"),
            "Expected security error, got: {}",
            err
        );
    }

    #[test]
    fn test_validate_rejects_proc_access() {
        let config = make_config(vec!["./**"], vec![]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        let result = validator.validate("/proc/self/environ");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("system") || err.contains("outside") || err.contains("allowed"),
            "Expected security error, got: {}",
            err
        );
    }

    #[test]
    fn test_validate_rejects_ssh_directory() {
        let config = make_config(vec![], vec!["**/.ssh/**"]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        let result = validator.validate("/home/user/.ssh/id_rsa");
        assert!(result.is_err());
    }

    /// Test for double encoding/path normalization bypass attempts
    #[test]
    fn test_validate_rejects_double_dot_variations() {
        let config = make_config(vec![], vec![]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);

        // Classic path traversal
        let result = validator.validate("../../etc/passwd");
        assert!(result.is_err(), "Classic path traversal should be blocked");
    }
}
