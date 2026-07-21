use super::*;
use tempfile::TempDir;

fn test_store() -> (ChatStore, TempDir) {
    let dir = TempDir::new().unwrap();
    let store = ChatStore {
        chats_dir: dir.path().to_path_buf(),
    };
    (store, dir)
}

#[test]
fn test_save_and_load() {
    let (store, _dir) = test_store();
    let messages = vec![
        Message::system("system prompt".to_string()),
        Message::user("hello".to_string()),
    ];
    store.save("test-chat", &messages, "test-model").unwrap();

    let loaded = store.load("test-chat").unwrap();
    assert_eq!(loaded.name, "test-chat");
    assert_eq!(loaded.model, "test-model");
    assert_eq!(loaded.messages.len(), 2);
}

#[test]
fn test_list_chats() {
    let (store, _dir) = test_store();
    let messages = vec![Message::user("hello".to_string())];
    store.save("chat-a", &messages, "model-1").unwrap();
    store.save("chat-b", &messages, "model-2").unwrap();

    let list = store.list().unwrap();
    assert_eq!(list.len(), 2);
}

#[test]
fn test_delete_chat() {
    let (store, _dir) = test_store();
    let messages = vec![Message::user("hello".to_string())];
    store.save("to-delete", &messages, "model").unwrap();
    assert!(store.delete("to-delete").is_ok());
    assert!(store.load("to-delete").is_err());
}

#[test]
fn test_delete_nonexistent() {
    let (store, _dir) = test_store();
    assert!(store.delete("nonexistent").is_err());
}

#[test]
fn test_load_nonexistent() {
    let (store, _dir) = test_store();
    assert!(store.load("nonexistent").is_err());
}

#[test]
fn test_chat_path_sanitization() {
    let (store, _dir) = test_store();
    let path = store.chat_path("my chat/with spaces");
    assert!(!path.to_string_lossy().contains(' '));
}
