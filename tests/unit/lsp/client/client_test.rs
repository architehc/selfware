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

// ---------------------------------------------------------------------------
// Indexing state: active progress tokens are tracked as a set
// ---------------------------------------------------------------------------

async fn dispatch_progress(
    indexing: &Arc<std::sync::Mutex<IndexingState>>,
    token: serde_json::Value,
    kind: &str,
) {
    let pending: Arc<Mutex<PendingMap>> = Arc::new(Mutex::new(HashMap::new()));
    let diags = Arc::new(Mutex::new(HashMap::new()));
    let msg = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "$/progress",
        "params": { "token": token, "value": { "kind": kind, "title": "x" } }
    });
    LspServerConnection::dispatch_message(msg, &pending, &diags, indexing).await;
}

#[tokio::test]
async fn indexing_stays_active_until_every_progress_stream_ends() {
    let indexing = Arc::new(std::sync::Mutex::new(IndexingState::default()));
    let is = |i: &Arc<std::sync::Mutex<IndexingState>>| i.lock().unwrap().is_indexing();

    dispatch_progress(
        &indexing,
        serde_json::json!("rustAnalyzer/Fetching"),
        "begin",
    )
    .await;
    dispatch_progress(
        &indexing,
        serde_json::json!("rustAnalyzer/Indexing"),
        "begin",
    )
    .await;
    dispatch_progress(&indexing, serde_json::json!(7), "begin").await;
    assert!(is(&indexing));

    // Ending ONE stream must not clear indexing while others still run
    // (the old AtomicBool was cleared by any `end`).
    dispatch_progress(&indexing, serde_json::json!("rustAnalyzer/Fetching"), "end").await;
    assert!(is(&indexing), "other progress streams are still active");
    dispatch_progress(&indexing, serde_json::json!(7), "report").await;
    dispatch_progress(&indexing, serde_json::json!(7), "end").await;
    assert!(is(&indexing), "Indexing stream is still active");

    dispatch_progress(&indexing, serde_json::json!("rustAnalyzer/Indexing"), "end").await;
    assert!(!is(&indexing), "all streams ended");
}

#[test]
fn server_status_busy_counts_as_indexing() {
    let mut st = IndexingState::default();
    assert!(!st.is_indexing());
    st.server_busy = true;
    assert!(st.is_indexing());
}

// ---------------------------------------------------------------------------
// query_settling_indexing: empty-while-indexing waits (bounded), retries once
// ---------------------------------------------------------------------------

#[tokio::test]
async fn empty_result_while_indexing_retries_after_indexing_finishes() {
    use std::sync::atomic::AtomicUsize;
    let indexing = Arc::new(AtomicBool::new(true));
    let calls = Arc::new(AtomicUsize::new(0));

    // Indexing finishes after 150ms.
    {
        let indexing = Arc::clone(&indexing);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            indexing.store(false, Ordering::SeqCst);
        });
    }

    let probe = Arc::clone(&indexing);
    let outcome = query_settling_indexing(
        move || probe.load(Ordering::SeqCst),
        Vec::is_empty,
        Duration::from_secs(10),
        || {
            let calls = Arc::clone(&calls);
            async move {
                // First answer (mid-indexing) is empty; the retry sees results.
                if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    Ok(vec![])
                } else {
                    Ok(vec![1u32, 2, 3])
                }
            }
        },
    )
    .await
    .unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 2, "retried exactly once");
    assert_eq!(outcome.value, vec![1, 2, 3]);
    assert!(!outcome.still_indexing);
}

#[tokio::test]
async fn empty_result_while_indexing_never_ends_is_marked_incomplete_after_bounded_wait() {
    use std::sync::atomic::AtomicUsize;
    let calls = Arc::new(AtomicUsize::new(0));
    let start = std::time::Instant::now();
    let outcome = query_settling_indexing(
        || true,
        Vec::<u32>::is_empty,
        Duration::from_millis(200),
        || {
            let calls = Arc::clone(&calls);
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(vec![])
            }
        },
    )
    .await
    .unwrap();
    let elapsed = start.elapsed();

    assert!(outcome.value.is_empty());
    assert!(
        outcome.still_indexing,
        "an empty result the index could not vouch for must be flagged"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2, "one bounded retry");
    assert!(elapsed >= Duration::from_millis(190), "waited: {elapsed:?}");
    assert!(elapsed < Duration::from_secs(3), "bounded: {elapsed:?}");
}

#[tokio::test]
async fn nonempty_or_idle_results_are_not_delayed() {
    use std::sync::atomic::AtomicUsize;
    let calls = Arc::new(AtomicUsize::new(0));
    let c = Arc::clone(&calls);
    let outcome = query_settling_indexing(
        || true,
        Vec::is_empty,
        Duration::from_secs(10),
        move || {
            let c = Arc::clone(&c);
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(vec![5u32])
            }
        },
    )
    .await
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(outcome.value, vec![5]);
    assert!(
        outcome.still_indexing,
        "partial-results flag still reported"
    );

    let start = std::time::Instant::now();
    let outcome = query_settling_indexing(
        || false,
        Vec::<u32>::is_empty,
        Duration::from_secs(10),
        || async { Ok(vec![]) },
    )
    .await
    .unwrap();
    assert!(outcome.value.is_empty() && !outcome.still_indexing);
    assert!(start.elapsed() < Duration::from_millis(500));
}

// ---------------------------------------------------------------------------
// Child death: fail fast with the real cause
// ---------------------------------------------------------------------------

#[test]
fn remediation_hint_recognizes_missing_rustup_component() {
    let old = vec![
        "error: Unknown binary 'rust-analyzer' in official toolchain 'stable-aarch64-apple-darwin'."
            .to_string(),
    ];
    let hint = remediation_hint("rust-analyzer", &old).expect("hint");
    assert!(hint.contains("rust-analyzer is not installed for this toolchain"));
    assert!(hint.contains("rustup component add rust-analyzer"));

    let new = vec![
        "error: 'rust-analyzer' is not installed for the toolchain 'stable-x86_64-unknown-linux-gnu'."
            .to_string(),
        "To install, run `rustup component add rust-analyzer`".to_string(),
    ];
    assert!(remediation_hint("rust-analyzer", &new).is_some());

    assert!(remediation_hint("gopls", &["panic: boom".to_string()]).is_none());
}

/// A rustup-proxy-like server that prints the "Unknown binary" error and
/// exits immediately, appending to `counter` on every spawn.
#[cfg(unix)]
fn exiting_server_candidates(counter: &Path) -> Vec<(String, Vec<String>)> {
    let script = format!(
        "echo spawned >> '{}'; \
         echo \"error: Unknown binary 'rust-analyzer' in official toolchain 'stable-aarch64-apple-darwin'.\" >&2; \
         exit 1",
        counter.display()
    );
    vec![("sh".to_string(), vec!["-c".to_string(), script])]
}

#[cfg(unix)]
#[tokio::test]
async fn server_that_exits_immediately_fails_fast_with_cause_and_is_not_retried() {
    let dir = tempfile::tempdir().unwrap();
    let counter = dir.path().join("spawns.txt");
    let file = dir.path().join("lib.rs");
    std::fs::write(&file, "fn main() {}\n").unwrap();
    let file = file.to_str().unwrap().to_string();

    let client = LspClient::new(dir.path())
        .with_server_candidates(Language::Rust, exiting_server_candidates(&counter));

    let start = std::time::Instant::now();
    let err = client
        .find_references(&file, 0, 0)
        .await
        .expect_err("a dead server must fail the call");
    let elapsed = start.elapsed();
    let msg = format!("{err:#}");
    assert!(
        elapsed < Duration::from_secs(1),
        "dead server must fail well under the 5s/30s timeouts, took {elapsed:?}: {msg}"
    );
    assert!(
        msg.contains("rust-analyzer is not installed for this toolchain")
            && msg.contains("rustup component add rust-analyzer"),
        "real cause + remediation expected, got: {msg}"
    );
    assert!(
        msg.contains("Unknown binary"),
        "stderr tail expected: {msg}"
    );
    assert!(
        !msg.contains("No LSP server available"),
        "must not claim the server is missing: {msg}"
    );

    // Remembered for the session: the next call fails immediately with the
    // recorded cause and does NOT spawn the server again.
    let start = std::time::Instant::now();
    let again = client
        .goto_definition(&file, 0, 0)
        .await
        .expect_err("failed start is remembered");
    let again_msg = format!("{again:#}");
    assert!(start.elapsed() < Duration::from_millis(200), "{again_msg}");
    assert!(
        again_msg.contains("failed to start earlier") && again_msg.contains("rustup component add"),
        "{again_msg}"
    );
    let spawns = std::fs::read_to_string(&counter).unwrap();
    assert_eq!(spawns.lines().count(), 1, "server spawned once: {spawns:?}");
}

#[cfg(unix)]
#[tokio::test]
async fn missing_server_binary_still_reports_not_installed() {
    let dir = tempfile::tempdir().unwrap();
    let client = LspClient::new(dir.path()).with_server_candidates(
        Language::Go,
        vec![("selfware-no-such-lsp-binary-xyz".to_string(), vec![])],
    );
    let err = client
        .find_references(dir.path().join("main.go").to_str().unwrap(), 0, 0)
        .await
        .unwrap_err();
    assert!(
        format!("{err:#}").contains("No LSP server available"),
        "{err:#}"
    );
}

/// A server that answers `initialize` and then dies while a request is
/// pending: the pending request must fail as soon as the process exits (not
/// after the 30s request timeout), with exit status + stderr.
#[cfg(unix)]
#[tokio::test]
async fn server_dying_mid_session_fails_pending_request_fast() {
    let body = r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}"#;
    let script = format!(
        "dd bs=1 count=1 >/dev/null 2>&1; \
         printf 'Content-Length: %d\\r\\n\\r\\n%s' {} '{}'; \
         sleep 1; echo 'fatal: index corrupted' >&2; exit 3",
        body.len(),
        body
    );
    let dir = tempfile::tempdir().unwrap();
    let conn = LspServerConnection::spawn(
        "sh",
        &["-c".to_string(), script],
        dir.path(),
        Language::Rust,
    )
    .await
    .expect("spawn fake server");
    conn.initialize()
        .await
        .expect("fake server answers initialize");

    let start = std::time::Instant::now();
    let err = conn
        .request("textDocument/references", serde_json::json!({}))
        .await
        .expect_err("server dies with the request pending");
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "pending request must fail when the server exits, took {elapsed:?}"
    );
    let typed = err
        .downcast_ref::<LspTransportError>()
        .unwrap_or_else(|| panic!("expected typed transport error, got {err:#}"));
    let msg = typed.to_string();
    assert!(
        matches!(typed, LspTransportError::ServerExited { .. }),
        "{msg}"
    );
    assert!(msg.contains('3'), "exit status expected: {msg}");
    assert!(msg.contains("fatal: index corrupted"), "stderr tail: {msg}");

    // Dead now: later requests and notifications fail immediately.
    let start = std::time::Instant::now();
    let again = conn
        .request("textDocument/hover", serde_json::json!({}))
        .await
        .unwrap_err();
    assert!(start.elapsed() < Duration::from_millis(200));
    assert_eq!(again.downcast_ref::<LspTransportError>(), Some(typed));
    assert!(conn
        .notify("textDocument/didClose", serde_json::json!({}))
        .await
        .is_err());
}

/// Writing to a server that closed its stdin (but keeps running, so the
/// reader sees no EOF) is a typed broken pipe that marks the connection dead.
#[cfg(unix)]
#[tokio::test]
async fn write_to_closed_stdin_is_typed_broken_pipe() {
    let dir = tempfile::tempdir().unwrap();
    let conn = LspServerConnection::spawn(
        "sh",
        &["-c".to_string(), "exec 0<&-; sleep 30".to_string()],
        dir.path(),
        Language::Rust,
    )
    .await
    .expect("spawn sh");
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut broken = None;
    for _ in 0..5 {
        let err = conn
            .request_with_timeout(
                "textDocument/hover",
                serde_json::json!({}),
                Duration::from_secs(1),
            )
            .await
            .unwrap_err();
        match err.downcast_ref::<LspTransportError>() {
            Some(e @ LspTransportError::BrokenPipe { .. }) => {
                broken = Some(e.clone());
                break;
            }
            Some(LspTransportError::TimedOut { .. }) => continue,
            other => panic!("unexpected error: {other:?} ({err:#})"),
        }
    }
    let broken = broken.expect("write to closed stdin must surface as BrokenPipe");
    assert!(broken.to_string().contains("broken pipe"), "{broken}");
    let start = std::time::Instant::now();
    let again = conn.request("x", serde_json::json!({})).await.unwrap_err();
    assert!(start.elapsed() < Duration::from_millis(200));
    assert_eq!(again.downcast_ref::<LspTransportError>(), Some(&broken));
    conn.kill_now().await;
}
