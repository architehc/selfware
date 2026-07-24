use std::fs;

use axum::http::StatusCode;
use serde_json::{json, Value};

use selfware::config::Config;
use selfware::evolve::{EvolveServer, Graph, Node};

use crate::{get_json, post_json};

/// Build a temp project with one real Rust file and a matching graph node.
fn fixture(tokens: usize) -> (tempfile::TempDir, Graph) {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let body = "pub fn f() {}\n".repeat((tokens / 4).max(1));
    fs::write(src_dir.join("a.rs"), &body).unwrap();
    let mut node = Node::code("crate::a", "src/a.rs");
    node.tokens = selfware::token_count::estimate_content_tokens(&body);
    (dir, Graph { nodes: vec![node], edges: vec![] })
}

/// POST /api/context/mode with the session header and return the JSON body.
async fn post_mode(server: &EvolveServer, mode: &str) -> Value {
    let (status, body) = post_json(server, "/api/context/mode", json!({ "mode": mode })).await;
    assert_eq!(status, StatusCode::OK);
    serde_json::from_str(&body).unwrap()
}

#[tokio::test]
async fn auto_mode_resolves_full_on_large_window_and_degrades_on_small() {
    let (dir, graph) = fixture(20_000);

    // Large window: auto resolves to a full tier and fits.
    let mut config = Config::default();
    config.context_length = 1_000_000;
    config.context_mode = "auto".to_string();
    let server = EvolveServer::with_config(graph.clone(), dir.path(), &config).unwrap();
    let (status, json) = get_json(&server, "/api/context").await;
    assert_eq!(status, StatusCode::OK);
    assert!(["full", "full_extended"].contains(&json["mode"].as_str().unwrap()));
    assert_eq!(json["requested_mode"].as_str().unwrap(), "auto");
    assert_eq!(json["fits_context_window"].as_bool().unwrap(), true);

    // 8k window: auto degrades below full, still reports coherently.
    let mut config = Config::default();
    config.context_length = 8_192;
    config.context_mode = "auto".to_string();
    let server = EvolveServer::with_config(graph, dir.path(), &config).unwrap();
    let (status, json) = get_json(&server, "/api/context").await;
    assert_eq!(status, StatusCode::OK);
    assert!(["map", "lite", "compact"].contains(&json["mode"].as_str().unwrap()));
    assert_eq!(json["requested_mode"].as_str().unwrap(), "auto");
}

#[tokio::test]
async fn invalid_context_mode_in_config_is_an_error() {
    let (dir, graph) = fixture(1_000);
    let mut config = Config::default();
    config.context_mode = "bogus".to_string();
    assert!(EvolveServer::with_config(graph, dir.path(), &config).is_err());
}

#[tokio::test]
async fn post_mode_auto_refits_and_pinned_mode_sticks() {
    let (dir, graph) = fixture(20_000);
    let mut config = Config::default();
    config.context_length = 1_000_000;
    let server = EvolveServer::with_config(graph, dir.path(), &config).unwrap();

    let json = post_mode(&server, "auto").await;
    assert_eq!(json["requested_mode"].as_str().unwrap(), "auto");

    let json = post_mode(&server, "lite").await;
    assert_eq!(json["mode"].as_str().unwrap(), "lite");
    assert_eq!(json["requested_mode"].as_str().unwrap(), "lite");
}
