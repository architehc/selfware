use super::*;
use crate::consolidation::collector::{CollectedItem, SourceType};
use std::collections::HashMap;

fn make_item(id: &str, minutes_ago: i64) -> CollectedItem {
    CollectedItem {
        source_id: id.into(),
        source_type: SourceType::Episode,
        content: format!("Event {id}"),
        timestamp: Utc::now() - chrono::Duration::minutes(minutes_ago),
        importance: 2,
        tags: vec!["test".into()],
        metadata: HashMap::new(),
        related_ids: Vec::new(),
        session_id: Some("session-1".into()),
        file_refs: Vec::new(),
    }
}

#[test]
fn test_group_items_by_time() {
    let config = ConsolidationConfig::default();
    let compactor = MemoryCompactor::new(config).unwrap();

    let items = vec![
        make_item("a", 10), // recent cluster
        make_item("b", 8),
        make_item("c", 120), // separate cluster (2h gap)
        make_item("d", 118),
    ];

    let groups = compactor.group_items(items);
    assert_eq!(groups.len(), 2, "Should create 2 groups with >30min gap");
}

#[test]
fn test_group_items_causal() {
    let config = ConsolidationConfig::default();
    let compactor = MemoryCompactor::new(config).unwrap();

    let mut items = vec![
        make_item("a", 100),
        make_item("b", 5), // far in time from 'a'
    ];
    // But 'b' is causally related to 'a'
    items[1].related_ids.push("a".into());

    let groups = compactor.group_items(items);
    // They should still be grouped because of causal link
    assert_eq!(groups.len(), 1);
}

#[test]
fn test_group_items_empty() {
    let config = ConsolidationConfig::default();
    let compactor = MemoryCompactor::new(config).unwrap();
    let groups = compactor.group_items(Vec::new());
    assert!(groups.is_empty());
}

#[test]
fn test_importance_label() {
    assert_eq!(importance_label(1), "LOW");
    assert_eq!(importance_label(2), "NORMAL");
    assert_eq!(importance_label(3), "HIGH");
    assert_eq!(importance_label(4), "CRITICAL");
}

#[test]
fn test_json_string_array() {
    let val = json!(["a", "b", "c"]);
    assert_eq!(json_string_array(&val), vec!["a", "b", "c"]);

    let empty = json!(null);
    assert!(json_string_array(&empty).is_empty());
}

#[test]
fn test_extract_json() {
    assert_eq!(extract_json(r#"{"a":1}"#), r#"{"a":1}"#);
    assert_eq!(
        extract_json("text\n```json\n{\"a\":1}\n```\nmore"),
        r#"{"a":1}"#
    );
    assert_eq!(extract_json("before {\"x\":2} after"), r#"{"x":2}"#);
}
