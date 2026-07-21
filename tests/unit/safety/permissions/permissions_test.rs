use super::*;

#[test]
fn test_permanent_grant() {
    let grant = PermissionGrant::permanent("file_write");
    assert!(!grant.is_expired());
    assert!(grant.matches_tool("file_write"));
    assert!(!grant.matches_tool("file_delete"));
}

#[test]
fn test_wildcard_grant() {
    let grant = PermissionGrant::permanent("file_*");
    assert!(grant.matches_tool("file_write"));
    assert!(grant.matches_tool("file_edit"));
    assert!(grant.matches_tool("file_delete"));
    assert!(!grant.matches_tool("shell_exec"));
}

#[test]
fn test_temporary_grant_not_expired() {
    let grant = PermissionGrant::temporary("shell_exec", Duration::hours(1));
    assert!(!grant.is_expired());
    assert!(grant.matches_tool("shell_exec"));
}

#[test]
fn test_session_grant_authorizes_tool_for_session() {
    // Backs the confirmation prompt's "always allow this tool" option:
    // a session grant must authorize that tool (and only that tool) for
    // the rest of the session.
    let grant = PermissionGrant::session("shell_exec");
    assert!(!grant.is_expired());
    assert!(grant.matches_tool("shell_exec"));
    assert!(!grant.matches_tool("file_delete"));

    let mut store = PermissionStore::new();
    store.add(PermissionGrant::session("shell_exec"));
    assert!(store.is_authorized("shell_exec", None));
    assert!(!store.is_authorized("file_write", None));
}

#[test]
fn test_expired_grant() {
    let mut grant = PermissionGrant::permanent("test");
    grant.expires_at = Some(Utc::now() - Duration::hours(1));
    assert!(grant.is_expired());
    assert!(!grant.matches_tool("test"));
}

#[test]
fn test_resource_pattern() {
    let grant = PermissionGrant::permanent("file_write").with_resource("./src/*");
    assert!(grant.matches("file_write", Some("./src/main.rs")));
    assert!(!grant.matches("file_write", Some("./tests/test.rs")));
    assert!(!grant.matches("file_write", None));
}

#[test]
fn test_permission_store() {
    let mut store = PermissionStore::new();
    assert!(!store.is_authorized("file_write", None));

    store.add(PermissionGrant::permanent("file_write"));
    assert!(store.is_authorized("file_write", None));
    assert!(!store.is_authorized("file_delete", None));

    assert_eq!(store.active_count(), 1);
}

#[test]
fn test_pattern_matches() {
    assert!(pattern_matches("*", "anything"));
    assert!(pattern_matches("file_*", "file_write"));
    assert!(pattern_matches("*_exec", "shell_exec"));
    assert!(pattern_matches("exact", "exact"));
    assert!(!pattern_matches("exact", "other"));
}
