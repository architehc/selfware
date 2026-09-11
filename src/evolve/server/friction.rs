//! Ephemeral, authenticated friction signals. No code, compiler output, prompts,
//! file names, or user-state inferences leave the existing workspace process.

use super::{require_session, ApiError, ApiResult, EvolveServer};
use crate::evolve::diagnostics::AnalysisReport;
use axum::body::to_bytes;
use axum::extract::{Query, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_EVENTS: usize = 256;
const MAX_PAGE_EVENTS: usize = 64;
const MAX_PAGE_BYTES: usize = 512 * 1024;
const MAX_DEDUP: usize = 512;
const MAX_BATCH: usize = 32;
const MAX_BODY: usize = 32 * 1024;
const MAX_DIAGNOSTICS: usize = 32;
const RETENTION_MS: u64 = 60 * 60 * 1000;

#[derive(Clone, Debug, Serialize)]
struct Event {
    id: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_id: Option<String>,
    at_ms: u64,
    source: &'static str,
    data: Value,
}

#[derive(Default)]
struct Feed {
    sequence: u64,
    events: VecDeque<Event>,
    dedup: HashMap<String, (String, u64)>,
    dedup_order: VecDeque<(String, u64)>,
    #[cfg(feature = "self-improvement")]
    runs: HashMap<String, TrackedRun>,
    #[cfg(feature = "self-improvement")]
    run_order: VecDeque<String>,
}

#[cfg(feature = "self-improvement")]
struct TrackedRun {
    task_id: String,
    started_ms: u64,
    last_status: Option<String>,
}

#[derive(Clone, Default)]
pub(super) struct FrictionFeed(Arc<Mutex<Feed>>);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn hash(value: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(value))
}

impl Feed {
    fn prune(&mut self, now: u64) {
        while self
            .events
            .front()
            .is_some_and(|event| now.saturating_sub(event.at_ms) >= RETENTION_MS)
        {
            self.events.pop_front();
        }
        while self.dedup_order.front().is_some_and(|(_, at)| {
            now.saturating_sub(*at) >= RETENTION_MS || self.dedup_order.len() > MAX_DEDUP
        }) {
            if let Some((id, _)) = self.dedup_order.pop_front() {
                self.dedup.remove(&id);
            }
        }
        #[cfg(feature = "self-improvement")]
        while self.run_order.front().is_some_and(|id| {
            self.runs.get(id).is_none_or(|run| {
                now.saturating_sub(run.started_ms) >= RETENTION_MS
                    || self.run_order.len() > MAX_EVENTS
            })
        }) {
            if let Some(id) = self.run_order.pop_front() {
                self.runs.remove(&id);
            }
        }
    }

    fn push(&mut self, mut event: Event) {
        self.sequence += 1;
        event.id = self.sequence.to_string();
        self.events.push_back(event);
        while self.events.len() > MAX_EVENTS {
            self.events.pop_front();
        }
    }

    fn snapshot(&mut self, after: u64, now: u64) -> Value {
        self.prune(now);
        let earliest = self.events.front().map_or(self.sequence + 1, |event| {
            event.id.parse::<u64>().unwrap_or(self.sequence + 1)
        });
        let reset = after > self.sequence || after.saturating_add(1) < earliest;
        let after = if reset { 0 } else { after };
        let mut events = Vec::new();
        let mut bytes = 0usize;
        for event in self
            .events
            .iter()
            .filter(|event| event.id.parse::<u64>().is_ok_and(|id| id > after))
        {
            // Bound pages separately from retained history so a full diagnostic
            // ring cannot exceed the browser's response limit and stall polling.
            let size = serde_json::to_vec(event).map_or(MAX_PAGE_BYTES, |value| value.len());
            if events.len() == MAX_PAGE_EVENTS || bytes + size > MAX_PAGE_BYTES {
                break;
            }
            bytes += size;
            events.push(event);
        }
        let cursor = events
            .last()
            .and_then(|event| event.id.parse::<u64>().ok())
            .unwrap_or(self.sequence);
        json!({"events":events,"cursor":cursor,"reset":reset,"capabilities":capabilities()})
    }
}

fn capabilities() -> Value {
    json!({
        "storage":"process_memory", "retention_ms":RETENTION_MS,
        "max_events":MAX_EVENTS, "network":"none", "local_model":false,
        "page_max_events":MAX_PAGE_EVENTS, "page_max_payload_bytes":MAX_PAGE_BYTES,
        "biometrics":false, "generation_link_verification":"unavailable",
        "hooks": {
            "diagnostics":"server_completed_runs",
            "generation":if cfg!(feature = "self-improvement") {"server_apply_registry"} else {"unavailable"},
            "review":"ide_reports", "undo":"ide_reports", "activity":"ide_reports",
            "lsp":"unavailable", "external_ide":"explicit_adapter_reports"
        }
    })
}

fn error(status: StatusCode, code: &'static str, message: &'static str) -> ApiError {
    (
        status,
        Json(json!({"error":{"code":code,"message":message}})),
    )
}

fn invalid() -> ApiError {
    error(
        StatusCode::BAD_REQUEST,
        "invalid_friction_event",
        "Only bounded, typed friction metadata is accepted",
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Batch {
    events: Vec<InputEvent>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputEvent {
    id: String,
    kind: String,
    task_id: Option<String>,
    generation_id: Option<String>,
    source: Option<String>,
    at_ms: Option<u64>,
    data: Value,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Diagnostic {
    code: String,
    fingerprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    symbol: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DiagnosticsData {
    success: bool,
    evidence_complete: bool,
    diagnostics: Vec<Diagnostic>,
    toolchain: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GenerationData {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    added_lines: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deleted_lines: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    files_changed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    diff_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    diagnostics: Option<Vec<Diagnostic>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence_complete: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason_code: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReviewData {
    decision: String,
    added_lines: u64,
    active_review_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    diff_digest: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct UndoData {
    #[serde(skip_serializing_if = "Option::is_none")]
    document_id: Option<String>,
    #[serde(default = "one")]
    count: u64,
}
fn one() -> u64 {
    1
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ActivityData {
    active_ms: u64,
    local_hour: u8,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ContextData {
    reason: String,
}

fn opaque(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn code(value: &str) -> bool {
    (value.len() == 5 && value.starts_with('E') && value[1..].bytes().all(|b| b.is_ascii_digit()))
        || (value.starts_with("clippy::")
            && value.len() <= 80
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b"_:".contains(&b)))
}
fn valid_diagnostics(items: &[Diagnostic]) -> bool {
    items.len() <= MAX_DIAGNOSTICS
        && items.iter().all(|item| {
            code(&item.code)
                && digest(&item.fingerprint)
                && item.symbol.as_ref().is_none_or(|s| {
                    s.len() <= 80
                        && !s.is_empty()
                        && s.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"_:".contains(&b))
                })
        })
}
fn bounded(value: Option<u64>, max: u64) -> bool {
    value.is_none_or(|value| value <= max)
}

fn typed_data<T: for<'de> Deserialize<'de> + Serialize>(
    data: Value,
    check: impl FnOnce(&T) -> bool,
) -> ApiResult<Value> {
    let parsed: T = serde_json::from_value(data).map_err(|_| invalid())?;
    if !check(&parsed) {
        return Err(invalid());
    }
    serde_json::to_value(parsed).map_err(|_| invalid())
}

fn validate(input: InputEvent, now: u64) -> ApiResult<(String, Event)> {
    if !opaque(&input.id)
        || input.task_id.as_deref().is_some_and(|id| !opaque(id))
        || input.generation_id.as_deref().is_some_and(|id| !opaque(id))
        || input
            .source
            .as_deref()
            .is_some_and(|source| source != "ide")
        || input
            .at_ms
            .is_some_and(|at| at.abs_diff(now) > 5 * 60 * 1000)
    {
        return Err(invalid());
    }
    let data = match input.kind.as_str() {
        "diagnostics_finished" => typed_data::<DiagnosticsData>(input.data, |data| {
            data.toolchain == "rust" && valid_diagnostics(&data.diagnostics)
        })?,
        "generation_finished" => typed_data::<GenerationData>(input.data, |data| {
            matches!(
                data.status.as_str(),
                "staged" | "rejected" | "failed" | "applied"
            ) && bounded(data.added_lines, 1_000_000)
                && bounded(data.deleted_lines, 1_000_000)
                && bounded(data.files_changed, 10_000)
                && data.diff_digest.as_deref().is_none_or(digest)
                && data
                    .diagnostics
                    .as_ref()
                    .is_none_or(|items| valid_diagnostics(items))
                && data.reason_code.as_deref().is_none_or(|reason| {
                    matches!(
                        reason,
                        "developer_rejected"
                            | "compile_failed"
                            | "empty_diff"
                            | "diff_out_of_scope"
                            | "agent_failed"
                            | "unknown"
                    )
                })
        })?,
        "review_closed" => typed_data::<ReviewData>(input.data, |data| {
            matches!(data.decision.as_str(), "rejected" | "accepted" | "closed")
                && data.added_lines <= 1_000_000
                && data.active_review_ms <= 24 * 60 * 60 * 1000
                && data.diff_digest.as_deref().is_none_or(digest)
        })?,
        "editor_undo" => typed_data::<UndoData>(input.data, |data| {
            (1..=100).contains(&data.count) && data.document_id.as_deref().is_none_or(digest)
        })?,
        "activity" => typed_data::<ActivityData>(input.data, |data| {
            data.active_ms <= 60_000 && data.local_hour < 24
        })?,
        "context_changed" => typed_data::<ContextData>(input.data, |data| {
            matches!(data.reason.as_str(), "manual" | "new_task" | "agent_reset")
        })?,
        _ => return Err(invalid()),
    };
    Ok((
        input.id,
        Event {
            id: String::new(),
            kind: input.kind,
            task_id: input.task_id,
            generation_id: input.generation_id,
            at_ms: now,
            source: "ide",
            data,
        },
    ))
}

impl FrictionFeed {
    pub(super) fn diagnostics(&self, report: &AnalysisReport) {
        let errors: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|item| item.level == "error")
            .collect();
        let diagnostics: Vec<_> = errors.iter().filter_map(|item| {
            let diagnostic_code = item.code.as_deref().filter(|value| code(value))?;
            Some(json!({"code":diagnostic_code,"fingerprint":hash(format!("{diagnostic_code}\0{}", item.message))}))
        }).take(MAX_DIAGNOSTICS).collect();
        self.server_event("diagnostics_finished", None, None, json!({
            "success":report.success, "evidence_complete":report.evidence_complete && diagnostics.len() == errors.len(),
            "diagnostics":diagnostics, "toolchain":"rust"
        }));
    }

    pub(super) fn diagnostics_unavailable(&self, reason: &'static str) {
        self.server_event("diagnostics_finished", None, None, json!({
            "success":false,"evidence_complete":false,"diagnostics":[],"toolchain":"rust","reason_code":reason
        }));
    }

    fn server_event(
        &self,
        kind: &str,
        task_id: Option<String>,
        generation_id: Option<String>,
        data: Value,
    ) {
        if let Ok(mut feed) = self.0.lock() {
            let at_ms = now_ms();
            feed.prune(at_ms);
            feed.push(Event {
                id: String::new(),
                kind: kind.into(),
                task_id,
                generation_id,
                at_ms,
                source: "server",
                data,
            });
        }
    }

    #[cfg(feature = "self-improvement")]
    pub(super) fn track_apply(&self, id: &str, action: &str, target: &str) {
        if let Ok(mut feed) = self.0.lock() {
            let now = now_ms();
            feed.prune(now);
            if feed.runs.contains_key(id) {
                return;
            }
            feed.runs.insert(
                id.into(),
                TrackedRun {
                    task_id: hash(format!("{action}\0{target}")),
                    started_ms: now,
                    last_status: None,
                },
            );
            feed.run_order.push_back(id.into());
            feed.prune(now);
        }
    }

    #[cfg(feature = "self-improvement")]
    pub(super) fn applied(&self, id: &str, diff_digest: &str, files_changed: usize) {
        let task_id = self.0.lock().ok().and_then(|mut feed| {
            let run = feed.runs.get_mut(id)?;
            run.last_status = Some("applied".into());
            Some(run.task_id.clone())
        });
        self.server_event(
            "generation_finished",
            task_id,
            Some(id.into()),
            json!({
                "status":"applied","diff_digest":diff_digest,"files_changed":files_changed
            }),
        );
    }

    #[cfg(feature = "self-improvement")]
    pub(super) async fn observe_apply(&self, registry: &crate::evolve::ApplyRegistry) {
        // Only runs explicitly started by this server are observed. The tracking
        // table is bounded and expires; polling never runs a compiler or agent.
        let registry = registry.lock().await;
        let Ok(mut feed) = self.0.lock() else {
            return;
        };
        let now = now_ms();
        feed.prune(now);
        let mut events = Vec::new();
        let order: Vec<_> = feed.run_order.iter().cloned().collect();
        for id in order {
            let Some(tracked) = feed.runs.get_mut(&id) else {
                continue;
            };
            let Some(run) = registry.get(&id) else {
                continue;
            };
            let Some(data) = apply_data(run) else {
                continue;
            };
            let status = data["status"].as_str().unwrap_or("failed");
            if tracked.last_status.as_deref() == Some(status) {
                continue;
            }
            tracked.last_status = Some(status.into());
            events.push(Event {
                id: String::new(),
                kind: "generation_finished".into(),
                task_id: Some(tracked.task_id.clone()),
                generation_id: Some(id.clone()),
                at_ms: now,
                source: "server",
                data,
            });
        }
        // Apply is serialized. Preserve run-start order when completions are
        // observed together; at_ms still denotes observation, not completion.
        for event in events {
            feed.push(event);
        }
    }
}

#[cfg(feature = "self-improvement")]
fn apply_data(run: &crate::evolve::ApplyRun) -> Option<Value> {
    use crate::evolve::ApplyStatus;
    let (status, reason) = match &run.status {
        ApplyStatus::Running => return None,
        ApplyStatus::Staged => ("staged", None),
        ApplyStatus::Rejected(reason) => ("rejected", Some(reason.as_str())),
        ApplyStatus::Succeeded => ("applied", None),
        ApplyStatus::Failed => ("failed", None),
    };
    let mut data = json!({"status":status});
    if let Some(diff) = &run.diff {
        data["added_lines"] = json!(diff.insertions);
        data["deleted_lines"] = json!(diff.deletions);
        data["files_changed"] = json!(diff.files_changed);
        data["diff_digest"] = json!(diff.digest);
    }
    if let Some(reason) = reason {
        let reason_code = if reason.starts_with("compile_failed:") {
            "compile_failed"
        } else if reason.starts_with("diff_out_of_scope:") {
            "diff_out_of_scope"
        } else if reason == "empty_diff" {
            "empty_diff"
        } else {
            "unknown"
        };
        data["reason_code"] = json!(reason_code);
        if reason_code == "compile_failed" {
            let mut diagnostics = Vec::new();
            for line in reason.lines().take(128) {
                let Some(start) = line.find("error[E") else {
                    continue;
                };
                let Some(code) = line.get(start + 6..start + 11).filter(|value| code(value)) else {
                    continue;
                };
                let header: String = line[start..].chars().take(512).collect();
                diagnostics.push(json!({"code":code,"fingerprint":hash(header)}));
                if diagnostics.len() == MAX_DIAGNOSTICS {
                    break;
                }
            }
            data["diagnostics"] = json!(diagnostics);
            // Apply retains a capped stderr excerpt, without pre-generation
            // diagnostics. It cannot establish that an API was invented.
            data["evidence_complete"] = json!(false);
        }
    } else if status == "failed" {
        data["reason_code"] = json!("agent_failed");
    }
    Some(data)
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FeedQuery {
    #[serde(default)]
    after: u64,
}

pub(super) fn routes() -> Router<Arc<EvolveServer>> {
    Router::new().route("/api/friction/events", get(events).post(report))
}

async fn events(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
    query: Result<Query<FeedQuery>, axum::extract::rejection::QueryRejection>,
) -> ApiResult<Json<Value>> {
    require_session(&headers, &server)?;
    let Query(query) = query.map_err(|_| invalid())?;
    #[cfg(feature = "self-improvement")]
    server.friction.observe_apply(&server.apply_runs).await;
    let now = now_ms();
    let mut feed = server.friction.0.lock().map_err(|_| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "friction_unavailable",
            "Friction feed lock unavailable",
        )
    })?;
    Ok(Json(feed.snapshot(query.after, now)))
}

async fn report(
    State(server): State<Arc<EvolveServer>>,
    request: Request,
) -> ApiResult<Json<Value>> {
    require_session(request.headers(), &server)?;
    if request
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .is_none_or(|value| value.split(';').next() != Some("application/json"))
    {
        return Err(error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "json_required",
            "Expected application/json",
        ));
    }
    let bytes = to_bytes(request.into_body(), MAX_BODY).await.map_err(|_| {
        error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "friction_body_too_large",
            "Friction body exceeds 32 KiB",
        )
    })?;
    let batch: Batch = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if batch.events.is_empty() || batch.events.len() > MAX_BATCH {
        return Err(invalid());
    }
    let now = now_ms();
    let validated: Vec<_> = batch
        .events
        .into_iter()
        .map(|input| validate(input, now))
        .collect::<ApiResult<_>>()?;
    #[cfg(feature = "self-improvement")]
    server.friction.observe_apply(&server.apply_runs).await;
    let now = now_ms();
    let mut feed = server.friction.0.lock().map_err(|_| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "friction_unavailable",
            "Friction feed lock unavailable",
        )
    })?;
    feed.prune(now);
    let mut incoming = HashMap::new();
    for (id, event) in &validated {
        let fingerprint = hash(serde_json::to_vec(&json!({"kind":event.kind,"task_id":event.task_id,"generation_id":event.generation_id,"data":event.data})).map_err(|_| invalid())?);
        if feed
            .dedup
            .get(id)
            .is_some_and(|(old, _)| old != &fingerprint)
            || incoming.get(id).is_some_and(|old| old != &fingerprint)
        {
            return Err(error(
                StatusCode::CONFLICT,
                "friction_event_conflict",
                "Event ID already names different metadata",
            ));
        }
        incoming.insert(id.clone(), fingerprint);
    }
    let mut accepted = 0usize;
    for (id, mut event) in validated {
        if feed.dedup.contains_key(&id) {
            continue;
        }
        event.at_ms = now;
        #[cfg(feature = "self-improvement")]
        let event = {
            let mut event = event;
            if let Some(run) = event
                .generation_id
                .as_deref()
                .and_then(|id| feed.runs.get(id))
            {
                event.task_id = Some(run.task_id.clone());
            }
            event
        };
        feed.push(event);
        feed.dedup.insert(id.clone(), (incoming[&id].clone(), now));
        feed.dedup_order.push_back((id, now));
        accepted += 1;
    }
    feed.prune(now);
    Ok(Json(json!({"accepted":accepted,"cursor":feed.sequence})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use tower::ServiceExt;

    fn server() -> (tempfile::TempDir, EvolveServer) {
        let root = tempfile::tempdir().unwrap();
        let graph = crate::evolve::Graph {
            nodes: vec![],
            edges: vec![],
        };
        let server = EvolveServer::for_project(graph, root.path()).unwrap();
        (root, server)
    }

    async fn call(
        server: &EvolveServer,
        method: &str,
        path: &str,
        body: Value,
        authorized: bool,
    ) -> (StatusCode, Value) {
        let mut request = axum::http::Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json");
        if authorized {
            request = request.header(super::super::SESSION_HEADER, server.session_token());
        }
        let response = server
            .router()
            .oneshot(request.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    fn undo(id: &str) -> Value {
        json!({"id":id,"kind":"editor_undo","data":{"count":1}})
    }
    fn batch(event: Value) -> Value {
        json!({"events":[event]})
    }

    #[tokio::test]
    async fn friction_routes_require_session_and_describe_actual_capabilities() {
        let (_root, server) = server();
        for method in ["GET", "POST"] {
            assert_eq!(
                call(
                    &server,
                    method,
                    "/api/friction/events",
                    batch(undo("one")),
                    false
                )
                .await
                .0,
                StatusCode::UNAUTHORIZED
            );
        }
        let (status, body) = call(&server, "GET", "/api/friction/events", Value::Null, true).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["events"], json!([]));
        assert_eq!(body["capabilities"]["storage"], "process_memory");
        assert_eq!(body["capabilities"]["local_model"], false);
        assert_eq!(body["capabilities"]["hooks"]["lsp"], "unavailable");
        assert_eq!(
            call(
                &server,
                "GET",
                "/api/friction/events?after=invalid",
                Value::Null,
                false
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn friction_rejects_raw_payloads_spoofed_provenance_and_unbounded_counts() {
        let (_root, server) = server();
        let mut invalid_events = vec![
            json!({"id":"one","kind":"editor_undo","source":"server","data":{"count":1}}),
            json!({"id":"one","kind":"editor_undo","data":{"count":0}}),
            json!({"id":"one","kind":"editor_undo","data":{"count":101}}),
            json!({"id":"one","kind":"editor_undo","data":{"count":-1}}),
            json!({"id":"one","kind":"editor_undo","data":{"document_id":"src/private.rs"}}),
            json!({"id":"one","kind":"activity","data":{"active_ms":60001,"local_hour":1}}),
            json!({"id":"one","kind":"activity","data":{"active_ms":100,"local_hour":24}}),
            json!({"id":"one","kind":"context_changed","data":{"reason":"private prompt"}}),
            json!({"id":"one","kind":"diagnostics_finished","data":{"success":false,"evidence_complete":true,"toolchain":"rust","diagnostics":[{"code":"E0432","fingerprint":"not a hash"}]}}),
        ];
        for field in [
            "prompt",
            "code",
            "path",
            "api_key",
            "biometrics",
            "message",
            "output",
        ] {
            let mut outer = undo("one");
            outer[field] = json!("private content");
            invalid_events.push(outer);
            let mut inner = undo("one");
            inner["data"][field] = json!("private content");
            invalid_events.push(inner);
        }
        for event in invalid_events {
            let (status, body) = call(
                &server,
                "POST",
                "/api/friction/events",
                batch(event.clone()),
                true,
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{event}");
            assert!(!body.to_string().contains("private content"));
        }
        let (_, body) = call(&server, "GET", "/api/friction/events", Value::Null, true).await;
        assert_eq!(body["cursor"], 0);
    }

    #[tokio::test]
    async fn friction_ingress_is_atomic_bounded_and_idempotent() {
        let (_root, server) = server();
        let invalid_batch =
            json!({"events":[undo("one"),{"id":"two","kind":"raw_code","data":{}}]});
        assert_eq!(
            call(&server, "POST", "/api/friction/events", invalid_batch, true)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        let (_, first) = call(
            &server,
            "POST",
            "/api/friction/events",
            batch(undo("one")),
            true,
        )
        .await;
        assert_eq!(first, json!({"accepted":1,"cursor":1}));
        let (_, duplicate) = call(
            &server,
            "POST",
            "/api/friction/events",
            batch(undo("one")),
            true,
        )
        .await;
        assert_eq!(duplicate, json!({"accepted":0,"cursor":1}));
        let mut changed = undo("one");
        changed["data"]["count"] = json!(2);
        assert_eq!(
            call(
                &server,
                "POST",
                "/api/friction/events",
                batch(changed),
                true
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            call(
                &server,
                "POST",
                "/api/friction/events",
                json!({"events":vec![undo("many");MAX_BATCH+1]}),
                true
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call(
                &server,
                "POST",
                "/api/friction/events",
                json!({"events":[],"raw":"x".repeat(MAX_BODY)}),
                true
            )
            .await
            .0,
            StatusCode::PAYLOAD_TOO_LARGE
        );
        let (_, body) = call(&server, "GET", "/api/friction/events", Value::Null, true).await;
        assert_eq!(body["events"][0]["source"], "ide");
        assert!(body["events"][0].get("generation_id").is_none());
    }

    #[test]
    fn friction_ring_and_dedup_are_bounded_and_cursor_reports_gaps_and_restart() {
        let mut feed = Feed::default();
        for index in 0..MAX_DEDUP + 10 {
            let (_, event) = validate(
                serde_json::from_value(undo(&index.to_string())).unwrap(),
                100,
            )
            .unwrap();
            feed.push(event);
            feed.dedup
                .insert(index.to_string(), ("fingerprint".into(), 100));
            feed.dedup_order.push_back((index.to_string(), 100));
        }
        let snapshot = feed.snapshot(1, 100);
        assert_eq!(snapshot["reset"], true);
        assert_eq!(
            snapshot["events"].as_array().unwrap().len(),
            MAX_PAGE_EVENTS
        );
        assert_eq!(feed.events.len(), MAX_EVENTS);
        assert_eq!(feed.dedup.len(), MAX_DEDUP);
        let mut cursor = 0;
        let mut ids = Vec::new();
        while cursor < feed.sequence {
            let page = feed.snapshot(cursor, 100);
            let next = page["cursor"].as_u64().unwrap();
            assert!(next > cursor);
            ids.extend(
                page["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|event| event["id"].as_str().unwrap().parse::<u64>().unwrap()),
            );
            cursor = next;
        }
        assert_eq!(
            ids,
            (feed.sequence - MAX_EVENTS as u64 + 1..=feed.sequence).collect::<Vec<_>>()
        );
        assert_eq!(feed.snapshot(feed.sequence, 100)["events"], json!([]));
        assert_eq!(feed.snapshot(feed.sequence + 100, 100)["reset"], true);
        let expired = feed.snapshot(1, RETENTION_MS + 100);
        assert_eq!(expired["reset"], true);
        assert_eq!(expired["events"], json!([]));
        assert!(feed.dedup.is_empty());
    }

    #[test]
    fn friction_dense_diagnostic_pages_stay_below_browser_limit_without_skipping_events() {
        let mut feed = Feed::default();
        for index in 0..MAX_EVENTS {
            let input: InputEvent = serde_json::from_value(json!({
                "id":index.to_string(),"kind":"generation_finished","task_id":"t".repeat(96),"generation_id":"g".repeat(96),
                "data":{"status":"rejected","added_lines":1000000,"deleted_lines":1000000,"files_changed":10000,"diff_digest":"a".repeat(64),
                    "reason_code":"compile_failed","evidence_complete":true,"diagnostics":vec![json!({
                        "code":format!("clippy::{}","x".repeat(72)),"fingerprint":"b".repeat(64),"symbol":"s".repeat(80)
                    });MAX_DIAGNOSTICS]}
            })).unwrap();
            let (_, event) = validate(input, 100).unwrap();
            feed.push(event);
        }
        let mut cursor = 0;
        let mut count = 0;
        while cursor < feed.sequence {
            let page = feed.snapshot(cursor, 100);
            assert!(serde_json::to_vec(&page).unwrap().len() < 1024 * 1024);
            assert!(page["events"].as_array().unwrap().len() <= MAX_PAGE_EVENTS);
            let next = page["cursor"].as_u64().unwrap();
            assert!(next > cursor);
            count += page["events"].as_array().unwrap().len();
            cursor = next;
        }
        assert_eq!(count, MAX_EVENTS);
    }

    #[test]
    fn friction_diagnostic_hook_retains_verdict_without_raw_evidence_or_invented_generation_link() {
        use crate::evolve::diagnostics::{AnalysisKind, CompilerDiagnostic, DiagnosticSpan};
        let feed = FrictionFeed::default();
        let mut report = AnalysisReport {
            kind: AnalysisKind::Check,
            label: "Cargo check".into(),
            command: vec!["cargo".into()],
            success: false,
            exit_code: Some(101),
            duration_ms: 100,
            diagnostics: vec![CompilerDiagnostic {
                level: "error".into(),
                code: Some("E0432".into()),
                message: "unresolved import `secret_symbol`".into(),
                rendered: Some("secret source".into()),
                spans: vec![DiagnosticSpan {
                    file: "private/file.rs".into(),
                    line_start: 1,
                    line_end: 1,
                    column_start: 1,
                    column_end: 1,
                    is_primary: true,
                    label: None,
                }],
            }],
            errors: 1,
            warnings: 0,
            stdout_tail: "private stdout".into(),
            stderr_tail: "private stderr".into(),
            evidence_complete: true,
        };
        feed.diagnostics(&report);
        report.evidence_complete = false;
        feed.diagnostics(&report);
        feed.diagnostics_unavailable("analysis_timed_out");
        let snapshot = feed.0.lock().unwrap().snapshot(0, now_ms());
        let serialized = snapshot.to_string();
        for private in [
            "secret_symbol",
            "secret source",
            "private/file.rs",
            "private stdout",
            "private stderr",
        ] {
            assert!(!serialized.contains(private), "leaked {private}");
        }
        let events = snapshot["events"].as_array().unwrap();
        assert_eq!(events[0]["data"]["evidence_complete"], true);
        assert_eq!(events[1]["data"]["evidence_complete"], false);
        assert_eq!(events[2]["data"]["reason_code"], "analysis_timed_out");
        assert!(events
            .iter()
            .all(|event| event.get("generation_id").is_none()));
        assert!(digest(
            events[0]["data"]["diagnostics"][0]["fingerprint"]
                .as_str()
                .unwrap()
        ));
    }

    #[cfg(feature = "self-improvement")]
    fn apply_fixture(id: &str, status: crate::evolve::ApplyStatus) -> crate::evolve::ApplyRun {
        crate::evolve::ApplyRun {
            id: id.into(),
            prompt: "private prompt".into(),
            status,
            output: "private output".into(),
            exit_code: Some(0),
            shadow_path: None,
            base_revision: None,
            diff: Some(crate::evolve::apply::StagedDiff {
                digest: "a".repeat(64),
                files_changed: 1,
                insertions: 400,
                deletions: 2,
                preview: "private code".into(),
            }),
        }
    }

    #[cfg(feature = "self-improvement")]
    #[tokio::test]
    async fn friction_apply_registry_hook_is_ordered_deduplicated_and_preserves_failure_evidence() {
        use crate::evolve::ApplyStatus;
        let feed = FrictionFeed::default();
        let registry = crate::evolve::apply::new_registry();
        feed.track_apply("z-first", "refactor", "private_module");
        feed.track_apply("a-second", "refactor", "private_module");
        registry.lock().await.insert(
            "z-first".into(),
            apply_fixture("z-first", ApplyStatus::Staged),
        );
        registry.lock().await.insert("a-second".into(),apply_fixture("a-second",ApplyStatus::Rejected("compile_failed: error[E0599]: no method named `private_method`\nprivate source".into())));
        feed.observe_apply(&registry).await;
        feed.observe_apply(&registry).await;
        let snapshot = feed.0.lock().unwrap().snapshot(0, now_ms());
        let events = snapshot["events"].as_array().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["generation_id"], "z-first");
        assert_eq!(events[0]["task_id"], events[1]["task_id"]);
        assert_eq!(events[0]["data"]["added_lines"], 400);
        assert_eq!(events[1]["data"]["reason_code"], "compile_failed");
        assert_eq!(events[1]["data"]["diagnostics"][0]["code"], "E0599");
        assert_eq!(events[1]["data"]["evidence_complete"], false);
        assert!(!snapshot.to_string().contains("private"));
        assert_eq!(registry.lock().await["z-first"].output, "private output");
        feed.applied("z-first", &"a".repeat(64), 1);
        registry.lock().await.remove("z-first");
        feed.observe_apply(&registry).await;
        let snapshot = feed.0.lock().unwrap().snapshot(2, now_ms());
        assert_eq!(snapshot["events"].as_array().unwrap().len(), 1);
        assert_eq!(snapshot["events"][0]["data"]["status"], "applied");
    }

    #[cfg(feature = "self-improvement")]
    #[tokio::test]
    async fn friction_review_uses_tracked_task_and_accepts_measured_long_review() {
        let (_root, server) = server();
        server.friction.track_apply("run-1", "refactor", "node-1");
        let event = json!({"id":"review-1","kind":"review_closed","generation_id":"run-1","task_id":"wrong-client-task","data":{
            "decision":"rejected","added_lines":400,"active_review_ms":120000,"diff_digest":"b".repeat(64)
        }});
        assert_eq!(
            call(
                &server,
                "POST",
                "/api/friction/events",
                batch(event.clone()),
                true
            )
            .await
            .0,
            StatusCode::OK
        );
        let (_, duplicate) =
            call(&server, "POST", "/api/friction/events", batch(event), true).await;
        assert_eq!(duplicate["accepted"], 0);
        let (_, body) = call(&server, "GET", "/api/friction/events", Value::Null, true).await;
        assert_eq!(body["events"][0]["task_id"], hash("refactor\0node-1"));
        assert_eq!(body["events"][0]["source"], "ide");
        assert_eq!(body["events"][0]["data"]["active_review_ms"], 120000);
    }

    #[cfg(feature = "self-improvement")]
    #[tokio::test]
    async fn friction_review_ingress_observes_generation_before_review_without_prior_poll() {
        let (_root, server) = server();
        server
            .friction
            .track_apply("run-first", "refactor", "node-1");
        server.apply_runs.lock().await.insert(
            "run-first".into(),
            apply_fixture("run-first", crate::evolve::ApplyStatus::Staged),
        );
        let review = json!({"id":"review","kind":"review_closed","generation_id":"run-first","data":{
            "decision":"rejected","added_lines":400,"active_review_ms":120000
        }});
        assert_eq!(
            call(&server, "POST", "/api/friction/events", batch(review), true)
                .await
                .0,
            StatusCode::OK
        );
        let (_, body) = call(&server, "GET", "/api/friction/events", Value::Null, true).await;
        let events = body["events"].as_array().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["kind"], "generation_finished");
        assert_eq!(events[1]["kind"], "review_closed");
        assert_eq!(events[0]["task_id"], events[1]["task_id"]);
        assert_eq!(events[0]["generation_id"], events[1]["generation_id"]);
        assert!(events[1]["at_ms"].as_u64().unwrap() >= events[0]["at_ms"].as_u64().unwrap());
    }
}
