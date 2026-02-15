use selfware::agent::context::ContextCompressor;
use selfware::api::types::Message;

#[test]
fn test_compression_threshold_small() {
    let compressor = ContextCompressor::new(100000);

    // Small messages should not trigger compression
    let small: Vec<Message> = vec![Message::system("test")];
    assert!(!compressor.should_compress(&small));
}

#[test]
fn test_compression_threshold_large() {
    // Use a smaller budget to make it easier to exceed
    let compressor = ContextCompressor::new(1000);
    // threshold = 1000 * 0.85 = 850 tokens

    // Create messages that will exceed the threshold
    // Each message: ~4000 chars / 4 factor + 50 = ~1050 tokens per message
    let large: Vec<Message> = vec![
        Message::system("A".repeat(4000)),
        Message::user("B".repeat(4000)),
    ];

    assert!(
        compressor.should_compress(&large),
        "Large messages should trigger compression"
    );
}

#[test]
fn test_estimate_tokens_text() {
    let compressor = ContextCompressor::new(10000);
    let messages = vec![Message::user("hello world")]; // 11 chars / 4 + 50 = 52 tokens

    let tokens = compressor.estimate_tokens(&messages);
    assert!(tokens > 50 && tokens < 100);
}

#[test]
fn test_estimate_tokens_code() {
    let compressor = ContextCompressor::new(10000);
    // Code uses factor 3 (contains { or ;)
    let messages = vec![Message::user("fn main() { println!(); }")]; // ~26 chars

    let tokens = compressor.estimate_tokens(&messages);
    // 26/3 + 50 = ~58 tokens
    assert!(tokens > 55 && tokens < 70);
}

#[test]
fn test_hard_compress_preserves_recent() {
    let compressor = ContextCompressor::new(100000);
    let messages = vec![
        Message::system("system"),
        Message::user("old1"),
        Message::assistant("old2"),
        Message::user("old3"),
        Message::assistant("old4"),
        Message::user("old5"),
        Message::assistant("old6"),
        Message::user("recent1"),
        Message::assistant("recent2"),
    ];

    let compressed = compressor.hard_compress(&messages);
    // Should preserve: system + min_messages_to_keep(6) recent + compression note
    assert!(
        compressed.len() >= 2,
        "Should preserve at least system and some recent"
    );
    assert_eq!(compressed[0].role, "system");
}

#[test]
fn test_hard_compress_adds_compression_note() {
    let compressor = ContextCompressor::new(100000);
    let messages = vec![Message::system("system"), Message::user("user1")];

    // hard_compress always adds compression note
    let compressed = compressor.hard_compress(&messages);
    // Result: system + compression note + last 3 messages + potential continuation prompt
    assert!(
        compressed.len() >= 2,
        "Should have at least system and note"
    );
    assert_eq!(compressed[0].role, "system");
    // Second message should be the compression note
    assert!(compressed[1].content.contains("compressed"));
}
