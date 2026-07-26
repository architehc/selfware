//! Server tests for symbol-level context retrieval: per-symbol expand
//! (`/api/context/expand?symbol=`) and task-aware symbol selection
//! (`/api/context/select/symbols`).

use axum::http::StatusCode;
use selfware::evolve::server::EvolveServer;
use selfware::evolve::{Graph, Node};

use crate::get_json;

/// A component whose file name shares the `parse` substring with two of its
/// functions, so one target both seeds file selection and filters symbols.
const PARSER_SRC: &str = "//! Parsing.\n\
                          \n\
                          pub fn parse(input: &str) -> usize {\n\
                          \x20   input.len()\n\
                          }\n\
                          \n\
                          pub fn parse_header(input: &str) -> &str {\n\
                          \x20   input\n\
                          }\n\
                          \n\
                          pub fn render(input: &str) -> String {\n\
                          \x20   input.to_string()\n\
                          }\n";

fn parser_server() -> (tempfile::TempDir, EvolveServer) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/parser.rs"), PARSER_SRC).unwrap();
    let graph = Graph {
        nodes: vec![Node::code("crate::parser", "src/parser.rs")],
        edges: vec![],
    };
    let server = EvolveServer::for_project(graph, dir.path()).unwrap();
    (dir, server)
}

#[tokio::test]
async fn expand_with_symbol_returns_only_that_symbols_span() {
    let (_dir, server) = parser_server();
    let (status, json) = get_json(
        &server,
        "/api/context/expand?component=crate::parser&symbol=parse_header",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["mode"].as_str(), Some("symbol"));
    assert_eq!(json["symbol"].as_str(), Some("parse_header"));
    let content = json["content"].as_str().unwrap();
    assert!(
        content.contains("input\n") || content.contains("    input"),
        "the symbol body must be present: {content}"
    );
    assert!(
        !content.contains("render"),
        "other symbols must be absent: {content}"
    );
}

#[tokio::test]
async fn expand_without_symbol_keeps_whole_component_behavior() {
    let (_dir, server) = parser_server();
    let (status, json) = get_json(&server, "/api/context/expand?component=crate::parser").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["mode"].as_str(), Some("signatures"));
    let content = json["content"].as_str().unwrap();
    assert!(content.contains("parse_header"));
    assert!(content.contains("render"));
}

#[tokio::test]
async fn expand_with_unknown_symbol_is_a_typed_400() {
    let (_dir, server) = parser_server();
    let (status, _) = get_json(
        &server,
        "/api/context/expand?component=crate::parser&symbol=nope",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn select_symbols_returns_matching_symbols_exact_name_first() {
    let (_dir, server) = parser_server();
    let (status, json) = get_json(
        &server,
        "/api/context/select/symbols?kind=understand&target=parse",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["kind"].as_str(), Some("understand"));
    assert_eq!(json["target"].as_str(), Some("parse"));
    assert_eq!(json["files_scanned"].as_u64(), Some(1));
    let symbols = json["symbols"].as_array().unwrap();
    let names: Vec<&str> = symbols
        .iter()
        .map(|s| s["symbol"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"parse") && names.contains(&"parse_header"),
        "expected parse symbols, got {names:?}"
    );
    assert!(
        !names.contains(&"render"),
        "non-matching symbols must be filtered out: {names:?}"
    );
    assert_eq!(
        names.first(),
        Some(&"parse"),
        "exact name match sorts first: {names:?}"
    );
    let first = &symbols[0];
    assert_eq!(first["component"].as_str(), Some("crate::parser"));
    assert_eq!(first["path"].as_str(), Some("src/parser.rs"));
    assert_eq!(first["kind"].as_str(), Some("function"));
    assert!(first["line"].as_u64().unwrap() > 0);
    assert!(first["signature"].as_str().unwrap().contains("fn parse"));
}

#[tokio::test]
async fn select_symbols_rejects_unknown_task_kind() {
    let (_dir, server) = parser_server();
    let (status, _) = get_json(
        &server,
        "/api/context/select/symbols?kind=bogus&target=parse",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
