//! Shared path validation logic for safety checks.

use crate::config::SafetyConfig;
use anyhow::Result;
use std::path::{Path, PathBuf};

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
        let path_buf = Path::new(path);
        let resolved = if path_buf.is_absolute() {
            path_buf.to_path_buf()
        } else {
            self.working_dir.join(path_buf)
        };

        // Check for symlink attacks on existing paths
        if resolved.exists() {
            self.check_symlink_safety(&resolved)?;
        } else if let Some(parent) = resolved.parent() {
            // For new files, check the parent directory
            if parent.exists() {
                self.check_symlink_safety(parent)?;
            }
        }

        // Canonicalize the path (resolves symlinks for existing paths)
        let canonical = resolved
            .canonicalize()
            .unwrap_or_else(|_| normalize_path(&resolved));
        let canonical_str = strip_unc_prefix(&canonical.to_string_lossy());

        // Strict path traversal check
        if path.contains("..") {
            let original_parent = self
                .working_dir
                .canonicalize()
                .unwrap_or_else(|_| self.working_dir.clone());

            // The resolved path must be within allowed boundaries
            let is_within_working_dir = canonical.starts_with(&original_parent);
            let is_explicitly_allowed = self.is_path_in_allowed_list(&canonical_str, path)?;

            if !is_within_working_dir && !is_explicitly_allowed {
                anyhow::bail!(
                    "Path traversal detected: {} resolves to {}",
                    path,
                    canonical_str
                );
            }
        }

        // Check against denied patterns using both original and canonical paths.
        for pattern in &self.config.denied_paths {
            let glob_pattern = glob::Pattern::new(pattern)?;

            if glob_pattern.matches(&canonical_str) {
                anyhow::bail!("Path matches denied pattern: {}", pattern);
            }
            if glob_pattern.matches(path) {
                anyhow::bail!("Path matches denied pattern: {}", pattern);
            }

            // Also check components for filename-only patterns like ".env".
            for component in canonical.components() {
                if let std::path::Component::Normal(name) = component {
                    let name_str = name.to_string_lossy();
                    if !pattern.contains('/')
                        && !pattern.contains('\\')
                        && glob_pattern.matches(&name_str)
                    {
                        anyhow::bail!("Path component matches denied pattern: {}", pattern);
                    }
                }
            }
        }

        if !self.config.allowed_paths.is_empty()
            && !self.is_path_in_allowed_list(&canonical_str, path)?
        {
            anyhow::bail!("Path not in allowed list: {}", canonical_str);
        }

        Ok(())
    }

    /// Check if a path is in the allowed list.
    ///
    /// IMPORTANT: We only check the canonical path, not the original path.
    pub fn is_path_in_allowed_list(
        &self,
        canonical_str: &str,
        _original_path: &str,
    ) -> Result<bool> {
        let working_dir_canonical = strip_unc_prefix(
            &self
                .working_dir
                .canonicalize()
                .unwrap_or_else(|_| self.working_dir.clone())
                .to_string_lossy(),
        );

        for pattern in &self.config.allowed_paths {
            // For relative patterns, expand using the working directory
            let expanded_pattern = if pattern.starts_with("./") || pattern == "." {
                let suffix = pattern.strip_prefix("./").unwrap_or("");
                if cfg!(target_os = "windows") {
                    // On Windows, use backslash separator for glob matching
                    format!("{}\\{}", working_dir_canonical, suffix)
                } else {
                    format!("{}/{}", working_dir_canonical, suffix)
                }
            } else {
                pattern.clone()
            };

            if glob::Pattern::new(&expanded_pattern)?.matches(canonical_str)
                || glob::Pattern::new(pattern)?.matches(canonical_str)
            {
                return Ok(true);
            }

            // Fallback: for "./**" pattern, do a simple prefix check
            if pattern == "./**" && canonical_str.starts_with(&working_dir_canonical) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Check for symlink-based attacks.
    pub fn check_symlink_safety(&self, path: &Path) -> Result<()> {
        let mut current = path.to_path_buf();
        let mut visited = std::collections::HashSet::new();
        let max_depth = 40; // Linux default MAXSYMLINKS

        for _ in 0..max_depth {
            if !current.is_symlink() {
                break;
            }

            let current_str = current.to_string_lossy().to_string();
            if visited.contains(&current_str) {
                anyhow::bail!("Symlink loop detected: {}", path.display());
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
                    anyhow::bail!(
                        "Symlink points to protected system path: {} -> {}",
                        path.display(),
                        target_str
                    );
                }
            }

            current = resolved_target;
        }

        if visited.len() >= max_depth {
            anyhow::bail!(
                "Symlink chain too deep (possible attack): {}",
                path.display()
            );
        }

        Ok(())
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

/// Normalize a path by resolving . and .. components.
pub fn normalize_path(path: &Path) -> PathBuf {
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
        }
    }

    // ===== normalize_path tests =====

    #[test]
    fn test_normalize_simple_absolute() {
        let path = normalize_path(Path::new("/foo/bar/baz"));
        assert_eq!(path, PathBuf::from("/foo/bar/baz"));
    }

    #[test]
    fn test_normalize_with_dot() {
        let path = normalize_path(Path::new("/foo/./bar"));
        assert_eq!(path, PathBuf::from("/foo/bar"));
    }

    #[test]
    fn test_normalize_with_dotdot() {
        let path = normalize_path(Path::new("/foo/bar/../baz"));
        assert_eq!(path, PathBuf::from("/foo/baz"));
    }

    #[test]
    fn test_normalize_multiple_dotdot() {
        let path = normalize_path(Path::new("/foo/bar/baz/../../qux"));
        assert_eq!(path, PathBuf::from("/foo/qux"));
    }

    #[test]
    fn test_normalize_dotdot_at_root() {
        // When all components are popped, the result is an empty path
        let path = normalize_path(Path::new("/foo/../.."));
        assert_eq!(path, PathBuf::from(""));
    }

    #[test]
    fn test_normalize_relative() {
        let path = normalize_path(Path::new("foo/./bar/../baz"));
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
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not in allowed list"));
    }

    #[test]
    fn test_validate_env_local_denied() {
        let config = make_config(vec![], vec!["**/.env.local"]);
        let cwd = std::env::current_dir().unwrap();
        let validator = PathValidator::new(&config, cwd);
        let result = validator.validate(".env.local");
        assert!(result.is_err());
    }
}
