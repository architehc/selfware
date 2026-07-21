use super::*;

#[test]
fn test_audit_event_serialization() {
    let event = AuditEvent {
        timestamp: Utc::now(),
        session_id: "test-123".to_string(),
        event_type: AuditEventType::ToolExecution,
        tool_name: Some("file_write".to_string()),
        args_hash: Some("abc123".to_string()),
        success: true,
        duration_ms: Some(42),
        user_decision: Some("auto-approved".to_string()),
        context: None,
    };

    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"tool_execution\""));
    assert!(json.contains("\"file_write\""));
    assert!(!json.contains("\"context\"")); // skip_serializing_if = None
}

#[test]
fn test_audit_event_types() {
    let types = vec![
        AuditEventType::ToolExecution,
        AuditEventType::SafetyBlock,
        AuditEventType::UserSkip,
        AuditEventType::SessionStart,
        AuditEventType::SessionEnd,
    ];
    for t in types {
        let json = serde_json::to_string(&t).unwrap();
        assert!(!json.is_empty());
    }
}
