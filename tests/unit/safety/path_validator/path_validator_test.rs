use super::*;

fn make_config(allowed: Vec<&str>, denied: Vec<&str>) -> SafetyConfig {
    SafetyConfig {
        allowed_paths: allowed.into_iter().map(|s| s.to_string()).collect(),
        denied_paths: denied.into_iter().map(|s| s.to_string()).collect(),
        protected_branches: vec![],
        require_confirmation: vec![],
        strict_permissions: false,
        permissions: vec![],
        trust_gate_tool_results: true,
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

#[test]
fn test_validate_rejects_sibling_prefix_escape() {
    let base = tempfile::tempdir().unwrap();
    let proj = base.path().join("proj");
    let evil = base.path().join("proj-evil");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::create_dir_all(&evil).unwrap();
    let secret = evil.join("secret.txt");
    std::fs::write(&secret, b"x").unwrap();

    let config = make_config(vec!["./**"], vec![]);
    let validator = PathValidator::new(&config, proj.clone());

    // A sibling dir sharing only the name PREFIX must be rejected.
    let result = validator.validate(secret.to_str().unwrap());
    assert!(
        result.is_err(),
        "sibling-prefix path must be rejected, got: {:?}",
        result
    );

    // A genuine child of the working dir is still allowed.
    let child = proj.join("ok.txt");
    std::fs::write(&child, b"y").unwrap();
    assert!(
        validator.validate(child.to_str().unwrap()).is_ok(),
        "real child of working dir must be allowed"
    );
}

/// `<path>/**` allow-list patterns must match even when the pattern's
/// parent differs from the canonical form of the input (symlinked or
/// Windows 8.3-short-named parent): the parent is canonicalized before
/// matching. Regression test for the branch that was unreachable inside
/// the no-metacharacters gate.
#[cfg(unix)]
#[test]
fn test_allow_list_globstar_pattern_canonicalizes_symlinked_parent() {
    let real = tempfile::tempdir().unwrap();
    let link_dir = real
        .path()
        .parent()
        .unwrap()
        .join(format!("sw-pv-link-{}", std::process::id()));
    std::os::unix::fs::symlink(real.path(), &link_dir).unwrap();

    let pattern = format!("{}/**", link_dir.display());
    let config = make_config(vec![pattern.as_str()], vec![]);
    let cwd = std::env::current_dir().unwrap();
    let validator = PathValidator::new(&config, cwd);

    // The canonical input lives under the REAL dir; the pattern points at
    // the SYMLINK. Pre-fix this missed; parent canonicalization must line
    // them up.
    let canonical_child = real.path().canonicalize().unwrap().join("file.txt");
    let allowed = validator
        .is_path_in_allowed_list(&canonical_child.to_string_lossy(), "ignored")
        .unwrap();
    let _ = std::fs::remove_dir_all(&link_dir);
    assert!(
        allowed,
        "<symlink>/** must match a canonical child of the real dir"
    );
}

#[test]
fn protected_magic_link_namespace_is_refused_before_canonicalization() {
    let dir = tempfile::tempdir().unwrap();
    let validator = PathValidator::new(&make_config(vec!["/**"], vec![]), dir.path().to_path_buf());
    for path in [
        "/proc/self/cwd",
        "/proc/self/cwd/new.txt",
        "/proc/self/exe",
        "/proc/self/fd/0",
        "/proc/thread-self/cwd",
        "/proc/123/fd/4",
        "../../../../../../proc/self/cwd",
        "/%70roc/self/cwd",
        "/sys/kernel",
        "/dev/fd/0",
    ] {
        assert!(
            matches!(
                validator.validate(path),
                Err(SelfwareError::Safety(
                    SafetyError::PathProtectedSystem { .. }
                ))
            ),
            "must reject protected input before OS-dependent resolution: {path}"
        );
    }
    // A local directory with the same name is ordinary workspace data.
    assert!(validator.validate("proc/self/cwd").is_ok());
}

#[test]
fn missing_path_ancestry_has_a_bounded_resolution_budget() {
    let dir = tempfile::tempdir().unwrap();
    let validator = PathValidator::new(&make_config(vec!["/**"], vec![]), dir.path().to_path_buf());
    let deep = (0..1000)
        .map(|i| format!("missing_{i}"))
        .collect::<Vec<_>>()
        .join("/");
    assert!(matches!(
        validator.validate(&deep),
        Err(SelfwareError::Safety(
            SafetyError::PathCanonicalizationFailed { .. }
        ))
    ));
    assert!(validator.validate("new/nested/module/file.rs").is_ok());
}

// ===== descriptor-level TOCTOU + FIFO tests =====

#[cfg(unix)]
mod fd_checked_io {
    use super::*;
    use std::os::unix::fs::symlink;

    /// `<root>/ws/sub/file.txt` (inside) and `<root>/outside/file.txt`.
    /// Only `ws/**` is allowed.
    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf, PathValidator) {
        let root = tempfile::tempdir().unwrap();
        let root_c = root.path().canonicalize().unwrap();
        let ws = root_c.join("ws");
        let outside = root_c.join("outside");
        std::fs::create_dir_all(ws.join("sub")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(ws.join("sub/file.txt"), "inside").unwrap();
        std::fs::write(outside.join("file.txt"), "SECRET").unwrap();
        let allowed = format!("{}/**", ws.display());
        let validator = PathValidator::new(&make_config(vec![&allowed], vec![]), ws.clone());
        (root, ws, outside, validator)
    }

    /// Replace the intermediate directory `ws/sub` with a symlink to `outside`.
    fn swap_sub_for_symlink(ws: &Path, outside: &Path) {
        std::fs::rename(ws.join("sub"), ws.join("sub_old")).unwrap();
        symlink(outside, ws.join("sub")).unwrap();
    }

    #[test]
    fn open_regular_file_reads_normal_file() {
        let (_root, ws, _outside, v) = setup();
        let mut vf = v
            .open_regular_file(&ws.join("sub/file.txt").to_string_lossy())
            .unwrap();
        let mut s = String::new();
        std::io::Read::read_to_string(&mut vf.file, &mut s).unwrap();
        assert_eq!(s, "inside");
    }

    #[test]
    fn open_regular_file_refuses_dir_swapped_after_validation() {
        let (_root, ws, outside, v) = setup();
        let target = ws.join("sub/file.txt");
        // The lexical path passes validation; the swap happens inside the
        // race window, before the descriptor is opened.
        assert!(v.validate(&target.to_string_lossy()).is_ok());
        let (ws2, out2) = (ws.clone(), outside.clone());
        set_after_lexical_validate_hook(Box::new(move || swap_sub_for_symlink(&ws2, &out2)));
        let err = v
            .open_regular_file(&target.to_string_lossy())
            .expect_err("descriptor resolved outside the allowed root must be refused");
        assert!(
            err.to_string().contains("not in allowed list"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn open_parent_dir_refuses_dir_swapped_after_validation() {
        let (_root, ws, outside, v) = setup();
        let target = ws.join("sub/new.txt");
        let (ws2, out2) = (ws.clone(), outside.clone());
        set_after_lexical_validate_hook(Box::new(move || swap_sub_for_symlink(&ws2, &out2)));
        assert!(v.open_parent_dir(&target.to_string_lossy(), true).is_err());
        assert!(!outside.join("new.txt").exists());
    }

    #[test]
    fn open_parent_dir_refuses_before_creating_under_swapped_ancestor() {
        let (_root, ws, outside, v) = setup();
        let target = ws.join("sub/deep/er/new.txt");
        let (ws2, out2) = (ws.clone(), outside.clone());
        set_after_lexical_validate_hook(Box::new(move || swap_sub_for_symlink(&ws2, &out2)));
        assert!(v.open_parent_dir(&target.to_string_lossy(), true).is_err());
        assert!(
            !outside.join("deep").exists(),
            "no directory may be created under a swapped-in ancestor"
        );
    }

    #[test]
    fn open_parent_dir_creates_missing_directories() {
        let (_root, ws, _outside, v) = setup();
        let target = ws.join("new/nested/dir/file.txt");
        let (_dir, name) = v.open_parent_dir(&target.to_string_lossy(), true).unwrap();
        assert_eq!(name, std::ffi::OsString::from("file.txt"));
        assert!(ws.join("new/nested/dir").is_dir());
    }

    #[test]
    fn fifo_is_rejected_without_hanging() {
        let (_root, ws, _outside, v) = setup();
        let fifo = ws.join("sub/pipe");
        let c = std::ffi::CString::new(fifo.to_string_lossy().as_bytes()).unwrap();
        // SAFETY: valid NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);

        let (tx, rx) = std::sync::mpsc::channel();
        let fifo_s = fifo.to_string_lossy().into_owned();
        std::thread::spawn(move || {
            let validated = v.validate(&fifo_s).is_ok();
            let opened = v
                .open_regular_file(&fifo_s)
                .map(|_| ())
                .map_err(|e| e.to_string());
            let _ = tx.send((validated, opened));
        });
        let (validated, opened) = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("validation/open of a FIFO must not block");
        assert!(
            validated,
            "a FIFO path inside the workspace is a valid path"
        );
        let err = opened.expect_err("a FIFO must be refused for file I/O");
        assert!(err.contains("FIFO"), "error should name the FIFO: {err}");
    }
}
