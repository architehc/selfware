use super::*;
use crate::swl::types::schema::{FieldType, StateField};

fn create_test_schema() -> StateSchema {
    StateSchema {
        fields: vec![
            StateField {
                name: "name".to_string(),
                field_type: FieldType::String,
                default: Some(serde_yaml::Value::String("default_name".to_string())),
                description: None,
            },
            StateField {
                name: "count".to_string(),
                field_type: FieldType::Integer,
                default: Some(serde_yaml::Value::Number(0.into())),
                description: None,
            },
        ],
    }
}

#[test]
fn test_state_manager_memory() {
    let mut manager = StateManager::new_memory("test_workflow");

    // Set values
    manager
        .set("key1".to_string(), serde_json::json!("value1"))
        .unwrap();
    manager
        .set("key2".to_string(), serde_json::json!(42))
        .unwrap();

    // Check values
    assert_eq!(manager.get("key1"), Some(&serde_json::json!("value1")));
    assert_eq!(manager.get("key2"), Some(&serde_json::json!(42)));
    assert!(manager.get("missing").is_none());

    // Check dirty flag
    assert!(manager.is_dirty());

    // Delete value
    assert!(manager.delete("key1"));
    assert!(!manager.delete("missing"));

    // Clear
    manager.clear();
    assert!(manager.get_all().is_empty());
}

#[test]
fn test_state_manager_with_schema() {
    let schema = create_test_schema();
    let mut manager = StateManager::new_memory("test_workflow").with_schema(schema);

    // Apply defaults
    manager.apply_defaults();

    // Check defaults were applied
    assert_eq!(
        manager.get("name"),
        Some(&serde_json::json!("default_name"))
    );
    assert_eq!(manager.get("count"), Some(&serde_json::json!(0)));
}

#[test]
fn test_state_manager_export_import() {
    let mut manager = StateManager::new_memory("test_workflow");

    manager
        .set("key1".to_string(), serde_json::json!("value1"))
        .unwrap();
    manager
        .set("key2".to_string(), serde_json::json!(42))
        .unwrap();

    // Export
    let json = manager.export_json().unwrap();
    assert!(json.contains("key1"));
    assert!(json.contains("value1"));

    // Clear and import
    manager.clear();
    manager.import_json(&json).unwrap();

    assert_eq!(manager.get("key1"), Some(&serde_json::json!("value1")));
    assert_eq!(manager.get("key2"), Some(&serde_json::json!(42)));
}

#[tokio::test]
async fn test_state_manager_file_backend() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut manager =
        StateManager::new_file_based("test_workflow", temp_dir.path().to_path_buf()).unwrap();

    manager
        .set("key1".to_string(), serde_json::json!("value1"))
        .unwrap();
    manager.save().await.unwrap();

    // Create new manager and load
    let mut manager2 =
        StateManager::new_file_based("test_workflow", temp_dir.path().to_path_buf()).unwrap();
    manager2.load().await.unwrap();

    assert_eq!(manager2.get("key1"), Some(&serde_json::json!("value1")));
}
