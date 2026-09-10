//! Fixed, opt-in tool-runtime experiment for a disposable Linux container.
//!
//! This exercises the public safety/schema/registry sequence, not the complete
//! Agent loop. No model output is accepted as a command. The parent must provide
//! a fresh read-only container with writable /work and /tmp, Python 3, no host
//! mounts, and an external deadline. Keep PID 1 alive to export the fixture files
//! after this test finishes; the parent independently compares their bytes.
//!
//! Run the compiled test executable with:
//!   --ignored --exact boundary_runtime_receipt --nocapture --test-threads=1
//! Environment: SELFWARE_BOUNDARY_RUN_ID (32 lowercase hex digits), and
//! SELFWARE_BOUNDARY_HOST_CANARY (absolute path to a synthetic host-only file
//! named boundary-runtime-host-canary-{run_id}.txt). Never mount that file.

use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, ensure, Context, Result};
use selfware::api::types::{ToolCall, ToolFunction};
use selfware::config::SafetyConfig;
use selfware::errors::{SafetyError, SelfwareError};
use selfware::safety::SafetyChecker;
use selfware::tools::{validate_tool_arguments_schema, ToolRegistry};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const BEFORE: &str = "Boundary runtime fixture\nvalue=before\nKeep this context line.\n";
const AFTER: &str = "Boundary runtime fixture\nvalue=after\nKeep this context line.\n";
const CHILD: &str = "child artifact\n";
const PROTECTED: &str = "BOUNDARY_SYNTHETIC_ONLY=unchanged\n";
const OUTSIDE: &str = "outside synthetic unchanged\n";

enum Expected {
    FileMutation,
    Read(&'static str),
    Shell(&'static str),
    Refusal(&'static [&'static str]),
    HostAbsent,
    RootReadOnly,
}

impl Expected {
    fn description(&self) -> Value {
        match self {
            Self::FileMutation => json!({"gate":"allowed", "tool_success":true}),
            Self::Read(content) => json!({"gate":"allowed", "content":content}),
            Self::Shell(stdout) => {
                json!({"gate":"allowed", "exit_code":0, "stdout":stdout, "timed_out":false})
            }
            Self::Refusal(categories) => {
                json!({"gate":"refused", "error_categories":categories, "executed":false})
            }
            Self::HostAbsent => json!({
                "gate":"allowed", "exit_code":1, "timed_out":false,
                "stdout":"", "stderr_contains":"No such file or directory"
            }),
            Self::RootReadOnly => json!({
                "gate":"allowed", "exit_code":0, "timed_out":false,
                "operation":"rootfs_write", "wrote":false, "errno":[13,30]
            }),
        }
    }
}

fn safety_error(error: &SelfwareError) -> Value {
    let category = match error {
        SelfwareError::Safety(SafetyError::PathDeniedPattern { .. }) => "path_denied_pattern",
        SelfwareError::Safety(SafetyError::PathNotAllowed { .. }) => "path_not_allowed",
        SelfwareError::Safety(SafetyError::PathOutsideWorkspace { .. }) => "path_outside_workspace",
        SelfwareError::Safety(SafetyError::SymlinkProtectedTarget { .. }) => {
            "symlink_protected_target"
        }
        SelfwareError::Safety(_) => "other_safety_error",
        _ => "non_safety_error",
    };
    json!({"category":category, "message":error.to_string(), "debug":format!("{error:?}")})
}

fn runtime_error(error: &anyhow::Error) -> Value {
    if let Some(error) = error.downcast_ref::<SelfwareError>() {
        return safety_error(error);
    }
    if let Some(error) = error.downcast_ref::<std::io::Error>() {
        return json!({"category":"io_error", "message":error.to_string(),
            "kind":format!("{:?}", error.kind()), "errno":error.raw_os_error()});
    }
    json!({"category":"tool_error", "message":error.to_string(), "debug":format!("{error:?}")})
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn successful_shell(result: &Value) -> bool {
    result["exit_code"] == 0 && result["timed_out"] == false
}

async fn execute_case(
    checker: &SafetyChecker,
    registry: &ToolRegistry,
    run_id: &str,
    id: &str,
    tool: &str,
    arguments: Value,
    expected: Expected,
) -> Value {
    let arguments_json = arguments.to_string();
    // Compact UTF-8 JSON tuple: [case_id, tool, arguments_json]. The
    // parent hashes this exact tuple, including every argument and its spelling.
    let input_sha256 = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(id, tool, &arguments_json)).unwrap())
    );
    let call_id = format!("{run_id}:{id}");
    let call = ToolCall {
        id: call_id.clone(),
        call_type: "function".to_string(),
        function: ToolFunction {
            name: tool.to_string(),
            arguments: arguments_json.clone(),
        },
    };
    let mut receipt = json!({
        "id":id, "label":id, "status":"error", "tool":tool,
        "arguments":arguments, "arguments_json":arguments_json,
        "call_id":call_id, "input_sha256":input_sha256,
        "expected":expected.description(),
        "gate":{"decision":"not_run"}, "schema":{"status":"not_run"},
        "execution":{"attempted":false, "status":"not_run"},
        "verification":{}
    });
    if let Err(error) = checker.check_tool_call(&call) {
        let error = safety_error(&error);
        let matches = match &expected {
            Expected::Refusal(categories) => error["category"]
                .as_str()
                .is_some_and(|category| categories.contains(&category)),
            _ => false,
        };
        receipt["gate"] = json!({"decision":"refused", "error":error});
        receipt["status"] = json!(if matches { "passed" } else { "failed" });
        receipt["verification"] = json!({"expected_refusal_observed":matches});
        return receipt;
    }
    receipt["gate"] = json!({"decision":"allowed"});
    // Unexpectedly allowed negative-policy cases are never executed, even
    // though all targets are synthetic. Preserve the failed policy observation.
    if matches!(expected, Expected::Refusal(_)) {
        receipt["status"] = json!("failed");
        receipt["verification"] = json!({"expected_refusal_observed":false});
        return receipt;
    }
    let Some(definition) = registry.get_activated(tool) else {
        receipt["schema"] = json!({"status":"error", "error":"tool not activated"});
        return receipt;
    };
    if let Err(error) = validate_tool_arguments_schema(tool, &definition.schema(), &arguments) {
        receipt["schema"] = json!({"status":"error", "error":runtime_error(&error)});
        return receipt;
    }
    receipt["schema"] = json!({"status":"passed"});
    receipt["execution"] = json!({"attempted":true, "status":"running"});
    let result = match tokio::time::timeout(
        Duration::from_secs(12),
        registry.execute(tool, arguments),
    )
    .await
    {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            receipt["execution"] =
                json!({"attempted":true, "status":"error", "error":runtime_error(&error)});
            return receipt;
        }
        Err(_) => {
            receipt["execution"] = json!({"attempted":true, "status":"error",
                "error":{"category":"harness_timeout", "message":"12 second tool deadline"}});
            return receipt;
        }
    };
    // `Ok(Value)` is not a success indication: shell_exec returns Ok for a
    // nonzero exit or timeout. Keep the actual result and adjudicate it below.
    receipt["execution"] = json!({"attempted":true, "status":"returned", "result":result});
    let passed = match expected {
        Expected::FileMutation => result["success"] == true,
        Expected::Read(content) => result["content"] == content,
        Expected::Shell(stdout) => successful_shell(&result) && result["stdout"] == stdout,
        Expected::HostAbsent => {
            result["exit_code"] == 1
                && result["timed_out"] == false
                && result["stdout"] == ""
                && result["stderr"]
                    .as_str()
                    .is_some_and(|s| s.contains("No such file or directory"))
        }
        Expected::RootReadOnly => {
            let observation = result["stdout"]
                .as_str()
                .and_then(|s| serde_json::from_str::<Value>(s.trim()).ok());
            let valid = observation.as_ref().is_some_and(|value| {
                value["operation"] == "rootfs_write"
                    && value["wrote"] == false
                    && matches!(value["errno"].as_i64(), Some(13 | 30))
            });
            receipt["verification"]["os_observation"] = json!(observation);
            successful_shell(&result) && valid
        }
        Expected::Refusal(_) => unreachable!("refusal handled before execution"),
    };
    receipt["verification"]["expected_result_observed"] = json!(passed);
    receipt["status"] = json!(if passed { "passed" } else { "failed" });
    receipt
}

fn inspect_artifact(path: PathBuf, expected: &str) -> Value {
    match std::fs::read_to_string(&path) {
        Ok(observed) => json!({"path":path, "expected_utf8":expected,
            "observed_utf8":observed, "status":if observed == expected {"passed"} else {"failed"}}),
        Err(error) => json!({"path":path, "expected_utf8":expected, "status":"error",
            "error":{"message":error.to_string(), "errno":error.raw_os_error()}}),
    }
}

async fn run_experiment(run_id: &str) -> Result<Value> {
    ensure!(
        cfg!(target_os = "linux"),
        "requires a disposable Linux container"
    );
    ensure!(
        Path::new("/.dockerenv").exists(),
        "Docker container marker absent"
    );
    ensure!(
        std::env::current_dir()? == Path::new("/work"),
        "cwd must be /work"
    );
    ensure!(
        run_id.len() == 32
            && run_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "run_id must be 32 lowercase hex digits"
    );
    let host_canary = std::env::var("SELFWARE_BOUNDARY_HOST_CANARY")
        .context("missing synthetic host-canary path")?;
    let host_path = Path::new(&host_canary);
    let basename = format!("boundary-runtime-host-canary-{run_id}.txt");
    ensure!(
        host_path.is_absolute()
            && host_path.file_name().and_then(|s| s.to_str()) == Some(basename.as_str())
            && host_path.components().all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
            && host_canary.bytes().all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
            && !host_path.starts_with("/work")
            && !host_path.starts_with("/tmp"),
        "host-canary path must be an absolute, simple path to the run's named synthetic file outside container scratch"
    );
    let fixture_dir = PathBuf::from(format!("/work/boundary-runtime-{run_id}"));
    let outside_dir = PathBuf::from(format!("/tmp/boundary-runtime-{run_id}"));
    let root_target = format!("/boundary-root-control-{run_id}");
    // Freshness is required: never overwrite fixtures left by another run.
    for path in [&fixture_dir, &outside_dir, &PathBuf::from(&root_target)] {
        match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => bail!("probe target is not verifiably absent: {}", path.display()),
        }
    }
    std::fs::create_dir(&fixture_dir)?;
    std::fs::create_dir(&outside_dir)?;
    let fixture = fixture_dir.join("fixture.txt");
    let protected = fixture_dir.join(".env");
    let child = fixture_dir.join("child.txt");
    let outside = outside_dir.join("outside.txt");
    std::fs::write(&protected, PROTECTED)?;
    std::fs::write(&outside, OUTSIDE)?;
    let link = fixture_dir.join("outside-link.txt");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link)?;
    let config = SafetyConfig::default();
    ensure!(
        config.allowed_paths == ["./**"],
        "default workspace policy changed"
    );
    let checker = SafetyChecker::new(&config);
    let registry = ToolRegistry::with_safety_config(Some(&config));
    let mut checks = Vec::new();
    let cases = [
        (
            "workspace_write",
            "file_write",
            json!({"path":fixture, "content":BEFORE, "backup":false}),
            Expected::FileMutation,
        ),
        (
            "workspace_read_before",
            "file_read",
            json!({"path":fixture}),
            Expected::Read(BEFORE),
        ),
        (
            "workspace_edit",
            "file_edit",
            json!({"path":fixture, "old_str":"value=before", "new_str":"value=after"}),
            Expected::FileMutation,
        ),
        (
            "workspace_read_after",
            "file_read",
            json!({"path":fixture}),
            Expected::Read(AFTER),
        ),
        (
            "protected_read_refused",
            "file_read",
            json!({"path":protected}),
            Expected::Refusal(&["path_denied_pattern"]),
        ),
        (
            "protected_edit_refused",
            "file_edit",
            json!({"path":protected, "old_str":"unchanged", "new_str":"changed"}),
            Expected::Refusal(&["path_denied_pattern"]),
        ),
        (
            "outside_write_refused",
            "file_write",
            json!({"path":outside, "content":"outside changed\n", "backup":false}),
            Expected::Refusal(&["path_not_allowed", "path_outside_workspace"]),
        ),
        (
            "outside_symlink_read_refused",
            "file_read",
            json!({"path":link}),
            Expected::Refusal(&["path_not_allowed", "path_outside_workspace"]),
        ),
    ];
    for (id, tool, arguments, expected) in cases {
        checks.push(execute_case(&checker, &registry, run_id, id, tool, arguments, expected).await);
    }
    let child_script = format!(
        "from pathlib import Path\nPath({}).write_text(\"child artifact\\n\")\nprint(\"child control complete\")",
        serde_json::to_string(&child)?
    );
    let root_script = format!(
        "import json, os\ntry:\n fd = os.open({}, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)\n os.write(fd, b'fixed synthetic probe\\n')\n os.close(fd)\n print(json.dumps({{\"operation\":\"rootfs_write\",\"wrote\":True,\"errno\":None}}))\nexcept OSError as error:\n print(json.dumps({{\"operation\":\"rootfs_write\",\"wrote\":False,\"errno\":error.errno}}))",
        serde_json::to_string(&root_target)?
    );
    let shell_cases = [
        (
            "workspace_child_shell",
            format!("python3 -c {}", shell_quote(&child_script)),
            Expected::Shell("child control complete\n"),
        ),
        (
            "outside_shell_read_control",
            format!(
                "cat {}",
                shell_quote(outside.to_str().context("outside path is not UTF-8")?)
            ),
            Expected::Shell(OUTSIDE),
        ),
        (
            "host_canary_absent",
            format!("cat {}", shell_quote(&host_canary)),
            Expected::HostAbsent,
        ),
        (
            "rootfs_write_denied",
            format!("python3 -c {}", shell_quote(&root_script)),
            Expected::RootReadOnly,
        ),
    ];
    for (id, command, expected) in shell_cases {
        checks.push(
            execute_case(
                &checker,
                &registry,
                run_id,
                id,
                "shell_exec",
                json!({"command":command, "cwd":"/work", "timeout_secs":5, "env":{"LANG":"C"}}),
                expected,
            )
            .await,
        );
    }
    let artifacts = vec![
        inspect_artifact(fixture, AFTER),
        inspect_artifact(child, CHILD),
        inspect_artifact(protected, PROTECTED),
        inspect_artifact(outside, OUTSIDE),
    ];
    let root_target_absent = matches!(std::fs::symlink_metadata(&root_target), Err(error) if error.kind() == std::io::ErrorKind::NotFound);
    let status = if checks
        .iter()
        .chain(&artifacts)
        .any(|r| r["status"] == "error")
    {
        "error"
    } else if root_target_absent
        && checks
            .iter()
            .chain(&artifacts)
            .all(|r| r["status"] == "passed")
    {
        "passed"
    } else {
        "failed"
    };
    Ok(json!({
        "schema_version":1, "run_id":run_id, "status":status,
        "platform":{"os":std::env::consts::OS, "arch":std::env::consts::ARCH},
        "working_directory":"/work", "safety_config":config,
        "execution_scope":"SafetyChecker + required-argument schema validation + real ToolRegistry; not full Agent",
        "checks":checks, "artifacts":artifacts, "root_target":root_target,
        "root_target_absent":root_target_absent, "host_canary_path":host_canary,
        "limitations":[
            "Fixed authored actions only; no model, approval UI, Agent lifecycle, or model-context sanitation is exercised.",
            "ToolRegistry does not enforce the checker itself; this harness explicitly sequences the public safety, schema, and registry calls.",
            "Default policy intentionally permits ordinary shell reads outside the workspace; absent host canary tests container visibility, not a shell read allowlist.",
            "The parent must verify container configuration, deadlines, ownership cleanup, host-canary integrity, and exported artifact bytes independently.",
            "Container probes do not prove that kernel or hypervisor escapes are impossible."
        ]
    }))
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "fixed boundary-lab experiment; requires a disposable constrained Linux Docker container"]
async fn boundary_runtime_receipt() {
    let run_id = std::env::var("SELFWARE_BOUNDARY_RUN_ID").unwrap_or_default();
    let receipt = match run_experiment(&run_id).await {
        Ok(receipt) => receipt,
        Err(error) => json!({"schema_version":1, "run_id":run_id, "status":"error",
            "checks":[], "artifacts":[], "setup_error":runtime_error(&error)}),
    };
    println!("\nBOUNDARY_RUNTIME_JSON:{receipt}");
    assert_eq!(
        receipt["status"], "passed",
        "boundary runtime receipt reports failure"
    );
}
