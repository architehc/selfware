//! Unit tests for the logical capability model.

use super::*;
use crate::evolve::{Edge, EdgeType, Graph, Node};

fn code(id: &str) -> Node {
    Node::code(id, &format!("src/{}.rs", id.replace("::", "/")))
}

#[test]
fn every_seed_capability_is_present_with_invariants() {
    let model = build_logical_model(&Graph::default(), std::path::Path::new("."));
    assert_eq!(model.capabilities.len(), 9);
    for cap in &model.capabilities {
        assert!(!cap.purpose.is_empty(), "{} needs a purpose", cap.id);
        assert!(!cap.invariants.is_empty(), "{} needs invariants", cap.id);
        assert!(!cap.clusters.is_empty(), "{} needs clusters", cap.id);
    }
    // The task loop is the spine.
    let task = model.capabilities.iter().find(|c| c.id == "task_loop").unwrap();
    assert_eq!(task.name, "Task Loop");
    assert!(task.invariants.iter().any(|i| i.contains("budget")));
}

#[test]
fn modules_and_tokens_derive_from_the_graph() {
    let mut g = Graph::default();
    let mut agent = code("crate::agent::execution");
    agent.tokens = 1000;
    let mut tools = code("crate::tools::shell");
    tools.tokens = 500;
    g.nodes = vec![agent, tools];

    let model = build_logical_model(&g, std::path::Path::new("."));
    // agent -> Loop Core -> task_loop ; tools -> Action -> tool_dispatch
    let task = model.capabilities.iter().find(|c| c.id == "task_loop").unwrap();
    assert!(task.modules.contains(&"agent".to_string()));
    assert_eq!(task.tokens, 1000);
    let dispatch = model.capabilities.iter().find(|c| c.id == "tool_dispatch").unwrap();
    assert!(dispatch.modules.contains(&"tools".to_string()));
    assert_eq!(dispatch.tokens, 500);
}

#[test]
fn dependency_edges_collapse_to_capability_level() {
    let mut g = Graph::default();
    g.nodes = vec![code("crate::agent::execution"), code("crate::tools::shell")];
    // agent (task_loop) depends on tools (tool_dispatch)
    g.edges = vec![Edge {
        from: "crate::agent::execution".to_string(),
        to: "crate::tools::shell".to_string(),
        edge_type: EdgeType::DependsOn,
    }];
    let model = build_logical_model(&g, std::path::Path::new("."));
    assert!(model
        .edges
        .iter()
        .any(|e| e.from == "task_loop" && e.to == "tool_dispatch"));
    let task = model.capabilities.iter().find(|c| c.id == "task_loop").unwrap();
    assert!(task.depends_on.contains(&"tool_dispatch".to_string()));
}
