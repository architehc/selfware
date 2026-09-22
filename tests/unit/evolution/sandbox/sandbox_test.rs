use super::*;

#[test]
fn test_copy_dir_recursive_excludes_target() {
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();

    // Create some files and a target/ dir that should be skipped.
    std::fs::write(src.path().join("Cargo.toml"), "[package]\n").unwrap();
    std::fs::create_dir_all(src.path().join("src")).unwrap();
    std::fs::write(src.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::create_dir_all(src.path().join("target")).unwrap();
    std::fs::write(src.path().join("target/should_not_copy.txt"), "nope\n").unwrap();

    copy_dir_recursive(src.path(), dst.path(), Some("target")).unwrap();

    // Normal files should be copied.
    assert!(dst.path().join("Cargo.toml").exists());
    assert!(dst.path().join("src/main.rs").exists());

    // target/ should NOT be copied.
    assert!(!dst.path().join("target").exists());
}

#[test]
fn test_parse_test_counts() {
    let output = "test result: ok. 5198 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out";
    let (passed, total) = parse_test_counts(output);
    assert_eq!(passed, 5198);
    assert_eq!(total, 5200);
}

#[test]
fn test_parse_memory_string() {
    assert_eq!(parse_memory_string("256.5MiB / 4GiB"), 268_959_744);
    assert_eq!(parse_memory_string("1.5GiB / 4GiB"), 1_610_612_736);
    assert_eq!(parse_memory_string("512KiB / 4GiB"), 524_288);
}

#[test]
fn test_sandbox_config_default() {
    let cfg = SandboxConfig::default();
    assert_eq!(cfg.cpus, "2");
    assert!(!cfg.network);
    assert_eq!(cfg.memory, "4g");
    assert_eq!(cfg.image, "selfware:latest");
    assert_eq!(cfg.timeout, Duration::from_secs(3600));
}

#[test]
fn test_parse_test_counts_no_result_line() {
    let output = "running 10 tests\ntest foo ... ok\ntest bar ... ok";
    let (passed, total) = parse_test_counts(output);
    assert_eq!(passed, 0);
    assert_eq!(total, 0);
}

#[test]
fn test_parse_test_counts_all_passed() {
    let output = "test result: ok. 100 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out";
    let (passed, total) = parse_test_counts(output);
    assert_eq!(passed, 100);
    assert_eq!(total, 100);
}

#[test]
fn test_parse_test_counts_all_failed() {
    let output = "test result: FAILED. 0 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out";
    let (passed, total) = parse_test_counts(output);
    assert_eq!(passed, 0);
    assert_eq!(total, 5);
}

#[test]
fn test_parse_test_counts_with_ignored() {
    let output = "test result: ok. 80 passed; 0 failed; 20 ignored; 0 measured; 0 filtered out";
    let (passed, total) = parse_test_counts(output);
    assert_eq!(passed, 80);
    assert_eq!(total, 100); // 80 + 0 + 20
}

#[test]
fn test_parse_test_counts_empty_output() {
    let (passed, total) = parse_test_counts("");
    assert_eq!(passed, 0);
    assert_eq!(total, 0);
}

#[test]
fn test_parse_memory_string_unknown_unit() {
    // "100B / 4GiB" — no recognized unit prefix → 0
    assert_eq!(parse_memory_string("100B / 4GiB"), 0);
}

#[test]
fn test_parse_memory_string_empty() {
    assert_eq!(parse_memory_string(""), 0);
}

#[test]
fn test_parse_memory_string_no_slash() {
    // "256MiB" without the " / limit" part
    let result = parse_memory_string("256MiB");
    let expected = (256.0 * 1024.0 * 1024.0) as u64;
    assert_eq!(result, expected);
}

#[test]
fn test_parse_test_counts_multiple_result_lines() {
    // Multiple test result lines — should use the last one (rev iteration)
    let output = "\
test result: ok. 50 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 100 passed; 2 failed; 3 ignored; 0 measured; 0 filtered out";
    let (passed, total) = parse_test_counts(output);
    // Last line: 100 passed, 2 failed, 3 ignored
    assert_eq!(passed, 100);
    assert_eq!(total, 105);
}

#[test]
fn test_sandbox_error_display() {
    assert!(format!("{}", SandboxError::DockerFailed("no docker".into())).contains("no docker"));
    assert!(format!("{}", SandboxError::ExecFailed("cmd failed".into())).contains("cmd failed"));
    assert!(format!("{}", SandboxError::IoError("disk full".into())).contains("disk full"));
    assert!(format!("{}", SandboxError::Timeout).contains("timed out"));
}

// ===========================================================================
// Container leak guard (2026-09-21 review, P2): sandboxes were only removed
// by an explicit `destroy(self)`, so an evaluation that errored — or any
// caller that early-returned — left its `docker run -d` sleep-infinity
// container running forever. `Sandbox` now cleans up on drop, idempotently
// with the explicit destroy. These tests prove `docker rm -f` is invoked
// with a PATH-scoped fake `docker` wrapper (records every invocation);
// nothing real is ever spawned, no window/docker interactions happen.
// ===========================================================================

use std::sync::OnceLock;

/// Serializes the PATH manipulation against any other test that spawns
/// processes: the fake `docker` sits FIRST on PATH, so it must not leak into
/// a concurrent test that happens to run git/cargo/the shell.
static DOCKER_PATH_LOCK: OnceLock<parking_lot::Mutex<()>> = OnceLock::new();

/// Create a temp dir with an executable `docker` stub that records each
/// invocation's argv as one line in `log_path`, and behaves like docker:
/// `run -d` prints a container id; everything else exits 0. Returns the bin
/// dir and a guard that scopes the PATH change to this test.
fn fake_docker_on_path(
    log_path: &std::path::Path,
    on_exec: &str,
) -> (
    tempfile::TempDir,
    crate::test_support::EnvGuard,
    parking_lot::MutexGuard<'static, ()>,
) {
    let lock = DOCKER_PATH_LOCK
        .get_or_init(|| parking_lot::Mutex::new(()))
        .lock();
    let bin = tempfile::tempdir().expect("bin tempdir");
    let script = bin.path().join("docker");
    // `exec` behavior is injected per test: "ok" (exit 0), "fail" (exit 125,
    // simulating a failed container step) or "missing" (the script deletes
    // itself, simulating the docker binary/daemon disappearing mid-run).
    let exec_body = match on_exec {
        "ok" => "",
        "fail" => "echo 'docker exec failed' >&2; exit 125",
        "missing" => "rm -f \"$0\"",
        other => panic!("unknown on_exec: {other}"),
    };
    let script_body = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"{}\"\ncase \"$1\" in\n  run)\n    echo \"fake-container-id\"\n    exit 0\n    ;;\n  exec)\n    {}\n    exit 125\n    ;;\n  *)\n    exit 0\n    ;;\nesac\n",
        log_path.display(),
        exec_body
    );
    std::fs::write(&script, script_body).expect("write fake docker");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let old_path = std::env::var_os("PATH").unwrap_or_default();
    let mut new_path = std::ffi::OsString::from(bin.path());
    new_path.push(":");
    new_path.push(old_path);
    std::env::set_var("PATH", new_path);
    let guard = crate::test_support::EnvGuard::capture(&["PATH"]);
    (bin, guard, lock)
}

fn recorded_invocations(log_path: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(log_path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

fn sandbox_without_creating(name: &str) -> Sandbox {
    Sandbox {
        container_id: "fake-container-id".to_string(),
        container_name: format!("selfware-arena-{name}"),
        config: SandboxConfig::default(),
        created_at: Instant::now(),
        _workspace_tmp: None,
        destroyed: false,
    }
}

#[test]
fn drop_without_destroy_removes_the_container() {
    // The leak: a sandbox dropped without an explicit destroy() — which is
    // exactly what an error early-return does — must run `docker rm -f`.
    let log_dir = tempfile::tempdir().unwrap();
    let log_path = log_dir.path().join("docker.log");
    let (_bin, _guard, _lock) = fake_docker_on_path(&log_path, "ok");
    {
        let _sb = sandbox_without_creating("leak-check");
        // Dropped here without destroy.
    }
    let invocations = recorded_invocations(&log_path);
    assert!(
        invocations
            .iter()
            .any(|i| i == "rm -f selfware-arena-leak-check"),
        "Drop must run docker rm -f; recorded: {invocations:?}"
    );
}

#[test]
fn error_path_drop_invokes_docker_rm_f() {
    // The scenario the finding names: an evaluation returns an error before
    // destroy(self) is ever called. Here the sandbox is created through the
    // REAL `Sandbox::create` path (the fake wrapper's `run -d` succeeds),
    // the evaluation then fails, and the caller propagates the error without
    // destroying — drop runs `docker rm -f`.
    let log_dir = tempfile::tempdir().unwrap();
    let log_path = log_dir.path().join("docker.log");
    let (_bin, _guard, _lock) = fake_docker_on_path(&log_path, "fail");
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(repo.path().join("Cargo.toml"), "[package]\nname=\"p\"\n").unwrap();

    let result = Sandbox::create("error-path", repo.path(), SandboxConfig::default());
    let sb = result.expect("create must succeed against the fake docker");

    // The evaluation path fails (compile step exits 125) — the caller treats
    // that as a failed/failed-over evaluation and propagates WITHOUT calling
    // destroy(); `sb` is dropped at the end of this scope.
    let eval = sb
        .evaluate()
        .expect("evaluate runs against the fake docker");
    assert!(!eval.compiled, "fake docker exec fails the compile step");
    drop(sb);

    let invocations = recorded_invocations(&log_path);
    assert!(
        invocations
            .iter()
            .any(|i| i == "rm -f selfware-arena-error-path"),
        "the guard must remove the container even though destroy() was never \
         called (evaluation errored/failed first); recorded: {invocations:?}"
    );
    // The container was created exactly once and removed exactly once.
    assert_eq!(
        invocations
            .iter()
            .filter(|i| i.starts_with("run -d"))
            .count(),
        1,
        "exactly one container creation: {invocations:?}"
    );
}

#[test]
fn explicit_destroy_then_drop_is_idempotent() {
    // destroy() and Drop share the same cleanup; a dropped-after-destroy
    // sandbox must not run `docker rm -f` twice.
    let log_dir = tempfile::tempdir().unwrap();
    let log_path = log_dir.path().join("docker.log");
    let (_bin, _guard, _lock) = fake_docker_on_path(&log_path, "ok");
    {
        let sb = sandbox_without_creating("idempotence");
        sb.destroy().expect("destroy succeeds");
        // Dropped here, AFTER the explicit destroy.
    }
    let invocations = recorded_invocations(&log_path);
    let rm_calls: Vec<_> = invocations
        .iter()
        .filter(|i| i == &"rm -f selfware-arena-idempotence")
        .collect();
    assert_eq!(
        rm_calls.len(),
        1,
        "docker rm -f must run exactly once; recorded: {invocations:?}"
    );
}
