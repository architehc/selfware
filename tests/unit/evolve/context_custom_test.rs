use selfware::evolve::{ContextComposer, ContextMode, Graph, Node};

/// Graph with two plain code nodes (no readable files needed — the composer
/// works off scan-time token counts).
fn fixture() -> Graph {
    let mut a = Node::code("crate::a", "src/a.rs");
    a.tokens = 1_000;
    let mut b = Node::code("crate::b", "src/b.rs");
    b.tokens = 2_000;
    Graph { nodes: vec![a, b], edges: vec![] }
}

#[test]
fn set_custom_filters_unknown_ids_and_becomes_custom() {
    let mut composer = ContextComposer::new(fixture());
    composer.set_custom(vec![
        "crate::a".to_string(),
        "crate::bogus".to_string(),
        "crate::b".to_string(),
    ]);
    assert_eq!(composer.mode(), &ContextMode::Custom);
    assert_eq!(
        composer.included_nodes(),
        vec!["crate::a".to_string(), "crate::b".to_string()],
        "unknown ids are dropped, input order is kept"
    );
}

#[test]
fn set_custom_with_empty_list_still_becomes_custom() {
    let mut composer = ContextComposer::new(fixture());
    composer.set_custom(vec![]);
    assert_eq!(composer.mode(), &ContextMode::Custom);
    assert!(composer.included_nodes().is_empty());
    assert_eq!(composer.estimate_tokens(), 0);
}

#[test]
fn custom_estimates_like_the_lite_signature_fraction() {
    let graph = fixture();
    let all: Vec<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();

    let mut lite = ContextComposer::new(graph.clone());
    lite.set_mode(ContextMode::Lite);

    let mut custom = ContextComposer::new(graph);
    custom.set_custom(all);

    assert!(lite.estimate_tokens() > 0);
    assert_eq!(
        custom.estimate_tokens(),
        lite.estimate_tokens(),
        "custom selections load at skeleton (Lite) detail"
    );
}

#[test]
fn custom_mode_serde_roundtrip_is_snake_case() {
    let json = serde_json::to_string(&ContextMode::Custom).unwrap();
    assert_eq!(json, "\"custom\"");
    let parsed: ContextMode = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, ContextMode::Custom);
}
