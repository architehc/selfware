use super::*;

#[test]
fn test_file_backend_path() {
    let dir = tempfile::tempdir().unwrap();
    let backend = FileBackend::new(dir.path().to_path_buf()).unwrap();
    assert_eq!(
        backend.state_file_path("my_workflow"),
        dir.path().join("my_workflow.json")
    );
}

#[test]
fn test_backend_type_default() {
    let default = StateBackendType::default();
    match default {
        StateBackendType::File { .. } => {}
        _ => panic!("Default backend should be File"),
    }
}

#[test]
fn test_backend_type_from_str() {
    assert!(matches!(
        StateBackendType::parse_str("memory"),
        Some(StateBackendType::Memory)
    ));
    assert!(matches!(
        StateBackendType::parse_str("Memory"),
        Some(StateBackendType::Memory)
    ));
    assert!(matches!(
        StateBackendType::parse_str("file"),
        Some(StateBackendType::File { .. })
    ));
}

#[tokio::test]
async fn test_memory_backend() {
    let backend = MemoryBackend::new();

    // Test save and load
    let mut state = HashMap::new();
    state.insert("key1".to_string(), serde_json::json!("value1"));
    state.insert("key2".to_string(), serde_json::json!(42));

    backend.save("test_workflow", &state).await.unwrap();

    let loaded = backend.load("test_workflow").await.unwrap();
    assert_eq!(loaded.get("key1"), Some(&serde_json::json!("value1")));
    assert_eq!(loaded.get("key2"), Some(&serde_json::json!(42)));

    // Test exists
    assert!(backend.exists("test_workflow").await);
    assert!(!backend.exists("nonexistent").await);

    // Test list
    let list = backend.list().await.unwrap();
    assert!(list.contains(&"test_workflow".to_string()));

    // Test delete
    backend.delete("test_workflow").await.unwrap();
    assert!(!backend.exists("test_workflow").await);
}

#[tokio::test]
async fn test_file_backend() {
    let temp_dir = tempfile::tempdir().unwrap();
    let backend = FileBackend::new(temp_dir.path().to_path_buf()).unwrap();

    // Test save and load
    let mut state = HashMap::new();
    state.insert("key1".to_string(), serde_json::json!("value1"));

    backend.save("test_workflow", &state).await.unwrap();

    let loaded = backend.load("test_workflow").await.unwrap();
    assert_eq!(loaded.get("key1"), Some(&serde_json::json!("value1")));

    // Test exists
    assert!(backend.exists("test_workflow").await);

    // Test list
    let list = backend.list().await.unwrap();
    assert!(list.contains(&"test_workflow".to_string()));

    // Test delete
    backend.delete("test_workflow").await.unwrap();
    assert!(!backend.exists("test_workflow").await);
}

#[tokio::test]
async fn test_file_backend_load_nonexistent() {
    let temp_dir = tempfile::tempdir().unwrap();
    let backend = FileBackend::new(temp_dir.path().to_path_buf()).unwrap();

    let loaded = backend.load("nonexistent").await.unwrap();
    assert!(loaded.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn test_file_backend_rejects_symlinks() {
    use std::os::unix::fs::symlink;

    let temp_dir = tempfile::tempdir().unwrap();
    let backend = FileBackend::new(temp_dir.path().to_path_buf()).unwrap();

    let outside = temp_dir.path().join("outside.txt");
    std::fs::write(&outside, r#"{"secret":"value"}"#).unwrap();
    let link = temp_dir.path().join("symflow.json");
    symlink(&outside, &link).unwrap();

    assert!(!backend.exists("symflow").await);
    assert!(!backend
        .list()
        .await
        .unwrap()
        .contains(&"symflow".to_string()));
    assert!(backend.load("symflow").await.is_err());
    assert!(backend.delete("symflow").await.is_err());
    assert!(outside.exists(), "symlink target must not be deleted");
}
