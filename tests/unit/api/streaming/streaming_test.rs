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
    assert!(matches!(&chunks[1], StreamChunk::Usage(u) if u.total_tokens == 7));
}
