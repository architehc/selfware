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
    // Pin the file to the crate root and hold the cwd lock: reading the
    // process cwd unguarded raced with tests that call set_current_dir.
    let _guard = crate::test_support::CwdGuard::hold();
    let config = default_test_safety_config();
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tools/lsp_tools.rs");
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

// -- indexing honesty (Rule 3) -------------------------------------------

fn loc(line: u32) -> crate::lsp::client::Location {
    crate::lsp::client::Location {
        file: "/w/src/lib.rs".into(),
        line,
        column: 0,
    }
}

#[test]
fn test_references_empty_while_indexing_is_incomplete_not_ok() {
    // 0 references while the server is still indexing is NOT a confirmed
    // zero: it must not be reported as a plain `ok, count 0`.
    let outcome = LspQueryOutcome::<Vec<crate::lsp::client::Location>> {
        value: vec![],
        still_indexing: true,
    };
    let r = list_response("references", &outcome, None);
    assert_eq!(r["status"], "incomplete", "{r}");
    assert_eq!(r["server_indexing"], true);
    assert_eq!(r["count"], 0);
    assert!(
        r["message"].as_str().unwrap().contains("still indexing"),
        "{r}"
    );
}

#[test]
fn test_definition_empty_while_indexing_is_incomplete_not_not_found() {
    let outcome = LspQueryOutcome::<Vec<crate::lsp::client::Location>> {
        value: vec![],
        still_indexing: true,
    };
    let r = list_response("definitions", &outcome, Some("No definition found"));
    assert_eq!(r["status"], "incomplete", "{r}");
}

#[test]
fn test_list_response_after_indexing_is_confirmed() {
    let done_empty = LspQueryOutcome::<Vec<crate::lsp::client::Location>> {
        value: vec![],
        still_indexing: false,
    };
    // References: a confirmed zero stays `ok, count 0`.
    let r = list_response("references", &done_empty, None);
    assert_eq!(r["status"], "ok");
    assert_eq!(r["count"], 0);
    assert!(r.get("server_indexing").is_none());
    // Definition: a confirmed miss is `not_found`.
    let r = list_response("definitions", &done_empty, Some("No definition found"));
    assert_eq!(r["status"], "not_found");
    assert_eq!(r["message"], "No definition found");

    let found = LspQueryOutcome {
        value: vec![loc(3), loc(9)],
        still_indexing: false,
    };
    let r = list_response("references", &found, None);
    assert_eq!(r["status"], "ok");
    assert_eq!(r["count"], 2);
    assert_eq!(r["references"].as_array().unwrap().len(), 2);
}

#[test]
fn test_list_response_nonempty_while_indexing_flags_partial() {
    let partial = LspQueryOutcome {
        value: vec![loc(1)],
        still_indexing: true,
    };
    let r = list_response("references", &partial, None);
    assert_eq!(r["status"], "ok");
    assert_eq!(r["count"], 1);
    assert_eq!(r["server_indexing"], true);
    assert!(r["note"].as_str().unwrap().contains("partial"), "{r}");
}

#[test]
fn test_hover_response_indexing_honesty() {
    let r = hover_response(&LspQueryOutcome {
        value: None,
        still_indexing: true,
    });
    assert_eq!(r["status"], "incomplete");
    let r = hover_response(&LspQueryOutcome {
        value: None,
        still_indexing: false,
    });
    assert_eq!(r["status"], "not_found");
    let r = hover_response(&LspQueryOutcome {
        value: Some("fn f()".to_string()),
        still_indexing: false,
    });
    assert_eq!(r["status"], "ok");
    assert_eq!(r["hover"], "fn f()");
}
