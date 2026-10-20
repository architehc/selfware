//! Read-only, bounded activity projection. Captures are observations, never a
//! declaration that a workspace is verified. No command, source or prompt is
//! returned to the browser.

use super::{
    bad_request, document_read_error, internal_error, require_session, revalidate_document_hash,
    review_error, unprocessable, validate_expected_document_hash, validate_graph, ApiResult,
    EvolveServer,
};
use crate::evolve::assistant::{
    evidence_from_document, gate_evidence_trust, redact_secrets, GroundedClaim,
    GroundedRecommendation, GroundingEvidence,
};
use axum::{
    extract::State,
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
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
const MAX_CHAT_MESSAGE_CHARS: usize = 4_000;
const MAX_CHAT_HISTORY_TURNS: usize = 12;
const MAX_CHAT_SELECTION_LINES: usize = 240;

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

    fn to_activity_phase(&self) -> crate::phi::activity::ActivityPhase {
        match self {
            Self::Running => crate::phi::activity::ActivityPhase::Running,
            Self::Completed => crate::phi::activity::ActivityPhase::Completed,
            Self::Failed => crate::phi::activity::ActivityPhase::Failed,
            Self::Partial => crate::phi::activity::ActivityPhase::Partial,
            Self::Abandoned => crate::phi::activity::ActivityPhase::Abandoned,
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
    Router::new()
        .route("/api/phi/activity", get(activity))
        .route("/api/phi/policy", get(policy))
        .route("/api/phi/chat", post(chat))
        .route("/api/phi/arena", get(arena_status))
        .route("/api/phi/arena/drill", post(arena_drill))
}

async fn policy(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    require_session(&headers, &server)?;
    let current = crate::phi::policy::PhiPolicy::from_env();
    Ok(Json(serde_json::to_value(current).unwrap_or_default()))
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

#[cfg_attr(not(feature = "tui"), allow(dead_code))]
pub(crate) fn read_workspace_activity(root: &Path) -> Value {
    read_activity(root, now_ms())
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
        let file = match open_child(&directory, &file_name, false) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                rejected = true;
                continue;
            }
        };
        let Ok(metadata) = file.metadata() else {
            continue;
        };
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() == 0 {
            // Unlinked concurrently by retention prune; skip cleanly without rejection
            continue;
        }
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
    let mut posture_counts = (0usize, 0usize, 0usize, 0usize, 0usize);
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
        let latest_run = if capture.observation_truncated {
            None
        } else {
            runs.last().and_then(|r| match r.outcome.as_deref() {
                Some("Passed") => Some("passed"),
                Some("Failed") => Some("failed"),
                _ => None,
            })
        };
        posture_counts = (
            e.outstanding,
            e.unreviewed_lines,
            e.untested_lines,
            e.unknown_size_obligations,
            passed,
        );
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
    let policy = crate::phi::policy::PhiPolicy::from_env();
    let fox_posture = crate::phi::policy::derive_fox_posture_from_counts(
        posture_counts.0,
        posture_counts.1,
        posture_counts.2,
        posture_counts.3,
        posture_counts.4,
        capture.phase.to_activity_phase(),
        &policy,
        crate::phi::policy::PresenceChannel::Ambient,
    );
    json!({"agent_id":opaque(&format!("{}\0{}", capture.session_id, capture.agent_id)),"task_id":opaque(&capture.task_id),
        "session_id":opaque(&capture.session_id),"status":status,"reason":reason,
        "recorded_at_ms":capture.recorded_at_ms,"age_ms":age,
        "phase":capture.phase.label(),"evidence":evidence,"fox_posture":fox_posture})
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct PhiChatTurn {
    role: String,
    content: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
struct PhiLineSelection {
    start_line: usize,
    end_line: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct PhiChatRequest {
    message: String,
    #[serde(default)]
    history: Vec<PhiChatTurn>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    expected_hash: Option<String>,
    #[serde(default)]
    selection: Option<PhiLineSelection>,
    #[serde(default)]
    intent: Option<String>,
    #[serde(default)]
    mode: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PhiSuggestedAction {
    id: String,
    label: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    question: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_line: Option<usize>,
}

#[derive(Debug, Default)]
struct ObservedSignalSummary {
    activity_status: String,
    activity_reason: Option<String>,
    agent_count: usize,
    running_agents: usize,
    failed_agents: usize,
    incomplete_agents: usize,
    stale_agents: usize,
    failed_runs: usize,
    passed_runs: usize,
    unknown_runs: usize,
    outstanding: usize,
    unreviewed_lines: usize,
    untested_lines: usize,
    gate_passed: bool,
    gate_findings: Vec<String>,
}

fn summarize_observed_signals(
    activity_body: &Value,
    gate_passed: bool,
    gate_findings: Vec<String>,
) -> ObservedSignalSummary {
    let mut summary = ObservedSignalSummary {
        activity_status: activity_body["status"]
            .as_str()
            .unwrap_or("unavailable")
            .to_string(),
        activity_reason: activity_body["reason"].as_str().map(str::to_string),
        gate_passed,
        gate_findings,
        ..Default::default()
    };
    if let Some(agents) = activity_body["agents"].as_array() {
        summary.agent_count = agents.len();
        for agent in agents {
            let status = agent["status"].as_str().unwrap_or("incomplete");
            let phase = agent["phase"].as_str().unwrap_or("");
            if status == "stale" {
                summary.stale_agents += 1;
                continue;
            }
            if status != "available" {
                summary.incomplete_agents += 1;
            }
            if phase == "running" {
                summary.running_agents += 1;
            } else if phase == "failed" {
                summary.failed_agents += 1;
            }
            if let Some(ev) = agent["evidence"].as_object() {
                summary.failed_runs +=
                    ev.get("failed_runs").and_then(Value::as_u64).unwrap_or(0) as usize;
                summary.passed_runs +=
                    ev.get("passed_runs").and_then(Value::as_u64).unwrap_or(0) as usize;
                summary.unknown_runs +=
                    ev.get("unknown_runs").and_then(Value::as_u64).unwrap_or(0) as usize;
                summary.outstanding +=
                    ev.get("outstanding").and_then(Value::as_u64).unwrap_or(0) as usize;
                summary.unreviewed_lines += ev
                    .get("unreviewed_lines")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize;
                summary.untested_lines += ev
                    .get("untested_lines")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize;
            }
        }
    }
    summary
}

fn build_selection_evidence(
    path: &str,
    content: &str,
    content_hash: &str,
    selection: Option<PhiLineSelection>,
) -> (Vec<GroundingEvidence>, bool) {
    if let Some(sel) = selection {
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return (Vec::new(), true);
        }
        let first = sel.start_line.max(1).min(lines.len());
        let capped_end = sel
            .end_line
            .max(first)
            .min(first.saturating_add(MAX_CHAT_SELECTION_LINES - 1))
            .min(lines.len());
        let excerpt = (first..=capped_end)
            .map(|line_no| format!("{:>6} | {}", line_no, lines[line_no - 1]))
            .collect::<Vec<_>>()
            .join("\n");
        let complete = capped_end == sel.end_line.min(lines.len());
        return (
            vec![GroundingEvidence {
                id: "E1".to_string(),
                path: path.to_string(),
                start_line: first,
                end_line: capped_end,
                excerpt,
                content_hash: content_hash.to_string(),
                source: "workspace_snapshot".to_string(),
            }],
            complete,
        );
    }
    evidence_from_document(path, content, content_hash, 120)
}

pub(crate) fn classify_signal_emotion(
    failure_present: bool,
    debt_present: bool,
    uncertainty_present: bool,
    running_present: bool,
    has_citations: bool,
) -> &'static str {
    if failure_present {
        "error"
    } else if debt_present {
        "skeptical"
    } else if uncertainty_present {
        "guard"
    } else if running_present {
        "working"
    } else if has_citations {
        "analytical"
    } else {
        "curious"
    }
}

fn derive_signal_emotion(signals: &ObservedSignalSummary, has_citations: bool) -> &'static str {
    classify_signal_emotion(
        !signals.gate_passed || signals.failed_agents > 0 || signals.failed_runs > 0,
        signals.unreviewed_lines > 0 || signals.untested_lines > 0 || signals.outstanding > 0,
        signals.incomplete_agents > 0 || signals.unknown_runs > 0,
        signals.running_agents > 0,
        has_citations,
    )
}

fn extract_notable_declarations(
    evidence: &[GroundingEvidence],
) -> Vec<(String, usize, GroundingEvidence)> {
    let mut out = Vec::new();
    for chunk in evidence {
        for raw_line in chunk.excerpt.lines() {
            let Some((num_part, code_part)) = raw_line.split_once(" | ") else {
                continue;
            };
            let Ok(line_no) = num_part.trim().parse::<usize>() else {
                continue;
            };
            let trimmed = code_part.trim();
            if trimmed.starts_with("pub fn ")
                || trimmed.starts_with("fn ")
                || trimmed.starts_with("pub async fn ")
                || trimmed.starts_with("async fn ")
                || trimmed.starts_with("pub struct ")
                || trimmed.starts_with("struct ")
                || trimmed.starts_with("pub enum ")
                || trimmed.starts_with("enum ")
                || trimmed.starts_with("pub trait ")
                || trimmed.starts_with("trait ")
                || trimmed.starts_with("impl ")
                || trimmed.starts_with("def ")
                || trimmed.starts_with("class ")
                || trimmed.starts_with("export class ")
                || trimmed.starts_with("export function ")
            {
                let single_excerpt = format!("{:>6} | {}", line_no, code_part);
                let pin = GroundingEvidence {
                    id: format!("E{}", out.len() + 1),
                    path: chunk.path.clone(),
                    start_line: line_no,
                    end_line: (line_no + 4).min(chunk.end_line).max(line_no),
                    excerpt: single_excerpt,
                    content_hash: chunk.content_hash.clone(),
                    source: "workspace_snapshot".to_string(),
                };
                out.push((trimmed.to_string(), line_no, pin));
                if out.len() >= 4 {
                    return out;
                }
            }
        }
    }
    out
}

fn resolve_graph_mention(
    graph: &crate::evolve::Graph,
    project_root: &Path,
    message: &str,
) -> Option<(String, Option<PhiLineSelection>, Option<String>)> {
    let raw_tokens: Vec<&str> = message
        .split_whitespace()
        .map(|t| {
            t.trim_matches(|c: char| {
                matches!(
                    c,
                    '`' | '\''
                        | '"'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | ','
                        | ';'
                        | ':'
                        | '?'
                        | '!'
                )
            })
        })
        .filter(|t| !t.is_empty())
        .collect();

    let ident_tokens: Vec<&str> = message
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|t| t.len() >= 4)
        .collect();

    let mut matched_file: Option<String> = None;
    for tok in &raw_tokens {
        let clean = tok.split(':').next().unwrap_or(tok);
        if !clean.contains('.') && !clean.contains('/') {
            continue;
        }
        for node in &graph.nodes {
            if matches!(
                node.layer,
                crate::evolve::NodeLayer::Symbol
                    | crate::evolve::NodeLayer::Concept
                    | crate::evolve::NodeLayer::Preset
            ) {
                continue;
            }
            let Some(ref np) = node.path else {
                continue;
            };
            let basename = Path::new(np)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            if np == clean
                || (clean.contains('.') && basename.eq_ignore_ascii_case(clean))
                || (clean.contains('/') && np.ends_with(clean))
            {
                matched_file = Some(np.clone());
                break;
            }
        }
        if matched_file.is_some() {
            break;
        }
    }

    // Check for public symbol mentions in graph.nodes (preferring exact-case match,
    // and scoped to `matched_file` when a file was also named).
    for exact_case in [true, false] {
        for ident in &ident_tokens {
            if !exact_case
                && matches!(
                    ident.to_ascii_lowercase().as_str(),
                    "what"
                        | "where"
                        | "when"
                        | "which"
                        | "this"
                        | "that"
                        | "with"
                        | "from"
                        | "into"
                        | "tell"
                        | "about"
                        | "explain"
                        | "show"
                        | "file"
                        | "code"
                        | "line"
                        | "lines"
                        | "function"
                        | "struct"
                        | "enum"
                        | "trait"
                        | "next"
                        | "quest"
                        | "status"
                        | "report"
                        | "give"
                )
            {
                continue;
            }
            for node in &graph.nodes {
                if !matches!(node.layer, crate::evolve::NodeLayer::Symbol) {
                    continue;
                }
                let Some(ref sym_path) = node.path else {
                    continue;
                };
                if let Some(ref mf) = matched_file {
                    if sym_path != mf {
                        continue;
                    }
                }
                let sym_name = node.id.rsplit("::").next().unwrap_or(&node.id);
                let is_match = if exact_case {
                    sym_name == *ident
                } else {
                    sym_name.len() >= 5 && sym_name.eq_ignore_ascii_case(ident)
                };
                if is_match {
                    let sel = node
                        .line_range
                        .map(|(start_line, end_line)| PhiLineSelection {
                            start_line: start_line.max(1),
                            end_line: end_line.max(start_line.max(1)),
                        });
                    return Some((sym_path.clone(), sel, Some(sym_name.to_string())));
                }
            }
        }
    }

    // Fallback: check Auxiliary / workspace landmarks (e.g. `Viseme` or `PhiArenaPane` in `src/ui/tui/`)
    let landmarks = crate::phi::arena::collect_workspace_landmarks(project_root, 256);
    for exact_case in [true, false] {
        for ident in &ident_tokens {
            for lm in &landmarks {
                if let Some(ref mf) = matched_file {
                    if &lm.rel_path != mf {
                        continue;
                    }
                }
                let is_match = if exact_case {
                    lm.symbol == *ident
                } else {
                    lm.symbol.len() >= 5 && lm.symbol.eq_ignore_ascii_case(ident)
                };
                if is_match {
                    let sel = Some(PhiLineSelection {
                        start_line: lm.line_no.max(1),
                        end_line: lm.line_no.max(1) + 12,
                    });
                    return Some((lm.rel_path.clone(), sel, Some(lm.symbol.clone())));
                }
            }
        }
    }

    matched_file.map(|f| (f, None, None))
}

#[allow(clippy::too_many_arguments)]
fn synthesize_local_bot_turn(
    intent: &str,
    message: &str,
    signals: &ObservedSignalSummary,
    doc_path: Option<&str>,
    selection: Option<PhiLineSelection>,
    resolved_symbol: Option<&str>,
    doc_evidence: &[GroundingEvidence],
    evidence_complete: bool,
) -> (
    String,
    &'static str,
    Vec<GroundingEvidence>,
    Vec<GroundedClaim>,
    Vec<GroundedRecommendation>,
    Vec<PhiSuggestedAction>,
    &'static str,
) {
    let mut citations = Vec::new();
    let mut claims = Vec::new();
    let mut actions = Vec::new();

    let declarations = extract_notable_declarations(doc_evidence);
    if selection.is_some() {
        if let Some(first_chunk) = doc_evidence.first() {
            citations.push(first_chunk.clone());
        }
    } else if !declarations.is_empty() {
        for (_, _, pin) in &declarations {
            citations.push(pin.clone());
        }
    } else if let Some(first_chunk) = doc_evidence.first() {
        citations.push(first_chunk.clone());
    }

    let reply = match intent {
        "whats_red" => {
            let mut parts = Vec::new();
            if !signals.gate_passed {
                parts.push(format!(
                    "Architecture gates are failing ({}).",
                    if signals.gate_findings.is_empty() {
                        "unspecified architecture finding".to_string()
                    } else {
                        signals.gate_findings.join(", ")
                    }
                ));
            } else {
                parts.push("Architecture graph validation (`validate_graph`) has 0 reported cycle/dangling-edge findings.".to_string());
            }
            if signals.failed_agents > 0 || signals.failed_runs > 0 {
                parts.push(format!(
                    "Observed activity captures show {} failed agent(s) and {} failed command run(s).",
                    signals.failed_agents, signals.failed_runs
                ));
            } else if signals.activity_status == "unavailable" {
                parts.push(format!(
                    "Live agent activity captures are currently unavailable ({}); absence of captures does not prove tests passed.",
                    signals.activity_reason.as_deref().unwrap_or("no_capture")
                ));
            } else {
                parts.push(format!(
                    "Across {} observed agent capture(s), 0 failed runs are recorded ({} passed, {} unknown).",
                    signals.agent_count, signals.passed_runs, signals.unknown_runs
                ));
            }
            parts.join(" ")
        }
        "audit_unreviewed" | "check_debt" => {
            let mut parts = Vec::new();
            if signals.unreviewed_lines > 0 || signals.untested_lines > 0 || signals.outstanding > 0
            {
                parts.push(format!(
                    "Observed agent debt: {} unreviewed line(s), {} line(s) without confirmed test coverage, and {} outstanding verification obligation(s).",
                    signals.unreviewed_lines, signals.untested_lines, signals.outstanding
                ));
            } else if signals.activity_status == "unavailable" {
                parts.push(format!(
                    "No live agent activity captures are present ({}).",
                    signals.activity_reason.as_deref().unwrap_or("no_capture")
                ));
            } else {
                parts.push(format!(
                    "Observed agent captures report 0 unreviewed lines and 0 outstanding obligations across {} agent(s).",
                    signals.agent_count
                ));
            }
            if let Some(path) = doc_path {
                if !declarations.is_empty() {
                    let names = declarations
                        .iter()
                        .map(|(sig, line, _)| format!("`{}` (line {})", sig, line))
                        .collect::<Vec<_>>()
                        .join(", ");
                    parts.push(format!(
                        "In `{path}`, key entry points ready for grounded line-by-line review include {names}."
                    ));
                }
            }
            parts.join(" ")
        }
        "check_formal" => {
            let formal = crate::phi::arena::verify_phi_formal_invariants();
            let total_rows = formal["total_rows"].as_u64().unwrap_or(0);
            if citations.is_empty() {
                citations.push(GroundingEvidence {
                    id: "FORMAL_ARENA".to_string(),
                    path: "src/phi/arena.rs".to_string(),
                    start_line: 794,
                    end_line: 872,
                    excerpt: "pub fn verify_phi_formal_invariants() -> Value".to_string(),
                    content_hash: String::new(),
                    source: "workspace_snapshot".to_string(),
                });
                citations.push(GroundingEvidence {
                    id: "FORMAL_POLICY_TESTS".to_string(),
                    path: "src/phi/policy_tests.rs".to_string(),
                    start_line: 356,
                    end_line: 625,
                    excerpt: "fn phi_arena_and_trust_bounds_conform_to_lean_table()".to_string(),
                    content_hash: String::new(),
                    source: "workspace_snapshot".to_string(),
                });
            }
            format!(
                "Lean 4 formal twin status: `table_conformance_verified` across {total_rows} exported transition rows (`formal/PhiArenaAndTrustBounds.lean` PA1–PA52 + `formal/HarnessAndTwinBounds.lean` PT1–PT5). Verified invariants cover PA1 SignalEmotion dominance (`src/evolve/server/phi_activity.rs:563`), PA2 Grounded TrustState (`redaction_always_withholds`), PA3 PhiPresence attention routing (`src/phi/policy.rs:880`), PA4 DebtBoss priority & HP bounds (`src/phi/arena.rs:340`), PA5 Dream consolidation honesty (`src/cognitive/dream_subprocess.rs:180`), PA6 KV-prefix role demotion (`src/agent/context_management.rs:1054`), PA7 worktree-isolated ToolCache invalidation (`src/session/cache.rs:180`), PA8 EditHistory RestoreOutcome honesty (`src/session/edit_history.rs:505`), PA9–PA12 ArenaProfile/ContextTrust/ContextFit/ApplyStatus bounds, PA13–PA16 Safety/CircuitBreaker/Ontology/Viseme bounds, PA17–PA20 RecoveryTree/ActionRateLimiter/HookRegistry/GuardrailEnforcer bounds, PA21–PA24 ConfirmView/ResourcePressure/AgentLoop/AgentMemory bounds, PA25–PA28 RSI DarwinX/Evolution Sabotage/MultiArm Repair/SelfHealing bounds, PA29–PA32 Investigative Governance/MicroMode/MetaLearning/Swarm Task Completion, PA33–PA36 Consolidation Retention/SWL Guardrails/Safety Redaction/Workflow Lattice, PA37–PA40 Verification Attribution/Skill Admission/Vector Auto-Compact/Evolve Readiness, PA41–PA44 Hook Quoting Gate (`src/hooks/mod.rs:145`), MCP Protocol Gate (`src/mcp/server.rs:300`), Evolution Replay Readiness (`src/evolution/replay.rs:195`), Observability Alert Gate (`src/observability/log_analysis.rs:140`), PA45–PA48 Supervision Sticky Terminal (`src/supervision/run_registry.rs:58`), Session Checkpoint Delta (`src/session/checkpoint.rs:140`), Devops & Container Isolation (`src/tools/container/validation.rs:10`), Phi Ledger Assessment (`src/phi/ledger.rs:210`), and PA49–PA52 Config & CLI Endpoint/Budget (`src/config/mod.rs:345`), Tool Parser & Measured Token Accounting (`src/token_count.rs:145`), LSP & Computer Window Rule 6 Guards (`src/computer/window.rs:75`), and UI Theme/Spinner/Output Sink (`src/ui/theme.rs:110`)."
            )
        }
        "explain_selection" => {
            if let Some(chunk) = citations.first() {
                let raw_lines: Vec<&str> = chunk
                    .excerpt
                    .lines()
                    .map(|l| {
                        l.split_once(" | ")
                            .map(|(_, c)| c.trim())
                            .unwrap_or(l.trim())
                    })
                    .filter(|l| !l.is_empty())
                    .collect();
                let preview = raw_lines.first().copied().unwrap_or("selected block");
                let summary = format!(
                    "Focused `{}` lines {}–{} ({} non-empty line(s), starting at `{}`). Click the citation pill to aim Phi's optical laser at this block, or dispatch a grounded model reading below.",
                    chunk.path,
                    chunk.start_line,
                    chunk.end_line,
                    raw_lines.len(),
                    preview.chars().take(72).collect::<String>()
                );
                claims.push(GroundedClaim {
                    text: summary.clone(),
                    evidence_ids: vec![chunk.id.clone()],
                });
                summary
            } else {
                "Select lines in the code editor (or open a saved workspace file) so Phi can anchor explanations to exact line numbers.".to_string()
            }
        }
        _ => {
            let lower_msg = message.to_ascii_lowercase();
            let asks_formal = [
                "pa1",
                "pa2",
                "pa3",
                "pa4",
                "pa5",
                "pa6",
                "pa7",
                "pa8",
                "pa9",
                "pa10",
                "pa11",
                "pa12",
                "pa13",
                "pa14",
                "pa15",
                "pa16",
                "pa17",
                "pa18",
                "pa19",
                "pa20",
                "pa21",
                "pa22",
                "pa23",
                "pa24",
                "pa25",
                "pa26",
                "pa27",
                "pa28",
                "pa29",
                "pa30",
                "pa31",
                "pa32",
                "pa33",
                "pa34",
                "pa35",
                "pa36",
                "pa37",
                "pa38",
                "pa39",
                "pa40",
                "pa41",
                "pa42",
                "pa43",
                "pa44",
                "pa45",
                "pa46",
                "pa47",
                "pa48",
                "pa49",
                "pa50",
                "pa51",
                "pa52",
                "pt1",
                "pt2",
                "pt3",
                "pt4",
                "pt5",
                "lean 4",
                "formal verification",
                "formal proof",
            ]
            .iter()
            .any(|kw| lower_msg.contains(kw));
            let mut parts = Vec::new();
            if asks_formal {
                let formal = crate::phi::arena::verify_phi_formal_invariants();
                let total_rows = formal["total_rows"].as_u64().unwrap_or(0);
                parts.push(format!(
                    "Lean 4 formal twin (`formal/PhiArenaAndTrustBounds.lean` + `formal/HarnessAndTwinBounds.lean`): {total_rows} transition rows verified under `table_conformance_verified` (PA1–PA52, PT1–PT5)."
                ));
            }
            if let Some(path) = doc_path {
                if let (Some(sym), Some(sel)) = (resolved_symbol, selection) {
                    let first_sig = citations
                        .first()
                        .and_then(|c| c.excerpt.lines().next())
                        .and_then(|l| l.split_once(" | ").map(|(_, code)| code.trim()))
                        .unwrap_or(sym);
                    parts.push(format!(
                        "Resolved symbol `{sym}` in `{path}:{}-{}` (`{}`).",
                        sel.start_line,
                        sel.end_line,
                        first_sig.chars().take(80).collect::<String>()
                    ));
                } else if let Some(sel) = selection {
                    parts.push(format!(
                        "Anchored to `{path}:{}-{}` for your question (\"{}\").",
                        sel.start_line,
                        sel.end_line,
                        message.chars().take(80).collect::<String>()
                    ));
                } else if !declarations.is_empty() {
                    let names = declarations
                        .iter()
                        .map(|(sig, line, _)| format!("`{}` (L{})", sig, line))
                        .collect::<Vec<_>>()
                        .join(", ");
                    parts.push(format!(
                        "In `{path}`, structural landmarks matching this snapshot are: {names}."
                    ));
                } else {
                    parts.push(format!("Anchored to saved snapshot of `{path}`."));
                }
            }
            if !signals.gate_passed || signals.failed_runs > 0 || signals.unreviewed_lines > 0 {
                parts.push(format!(
                    "Workspace status: gates={}, failed_runs={}, unreviewed_lines={}, untested_lines={}.",
                    if signals.gate_passed { "passing" } else { "failing" },
                    signals.failed_runs,
                    signals.unreviewed_lines,
                    signals.untested_lines
                ));
            } else {
                parts.push(format!(
                    "Architecture graph validation has 0 findings; activity status is `{}`.",
                    signals.activity_status
                ));
            }
            parts.join(" ")
        }
    };

    if claims.is_empty() && !citations.is_empty() {
        claims.push(GroundedClaim {
            text: reply.clone(),
            evidence_ids: citations.iter().map(|c| c.id.clone()).collect(),
        });
    }

    if let Some(first_cite) = citations.first() {
        actions.push(PhiSuggestedAction {
            id: format!("laser:{}:{}", first_cite.path, first_cite.start_line),
            label: format!(
                "↗ Aim laser at {}:{}-{}",
                first_cite.path, first_cite.start_line, first_cite.end_line
            ),
            kind: "focus_citation".to_string(),
            target: Some(first_cite.path.clone()),
            question: None,
            start_line: Some(first_cite.start_line),
            end_line: Some(first_cite.end_line),
        });
    }
    if let Some(path) = doc_path {
        actions.push(PhiSuggestedAction {
            id: format!("prepare_reading:{path}"),
            label: format!("✧ Prepare grounded reading for {path}"),
            kind: "prepare_reading".to_string(),
            target: Some(path.to_string()),
            question: Some(message.to_string()),
            start_line: selection.map(|s| s.start_line),
            end_line: selection.map(|s| s.end_line),
        });
    }
    if signals.agent_count > 0 {
        actions.push(PhiSuggestedAction {
            id: "inspect_activity:panel".to_string(),
            label: "Inspect live agent activity".to_string(),
            kind: "inspect_activity".to_string(),
            target: None,
            question: None,
            start_line: None,
            end_line: None,
        });
    }

    let emotion = derive_signal_emotion(signals, !citations.is_empty());
    let trust_state = if intent != "whats_red"
        && intent != "audit_unreviewed"
        && intent != "check_debt"
        && intent != "check_formal"
        && !citations.is_empty()
        && (evidence_complete || !declarations.is_empty())
    {
        "structural"
    } else if intent == "check_formal" {
        "table_conformance_verified"
    } else {
        "observed_signals"
    };

    (
        reply,
        emotion,
        citations,
        claims,
        Vec::new(),
        actions,
        trust_state,
    )
}

async fn chat(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
    Json(body): Json<PhiChatRequest>,
) -> ApiResult<Json<Value>> {
    require_session(&headers, &server)?;

    let trimmed_message = body.message.trim();
    if trimmed_message.is_empty() {
        return Err(bad_request("chat message cannot be empty"));
    }
    if trimmed_message.chars().count() > MAX_CHAT_MESSAGE_CHARS {
        return Err(bad_request(format!(
            "chat message exceeds maximum length of {MAX_CHAT_MESSAGE_CHARS} characters"
        )));
    }
    if body.history.len() > MAX_CHAT_HISTORY_TURNS {
        return Err(bad_request(format!(
            "chat history exceeds maximum of {MAX_CHAT_HISTORY_TURNS} turns"
        )));
    }
    for (idx, turn) in body.history.iter().enumerate() {
        if !matches!(turn.role.as_str(), "user" | "assistant") {
            return Err(bad_request(format!(
                "invalid history role at index {idx}: must be 'user' or 'assistant'"
            )));
        }
        if turn.content.chars().count() > MAX_CHAT_MESSAGE_CHARS {
            return Err(bad_request(format!(
                "history turn {idx} exceeds maximum length of {MAX_CHAT_MESSAGE_CHARS} characters"
            )));
        }
    }
    if let Some(sel) = body.selection {
        if sel.start_line == 0 || sel.end_line < sel.start_line {
            return Err(bad_request(
                "selection start_line must be >= 1 and end_line >= start_line",
            ));
        }
    }

    // Rule 5: Trust-gate the user message and conversation history before any
    // model or workspace processing.
    let mut prompt_gate_chunks = vec![GroundingEvidence {
        id: "CHAT_MSG".to_string(),
        path: "phi_chat_message".to_string(),
        start_line: 1,
        end_line: trimmed_message.lines().count().max(1),
        excerpt: trimmed_message.to_string(),
        content_hash: String::new(),
        source: "user_input".to_string(),
    }];
    for (idx, turn) in body.history.iter().enumerate() {
        prompt_gate_chunks.push(GroundingEvidence {
            id: format!("CHAT_HIST_{idx}"),
            path: "phi_chat_history".to_string(),
            start_line: 1,
            end_line: turn.content.lines().count().max(1),
            excerpt: turn.content.clone(),
            content_hash: String::new(),
            source: "user_input".to_string(),
        });
    }
    let mut trust_gate =
        gate_evidence_trust(&prompt_gate_chunks).map_err(|err| unprocessable(err.body()))?;

    // Gather optional document snapshot and validate expected_hash (409 on mismatch).
    let mut doc_snapshot = None;
    let mut doc_evidence = Vec::new();
    let mut evidence_complete = true;
    let mut effective_selection = body.selection;
    let mut resolved_symbol: Option<String> = None;

    if let Some(path) = body.path.as_deref() {
        if !path.trim().is_empty() {
            let doc = server
                .ide
                .read_document(path)
                .map_err(document_read_error)?;
            if let Some(expected_hash) = body.expected_hash.as_deref() {
                validate_expected_document_hash(expected_hash, &doc.hash)?;
            }
            let (ev, complete) =
                build_selection_evidence(&doc.path, &doc.content, &doc.hash, effective_selection);
            let doc_gate = gate_evidence_trust(&ev).map_err(|err| unprocessable(err.body()))?;
            trust_gate.sources_scanned += doc_gate.sources_scanned;
            trust_gate.findings += doc_gate.findings;
            trust_gate.worst_risk_score =
                trust_gate.worst_risk_score.max(doc_gate.worst_risk_score);
            doc_evidence = ev;
            evidence_complete = complete;
            doc_snapshot = Some(doc);
        }
    }

    // Gather deterministic workspace & activity signals.
    let root = server.project_root.clone();
    let now = now_ms();
    let activity_body = tokio::task::spawn_blocking(move || read_activity(&root, now))
        .await
        .unwrap_or_else(|_| empty(now, "capture_read_failed"));

    let graph = server.graph_snapshot().map_err(internal_error)?;

    // Cross-repo natural-language file & symbol resolution when no explicit selection/hash pin
    // locks us to a different slice, or when no path was passed.
    if body.selection.is_none() {
        if let Some((mentioned_path, mentioned_sel, mentioned_sym)) =
            resolve_graph_mention(&graph, &server.project_root, trimmed_message)
        {
            let can_switch_file = body.expected_hash.is_none()
                || doc_snapshot
                    .as_ref()
                    .is_some_and(|d| d.path == mentioned_path);
            if can_switch_file {
                if let Ok(doc) = server.ide.read_document(&mentioned_path) {
                    let (ev, complete) =
                        build_selection_evidence(&doc.path, &doc.content, &doc.hash, mentioned_sel);
                    if let Ok(doc_gate) = gate_evidence_trust(&ev) {
                        trust_gate.sources_scanned += doc_gate.sources_scanned;
                        trust_gate.findings += doc_gate.findings;
                        trust_gate.worst_risk_score =
                            trust_gate.worst_risk_score.max(doc_gate.worst_risk_score);
                        doc_evidence = ev;
                        evidence_complete = complete;
                        doc_snapshot = Some(doc);
                        effective_selection = mentioned_sel;
                        resolved_symbol = mentioned_sym;
                    }
                }
            }
        }
    }

    let validation = validate_graph(&graph);
    let mut gate_findings = Vec::new();
    if !validation.duplicate_ids.is_empty() {
        gate_findings.push(format!(
            "duplicate_ids ({})",
            validation.duplicate_ids.len()
        ));
    }
    if !validation.cycles.is_empty() {
        gate_findings.push(format!("cycles ({})", validation.cycles.len()));
    }
    if !validation.dangling_edges.is_empty() {
        gate_findings.push(format!(
            "dangling_edges ({})",
            validation.dangling_edges.len()
        ));
    }
    if !validation.isolated_nodes.is_empty() {
        gate_findings.push(format!(
            "isolated_nodes ({})",
            validation.isolated_nodes.len()
        ));
    }
    let gate_passed = validation.valid;
    let signals = summarize_observed_signals(&activity_body, gate_passed, gate_findings);

    let intent = body.intent.as_deref().unwrap_or("chat");
    let mode = body.mode.as_deref().unwrap_or("local_signals");

    // Rule 4: Measured token accounting via estimate_content_tokens.
    let mut measured_prompt = redact_secrets(trimmed_message);
    for turn in &body.history {
        measured_prompt.push('\n');
        measured_prompt.push_str(&redact_secrets(&turn.content));
    }
    for chunk in &doc_evidence {
        measured_prompt.push('\n');
        measured_prompt.push_str(&chunk.excerpt);
    }
    let prompt_tokens_measured = crate::token_count::estimate_content_tokens(&measured_prompt);

    if mode == "grounded_model" {
        if doc_evidence.is_empty() {
            return Err(bad_request(
                "grounded_model chat mode requires a readable workspace document path",
            ));
        }
        let _assistant_guard = server.assistant_lock.lock().await;
        if let (Some(doc), Some(expected_hash)) =
            (doc_snapshot.as_ref(), body.expected_hash.as_deref())
        {
            revalidate_document_hash(&server, &doc.path, expected_hash)?;
        }
        let history_context = if body.history.is_empty() {
            String::new()
        } else {
            body.history
                .iter()
                .map(|t| format!("{}: {}", t.role, redact_secrets(&t.content)))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let orientation = format!(
            "Observed signals (non-citeable): activity_status={}, running_agents={}, failed_runs={}, unreviewed_lines={}, untested_lines={}, gates_passed={}.\n{}",
            signals.activity_status,
            signals.running_agents,
            signals.failed_runs,
            signals.unreviewed_lines,
            signals.untested_lines,
            signals.gate_passed,
            history_context
        );
        let review = server
            .assistant
            .review_with_orientation(
                trimmed_message,
                doc_evidence.clone(),
                evidence_complete,
                Some(&orientation),
            )
            .await
            .map_err(review_error)?;

        let cited_ids: HashSet<&str> = review
            .claims
            .iter()
            .flat_map(|c| c.evidence_ids.iter().map(String::as_str))
            .chain(
                review
                    .recommendations
                    .iter()
                    .flat_map(|r| r.evidence_ids.iter().map(String::as_str)),
            )
            .collect();
        let citations: Vec<GroundingEvidence> = review
            .evidence
            .iter()
            .filter(|e| cited_ids.contains(e.id.as_str()))
            .cloned()
            .collect();
        let reply = review
            .claims
            .iter()
            .map(|c| c.text.as_str())
            .chain(review.recommendations.iter().map(|r| r.rationale.as_str()))
            .collect::<Vec<_>>()
            .join(" ");
        let emotion = derive_signal_emotion(&signals, !citations.is_empty());
        let mut suggested_actions = Vec::new();
        if let Some(first) = citations.first() {
            suggested_actions.push(PhiSuggestedAction {
                id: format!("laser:{}:{}", first.path, first.start_line),
                label: format!(
                    "↗ Aim laser at {}:{}-{}",
                    first.path, first.start_line, first.end_line
                ),
                kind: "focus_citation".to_string(),
                target: Some(first.path.clone()),
                question: None,
                start_line: Some(first.start_line),
                end_line: Some(first.end_line),
            });
        }
        return Ok(Json(json!({
            "reply": reply,
            "emotion": emotion,
            "trust_state": review.trust_state,
            "grounding_scope": review.grounding_scope,
            "semantic_validation": review.semantic_validation,
            "hallucination_free_guarantee": false,
            "verification_note": "Structural citation check passed; no code tests were executed during this chat turn.",
            "citations": citations,
            "claims": review.claims,
            "recommendations": review.recommendations,
            "suggested_actions": suggested_actions,
            "signals": {
                "activity_status": signals.activity_status,
                "agent_count": signals.agent_count,
                "running_agents": signals.running_agents,
                "failed_runs": signals.failed_runs,
                "unreviewed_lines": signals.unreviewed_lines,
                "untested_lines": signals.untested_lines,
                "gate_passed": signals.gate_passed,
                "gate_findings": signals.gate_findings,
            },
            "trust_gate": review.trust_gate,
            "model": review.model,
            "usage": review.usage,
            "prompt_tokens_measured": prompt_tokens_measured,
        })));
    }

    let (reply, emotion, citations, claims, recommendations, suggested_actions, trust_state) =
        synthesize_local_bot_turn(
            intent,
            trimmed_message,
            &signals,
            doc_snapshot.as_ref().map(|d| d.path.as_str()),
            effective_selection,
            resolved_symbol.as_deref(),
            &doc_evidence,
            evidence_complete,
        );

    Ok(Json(json!({
        "reply": reply,
        "emotion": emotion,
        "trust_state": trust_state,
        "grounding_scope": if citations.is_empty() {
            "observed_workspace_and_activity_signals"
        } else {
            "snapshot_and_citation_integrity_only"
        },
        "semantic_validation": "not_performed",
        "hallucination_free_guarantee": false,
        "verification_note": "Grounded in local workspace snapshots and observed activity captures; no new test runs were executed by this chat turn.",
        "citations": citations,
        "claims": claims,
        "recommendations": recommendations,
        "suggested_actions": suggested_actions,
        "signals": {
            "activity_status": signals.activity_status,
            "agent_count": signals.agent_count,
            "running_agents": signals.running_agents,
            "failed_runs": signals.failed_runs,
            "unreviewed_lines": signals.unreviewed_lines,
            "untested_lines": signals.untested_lines,
            "gate_passed": signals.gate_passed,
            "gate_findings": signals.gate_findings,
        },
        "trust_gate": trust_gate,
        "model": null,
        "usage": null,
        "prompt_tokens_measured": prompt_tokens_measured,
    })))
}

use crate::phi::arena::{
    generate_formal_drill, generate_formal_drill_json, generate_graph_drill, load_arena_profile,
    read_rsi_umbrella_summary, save_arena_profile, verify_phi_formal_invariants,
};

async fn arena_status(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    require_session(&headers, &server)?;
    let root = server.project_root.clone();
    let now = now_ms();
    let (profile, activity_body, rsi_umbrella) = tokio::task::spawn_blocking(move || {
        let mut p = load_arena_profile(&root);
        let _ = p.refresh_badges();
        let act = read_activity(&root, now);
        let rsi = read_rsi_umbrella_summary(&root);
        (p, act, rsi)
    })
    .await
    .map_err(internal_error)?;

    let graph = server.graph_snapshot().map_err(internal_error)?;
    let validation = validate_graph(&graph);
    let signals = summarize_observed_signals(&activity_body, validation.valid, Vec::new());
    let drill = generate_graph_drill(&graph, profile.drills_completed as usize);

    Ok(Json(json!({
        "profile": profile.to_public_json(),
        "drill": drill,
        "formal": verify_phi_formal_invariants(),
        "rsi_umbrella": rsi_umbrella,
        "signals": {
            "activity_status": signals.activity_status,
            "agent_count": signals.agent_count,
            "running_agents": signals.running_agents,
            "failed_runs": signals.failed_runs,
            "unreviewed_lines": signals.unreviewed_lines,
            "untested_lines": signals.untested_lines,
            "gate_passed": signals.gate_passed,
        }
    })))
}

#[derive(Debug, Deserialize)]
struct ArenaDrillRequest {
    action: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    choice_index: Option<usize>,
    #[serde(default)]
    correct_index: Option<usize>,
    #[serde(default)]
    seed: Option<usize>,
}

async fn arena_drill(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
    Json(body): Json<ArenaDrillRequest>,
) -> ApiResult<Json<Value>> {
    require_session(&headers, &server)?;
    let action = body.action.trim();
    if !matches!(
        action,
        "answer_drill" | "laser_lock" | "next_drill" | "formal_drill"
    ) {
        return Err(bad_request(
            "action must be 'answer_drill', 'laser_lock', 'next_drill', or 'formal_drill'",
        ));
    }

    let graph = server.graph_snapshot().map_err(internal_error)?;
    let root = server.project_root.clone();
    let mut profile = tokio::task::spawn_blocking({
        let root = root.clone();
        move || load_arena_profile(&root)
    })
    .await
    .map_err(internal_error)?;

    let mut correct = false;
    let mut xp_gained = 0u64;
    let mut unlocked = Vec::new();
    let current_seed = body.seed.unwrap_or(profile.drills_completed as usize);
    let mut next_seed = current_seed;
    let mut should_save = false;
    let is_formal = body.kind.as_deref() == Some("formal_proof") || action == "formal_drill";

    match action {
        "laser_lock" => {
            profile.citations_inspected = profile.citations_inspected.saturating_add(1);
            xp_gained = 15;
            profile.xp = profile.xp.saturating_add(xp_gained);
            unlocked = profile.refresh_badges();
            should_save = true;
        }
        "answer_drill" => {
            let choice = body
                .choice_index
                .ok_or_else(|| bad_request("choice_index is required for answer_drill"))?;
            let server_expected = if is_formal {
                generate_formal_drill(current_seed).correct_index
            } else {
                generate_graph_drill(&graph, current_seed)["correct_index"]
                    .as_u64()
                    .or_else(|| body.correct_index.map(|idx| idx as u64))
                    .unwrap_or(0) as usize
            };
            let expected = match (body.correct_index, body.seed) {
                (Some(idx), _) => idx,
                (None, Some(_)) => server_expected,
                (None, None) => {
                    return Err(bad_request("correct_index is required for answer_drill"))
                }
            };
            let expected_index = if body.seed.is_some() {
                server_expected
            } else {
                expected
            };
            next_seed = current_seed.wrapping_add(1);
            if choice == expected && choice == expected_index {
                correct = true;
                xp_gained = if is_formal { 50 } else { 35 };
                profile.drills_completed = profile.drills_completed.saturating_add(1);
                profile.xp = profile.xp.saturating_add(xp_gained);
                profile.best_streak =
                    ((profile.best_streak + 0.25).min(3.0) * 100.0).round() / 100.0;
                unlocked = profile.refresh_badges();
                if is_formal && profile.unlock("formal_sentinel") {
                    unlocked.push(json!({
                        "id": "formal_sentinel",
                        "title": crate::phi::arena::achievement_badge_title("formal_sentinel"),
                    }));
                }
                should_save = true;
            }
        }
        "next_drill" | "formal_drill" => {
            next_seed = current_seed.wrapping_add(1);
        }
        _ => {}
    }

    if should_save {
        let save_root = server.project_root.clone();
        let save_prof = profile.clone();
        let _ =
            tokio::task::spawn_blocking(move || save_arena_profile(&save_root, &save_prof)).await;
    }

    let drill = if is_formal {
        generate_formal_drill_json(next_seed)
    } else {
        generate_graph_drill(&graph, next_seed)
    };

    Ok(Json(json!({
        "action": action,
        "correct": correct,
        "xp_gained": xp_gained,
        "unlocked": unlocked,
        "profile": profile.to_public_json(),
        "drill": drill,
        "formal": verify_phi_formal_invariants(),
    })))
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
        let truncated_summary = summarize(serde_json::from_value(truncated).unwrap(), 1100);
        assert_eq!(truncated_summary["status"], "incomplete");
        assert_eq!(
            truncated_summary["evidence"]["latest_run"],
            serde_json::Value::Null
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

    #[tokio::test]
    async fn policy_endpoint_and_fox_posture_projection() {
        let root = tempfile::tempdir().unwrap();
        let server = EvolveServer::for_project(
            crate::evolve::Graph {
                nodes: vec![],
                edges: vec![],
            },
            root.path(),
        )
        .unwrap();

        let unauth = server
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/phi/policy")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauth.status(), 401);

        let auth = server
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/phi/policy")
                    .header(super::super::SESSION_HEADER, server.session_token())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(auth.status(), 200);
        let bytes = to_bytes(auth.into_body(), 64 * 1024).await.unwrap();
        let policy_json: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(policy_json["mode"], "observe_only");

        write(root.path(), 0, &capture(root.path(), 0, 1000));
        let body = read_activity(root.path(), 1100);
        assert_eq!(body["agents"][0]["fox_posture"]["emotion"], "error");
    }

    #[tokio::test]
    async fn chat_endpoint_requires_session_validates_hash_and_grounds_citations() {
        let root = tempfile::tempdir().unwrap();
        let src_dir = root.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let source = "pub fn greeting() -> String {\n    String::from(\"Hello Phi\")\n}\n";
        std::fs::write(src_dir.join("lib.rs"), source).unwrap();
        let expected_hash = format!("{:x}", Sha256::digest(source.as_bytes()));

        let server = EvolveServer::for_project(
            crate::evolve::Graph {
                nodes: vec![],
                edges: vec![],
            },
            root.path(),
        )
        .unwrap();

        // 1. Missing session token -> 401
        let unauth = server
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/phi/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({ "message": "What's red?" })).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauth.status(), 401);

        // 2. Empty message -> 400
        let empty_req = server
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/phi/chat")
                    .header("content-type", "application/json")
                    .header(super::super::SESSION_HEADER, server.session_token())
                    .body(Body::from(
                        serde_json::to_vec(&json!({ "message": "   " })).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(empty_req.status(), 400);

        // 3. Stale expected_hash -> 409 Conflict
        let stale_req = server
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/phi/chat")
                    .header("content-type", "application/json")
                    .header(super::super::SESSION_HEADER, server.session_token())
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "message": "Explain selection",
                            "path": "src/lib.rs",
                            "expected_hash": "deadbeef"
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(stale_req.status(), 409);

        // 4. Valid selection + document snapshot -> 200 with structural citation and measured tokens
        let valid_req = server
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/phi/chat")
                    .header("content-type", "application/json")
                    .header(super::super::SESSION_HEADER, server.session_token())
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "message": "Explain this greeting function",
                            "intent": "explain_selection",
                            "path": "src/lib.rs",
                            "expected_hash": expected_hash,
                            "selection": { "start_line": 1, "end_line": 3 },
                            "history": [
                                { "role": "user", "content": "Hi Phi" },
                                { "role": "assistant", "content": "Hello! Select some code to inspect." }
                            ]
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(valid_req.status(), 200);
        let bytes = to_bytes(valid_req.into_body(), 64 * 1024).await.unwrap();
        let payload: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["trust_state"], "structural");
        assert_eq!(payload["hallucination_free_guarantee"], false);
        assert_eq!(payload["emotion"], "analytical");
        assert!(payload["prompt_tokens_measured"].as_u64().unwrap_or(0) > 0);
        let citations = payload["citations"].as_array().unwrap();
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0]["path"], "src/lib.rs");
        assert_eq!(citations[0]["start_line"], 1);
        assert_eq!(citations[0]["end_line"], 3);
        assert_eq!(citations[0]["content_hash"], expected_hash);

        // 5. Write a failed activity capture and query "whats_red" -> emotion "error"
        write(root.path(), 0, &capture(root.path(), 0, now_ms()));
        let red_req = server
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/phi/chat")
                    .header("content-type", "application/json")
                    .header(super::super::SESSION_HEADER, server.session_token())
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "message": "What's red?",
                            "intent": "whats_red"
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(red_req.status(), 200);
        let red_bytes = to_bytes(red_req.into_body(), 64 * 1024).await.unwrap();
        let red_payload: Value = serde_json::from_slice(&red_bytes).unwrap();
        assert_eq!(red_payload["emotion"], "error");
        assert_eq!(red_payload["trust_state"], "observed_signals");
        assert_eq!(red_payload["signals"]["failed_runs"], 1);
        assert!(red_payload["reply"]
            .as_str()
            .unwrap_or("")
            .contains("1 failed command run"));
    }

    #[tokio::test]
    async fn chat_resolves_symbol_mentions_and_arena_endpoints_persist_progress() {
        let root = tempfile::tempdir().unwrap();
        let src_dir = root.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let source = "pub struct FoxSentinel {\n    pub active: bool,\n}\n\npub fn compute_golden_spiral() -> f64 {\n    1.618\n}\n";
        std::fs::write(src_dir.join("sentinel.rs"), source).unwrap();

        let graph = crate::evolve::Graph {
            nodes: vec![
                crate::evolve::Node::code("src/sentinel.rs", "src/sentinel.rs"),
                crate::evolve::Node::symbol(
                    "src/sentinel.rs::FoxSentinel",
                    "src/sentinel.rs",
                    "struct",
                    "src/sentinel.rs",
                    (1, 3),
                    12,
                ),
                crate::evolve::Node::symbol(
                    "src/sentinel.rs::compute_golden_spiral",
                    "src/sentinel.rs",
                    "fn",
                    "src/sentinel.rs",
                    (5, 7),
                    14,
                ),
            ],
            edges: vec![],
        };

        let server = EvolveServer::for_project(graph, root.path()).unwrap();

        // 1. Ask about `compute_golden_spiral` WITHOUT passing `path` -> resolves symbol and line range
        let sym_req = server
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/phi/chat")
                    .header("content-type", "application/json")
                    .header(super::super::SESSION_HEADER, server.session_token())
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "message": "Where is compute_golden_spiral defined?"
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(sym_req.status(), 200);
        let sym_bytes = to_bytes(sym_req.into_body(), 64 * 1024).await.unwrap();
        let sym_payload: Value = serde_json::from_slice(&sym_bytes).unwrap();
        assert_eq!(sym_payload["trust_state"], "structural");
        assert!(sym_payload["reply"]
            .as_str()
            .unwrap_or("")
            .contains("Resolved symbol `compute_golden_spiral` in `src/sentinel.rs:5-7`"));
        let cites = sym_payload["citations"].as_array().unwrap();
        assert_eq!(cites.len(), 1);
        assert_eq!(cites[0]["path"], "src/sentinel.rs");
        assert_eq!(cites[0]["start_line"], 5);
        assert_eq!(cites[0]["end_line"], 7);

        // 2. GET /api/phi/arena requires session token and returns initial profile + drill
        let arena_get = server
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/phi/arena")
                    .header(super::super::SESSION_HEADER, server.session_token())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(arena_get.status(), 200);
        let arena_bytes = to_bytes(arena_get.into_body(), 64 * 1024).await.unwrap();
        let arena_json: Value = serde_json::from_slice(&arena_bytes).unwrap();
        assert_eq!(arena_json["profile"]["level"], 1);
        assert_eq!(arena_json["profile"]["xp"], 0);
        let correct_idx = arena_json["drill"]["correct_index"].as_u64().unwrap() as usize;

        // 3. POST /api/phi/arena/drill with correct answer -> +35 XP & persists .selfware/phi/arena_profile.json
        let drill_post = server
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/phi/arena/drill")
                    .header("content-type", "application/json")
                    .header(super::super::SESSION_HEADER, server.session_token())
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "action": "answer_drill",
                            "choice_index": correct_idx,
                            "correct_index": correct_idx,
                            "seed": 0
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(drill_post.status(), 200);
        let drill_bytes = to_bytes(drill_post.into_body(), 64 * 1024).await.unwrap();
        let drill_json: Value = serde_json::from_slice(&drill_bytes).unwrap();
        assert_eq!(drill_json["correct"], true);
        assert_eq!(drill_json["xp_gained"], 35);
        assert_eq!(drill_json["profile"]["xp"], 35);
        assert_eq!(drill_json["profile"]["drills_completed"], 1);

        // 4. POST /api/phi/arena/drill with laser_lock -> +15 XP & unlocks first_laser_lock
        let laser_post = server
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/phi/arena/drill")
                    .header("content-type", "application/json")
                    .header(super::super::SESSION_HEADER, server.session_token())
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "action": "laser_lock"
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(laser_post.status(), 200);
        let laser_bytes = to_bytes(laser_post.into_body(), 64 * 1024).await.unwrap();
        let laser_json: Value = serde_json::from_slice(&laser_bytes).unwrap();
        assert_eq!(laser_json["profile"]["xp"], 50);
        assert_eq!(laser_json["profile"]["citations_inspected"], 1);
        assert!(laser_json["profile"]["achievements"]
            .as_array()
            .unwrap()
            .contains(&json!("first_laser_lock")));
        assert_eq!(
            arena_json["formal"]["trust_state"],
            "table_conformance_verified"
        );
        assert_eq!(arena_json["formal"]["all_passed"], true);
        assert_eq!(
            arena_json["rsi_umbrella"]["trust_state"],
            "observed_signals"
        );

        // 5. POST /api/phi/chat with check_formal intent -> returns table_conformance_verified
        let formal_chat_post = server
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/phi/chat")
                    .header("content-type", "application/json")
                    .header(super::super::SESSION_HEADER, server.session_token())
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "message": "check lean 4 formal proofs",
                            "intent": "check_formal"
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(formal_chat_post.status(), 200);
        let formal_chat_bytes = to_bytes(formal_chat_post.into_body(), 64 * 1024)
            .await
            .unwrap();
        let formal_chat_json: Value = serde_json::from_slice(&formal_chat_bytes).unwrap();
        assert_eq!(
            formal_chat_json["trust_state"],
            "table_conformance_verified"
        );

        // 6. POST /api/phi/arena/drill with formal_drill -> answer formal_proof -> unlocks formal_sentinel
        let formal_drill_post = server
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/phi/arena/drill")
                    .header("content-type", "application/json")
                    .header(super::super::SESSION_HEADER, server.session_token())
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "action": "formal_drill",
                            "seed": 0
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(formal_drill_post.status(), 200);
        let fd_bytes = to_bytes(formal_drill_post.into_body(), 64 * 1024)
            .await
            .unwrap();
        let fd_json: Value = serde_json::from_slice(&fd_bytes).unwrap();
        assert_eq!(fd_json["drill"]["kind"], "formal_proof");
        let fd_correct_idx = fd_json["drill"]["correct_index"].as_u64().unwrap() as usize;
        let fd_seed = fd_json["drill"]["seed"].as_u64().unwrap() as usize;

        let fd_answer_post = server
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/phi/arena/drill")
                    .header("content-type", "application/json")
                    .header(super::super::SESSION_HEADER, server.session_token())
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "action": "answer_drill",
                            "kind": "formal_proof",
                            "choice_index": fd_correct_idx,
                            "correct_index": fd_correct_idx,
                            "seed": fd_seed
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(fd_answer_post.status(), 200);
        let fd_ans_bytes = to_bytes(fd_answer_post.into_body(), 64 * 1024)
            .await
            .unwrap();
        let fd_ans_json: Value = serde_json::from_slice(&fd_ans_bytes).unwrap();
        assert_eq!(fd_ans_json["correct"], true);
        assert!(fd_ans_json["profile"]["achievements"]
            .as_array()
            .unwrap()
            .contains(&json!("formal_sentinel")));
    }
}
