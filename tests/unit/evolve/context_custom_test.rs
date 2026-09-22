use selfware::evolve::{
    context_reduce::reduce_source, skeleton::extract_rust_skeleton, ContextComposer, ContextMode,
    Graph, Node,
};
use selfware::token_count::estimate_content_tokens;
use std::path::Path;

/// Graph with two plain code nodes (no readable files needed — the composer
/// works off scan-time token counts).
fn fixture() -> Graph {
    let mut a = Node::code("crate::a", "src/a.rs");
    a.tokens = 1_000;
    let mut b = Node::code("crate::b", "src/b.rs");
    b.tokens = 2_000;
    Graph {
        nodes: vec![a, b],
        edges: vec![],
    }
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

/// A real `.rs` fixture whose signature skeleton is a SMALL fraction of the
/// file (few public items, fat bodies). This is the shape for which the old
/// blind 0.18 signature fraction overstated Lite by ~3x.
fn rust_fixture() -> (tempfile::TempDir, Graph, String) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    let mut src = String::from(
        "pub struct Sink {\n    pub seed: u64,\n}\n\nimpl Sink {\n    pub fn run(&self) -> u64 {\n",
    );
    // 60 fat filler body lines: the skeleton knows nothing about them, so the
    // measured Lite cost stays tiny while the full-file count balloons.
    for i in 0..60 {
        src.push_str(&format!(
            "        let filler{i} = (i as u64).wrapping_mul(0x9E3779B97F4A7C15).rotate_left(17);\n"
        ));
    }
    src.push_str(
        "        (0..64).fold(self.seed, |acc, k| acc.wrapping_mul(k as u64))\n    }\n}\n",
    );
    let path = dir.path().join("src").join("est.rs");
    std::fs::write(&path, &src).unwrap();

    let mut node = Node::code("crate::est", "src/est.rs");
    node.tokens = estimate_content_tokens(&src);
    node.lines = src.lines().count();
    let graph = Graph {
        nodes: vec![node],
        edges: vec![],
    };
    (dir, graph, src)
}

/// Regression (pass-3, Rule 4): a representative Lite estimate must come from
/// the MEASURED skeleton projection, not from the old 0.18 fraction — for a
/// body-heavy file the fraction exaggerates the signature tier by ~3x (the
/// tree-wide overstatement was ~42%).
#[test]
fn lite_estimate_uses_measured_skeleton_not_the_fraction() {
    let (dir, graph, src) = rust_fixture();
    let code_tokens = graph.nodes[0].tokens;

    let mut composer = ContextComposer::with_root(graph, dir.path());
    composer.set_mode(ContextMode::Lite);

    // The estimator must agree with the measured projection the envelope
    // ships (extract_rust_skeleton is the same machinery TierMeasurer uses).
    let measured = extract_rust_skeleton(Path::new("src/est.rs"), &src).token_count;
    assert_eq!(composer.estimate_tokens(), measured);

    // And it must NOT be the blind fraction: the old estimate diverged from
    // the measured skeleton by ~42% tree-wide, ~3x for this fixture. This
    // guard fails if anyone reverts to the fraction.
    let stale_fraction = (0.18_f64 * code_tokens as f64).round() as usize;
    assert_ne!(
        measured, stale_fraction,
        "fixture must separate the measured skeleton from the 0.18 fraction \
         (measured={measured}, fraction={stale_fraction})"
    );
}

/// Regression (pass-3, Rule 4): the Compact estimate must come from the
/// measured comment-stripped source, not the 0.82 fraction.
#[test]
fn compact_estimate_uses_measured_stripped_source() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    let mut src = String::from("//! Module header\n\npub fn f() -> usize {\n");
    for i in 0..40 {
        src.push_str(&format!(
            "    // a comment line only explaining filler {i}\n"
        ));
    }
    src.push_str("    42\n}\n");
    std::fs::write(dir.path().join("src/est.rs"), &src).unwrap();

    let mut node = Node::code("crate::est", "src/est.rs");
    node.tokens = estimate_content_tokens(&src);
    node.lines = src.lines().count();
    let code_tokens = node.tokens;
    let graph = Graph {
        nodes: vec![node],
        edges: vec![],
    };

    let mut composer = ContextComposer::with_root(graph, dir.path());
    composer.set_mode(ContextMode::Compact);

    let measured = estimate_content_tokens(&reduce_source(&src));
    assert_eq!(composer.estimate_tokens(), measured);
    // Comment-heavy fixture: the 0.82 fraction keeps ~7x too many token.
    assert_ne!(measured, (0.82_f64 * code_tokens as f64).round() as usize);
}

/// The fractions survive ONLY as per-node fallbacks: when a `.rs` node's
/// source cannot be read, the estimate degrades to the 0.18 factor (the same
/// fallback `TierMeasurer::measure_lite` uses) — it must keep the old
/// behavior for unreadable files, never the measured path.
#[test]
fn unreadable_source_falls_back_to_per_node_fraction() {
    let dir = tempfile::tempdir().unwrap();
    let mut node = Node::code("crate::missing", "src/does_not_exist.rs");
    node.tokens = 1000;
    let graph = Graph {
        nodes: vec![node],
        edges: vec![],
    };

    let mut composer = ContextComposer::with_root(graph, dir.path());
    composer.set_mode(ContextMode::Lite);
    assert_eq!(
        composer.estimate_tokens(),
        (0.18_f64 * 1000.0).round() as usize
    );

    let mut composer = ContextComposer::with_root(Graph::default(), dir.path());
    composer.set_mode(ContextMode::Compact);
    assert_eq!(composer.estimate_tokens(), 0);
}
