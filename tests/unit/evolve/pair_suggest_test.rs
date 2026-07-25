//! Unit tests for level-1 pair discovery and context assembly.

use super::*;
use crate::evolve::{Edge, EdgeType, Graph, Node};

fn code(id: &str, path: &str) -> Node {
    Node::code(id, path)
}

fn edge(from: &str, to: &str, kind: EdgeType) -> Edge {
    Edge {
        from: from.to_string(),
        to: to.to_string(),
        edge_type: kind,
    }
}

#[test]
fn connected_pairs_are_undirected_deduped_and_component_level() {
    let graph = Graph {
        nodes: vec![
            code("crate::agent::execution", "src/agent/execution.rs"),
            code("crate::tools::shell", "src/tools/shell.rs"),
            code("crate::agent::mod", "src/agent/mod.rs"),
        ],
        edges: vec![
            edge(
                "crate::agent::execution",
                "crate::tools::shell",
                EdgeType::DependsOn,
            ),
            // Reverse direction between the same components — must dedup, not double.
            edge(
                "crate::tools::shell",
                "crate::agent::mod",
                EdgeType::DependsOn,
            ),
            // Intra-component edge — must be excluded (agent <-> agent).
            edge(
                "crate::agent::execution",
                "crate::agent::mod",
                EdgeType::DependsOn,
            ),
        ],
    };
    let pairs = connected_pairs(&graph);
    assert_eq!(
        pairs.len(),
        1,
        "agent<->tools collapses to one undirected pair"
    );
    let p = &pairs[0];
    assert_eq!((p.a.as_str(), p.b.as_str()), ("agent", "tools"));
    assert_eq!(p.weight, 2, "two node-edges collapse into the pair");
    assert!(p.relations.contains(&"DependsOn".to_string()));
}

#[test]
fn structural_and_context_edges_are_not_evolution_connections() {
    let graph = Graph {
        nodes: vec![
            code("crate::agent::execution", "src/agent/execution.rs"),
            code("crate::tools::shell", "src/tools/shell.rs"),
        ],
        edges: vec![
            edge(
                "crate::agent::execution",
                "crate::tools::shell",
                EdgeType::Contains,
            ),
            edge(
                "crate::agent::execution",
                "crate::tools::shell",
                EdgeType::ContextIncluded,
            ),
        ],
    };
    assert!(connected_pairs(&graph).is_empty());
}

#[test]
fn cross_cluster_pairs_sort_first() {
    let graph = Graph {
        nodes: vec![
            code("crate::agent::a", "src/agent/a.rs"),
            code("crate::orchestration::b", "src/orchestration/b.rs"),
            code("crate::tools::c", "src/tools/c.rs"),
        ],
        edges: vec![
            // Same cluster (Loop Core): agent <-> orchestration.
            edge(
                "crate::agent::a",
                "crate::orchestration::b",
                EdgeType::DependsOn,
            ),
            // Cross cluster: agent (Loop Core) <-> tools (Action).
            edge("crate::agent::a", "crate::tools::c", EdgeType::DependsOn),
        ],
    };
    let pairs = connected_pairs(&graph);
    assert_eq!(pairs.len(), 2);
    assert!(pairs[0].cross_cluster, "cross-cluster pair ranks first");
    assert_eq!(
        (pairs[0].a.as_str(), pairs[0].b.as_str()),
        ("agent", "tools")
    );
}

#[test]
fn pair_context_names_both_components_and_relationship() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("agent")).unwrap();
    std::fs::create_dir_all(dir.path().join("tools")).unwrap();
    std::fs::write(
        dir.path().join("agent/execution.rs"),
        "//! Agent execution.\npub fn run() {}\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("tools/shell.rs"),
        "//! Shell tool.\npub struct Shell;\n",
    )
    .unwrap();

    let graph = Graph {
        nodes: vec![
            code("crate::agent::execution", "agent/execution.rs"),
            code("crate::tools::shell", "tools/shell.rs"),
        ],
        edges: vec![edge(
            "crate::agent::execution",
            "crate::tools::shell",
            EdgeType::DependsOn,
        )],
    };
    let pairs = connected_pairs(&graph);
    let ctx = pair_context(&graph, dir.path(), &pairs[0]);
    assert!(ctx.contains("Component pair: agent <-> tools"));
    assert!(ctx.contains("DependsOn"));
    assert!(ctx.contains("Agent execution."));
    assert!(ctx.contains("Shell tool."));
    assert!(ctx.contains("fns: run"));
    assert!(ctx.contains("types: Shell"));
}
