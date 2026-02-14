//! Unit tests for the API module
//!
//! Tests cover:
//! - Message creation and serialization
//! - ToolCall and ToolFunction types
//! - ChatResponse parsing
//! - Usage tracking

use selfware::api::types::{
    ChatResponse, Choice, FunctionDefinition, Message, ToolCall, ToolDefinition, ToolFunction,
    Usage,
};

// ============================================================================
// Message Tests
// ============================================================================

mod message_tests {
    use super::*;

    #[test]
    fn test_system_message() {
        let msg = Message::system("You are a helpful assistant.");
        assert_eq!(msg.role, "system");
        assert_eq!(msg.content, "You are a helpful assistant.");
        assert!(msg.reasoning_content.is_none());
        assert!(msg.tool_calls.is_none());
        assert!(msg.tool_call_id.is_none());
        assert!(msg.name.is_none());
    }

    #[test]
    fn test_user_message() {
        let msg = Message::user("Hello, world!");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "Hello, world!");
    }

    #[test]
    fn test_assistant_message() {
        let msg = Message::assistant("I can help you with that.");
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "I can help you with that.");
    }

    #[test]
    fn test_assistant_with_reasoning() {
        let msg = Message::assistant_with_reasoning(
            "The answer is 42.",
            "I calculated this by analyzing the question.",
        );
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "The answer is 42.");
        assert_eq!(
            msg.reasoning_content,
            Some("I calculated this by analyzing the question.".to_string())
        );
    }

    #[test]
    fn test_tool_message() {
        let msg = Message::tool("Tool result here", "call_123");
        assert_eq!(msg.role, "tool");
        assert_eq!(msg.content, "Tool result here");
        assert_eq!(msg.tool_call_id, Some("call_123".to_string()));
    }

    #[test]
    fn test_message_with_string_ownership() {
        let content = String::from("Dynamic content");
        let msg = Message::user(content);
        assert_eq!(msg.content, "Dynamic content");
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message::user("Test message");
        let json = serde_json::to_string(&msg).unwrap();

        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"Test message\""));
        // Optional fields should be skipped
        assert!(!json.contains("reasoning_content"));
        assert!(!json.contains("tool_calls"));
    }

    #[test]
    fn test_message_deserialization() {
        let json = r#"{"role":"assistant","content":"Hello"}"#;
        let msg: Message = serde_json::from_str(json).unwrap();

        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "Hello");
    }

    #[test]
    fn test_message_with_tool_calls_deserialization() {
        let json = r#"{
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "arguments": "{\"city\":\"London\"}"
                }
            }]
        }"#;

        let msg: Message = serde_json::from_str(json).unwrap();
        assert_eq!(msg.role, "assistant");
        assert!(msg.tool_calls.is_some());

        let tool_calls = msg.tool_calls.unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_1");
        assert_eq!(tool_calls[0].function.name, "get_weather");
    }

    #[test]
    fn test_message_clone() {
        let original = Message::assistant_with_reasoning("content", "reasoning");
        let cloned = original.clone();

        assert_eq!(original.role, cloned.role);
        assert_eq!(original.content, cloned.content);
        assert_eq!(original.reasoning_content, cloned.reasoning_content);
    }

    #[test]
    fn test_message_debug() {
        let msg = Message::user("test");
        let debug = format!("{:?}", msg);
        assert!(debug.contains("user"));
        assert!(debug.contains("test"));
    }
}

// ============================================================================
// ToolCall Tests
// ============================================================================

mod tool_call_tests {
    use super::*;

    #[test]
    fn test_tool_call_serialization() {
        let tool_call = ToolCall {
            id: "call_abc123".to_string(),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: "read_file".to_string(),
                arguments: r#"{"path":"./src/main.rs"}"#.to_string(),
            },
        };

        let json = serde_json::to_string(&tool_call).unwrap();
        assert!(json.contains("call_abc123"));
        assert!(json.contains("read_file"));
        assert!(json.contains("function"));
    }

    #[test]
    fn test_tool_call_deserialization() {
        let json = r#"{
            "id": "call_xyz",
            "type": "function",
            "function": {
                "name": "shell_exec",
                "arguments": "{\"command\":\"ls -la\"}"
            }
        }"#;

        let tool_call: ToolCall = serde_json::from_str(json).unwrap();
        assert_eq!(tool_call.id, "call_xyz");
        assert_eq!(tool_call.call_type, "function");
        assert_eq!(tool_call.function.name, "shell_exec");
    }

    #[test]
    fn test_tool_function_with_complex_arguments() {
        let tool_function = ToolFunction {
            name: "file_edit".to_string(),
            arguments: r#"{"path":"test.rs","old":"foo","new":"bar"}"#.to_string(),
        };

        let parsed: serde_json::Value = serde_json::from_str(&tool_function.arguments).unwrap();
        assert_eq!(parsed["path"], "test.rs");
        assert_eq!(parsed["old"], "foo");
        assert_eq!(parsed["new"], "bar");
    }

    #[test]
    fn test_tool_call_clone() {
        let original = ToolCall {
            id: "id".to_string(),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: "test".to_string(),
                arguments: "{}".to_string(),
            },
        };

        let cloned = original.clone();
        assert_eq!(original.id, cloned.id);
        assert_eq!(original.function.name, cloned.function.name);
    }
}

// ============================================================================
// ToolDefinition Tests
// ============================================================================

mod tool_definition_tests {
    use super::*;

    #[test]
    fn test_tool_definition_creation() {
        let def = ToolDefinition {
            def_type: "function".to_string(),
            function: FunctionDefinition {
                name: "get_weather".to_string(),
                description: "Get current weather for a city".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "city": {"type": "string"}
                    },
                    "required": ["city"]
                }),
            },
        };

        assert_eq!(def.def_type, "function");
        assert_eq!(def.function.name, "get_weather");
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
        assert!(json.contains("test_tool"));
        assert!(json.contains("A test tool"));
    }
}

// ============================================================================
// ChatResponse Tests
// ============================================================================

mod chat_response_tests {
    use super::*;

    fn create_test_response() -> ChatResponse {
        ChatResponse {
            id: "chatcmpl-123".to_string(),
            object: "chat.completion".to_string(),
            created: 1677858242,
            model: "gpt-4".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message::assistant("Hello!"),
                reasoning_content: None,
                finish_reason: Some("stop".to_string()),
            }],
            usage: Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
        }
    }

    #[test]
    fn test_chat_response_creation() {
        let response = create_test_response();
        assert_eq!(response.id, "chatcmpl-123");
        assert_eq!(response.model, "gpt-4");
        assert_eq!(response.choices.len(), 1);
    }

    #[test]
    fn test_chat_response_usage() {
        let response = create_test_response();
        assert_eq!(response.usage.prompt_tokens, 10);
        assert_eq!(response.usage.completion_tokens, 5);
        assert_eq!(response.usage.total_tokens, 15);
    }

    #[test]
    fn test_chat_response_choice_access() {
        let response = create_test_response();
        let choice = &response.choices[0];

        assert_eq!(choice.index, 0);
        assert_eq!(choice.message.role, "assistant");
        assert_eq!(choice.message.content, "Hello!");
        assert_eq!(choice.finish_reason, Some("stop".to_string()));
    }

    #[test]
    fn test_chat_response_deserialization() {
        let json = r#"{
            "id": "cmpl-test",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "test-model",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Test response"},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 50,
                "total_tokens": 150
            }
        }"#;

        let response: ChatResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "cmpl-test");
        assert_eq!(response.usage.total_tokens, 150);
    }

    #[test]
    fn test_chat_response_multiple_choices() {
        let response = ChatResponse {
            id: "test".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "test".to_string(),
            choices: vec![
                Choice {
                    index: 0,
                    message: Message::assistant("First"),
                    reasoning_content: None,
                    finish_reason: Some("stop".to_string()),
                },
                Choice {
                    index: 1,
                    message: Message::assistant("Second"),
                    reasoning_content: None,
                    finish_reason: Some("stop".to_string()),
                },
            ],
            usage: Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            },
        };

        assert_eq!(response.choices.len(), 2);
        assert_eq!(response.choices[0].message.content, "First");
        assert_eq!(response.choices[1].message.content, "Second");
    }
}

// ============================================================================
// Usage Tests
// ============================================================================

mod usage_tests {
    use super::*;

    #[test]
    fn test_usage_creation() {
        let usage = Usage {
            prompt_tokens: 100,
            completion_tokens: 200,
            total_tokens: 300,
        };

        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 200);
        assert_eq!(usage.total_tokens, 300);
    }

    #[test]
    fn test_usage_serialization() {
        let usage = Usage {
            prompt_tokens: 50,
            completion_tokens: 25,
            total_tokens: 75,
        };

        let json = serde_json::to_string(&usage).unwrap();
        assert!(json.contains("\"prompt_tokens\":50"));
        assert!(json.contains("\"completion_tokens\":25"));
        assert!(json.contains("\"total_tokens\":75"));
    }

    #[test]
    fn test_usage_clone() {
        let original = Usage {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
        };

        let cloned = original.clone();
        assert_eq!(original.prompt_tokens, cloned.prompt_tokens);
        assert_eq!(original.completion_tokens, cloned.completion_tokens);
        assert_eq!(original.total_tokens, cloned.total_tokens);
    }
}
