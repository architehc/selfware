//! Unit tests for the read-only graph index (`evolve::graph_index`).
//!
//! The fixture is a small in-memory graph — never the real
//! `.selfware/evolve-graph.yaml` (110K lines; parsing it in a unit test
//! would be slow and brittle).

use super::*;
use crate::evolve::{Edge, Node};

fn node(id: &str, path: &str, tokens: usize, lines: usize, complexity: Option<f64>) -> Node {
    let mut node = Node::code(id, path);
    node.tokens = tokens;
    node.lines = lines;
    node.complexity = complexity;
    node
}

/// alpha ← beta ← beta::inner, alpha ← gamma, alpha Contains alpha_test,
/// gamma SimilarTo beta.
fn fixture() -> GraphIndex {
    let mut test_node = Node::test("tests::alpha_test", "tests/alpha_test.rs");
    test_node.tokens = 40;
    test_node.lines = 20;
    let graph = Graph {
        nodes: vec![
            node("crate::alpha", "src/alpha.rs", 100, 50, Some(10.0)),
            node("crate::beta", "src/beta.rs", 300, 100, Some(30.0)),
            node("crate::beta::inner", "src/beta/inner.rs", 50, 20, None),
            node("crate::gamma", "src/gamma.rs", 200, 80, None),
            test_node,
        ],
        edges: vec![
            Edge {
                from: "crate::beta".into(),
                to: "crate::alpha".into(),
                edge_type: EdgeType::DependsOn,
            },
            Edge {
                from: "crate::gamma".into(),
                to: "crate::alpha".into(),
                edge_type: EdgeType::DependsOn,
            },
            Edge {
                from: "crate::beta::inner".into(),
                to: "crate::beta".into(),
                edge_type: EdgeType::DependsOn,
            },
            Edge {
                from: "crate::alpha".into(),
                to: "tests::alpha_test".into(),
                edge_type: EdgeType::Contains,
            },
            Edge {
                from: "crate::gamma".into(),
                to: "crate::beta".into(),
                edge_type: EdgeType::SimilarTo,
            },
        ],
    };
    GraphIndex::from_graph(Arc::new(graph), &"ab".repeat(32))
}

#[test]
fn revision_is_truncated_to_12_hex_chars() {
    let index = fixture();
    assert_eq!(index.revision, "abababababab");
    assert_eq!(index.revision.len(), 12);
}

#[test]
fn node_lookup_hits_and_misses() {
    let index = fixture();
    assert_eq!(index.node("crate::alpha").map(|n| n.tokens), Some(100));
    assert!(index.node("crate::nope").is_none());
}

#[test]
fn dependents_and_dependencies_follow_depends_on_direction() {
    let index = fixture();
    assert_eq!(
        index.dependents("crate::alpha"),
        &["crate::beta".to_string(), "crate::gamma".to_string()]
    );
    assert_eq!(
        index.dependencies("crate::beta"),
        &["crate::alpha".to_string()]
    );
    assert!(index.dependents("crate::nope").is_empty());
    assert!(index.dependencies("crate::alpha").is_empty());
}

#[test]
fn neighbors_reports_both_directions_and_filters_by_kind() {
    let index = fixture();
    let all = index.neighbors("crate::beta", None);
    assert_eq!(
        all,
        vec![
            ("crate::alpha".to_string(), EdgeType::DependsOn),
            ("crate::beta::inner".to_string(), EdgeType::DependsOn),
            ("crate::gamma".to_string(), EdgeType::SimilarTo),
        ]
    );
    let similar = index.neighbors("crate::beta", Some(EdgeType::SimilarTo));
    assert_eq!(
        similar,
        vec![("crate::gamma".to_string(), EdgeType::SimilarTo)]
    );
}

#[test]
fn impact_closure_is_depth_limited_bfs_over_dependents() {
    let index = fixture();
    assert_eq!(
        index.impact_closure("crate::alpha", 1),
        vec!["crate::beta".to_string(), "crate::gamma".to_string()]
    );
    assert_eq!(
        index.impact_closure("crate::alpha", 2),
        vec![
            "crate::beta".to_string(),
            "crate::gamma".to_string(),
            "crate::beta::inner".to_string()
        ]
    );
    assert!(index.impact_closure("crate::alpha", 0).is_empty());
    // Deep caps just exhaust the reachable set.
    assert_eq!(
        index.impact_closure("crate::beta", 5),
        vec!["crate::beta::inner".to_string()]
    );
}

#[test]
fn tests_for_returns_only_test_layer_contains_children() {
    let index = fixture();
    assert_eq!(
        index.tests_for("crate::alpha"),
        vec!["tests::alpha_test".to_string()]
    );
    assert!(index.tests_for("crate::beta").is_empty());
}

#[test]
fn hotspots_rank_by_metric_with_deterministic_ties() {
    let index = fixture();
    let by_tokens: Vec<&str> = index
        .hotspots(Metric::Tokens, Some(NodeLayer::Code), None, 2)
        .iter()
        .map(|n| n.id.as_str())
        .collect();
    assert_eq!(by_tokens, vec!["crate::beta", "crate::gamma"]);

    // density: beta::inner 50/20 = 2.5, gamma 200/80 = 2.5 (tokens-per-line
    // fallback), beta 30/100 = 0.3, alpha 10/50 = 0.2 — id breaks the tie.
    let by_density: Vec<&str> = index
        .hotspots(Metric::Density, Some(NodeLayer::Code), None, 4)
        .iter()
        .map(|n| n.id.as_str())
        .collect();
    assert_eq!(
        by_density,
        vec![
            "crate::beta::inner",
            "crate::gamma",
            "crate::beta",
            "crate::alpha"
        ]
    );

    let by_complexity = index.hotspots(Metric::Complexity, None, None, 1);
    assert_eq!(by_complexity[0].id, "crate::beta");

    // Layer filter: the only Test node wins any ranking.
    let tests = index.hotspots(Metric::Tokens, Some(NodeLayer::Test), None, 5);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].id, "tests::alpha_test");
}

#[test]
fn hotspots_exclude_prefix_filters_before_ranking() {
    let index = fixture();
    // Excluding by id prefix removes beta and beta::inner from the candidates,
    // so gamma (200 tokens) becomes the top row even at k=2.
    let by_tokens: Vec<&str> = index
        .hotspots(
            Metric::Tokens,
            Some(NodeLayer::Code),
            Some("crate::beta"),
            2,
        )
        .iter()
        .map(|n| n.id.as_str())
        .collect();
    assert_eq!(by_tokens, vec!["crate::gamma", "crate::alpha"]);

    // Excluding by path prefix works too.
    let by_path: Vec<&str> = index
        .hotspots(Metric::Tokens, None, Some("src/beta"), 10)
        .iter()
        .map(|n| n.id.as_str())
        .collect();
    assert!(!by_path.contains(&"crate::beta"));
    assert!(!by_path.contains(&"crate::beta::inner"));
    assert!(by_path.contains(&"crate::alpha"));
}

#[test]
fn lexical_match_scores_by_term_hits() {
    let index = fixture();
    let matched = index.lexical_match(&["alpha".to_string()]);
    let ids: Vec<&str> = matched.iter().map(|(n, _)| n.id.as_str()).collect();
    assert_eq!(ids, vec!["crate::alpha", "tests::alpha_test"]);
    assert!(matched.iter().all(|(_, hits)| *hits == 1));
    assert!(index.lexical_match(&["zzz".to_string()]).is_empty());
}

#[test]
fn nearest_matches_suggests_close_ids() {
    let index = fixture();
    let suggestions = index.nearest_matches("crate::alpa", 3);
    assert_eq!(
        suggestions.first().map(String::as_str),
        Some("crate::alpha")
    );
}

#[test]
fn split_terms_and_lexical_hits_follow_the_graphrag_rule() {
    assert_eq!(
        split_terms("Fix the tool_dispatch bug!"),
        vec![
            "fix".to_string(),
            "the".to_string(),
            "tool".to_string(),
            "dispatch".to_string(),
            "bug".to_string()
        ]
    );
    // Terms shorter than 3 chars are dropped.
    assert!(split_terms("a to zz").is_empty());
    let alpha = node("crate::alpha", "src/alpha.rs", 0, 0, None);
    let terms = split_terms("alpha beta");
    assert_eq!(lexical_hits(&alpha, &terms), 1);
}

#[test]
fn by_component_groups_under_top_level_modules() {
    let index = fixture();
    let beta_members = &index.by_component()["beta"];
    assert_eq!(
        beta_members,
        &vec!["crate::beta".to_string(), "crate::beta::inner".to_string()]
    );
}

#[test]
fn directed_neighbors_filter_by_kind_and_direction() {
    let index = fixture();
    // beta: out DependsOn→alpha, in DependsOn←beta::inner, in SimilarTo←gamma.
    let all = index.directed_neighbors("crate::beta", None, Direction::Both);
    assert_eq!(
        all,
        vec![
            (
                "crate::alpha".to_string(),
                EdgeType::DependsOn,
                Direction::Out
            ),
            (
                "crate::beta::inner".to_string(),
                EdgeType::DependsOn,
                Direction::In
            ),
            (
                "crate::gamma".to_string(),
                EdgeType::SimilarTo,
                Direction::In
            ),
        ]
    );

    let incoming = index.directed_neighbors("crate::beta", None, Direction::In);
    assert_eq!(incoming.len(), 2);
    assert!(incoming.iter().all(|(_, _, d)| *d == Direction::In));

    let outgoing = index.directed_neighbors("crate::beta", None, Direction::Out);
    assert_eq!(
        outgoing,
        vec![(
            "crate::alpha".to_string(),
            EdgeType::DependsOn,
            Direction::Out
        )]
    );

    let similar_in =
        index.directed_neighbors("crate::beta", Some(&EdgeType::SimilarTo), Direction::In);
    assert_eq!(
        similar_in,
        vec![(
            "crate::gamma".to_string(),
            EdgeType::SimilarTo,
            Direction::In
        )]
    );
    assert!(index
        .directed_neighbors("crate::beta", Some(&EdgeType::SimilarTo), Direction::Out)
        .is_empty());
}

#[test]
fn impact_frontier_carries_depth_and_via() {
    let index = fixture();
    // alpha ← beta ← beta::inner, alpha ← gamma.
    let frontier = index.impact_frontier("crate::alpha", 2);
    assert_eq!(
        frontier,
        vec![
            ("crate::beta".to_string(), 1, "crate::alpha".to_string()),
            ("crate::gamma".to_string(), 1, "crate::alpha".to_string()),
            (
                "crate::beta::inner".to_string(),
                2,
                "crate::beta".to_string()
            ),
        ]
    );
    // impact_closure stays a thin projection of the frontier.
    assert_eq!(
        index.impact_closure("crate::alpha", 2),
        frontier
            .iter()
            .map(|(id, _, _)| id.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn edge_type_names_and_kind_parsing_round_trip() {
    assert_eq!(edge_type_name(&EdgeType::DependsOn), "depends_on");
    assert_eq!(edge_type_name(&EdgeType::Contains), "contains");
    assert_eq!(edge_type_name(&EdgeType::DuplicateOf), "duplicate_of");
    assert_eq!(edge_type_name(&EdgeType::SimilarTo), "similar_to");
    for name in ["depends_on", "contains", "duplicate_of", "similar_to"] {
        let kind = parse_edge_kind(name).expect("known kind");
        assert_eq!(edge_type_name(&kind), name);
    }
    assert!(parse_edge_kind("all").is_none());
    assert!(parse_edge_kind("vibes").is_none());
    assert_eq!(Direction::parse("in"), Some(Direction::In));
    assert_eq!(Direction::parse("out"), Some(Direction::Out));
    assert_eq!(Direction::parse("both"), Some(Direction::Both));
    assert_eq!(Direction::parse("sideways"), None);
}

#[test]
fn dependency_cycles_finds_a_three_node_cycle() {
    // a → b → c → a, plus an acyclic e → d → a tail feeding the cycle.
    let graph = Graph {
        nodes: vec![
            node("crate::a", "src/a.rs", 100, 10, None),
            node("crate::b", "src/b.rs", 200, 20, None),
            node("crate::c", "src/c.rs", 300, 30, None),
            node("crate::d", "src/d.rs", 50, 5, None),
            node("crate::e", "src/e.rs", 60, 6, None),
        ],
        edges: vec![
            Edge {
                from: "crate::a".into(),
                to: "crate::b".into(),
                edge_type: EdgeType::DependsOn,
            },
            Edge {
                from: "crate::b".into(),
                to: "crate::c".into(),
                edge_type: EdgeType::DependsOn,
            },
            Edge {
                from: "crate::c".into(),
                to: "crate::a".into(),
                edge_type: EdgeType::DependsOn,
            },
            Edge {
                from: "crate::d".into(),
                to: "crate::a".into(),
                edge_type: EdgeType::DependsOn,
            },
            Edge {
                from: "crate::e".into(),
                to: "crate::d".into(),
                edge_type: EdgeType::DependsOn,
            },
        ],
    };
    let index = GraphIndex::from_graph(Arc::new(graph), &"cd".repeat(32));
    let cycles = index.dependency_cycles(20);
    assert_eq!(cycles.len(), 1, "exactly one cycle: {cycles:?}");
    let cycle = &cycles[0];
    assert_eq!(
        cycle.first(),
        cycle.last(),
        "cycle path must close on itself: {cycle:?}"
    );
    let members: std::collections::HashSet<&str> = cycle.iter().map(String::as_str).collect();
    assert_eq!(members.len(), 3);
    for member in ["crate::a", "crate::b", "crate::c"] {
        assert!(members.contains(member), "missing {member}: {cycle:?}");
    }
    // d and e feed the cycle but are not part of it.
    assert!(!members.contains("crate::d"));
    assert!(!members.contains("crate::e"));
}

#[test]
fn dependency_cycles_empty_when_acyclic() {
    let index = fixture();
    assert!(
        index.dependency_cycles(20).is_empty(),
        "the acyclic fixture must report no cycles"
    );
}

#[test]
fn dependency_cycles_reports_self_loops_and_respects_k() {
    let graph = Graph {
        nodes: vec![
            node("crate::selfish", "src/selfish.rs", 10, 1, None),
            node("crate::x", "src/x.rs", 10, 1, None),
            node("crate::y", "src/y.rs", 10, 1, None),
        ],
        edges: vec![
            Edge {
                from: "crate::selfish".into(),
                to: "crate::selfish".into(),
                edge_type: EdgeType::DependsOn,
            },
            Edge {
                from: "crate::x".into(),
                to: "crate::y".into(),
                edge_type: EdgeType::DependsOn,
            },
            Edge {
                from: "crate::y".into(),
                to: "crate::x".into(),
                edge_type: EdgeType::DependsOn,
            },
        ],
    };
    let index = GraphIndex::from_graph(Arc::new(graph), &"ef".repeat(32));
    let cycles = index.dependency_cycles(20);
    assert_eq!(cycles.len(), 2, "self-loop + pair cycle: {cycles:?}");
    assert!(
        cycles
            .iter()
            .any(|c| c == &vec!["crate::selfish".to_string(), "crate::selfish".to_string()]),
        "self-loop must surface: {cycles:?}"
    );
    // k caps the number of reported cycles.
    assert_eq!(index.dependency_cycles(1).len(), 1);
}

#[test]
fn hotspots_exclude_symbol_nodes_unless_requested() {
    let mut graph_nodes: Vec<Node> = vec![
        node("crate::alpha", "src/alpha.rs", 100, 10, None),
        node("crate::beta", "src/beta.rs", 300, 30, None),
    ];
    graph_nodes.push(Node::symbol(
        "crate::beta::worker",
        "crate::beta",
        "fn",
        "src/beta.rs",
        (1, 5),
        500,
    ));
    let graph = Graph {
        nodes: graph_nodes,
        edges: vec![],
    };
    let index = GraphIndex::from_graph(Arc::new(graph), &"ab".repeat(32));

    let default_layers: Vec<NodeLayer> = index
        .hotspots(Metric::Tokens, None, None, 10)
        .iter()
        .map(|n| n.layer)
        .collect();
    assert!(
        !default_layers.contains(&NodeLayer::Symbol),
        "the default all-layers view stays file-level: {default_layers:?}"
    );

    let symbols = index.hotspots(Metric::Tokens, Some(NodeLayer::Symbol), None, 10);
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].id, "crate::beta::worker");
}
