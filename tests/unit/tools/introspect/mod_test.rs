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
