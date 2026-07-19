use selfware::evolve::context::{ContextComposer, ContextMode};
use selfware::evolve::{Graph, Node};

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
