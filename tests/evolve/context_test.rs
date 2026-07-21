use selfware::evolve::context::{ContextComposer, ContextMode};
use selfware::evolve::{Graph, Node, NodeLayer};

#[test]
fn test_context_composer_full_mode_includes_all_code() {
    let graph = Graph {
        nodes: vec![
            Node::code("agent", "src/agent"),
            Node::code("tools", "src/tools"),
        ],
        edges: vec![],
    };
    let mut composer = ContextComposer::new(graph);
    composer.set_mode(ContextMode::Full);
    assert!(composer.estimate_tokens() > 0);
    assert!(composer.included_nodes().len() == 2);
}

#[test]
fn test_modes_and_token_estimation() {
    let mut with_tokens = Node::code("agent", "src/agent");
    with_tokens.tokens = 500;
    let mut with_lines = Node::code("tools", "src/tools");
    with_lines.lines = 20;
    let graph = Graph {
        nodes: vec![with_tokens, with_lines],
        edges: vec![],
    };

    // Lite (default) includes nothing.
    let mut composer = ContextComposer::new(graph);
    assert!(composer.included_nodes().is_empty());
    assert_eq!(composer.estimate_tokens(), 0);

    // Full sums measured tokens with the lines-based fallback (20 * 10).
    composer.set_mode(ContextMode::Full);
    assert_eq!(composer.estimate_tokens(), 700);

    // Preset includes only the named entry.
    composer.set_mode(ContextMode::Preset("agent".to_string()));
    assert_eq!(composer.included_nodes(), vec!["agent".to_string()]);
    assert_eq!(composer.estimate_tokens(), 500);
}

#[test]
fn test_context_modes_and_layer_summaries_distinguish_test_nodes() {
    let mut code = Node::code("agent", "src/agent");
    code.tokens = 500;
    code.files = 3;
    code.inline_test_ranges = 2;
    code.inline_test_lines = 18;
    code.inline_test_tokens = 80;

    let mut test = Node::test("tests::agent_test", "tests/agent_test.rs");
    test.tokens = 120;
    test.files = 1;

    let mut example = Node::test("example::demo", "examples/demo.rs");
    example.tokens = 30;
    example.files = 1;

    let mut concept = Node::code("safety", "safety");
    concept.layer = NodeLayer::Concept;
    concept.path = None;
    concept.tokens = 40;
    concept.files = 0;

    let structure = Node::structure("repo::src");

    let graph = Graph {
        nodes: vec![code, test, example, concept, structure],
        edges: vec![],
    };
    let mut composer = ContextComposer::new(graph);

    assert_eq!(composer.mode_name(), "lite");
    assert!(composer.layer_summaries().is_empty());

    composer.set_mode(ContextMode::Full);
    assert_eq!(composer.mode_name(), "full");
    assert_eq!(composer.included_nodes(), vec!["agent".to_string()]);
    assert_eq!(
        composer.layer_summaries(),
        vec![selfware::evolve::ContextLayerSummary {
            layer: NodeLayer::Code,
            nodes: 1,
            tokens: 420,
            files: 3,
        }]
    );

    composer.set_mode(ContextMode::FullExtended);
    assert_eq!(composer.mode_name(), "full_extended");
    assert_eq!(
        composer.included_nodes(),
        vec![
            "agent".to_string(),
            "tests::agent_test".to_string(),
            "example::demo".to_string()
        ]
    );
    assert_eq!(composer.estimate_tokens(), 650);
    assert_eq!(composer.layer_summaries().len(), 2);
    let summary = composer.summary();
    assert_eq!(composer.mode(), &ContextMode::FullExtended);
    assert_eq!(summary.production.nodes, 1);
    assert_eq!(summary.production.tokens, 500);
    assert_eq!(summary.production.files, 3);
    assert_eq!(summary.tests.nodes, 1);
    assert_eq!(summary.tests.tokens, 120);
    assert_eq!(summary.examples.nodes, 1);
    assert_eq!(summary.examples.tokens, 30);
    assert!(!summary.file_partition_complete);
    assert_eq!(summary.production_files_with_inline_tests, 1);
    assert_eq!(summary.inline_test_ranges, 2);
    assert_eq!(summary.inline_test_lines, 18);
    assert_eq!(summary.estimated_tokens, 650);
    assert_eq!(
        serde_json::to_value(summary).unwrap()["mode"],
        "full_extended"
    );

    composer.set_mode(ContextMode::Preset("safety".to_string()));
    assert_eq!(composer.mode_name(), "preset");
    assert_eq!(composer.layer_summaries()[0].layer, NodeLayer::Concept);
}
