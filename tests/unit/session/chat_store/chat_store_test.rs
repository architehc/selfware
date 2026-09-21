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

// ── Naming collision discipline (regression) ────────────────────────

/// Legacy helper: write a chat FILE directly (as the pre-fix sanitizer did),
/// bypassing the store's save gate so we can simulate old on-disk data.
/// Plaintext — matches the test environment where no keychain encryption is
/// configured (same assumption the existing save/load tests rely on).
fn write_legacy_file(name: &str, path: &std::path::Path) {
    let legacy = SavedChat {
        name: name.to_string(),
        saved_at: Utc::now(),
        model: "legacy-model".to_string(),
        messages: vec![Message::user(format!("legacy content of '{}'", name))],
    };
    std::fs::write(path, serde_json::to_string_pretty(&legacy).unwrap()).unwrap();
}

#[test]
fn test_differently_named_sessions_do_not_collide() {
    // Regression: on the old sanitizer, "my_session" and "my-session" were
    // distinct files but "my session" would have collapsed onto
    // "my_session.json". Distinct safe names must stay independent.
    let (store, _dir) = test_store();
    let messages = vec![Message::user("hello".to_string())];
    store.save("my_session", &messages, "m1").unwrap();
    store.save("my-session", &messages, "m2").unwrap();

    let a = store.load("my_session").unwrap();
    let b = store.load("my-session").unwrap();
    assert_eq!(a.name, "my_session");
    assert_eq!(a.model, "m1");
    assert_eq!(b.name, "my-session");
    assert_eq!(b.model, "m2");
    assert_eq!(store.list().unwrap().len(), 2, "both sessions must list");
}

#[test]
fn test_save_rejects_alias_prone_names() {
    // A name that the sanitizer would fold onto another session's file must
    // be rejected at write time, not silently collapsed.
    let (store, _dir) = test_store();
    let err = store
        .save("my session", &[Message::user("hi".to_string())], "m")
        .expect_err("spaces in a chat name must be rejected, not aliased");
    let msg = err.to_string();
    assert!(
        msg.contains("my session") && msg.contains("use only letters, digits"),
        "the rejection must name the name and the allowed charset, got: {}",
        msg
    );
}

#[test]
fn test_legacy_aliased_file_does_not_cross_load() {
    // Pre-fix data: "my session" was written to my_session.json by the old
    // sanitizer. Loading "my session" must work (exact recorded name), but
    // loading "my_session" must be a hard error — never silently return the
    // OTHER session.
    let (store, _dir) = test_store();
    write_legacy_file("my session", &store.chat_path("my session"));

    let loaded = store
        .load("my session")
        .expect("legacy file must load under its recorded name");
    assert_eq!(loaded.name, "my session");

    let err = store
        .load("my_session")
        .expect_err("aliased name must not return a different session");
    let msg = err.to_string();
    assert!(
        msg.contains("my_session") && msg.contains("my session"),
        "the refusal must name both the requested and the recorded session, got: {}",
        msg
    );
    assert!(
        msg.contains("refusing to load a different session"),
        "must fail closed instead of cross-loading, got: {}",
        msg
    );
}

#[test]
fn test_save_refuses_to_overwrite_foreign_session() {
    // Saving under a name whose file holds a DIFFERENT session (legacy
    // collision) must fail, not clobber the other session's data.
    let (store, _dir) = test_store();
    write_legacy_file("my session", &store.chat_path("my session"));

    let err = store
        .save("my_session", &[Message::user("new".to_string())], "m")
        .expect_err("must not overwrite a file that holds another session");
    let msg = err.to_string();
    assert!(
        msg.contains("refusing to overwrite"),
        "the refusal must be explicit, got: {}",
        msg
    );

    // And the other session's data is untouched.
    assert_eq!(store.load("my session").unwrap().name, "my session");
}

// ── Delete identity validation (regression) ─────────────────────────

#[test]
fn test_aliased_delete_does_not_remove_real_session() {
    // A legitimate "my_session" on disk must not be deletable through the
    // colliding spelling "my session" (both resolve to my_session.json).
    let (store, _dir) = test_store();
    let messages = vec![Message::user("hello".to_string())];
    store.save("my_session", &messages, "m").unwrap();

    let err = store
        .delete("my session")
        .expect_err("deleting 'my session' must not remove the 'my_session' session");
    let msg = err.to_string();
    assert!(
        msg.contains("refusing to delete") && msg.contains("my_session"),
        "the refusal must name the intent and the session, got: {}",
        msg
    );

    // Survival: the real session is intact and still loads.
    assert!(
        store.chat_path("my_session").exists(),
        "my_session.json must survive the aliased delete"
    );
    assert_eq!(store.load("my_session").unwrap().name, "my_session");
}

#[test]
fn test_legacy_aliased_delete_cannot_delete_legacy_session() {
    // Legacy direction: my_session.json holds "my session" (pre-fix save).
    // Deleting under the OTHER spelling must fail and leave the file intact.
    let (store, _dir) = test_store();
    write_legacy_file("my session", &store.chat_path("my session"));

    let err = store
        .delete("my_session")
        .expect_err("aliased spelling must not delete the legacy session's file");
    let msg = err.to_string();
    assert!(
        msg.contains("refusing to delete"),
        "the refusal must be explicit, got: {}",
        msg
    );

    assert!(
        store.chat_path("my session").exists(),
        "the legacy file must survive the aliased delete"
    );
    assert_eq!(store.load("my session").unwrap().name, "my session");
}

#[test]
fn test_delete_verifying_file_still_works() {
    // Repair path preserved: a file that verifies as the requested session
    // (fresh save under its own name) is still deletable.
    let (store, _dir) = test_store();
    let messages = vec![Message::user("hi".to_string())];
    store.save("my_session", &messages, "m").unwrap();
    store.delete("my_session").unwrap();
    assert!(!store.chat_path("my_session").exists());
    assert!(store.load("my_session").is_err());
}

#[test]
fn test_delete_legacy_file_under_its_recorded_name_still_works() {
    // Repair path preserved for legacy files too: deleting by the name the
    // file itself records ("my session") still works.
    let (store, _dir) = test_store();
    write_legacy_file("my session", &store.chat_path("my session"));
    store.delete("my session").unwrap();
    assert!(!store.chat_path("my session").exists());
}
