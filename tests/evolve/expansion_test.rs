//! HTTP contract tests for the expansion catalog API
//! (`/api/expansion/*` on the evolve server). The fixture is the real
//! `expansion_recommendation/` catalog: integration tests run from the repo
//! root, so `EvolveServer::new` resolves it via project_root ".".

use axum::http::StatusCode;

use selfware::evolve::EvolveServer;

use crate::{get_json, sample_graph};

const ENGINE_COMPONENTS: [&str; 11] = [
    "agent",
    "tools",
    "evolve",
    "cognitive",
    "safety",
    "config",
    "analysis",
    "orchestration",
    "evolution",
    "api",
    "session",
];

#[tokio::test]
async fn test_expansion_index_lists_engine_components_and_total() {
    let server = EvolveServer::new(sample_graph());
    let (status, json) = get_json(&server, "/api/expansion/index").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["total_examples"], 580);
    let components: Vec<&str> = json["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    for engine in ENGINE_COMPONENTS {
        assert!(
            components.contains(&engine),
            "index missing engine component {engine}"
        );
    }
    assert_eq!(components.len(), 29);
}

#[tokio::test]
async fn test_expansion_component_returns_full_document() {
    let server = EvolveServer::new(sample_graph());
    let (status, json) = get_json(&server, "/api/expansion/evolve").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["component"], "evolve");
    assert_eq!(json["examples"].as_array().unwrap().len(), 20);
}

#[tokio::test]
async fn test_expansion_example_returns_single_example() {
    let server = EvolveServer::new(sample_graph());
    let (_, component) = get_json(&server, "/api/expansion/evolve").await;
    let first = component["examples"][0].clone();
    let id = first["id"].as_str().unwrap();

    let (status, json) = get_json(&server, &format!("/api/expansion/evolve/{id}")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json, first);
}

#[tokio::test]
async fn test_expansion_unknown_component_returns_typed_404() {
    let server = EvolveServer::new(sample_graph());
    let (status, json) = get_json(&server, "/api/expansion/no_such_component").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "unknown_component");
}

#[tokio::test]
async fn test_expansion_unknown_example_returns_typed_404() {
    let server = EvolveServer::new(sample_graph());
    let (status, json) = get_json(&server, "/api/expansion/evolve/evolve-99").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "unknown_example");

    // Unknown component under the example route is still unknown_component.
    let (status, json) = get_json(&server, "/api/expansion/nope/nope-01").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "unknown_component");
}

#[tokio::test]
async fn test_expansion_missing_catalog_dir_degrades_without_500() {
    let project = tempfile::tempdir().unwrap();
    let server = EvolveServer::for_project(sample_graph(), project.path()).unwrap();

    let (status, json) = get_json(&server, "/api/expansion/index").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["components"].as_array().unwrap().len(), 0);
    assert_eq!(json["total_examples"], 0);

    let (status, json) = get_json(&server, "/api/expansion/evolve").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "unknown_component");

    let (status, json) = get_json(&server, "/api/expansion/evolve/evolve-01").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "unknown_component");
}
