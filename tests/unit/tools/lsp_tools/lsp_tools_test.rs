use super::*;

#[test]
fn test_goto_definition_tool_metadata() {
    let (goto, _refs, _syms, _hover) = create_lsp_tools(PathBuf::from("/tmp/test"), None);
    assert_eq!(goto.name(), "lsp_goto_definition");
    assert!(!goto.description().is_empty());

    let schema = goto.schema();
    let required = schema.get("required").unwrap().as_array().unwrap();
    assert!(required.contains(&json!("file")));
    assert!(required.contains(&json!("line")));
    assert!(required.contains(&json!("column")));
}

#[test]
fn test_find_references_tool_metadata() {
    let (_goto, refs, _syms, _hover) = create_lsp_tools(PathBuf::from("/tmp/test"), None);
    assert_eq!(refs.name(), "lsp_find_references");
    assert!(!refs.description().is_empty());
}

#[test]
fn test_document_symbols_tool_metadata() {
    let (_goto, _refs, syms, _hover) = create_lsp_tools(PathBuf::from("/tmp/test"), None);
    assert_eq!(syms.name(), "lsp_document_symbols");

    let schema = syms.schema();
    let required = schema.get("required").unwrap().as_array().unwrap();
    assert!(required.contains(&json!("file")));
}

#[test]
fn test_hover_tool_metadata() {
    let (_goto, _refs, _syms, hover) = create_lsp_tools(PathBuf::from("/tmp/test"), None);
    assert_eq!(hover.name(), "lsp_hover");
    assert!(!hover.description().is_empty());
}

#[test]
fn test_all_tools_share_handle() {
    let (goto, refs, syms, hover) = create_lsp_tools(PathBuf::from("/tmp/test"), None);
    // They all share the same Arc handle.
    assert!(Arc::ptr_eq(&goto.handle, &refs.handle));
    assert!(Arc::ptr_eq(&refs.handle, &syms.handle));
    assert!(Arc::ptr_eq(&syms.handle, &hover.handle));
}

#[test]
fn test_diagnostics_tool_metadata() {
    let (diag, _ws, _impl) = create_extra_lsp_tools(PathBuf::from("/tmp/test"), None);
    assert_eq!(diag.name(), "lsp_diagnostics");
    assert!(!diag.description().is_empty());

    let schema = diag.schema();
    let required = schema.get("required").unwrap().as_array().unwrap();
    assert!(required.contains(&json!("file")));
}

#[test]
fn test_workspace_symbols_tool_metadata() {
    let (_diag, ws, _impl) = create_extra_lsp_tools(PathBuf::from("/tmp/test"), None);
    assert_eq!(ws.name(), "lsp_workspace_symbols");
    assert!(!ws.description().is_empty());

    let schema = ws.schema();
    let required = schema.get("required").unwrap().as_array().unwrap();
    assert!(required.contains(&json!("query")));
}

#[test]
fn test_goto_implementation_tool_metadata() {
    let (_diag, _ws, imp) = create_extra_lsp_tools(PathBuf::from("/tmp/test"), None);
    assert_eq!(imp.name(), "lsp_goto_implementation");
    assert!(!imp.description().is_empty());

    let schema = imp.schema();
    let required = schema.get("required").unwrap().as_array().unwrap();
    assert!(required.contains(&json!("file")));
    assert!(required.contains(&json!("line")));
    assert!(required.contains(&json!("column")));
}

fn default_test_safety_config() -> SafetyConfig {
    SafetyConfig::default()
}

#[test]
fn test_validate_lsp_file_rejects_etc_passwd() {
    let config = default_test_safety_config();
    let result = validate_lsp_file("/etc/passwd", Some(&config));
    assert!(
        result.is_err(),
        "/etc/passwd should be rejected by path validation"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("outside")
            || err.contains("traversal")
            || err.contains("system")
            || err.contains("allowed"),
        "Expected security error, got: {}",
        err
    );
}

#[test]
fn test_validate_lsp_file_allows_workspace_file() {
    let cwd = std::env::current_dir().unwrap();
    let config = default_test_safety_config();
    let file = cwd.join("src/tools/lsp_tools.rs");
    let result = validate_lsp_file(file.to_str().unwrap(), Some(&config));
    assert!(
        result.is_ok(),
        "Workspace file should be allowed, got error: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_goto_definition_rejects_etc_passwd() {
    let (goto, _, _, _) = create_lsp_tools(
        PathBuf::from("/tmp/test"),
        Some(default_test_safety_config()),
    );
    let result = goto
        .execute(json!({"file": "/etc/passwd", "line": 0, "column": 0}))
        .await;
    assert!(result.is_err(), "goto_definition should reject /etc/passwd");
}

#[tokio::test]
async fn test_find_references_rejects_etc_passwd() {
    let (_, refs, _, _) = create_lsp_tools(
        PathBuf::from("/tmp/test"),
        Some(default_test_safety_config()),
    );
    let result = refs
        .execute(json!({"file": "/etc/passwd", "line": 0, "column": 0}))
        .await;
    assert!(result.is_err(), "find_references should reject /etc/passwd");
}

#[tokio::test]
async fn test_document_symbols_rejects_etc_passwd() {
    let (_, _, syms, _) = create_lsp_tools(
        PathBuf::from("/tmp/test"),
        Some(default_test_safety_config()),
    );
    let result = syms.execute(json!({"file": "/etc/passwd"})).await;
    assert!(
        result.is_err(),
        "document_symbols should reject /etc/passwd"
    );
}

#[tokio::test]
async fn test_hover_rejects_etc_passwd() {
    let (_, _, _, hover) = create_lsp_tools(
        PathBuf::from("/tmp/test"),
        Some(default_test_safety_config()),
    );
    let result = hover
        .execute(json!({"file": "/etc/passwd", "line": 0, "column": 0}))
        .await;
    assert!(result.is_err(), "hover should reject /etc/passwd");
}

#[tokio::test]
async fn test_diagnostics_rejects_etc_passwd() {
    let (diag, _, _) = create_extra_lsp_tools(
        PathBuf::from("/tmp/test"),
        Some(default_test_safety_config()),
    );
    let result = diag.execute(json!({"file": "/etc/passwd"})).await;
    assert!(result.is_err(), "diagnostics should reject /etc/passwd");
}

#[tokio::test]
async fn test_goto_implementation_rejects_etc_passwd() {
    let (_, _, imp) = create_extra_lsp_tools(
        PathBuf::from("/tmp/test"),
        Some(default_test_safety_config()),
    );
    let result = imp
        .execute(json!({"file": "/etc/passwd", "line": 0, "column": 0}))
        .await;
    assert!(
        result.is_err(),
        "goto_implementation should reject /etc/passwd"
    );
}

// -- diagnostics_response honesty ----------------------------------------

#[test]
fn test_diagnostics_response_empty_is_unavailable_not_ok() {
    // An empty store after the didOpen wait window must NOT be reported
    // as `status: ok, 0 errors` — that falsely confirms a possibly
    // error-filled file as clean.
    let result = diagnostics_response("src/main.rs", &[], std::time::Duration::from_secs(5));
    assert_eq!(result["status"], "unavailable");
    let message = result["message"].as_str().unwrap();
    assert!(
        message.contains("do NOT treat this as confirmation"),
        "message must warn against reading it as a clean bill: {}",
        message
    );
}

#[test]
fn test_diagnostics_response_counts_severities() {
    let diags = vec![
        crate::lsp::client::Diagnostic {
            message: "mismatched types".into(),
            severity: "error".into(),
            line: 3,
            column: 5,
        },
        crate::lsp::client::Diagnostic {
            message: "unused variable".into(),
            severity: "warning".into(),
            line: 7,
            column: 9,
        },
        crate::lsp::client::Diagnostic {
            message: "missing docs".into(),
            severity: "info".into(),
            line: 1,
            column: 1,
        },
    ];
    let result = diagnostics_response("src/main.rs", &diags, std::time::Duration::from_secs(5));
    assert_eq!(result["status"], "ok");
    assert_eq!(result["count"], 3);
    assert_eq!(result["errors"], 1);
    assert_eq!(result["warnings"], 1);
    assert_eq!(result["diagnostics"].as_array().unwrap().len(), 3);
}
