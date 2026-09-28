use super::*;

/// Conversational messages: greetings, wellbeing, thanks, farewells and
/// questions about the agent itself.
const CHAT: &[&str] = &[
    "hi",
    "Hi",
    "hi!",
    "hello",
    "Hello there",
    "hey",
    "heyyy",
    "hiii",
    "yo",
    "good morning",
    "Good morning!",
    "hi there, how are you?",
    "How's it going?",
    "what's up",
    "thanks",
    "Thanks!",
    "thank you",
    "Thank you so much!",
    "thx",
    "ok thanks",
    "great, thanks!",
    "perfect, thank you",
    "awesome 👍",
    "thanks 🙏",
    "bye",
    "see you later",
    "good night",
    "who are you",
    "Who are you?",
    "what can you do?",
    "What can you do",
    "what can you help me with?",
    "what are you?",
    "what's your name?",
    "tell me about yourself",
    "introduce yourself",
    "are you an AI?",
    "hi! what can you do?",
    "hello, who are you?",
    "thanks, bye!",
    "hey selfware",
    "help",
];

/// Real tasks — some with a greeting or thanks in front — and messages that
/// are not chat (confirmations, other languages, code).
const NOT_CHAT: &[&str] = &[
    "",
    "   ",
    "hi, can you fix the failing test in src/x.rs",
    "hi can you fix the failing test",
    "hey, run the tests",
    "thanks, now add a test for it",
    "thanks! also update the README",
    "hello, what does this project do?",
    "hi, what files are in this repo?",
    "what can you do with this codebase?",
    "what is in src/main.rs?",
    "who wrote this function?",
    "fix the bug",
    "review the code",
    "ok",
    "okay",
    "yes",
    "sure, go ahead",
    "do it",
    "continue",
    "hi `cargo test`",
    "hello world program in rust",
    "hola, ¿qué tal?",
    "explain how a hash map works",
    "what is 2+2",
    "help me fix the build",
    "thanks for fixing it, but the test still fails",
    "hi\nplease refactor lib.rs",
    "how are you going to fix the parser?",
    "who are you going to call for the database?",
    "good morning! please summarize the changes in the last commit",
];

#[test]
fn phrasing_table() {
    for msg in CHAT {
        assert!(is_pure_chat(msg), "should be chat: {msg:?}");
    }
    for msg in NOT_CHAT {
        assert!(!is_pure_chat(msg), "should be a task: {msg:?}");
    }
}

#[test]
fn long_or_many_word_messages_are_never_chat() {
    let long = "hi ".repeat(30);
    assert!(!is_pure_chat(&long), "over the length cap");
    assert!(!is_pure_chat(
        "hi hello hey hi hello hey hi hello hey hi hello hey hi"
    ));
}

#[test]
fn every_listed_clause_is_chat_on_its_own() {
    for clause in CHAT_CLAUSES {
        assert!(is_pure_chat(clause), "{clause:?}");
    }
}

/// Chat never requires mutation, whatever the raw keyword classifier says
/// about the words in it; a task with a greeting in front keeps its own
/// classification.
#[tokio::test]
async fn chat_classification_is_stored_at_task_start() {
    let config = crate::test_support::mock_agent_config("http://127.0.0.1:9/v1");
    let mut agent = crate::agent::Agent::new(config).await.unwrap();
    for msg in CHAT {
        agent.current_task_context = msg.to_string();
        agent.classify_task_policy();
        assert!(agent.task_is_chat, "{msg:?}");
        assert!(!agent.current_task_requires_mutation(), "{msg:?}");
    }
    agent.current_task_context = "hi, can you fix the failing test in src/x.rs".to_string();
    agent.classify_task_policy();
    assert!(!agent.task_is_chat);
    assert!(agent.current_task_requires_mutation());
}

fn request_json(raw: &str) -> serde_json::Value {
    serde_json::from_str(&raw[raw.find('{').expect("a JSON body")..]).expect("request body is JSON")
}

fn chat_config(endpoint: String) -> crate::config::Config {
    let mut config = crate::test_support::mock_agent_config(&endpoint);
    config.agent.streaming = false;
    config
}

/// "hi" is answered in one request that carries no tool schemas, the chat
/// system prompt instead of the tool protocol, and none of the workspace
/// context; nothing is dispatched.
#[tokio::test]
async fn hi_is_answered_in_one_request_without_tools() {
    use crate::testing::mock_api::MockLlmServer;
    let _g = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("Hi! What are we working on?")
        .build()
        .await;
    let mut agent = crate::agent::Agent::new(chat_config(format!("{}/v1", server.url())))
        .await
        .unwrap();
    agent.run_task("hi").await.expect("chat turn completes");

    let bodies = server.captured_request_bodies().await;
    assert_eq!(bodies.len(), 1, "one request: {bodies:?}");
    let body = request_json(&bodies[0]);
    assert!(body.get("tools").is_none(), "no tool schemas: {body}");
    assert!(body.get("tool_choice").is_none(), "{body}");
    let messages = body["messages"].as_array().expect("messages");
    let text: String = messages
        .iter()
        .map(|m| m["content"].as_str().unwrap_or_default().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("No tools are available for this reply"),
        "{text}"
    );
    assert!(!text.contains("<tool>"), "no tool protocol: {text}");
    assert!(
        !text.contains("selfware_context_note"),
        "no workspace context: {text}"
    );
    assert_eq!(messages.last().unwrap()["content"], "hi");

    assert_eq!(agent.total_tool_call_count(), 0);
    assert_eq!(agent.last_assistant_response, "Hi! What are we working on?");
    server.stop().await;
}

/// A reply that calls a tool anyway means the message was a task: it is
/// planned normally (tools offered), and the chat reply never becomes the
/// answer.
#[tokio::test]
async fn a_chat_reply_that_calls_a_tool_falls_back_to_planning() {
    use crate::testing::mock_api::MockLlmServer;
    let _g = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response(
            "<tool>\n<name>directory_tree</name>\n<arguments>{\"path\": \".\"}</arguments>\n</tool>",
        )
        .with_response("Hello! I'm Selfware, a coding agent. What should we work on today?")
        .build()
        .await;
    let mut agent = crate::agent::Agent::new(chat_config(format!("{}/v1", server.url())))
        .await
        .unwrap();
    agent.run_task("hello").await.expect("completes");
    let bodies = server.captured_request_bodies().await;
    assert!(bodies.len() >= 2, "fell back to a normal planning request");
    assert!(
        bodies[1].contains("<tool>") || bodies[1].contains("\"tools\""),
        "the fallback request offers tools"
    );
    assert!(!agent.task_is_chat);
    server.stop().await;
}

/// A task with a greeting in front is planned as a task: its first request
/// offers the tools.
#[tokio::test]
async fn a_greeting_in_front_of_a_task_is_still_a_task() {
    use crate::testing::mock_api::MockLlmServer;
    let _g = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("I cannot see a failing test here; nothing to fix.")
        .with_response("Nothing to fix.")
        .with_response("Nothing to fix.")
        .with_response("Nothing to fix.")
        .with_response("Nothing to fix.")
        .build()
        .await;
    let mut config = chat_config(format!("{}/v1", server.url()));
    config.agent.max_iterations = 2;
    let mut agent = crate::agent::Agent::new(config).await.unwrap();
    let _ = agent
        .run_task("hi, can you fix the failing test in src/x.rs")
        .await;
    assert!(!agent.task_is_chat);
    let bodies = server.captured_request_bodies().await;
    assert!(!bodies.is_empty());
    assert!(
        !bodies[0].contains("No tools are available for this reply"),
        "planned with the normal prompt"
    );
    server.stop().await;
}

/// Planning quota, thinking off: the chat request takes the planning
/// `max_tokens` and switches thinking off where the endpoint takes
/// chat-template kwargs; elsewhere no kwargs are invented.
#[tokio::test]
async fn chat_turn_uses_the_planning_cap_with_thinking_off() {
    use crate::testing::mock_api::MockLlmServer;
    let _g = crate::test_support::ExecGuard::hold();
    for takes_kwargs in [true, false] {
        let server = MockLlmServer::builder()
            .with_response("Hello! What should we build?")
            .build()
            .await;
        let mut config = chat_config(format!("{}/v1", server.url()));
        config.workloads.planning.max_tokens = Some(1234);
        if takes_kwargs {
            config.extra_body = Some(
                serde_json::json!({"chat_template_kwargs": {"enable_thinking": true}})
                    .as_object()
                    .unwrap()
                    .clone(),
            );
        }
        let mut agent = crate::agent::Agent::new(config).await.unwrap();
        agent
            .run_task("thanks!")
            .await
            .expect("chat turn completes");
        let bodies = server.captured_request_bodies().await;
        assert_eq!(bodies.len(), 1);
        let body = request_json(&bodies[0]);
        assert_eq!(body["max_tokens"], 1234, "{body}");
        if takes_kwargs {
            assert_eq!(
                body["chat_template_kwargs"]["enable_thinking"], false,
                "{body}"
            );
        } else {
            assert!(body.get("chat_template_kwargs").is_none(), "{body}");
        }
        server.stop().await;
    }
}
