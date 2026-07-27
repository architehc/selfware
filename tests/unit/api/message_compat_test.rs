//! Provider/model message-format compatibility matrix — code-based.
//!
//! Different OpenAI-compatible providers reject different message shapes. The
//! most common footgun: an `assistant` message with empty content (produced by
//! a pure "thinking" turn) is rejected by Moonshot/Kimi with a 400. These tests
//! pin the invariants selfware's message-prep (`canonicalize_message_order`,
//! which runs `sanitize_assistant_content`) must uphold, checked against a
//! matrix of representative models.

use super::{canonicalize_message_order, sanitize_assistant_content};
use crate::api::types::{Message, ToolCall, ToolFunction};

/// One model's known message-format constraints. This is the compatibility
/// matrix, expressed as code: every model that requires non-empty assistant
/// content must pass the empty-assistant test below.
struct ModelProfile {
    model: &'static str,
    provider: &'static str,
    /// Rejects `assistant` messages whose content is empty (no text).
    requires_nonempty_assistant: bool,
    /// Accepts native `tool_calls` on the assistant message.
    native_tool_calls: bool,
}

const MATRIX: &[ModelProfile] = &[
    ModelProfile {
        model: "moonshotai/kimi-k3",
        provider: "Moonshot",
        requires_nonempty_assistant: true,
        native_tool_calls: true,
    },
    ModelProfile {
        model: "moonshotai/kimi-k2",
        provider: "Moonshot",
        requires_nonempty_assistant: true,
        native_tool_calls: true,
    },
    ModelProfile {
        model: "z-ai/glm-5.2",
        provider: "Z.ai",
        requires_nonempty_assistant: false,
        native_tool_calls: true,
    },
    ModelProfile {
        model: "openai/gpt-4o",
        provider: "OpenAI",
        requires_nonempty_assistant: false,
        native_tool_calls: true,
    },
    ModelProfile {
        model: "anthropic/claude-3.7",
        provider: "Anthropic",
        requires_nonempty_assistant: false,
        native_tool_calls: true,
    },
    ModelProfile {
        model: "deepseek/deepseek-v4",
        provider: "DeepSeek",
        requires_nonempty_assistant: true,
        native_tool_calls: true,
    },
    ModelProfile {
        model: "qwen/qwen3-235b",
        provider: "vLLM",
        requires_nonempty_assistant: false,
        native_tool_calls: true,
    },
    ModelProfile {
        model: "mistral/mistral-large",
        provider: "Mistral",
        requires_nonempty_assistant: false,
        native_tool_calls: true,
    },
    ModelProfile {
        model: "google/gemini-2.5",
        provider: "Google",
        requires_nonempty_assistant: false,
        native_tool_calls: true,
    },
    ModelProfile {
        model: "meta/llama-4",
        provider: "Meta",
        requires_nonempty_assistant: true,
        native_tool_calls: true,
    },
];

fn tool_call(name: &str, args: &str) -> ToolCall {
    ToolCall {
        id: "call_1".to_string(),
        call_type: "function".to_string(),
        function: ToolFunction {
            name: name.to_string(),
            arguments: args.to_string(),
        },
    }
}

/// A conversation that ends with a pure "thinking" assistant turn: empty
/// content, no tool calls, only reasoning — the exact shape that 400s on Kimi.
fn conversation_with_empty_assistant() -> Vec<Message> {
    let mut think = Message::assistant("");
    think.reasoning_content = Some("Let me consider the options...".to_string());
    vec![
        Message::system("You are a helpful agent."),
        Message::user("Refactor the duplicated function."),
        think,
    ]
}

fn assistant_indices_empty(messages: &[Message]) -> Vec<usize> {
    messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m.role == "assistant")
        .filter(|(_, m)| {
            m.content.text().trim().is_empty() && m.tool_calls.as_ref().is_none_or(|c| c.is_empty())
        })
        .map(|(i, _)| i)
        .collect()
}

#[test]
fn empty_assistant_is_filled_for_every_model_that_requires_it() {
    for profile in MATRIX {
        let mut messages = conversation_with_empty_assistant();
        canonicalize_message_order(&mut messages);
        let empties = assistant_indices_empty(&messages);
        assert!(
            empties.is_empty(),
            "model {} ({}) requires_nonempty_assistant={} but empty assistant remained at {:?}",
            profile.model,
            profile.provider,
            profile.requires_nonempty_assistant,
            empties,
        );
    }
}

#[test]
fn empty_assistant_recovers_reasoning_as_content() {
    let mut messages = conversation_with_empty_assistant();
    sanitize_assistant_content(&mut messages);
    let assistant = messages.iter().find(|m| m.role == "assistant").unwrap();
    assert_eq!(assistant.content.text(), "Let me consider the options...");
}

#[test]
fn empty_assistant_without_reasoning_gets_placeholder() {
    let mut messages = vec![Message::user("hi"), Message::assistant("")];
    sanitize_assistant_content(&mut messages);
    let assistant = messages.iter().find(|m| m.role == "assistant").unwrap();
    assert!(!assistant.content.text().trim().is_empty());
}

#[test]
fn assistant_with_tool_calls_keeps_empty_content() {
    // A tool-call turn legitimately has empty content — must be left untouched
    // so native tool_calls aren't clobbered with placeholder text.
    let mut assistant = Message::assistant("");
    assistant.tool_calls = Some(vec![tool_call("file_read", r#"{"path":"a.rs"}"#)]);
    let mut messages = vec![Message::user("read a.rs"), assistant];
    sanitize_assistant_content(&mut messages);
    let a = messages.iter().find(|m| m.role == "assistant").unwrap();
    assert_eq!(a.content.text(), "");
    assert_eq!(a.tool_calls.as_ref().unwrap().len(), 1);
}

#[test]
fn nonempty_assistant_is_untouched() {
    let mut messages = vec![Message::assistant("Here is the answer.")];
    sanitize_assistant_content(&mut messages);
    assert_eq!(messages[0].content.text(), "Here is the answer.");
}

#[test]
fn tool_call_serializes_in_native_openai_shape() {
    // Tool-call syntax the whole matrix expects: {id, type:"function", function:{name, arguments}}.
    let mut assistant = Message::assistant("");
    assistant.tool_calls = Some(vec![tool_call("shell_exec", r#"{"command":"ls"}"#)]);
    let value = serde_json::to_value(&assistant).unwrap();
    let call = &value["tool_calls"][0];
    assert_eq!(call["id"], "call_1");
    assert_eq!(call["type"], "function");
    assert_eq!(call["function"]["name"], "shell_exec");
    assert_eq!(call["function"]["arguments"], r#"{"command":"ls"}"#);
    for profile in MATRIX.iter().filter(|p| p.native_tool_calls) {
        assert!(
            value.get("tool_calls").is_some(),
            "native tool_calls expected for {}",
            profile.model
        );
    }
}

#[test]
fn full_matrix_serialized_payload_has_no_empty_assistant() {
    // End-to-end: after prep, the serialized JSON that would be POSTed contains
    // no assistant message with an empty content string, for any model.
    for profile in MATRIX {
        let mut messages = conversation_with_empty_assistant();
        canonicalize_message_order(&mut messages);
        let json = serde_json::to_value(&messages).unwrap();
        for m in json.as_array().unwrap() {
            if m["role"] == "assistant" && m.get("tool_calls").is_none() {
                let content = m["content"].as_str().unwrap_or("");
                assert!(
                    !content.trim().is_empty(),
                    "empty assistant content would be POSTed to {}",
                    profile.model
                );
            }
        }
    }
}
