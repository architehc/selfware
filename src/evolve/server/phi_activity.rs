//! Read-only, bounded activity projection. Captures are observations, never a
//! declaration that a workspace is verified. No command, source or prompt is
//! returned to the browser.

use super::{require_session, ApiResult, EvolveServer};
use axum::{extract::State, http::HeaderMap, routing::get, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_SCAN: usize = 256;
const MAX_AGENTS: usize = 64;
const MAX_FILE: u64 = 256 * 1024;
const MAX_TOTAL: u64 = 4 * 1024 * 1024;
const STALE_MS: u64 = 5 * 60 * 1000;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Running,
    Completed,
    Failed,
    Partial,
    Abandoned,
}

impl Phase {
    fn label(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Partial => "partial",
            Self::Abandoned => "abandoned",
        }
    }
}

#[derive(Deserialize)]
struct Capture {
    schema_version: u32,
    agent_id: String,
    task_id: String,
    session_id: String,
    workspace_root: String,
    recorded_at_ms: u64,
    phase: Phase,
    evidence: Option<crate::agent::turn_artifacts::EvidenceSnapshot>,
    #[serde(default)]
    observation_truncated: bool,
}

pub(super) fn routes() -> Router<Arc<EvolveServer>> {
    Router::new().route("/api/phi/activity", get(activity))
}

async fn activity(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    require_session(&headers, &server)?;
    let root = server.project_root.clone();
    let now = now_ms();
    let body = tokio::task::spawn_blocking(move || read_activity(&root, now))
        .await
        .unwrap_or_else(|_| empty(now, "capture_read_failed"));
    Ok(Json(body))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn empty(now: u64, reason: &str) -> Value {
    json!({"status":"unavailable","reason":reason,"observed_at_ms":now,
        "agents":[],"truncated":false})
}

fn opaque(value: &str) -> String {
    format!("{:x}", Sha256::digest(value))[..24].to_string()
}

// Pin each directory with openat + O_NOFOLLOW so replacing a parent with a
// symlink during a poll cannot redirect the read to protected workspace data.
#[cfg(unix)]
fn open_child(parent: &File, name: &std::ffi::OsStr, directory: bool) -> std::io::Result<File> {
    use nix::libc;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    let name = std::ffi::CString::new(name.as_bytes())?;
    let flags = libc::O_RDONLY
        | libc::O_CLOEXEC
        | libc::O_NOFOLLOW
        | libc::O_NONBLOCK
        | if directory { libc::O_DIRECTORY } else { 0 };
    // SAFETY: the directory fd remains owned by `parent`, and name is a valid
    // NUL-terminated component. A successful new fd is transferred once.
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

#[cfg(not(unix))]
fn read_activity(_root: &Path, now: u64) -> Value {
    empty(now, "safe_capture_reader_unavailable")
}

#[cfg(unix)]
fn read_activity(root: &Path, now: u64) -> Value {
    let Ok(root) = root.canonicalize() else {
        return empty(now, "workspace_unavailable");
    };
    let Ok(mut directory) = File::open(&root) else {
        return empty(now, "workspace_unavailable");
    };
    let mut path = root.clone();
    for part in [".selfware", "phi", "activity"] {
        directory = match open_child(&directory, part.as_ref(), true) {
            Ok(dir) => dir,
            Err(error) => {
                return empty(
                    now,
                    if error.kind() == std::io::ErrorKind::NotFound {
                        "no_capture"
                    } else {
                        "unsafe_capture"
                    },
                )
            }
        };
        path.push(part);
    }
    let Ok(entries) = std::fs::read_dir(&path) else {
        return empty(now, "capture_read_failed");
    };
    let mut json_files = Vec::new();
    let mut truncated = false;
    let mut rejected = false;
    let now_time = std::time::SystemTime::now();

    for entry in entries {
        let Ok(entry) = entry else {
            rejected = true;
            continue;
        };
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();

        // Ignore temporary files
        if name_str.starts_with(".tmp-") {
            continue;
        }

        if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let mtime = entry.metadata().ok().and_then(|m| m.modified().ok());
        json_files.push((mtime, file_name));
    }

    if json_files.len() > MAX_SCAN {
        truncated = true;
    }
    // Two-pass: sort all json receipts descending by modified time BEFORE capping
    json_files.sort_by_key(|b| std::cmp::Reverse(b.0));

    // Cap candidate scan without deleting files during read
    if json_files.len() > MAX_SCAN {
        json_files.truncate(MAX_SCAN);
    }

    // Retention policy: filter expired receipts older than 24 hours in memory (non-destructive)
    const RETENTION_SECS: u64 = 24 * 60 * 60;
    json_files.retain(|(mtime, _file_name)| {
        if let Some(mt) = mtime {
            if let Ok(age) = now_time.duration_since(*mt) {
                if age.as_secs() > RETENTION_SECS {
                    return false;
                }
            }
        }
        true
    });

    let mut candidates = Vec::new();
    for (_, file_name) in json_files.into_iter().take(MAX_SCAN) {
        let Ok(file) = open_child(&directory, &file_name, false) else {
            rejected = true;
            continue;
        };
        let Ok(metadata) = file.metadata() else {
            rejected = true;
            continue;
        };
        use std::os::unix::fs::MetadataExt;
        if !metadata.is_file() || metadata.nlink() != 1 || metadata.len() > MAX_FILE {
            rejected = true;
            continue;
        }
        candidates.push((metadata.modified().ok(), file));
    }
    let mut agents = Vec::new();
    let mut total = 0;
    for (_, file) in candidates {
        if total >= MAX_TOTAL {
            truncated = true;
            break;
        }
        let mut bytes = Vec::new();
        let cap = (MAX_TOTAL - total).min(MAX_FILE + 1);
        let read = file.take(cap).read_to_end(&mut bytes);
        total += bytes.len() as u64;
        if read.is_err() {
            rejected = true;
            continue;
        }
        if bytes.len() as u64 > MAX_FILE {
            rejected = true;
            continue;
        }
        let Ok(capture) = serde_json::from_slice::<Capture>(&bytes) else {
            rejected = true;
            continue;
        };
        if capture.schema_version != 1
            || Path::new(&capture.workspace_root) != root
            || [&capture.agent_id, &capture.task_id, &capture.session_id]
                .iter()
                .any(|id| id.is_empty() || id.len() > 128)
        {
            rejected = true;
            continue;
        }
        agents.push(summarize(capture, now));
    }
    agents.sort_by_key(|row| std::cmp::Reverse(row["recorded_at_ms"].as_u64().unwrap_or(0)));
    // Retained task receipts from one session are not separate live agents.
    let mut identities = std::collections::HashSet::new();
    agents.retain(|row| identities.insert(row["agent_id"].as_str().unwrap_or("").to_owned()));
    if agents.len() > MAX_AGENTS {
        agents.truncate(MAX_AGENTS);
        truncated = true;
    }
    let (status, reason) = if rejected || truncated {
        (
            "incomplete",
            Some(if truncated {
                "capture_limit"
            } else {
                "invalid_capture"
            }),
        )
    } else if agents.is_empty() {
        ("unavailable", Some("no_capture"))
    } else {
        ("available", None)
    };
    json!({"status":status,"reason":reason,"observed_at_ms":now,
        "agents":agents,"truncated":truncated})
}

fn summarize(capture: Capture, now: u64) -> Value {
    let age = now.saturating_sub(capture.recorded_at_ms);
    let mut status = "available";
    let mut reason = None;
    let evidence = capture.evidence.map(|e| {
        let runs: Vec<_> = e
            .observations
            .iter()
            .filter(|r| matches!(r.kind.as_str(), "run_finished" | "opaque_run"))
            .collect();
        let passed = runs
            .iter()
            .filter(|r| r.outcome.as_deref() == Some("Passed"))
            .count();
        let failed = runs
            .iter()
            .filter(|r| r.outcome.as_deref() == Some("Failed"))
            .count();
        if e.unknown_size_obligations > 0
            || !e.unattributed.is_empty()
            || e.possible_unrecorded_mutations > 0
            || capture.observation_truncated
            || runs.len() != passed + failed
        {
            status = "incomplete";
            reason = Some("uncertain_evidence");
        }
        let latest_run = runs.last().and_then(|r| match r.outcome.as_deref() {
            Some("Passed") => Some("passed"),
            Some("Failed") => Some("failed"),
            _ => None,
        });
        json!({"outstanding":e.outstanding,"unreviewed_lines":e.unreviewed_lines,
            "untested_lines":e.untested_lines,"unknown_size_obligations":e.unknown_size_obligations,
            "unattributed_mutations":e.unattributed.len(),
            "possible_unrecorded_mutations":e.possible_unrecorded_mutations,
            "observed_runs":runs.len(),"passed_runs":passed,"failed_runs":failed,
            "unknown_runs":runs.len()-passed-failed,"latest_run":latest_run})
    });
    if evidence.is_none() {
        status = "incomplete";
        reason = Some("evidence_unavailable");
    }
    if age > STALE_MS {
        status = "stale";
        reason = Some("capture_expired");
    }
    if capture.recorded_at_ms > now.saturating_add(5000) {
        status = "incomplete";
        reason = Some("capture_in_future");
    }
    json!({"agent_id":opaque(&format!("{}\0{}", capture.session_id, capture.agent_id)),"task_id":opaque(&capture.task_id),
        "session_id":opaque(&capture.session_id),"status":status,"reason":reason,
        "recorded_at_ms":capture.recorded_at_ms,"age_ms":age,
        "phase":capture.phase.label(),"evidence":evidence})
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use tower::ServiceExt;

    fn capture(root: &Path, id: usize, now: u64) -> Value {
        json!({"schema_version":1,"agent_id":format!("agent-{id}"),
            "task_id":format!("task-{id}"),"session_id":format!("session-{id}"),
            "workspace_root":root.canonicalize().unwrap(),"recorded_at_ms":now,
            "phase":"failed","evidence":{"outstanding":2,"unreviewed_lines":8,
                "untested_lines":8,"unknown_size_obligations":0,"unattributed":[],
                "possible_unrecorded_mutations":0,"citations":["PRIVATE SOURCE"],
                "observations":[{"turn":1,"tool":"shell_exec","command":"SECRET COMMAND",
                    "kind":"run_finished","outcome":"Failed","may_have_mutated":false,
                    "reason":null,"call_id":"secret-call"}]}})
    }

    fn write(root: &Path, id: usize, value: &Value) -> std::path::PathBuf {
        let dir = root.join(".selfware/phi/activity");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{id}.json"));
        std::fs::write(&path, serde_json::to_vec(value).unwrap()).unwrap();
        path
    }

    #[tokio::test]
    async fn activity_requires_session_and_reports_absence() {
        let root = tempfile::tempdir().unwrap();
        let server = EvolveServer::for_project(
            crate::evolve::Graph {
                nodes: vec![],
                edges: vec![],
            },
            root.path(),
        )
        .unwrap();
        for authorized in [false, true] {
            let mut request = axum::http::Request::builder().uri("/api/phi/activity");
            if authorized {
                request = request.header(super::super::SESSION_HEADER, server.session_token());
            }
            let response = server
                .router()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), if authorized { 200 } else { 401 });
            let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
            if authorized {
                let body: Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(body["status"], "unavailable");
                assert_eq!(body["reason"], "no_capture");
                assert_eq!(body["agents"], json!([]));
            }
        }
    }

    #[test]
    fn sixteen_agents_preserve_failures_without_exposing_raw_capture_text() {
        let root = tempfile::tempdir().unwrap();
        for id in 0..16 {
            write(root.path(), id, &capture(root.path(), id, 1000 + id as u64));
        }
        let body = read_activity(root.path(), 2000);
        assert_eq!(body["status"], "available");
        assert_eq!(body["agents"].as_array().unwrap().len(), 16);
        assert_eq!(body["agents"][0]["recorded_at_ms"], 1015);
        for row in body["agents"].as_array().unwrap() {
            assert_eq!(row["phase"], "failed");
            assert_eq!(row["evidence"]["failed_runs"], 1);
            assert_eq!(row["evidence"]["passed_runs"], 0);
            assert_eq!(row["evidence"]["outstanding"], 2);
        }
        let public = body.to_string();
        for private in [
            "PRIVATE SOURCE",
            "SECRET COMMAND",
            "secret-call",
            "task-0",
            "agent-0",
        ] {
            assert!(!public.contains(private));
        }
        assert!(!public.contains(root.path().to_str().unwrap()));
    }

    #[test]
    fn stale_missing_and_uncertain_evidence_are_never_current_complete() {
        let root = tempfile::tempdir().unwrap();
        let mut missing = capture(root.path(), 0, 1000);
        missing["evidence"] = Value::Null;
        write(root.path(), 0, &missing);
        let mut uncertain = capture(root.path(), 1, 1000);
        uncertain["evidence"]["possible_unrecorded_mutations"] = json!(1);
        write(root.path(), 1, &uncertain);
        let body = read_activity(root.path(), 1100);
        assert!(body["agents"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["status"] == "incomplete"));
        let stale = read_activity(root.path(), STALE_MS + 1001);
        assert!(stale["agents"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["status"] == "stale"));
        let mut truncated = capture(root.path(), 2, 1000);
        truncated["observation_truncated"] = json!(true);
        assert_eq!(
            summarize(serde_json::from_value(truncated).unwrap(), 1100)["status"],
            "incomplete"
        );
    }

    #[test]
    fn rejects_symlinks_hardlinks_wrong_workspace_and_oversized_files() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let valid = write(outside.path(), 0, &capture(root.path(), 0, 1000));
        let dir = root.path().join(".selfware/phi/activity");
        std::fs::create_dir_all(&dir).unwrap();
        symlink(&valid, dir.join("symlink.json")).unwrap();
        std::fs::hard_link(&valid, dir.join("hardlink.json")).unwrap();
        let mut other = capture(root.path(), 1, 1000);
        other["workspace_root"] = json!(outside.path());
        write(root.path(), 1, &other);
        std::fs::write(
            dir.join("oversized.json"),
            vec![b' '; MAX_FILE as usize + 1],
        )
        .unwrap();
        let body = read_activity(root.path(), 1100);
        assert_eq!(body["status"], "incomplete");
        assert_eq!(body["agents"], json!([]));
        let linked_root = tempfile::tempdir().unwrap();
        symlink(
            root.path().join(".selfware"),
            linked_root.path().join(".selfware"),
        )
        .unwrap();
        assert_eq!(
            read_activity(linked_root.path(), 1100)["reason"],
            "unsafe_capture"
        );
    }

    #[test]
    fn bounds_agent_rows_and_reports_truncation() {
        let root = tempfile::tempdir().unwrap();
        for id in 0..MAX_AGENTS + 1 {
            write(root.path(), id, &capture(root.path(), id, 1000));
        }
        let body = read_activity(root.path(), 1100);
        assert_eq!(body["agents"].as_array().unwrap().len(), MAX_AGENTS);
        assert_eq!(body["truncated"], true);
        assert_eq!(body["status"], "incomplete");
    }

    #[test]
    fn retained_tasks_do_not_inflate_agent_count() {
        let root = tempfile::tempdir().unwrap();
        let old = capture(root.path(), 0, 1000);
        write(root.path(), 0, &old);
        let mut latest = old;
        latest["task_id"] = json!("next-task");
        latest["recorded_at_ms"] = json!(1200);
        write(root.path(), 1, &latest);
        let body = read_activity(root.path(), 1300);
        assert_eq!(body["agents"].as_array().unwrap().len(), 1);
        assert_eq!(body["agents"][0]["recorded_at_ms"], 1200);
        assert_eq!(body["agents"][0]["task_id"], opaque("next-task"));
    }

    #[test]
    fn writer_to_server_round_trip() {
        let root = tempfile::tempdir().unwrap();
        let recorder = crate::phi::activity::ActivityCapture::new(
            root.path(),
            "writer-session",
            "writer-agent",
        )
        .expect("recorder should initialize");

        let evidence = json!({
            "outstanding": 0,
            "unreviewed_lines": 0,
            "untested_lines": 0,
            "unknown_size_obligations": 0,
            "unattributed": [],
            "possible_unrecorded_mutations": 0,
            "citations": ["src/main.rs"],
            "observations": [
                {
                    "turn": 1,
                    "tool": "cargo_check",
                    "command": "cargo check",
                    "kind": "run_finished",
                    "outcome": "Passed",
                    "may_have_mutated": false,
                    "reason": null,
                    "call_id": "call-1"
                }
            ]
        });

        recorder
            .write(
                "task-roundtrip",
                crate::phi::activity::ActivityPhase::Completed,
                evidence,
                false,
            )
            .expect("recording snapshot should succeed");

        let now = now_ms();
        let body = read_activity(root.path(), now);
        assert_eq!(body["status"], "available");
        let agents = body["agents"].as_array().expect("agents array");
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0]["phase"], "completed");
        assert_eq!(agents[0]["status"], "available");
        assert_eq!(agents[0]["evidence"]["passed_runs"], 1);
        assert_eq!(agents[0]["evidence"]["failed_runs"], 0);
    }

    #[test]
    fn bounded_gc_prunes_excess_receipts() {
        let root = tempfile::tempdir().unwrap();
        // Write MAX_SCAN + 5 distinct receipt files
        for id in 0..MAX_SCAN + 5 {
            write(root.path(), id, &capture(root.path(), id, 1000 + id as u64));
        }
        let act_dir = root.path().join(".selfware/phi/activity");
        let initial_count = std::fs::read_dir(&act_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
            .count();
        assert_eq!(initial_count, MAX_SCAN + 5);

        // read_activity is strictly read-only and must not delete files
        let body = read_activity(root.path(), 2000);
        assert_eq!(body["truncated"], true);

        // Subsequent check verifies NO files were pruned from disk by GET
        let post_read_count = std::fs::read_dir(&act_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
            .count();
        assert_eq!(post_read_count, MAX_SCAN + 5);
    }
}
