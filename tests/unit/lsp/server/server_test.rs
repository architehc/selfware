use super::*;

#[test]
fn extracts_token_at_cursor() {
    let source = "pub fn build_widget(widget: Widget) {}\n";
    assert_eq!(
        extract_token_at(source, 0, 8).as_deref(),
        Some("build_widget")
    );
    assert_eq!(extract_token_at(source, 0, 26).as_deref(), Some("widget"));
}

// Windows: `Url::to_file_path` parses `file:///tmp/example` as a UNC-style
// path (`\tmp\example`), not the bare absolute `/tmp/example` we assert on.
// There is no equivalent Windows literal to substitute for `/tmp` in this
// assertion, so we keep the gate scoped to non-Windows.
#[cfg(not(target_os = "windows"))]
#[test]
fn resolves_root_from_initialize_params() {
    let params = serde_json::json!({
        "workspaceFolders": [
            { "uri": "file:///tmp/example" }
        ]
    });
    assert_eq!(
        extract_root(Some(&params)),
        Some(PathBuf::from("/tmp/example"))
    );
}

#[test]
fn maps_symbol_kinds_to_lsp_numbers() {
    assert_eq!(symbol_kind_number(&SymbolKind::Function), 12);
    assert_eq!(symbol_kind_number(&SymbolKind::Struct), 23);
    assert_eq!(symbol_kind_number(&SymbolKind::Trait), 11);
}
