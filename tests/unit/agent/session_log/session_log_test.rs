use super::*;
use tempfile::tempdir;

#[tokio::test]
async fn session_logger_writes_and_reads_recent_events() {
    let dir = tempdir().unwrap();
    let logger = SessionLogger::new_in("session-test", dir.path().to_path_buf())
        .await
        .unwrap();
    let path = logger.path().to_path_buf();

    logger.log(SessionLogEvent {
        timestamp: Utc::now(),
        session_id: String::new(),
        event_type: SessionEventType::SessionStart,
        task_id: None,
        tool_name: None,
        input: None,
        arguments: None,
        result: None,
        success: Some(true),
        duration_ms: None,
        details: Some(json!({"model": "test-model"})),
    });
    logger.log(SessionLogEvent {
        timestamp: Utc::now(),
        session_id: String::new(),
        event_type: SessionEventType::ToolCall,
        task_id: Some("task-1".to_string()),
        tool_name: Some("shell_exec".to_string()),
        input: None,
        arguments: Some("{\"command\":\"echo hi\"}".to_string()),
        result: Some("{\"exit_code\":0}".to_string()),
        success: Some(true),
        duration_ms: Some(12),
        details: None,
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let events = SessionLogger::read_recent(&path, 10).await.unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type, SessionEventType::SessionStart);
    assert_eq!(events[1].event_type, SessionEventType::ToolCall);
    assert_eq!(events[1].tool_name.as_deref(), Some("shell_exec"));
    assert_eq!(events[1].session_id, "session-test");
}

#[tokio::test]
async fn session_logger_redacts_secrets_in_tool_call_fields() {
    let dir = tempdir().unwrap();
    let logger = SessionLogger::new_in("session-redact-test", dir.path().to_path_buf())
        .await
        .unwrap();
    let path = logger.path().to_path_buf();

    logger.log(SessionLogEvent {
            timestamp: Utc::now(),
            session_id: String::new(),
            event_type: SessionEventType::ToolCall,
            task_id: Some("task-1".to_string()),
            tool_name: Some("shell_exec".to_string()),
            input: Some("Authorization: Bearer sk-live-abcdefghijklmnop".to_string()),
            arguments: Some(
                r#"{"command":"curl -H 'Authorization: Bearer sk-live-abcdefghijklmnop' https://api.example.com"}"#
                    .to_string(),
            ),
            result: Some(r#"{"error":"password=hunter2 invalid"}"#.to_string()),
            success: Some(false),
            duration_ms: Some(12),
            details: Some(json!({"raw_response": "token-abcdefghijklmnop leaked here"})),
        });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Check both the in-memory ring buffer (recent_events) and what
    // actually landed on disk -- both flow through the same `log()`.
    for events in [
        logger.recent_events(10),
        SessionLogger::read_recent(&path, 10).await.unwrap(),
    ] {
        assert_eq!(events.len(), 1);
        let event = &events[0];
        let input = event.input.as_deref().unwrap();
        let arguments = event.arguments.as_deref().unwrap();
        let result = event.result.as_deref().unwrap();
        let details = event.details.as_ref().unwrap().to_string();

        assert!(!input.contains("sk-live-abcdefghijklmnop"), "{input}");
        assert!(
            !arguments.contains("sk-live-abcdefghijklmnop"),
            "{arguments}"
        );
        assert!(!result.contains("hunter2"), "{result}");
        assert!(!details.contains("token-abcdefghijklmnop"), "{details}");

        assert!(input.contains("[REDACTED]"));
        assert!(arguments.contains("[REDACTED]"));
        assert!(result.contains("[REDACTED]"));
        assert!(details.contains("[REDACTED]"));
    }
}

#[test]
fn session_event_serializes_as_jsonl() {
    let event = SessionLogEvent {
        timestamp: Utc::now(),
        session_id: "session-1".to_string(),
        event_type: SessionEventType::ToolValidationFailed,
        task_id: Some("task-1".to_string()),
        tool_name: Some("shell_exec".to_string()),
        input: None,
        arguments: Some("{broken".to_string()),
        result: Some("Invalid JSON".to_string()),
        success: Some(false),
        duration_ms: None,
        details: Some(json!({"call_id": "call-1"})),
    };

    let encoded = serde_json::to_string(&event).unwrap();
    assert!(encoded.contains("\"tool_validation_failed\""));
    assert!(encoded.contains("\"shell_exec\""));
    assert!(encoded.contains("\"call-1\""));
}
