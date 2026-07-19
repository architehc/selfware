use selfware::evolve::dedup::{jaccard, DeduplicationAnalyzer, DuplicateKind, DuplicatePair};
use selfware::evolve::{Graph, Node};
use std::collections::HashSet;

#[test]
fn test_dedup_finds_no_duplicates_in_clean_repo() {
    let graph = Graph {
        nodes: vec![],
        edges: vec![],
    };
    let dedup = DeduplicationAnalyzer::new();
    let dupes = dedup.find_duplicates(&graph).unwrap();
    assert!(dupes.is_empty());
}

#[test]
fn test_dedup_finds_exact_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.rs");
    let b = dir.path().join("b.rs");
    let c = dir.path().join("c.rs");
    std::fs::write(&a, "fn hello() { println!(\"hi\"); }\n").unwrap();
    std::fs::write(&b, "fn hello() { println!(\"hi\"); }\n").unwrap();
    std::fs::write(&c, "fn world() { println!(\"bye\"); }\n").unwrap();

    let graph = Graph {
        nodes: vec![
            Node::code("a", a.to_str().unwrap()),
            Node::code("b", b.to_str().unwrap()),
            Node::code("c", c.to_str().unwrap()),
        ],
        edges: vec![],
    };
    let dedup = DeduplicationAnalyzer::new();
    let dupes = dedup.find_duplicates(&graph).unwrap();
    assert_eq!(
        dupes,
        vec![DuplicatePair {
            first: "a".to_string(),
            second: "b".to_string(),
            kind: DuplicateKind::Exact,
        }]
    );
}
#[test]
fn test_dedup_finds_near_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let body: String = (0..20).map(|i| format!("let x{i} = {i};\n")).collect();
    let mut near = body.clone();
    near.push_str("let extra = 1;\n");

    let a = dir.path().join("a.rs");
    let b = dir.path().join("b.rs");
    std::fs::write(&a, &body).unwrap();
    std::fs::write(&b, &near).unwrap();

    let graph = Graph {
        nodes: vec![
            Node::code("a", a.to_str().unwrap()),
            Node::code("b", b.to_str().unwrap()),
        ],
        edges: vec![],
    };
    let dedup = DeduplicationAnalyzer::new();
    let dupes = dedup.find_duplicates(&graph).unwrap();
    assert_eq!(
        dupes,
        vec![DuplicatePair {
            first: "a".to_string(),
            second: "b".to_string(),
            kind: DuplicateKind::Near,
        }]
    );
}

#[test]
fn test_dedup_ignores_unreadable_paths() {
    let graph = Graph {
        nodes: vec![
            Node::code("missing1", "/nonexistent/path/one"),
            Node::code("missing2", "/nonexistent/path/two"),
        ],
        edges: vec![],
    };
    let dedup = DeduplicationAnalyzer::new();
    let dupes = dedup.find_duplicates(&graph).unwrap();
    assert!(dupes.is_empty());
}

#[test]
fn test_dedup_skips_directories_without_rs_files() {
    let dir = tempfile::tempdir().unwrap();
    let empty_a = dir.path().join("empty_a");
    let empty_b = dir.path().join("empty_b");
    std::fs::create_dir(&empty_a).unwrap();
    std::fs::create_dir(&empty_b).unwrap();

    let graph = Graph {
        nodes: vec![
            Node::code("empty_a", empty_a.to_str().unwrap()),
            Node::code("empty_b", empty_b.to_str().unwrap()),
        ],
        edges: vec![],
    };
    let dedup = DeduplicationAnalyzer::new();
    let dupes = dedup.find_duplicates(&graph).unwrap();
    assert!(dupes.is_empty());
}

#[test]
fn test_jaccard_returns_zero_for_two_empty_sets() {
    let a = HashSet::new();
    let b = HashSet::new();
    assert_eq!(jaccard(&a, &b), 0.0);
}
