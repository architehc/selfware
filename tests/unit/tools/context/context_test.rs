use super::*;

#[test]
fn test_context_tool_constants() {
    assert_eq!(CONTEXT_STATUS, "context_status");
    assert_eq!(CONTEXT_FOCUS, "context_focus");
    assert_eq!(CONTEXT_EVICT, "context_evict");
    assert_eq!(CONTEXT_RECOMMEND, "context_recommend");
    assert_eq!(CONTEXT_LOAD_SKELETON, "context_load_skeleton");
    assert_eq!(CONTEXT_BULK_READ, "context_bulk_read");
    assert_eq!(CONTEXT_SUMMARY, "context_summary");
}

#[test]
fn test_context_tool_names() {
    assert_eq!(CONTEXT_TOOL_NAMES.len(), 7);
    assert!(CONTEXT_TOOL_NAMES.contains(&CONTEXT_STATUS));
    assert!(CONTEXT_TOOL_NAMES.contains(&CONTEXT_FOCUS));
    assert!(CONTEXT_TOOL_NAMES.contains(&CONTEXT_EVICT));
    assert!(CONTEXT_TOOL_NAMES.contains(&CONTEXT_RECOMMEND));
    assert!(CONTEXT_TOOL_NAMES.contains(&CONTEXT_LOAD_SKELETON));
    assert!(CONTEXT_TOOL_NAMES.contains(&CONTEXT_BULK_READ));
    assert!(CONTEXT_TOOL_NAMES.contains(&CONTEXT_SUMMARY));
}

#[test]
fn test_is_context_tool() {
    assert!(is_context_tool("context_status"));
    assert!(is_context_tool("context_focus"));
    assert!(is_context_tool("context_evict"));
    assert!(is_context_tool("context_recommend"));
    assert!(is_context_tool("context_load_skeleton"));
    assert!(is_context_tool("context_bulk_read"));
    assert!(is_context_tool("context_summary"));

    assert!(!is_context_tool("file_read"));
    assert!(!is_context_tool("shell_exec"));
    assert!(!is_context_tool(""));
    assert!(!is_context_tool("context_invalid"));
}

#[test]
fn test_context_tool_descriptions() {
    let descriptions = context_tool_descriptions();
    assert_eq!(descriptions.len(), 7);

    // Check each tool has required fields
    for desc in &descriptions {
        assert!(!desc.name.is_empty());
        assert!(!desc.description.is_empty());
        assert!(desc.schema.is_object());
        assert!(desc.schema.get("type").is_some());
    }
}

#[test]
fn test_context_status_schema() {
    let descriptions = context_tool_descriptions();
    let status = descriptions
        .iter()
        .find(|d| d.name == CONTEXT_STATUS)
        .unwrap();

    assert!(status.description.contains("context window"));
    assert_eq!(status.schema["type"], "object");
}

#[test]
fn test_context_focus_schema() {
    let descriptions = context_tool_descriptions();
    let focus = descriptions
        .iter()
        .find(|d| d.name == CONTEXT_FOCUS)
        .unwrap();

    assert!(focus.description.contains("Promote"));
    assert_eq!(focus.schema["type"], "object");
    assert!(focus.schema["properties"]["query"].is_object());
    assert!(focus.schema["required"]
        .as_array()
        .unwrap()
        .contains(&json!("query")));
}

#[test]
fn test_context_evict_schema() {
    let descriptions = context_tool_descriptions();
    let evict = descriptions
        .iter()
        .find(|d| d.name == CONTEXT_EVICT)
        .unwrap();

    assert!(evict.description.contains("Remove"));
    assert!(evict.schema["properties"]["path"].is_object());
    assert!(evict.schema["required"]
        .as_array()
        .unwrap()
        .contains(&json!("path")));
}

#[test]
fn test_context_bulk_read_default_max_files() {
    let descriptions = context_tool_descriptions();
    let bulk = descriptions
        .iter()
        .find(|d| d.name == CONTEXT_BULK_READ)
        .unwrap();

    assert_eq!(bulk.schema["properties"]["max_files"]["default"], 20);
}

#[test]
fn test_context_focus_default_max_files() {
    let descriptions = context_tool_descriptions();
    let focus = descriptions
        .iter()
        .find(|d| d.name == CONTEXT_FOCUS)
        .unwrap();

    assert_eq!(focus.schema["properties"]["max_files"]["default"], 5);
}
