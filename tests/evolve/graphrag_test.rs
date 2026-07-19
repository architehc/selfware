use selfware::evolve::graphrag::GraphRag;
use selfware::evolve::{Graph, Node};

fn test_graph() -> Graph {
    let mut agent = Node::code("agent", "src/agent");
    agent.lines = 100;
    agent.tokens = 400;
    agent.files = 3;
    Graph {
        nodes: vec![agent, Node::code("config", "src/config")],
        edges: vec![],
    }
}

#[test]
fn test_graphrag_returns_grounded_facts() {
    let graph = Graph {
        nodes: vec![],
        edges: vec![],
    };
    let rag = GraphRag::new(graph);
    let facts = rag.query("What is the agent module?").unwrap();
    assert!(facts.is_empty()); // no nodes yet
}

#[test]
fn test_graphrag_grounds_matching_nodes() {
    let rag = GraphRag::new(test_graph());
    let facts = rag.query("What does the agent module do?").unwrap();
    // A stub that always returns empty would fail this assertion.
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].file, "src/agent");
    assert!(facts[0].text.contains("agent"));
}

#[test]
fn test_graphrag_no_match_returns_empty() {
    let rag = GraphRag::new(test_graph());
    let facts = rag.query("nonexistent component xyzzy").unwrap();
    assert!(facts.is_empty());
}
