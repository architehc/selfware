use super::*;

// ── Introspection tool path policy (2026-09-21 review sweep) ─────────────
//
// code_query.scope, code_plan.codebase_root, code_diff_plan's
// target_file/codebase_root — and code_introspect's target — walk and read
// the filesystem; each must obey the same workspace path policy as
// file_read BEFORE any filesystem access.

fn is_path_policy_error(msg: &str) -> bool {
    msg.contains("not allowed")
        || msg.contains("not in allowed")
        || msg.contains("outside working")
        || msg.contains("protected")
        || msg.contains("denied pattern")
}

#[tokio::test]
async fn test_code_query_scope_policy() {
    let tool = CodeQuery::new();
    let err = tool
        .execute(serde_json::json!({"query": "config", "scope": "/etc"}))
        .await
        .unwrap_err();
    assert!(
        is_path_policy_error(&err.to_string()),
        "code_query scope outside the workspace must be refused, got: {err}"
    );
}

#[tokio::test]
async fn test_code_plan_codebase_root_policy() {
    let tool = CodePlan::new();
    let err = tool
        .execute(serde_json::json!({"goal": "g", "codebase_root": "/etc"}))
        .await
        .unwrap_err();
    assert!(
        is_path_policy_error(&err.to_string()),
        "code_plan codebase_root outside the workspace must be refused, got: {err}"
    );
}

#[tokio::test]
async fn test_code_diff_plan_paths_policy() {
    let tool = CodeDiffPlan::new();
    for args in [
        serde_json::json!({"target_file": "/etc/passwd", "change_type": "modify"}),
        serde_json::json!({"target_file": "src/main.rs", "change_type": "modify", "codebase_root": "/etc"}),
    ] {
        let err = tool.execute(args).await.unwrap_err();
        assert!(
            is_path_policy_error(&err.to_string()),
            "code_diff_plan out-of-workspace path must be refused, got: {err}"
        );
    }
}

#[tokio::test]
async fn test_code_introspect_target_policy() {
    let tool = CodeIntrospect::new();
    let err = tool
        .execute(serde_json::json!({"target": "/etc"}))
        .await
        .unwrap_err();
    assert!(
        is_path_policy_error(&err.to_string()),
        "code_introspect target outside the workspace must be refused, got: {err}"
    );
}

#[tokio::test]
async fn test_code_query_scope_in_workspace_ok() {
    let tool = CodeQuery::new();
    // An in-workspace scope passes the path policy; any error that surfaces
    // must be a search/build error, not a path-policy refusal.
    let result = tool
        .execute(serde_json::json!({"query": "fn main", "scope": "src"}))
        .await;
    match result {
        Ok(_) => {}
        Err(e) => assert!(
            !is_path_policy_error(&e.to_string()),
            "in-workspace scope must not trip the path policy, got: {e}"
        ),
    }
}

// ── Recursive-walk validation (2026-09-21 review, P2) ────────────────────
//
// The introspection walkers validated only the ROOT target: denied source
// files nested under an allowed directory, and symlinks escaping the
// workspace, were discovered and READ unvalidated. Every candidate the walk
// will read must pass the same workspace path policy as the root.

fn scoped_config(allowed: String, denied: Vec<String>) -> SafetyConfig {
    SafetyConfig {
        allowed_paths: vec![allowed],
        denied_paths: denied,
        ..SafetyConfig::default()
    }
}

/// A denied source file nested under an allowed directory must be refused —
/// the walk discovers it, validation refuses it, and nothing is read.
#[tokio::test]
async fn test_code_introspect_refuses_denied_file_nested_under_allowed_dir() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("allowed");
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(root.join("ok.rs"), "pub fn ok() {}\n").unwrap();
    let dir_str = dir.path().to_string_lossy().to_string();

    // A denied file with a source extension, two levels below the allowed
    // root: under the old walk only `target` was validated, so this would
    // have been read.
    std::fs::write(
        root.join("sub").join("secret_calc.rs"),
        "pub fn secret() {}\n",
    )
    .unwrap();

    let config = scoped_config(
        format!("{dir_str}/**"),
        vec!["**/secret_calc.rs".to_string()],
    );
    let tool = CodeIntrospect::with_safety_config(config);
    let target = root.to_string_lossy().to_string();
    let err = tool
        .execute(serde_json::json!({"target": target, "depth": "full"}))
        .await
        .unwrap_err();
    assert!(
        is_path_policy_error(&err.to_string()),
        "a denied file nested under an allowed directory must be refused, got: {err}"
    );
    assert!(
        err.to_string().contains("denied"),
        "the refusal should name the denied pattern, got: {err}"
    );
}

/// A symlink inside the allowed tree pointing OUTSIDE the workspace must be
/// refused — containment is enforced per candidate, so the target is never
/// read.
#[cfg(unix)]
#[tokio::test]
async fn test_code_introspect_refuses_symlink_escaping_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let allowed = dir.path().join("allowed");
    std::fs::create_dir_all(&allowed).unwrap();
    std::fs::write(allowed.join("ok.rs"), "pub fn ok() {}\n").unwrap();

    // A real file OUTSIDE the allowed root (but inside the temp dir), plus
    // a source-extension symlink inside the allowed tree pointing at it.
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("leak_target.rs"), "pub fn leaked() {}\n").unwrap();
    std::os::unix::fs::symlink(outside.join("leak_target.rs"), allowed.join("evil.rs")).unwrap();

    let dir_str = dir.path().to_string_lossy().to_string();
    let config = scoped_config(format!("{dir_str}/allowed/**"), vec![]);
    let tool = CodeIntrospect::with_safety_config(config);
    let target = allowed.to_string_lossy().to_string();
    let err = tool
        .execute(serde_json::json!({"target": target, "depth": "full"}))
        .await
        .unwrap_err();
    assert!(
        is_path_policy_error(&err.to_string()),
        "a symlink escaping the allowed workspace must be refused, got: {err}"
    );
}

/// Control: an allowed directory holding only benign source files must not
/// trip the path policy — per-candidate validation must not over-block.
#[tokio::test]
async fn test_code_introspect_allowed_tree_with_benign_files_is_not_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("allowed");
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(root.join("ok.rs"), "pub fn ok() {}\n").unwrap();
    std::fs::write(root.join("sub").join("helper.rs"), "pub fn helper() {}\n").unwrap();

    let dir_str = dir.path().to_string_lossy().to_string();
    let config = scoped_config(format!("{dir_str}/**"), vec![]);
    let tool = CodeIntrospect::with_safety_config(config);
    let target = root.to_string_lossy().to_string();
    let result = tool
        .execute(serde_json::json!({"target": target, "depth": "full"}))
        .await;
    match result {
        Ok(_) => {}
        Err(e) => assert!(
            !is_path_policy_error(&e.to_string()),
            "an allowed tree must not trip the path policy, got: {e}"
        ),
    }
}

/// code_query walks the same way: a denied source file nested under an
/// allowed scope must be refused there too.
#[tokio::test]
async fn test_code_query_refuses_denied_file_nested_under_allowed_scope() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("allowed");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("ok.rs"), "pub fn ok() {}\n").unwrap();
    std::fs::write(root.join("secret_query.rs"), "pub fn secret() {}\n").unwrap();

    let dir_str = dir.path().to_string_lossy().to_string();
    let config = scoped_config(
        format!("{dir_str}/**"),
        vec!["**/secret_query.rs".to_string()],
    );
    let tool = CodeQuery::with_safety_config(config);
    let scope = root.to_string_lossy().to_string();
    let err = tool
        .execute(serde_json::json!({"query": "secret", "scope": scope}))
        .await
        .unwrap_err();
    assert!(
        is_path_policy_error(&err.to_string()),
        "code_query must refuse a denied file inside the scope, got: {err}"
    );
}
