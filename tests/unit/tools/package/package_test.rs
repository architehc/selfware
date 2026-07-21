use super::*;

#[test]
fn test_npm_install_schema() {
    let tool = NpmInstall;
    let schema = tool.schema();
    assert!(schema.get("properties").is_some());
    assert!(schema["properties"].get("packages").is_some());
}

#[test]
fn install_tools_expose_timeout_secs() {
    // The hang-prevention timeout must be a real, discoverable arg so the
    // model (and operators) can override the default bound.
    for schema in [NpmInstall.schema(), PipInstall.schema()] {
        assert!(
            schema["properties"].get("timeout_secs").is_some(),
            "install tool should expose timeout_secs: {schema}"
        );
    }
}

#[test]
fn test_npm_run_schema() {
    let tool = NpmRun;
    let schema = tool.schema();
    assert!(schema.get("required").is_some());
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .contains(&json!("script")));
}

#[test]
fn test_pip_install_schema() {
    let tool = PipInstall;
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
    assert_eq!(NpmInstall.name(), "npm_install");
    assert_eq!(NpmRun.name(), "npm_run");
    assert_eq!(NpmScripts.name(), "npm_scripts");
    assert_eq!(PipInstall.name(), "pip_install");
    assert_eq!(PipList.name(), "pip_list");
    assert_eq!(PipFreeze.name(), "pip_freeze");
    assert_eq!(YarnInstall.name(), "yarn_install");
}

#[test]
fn test_tool_descriptions() {
    assert!(!NpmInstall.description().is_empty());
    assert!(!NpmRun.description().is_empty());
    assert!(!PipInstall.description().is_empty());
    assert!(PipInstall.description().contains("pip"));
}

#[tokio::test]
async fn test_npm_scripts_no_package_json() {
    let tool = NpmScripts;
    let result = tool.execute(json!({"path": "/nonexistent/path"})).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_pip_install_no_packages() {
    let tool = PipInstall;
    let result = tool.execute(json!({})).await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("must be specified"));
}

#[tokio::test]
#[cfg(unix)]
async fn npm_install_timeout_kills_child_process() {
    // Regression: `Command::output()` timeouts used to leave the child
    // running — a "timed-out" install kept mutating the tree afterwards.
    // The child must be killed (kill_on_drop) when the timeout fires.
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("npm.pid");
    let stub = dir.path().join("npm");
    std::fs::write(
        &stub,
        format!("#!/bin/sh\necho $$ > {}\nsleep 60\n", pidfile.display()),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();

    // Prepend the stub dir to PATH so `npm` resolves to our hanging stub;
    // restore PATH immediately after the call, before any assertions.
    let old_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{}", dir.path().display(), old_path));
    let tool = NpmInstall;
    let start = std::time::Instant::now();
    let result = tool
        .execute(json!({"packages": ["express"], "timeout_secs": 1}))
        .await;
    std::env::set_var("PATH", &old_path);

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
    // Verify the child is really killed instead of still sleeping for the
    // next minute. A SIGKILL'd child is "no longer running" the moment it
    // dies — but under coverage instrumentation (tarpaulin ptraces every
    // process) the runtime's reaping is delayed, so the killed child
    // lingers as an unreaped ZOMBIE. `kill(pid, 0)` returns Ok for a
    // zombie (it still occupies a slot in the process table), so an
    // existence check alone is fooled. Treat "gone" OR "zombie/dead" as
    // successfully killed; only a genuinely running (sleeping) process
    // counts as alive.
    fn pid_is_running(pid: i32) -> bool {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;
        if kill(Pid::from_raw(pid), None).is_err() {
            return false; // gone / already reaped
        }
        #[cfg(target_os = "linux")]
        {
            // /proc/<pid>/stat: "pid (comm) STATE ...". A zombie ('Z') or
            // dead ('X'/'x') process has been killed, just not yet reaped.
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
        if !pid_is_running(child_pid) {
            alive = false;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    assert!(!alive, "timed-out npm child pid {child_pid} must be killed");
}
