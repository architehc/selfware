use super::*;

#[test]
fn test_message_system() {
    let msg = Message::system("You are a helpful assistant");
    assert_eq!(msg.role, "system");
    assert_eq!(msg.content, "You are a helpful assistant");
    assert!(msg.reasoning_content.is_none());
    assert!(msg.tool_calls.is_none());
}

#[test]
fn test_message_user() {
    let msg = Message::user("Hello!");
    assert_eq!(msg.role, "user");
    assert_eq!(msg.content, "Hello!");
}

#[test]
fn test_message_assistant() {
    let msg = Message::assistant("Hi there!");
    assert_eq!(msg.role, "assistant");
    assert_eq!(msg.content, "Hi there!");
}

#[test]
fn test_message_assistant_with_reasoning() {
    let msg = Message::assistant_with_reasoning("Answer", "I thought about it");
    assert_eq!(msg.role, "assistant");
    assert_eq!(msg.content, "Answer");
    assert_eq!(
        msg.reasoning_content,
        Some("I thought about it".to_string())
    );
}

#[test]
fn test_message_tool() {
    let msg = Message::tool(r#"{"result": "success"}"#, "call_123");
    assert_eq!(msg.role, "tool");
    assert_eq!(msg.tool_call_id, Some("call_123".to_string()));
}

#[test]
fn test_message_serialization() {
    let msg = Message::user("Test message");
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"role\":\"user\""));
    assert!(json.contains("\"content\":\"Test message\""));
    // Optional fields should not appear when None
    assert!(!json.contains("reasoning_content"));
}

#[test]
fn test_message_deserialization() {
    let json = r#"{"role": "assistant", "content": "Hello"}"#;
    let msg: Message = serde_json::from_str(json).unwrap();
    assert_eq!(msg.role, "assistant");
    assert_eq!(msg.content, "Hello");
}

#[test]
fn test_tool_call_serialization() {
    let call = ToolCall {
        id: "call_1".to_string(),
        call_type: "function".to_string(),
        function: ToolFunction {
            name: "file_read".to_string(),
            arguments: r#"{"path": "test.txt"}"#.to_string(),
        },
    };
    let json = serde_json::to_string(&call).unwrap();
    assert!(json.contains("\"type\":\"function\""));
    assert!(json.contains("\"name\":\"file_read\""));
}

#[test]
fn test_chat_response_deserialization() {
    let json = r#"{
            "id": "resp_123",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "test-model",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hello!"},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }"#;
    let response: ChatResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.id, "resp_123");
    assert_eq!(response.choices.len(), 1);
    assert_eq!(response.usage.total_tokens, 15);
}

#[test]
fn test_chat_response_missing_usage_defaults_to_zero() {
    // Some OpenAI-compatible providers omit the top-level `usage` object.
    // Deserialization must still succeed with all usage fields defaulting to 0.
    let json = r#"{"id":"x","object":"chat.completion","created":0,"model":"m","choices":[{"index":0,"message":{"role":"assistant","content":"hi"},"finish_reason":"stop"}]}"#;
    let response: ChatResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.id, "x");
    assert_eq!(response.choices.len(), 1);
    assert_eq!(response.choices[0].message.content, "hi");
    assert_eq!(response.usage.prompt_tokens, 0);
    assert_eq!(response.usage.completion_tokens, 0);
    assert_eq!(response.usage.total_tokens, 0);
}

#[test]
fn test_chat_response_with_usage_round_trips() {
    // A response that includes a usage object must still deserialize correctly.
    let json = r#"{"id":"y","object":"chat.completion","created":1,"model":"m","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":7,"completion_tokens":3,"total_tokens":10}}"#;
    let response: ChatResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.usage.prompt_tokens, 7);
    assert_eq!(response.usage.completion_tokens, 3);
    assert_eq!(response.usage.total_tokens, 10);
}

#[test]
fn test_usage_struct() {
    let usage = Usage {
        prompt_tokens: 100,
        completion_tokens: 50,
        total_tokens: 150,
        cost: None,
    };
    assert_eq!(
        usage.prompt_tokens + usage.completion_tokens,
        usage.total_tokens
    );
}

#[test]
fn test_tool_definition_serialization() {
    let def = ToolDefinition {
        def_type: "function".to_string(),
        function: FunctionDefinition {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        },
    };
    let json = serde_json::to_string(&def).unwrap();
    assert!(json.contains("\"type\":\"function\""));
    assert!(json.contains("\"name\":\"test_tool\""));
}

#[test]
fn test_message_delta_default() {
    let delta = MessageDelta::default();
    assert!(delta.role.is_none());
    assert!(delta.content.is_none());
    assert!(delta.reasoning_content.is_none());
    assert!(delta.tool_calls.is_none());
}

#[test]
fn test_choice_struct() {
    let choice = Choice {
        index: 0,
        message: Message::assistant("Hello"),
        reasoning_content: Some("I thought about it".to_string()),
        finish_reason: Some("stop".to_string()),
    };
    assert_eq!(choice.index, 0);
    assert_eq!(choice.message.content, "Hello");
    assert_eq!(
        choice.reasoning_content,
        Some("I thought about it".to_string())
    );
    assert_eq!(choice.finish_reason, Some("stop".to_string()));
}

#[test]
fn test_tool_function_struct() {
    let func = ToolFunction {
        name: "file_read".to_string(),
        arguments: r#"{"path": "/test"}"#.to_string(),
    };
    assert_eq!(func.name, "file_read");
    assert!(func.arguments.contains("path"));
}

#[test]
fn test_function_definition_struct() {
    let def = FunctionDefinition {
        name: "my_tool".to_string(),
        description: "Does something".to_string(),
        parameters: serde_json::json!({"type": "object", "properties": {}}),
    };
    assert_eq!(def.name, "my_tool");
    assert_eq!(def.description, "Does something");
}

#[test]
fn test_chat_response_chunk_deserialization() {
    let json = r#"{
            "id": "chunk_123",
            "object": "chat.completion.chunk",
            "created": 1234567890,
            "model": "test-model",
            "choices": [{
                "index": 0,
                "delta": {},
                "finish_reason": null
            }]
        }"#;
    let chunk: ChatResponseChunk = serde_json::from_str(json).unwrap();
    assert_eq!(chunk.id, "chunk_123");
    assert_eq!(chunk.choices.len(), 1);
}

#[test]
fn test_tool_call_delta_deserialization() {
    let json = r#"{
            "index": 0,
            "id": "call_123",
            "type": "function",
            "function": {"name": "test", "arguments": "{}"}
        }"#;
    let delta: ToolCallDelta = serde_json::from_str(json).unwrap();
    assert_eq!(delta.index, 0);
    assert_eq!(delta.id, Some("call_123".to_string()));
}

#[test]
fn test_function_delta_struct() {
    let delta = FunctionDelta {
        name: Some("test_func".to_string()),
        arguments: Some("{\"a\": 1}".to_string()),
    };
    assert_eq!(delta.name, Some("test_func".to_string()));
    assert!(delta.arguments.is_some());
}

#[test]
fn test_message_with_tool_calls() {
    let json = r#"{
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "file_read",
                    "arguments": "{\"path\": \"test.txt\"}"
                }
            }]
        }"#;
    let msg: Message = serde_json::from_str(json).unwrap();
    assert!(msg.tool_calls.is_some());
    let calls = msg.tool_calls.unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].function.name, "file_read");
}

#[test]
fn test_message_clone() {
    let msg1 = Message::user("Test");
    let msg2 = msg1.clone();
    assert_eq!(msg1.content, msg2.content);
    assert_eq!(msg1.role, msg2.role);
}

#[test]
fn test_message_debug() {
    let msg = Message::user("Debug test");
    let debug_str = format!("{:?}", msg);
    assert!(debug_str.contains("user"));
    assert!(debug_str.contains("Debug test"));
}

#[test]
fn test_message_content_text_serde_roundtrip() {
    // Text content serializes as a plain JSON string
    let msg = Message::user("Hello world");
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"content\":\"Hello world\""));
    let parsed: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.content.text(), "Hello world");
    assert!(!parsed.content.has_images());
}

#[test]
fn test_message_content_blocks_serde_roundtrip() {
    // Blocks content serializes as a JSON array
    let content = MessageContent::from_text("Describe this image").with_image("iVBORw0KGgo=");
    let msg = Message::user_multimodal(content);
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"text\""));
    assert!(json.contains("\"type\":\"image_url\""));
    let parsed: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.content.text(), "Describe this image");
    assert!(parsed.content.has_images());
}

#[test]
fn test_message_content_backward_compat_deserialization() {
    // Plain string JSON deserializes as Text variant
    let json = r#"{"role": "user", "content": "Hello"}"#;
    let msg: Message = serde_json::from_str(json).unwrap();
    assert_eq!(msg.content.text(), "Hello");
    assert!(!msg.content.has_images());
}

#[test]
fn test_message_content_blocks_deserialization() {
    // Array JSON deserializes as Blocks variant
    let json = r#"{"role": "user", "content": [
            {"type": "text", "text": "What is this?"},
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,abc="}}
        ]}"#;
    let msg: Message = serde_json::from_str(json).unwrap();
    assert_eq!(msg.content.text(), "What is this?");
    assert!(msg.content.has_images());
}

#[test]
fn test_text_all_plain_text() {
    let mc = MessageContent::from_text("hello world");
    assert_eq!(mc.text_all(), "hello world");
}

#[test]
fn test_text_all_blocks_with_images() {
    let mc = MessageContent::from_text("first")
        .with_image("img1")
        .with_image("img2");
    // Add a second text block manually
    let mut blocks = match mc {
        MessageContent::Blocks(b) => b,
        _ => panic!("expected Blocks"),
    };
    blocks.push(ContentBlock::Text {
        text: "second".to_string(),
    });
    let mc = MessageContent::Blocks(blocks);
    assert_eq!(mc.text_all(), "first\nsecond");
}

#[test]
fn test_image_count() {
    let mc = MessageContent::from_text("hello");
    assert_eq!(mc.image_count(), 0);

    let mc = mc.with_image("img1").with_image("img2");
    assert_eq!(mc.image_count(), 2);
}

#[test]
fn test_strip_images_plain_text() {
    let mc = MessageContent::from_text("hello");
    let stripped = mc.strip_images();
    assert_eq!(stripped.text(), "hello");
    assert!(!stripped.has_images());
}

#[test]
fn test_strip_images_blocks() {
    let mc = MessageContent::from_text("describe this").with_image("abc123");
    assert!(mc.has_images());
    let stripped = mc.strip_images();
    assert!(!stripped.has_images());
    assert_eq!(stripped.text(), "describe this");
    // Should collapse to Text variant when only one text block remains
    assert!(matches!(stripped, MessageContent::Text(_)));
}

#[test]
fn test_message_strip_images() {
    let content = MessageContent::from_text("look at this").with_image("img_data");
    let msg = Message::user_multimodal(content);
    assert!(msg.content.has_images());
    let stripped = msg.strip_images();
    assert!(!stripped.content.has_images());
    assert_eq!(stripped.role, "user");
    assert_eq!(stripped.content.text(), "look at this");
}

#[test]
fn test_message_content_helpers() {
    let mc = MessageContent::from_text("hello");
    assert_eq!(mc.len(), 5);
    assert!(!mc.is_empty());
    assert!(mc.contains("ell"));
    assert!(!mc.contains("xyz"));
    assert_eq!(mc.chars().count(), 5);
    assert_eq!(format!("{}", mc), "hello");
}

#[test]
fn test_usage_deserialization_with_missing_fields() {
    // Providers may omit `total_tokens` (or any counter) on an otherwise
    // successful billed response; missing fields must default to 0 instead
    // of failing the whole parse.
    let usage: Usage = serde_json::from_str(r#"{"prompt_tokens": 5}"#).unwrap();
    assert_eq!(usage.prompt_tokens, 5);
    assert_eq!(usage.completion_tokens, 0);
    assert_eq!(usage.total_tokens, 0);
    assert!(usage.cost.is_none());
}
