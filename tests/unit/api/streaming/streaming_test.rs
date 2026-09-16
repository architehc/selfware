use super::{append_utf8_chunk, parse_sse_event, StreamChunk, ToolCallAccumulator};

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
