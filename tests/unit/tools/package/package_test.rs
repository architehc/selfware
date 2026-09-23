use super::*;

#[test]
fn test_npm_install_schema() {
    let tool = NpmInstall::new();
    let schema = tool.schema();
    assert!(schema.get("properties").is_some());
    assert!(schema["properties"].get("packages").is_some());
}

#[test]
fn install_tools_expose_timeout_secs() {
    // The hang-prevention timeout must be a real, discoverable arg so the
    // model (and operators) can override the default bound.
    for schema in [NpmInstall::new().schema(), PipInstall::new().schema()] {
        assert!(
            schema["properties"].get("timeout_secs").is_some(),
            "install tool should expose timeout_secs: {schema}"
        );
    }
}

#[test]
fn test_npm_run_schema() {
    let tool = NpmRun::new();
    let schema = tool.schema();
    assert!(schema.get("required").is_some());
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .contains(&json!("script")));
}

#[test]
fn test_pip_install_schema() {
    let tool = PipInstall::new();
    let schema = tool.schema();
    assert!(schema["properties"].get("packages").is_some());
    assert!(schema["properties"].get("requirements").is_some());
}

#[test]
fn test_parse_npm_install_output() {
    let stdout = "added 5 packages in 2s";
    let stderr = "";
    let result = parse_npm_install_output(stdout, stderr);
    assert!(!result.is_empty());
    assert!(result[0].contains("added"));
}

#[test]
fn test_parse_npm_install_output_with_plus() {
    let stdout = "+ express@4.18.2\n+ lodash@4.17.21";
    let stderr = "";
    let result = parse_npm_install_output(stdout, stderr);
    assert_eq!(result.len(), 2);
}

#[test]
fn test_parse_pip_install_output() {
    let stdout = "Collecting requests\nSuccessfully installed requests-2.28.0 urllib3-1.26.0";
    let result = parse_pip_install_output(stdout);
    assert_eq!(result.len(), 2);
    assert!(result.contains(&"requests-2.28.0".to_string()));
}

#[test]
fn test_parse_pip_install_already_satisfied() {
    let stdout = "Requirement already satisfied: requests in /usr/lib/python3/dist-packages";
    let result = parse_pip_install_output(stdout);
    assert_eq!(result.len(), 1);
    assert!(result[0].contains("already installed"));
}

#[test]
fn test_truncate_output_short() {
    let output = "short output";
    assert_eq!(truncate_output(output, 100), output);
}

#[test]
fn test_truncate_output_long() {
    let output = "a".repeat(200);
    let result = truncate_output(&output, 50);
    assert!(result.contains("truncated"));
    assert!(result.contains("200 total chars"));
}

#[test]
fn test_tool_names() {
    assert_eq!(NpmInstall::new().name(), "npm_install");
    assert_eq!(NpmRun::new().name(), "npm_run");
    assert_eq!(NpmScripts::new().name(), "npm_scripts");
    assert_eq!(PipInstall::new().name(), "pip_install");
    assert_eq!(PipList.name(), "pip_list");
    assert_eq!(PipFreeze::new().name(), "pip_freeze");
    assert_eq!(YarnInstall::new().name(), "yarn_install");
}

#[test]
fn test_tool_descriptions() {
    assert!(!NpmInstall::new().description().is_empty());
    assert!(!NpmRun::new().description().is_empty());
    assert!(!PipInstall::new().description().is_empty());
    assert!(PipInstall::new().description().contains("pip"));
}

#[tokio::test]
async fn test_npm_scripts_no_package_json() {
    let _state = policy_guard();
    let tool = NpmScripts::new();
    // An in-workspace path (not an out-of-workspace one) exercises the
    // missing-package.json error; out-of-workspace paths are now refused
    // by the path policy before any filesystem access.
    let result = tool.execute(json!({"path": "no-such-dir-anywhere"})).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_pip_install_no_packages() {
    let tool = PipInstall::new();
    let result = tool.execute(json!({})).await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("must be specified"));
}

/// Hold the SHARED process-state lock and snapshot `PATH` (restored on drop,
/// including on panic). The old file-local `PATH_MUTEX` only serialized these
/// two tests against each other: every other env/cwd-mutating test in the lib
/// (which all take `test_support::state_lock`) could still run concurrently —
/// changing the cwd that `validate_tool_path(".")` resolves against, or
/// rewriting `PATH` so `npm` no longer resolved to the stub.
fn path_env_guard() -> crate::test_support::EnvGuard {
    crate::test_support::EnvGuard::capture(&["PATH"])
}

/// Poll `path` until it holds a parseable pid or `deadline` passes. The stub
/// writes it asynchronously; a fixed number of short sleeps flaked under load.
#[cfg(unix)]
async fn wait_for_pid(path: &std::path::Path, deadline: std::time::Instant) -> Option<i32> {
    loop {
        if let Some(pid) = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
        {
            return Some(pid);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

#[tokio::test]
#[cfg(unix)]
async fn npm_install_timeout_kills_child_process() {
    let env = path_env_guard();
    // Regression: `Command::output()` timeouts used to leave the child
    // running — a "timed-out" install kept mutating the tree afterwards.
    // The child must be killed (kill_on_drop) when the timeout fires.
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("npm.pid");
    let sleep_pidfile = dir.path().join("sleep.pid");
    let stub = dir.path().join("npm");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\necho $$ > {}\nsleep 60 &\necho $! > {}\nwait\n",
            pidfile.display(),
            sleep_pidfile.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();

    // Prepend the stub dir to PATH so `npm` resolves to our hanging stub;
    // restore PATH immediately after the call, before any assertions (the
    // guard restores it again on drop/panic).
    let old_path = std::env::var("PATH").unwrap_or_default();
    env.set("PATH", format!("{}:{}", dir.path().display(), old_path));
    let tool = NpmInstall::new();
    let start = std::time::Instant::now();
    // 3s (not 1s): the stub must get far enough to record both pids before
    // the timeout kills it; under a loaded full-suite run a 1s budget could
    // expire before `sh` even started, leaving no pidfile to check.
    let result = tool
        .execute(json!({"packages": ["express"], "timeout_secs": 3}))
        .await;
    env.set("PATH", &old_path);

    assert!(start.elapsed().as_secs() < 15, "must return at the timeout");
    let err = result.expect_err("a hung install must time out");
    assert!(
        err.to_string().contains("timed out"),
        "unexpected error: {err}"
    );

    let child_pid: i32 = std::fs::read_to_string(&pidfile)
        .expect("stub wrote its pid")
        .trim()
        .parse()
        .expect("valid pid");
    let sleep_pid: i32 = std::fs::read_to_string(&sleep_pidfile)
        .expect("stub wrote descendant pid")
        .trim()
        .parse()
        .expect("valid descendant pid");

    fn pid_is_running(pid: i32) -> bool {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;
        if kill(Pid::from_raw(pid), None).is_err() {
            return false; // gone / already reaped
        }
        #[cfg(target_os = "linux")]
        {
            if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
                let state = stat
                    .rsplit(')')
                    .next()
                    .and_then(|rest| rest.split_whitespace().next());
                if matches!(state, Some("Z") | Some("X") | Some("x")) {
                    return false;
                }
            }
        }
        true
    }

    let mut alive = true;
    for _ in 0..75 {
        if !pid_is_running(child_pid) && !pid_is_running(sleep_pid) {
            alive = false;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    assert!(
        !alive,
        "timed-out npm child {child_pid} and descendant {sleep_pid} must be killed"
    );
}

#[tokio::test]
#[cfg(unix)]
async fn npm_install_future_cancellation_reaps_process_group() {
    let env = path_env_guard();
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("npm.pid");
    let sleep_pidfile = dir.path().join("sleep.pid");
    let stub = dir.path().join("npm");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\necho $$ > {}\nsleep 60 &\necho $! > {}\nwait\n",
            pidfile.display(),
            sleep_pidfile.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();

    let old_path = std::env::var("PATH").unwrap_or_default();
    env.set("PATH", format!("{}:{}", dir.path().display(), old_path));
    let tool = NpmInstall::new();

    let mut fut = Box::pin(tool.execute(json!({"packages": ["express"], "timeout_secs": 60})));

    // Drive the install until the stub has recorded BOTH pids, bounded by a
    // generous deadline (was 50 x 50ms = 2.5s, too tight under load).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let pids = async {
        let child = wait_for_pid(&pidfile, deadline).await;
        let sleeper = wait_for_pid(&sleep_pidfile, deadline).await;
        (child, sleeper)
    };
    let (child_pid, sleep_pid) = tokio::select! {
        pids = pids => pids,
        res = &mut fut => panic!("install finished before it could be cancelled: {res:?}"),
    };
    env.set("PATH", &old_path);

    let child_pid: i32 = child_pid.expect("stub wrote its pid");
    let sleep_pid: i32 = sleep_pid.expect("stub wrote descendant pid");

    // Drop the pinned Box future mid-flight to simulate agent task cancellation
    drop(fut);

    fn pid_is_running(pid: i32) -> bool {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;
        if kill(Pid::from_raw(pid), None).is_err() {
            return false;
        }
        #[cfg(target_os = "linux")]
        {
            if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
                let state = stat
                    .rsplit(')')
                    .next()
                    .and_then(|rest| rest.split_whitespace().next());
                if matches!(state, Some("Z") | Some("X") | Some("x")) {
                    return false;
                }
            }
        }
        true
    }

    let mut alive = true;
    for _ in 0..75 {
        if !pid_is_running(child_pid) && !pid_is_running(sleep_pid) {
            alive = false;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(
        !alive,
        "both npm parent {child_pid} and descendant {sleep_pid} should be killed on future drop"
    );
}

// ── Path-policy enforcement (2026-09-21 review sweep) ────────────────────
//
// Package-manager tools take path operands (install cwd, requirements
// file, freeze output); each must obey the workspace path policy BEFORE
// spawning the child or writing the file. The policy refusal surfaces as a
// path error, not "npm/pip failed".

/// Path-policy tests resolve operands against the process cwd and the
/// process-global `SAFETY_CONFIG`. Both are shared state: a concurrent
/// cwd-switching test (under the shared lock) or an agent test that left a
/// permissive global config behind (`Agent::new` -> `init_safety_config`)
/// made these refusals flake under `--test-threads 64`. Hold the shared state
/// lock (stable cwd) and start from the default safety config.
fn policy_guard() -> crate::test_support::CwdGuard {
    let guard = crate::test_support::CwdGuard::hold();
    crate::tools::file::reset_safety_config_for_tests();
    guard
}

fn is_path_policy_error(msg: &str) -> bool {
    msg.contains("not allowed")
        || msg.contains("not in allowed")
        || msg.contains("outside working")
        || msg.contains("protected")
        || msg.contains("denied pattern")
}

#[tokio::test]
async fn test_pip_freeze_output_file_policy() {
    let _state = policy_guard();
    let tool = PipFreeze::new();
    let err = tool
        .execute(json!({"output_file": "/etc/freeze-out.txt"}))
        .await
        .unwrap_err();
    assert!(
        is_path_policy_error(&err.to_string()),
        "pip_freeze output_file outside the workspace must be refused, got: {err}"
    );
}

#[tokio::test]
async fn test_npm_install_path_policy() {
    let _state = policy_guard();
    let tool = NpmInstall::new();
    let err = tool
        .execute(json!({"packages": ["express"], "path": "/etc"}))
        .await
        .unwrap_err();
    assert!(
        is_path_policy_error(&err.to_string()),
        "npm_install path outside the workspace must be refused, got: {err}"
    );
}

#[tokio::test]
async fn test_pip_install_requirements_policy() {
    let _state = policy_guard();
    let tool = PipInstall::new();
    let err = tool
        .execute(json!({"requirements": "/etc/requirements.txt"}))
        .await
        .unwrap_err();
    assert!(
        is_path_policy_error(&err.to_string()),
        "pip_install requirements outside the workspace must be refused, got: {err}"
    );
}

#[tokio::test]
async fn test_yarn_install_path_policy() {
    let _state = policy_guard();
    let tool = YarnInstall::new();
    let err = tool
        .execute(json!({"packages": ["x"], "path": "/etc"}))
        .await
        .unwrap_err();
    assert!(
        is_path_policy_error(&err.to_string()),
        "yarn_install path outside the workspace must be refused, got: {err}"
    );
}

#[tokio::test]
async fn test_npm_run_path_policy() {
    let _state = policy_guard();
    let tool = NpmRun::new();
    let err = tool
        .execute(json!({"script": "test", "path": "/etc"}))
        .await
        .unwrap_err();
    assert!(
        is_path_policy_error(&err.to_string()),
        "npm_run path outside the workspace must be refused, got: {err}"
    );
}
