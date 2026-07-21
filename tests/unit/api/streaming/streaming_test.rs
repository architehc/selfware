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
