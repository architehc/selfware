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
fn test_untracked_escaping_symlink_is_rejected() {
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
        .args(["add", "."])
        .current_dir(rpath)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(rpath)
        .status();

    // 1. Escaping relative symlink
    let escaping_rel = rpath.join("escape_rel.txt");
    std::os::unix::fs::symlink(Path::new("../../etc/passwd"), &escaping_rel).unwrap();

    let result = CompilationSandbox::new(rpath);
    assert!(result.is_err(), "escaping symlink must be rejected");
    let err_msg = result.err().unwrap().to_string();
    assert!(
        err_msg.contains("targets path outside repository"),
        "error message should cite path outside repository: {err_msg}"
    );

    // Remove escaping relative symlink
    std::fs::remove_file(&escaping_rel).unwrap();

    // 2. Escaping absolute symlink
    let escaping_abs = rpath.join("escape_abs.txt");
    std::os::unix::fs::symlink(Path::new("/etc/passwd"), &escaping_abs).unwrap();

    let result_abs = CompilationSandbox::new(rpath);
    assert!(
        result_abs.is_err(),
        "absolute escaping symlink must be rejected"
    );
    let err_msg_abs = result_abs.err().unwrap().to_string();
    assert!(
        err_msg_abs.contains("targets path outside repository"),
        "error message should cite path outside repository: {err_msg_abs}"
    );
}

#[cfg(unix)]
#[test]
fn test_relative_dot_dot_escape_symlink_is_rejected() {
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
        .args(["add", "."])
        .current_dir(rpath)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(rpath)
        .status();

    let escaping_rel = rpath.join("escape_rel");
    std::os::unix::fs::symlink(Path::new("../.."), &escaping_rel).unwrap();
    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(rpath)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-m", "commit relative escaping symlink"])
        .current_dir(rpath)
        .status();

    let result = CompilationSandbox::new(rpath);
    assert!(
        result.is_err(),
        "relative ../.. escaping symlink must be rejected"
    );
    let err_msg = result.err().unwrap().to_string();
    assert!(
        err_msg.contains("targets path outside"),
        "error message should cite path outside sandbox: {err_msg}"
    );
}

#[cfg(unix)]
#[test]
fn test_untracked_symlink_chain_resolving_outside_is_rejected() {
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

    // Create an intermediate tracked symlink that points to /etc
    let bridge = rpath.join("bridge");
    std::os::unix::fs::symlink(Path::new("/etc"), &bridge).unwrap();

    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(rpath)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(rpath)
        .status();

    // Now create an untracked relative symlink that points into bridge: link.txt -> bridge/passwd
    let link = rpath.join("link.txt");
    std::os::unix::fs::symlink(Path::new("bridge/passwd"), &link).unwrap();

    let result = CompilationSandbox::new(rpath);
    assert!(
        result.is_err(),
        "symlink chain resolving outside sandbox must be rejected"
    );
}

#[test]
fn test_untracked_selfware_sandbox_prefix_user_file_is_preserved() {
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
        .args(["commit", "-m", "initial commit"])
        .current_dir(rpath)
        .status();

    // Create an untracked file that starts with .selfware-sandbox- (legit user file)
    let user_fixture = rpath.join(".selfware-sandbox-user-fixture.rs");
    std::fs::write(&user_fixture, b"// test fixture\n").unwrap();

    let sandbox = CompilationSandbox::new(rpath).expect("sandbox creation should succeed");
    assert!(
        sandbox
            .work_dir()
            .join(".selfware-sandbox-user-fixture.rs")
            .exists(),
        "untracked file sharing prefix with sandbox directory must not be dropped"
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

#[cfg(unix)]
#[test]
fn test_tracked_absolute_internal_symlink_rebase() {
    let temp_orig = tempfile::tempdir().unwrap();
    let orig_dir = temp_orig.path();
    let temp_work = tempfile::tempdir().unwrap();
    let work_dir = temp_work.path();

    let target_file = orig_dir.join("target.txt");
    std::fs::write(&target_file, b"content").unwrap();

    let work_target = work_dir.join("target.txt");
    std::fs::write(&work_target, b"content in sandbox").unwrap();

    // In sandbox, a symlink points absolutely to orig_dir/target.txt
    let work_link = work_dir.join("link.txt");
    std::os::unix::fs::symlink(&target_file, &work_link).unwrap();

    let res = rebase_and_validate_sandbox_symlinks(work_dir, orig_dir);
    assert!(
        res.is_ok(),
        "rebasing absolute internal symlink must succeed: {:?}",
        res.err()
    );

    let new_target = std::fs::read_link(&work_link).unwrap();
    assert!(
        new_target.is_relative(),
        "rebased link must be relative: {:?}",
        new_target
    );
    assert_eq!(
        std::fs::read_to_string(&work_link).unwrap(),
        "content in sandbox"
    );
}

#[cfg(unix)]
#[test]
fn test_tracked_symlink_chain_resolving_outside_is_rejected() {
    let temp_orig = tempfile::tempdir().unwrap();
    let orig_dir = temp_orig.path();
    let temp_work = tempfile::tempdir().unwrap();
    let work_dir = temp_work.path();

    // link1 -> link2 -> outside
    let outside = tempfile::tempdir().unwrap();
    let outside_file = outside.path().join("secret.txt");
    std::fs::write(&outside_file, b"secret").unwrap();

    let link2 = work_dir.join("link2.txt");
    std::os::unix::fs::symlink(&outside_file, &link2).unwrap();

    let link1 = work_dir.join("link1.txt");
    std::os::unix::fs::symlink("link2.txt", &link1).unwrap();

    let res = rebase_and_validate_sandbox_symlinks(work_dir, orig_dir);
    assert!(res.is_err(), "symlink chain resolving outside must fail");
    let err = res.unwrap_err().to_string();
    assert!(
        err.contains("outside sandbox"),
        "error must cite path outside sandbox: {err}"
    );
}

#[cfg(unix)]
#[test]
fn test_tracked_symlink_cycle_detected() {
    let temp_orig = tempfile::tempdir().unwrap();
    let orig_dir = temp_orig.path();
    let temp_work = tempfile::tempdir().unwrap();
    let work_dir = temp_work.path();

    // a -> b -> a
    let link_a = work_dir.join("link_a.txt");
    let link_b = work_dir.join("link_b.txt");
    std::os::unix::fs::symlink("link_b.txt", &link_a).unwrap();
    std::os::unix::fs::symlink("link_a.txt", &link_b).unwrap();

    let res = rebase_and_validate_sandbox_symlinks(work_dir, orig_dir);
    assert!(res.is_err(), "symlink cycle must be detected and rejected");
    let err = res.unwrap_err().to_string();
    assert!(
        err.contains("Symlink cycle detected in sandbox"),
        "error must cite cycle detection: {err}"
    );
}

#[cfg(unix)]
#[test]
fn test_tracked_symlink_hop_cap_exceeded() {
    let temp_orig = tempfile::tempdir().unwrap();
    let orig_dir = temp_orig.path();
    let temp_work = tempfile::tempdir().unwrap();
    let work_dir = temp_work.path();

    // Create a 35-hop chain: hop_0 -> hop_1 -> ... -> hop_34 -> target.txt
    let target = work_dir.join("target.txt");
    std::fs::write(&target, b"deep").unwrap();

    std::os::unix::fs::symlink("target.txt", work_dir.join("hop_34.txt")).unwrap();
    for i in (0..34).rev() {
        std::os::unix::fs::symlink(
            format!("hop_{}.txt", i + 1),
            work_dir.join(format!("hop_{}.txt", i)),
        )
        .unwrap();
    }

    let res = rebase_and_validate_sandbox_symlinks(work_dir, orig_dir);
    assert!(res.is_err(), "chain exceeding 32 hops must be rejected");
    let err = res.unwrap_err().to_string();
    assert!(
        err.contains("exceeded maximum hop depth (32)"),
        "error must cite hop depth exceeded: {err}"
    );
}
