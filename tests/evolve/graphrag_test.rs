use selfware::evolve::graphrag::GraphRag;
use selfware::evolve::{Graph, Node};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

fn test_graph(path: &Path) -> Graph {
    let mut agent = Node::code("agent", &path.to_string_lossy());
    // Deliberately stale graph metrics must not appear as source evidence.
    agent.lines = 100;
    agent.tokens = 400;
    agent.files = 3;
    Graph {
        nodes: vec![agent],
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
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("agent.rs");
    let source = [
        "use crate::config::Config;",
        "",
        "pub struct Agent {",
        "    config: Config,",
        "}",
        "",
        "impl Agent {}",
    ]
    .join("\n");
    fs::write(&path, &source).unwrap();

    let rag = GraphRag::new(test_graph(&path));
    let facts = rag.query("What does the agent module do?").unwrap();
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].file, path.to_string_lossy());
    assert!(facts[0].text.contains("agent"));
    assert_eq!(facts[0].line_range, (1, 5));
    assert_eq!(
        facts[0].excerpt,
        [
            "use crate::config::Config;",
            "",
            "pub struct Agent {",
            "    config: Config,",
            "}"
        ]
        .join("\n")
    );
    assert_eq!(
        facts[0].content_hash,
        hex::encode(Sha256::digest(source.as_bytes()))
    );
    assert_eq!(facts[0].source, "workspace_snapshot");
    assert!(facts[0].evidence_id.starts_with("GF-"));
    assert_eq!(facts[0].evidence_id.len(), 67);
    assert!(!facts[0].text.contains("100 lines"));
}

#[test]
fn test_graphrag_no_match_returns_empty() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("agent.rs");
    fs::write(&path, "pub struct Agent;\n").unwrap();
    let rag = GraphRag::new(test_graph(&path));
    let facts = rag.query("nonexistent component xyzzy").unwrap();
    assert!(facts.is_empty());
}

#[test]
fn test_graphrag_line_ranges_are_one_based_and_excerpt_is_exact() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("worker.rs");
    let source = "line one\nline two\npub fn worker() {}\nline four\nline five\nline six\n";
    fs::write(&path, source).unwrap();
    let graph = Graph {
        nodes: vec![Node::code("worker", &path.to_string_lossy())],
        edges: vec![],
    };

    let fact = GraphRag::new(graph).query("worker").unwrap().remove(0);

    assert_eq!(fact.line_range, (1, 5));
    assert_eq!(
        fact.excerpt,
        "line one\nline two\npub fn worker() {}\nline four\nline five"
    );
}

#[test]
fn test_graphrag_evidence_identity_is_snapshot_specific_and_deterministic() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("agent.rs");
    fs::write(&path, "pub struct Agent;\n").unwrap();
    let rag = GraphRag::new(test_graph(&path));

    let first = rag.query("agent").unwrap().remove(0);
    let repeated = rag.query("agent").unwrap().remove(0);
    assert_eq!(first.evidence_id, repeated.evidence_id);
    assert_eq!(first.content_hash, repeated.content_hash);

    fs::write(&path, "pub struct Agent { id: u64 }\n").unwrap();
    let changed = rag.query("agent").unwrap().remove(0);
    assert_ne!(first.content_hash, changed.content_hash);
    assert_ne!(first.evidence_id, changed.evidence_id);
}

#[test]
fn test_graphrag_does_not_emit_graph_only_facts_for_missing_sources() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("agent.rs");
    let rag = GraphRag::new(test_graph(&missing));

    assert!(rag.query("agent").unwrap().is_empty());
}
