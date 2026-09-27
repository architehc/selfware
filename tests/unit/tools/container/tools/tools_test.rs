use super::*;
use crate::tools::Tool;

// =========================================================================
// parse_build_output tests
// =========================================================================

#[test]
fn test_parse_build_output_successfully_built() {
    let stdout = "Step 3/3: COPY . /app\nSuccessfully built abc123def456";
    assert_eq!(
        parse_build_output(stdout, ""),
        Some("abc123def456".to_string())
    );
}

#[test]
fn test_parse_build_output_sha256() {
    let stderr = "writing image sha256:deadbeef01234567890";
    assert_eq!(
        parse_build_output("", stderr),
        Some("deadbeef01234567890".to_string())
    );
}

#[test]
fn test_parse_build_output_sha256_with_trailing_text() {
    let stderr = "writing image sha256:abc123 done";
    assert_eq!(parse_build_output("", stderr), Some("abc123".to_string()));
}

#[test]
fn test_parse_build_output_no_match() {
    assert_eq!(
        parse_build_output("just some log output", "another line"),
        None
    );
}

#[test]
fn test_parse_build_output_empty() {
    assert_eq!(parse_build_output("", ""), None);
}

#[test]
fn test_parse_build_output_in_stderr() {
    let stderr = "Step 1/3: FROM ubuntu\nStep 2/3: RUN apt-get update\nSuccessfully built xyz789";
    assert_eq!(parse_build_output("", stderr), Some("xyz789".to_string()));
}

#[test]
fn test_parse_build_output_prefers_first_match_in_stdout() {
    let stdout = "Successfully built first_id\nSuccessfully built second_id";
    assert_eq!(parse_build_output(stdout, ""), Some("first_id".to_string()));
}

// =========================================================================
// truncate_output tests
// =========================================================================

#[test]
fn test_truncate_output_short() {
    let short = "hello world";
    assert_eq!(truncate_output(short, 100), short);
}

#[test]
fn test_truncate_output_exact() {
    let s = "12345";
    assert_eq!(truncate_output(s, 5), "12345");
}

#[test]
fn test_truncate_output_long() {
    let long = "x".repeat(1000);
    let result = truncate_output(&long, 50);
    assert!(result.len() < 1000);
    assert!(result.contains("truncated"));
}

#[test]
fn test_truncate_output_empty() {
    assert_eq!(truncate_output("", 100), "");
}

// =========================================================================
// Tool name tests
// =========================================================================

#[test]
fn test_container_run_name() {
    assert_eq!(ContainerRun.name(), "container_run");
}

#[test]
fn test_container_stop_name() {
    assert_eq!(ContainerStop.name(), "container_stop");
}

#[test]
fn test_container_list_name() {
    assert_eq!(ContainerList.name(), "container_list");
}

#[test]
fn test_container_logs_name() {
    assert_eq!(ContainerLogs.name(), "container_logs");
}

#[test]
fn test_container_exec_name() {
    assert_eq!(ContainerExec.name(), "container_exec");
}

#[test]
fn test_container_build_name() {
    assert_eq!(ContainerBuild.name(), "container_build");
}

#[test]
fn test_container_images_name() {
    assert_eq!(ContainerImages.name(), "container_images");
}

#[test]
fn test_container_pull_name() {
    assert_eq!(ContainerPull.name(), "container_pull");
}

#[test]
fn test_container_remove_name() {
    assert_eq!(ContainerRemove.name(), "container_remove");
}

#[test]
fn test_compose_up_name() {
    assert_eq!(ComposeUp.name(), "compose_up");
}

// =========================================================================
// Tool description tests
// =========================================================================

#[test]
fn test_all_descriptions_non_empty() {
    assert!(!ContainerRun.description().is_empty());
    assert!(!ContainerStop.description().is_empty());
    assert!(!ContainerList.description().is_empty());
    assert!(!ContainerLogs.description().is_empty());
    assert!(!ContainerExec.description().is_empty());
    assert!(!ContainerBuild.description().is_empty());
    assert!(!ContainerImages.description().is_empty());
    assert!(!ContainerPull.description().is_empty());
    assert!(!ContainerRemove.description().is_empty());
    assert!(!ComposeUp.description().is_empty());
}

// =========================================================================
// Tool schema tests
// =========================================================================

#[test]
fn test_container_run_schema_has_image() {
    let schema = ContainerRun.schema();
    assert!(schema["properties"].get("image").is_some());
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&json!("image")));
}

#[test]
fn test_container_run_schema_has_ports() {
    let schema = ContainerRun.schema();
    assert!(schema["properties"].get("ports").is_some());
}

#[test]
fn test_container_run_schema_has_volumes() {
    let schema = ContainerRun.schema();
    assert!(schema["properties"].get("volumes").is_some());
}

#[test]
fn test_container_run_schema_has_env() {
    let schema = ContainerRun.schema();
    assert!(schema["properties"].get("env").is_some());
}

#[test]
fn test_container_stop_schema_has_container() {
    let schema = ContainerStop.schema();
    assert!(schema["properties"].get("container").is_some());
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&json!("container")));
}

#[test]
fn test_container_exec_schema_has_command() {
    let schema = ContainerExec.schema();
    assert!(schema["properties"].get("command").is_some());
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&json!("container")));
    assert!(required.contains(&json!("command")));
}

#[test]
fn test_container_build_schema_has_tag() {
    let schema = ContainerBuild.schema();
    assert!(schema["properties"].get("tag").is_some());
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&json!("tag")));
}

#[test]
fn test_container_pull_schema_has_image() {
    let schema = ContainerPull.schema();
    assert!(schema["properties"].get("image").is_some());
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&json!("image")));
}

#[test]
fn test_container_remove_schema_has_force() {
    let schema = ContainerRemove.schema();
    assert!(schema["properties"].get("force").is_some());
}

#[test]
fn test_container_logs_schema_has_tail() {
    let schema = ContainerLogs.schema();
    assert!(schema["properties"].get("tail").is_some());
    assert!(schema["properties"].get("since").is_some());
}

#[test]
fn test_compose_up_schema_has_path() {
    let schema = ComposeUp.schema();
    assert!(schema["properties"].get("path").is_some());
    assert!(schema["properties"].get("services").is_some());
}

// =========================================================================
// ContainerInfo serialization tests
// =========================================================================

#[test]
fn test_container_info_serialization() {
    let info = ContainerInfo {
        id: "abc123".to_string(),
        image: "nginx:latest".to_string(),
        command: "/docker-entrypoint.sh".to_string(),
        created: "2024-01-01".to_string(),
        status: "Up 5 minutes".to_string(),
        ports: "0.0.0.0:80->80/tcp".to_string(),
        names: "my-nginx".to_string(),
    };
    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("abc123"));
    assert!(json.contains("nginx:latest"));
    assert!(json.contains("my-nginx"));
}

#[test]
fn test_image_info_serialization() {
    let info = ImageInfo {
        id: "sha256:abc".to_string(),
        repository: "nginx".to_string(),
        tag: "latest".to_string(),
        created: "3 days ago".to_string(),
        size: "142MB".to_string(),
    };
    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("nginx"));
    assert!(json.contains("latest"));
    assert!(json.contains("142MB"));
}

// =========================================================================
// Runtime-specific schema field tests
// =========================================================================

#[test]
fn test_all_schemas_have_runtime_field() {
    let tools: Vec<Box<dyn Tool>> = vec![
        Box::new(ContainerRun),
        Box::new(ContainerStop),
        Box::new(ContainerList),
        Box::new(ContainerLogs),
        Box::new(ContainerExec),
        Box::new(ContainerBuild),
        Box::new(ContainerImages),
        Box::new(ContainerPull),
        Box::new(ContainerRemove),
        Box::new(ComposeUp),
    ];
    for tool in &tools {
        let schema = tool.schema();
        assert!(
            schema["properties"].get("runtime").is_some(),
            "Tool {} is missing runtime field in schema",
            tool.name()
        );
    }
}

// ---------------------------------------------------------------------------
// Option-shaped positional operands are refused before any runtime is probed
// or spawned (review, 0.9.2). `image: "--privileged"` used to yield a
// privileged container and `image: "--volume=/:/host"` bypassed
// validate_volume_spec and the Yolo volume guard.
// ---------------------------------------------------------------------------

async fn assert_refused_as_option(tool: &dyn Tool, args: serde_json::Value) {
    let err = tool
        .execute(args.clone())
        .await
        .expect_err("option-shaped operand must be refused");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("must be a name, not an option"),
        "{} {args}: {msg}",
        tool.name()
    );
}

#[tokio::test]
async fn container_run_refuses_option_shaped_image() {
    for image in [
        "--privileged",
        "--volume=/:/host",
        "-v/:/host",
        " --pid=host",
    ] {
        assert_refused_as_option(&ContainerRun, serde_json::json!({ "image": image })).await;
    }
}

#[tokio::test]
async fn container_operand_tools_refuse_option_shaped_names() {
    let exec_args = serde_json::json!({"container": "--privileged", "command": ["id"]});
    assert_refused_as_option(&ContainerExec, exec_args).await;
    assert_refused_as_option(
        &ContainerExec,
        serde_json::json!({"container": "--user=root", "command": ["id"]}),
    )
    .await;
    assert_refused_as_option(&ContainerStop, serde_json::json!({"container": "-t0"})).await;
    assert_refused_as_option(&ContainerLogs, serde_json::json!({"container": "--help"})).await;
    assert_refused_as_option(
        &ContainerRemove,
        serde_json::json!({"container": "--force"}),
    )
    .await;
    assert_refused_as_option(&ContainerPull, serde_json::json!({"image": "--all-tags"})).await;
    assert_refused_as_option(
        &ContainerBuild,
        serde_json::json!({"tag": "t", "path": "--file=/etc/passwd"}),
    )
    .await;
    assert_refused_as_option(
        &ContainerBuild,
        serde_json::json!({"tag": "t", "path": ".", "dockerfile": "-"}),
    )
    .await;
    assert_refused_as_option(
        &ComposeUp,
        serde_json::json!({"path": ".", "services": ["web", "--project-directory=/"]}),
    )
    .await;
    assert_refused_as_option(&ComposeUp, serde_json::json!({"path": ".", "file": "-"})).await;
    assert_refused_as_option(&ComposeDown, serde_json::json!({"path": "--x"})).await;
}

#[tokio::test]
async fn container_build_and_compose_refuse_paths_outside_policy() {
    for (tool, args) in [
        (
            &ContainerBuild as &dyn Tool,
            serde_json::json!({"tag": "t", "path": "/etc"}),
        ),
        (
            &ContainerBuild as &dyn Tool,
            serde_json::json!({"tag": "t", "path": ".", "dockerfile": "/etc/passwd"}),
        ),
        (&ComposeUp as &dyn Tool, serde_json::json!({"path": "/etc"})),
        (
            &ComposeDown as &dyn Tool,
            serde_json::json!({"path": ".", "file": "/etc/passwd"}),
        ),
    ] {
        let err = tool
            .execute(args.clone())
            .await
            .expect_err("path outside policy must be refused");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("outside the allowed paths"),
            "{} {args}: {msg}",
            tool.name()
        );
    }
}

// =========================================================================
// container_run cancelled mid-run: still recorded, drained at task/session end
// =========================================================================
//
// A stub `docker` on PATH (under the shared env lock) records its argv and
// hangs, like `docker run` attached to a container. The test drops the
// tool future (what `run_tool_bounded` does on Ctrl-C) before any cidfile
// exists, then drains with the scripted FakeDriver — no real docker.

#[cfg(unix)]
struct StubDocker {
    _env: crate::test_support::EnvGuard,
    _dir: tempfile::TempDir,
    args_file: std::path::PathBuf,
}

/// `write_cid`: write a container id into the `--cidfile` operand and exit
/// 0 (a normal detached run); otherwise hang without writing it.
#[cfg(unix)]
fn stub_docker(write_cid: Option<&str>) -> StubDocker {
    use std::os::unix::fs::PermissionsExt;
    let env = crate::test_support::EnvGuard::capture(&["PATH"]);
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args");
    let tail = match write_cid {
        Some(cid) => format!(
            "prev=\"\"\nfor a in \"$@\"; do\n  if [ \"$prev\" = \"--cidfile\" ]; then echo {cid} > \"$a\"; fi\n  prev=\"$a\"\ndone\necho {cid}\n"
        ),
        None => "sleep 60 &\nwait\n".to_string(),
    };
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}.tmp' && mv '{}.tmp' '{}'\n{tail}",
        args_file.display(),
        args_file.display(),
        args_file.display()
    );
    let stub = dir.path().join("docker");
    std::fs::write(&stub, script).unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    let old = std::env::var("PATH").unwrap_or_default();
    env.set("PATH", format!("{}:{old}", dir.path().display()));
    StubDocker {
        _env: env,
        _dir: dir,
        args_file,
    }
}

/// The `selfware.run=<value>` label the stub was invoked with.
#[cfg(unix)]
fn run_label_from(args_file: &std::path::Path) -> String {
    let args = std::fs::read_to_string(args_file).unwrap();
    args.lines()
        .find_map(|l| l.strip_prefix("selfware.run="))
        .expect("docker run carries a selfware.run label")
        .to_string()
}

#[cfg(unix)]
fn entry_with_run_label(run: &str) -> crate::resources::Resource {
    crate::resources::ResourceRegistry::global()
        .snapshot()
        .resources
        .into_iter()
        .find(|r| {
            matches!(&r.handle,
                crate::resources::ResourceHandle::Container { run_label: Some(l), .. } if l == run)
        })
        .expect("container_run registered its container")
}

/// Start `container_run` (attached, so the stub's hang is the container
/// running), wait until the stub was invoked, then drop the future.
#[cfg(unix)]
async fn cancel_container_run_mid_run(owner: Option<crate::resources::Owner>) -> String {
    let stub = stub_docker(None);
    let args = serde_json::json!({"image": "alpine", "detach": false, "runtime": "docker"});
    let run = ContainerRun.execute(args);
    let mut fut: std::pin::Pin<Box<dyn std::future::Future<Output = _>>> = match owner {
        Some(owner) => Box::pin(crate::resources::context::scope(owner, run)),
        None => Box::pin(run),
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        tokio::select! {
            res = &mut fut => panic!("container_run returned before it was cancelled: {res:?}"),
            _ = tokio::time::sleep(std::time::Duration::from_millis(25)) => {}
        }
        if stub.args_file.exists() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "stub docker never ran"
        );
    }
    drop(fut);
    run_label_from(&stub.args_file)
}

#[tokio::test]
#[cfg(unix)]
async fn cancelled_container_run_is_recorded_and_drained_at_task_end() {
    use crate::resources::fake::FakeDriver;
    use crate::resources::teardown::{teardown_task, TeardownPolicy};
    use crate::resources::{Owner, ResourceHandle, ResourceRegistry, ResourceState};

    let task = format!("t-cancel-{}", uuid::Uuid::new_v4().simple());
    let run = cancel_container_run_mid_run(Some(Owner::for_task(task.clone()))).await;

    let entry = entry_with_run_label(&run);
    assert_eq!(entry.owner_task, task);
    assert_eq!(entry.state, ResourceState::Starting, "{entry:?}");
    assert!(
        matches!(&entry.handle, ResourceHandle::Container { id, task_label, .. }
            if id.is_empty() && task_label == &task),
        "{entry:?}"
    );

    let driver = FakeDriver::new();
    driver
        .run_labels
        .lock()
        .unwrap()
        .insert(run.clone(), "c0ffee0123456789".into());
    let policy = TeardownPolicy {
        deadline: std::time::Duration::from_millis(40),
        force_grace: std::time::Duration::from_millis(40),
        poll: std::time::Duration::from_millis(5),
    };
    let report = teardown_task(ResourceRegistry::global(), &driver, &task, policy).await;

    assert_eq!(report.released.len(), 1, "{report:?}");
    assert!(report.leaked.is_empty(), "{report:?}");
    assert_eq!(
        driver.calls(),
        vec![
            format!("lookup:docker:{run}"),
            "polite:container c0ffee012345".to_string(),
            "finalize:container c0ffee012345".to_string(),
        ]
    );
    let after = ResourceRegistry::global().get(&entry.id).unwrap();
    assert_eq!(after.state, ResourceState::Released);
    assert!(
        matches!(&after.handle, ResourceHandle::Container { id, .. } if id == "c0ffee0123456789"),
        "the resolved id is recorded: {after:?}"
    );
}

#[tokio::test]
#[cfg(unix)]
async fn cancelled_container_run_outside_a_task_is_drained_at_session_end() {
    use crate::resources::context::session_owner;
    use crate::resources::fake::FakeDriver;
    use crate::resources::teardown::{teardown_session, TeardownPolicy};
    use crate::resources::{ResourceRegistry, ResourceState};

    let run = cancel_container_run_mid_run(None).await;
    let entry = entry_with_run_label(&run);
    assert_eq!(entry.owner_task, session_owner());
    assert_eq!(entry.state, ResourceState::Starting);

    // Session teardown on a registry holding just this entry (the global
    // one is shared with concurrent tests); same session id.
    let reg = ResourceRegistry::in_memory();
    reg.adopt(entry.clone());
    let driver = FakeDriver::new();
    driver
        .run_labels
        .lock()
        .unwrap()
        .insert(run.clone(), "feedface01234567".into());
    let policy = TeardownPolicy {
        deadline: std::time::Duration::from_millis(40),
        force_grace: std::time::Duration::from_millis(40),
        poll: std::time::Duration::from_millis(5),
    };
    let report = teardown_session(&reg, &driver, policy).await;
    assert_eq!(report.released.len(), 1, "{report:?}");
    assert!(driver
        .calls()
        .contains(&"polite:container feedface0123".to_string()));
    assert_eq!(reg.get(&entry.id).unwrap().state, ResourceState::Released);
}

#[tokio::test]
#[cfg(unix)]
async fn container_run_that_returns_records_the_cidfile_id_as_live() {
    use crate::resources::{Owner, ResourceHandle, ResourceState};
    let stub = stub_docker(Some("abcabc0123456789"));
    let task = format!("t-live-{}", uuid::Uuid::new_v4().simple());
    let out = crate::resources::context::scope(
        Owner::for_task(task.clone()),
        ContainerRun.execute(serde_json::json!({"image": "alpine", "runtime": "docker"})),
    )
    .await
    .unwrap();
    assert_eq!(out["success"], true, "{out}");
    let entry = entry_with_run_label(&run_label_from(&stub.args_file));
    assert_eq!(entry.state, ResourceState::Live);
    assert_eq!(entry.owner_task, task);
    assert!(
        matches!(&entry.handle, ResourceHandle::Container { id, .. } if id == "abcabc0123456789"),
        "{entry:?}"
    );
}
