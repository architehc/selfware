//! Authenticated, bounded same-origin bridge to an optional owned speech worker.
//!
//! Inference and model installation stay outside the Rust process. The browser
//! never receives the worker address or bearer token.

use super::{require_session, ApiError, ApiResult, EvolveServer};
use anyhow::{bail, Context, Result};
use axum::{
    extract::{rejection::JsonRejection, DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use url::{Host, Url};

const JSON_LIMIT: usize = 1024 * 1024;
const AUDIO_LIMIT: usize = 32 * 1024 * 1024;
const TEXT_LIMIT: usize = 5000;

#[derive(Clone, Default)]
pub(super) struct SpeechBridge(Option<Worker>);

#[derive(Clone)]
struct Worker {
    endpoint: Url,
    token: String,
    client: Client,
}

impl SpeechBridge {
    pub(super) fn from_env() -> Result<Self> {
        let endpoint = std::env::var("SELFWARE_PHI_TTS_ENDPOINT");
        let token = std::env::var("SELFWARE_PHI_TTS_TOKEN");
        match (endpoint, token) {
            (Err(std::env::VarError::NotPresent), Err(std::env::VarError::NotPresent)) => {
                Ok(Self::default())
            }
            (Ok(endpoint), Ok(token)) => Self::configured(&endpoint, token),
            _ => bail!(
                "Phi speech requires both SELFWARE_PHI_TTS_ENDPOINT and SELFWARE_PHI_TTS_TOKEN"
            ),
        }
    }

    fn configured(endpoint: &str, token: String) -> Result<Self> {
        let endpoint = Url::parse(endpoint).context("invalid Phi speech worker endpoint")?;
        let loopback = match endpoint.host() {
            Some(Host::Ipv4(ip)) => ip.is_loopback(),
            Some(Host::Ipv6(ip)) => ip.is_loopback(),
            _ => false,
        };
        if endpoint.scheme() != "http"
            || !loopback
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.path() != "/"
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            bail!("Phi speech worker must use an HTTP loopback IP origin without credentials, path, query, or fragment");
        }
        if !(32..=256).contains(&token.len())
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            bail!("Phi speech worker token must contain 32–256 URL-safe ASCII characters");
        }
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(10))
            .build()
            .context("failed to create Phi speech worker client")?;
        Ok(Self(Some(Worker {
            endpoint,
            token,
            client,
        })))
    }

    fn worker(&self) -> ApiResult<&Worker> {
        self.0.as_ref().ok_or_else(|| {
            failure(
                StatusCode::SERVICE_UNAVAILABLE,
                "not_configured",
                "Local ONNX speech is not configured",
            )
        })
    }
}

pub(super) fn routes() -> Router<Arc<EvolveServer>> {
    Router::new()
        .route("/api/speech/capabilities", get(capabilities))
        .route("/api/speech/jobs", post(create))
        .route("/api/speech/jobs/:id", get(status))
        .route("/api/speech/jobs/:id/cancel", post(cancel))
        .route("/api/speech/jobs/:id/audio", get(audio))
        .layer(DefaultBodyLimit::max(JSON_LIMIT))
}

fn failure(status: StatusCode, code: &str, message: &str) -> ApiError {
    (
        status,
        Json(json!({"status":"error", "error": {"code":code, "message":message}})),
    )
}

fn worker_failure(error: reqwest::Error) -> ApiError {
    // reqwest errors can include URLs; do not expose worker details to clients.
    if error.is_timeout() {
        failure(
            StatusCode::GATEWAY_TIMEOUT,
            "worker_timeout",
            "Local speech worker timed out",
        )
    } else {
        failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "worker_unavailable",
            "Local speech worker is unavailable",
        )
    }
}

fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn job_path(id: &str, suffix: &str) -> ApiResult<String> {
    if !valid_id(id) {
        return Err(failure(
            StatusCode::BAD_REQUEST,
            "invalid_job_id",
            "Speech job ID must be 32 lowercase hexadecimal characters",
        ));
    }
    Ok(format!("/api/speech/jobs/{id}{suffix}"))
}

impl Worker {
    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> ApiResult<reqwest::Response> {
        let mut url = self.endpoint.clone();
        url.set_path(path);
        let mut request = self.client.request(method, url).bearer_auth(&self.token);
        if let Some(body) = body {
            request = request.json(body);
        }
        request.send().await.map_err(worker_failure)
    }

    async fn json(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> ApiResult<(StatusCode, Json<Value>)> {
        let response = self.request(method, path, body).await?;
        let status = response.status();
        if status.is_redirection() {
            return Err(failure(
                StatusCode::BAD_GATEWAY,
                "invalid_worker_response",
                "Local speech worker returned an unexpected redirect",
            ));
        }
        let bytes = read_limited(response, JSON_LIMIT).await?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            failure(
                StatusCode::BAD_GATEWAY,
                "invalid_worker_response",
                "Local speech worker returned invalid JSON",
            )
        })?;
        if !value.is_object() {
            return Err(failure(
                StatusCode::BAD_GATEWAY,
                "invalid_worker_response",
                "Local speech worker returned a non-object response",
            ));
        }
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(failure(
                StatusCode::BAD_GATEWAY,
                "worker_auth_failed",
                "Local speech worker authentication failed",
            ));
        }
        Ok((status, Json(value)))
    }
}

async fn read_limited(mut response: reqwest::Response, limit: usize) -> ApiResult<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(failure(
            StatusCode::BAD_GATEWAY,
            "worker_response_too_large",
            "Local speech worker response exceeds the size limit",
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(worker_failure)? {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err(failure(
                StatusCode::BAD_GATEWAY,
                "worker_response_too_large",
                "Local speech worker response exceeds the size limit",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn capabilities(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
) -> ApiResult<(StatusCode, Json<Value>)> {
    require_session(&headers, &server)?;
    let Some(worker) = &server.speech.0 else {
        return Ok((
            StatusCode::OK,
            Json(
                json!({"configured":false,"status":"unavailable","reason":"not_configured","voices":[]}),
            ),
        ));
    };
    worker
        .json(Method::GET, "/api/speech/capabilities", None)
        .await
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CreateJob {
    text: String,
    voice: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    request_id: Option<String>,
}

async fn create(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
    payload: Result<Json<CreateJob>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    require_session(&headers, &server)?;
    let Json(job) = payload.map_err(|error| {
        failure(
            error.status(),
            "invalid_request",
            "Expected a speech request with text and voice",
        )
    })?;
    if job.text.trim().is_empty()
        || job.text.chars().count() > TEXT_LIMIT
        || job.voice.is_empty()
        || job.voice.len() > 64
        || job.request_id.as_deref().is_some_and(|id| !valid_id(id))
        || !job
            .voice
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return Err(failure(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Speech requires 1–5000 text characters, a valid voice, and an optional 32-character lowercase hexadecimal request_id",
        ));
    }
    let body = serde_json::to_value(job).expect("speech request serialization is infallible");
    server
        .speech
        .worker()?
        .json(Method::POST, "/api/speech/jobs", Some(&body))
        .await
}

async fn status(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    require_session(&headers, &server)?;
    let path = job_path(&id, "")?;
    server.speech.worker()?.json(Method::GET, &path, None).await
}

async fn cancel(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    require_session(&headers, &server)?;
    let path = job_path(&id, "/cancel")?;
    server
        .speech
        .worker()?
        .json(Method::POST, &path, Some(&json!({})))
        .await
}

async fn audio(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    require_session(&headers, &server)?;
    let path = job_path(&id, "/audio")?;
    let response = server
        .speech
        .worker()?
        .request(Method::GET, &path, None)
        .await?;
    if !response.status().is_success() {
        let status = match response.status() {
            StatusCode::NOT_FOUND => StatusCode::NOT_FOUND,
            StatusCode::CONFLICT | StatusCode::GONE => StatusCode::CONFLICT,
            _ => StatusCode::BAD_GATEWAY,
        };
        return Err(failure(
            status,
            "audio_unavailable",
            "Speech audio is not available",
        ));
    }
    let mime = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if mime.split(';').next().map(str::trim) != Some("audio/wav") {
        return Err(failure(
            StatusCode::BAD_GATEWAY,
            "invalid_audio",
            "Local speech worker did not return WAV audio",
        ));
    }
    let bytes = read_limited(response, AUDIO_LIMIT).await?;
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(failure(
            StatusCode::BAD_GATEWAY,
            "invalid_audio",
            "Local speech worker returned invalid WAV audio",
        ));
    }
    Ok(([("content-type", "audio/wav")], bytes).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use std::sync::Mutex;
    use tower::ServiceExt;

    const ID: &str = "0123456789abcdef0123456789abcdef";
    const TOKEN: &str = "worker_fixture_token_0123456789abcdef";
    type ObservedRequest = (String, String, String, Value);

    struct Fixture {
        bridge: SpeechBridge,
        requests: Arc<Mutex<Vec<ObservedRequest>>>,
        task: tokio::task::JoinHandle<()>,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    async fn worker(
        status: StatusCode,
        mime: &'static str,
        body: Vec<u8>,
        chunked: bool,
    ) -> Fixture {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let router = Router::new().fallback(move |request: Request<Body>| {
            let requests = recorded.clone();
            let body = body.clone();
            async move {
                let method = request.method().to_string();
                let path = request.uri().to_string();
                let auth = request
                    .headers()
                    .get("authorization")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned();
                let request_body = axum::body::to_bytes(request.into_body(), JSON_LIMIT)
                    .await
                    .unwrap();
                let payload = serde_json::from_slice(&request_body).unwrap_or(Value::Null);
                requests.lock().unwrap().push((method, path, auth, payload));
                let body = if chunked {
                    Body::from_stream(futures::stream::iter([
                        Ok::<_, std::convert::Infallible>(body[..body.len() / 2].to_vec()),
                        Ok(body[body.len() / 2..].to_vec()),
                    ]))
                } else {
                    Body::from(body)
                };
                Response::builder()
                    .status(status)
                    .header("content-type", mime)
                    .header("location", "http://192.0.2.1/unreachable")
                    .body(body)
                    .unwrap()
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Fixture {
            bridge: SpeechBridge::configured(&endpoint, TOKEN.into()).unwrap(),
            requests,
            task,
        }
    }

    fn make_server(bridge: SpeechBridge) -> (tempfile::TempDir, EvolveServer) {
        let root = tempfile::tempdir().unwrap();
        let graph = crate::evolve::Graph {
            nodes: vec![],
            edges: vec![],
        };
        let mut server = EvolveServer::for_project(graph, root.path()).unwrap();
        server.speech = bridge;
        (root, server)
    }

    async fn call(
        server: &EvolveServer,
        method: &str,
        path: &str,
        body: Value,
        authorized: bool,
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        let mut request = Request::builder()
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
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), AUDIO_LIMIT + 1)
            .await
            .unwrap()
            .to_vec();
        (status, headers, body)
    }

    fn request() -> Value {
        json!({"text":"Hello from Phi.","voice":"Emma"})
    }

    #[test]
    fn speech_configuration_requires_authenticated_loopback_origin() {
        for endpoint in [
            "http://localhost:8766",
            "http://192.0.2.1:8766",
            "https://127.0.0.1:8766",
            "http://user@127.0.0.1:8766",
            "http://127.0.0.1:8766/path",
            "http://127.0.0.1:8766/?token=x",
            "http://127.0.0.1:8766/#x",
        ] {
            assert!(
                SpeechBridge::configured(endpoint, TOKEN.into()).is_err(),
                "{endpoint}"
            );
        }
        for token in [
            "",
            "short",
            "has spaces and is deliberately long enough",
            "secret\r\nwith_header_injection_0123456789",
        ] {
            assert!(SpeechBridge::configured("http://127.0.0.1:8766", token.into()).is_err());
        }
        assert!(SpeechBridge::configured("http://127.0.0.1:8766", TOKEN.into()).is_ok());
        assert!(SpeechBridge::configured("http://[::1]:8766", TOKEN.into()).is_ok());
    }

    #[tokio::test]
    async fn speech_all_routes_require_session_and_disabled_is_explicit() {
        let (_root, server) = make_server(SpeechBridge::default());
        for (method, path) in [
            ("GET", "/api/speech/capabilities".to_string()),
            ("POST", "/api/speech/jobs".to_string()),
            ("GET", format!("/api/speech/jobs/{ID}")),
            ("POST", format!("/api/speech/jobs/{ID}/cancel")),
            ("GET", format!("/api/speech/jobs/{ID}/audio")),
        ] {
            assert_eq!(
                call(&server, method, &path, request(), false).await.0,
                StatusCode::UNAUTHORIZED,
                "{path}"
            );
        }
        let (status, _, bytes) = call(
            &server,
            "GET",
            "/api/speech/capabilities",
            Value::Null,
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            json!({"configured":false,"status":"unavailable","reason":"not_configured","voices":[]})
        );
        assert_eq!(
            call(&server, "POST", "/api/speech/jobs", request(), true)
                .await
                .0,
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[tokio::test]
    async fn speech_job_lifecycle_forwards_only_server_bearer() {
        let fixture = worker(
            StatusCode::ACCEPTED,
            "application/json",
            json!({"id":ID,"status":"queued"}).to_string().into_bytes(),
            false,
        )
        .await;
        let (_root, server) = make_server(fixture.bridge.clone());
        for (method, path) in [
            ("GET", "/api/speech/capabilities".to_string()),
            ("POST", "/api/speech/jobs".to_string()),
            ("GET", format!("/api/speech/jobs/{ID}")),
            ("POST", format!("/api/speech/jobs/{ID}/cancel")),
        ] {
            let (status, headers, bytes) = call(&server, method, &path, request(), true).await;
            assert_eq!(status, StatusCode::ACCEPTED);
            assert_eq!(headers["cache-control"], "no-store");
            assert_eq!(
                serde_json::from_slice::<Value>(&bytes).unwrap()["status"],
                "queued"
            );
            assert!(!String::from_utf8(bytes).unwrap().contains(TOKEN));
        }
        let requests = fixture.requests.lock().unwrap();
        assert_eq!(requests.len(), 4);
        assert!(requests
            .iter()
            .all(|entry| entry.2 == format!("Bearer {TOKEN}")));
        assert_eq!(requests[1].3, request());
        assert_eq!(
            (&requests[1].0, &requests[1].1),
            (&"POST".to_string(), &"/api/speech/jobs".to_string())
        );
        assert_eq!(requests[2].1, format!("/api/speech/jobs/{ID}"));
        assert_eq!(requests[3].1, format!("/api/speech/jobs/{ID}/cancel"));
        assert_eq!(requests[3].3, json!({}));
    }

    #[tokio::test]
    async fn speech_rejects_invalid_job_ids_and_requests_before_worker() {
        let fixture = worker(StatusCode::OK, "application/json", b"{}".to_vec(), false).await;
        let (_root, server) = make_server(fixture.bridge.clone());
        for suffix in ["", "/cancel", "/audio"] {
            let method = if suffix == "/cancel" { "POST" } else { "GET" };
            assert_eq!(
                call(
                    &server,
                    method,
                    &format!("/api/speech/jobs/not-a-job{suffix}"),
                    Value::Null,
                    true
                )
                .await
                .0,
                StatusCode::BAD_REQUEST
            );
        }
        for payload in [
            json!({"text":" ","voice":"Emma"}),
            json!({"text":"x".repeat(TEXT_LIMIT+1),"voice":"Emma"}),
            json!({"text":"Hello","voice":"../voice"}),
            json!({"text":"Hello","voice":"Emma","endpoint":"http://192.0.2.1"}),
        ] {
            assert!(call(&server, "POST", "/api/speech/jobs", payload, true)
                .await
                .0
                .is_client_error());
        }
        assert!(fixture.requests.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn speech_optional_request_id_is_forwarded_and_invalid_ids_never_reach_worker() {
        let fixture = worker(
            StatusCode::ACCEPTED,
            "application/json",
            json!({"id":ID,"status":"queued"}).to_string().into_bytes(),
            false,
        )
        .await;
        let (_root, server) = make_server(fixture.bridge.clone());
        let mut payload = request();
        payload["request_id"] = json!(ID);
        assert_eq!(
            call(&server, "POST", "/api/speech/jobs", payload.clone(), true)
                .await
                .0,
            StatusCode::ACCEPTED
        );
        assert_eq!(fixture.requests.lock().unwrap()[0].3, payload);
        for id in [
            "",
            "../cancel",
            "0123456789ABCDEF0123456789ABCDEF",
            "01234567-89ab-cdef-0123-456789abcdef",
        ] {
            payload["request_id"] = json!(id);
            assert_eq!(
                call(&server, "POST", "/api/speech/jobs", payload.clone(), true)
                    .await
                    .0,
                StatusCode::BAD_REQUEST
            );
        }
        assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn speech_idempotency_conflict_is_preserved_for_the_client() {
        let failure = json!({"status":"failed","error":{"code":"request_id_conflict","message":"Request ID is already bound to another text"}});
        let fixture = worker(
            StatusCode::CONFLICT,
            "application/json",
            failure.to_string().into_bytes(),
            false,
        )
        .await;
        let (_root, server) = make_server(fixture.bridge.clone());
        let mut payload = request();
        payload["request_id"] = json!(ID);
        let (status, _, bytes) = call(&server, "POST", "/api/speech/jobs", payload, true).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), failure);
        assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn speech_rejects_redirect_invalid_json_and_oversized_chunked_json() {
        for (status, body, chunked, code) in [
            (
                StatusCode::TEMPORARY_REDIRECT,
                b"{}".to_vec(),
                false,
                "invalid_worker_response",
            ),
            (
                StatusCode::OK,
                b"not json".to_vec(),
                false,
                "invalid_worker_response",
            ),
            (
                StatusCode::OK,
                b"[]".to_vec(),
                false,
                "invalid_worker_response",
            ),
            (
                StatusCode::OK,
                vec![b' '; JSON_LIMIT + 1],
                false,
                "worker_response_too_large",
            ),
            (
                StatusCode::OK,
                vec![b' '; JSON_LIMIT + 1],
                true,
                "worker_response_too_large",
            ),
            (
                StatusCode::UNAUTHORIZED,
                b"{}".to_vec(),
                false,
                "worker_auth_failed",
            ),
        ] {
            let fixture = worker(status, "application/json", body, chunked).await;
            let (_root, server) = make_server(fixture.bridge.clone());
            let (status, _, bytes) = call(
                &server,
                "GET",
                "/api/speech/capabilities",
                Value::Null,
                true,
            )
            .await;
            assert_eq!(status, StatusCode::BAD_GATEWAY);
            assert_eq!(
                serde_json::from_slice::<Value>(&bytes).unwrap()["error"]["code"],
                code
            );
            assert_eq!(fixture.requests.lock().unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn speech_audio_is_private_bounded_wav_and_not_an_arbitrary_proxy() {
        let wav = b"RIFF\x24\x00\x00\x00WAVEfmt \x10\x00\x00\x00\x01\x00\x01\x00\xc0\x5d\x00\x00\x80\xbb\x00\x00\x02\x00\x10\x00data\x00\x00\x00\x00";
        let fixture = worker(StatusCode::OK, "audio/wav", wav.to_vec(), false).await;
        let (_root, server) = make_server(fixture.bridge.clone());
        let path = format!("/api/speech/jobs/{ID}/audio");
        assert_eq!(
            call(&server, "GET", &path, Value::Null, false).await.0,
            StatusCode::UNAUTHORIZED
        );
        assert!(fixture.requests.lock().unwrap().is_empty());
        let (status, headers, bytes) = call(&server, "GET", &path, Value::Null, true).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers["content-type"], "audio/wav");
        assert_eq!(bytes, wav);
        assert_eq!(fixture.requests.lock().unwrap()[0].1, path);
        for (mime, body) in [
            ("text/html", wav.to_vec()),
            ("audio/wav", b"not a WAV".to_vec()),
            ("audio/wav", vec![0; AUDIO_LIMIT + 1]),
        ] {
            let fixture = worker(StatusCode::OK, mime, body, false).await;
            let (_root, server) = make_server(fixture.bridge.clone());
            assert_eq!(
                call(&server, "GET", &path, Value::Null, true).await.0,
                StatusCode::BAD_GATEWAY
            );
        }
    }

    #[tokio::test]
    async fn speech_connection_failure_retains_unavailable_status() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let (_root, server) =
            make_server(SpeechBridge::configured(&endpoint, TOKEN.into()).unwrap());
        let (status, _, bytes) = call(
            &server,
            "GET",
            "/api/speech/capabilities",
            Value::Null,
            true,
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap()["error"]["code"],
            "worker_unavailable"
        );
        assert!(!String::from_utf8(bytes).unwrap().contains(&endpoint));
    }
}
