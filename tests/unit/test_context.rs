use selfware::agent::context::ContextCompressor;
use selfware::api::types::Message;

#[test]
fn test_compression_threshold() {
    let compressor = ContextCompressor::new(100000);
    
    let small: Vec<Message> = vec![Message::system("test")];
    assert!(!compressor.should_compress(&small));
    
    let mut large = vec![Message::system("test".repeat(10000))];
    for _ in 0..20 {
        large.push(Message::user("more content here".repeat(100)));
    }
    assert!(compressor.should_compress(&large));
}

#[test]
fn test_hard_compress_preserves_recent() {
    let compressor = ContextCompressor::new(100000);
    let messages = vec![
        Message::system("system"),
        Message::user("old1"),
        Message::user("old2"),
        Message::user("recent1"),
        Message::user("recent2"),
    ];
    
    let compressed = compressor.hard_compress(&messages);
    assert_eq!(compressed.len(), 4); // system + 2 recent + note
    assert_eq!(compressed[0].role, "system");
}
