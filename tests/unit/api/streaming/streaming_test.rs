use super::{append_utf8_chunk, parse_sse_event, StreamChunk, ToolCallAccumulator};
use std::sync::{Arc, Mutex};

#[test]
fn append_utf8_chunk_preserves_split_multibyte_codepoint() {
    let mut buffer = String::new();
    let mut pending = Vec::new();
    let text = "data: hello 🦀\n\n";
    let bytes = text.as_bytes();
    let split = text.find('🦀').unwrap() + 1;

    append_utf8_chunk(&mut buffer, &mut pending, &bytes[..split]);
    assert!(!pending.is_empty());

    append_utf8_chunk(&mut buffer, &mut pending, &bytes[split..]);
    assert_eq!(buffer, text);
    assert!(pending.is_empty());
}

#[test]
fn append_utf8_chunk_replaces_invalid_bytes_with_replacement_char() {
    // A provider sending genuinely malformed UTF-8 (0xFF/0xFE can never
    // start a valid sequence) must not stall the stream: each maximal
    // invalid subsequence is replaced with U+FFFD and decoding continues.
    let mut buffer = String::new();
    let mut pending = Vec::new();

    append_utf8_chunk(&mut buffer, &mut pending, b"data: \xff\xfe\n\n");

    assert_eq!(buffer, "data: \u{FFFD}\u{FFFD}\n\n");
    assert!(pending.is_empty());
}

#[test]
fn parse_sse_event_handles_crlf_delimiters() {
    let event = "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\r\n\r\n";
    let mut acc = ToolCallAccumulator::new();
    let chunks = parse_sse_event(event, &mut acc);
    assert_eq!(chunks.len(), 1);
    assert!(matches!(&chunks[0], StreamChunk::Content(text) if text == "hello"));
}

#[test]
fn parse_sse_event_handles_mid_stream_error() {
    let event = "data: {\"error\":{\"message\":\"boom\"}}\n\n";
    let mut acc = ToolCallAccumulator::new();
    let chunks = parse_sse_event(event, &mut acc);
    assert_eq!(chunks.len(), 1);
    assert!(matches!(&chunks[0], StreamChunk::Error(msg) if msg == "boom"));
}

#[test]
fn parse_sse_event_accepts_no_space_after_data_prefix() {
    let event = "data:{\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n";
    let mut acc = ToolCallAccumulator::new();
    let chunks = parse_sse_event(event, &mut acc);
    assert_eq!(chunks.len(), 1);
    assert!(matches!(&chunks[0], StreamChunk::Content(text) if text == "hi"));
}

#[test]
fn parse_sse_event_accepts_no_space_done_sentinel() {
    let event = "data:[DONE]\n\n";
    let mut acc = ToolCallAccumulator::new();
    let chunks = parse_sse_event(event, &mut acc);
    assert_eq!(chunks.len(), 1);
    assert!(matches!(&chunks[0], StreamChunk::Done));
}

#[test]
fn parse_sse_event_joins_multiline_data_field_per_sse_spec() {
    // SSE spec: a data field split across multiple `data:` lines is the
    // lines joined by \n. A provider that splits one JSON payload at a
    // token boundary (here: after the choices array's comma) must not have
    // its content silently dropped. Under the old per-line behavior both
    // halves were invalid JSON and the whole event was lost.
    let event = "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"}}],\ndata: \"usage\":{\"prompt_tokens\":5,\"completion_tokens\":2,\"total_tokens\":7}}";
    let mut acc = ToolCallAccumulator::new();
    let chunks = parse_sse_event(event, &mut acc);
    assert_eq!(chunks.len(), 2);
    assert!(matches!(&chunks[0], StreamChunk::Content(text) if text == "hello"));
    assert!(matches!(&chunks[1], StreamChunk::Usage(u, _) if u.total_tokens == 7));
}

#[test]
fn parse_sse_event_preserves_logprobs_and_flat_reasoning_tokens() {
    let event = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"yes\"},\"logprobs\":{\"tokens\":[\"yes\"],\"logprobs\":[-0.01]},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":1,\"total_tokens\":11,\"reasoning_tokens\":5}}\n\n";
    let mut acc = ToolCallAccumulator::new();
    let chunks = parse_sse_event(event, &mut acc);

    let mut found_content = false;
    let mut found_logprobs = false;
    let mut found_usage = false;
    for c in chunks {
        match c {
            StreamChunk::Content(t) if t == "yes" => found_content = true,
            StreamChunk::Logprobs(lp) => {
                assert_eq!(lp["tokens"][0], "yes");
                found_logprobs = true;
            }
            StreamChunk::Usage(u, _) => {
                assert_eq!(u.reasoning_tokens, Some(5));
                assert_eq!(u.reasoning_tokens(), Some(5));
                found_usage = true;
            }
            _ => {}
        }
    }
    assert!(found_content, "content must survive SSE parse");
    assert!(found_logprobs, "logprobs must survive SSE parse");
    assert!(found_usage, "flat reasoning tokens must survive SSE parse");
}

#[tokio::test]
async fn multi_token_stream_collection_merges_logprobs() {
    use super::StreamingResponse;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let sse_data = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"logprobs\":{\"content\":[{\"token\":\"Hello\",\"logprob\":-0.05}]}}]}\n\n\
                    data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"},\"logprobs\":{\"content\":[{\"token\":\" world\",\"logprob\":-0.12}]}}]}\n\n\
                    data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":2,\"total_tokens\":7}}\n\n\
                    data: [DONE]\n\n";

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        let _ = socket.read(&mut buf).await;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{}",
            sse_data
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{}/stream", addr))
        .send()
        .await
        .unwrap();

    let streaming_resp = StreamingResponse::new(resp, std::time::Duration::from_secs(5), None);
    let chat_resp = streaming_resp.collect().await.unwrap();
    server.await.unwrap();

    assert_eq!(chat_resp.choices.len(), 1);
    assert_eq!(chat_resp.choices[0].message.content, "Hello world");
    assert_eq!(chat_resp.choices[0].finish_reason.as_deref(), Some("stop"));

    let logprobs = chat_resp.choices[0]
        .logprobs
        .as_ref()
        .expect("logprobs must survive multi-chunk stream");
    let content_tokens = logprobs["content"]
        .as_array()
        .expect("logprobs.content must be an array");
    assert_eq!(
        content_tokens.len(),
        2,
        "both token logprobs must be retained"
    );
    assert_eq!(content_tokens[0]["token"], "Hello");
    assert_eq!(content_tokens[1]["token"], " world");
}

#[tokio::test]
async fn test_stream_collection_merges_complete_then_partial() {
    use super::StreamingResponse;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let sse_data = "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}],\"usage\":{\"prompt_tokens\":100,\"completion_tokens\":50,\"total_tokens\":150,\"reasoning_tokens\":30,\"completion_tokens_details\":{\"reasoning_tokens\":30}}}\n\n\
                    data: {\"choices\":[{\"delta\":{\"content\":\" world\"}}],\"usage\":{\"prompt_tokens\":100}}\n\n\
                    data: [DONE]\n\n";

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        let _ = socket.read(&mut buf).await.unwrap();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{}",
            sse_data
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{}/stream", addr))
        .send()
        .await
        .unwrap();

    let streaming_resp = StreamingResponse::new(resp, std::time::Duration::from_secs(5), None);
    let chat_resp = streaming_resp.collect().await.unwrap();
    server.await.unwrap();

    // Earlier complete snapshot must not be wiped out by subsequent partial snapshot
    assert_eq!(chat_resp.usage.prompt_tokens, 100);
    assert_eq!(chat_resp.usage.completion_tokens, 50);
    assert_eq!(chat_resp.usage.total_tokens, 150);
    assert_eq!(chat_resp.usage.reasoning_tokens(), Some(30));
    assert!(chat_resp.usage.completion_tokens_details.is_some());
}

#[tokio::test]
async fn test_stream_collection_handles_total_only_and_partial_sequences() {
    use super::StreamingResponse;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Emits prompt_tokens first, then total_tokens without completion_tokens
    let sse_data = "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}],\"usage\":{\"prompt_tokens\":100}}\n\n\
                    data: {\"choices\":[{\"delta\":{\"content\":\" world\"}}],\"usage\":{\"total_tokens\":150}}\n\n\
                    data: [DONE]\n\n";

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        let _ = socket.read(&mut buf).await.unwrap();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{}",
            sse_data
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{}/stream", addr))
        .send()
        .await
        .unwrap();

    let streaming_resp = StreamingResponse::new(resp, std::time::Duration::from_secs(5), None);
    let chat_resp = streaming_resp.collect().await.unwrap();
    server.await.unwrap();

    // Both partial snapshots must be merged without being rejected or zeroed out
    assert_eq!(chat_resp.usage.prompt_tokens, 100);
    assert_eq!(chat_resp.usage.total_tokens, 150);
}

#[tokio::test]
async fn test_stream_collection_retains_reported_components_on_inconsistent_total() {
    use super::StreamingResponse;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Emits a chunk with prompt=100, completion=20, total=0, reasoning=10
    // Total is inconsistent with prompt+completion, but components and details must NOT be discarded.
    let sse_data = "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}],\"usage\":{\"prompt_tokens\":100,\"completion_tokens\":20,\"total_tokens\":0,\"completion_tokens_details\":{\"reasoning_tokens\":10}}}\n\n\
                    data: [DONE]\n\n";

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        let _ = socket.read(&mut buf).await.unwrap();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{}",
            sse_data
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{}/stream", addr))
        .send()
        .await
        .unwrap();

    let streaming_resp = StreamingResponse::new(resp, std::time::Duration::from_secs(5), None);
    let chat_resp = streaming_resp.collect().await.unwrap();
    server.await.unwrap();

    assert_eq!(chat_resp.usage.prompt_tokens, 100);
    assert_eq!(chat_resp.usage.completion_tokens, 20);
    assert_eq!(chat_resp.usage.reasoning_tokens(), Some(10));
    // Derived total should reconcile to prompt + completion (120)
    assert_eq!(chat_resp.usage.total_tokens, 120);
}

// ── Truncated streams must fail typed, never read as success ─────────────
//
// Regression (2026-09-21 review, P2): a mock sent one valid content event
// and closed — no [DONE], no finish_reason. collect() returned Ok with the
// half-written prose and finish_reason=None, and the runtime synthesized
// "stream_end", accepting truncated tool calls and partial prose as
// completed work. Every stream must reach an accepted terminal indication
// ([DONE] or a provider finish_reason) or fail typed.

#[tokio::test]
async fn test_stream_collection_truncated_after_one_content_event_is_error() {
    use super::StreamingResponse;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // The review's exact probe shape: one content event, then the mock
    // closes the connection. No [DONE], no finish_reason.
    let sse_data = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"The incomplete answer is\"}}]}\n\n";

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        let _ = socket.read(&mut buf).await.unwrap();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{}",
            sse_data
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{}/stream", addr))
        .send()
        .await
        .unwrap();

    let streaming_resp = StreamingResponse::new(resp, std::time::Duration::from_secs(5), None);
    let err = streaming_resp
        .collect()
        .await
        .expect_err("a stream with no terminal indication must not succeed");
    server.await.unwrap();

    assert!(
        err.chain().any(|c| matches!(
            c.downcast_ref::<crate::errors::ApiError>(),
            Some(crate::errors::ApiError::Parse(_))
        )),
        "expected a typed ApiError::Parse incomplete-stream outcome, got: {err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("accepted terminal indication"),
        "the error must name the missing terminal indication, got: {msg}"
    );
    assert!(
        msg.contains("usage retained"),
        "the typed outcome must retain the accumulated usage, got: {msg}"
    );
}

#[tokio::test]
async fn test_stream_collection_truncated_partial_tool_call_is_error() {
    use super::StreamingResponse;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // A tool-call delta whose arguments are cut off mid-JSON, then the
    // connection drops before [DONE]/finish_reason: the half-formed call
    // must not be returned as a successful completion.
    let sse_data = "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"file_read\",\"arguments\":\"{\\\"path\\\": \\\"partial\"}}]}}]}\n\n";

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        let _ = socket.read(&mut buf).await.unwrap();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{}",
            sse_data
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{}/stream", addr))
        .send()
        .await
        .unwrap();

    let streaming_resp = StreamingResponse::new(resp, std::time::Duration::from_secs(5), None);
    let err = streaming_resp
        .collect()
        .await
        .expect_err("a stream cut off mid-tool-call must not succeed");
    server.await.unwrap();
    assert!(
        err.chain().any(|c| matches!(
            c.downcast_ref::<crate::errors::ApiError>(),
            Some(crate::errors::ApiError::Parse(_))
        )),
        "expected a typed incomplete-stream outcome, got: {err:?}"
    );
}

#[tokio::test]
async fn test_stream_collection_finish_reason_without_done_is_accepted() {
    use super::StreamingResponse;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Some providers end the stream with a finish_reason choice and NO
    // [DONE] sentinel. The finish_reason IS the accepted terminal
    // indication — this must stay Ok (collect()'s contract, kept from the
    // pre-fix behavior: "clean EOF after valid SSE events is accepted,
    // including providers that finish with finish_reason but no [DONE]").
    let sse_data = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Complete answer\"}}]}\n\n\
                    data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":2,\"total_tokens\":6}}\n\n";

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        let _ = socket.read(&mut buf).await.unwrap();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{}",
            sse_data
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{}/stream", addr))
        .send()
        .await
        .unwrap();

    let streaming_resp = StreamingResponse::new(resp, std::time::Duration::from_secs(5), None);
    let chat_resp = streaming_resp.collect().await.unwrap();
    server.await.unwrap();

    assert_eq!(chat_resp.choices[0].message.content, "Complete answer");
    assert_eq!(chat_resp.choices[0].finish_reason.as_deref(), Some("stop"));
    assert_eq!(chat_resp.usage.total_tokens, 6);
}

/// SGLang emits `"usage": null` on streaming chunks that carry no usage
/// report. This must not warn per token/chunk (2026-09-21 review, P2) and
/// must not fabricate a zeroed Usage chunk — content keeps flowing and no
/// "Failed to parse streamed usage" warning is emitted.
#[test]
fn null_usage_chunks_are_skipped_silently_without_warning() {
    use super::parse_sse_event;

    let captured: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    // Capture everything the subscriber would emit at warn level or below.
    let writer = Arc::clone(&captured);
    let filter = tracing_subscriber::EnvFilter::new("warn");
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(move || {
            let w = Arc::clone(&writer);
            std::io::BufWriter::new(CaptureWriter(w))
        })
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        let mut acc = ToolCallAccumulator::new();
        // One content chunk with `"usage": null` (the SGLang shape), then
        // a normal finish. Under the old code the null chunk warned.
        let event =
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"}}],\"usage\":null}\n\n";
        let chunks = parse_sse_event(event, &mut acc);
        assert_eq!(chunks.len(), 1, "null usage must not emit extra chunks");
        assert!(
            matches!(&chunks[0], StreamChunk::Content(t) if t == "hi"),
            "content must survive a null-usage chunk, got: {chunks:?}"
        );
        assert!(
            !chunks.iter().any(|c| matches!(c, StreamChunk::Usage(_, _))),
            "a null usage report is 'not reported', not a zeroed Usage chunk"
        );
    });

    let out = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
    assert!(
        !out.contains("Failed to parse streamed usage"),
        "null usage must be skipped silently, captured output was: {out}"
    );
}

struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// -----------------------------------------------------------------------
// classify_stream_timeout: run deadline vs per-call cap vs plain timeout
// -----------------------------------------------------------------------

mod stream_timeout_classification {
    use super::super::classify_stream_timeout;
    use crate::api::client::{CallTimeBudgetExceeded, WallClockBudgetExceeded};
    use crate::errors::ApiError;
    use std::time::{Duration, Instant};

    #[test]
    fn wall_clock_fires_iff_run_deadline_passed_with_run_elapsed() {
        let now = Instant::now() + Duration::from_secs(10_000);
        // Late in a 600 s run: this call only started 10 s ago, but the RUN
        // deadline passed 5 s ago. The old per-call comparison (10 s < 600 s)
        // mis-typed this as a plain timeout.
        let err = classify_stream_timeout(
            now,
            Some(now - Duration::from_secs(10)),
            None,
            Some(now - Duration::from_secs(5)),
            Some(600),
        );
        let wall = err
            .downcast_ref::<WallClockBudgetExceeded>()
            .unwrap_or_else(|| panic!("expected WallClockBudgetExceeded, got {err}"));
        assert_eq!(wall.limit_secs, 600);
        assert_eq!(wall.elapsed_secs, 605, "run-elapsed, not call-elapsed");
    }

    #[test]
    fn long_call_before_run_deadline_is_not_a_wall_clock_stop() {
        let now = Instant::now() + Duration::from_secs(10_000);
        // The call itself ran 700 s (> the 600 s limit) but the run deadline
        // is still 100 s away: the old comparison claimed the wall budget
        // was exhausted while budget remained.
        let err = classify_stream_timeout(
            now,
            Some(now - Duration::from_secs(700)),
            None,
            Some(now + Duration::from_secs(100)),
            Some(600),
        );
        assert!(err.downcast_ref::<WallClockBudgetExceeded>().is_none());
        assert!(matches!(
            err.downcast_ref::<ApiError>(),
            Some(ApiError::Timeout)
        ));
    }

    #[test]
    fn per_call_cap_fires_when_run_deadline_not_passed() {
        let now = Instant::now() + Duration::from_secs(10_000);
        let err = classify_stream_timeout(
            now,
            Some(now - Duration::from_secs(31)),
            Some(30),
            Some(now + Duration::from_secs(100)),
            Some(600),
        );
        let cap = err
            .downcast_ref::<CallTimeBudgetExceeded>()
            .unwrap_or_else(|| panic!("expected CallTimeBudgetExceeded, got {err}"));
        assert_eq!(cap.limit_secs, 30);
        assert_eq!(cap.elapsed_secs, 31);
    }

    #[test]
    fn run_deadline_takes_precedence_over_call_cap() {
        let now = Instant::now() + Duration::from_secs(10_000);
        let err = classify_stream_timeout(
            now,
            Some(now - Duration::from_secs(31)),
            Some(30),
            Some(now),
            Some(600),
        );
        assert!(err.downcast_ref::<WallClockBudgetExceeded>().is_some());
    }

    #[test]
    fn chunk_stall_without_any_budget_is_plain_timeout() {
        let now = Instant::now() + Duration::from_secs(10_000);
        let err = classify_stream_timeout(
            now,
            Some(now - Duration::from_secs(5)),
            Some(30),
            Some(now + Duration::from_secs(100)),
            Some(600),
        );
        assert!(matches!(
            err.downcast_ref::<ApiError>(),
            Some(ApiError::Timeout)
        ));
        let err = classify_stream_timeout(now, None, None, None, None);
        assert!(matches!(
            err.downcast_ref::<ApiError>(),
            Some(ApiError::Timeout)
        ));
    }
}

#[test]
fn mid_stream_error_message_is_scrubbed_of_echoed_keys() {
    // Review (0.9.1): a streamed {"error":{"message":…}} reached the run's
    // error text raw; only HTTP-status bodies were redacted.
    let mut acc = ToolCallAccumulator::new();
    let event = "data: {\"error\":{\"message\":\"invalid key sk-or-v1-abcdef0123456789abcdef0123456789 for this route\"}}";
    let chunks = parse_sse_event(event, &mut acc);
    let msg = chunks
        .iter()
        .find_map(|c| match c {
            StreamChunk::Error(m) => Some(m.clone()),
            _ => None,
        })
        .expect("error chunk");
    assert!(!msg.contains("abcdef0123456789abcdef"), "{msg}");
    assert!(msg.contains("[REDACTED]"), "{msg}");
    assert!(msg.contains("invalid key"), "non-secret text kept: {msg}");
}
