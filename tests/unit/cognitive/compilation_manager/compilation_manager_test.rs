use super::*;
use std::process::{Command, Output};

/// Helper: create a CompilationSandbox struct directly for parse_output testing
/// without actually cloning a git repo (which is expensive and requires a real repo).
fn dummy_sandbox() -> CompilationSandbox {
    CompilationSandbox {
        _original_dir: PathBuf::from("/tmp/dummy-original"),
        work_dir: PathBuf::from("/tmp/dummy-sandbox"),
        owns_work_dir: false,
    }
}

/// Helper: produce an Output from running a simple command with known exit code.
fn make_output(exit_code: i32, stdout: &str, stderr: &str) -> Output {
    // We construct Output by running a real shell command that gives us
    // the exact stdout, stderr, and exit code we want.
    let cmd_str = format!(
        "printf '{}'; printf '{}' >&2; exit {}",
        stdout.replace('\'', "'\\''"),
        stderr.replace('\'', "'\\''"),
        exit_code
    );
    Command::new("sh")
        .arg("-c")
        .arg(&cmd_str)
        .output()
        .expect("failed to run helper command")
}

// --- parse_output tests ---

#[test]
fn test_parse_output_success() {
    let sandbox = dummy_sandbox();
    let output = make_output(0, "all good", "");
    let result = sandbox.parse_output(output).unwrap();
    assert!(result.success);
    assert_eq!(result.stdout, "all good");
    assert!(result.stderr.is_empty());
}

#[test]
fn test_parse_output_failure() {
    let sandbox = dummy_sandbox();
    let output = make_output(1, "", "error: something broke");
    let result = sandbox.parse_output(output).unwrap();
    assert!(!result.success);
    assert!(result.stdout.is_empty());
    assert_eq!(result.stderr, "error: something broke");
}

#[test]
fn test_parse_output_empty() {
    let sandbox = dummy_sandbox();
    let output = make_output(0, "", "");
    let result = sandbox.parse_output(output).unwrap();
    assert!(result.success);
    assert!(result.stdout.is_empty());
    assert!(result.stderr.is_empty());
}

#[test]
fn test_parse_output_mixed_stdout_stderr() {
    let sandbox = dummy_sandbox();
    let output = make_output(0, "compiled OK", "warning: unused variable");
    let result = sandbox.parse_output(output).unwrap();
    assert!(result.success);
    assert_eq!(result.stdout, "compiled OK");
    assert_eq!(result.stderr, "warning: unused variable");
}

#[test]
fn test_parse_output_nonzero_exit_with_both_streams() {
    let sandbox = dummy_sandbox();
    let output = make_output(42, "partial output", "fatal error");
    let result = sandbox.parse_output(output).unwrap();
    assert!(!result.success);
    assert_eq!(result.stdout, "partial output");
    assert_eq!(result.stderr, "fatal error");
}

// --- CompilationSandbox::new with nonexistent path ---

#[test]
fn test_new_with_nonexistent_path() {
    let result = CompilationSandbox::new("/tmp/nonexistent-selfware-test-path-abc123xyz");
    // Should fail because git clone from a nonexistent dir will fail
    assert!(result.is_err());
}

// --- cleanup with nonexistent dir (should not panic) ---

#[test]
fn test_cleanup_nonexistent_dir_no_panic() {
    let sandbox = CompilationSandbox {
        _original_dir: PathBuf::from("/tmp/does-not-exist-original"),
        work_dir: PathBuf::from("/tmp/does-not-exist-sandbox-xyz123"),
        owns_work_dir: false,
    };
    // cleanup checks `self.work_dir.exists()` before removing, so this should be Ok
    let result = sandbox.cleanup();
    assert!(result.is_ok());
}

#[test]
fn test_drop_does_not_remove_unowned_dir() {
    let temp_dir = tempfile::tempdir().unwrap();
    let sub = temp_dir.path().join("preserved_dir");
    std::fs::create_dir_all(&sub).unwrap();
    assert!(sub.exists());

    {
        let _unowned = CompilationSandbox {
            _original_dir: temp_dir.path().to_path_buf(),
            work_dir: sub.clone(),
            owns_work_dir: false,
        };
    } // dropped here

    assert!(
        sub.exists(),
        "Directory should not be removed when owns_work_dir is false"
    );
}

// --- CompileResult fields ---

#[test]
fn test_compile_result_debug() {
    let result = CompileResult {
        success: true,
        stdout: "ok".to_string(),
        stderr: String::new(),
    };
    // Verify Debug is derived
    let debug_str = format!("{:?}", result);
    assert!(debug_str.contains("CompileResult"));
    assert!(debug_str.contains("true"));
}

#[test]
fn test_untracked_files_with_spaces_and_size_bounds() {
    let repo_dir = tempfile::tempdir().unwrap();
    let rpath = repo_dir.path();

    let init_ok = Command::new("git")
        .args(["init"])
        .current_dir(rpath)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !init_ok {
        return;
    }
    let _ = Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(rpath)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(rpath)
        .status();

    // Commit an initial file
    std::fs::write(rpath.join("README.md"), b"# Test Repo\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "README.md"])
        .current_dir(rpath)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(rpath)
        .status();

    // Add untracked file with spaces in filename
    let spaced_file = "my untracked file.txt";
    std::fs::write(rpath.join(spaced_file), b"untracked content").unwrap();

    let sandbox = CompilationSandbox::new(rpath).expect("sandbox creation should succeed");
    assert_eq!(
        std::fs::read_to_string(sandbox.work_dir().join(spaced_file)).unwrap(),
        "untracked content"
    );
    drop(sandbox);

    // Add an oversized untracked file (> 10MB)
    let large_file = rpath.join("large_blob.bin");
    let file = std::fs::File::create(&large_file).unwrap();
    file.set_len(10 * 1024 * 1024 + 1).unwrap();

    let result = CompilationSandbox::new(rpath);
    assert!(
        result.is_err(),
        "should reject untracked file exceeding 10MB"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("exceeds size limit"),
        "error message should cite size limit: {err_msg}"
    );
}

#[cfg(unix)]
#[test]
fn test_untracked_symlinks_are_replicated() {
    let repo_dir = tempfile::tempdir().unwrap();
    let rpath = repo_dir.path();

    let init_ok = Command::new("git")
        .args(["init"])
        .current_dir(rpath)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !init_ok {
        return;
    }
    let _ = Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(rpath)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(rpath)
        .status();

    // Commit an initial file
    std::fs::write(rpath.join("README.md"), b"# Test Repo\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "README.md"])
        .current_dir(rpath)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(rpath)
        .status();

    // Create untracked symlink
    let symlink_path = rpath.join("link_to_readme.md");
    std::os::unix::fs::symlink(Path::new("README.md"), &symlink_path).unwrap();

    let sandbox = CompilationSandbox::new(rpath).expect("sandbox creation should succeed");
    let sandbox_symlink = sandbox.work_dir().join("link_to_readme.md");
    assert!(
        sandbox_symlink
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink(),
        "untracked symlink must be replicated as a symlink"
    );
    assert_eq!(
        std::fs::read_link(&sandbox_symlink).unwrap(),
        PathBuf::from("README.md")
    );
    assert_eq!(
        std::fs::read_to_string(&sandbox_symlink).unwrap(),
        "# Test Repo\n"
    );
}

#[cfg(unix)]
#[test]
fn test_untracked_escaping_symlink_is_skipped_with_warn() {
    let repo_dir = tempfile::tempdir().unwrap();
    let rpath = repo_dir.path();

    let init_ok = Command::new("git")
        .args(["init"])
        .current_dir(rpath)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !init_ok {
        return;
    }
    let _ = Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(rpath)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(rpath)
        .status();

    std::fs::write(rpath.join("README.md"), b"# Test Repo\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "README.md"])
        .current_dir(rpath)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(rpath)
        .status();

    // 1. Escaping relative symlink
    let escaping_rel = rpath.join("escape_rel.txt");
    std::os::unix::fs::symlink(Path::new("../../etc/passwd"), &escaping_rel).unwrap();

    // 2. Escaping absolute symlink
    let escaping_abs = rpath.join("escape_abs.txt");
    std::os::unix::fs::symlink(Path::new("/etc/passwd"), &escaping_abs).unwrap();

    // Sandbox creation succeeds via skip-with-warn (does not abort entire sandbox)
    let sandbox =
        CompilationSandbox::new(rpath).expect("escaping symlinks should be skipped with warning");
    assert!(
        !sandbox.work_dir().join("escape_rel.txt").exists(),
        "escaping relative symlink must be skipped from sandbox"
    );
    assert!(
        !sandbox.work_dir().join("escape_abs.txt").exists(),
        "escaping absolute symlink must be skipped from sandbox"
    );
    assert!(
        sandbox.work_dir().join("README.md").exists(),
        "valid repository files must still be present in sandbox"
    );
}

#[cfg(unix)]
#[test]
fn test_untracked_internal_absolute_symlink_is_rewritten_to_relative() {
    let repo_dir = tempfile::tempdir().unwrap();
    let rpath = repo_dir.path();

    let init_ok = Command::new("git")
        .args(["init"])
        .current_dir(rpath)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !init_ok {
        return;
    }
    let _ = Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(rpath)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(rpath)
        .status();

    std::fs::write(rpath.join("README.md"), b"# Test Repo\n").unwrap();
    let sub = rpath.join("subdir");
    std::fs::create_dir(&sub).unwrap();
    std::fs::write(sub.join("subfile.txt"), b"sub content\n").unwrap();

    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(rpath)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(rpath)
        .status();

    // Create untracked absolute symlink pointing inside repository
    let abs_target = rpath.join("README.md");
    let link_in_sub = sub.join("link_to_root.txt");
    std::os::unix::fs::symlink(&abs_target, &link_in_sub).unwrap();

    let sandbox = CompilationSandbox::new(rpath).expect("sandbox creation should succeed");
    let sandbox_link = sandbox.work_dir().join("subdir/link_to_root.txt");
    assert!(
        sandbox_link
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink(),
        "link must be replicated as symlink"
    );

    let raw_target = std::fs::read_link(&sandbox_link).unwrap();
    assert_eq!(
        raw_target,
        PathBuf::from("../README.md"),
        "absolute internal target must be rewritten to relative inside sandbox"
    );
    assert_eq!(
        std::fs::read_to_string(&sandbox_link).unwrap(),
        "# Test Repo\n"
    );
}

#[test]
fn test_untracked_aggregate_size_cap_exceeded() {
    let repo_dir = tempfile::tempdir().unwrap();
    let rpath = repo_dir.path();

    let init_ok = Command::new("git")
        .args(["init"])
        .current_dir(rpath)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !init_ok {
        return;
    }
    let _ = Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(rpath)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(rpath)
        .status();

    std::fs::write(rpath.join("README.md"), b"# Test\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(rpath)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(rpath)
        .status();

    // Create 6 untracked files of 9 MB each (total 54 MB > 50 MB cap)
    for i in 0..6 {
        let f = std::fs::File::create(rpath.join(format!("big_{i}.bin"))).unwrap();
        f.set_len(9 * 1024 * 1024).unwrap();
    }

    let result = CompilationSandbox::new(rpath);
    assert!(
        result.is_err(),
        "aggregate size > 50 MB must fail sandbox creation"
    );
    let err = result.err().unwrap().to_string();
    assert!(
        err.contains("Aggregate untracked file size limit exceeded"),
        "error message should cite aggregate limit: {err}"
    );
}

#[test]
fn test_symlink_target_containment_logic() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path();
    let sub = base.join("subdir");
    std::fs::create_dir(&sub).unwrap();

    let link_in_sub = sub.join("link.txt");
    let link_in_base = base.join("link.txt");

    // Internal relative: same dir
    assert!(symlink_target_is_contained(
        base,
        &link_in_sub,
        Path::new("sibling.txt")
    ));
    // Internal relative: parent dir inside repo
    assert!(symlink_target_is_contained(
        base,
        &link_in_sub,
        Path::new("../root_file.txt")
    ));
    // Escaping relative: escaping repo root
    assert!(!symlink_target_is_contained(
        base,
        &link_in_sub,
        Path::new("../../outside.txt")
    ));
    assert!(!symlink_target_is_contained(
        base,
        &link_in_base,
        Path::new("../outside.txt")
    ));
    // Absolute external
    assert!(!symlink_target_is_contained(
        base,
        &link_in_base,
        Path::new("/etc/passwd")
    ));
}
