use super::*;

#[test]
fn test_language_from_path() {
    assert_eq!(Language::from_path("src/main.rs"), Some(Language::Rust));
    assert_eq!(Language::from_path("app.py"), Some(Language::Python));
    assert_eq!(Language::from_path("index.ts"), Some(Language::TypeScript));
    assert_eq!(Language::from_path("index.tsx"), Some(Language::TypeScript));
    assert_eq!(Language::from_path("app.js"), Some(Language::JavaScript));
    assert_eq!(Language::from_path("main.go"), Some(Language::Go));
    assert_eq!(Language::from_path("README.md"), None);
    assert_eq!(Language::from_path("Makefile"), None);
}

#[test]
fn test_language_id() {
    assert_eq!(Language::Rust.id(), "rust");
    assert_eq!(Language::Python.id(), "python");
    assert_eq!(Language::TypeScript.id(), "typescript");
    assert_eq!(Language::JavaScript.id(), "javascript");
    assert_eq!(Language::Go.id(), "go");
}

#[test]
fn test_file_uri() {
    // Already a URI should pass through on all platforms
    let uri = LspClient::file_uri("file:///already/a/uri.rs");
    assert_eq!(uri, "file:///already/a/uri.rs");

    // Unix-style absolute paths only valid on non-Windows
    #[cfg(not(target_os = "windows"))]
    {
        let uri = LspClient::file_uri("/home/user/project/src/main.rs");
        assert_eq!(uri, "file:///home/user/project/src/main.rs");
    }
}

#[test]
fn test_file_uri_percent_encoding() {
    // Verify that special characters are percent-encoded.
    // We test with a path that doesn't need canonicalization by
    // using an already-absolute path that contains special chars.
    #[cfg(not(target_os = "windows"))]
    {
        // Use a temp dir with special characters in the name
        let dir = std::env::temp_dir().join("selfware test#dir%");
        // Create the directory so canonicalize works
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("test file.rs");
        let _ = std::fs::write(&file, "fn main() {}");

        let uri = LspClient::file_uri(file.to_str().unwrap());
        // The URI should contain percent-encoded versions of space,
        // #, and %
        assert!(uri.starts_with("file:///"));
        assert!(
            !uri.contains(' '),
            "URI should not contain raw spaces: {}",
            uri
        );
        assert!(!uri.contains('#'), "URI should not contain raw #: {}", uri);

        // Clean up
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn test_uri_to_path() {
    assert_eq!(LspClient::uri_to_path("/plain/path"), "/plain/path");

    #[cfg(not(target_os = "windows"))]
    assert_eq!(
        LspClient::uri_to_path("file:///home/user/main.rs"),
        "/home/user/main.rs"
    );
}

#[test]
fn test_parse_locations_null() {
    let locs = LspClient::parse_locations(&Value::Null).unwrap();
    assert!(locs.is_empty());
}

#[test]
fn test_parse_locations_single() {
    let val = serde_json::json!({
        "uri": "file:///src/main.rs",
        "range": {
            "start": { "line": 10, "character": 5 },
            "end": { "line": 10, "character": 15 }
        }
    });
    let locs = LspClient::parse_locations(&val).unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].file, "/src/main.rs");
    assert_eq!(locs[0].line, 10);
    assert_eq!(locs[0].column, 5);
}

#[test]
fn test_parse_locations_array() {
    let val = serde_json::json!([
        {
            "uri": "file:///a.rs",
            "range": { "start": { "line": 1, "character": 2 }, "end": { "line": 1, "character": 10 } }
        },
        {
            "uri": "file:///b.rs",
            "range": { "start": { "line": 5, "character": 0 }, "end": { "line": 5, "character": 8 } }
        }
    ]);
    let locs = LspClient::parse_locations(&val).unwrap();
    assert_eq!(locs.len(), 2);
    assert_eq!(locs[0].file, "/a.rs");
    assert_eq!(locs[1].file, "/b.rs");
}

#[test]
fn test_parse_locations_location_link() {
    let val = serde_json::json!([{
        "targetUri": "file:///target.rs",
        "targetSelectionRange": {
            "start": { "line": 20, "character": 4 },
            "end": { "line": 20, "character": 12 }
        },
        "targetRange": {
            "start": { "line": 18, "character": 0 },
            "end": { "line": 25, "character": 1 }
        }
    }]);
    let locs = LspClient::parse_locations(&val).unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].file, "/target.rs");
    assert_eq!(locs[0].line, 20);
    assert_eq!(locs[0].column, 4);
}

#[test]
fn test_parse_symbols_flat() {
    let val = serde_json::json!([
        {
            "name": "main",
            "kind": 12,
            "location": {
                "uri": "file:///main.rs",
                "range": { "start": { "line": 0, "character": 3 }, "end": { "line": 5, "character": 1 } }
            }
        },
        {
            "name": "Config",
            "kind": 23,
            "location": {
                "uri": "file:///main.rs",
                "range": { "start": { "line": 7, "character": 4 }, "end": { "line": 10, "character": 1 } }
            }
        }
    ]);
    let symbols = LspClient::parse_symbols(&val).unwrap();
    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[0].name, "main");
    assert_eq!(symbols[0].kind, "function");
    assert_eq!(symbols[1].name, "Config");
    assert_eq!(symbols[1].kind, "struct");
}

#[test]
fn test_parse_symbols_hierarchical() {
    let val = serde_json::json!([{
        "name": "MyStruct",
        "kind": 23,
        "selectionRange": { "start": { "line": 0, "character": 4 }, "end": { "line": 0, "character": 12 } },
        "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 10, "character": 1 } },
        "children": [
            {
                "name": "field_a",
                "kind": 8,
                "selectionRange": { "start": { "line": 1, "character": 4 }, "end": { "line": 1, "character": 11 } },
                "range": { "start": { "line": 1, "character": 0 }, "end": { "line": 1, "character": 20 } }
            },
            {
                "name": "do_thing",
                "kind": 6,
                "selectionRange": { "start": { "line": 5, "character": 7 }, "end": { "line": 5, "character": 15 } },
                "range": { "start": { "line": 5, "character": 0 }, "end": { "line": 9, "character": 1 } }
            }
        ]
    }]);
    let symbols = LspClient::parse_symbols(&val).unwrap();
    assert_eq!(symbols.len(), 3);
    assert_eq!(symbols[0].name, "MyStruct");
    assert_eq!(symbols[0].kind, "struct");
    assert_eq!(symbols[1].name, "field_a");
    assert_eq!(symbols[1].kind, "field");
    assert_eq!(symbols[2].name, "do_thing");
    assert_eq!(symbols[2].kind, "method");
}

#[test]
fn test_symbol_kind_name() {
    assert_eq!(symbol_kind_name(12), "function");
    assert_eq!(symbol_kind_name(5), "class");
    assert_eq!(symbol_kind_name(23), "struct");
    assert_eq!(symbol_kind_name(6), "method");
    assert_eq!(symbol_kind_name(999), "unknown");
}

#[test]
fn test_diagnostics_parsing() {
    let params = serde_json::json!({
        "uri": "file:///test.rs",
        "diagnostics": [
            {
                "range": { "start": { "line": 5, "character": 10 }, "end": { "line": 5, "character": 20 } },
                "severity": 1,
                "message": "expected `;`"
            },
            {
                "range": { "start": { "line": 12, "character": 0 }, "end": { "line": 12, "character": 15 } },
                "severity": 2,
                "message": "unused variable"
            }
        ]
    });

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let store: Arc<Mutex<HashMap<String, Vec<Diagnostic>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        LspServerConnection::handle_diagnostics(&params, &store).await;

        let s = store.lock().await;
        let diags = s.get("file:///test.rs").unwrap();
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].severity, "error");
        assert_eq!(diags[0].message, "expected `;`");
        assert_eq!(diags[0].line, 5);
        assert_eq!(diags[1].severity, "warning");
        assert_eq!(diags[1].message, "unused variable");
    });
}

#[test]
fn test_server_candidates() {
    let rust_candidates = server_candidates(Language::Rust);
    assert!(!rust_candidates.is_empty());
    assert_eq!(rust_candidates[0].0, "rust-analyzer");

    let py_candidates = server_candidates(Language::Python);
    assert!(py_candidates.len() >= 2);

    let go_candidates = server_candidates(Language::Go);
    assert_eq!(go_candidates[0].0, "gopls");
}

#[test]
fn test_location_serialization() {
    let loc = Location {
        file: "/src/main.rs".to_string(),
        line: 42,
        column: 7,
    };
    let json = serde_json::to_value(&loc).unwrap();
    assert_eq!(json["file"], "/src/main.rs");
    assert_eq!(json["line"], 42);
    assert_eq!(json["column"], 7);
}

#[test]
fn test_symbol_info_serialization() {
    let sym = SymbolInfo {
        name: "my_func".to_string(),
        kind: "function".to_string(),
        line: 10,
        column: 0,
    };
    let json = serde_json::to_value(&sym).unwrap();
    assert_eq!(json["name"], "my_func");
    assert_eq!(json["kind"], "function");
}

#[test]
fn test_diagnostic_serialization() {
    let diag = Diagnostic {
        message: "type mismatch".to_string(),
        severity: "error".to_string(),
        line: 15,
        column: 8,
    };
    let json = serde_json::to_value(&diag).unwrap();
    assert_eq!(json["severity"], "error");
    assert_eq!(json["message"], "type mismatch");
}
