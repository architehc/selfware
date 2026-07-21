use super::*;

// ── HotReloadManager::new() ──────────────────────────────────────

#[test]
fn new_creates_empty_tools_map() {
    let mgr = HotReloadManager::new();
    // tool_paths is wrapped in a Mutex -- verify it starts empty
    assert!(mgr.tool_paths.lock().unwrap().is_empty());
}

#[test]
fn default_is_same_as_new() {
    let mgr = HotReloadManager::default();
    assert!(mgr.tool_paths.lock().unwrap().is_empty());
}

// ── get_tool() returns None for unknown names ────────────────────

#[tokio::test]
async fn get_tool_returns_none_for_unknown() {
    let mgr = HotReloadManager::new();
    assert!(mgr.get_tool("nonexistent").await.is_none());
}

#[tokio::test]
async fn get_tool_returns_none_for_empty_string() {
    let mgr = HotReloadManager::new();
    assert!(mgr.get_tool("").await.is_none());
}

// ── reload() returns error for unregistered tools ────────────────

#[tokio::test]
async fn reload_errors_for_unregistered_tool() {
    let mut mgr = HotReloadManager::new();
    let result = mgr.reload("unknown_tool").await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not registered"),
        "Expected 'not registered' in error, got: {}",
        err_msg
    );
}

#[tokio::test]
async fn reload_errors_for_empty_name() {
    let mut mgr = HotReloadManager::new();
    let result = mgr.reload("").await;
    assert!(result.is_err());
}

// ── DynamicTool::load() path restriction ─────────────────────────
//
// We test the error path without needing a real .dylib.
// The function should reject any path that does not reside under
// `$CWD/.selfware/plugins/`.

#[test]
fn load_rejects_path_outside_plugin_dir() {
    // A path that definitely exists but is not under .selfware/plugins/
    let result = DynamicTool::load("/tmp/evil.dylib");
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    // The error should mention the restriction or path resolution failure
    assert!(
        err_msg.contains("restricted") || err_msg.contains("Cannot resolve"),
        "Expected restriction or resolution error, got: {}",
        err_msg
    );
}

#[test]
fn load_rejects_relative_path_outside_plugin_dir() {
    let result = DynamicTool::load("../../etc/passwd");
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("restricted") || err_msg.contains("Cannot resolve"),
        "Expected restriction or resolution error, got: {}",
        err_msg
    );
}

#[test]
fn load_rejects_absolute_system_path() {
    let result = DynamicTool::load("/usr/lib/libSystem.dylib");
    assert!(result.is_err());
}

#[test]
fn load_rejects_nonexistent_path_in_plugin_dir() {
    // Even a path that *would* be in the plugins dir but doesn't exist
    // should fail at canonicalize.
    let result = DynamicTool::load(".selfware/plugins/nonexistent.dylib");
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("Cannot resolve"),
        "Expected resolution error for missing file, got: {}",
        err_msg
    );
}

// ── Debug / Display helpers ──────────────────────────────────────
//
// DynamicTool does not derive Debug; verify we can still extract
// meaningful error info from load failures.

#[test]
fn load_error_is_displayable() {
    let result = DynamicTool::load("/tmp/no_such_plugin.dylib");
    assert!(result.is_err());
    let err = result.unwrap_err();
    // anyhow errors implement Display; make sure it produces a non-empty string
    let msg = format!("{}", err);
    assert!(!msg.is_empty(), "Error Display should produce output");
    // Also check Debug works via anyhow's chain
    let debug_msg = format!("{:?}", err);
    assert!(!debug_msg.is_empty(), "Error Debug should produce output");
}
