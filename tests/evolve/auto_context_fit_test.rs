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

/// POST /api/context/mode and return the raw status + JSON body, so tests can
/// assert on non-200 responses (e.g. the over-budget 422).
async fn post_mode_raw(server: &EvolveServer, mode: &str) -> (StatusCode, Value) {
    let (status, body) = post_json(server, "/api/context/mode", json!({ "mode": mode })).await;
    (status, serde_json::from_str(&body).unwrap())
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
async fn auto_mode_reports_fit_and_pinned_mode_clears_it() {
    let (dir, graph) = fixture(20_000);
    let mut config = Config::default();
    config.context_length = 1_000_000;
    config.context_mode = "auto".to_string();
    let server = EvolveServer::with_config(graph.clone(), dir.path(), &config).unwrap();

    // Auto: the fit outcome rides along with the context payload.
    let (status, json) = get_json(&server, "/api/context").await;
    assert_eq!(status, StatusCode::OK);
    let fit = &json["auto_fit"];
    let expected_budget = (0.7
        * (config.context_length - config.max_tokens.min(config.context_length / 4)) as f64)
        as usize;
    assert_eq!(
        fit["budget_tokens"].as_u64().unwrap() as usize,
        expected_budget
    );
    assert_eq!(fit["fits"].as_bool().unwrap(), true);
    // The resolved tier is measured, so its size matches the fixture exactly.
    let tier_tokens: usize = graph.nodes.iter().map(|n| n.tokens).sum();
    assert_eq!(
        fit["measured_tokens"].as_u64().unwrap() as usize,
        tier_tokens
    );
    assert!(fit["measured_tokens"].as_u64().unwrap() as usize <= expected_budget);

    // Pinned: auto_fit clears to null.
    let json = post_mode(&server, "lite").await;
    assert!(json["auto_fit"].is_null());
    let (status, json) = get_json(&server, "/api/context").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["auto_fit"].is_null());

    // Back to auto: the fit outcome is reported again.
    let json = post_mode(&server, "auto").await;
    assert!(json["auto_fit"].is_object());
    let (status, json) = get_json(&server, "/api/context").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["auto_fit"].is_object());
}

/// Temp project with two real Rust files and matching graph nodes.
/// Sized so the verbatim (full tier) payload is ~20k tokens — comfortably over
/// the usable budget of an 8k context window.
fn fixture_two_files() -> (tempfile::TempDir, Graph) {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let a = "pub fn a() {}\n".repeat(1_000);
    let b = "pub fn b() {}\n".repeat(2_000);
    fs::write(src_dir.join("a.rs"), &a).unwrap();
    fs::write(src_dir.join("b.rs"), &b).unwrap();
    let mut node_a = Node::code("crate::a", "src/a.rs");
    node_a.tokens = selfware::token_count::estimate_content_tokens(&a);
    let mut node_b = Node::code("crate::b", "src/b.rs");
    node_b.tokens = selfware::token_count::estimate_content_tokens(&b);
    (dir, Graph { nodes: vec![node_a, node_b], edges: vec![] })
}

/// POST /api/context/custom with the session header and return the JSON body.
async fn post_custom(server: &EvolveServer, components: Vec<&str>) -> Value {
    let (status, body) =
        post_json(server, "/api/context/custom", json!({ "components": components })).await;
    assert_eq!(status, StatusCode::OK);
    serde_json::from_str(&body).unwrap()
}

#[tokio::test]
async fn custom_mode_applies_handpicked_selection_and_drops_unknown() {
    let (dir, graph) = fixture_two_files();
    let mut config = Config::default();
    config.context_length = 1_000_000;
    config.context_mode = "auto".to_string();
    let server = EvolveServer::with_config(graph, dir.path(), &config).unwrap();

    let json = post_custom(&server, vec!["crate::a", "crate::bogus", "crate::b"]).await;
    assert_eq!(json["mode"].as_str().unwrap(), "custom");
    assert_eq!(json["requested_mode"].as_str().unwrap(), "custom");
    let included: Vec<&str> = json["included"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap())
        .collect();
    assert_eq!(included, ["crate::a", "crate::b"], "unknown ids dropped");

    // The selection sticks for subsequent context reads.
    let (status, json) = get_json(&server, "/api/context").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["mode"].as_str().unwrap(), "custom");
    assert_eq!(json["requested_mode"].as_str().unwrap(), "custom");
}

#[tokio::test]
async fn custom_mode_empty_selection_clears_back_to_auto() {
    let (dir, graph) = fixture_two_files();
    let mut config = Config::default();
    config.context_length = 1_000_000;
    config.context_mode = "auto".to_string();
    let server = EvolveServer::with_config(graph, dir.path(), &config).unwrap();

    let json = post_custom(&server, vec!["crate::a"]).await;
    assert_eq!(json["requested_mode"].as_str().unwrap(), "custom");

    let json = post_custom(&server, vec![]).await;
    assert_eq!(json["requested_mode"].as_str().unwrap(), "auto");
    assert!(
        ["map", "lite", "compact", "full", "full_extended"]
            .contains(&json["mode"].as_str().unwrap()),
        "cleared custom re-fits to a concrete ladder tier, got {}",
        json["mode"]
    );
    assert!(json["auto_fit"].is_object());
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

#[tokio::test]
async fn context_json_reports_envelope_fields() {
    let (dir, graph) = fixture_two_files();
    let mut config = Config::default();
    config.context_length = 1_000_000;
    let server = EvolveServer::with_config(graph, dir.path(), &config).unwrap();
    let (status, json) = get_json(&server, "/api/context").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["envelope_hash"].is_string());
    assert!(json["envelope_tokens"].as_u64().unwrap() > 0);
    // Auto on a huge window resolves to full/full_extended: envelope tokens
    // must equal the composer's own estimate (verbatim source).
    assert!(
        json["envelope_tokens"].as_u64().unwrap()
            >= json["production"]["tokens"].as_u64().unwrap() / 2
    );
}

#[tokio::test]
async fn pinned_over_budget_mode_is_rejected_with_422() {
    let (dir, graph) = fixture_two_files();
    let mut config = Config::default();
    config.context_length = 8_192; // usable budget = 0.7 * (8192 - 2048) = 4300 tokens
    config.context_mode = "auto".to_string();
    let server = EvolveServer::with_config(graph, dir.path(), &config).unwrap();

    let (status, body) = post_mode_raw(&server, "full").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "context_over_budget");
    assert!(
        body["measured_tokens"].as_u64().unwrap() > body["budget_tokens"].as_u64().unwrap()
    );

    // Auto remains accepted on the same server.
    let (status, _) = post_mode_raw(&server, "auto").await;
    assert_eq!(status, StatusCode::OK);
}
