//! Unit tests for the process-wide evolve graph cache (`evolve::graph_cache`).
//!
//! The cache-hit logic is driven through a local slot (`cached_or_load`) so
//! tests stay deterministic under parallel execution; only the missing-file
//! error path goes through the public `shared_graph_index`.

use super::*;
use crate::evolve::{Edge, EdgeType, Graph, Node};

fn tiny_graph(id: &str) -> Graph {
    Graph {
        nodes: vec![Node::code(id, "src/alpha.rs")],
        edges: vec![],
    }
}

fn save(root: &Path, graph: &Graph) -> PathBuf {
    let path = graph_path(root);
    OntologyStore::new(&path).save(graph).expect("save graph");
    path
}

#[test]
fn missing_graph_errors_honestly_with_build_instructions() {
    let temp = tempfile::tempdir().expect("tempdir");
    let error = shared_graph_index(temp.path()).expect_err("must fail without a graph");
    let message = error.to_string();
    assert!(message.contains("no evolve graph"), "got: {message}");
    assert!(message.contains("selfware self-evolve"), "got: {message}");
}

#[test]
fn unchanged_file_hits_the_cache() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = save(temp.path(), &tiny_graph("crate::alpha"));
    let mut slot = None;
    let first = cached_or_load(&mut slot, &path)
        .expect("first load")
        .clone();
    let second = cached_or_load(&mut slot, &path)
        .expect("second load")
        .clone();
    assert!(
        Arc::ptr_eq(&first, &second),
        "unchanged (mtime, len) must return the cached Arc"
    );
    assert_eq!(
        second.node("crate::alpha").map(|n| n.id.as_str()),
        Some("crate::alpha")
    );
    assert_eq!(second.revision.len(), 12);
}

#[test]
fn rewritten_file_reparses() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = save(temp.path(), &tiny_graph("crate::alpha"));
    let mut slot = None;
    let first = cached_or_load(&mut slot, &path)
        .expect("first load")
        .clone();
    // Different-sized YAML ⇒ different (mtime, len) key ⇒ one re-parse.
    save(temp.path(), &tiny_graph("crate::omega_longer"));
    let second = cached_or_load(&mut slot, &path).expect("reload").clone();
    assert!(
        !Arc::ptr_eq(&first, &second),
        "rewritten graph must not be served from the cache"
    );
    assert!(second.node("crate::omega_longer").is_some());
    assert!(second.node("crate::alpha").is_none());
}

#[test]
fn different_paths_do_not_share_a_slot() {
    let temp_a = tempfile::tempdir().expect("tempdir a");
    let temp_b = tempfile::tempdir().expect("tempdir b");
    let path_a = save(temp_a.path(), &tiny_graph("crate::alpha"));
    let path_b = save(temp_b.path(), &tiny_graph("crate::alpha"));
    let mut slot = None;
    let first = cached_or_load(&mut slot, &path_a).expect("load a").clone();
    let second = cached_or_load(&mut slot, &path_b).expect("load b").clone();
    assert!(
        !Arc::ptr_eq(&first, &second),
        "a slot keyed to path A must not serve path B"
    );
}

#[test]
fn graph_with_private_tool_state_nodes_is_rejected_for_rebuild() {
    for private_dir in [".claude", ".codex", ".agents", ".qwen", ".superpowers"] {
        let temp = tempfile::tempdir().expect("tempdir");
        let private_id = format!("tool::{private_dir}::state::private");
        let graph = Graph {
            nodes: vec![
                Node::code("crate::alpha", "src/alpha.rs"),
                Node::code(&private_id, &format!("{private_dir}/state/private.rs")),
            ],
            edges: vec![Edge {
                from: "crate::alpha".to_string(),
                to: private_id,
                edge_type: EdgeType::DependsOn,
            }],
        };
        let path = save(temp.path(), &graph);
        let mut slot = None;
        let error = cached_or_load(&mut slot, &path).expect_err("stale graph must be rejected");
        let message = error.to_string();
        assert!(message.contains("private tool-state"), "got: {message}");
        assert!(message.contains("selfware self-evolve"), "got: {message}");
        assert!(slot.is_none());
    }
}

#[test]
fn graph_with_private_tool_state_edge_endpoint_is_rejected() {
    let temp = tempfile::tempdir().expect("tempdir");
    let graph = Graph {
        nodes: vec![Node::code("crate::alpha", "src/alpha.rs")],
        edges: vec![Edge {
            from: "crate::alpha".to_string(),
            to: "tool::.codex::state::private".to_string(),
            edge_type: EdgeType::DependsOn,
        }],
    };
    let path = save(temp.path(), &graph);
    let mut slot = None;
    let error = cached_or_load(&mut slot, &path).expect_err("private edge must be rejected");
    assert!(error.to_string().contains("selfware self-evolve"));
    assert!(slot.is_none());
}

#[test]
fn graph_under_explicitly_selected_private_named_root_still_loads() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join(".claude");
    std::fs::create_dir(&root).expect("mkdir selected root");
    let path = save(&root, &tiny_graph("crate::alpha"));
    let mut slot = None;
    let index = cached_or_load(&mut slot, &path).expect("selected root is allowed");
    assert!(index.node("crate::alpha").is_some());
}

#[test]
fn loads_pre_v2_yaml_without_symbol_fields() {
    // A graph file written before schema v2 (no symbol_kind / parent_id /
    // line_range fields, no Symbol layer) must still load: serde defaults
    // fill the new Option fields with None.
    let temp = tempfile::tempdir().expect("tempdir");
    let path = graph_path(temp.path());
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(
        &path,
        "nodes:\n\
         - id: crate::alpha\n\
         \x20 layer: Code\n\
         \x20 path: src/alpha.rs\n\
         \x20 tokens: 10\n\
         \x20 lines: 2\n\
         \x20 files: 1\n\
         \x20 coverage: null\n\
         \x20 dead_code_annotation_ratio: null\n\
         \x20 warning_count: null\n\
         \x20 complexity: null\n\
         edges: []\n",
    )
    .expect("write old-format yaml");

    let mut slot = None;
    let index = cached_or_load(&mut slot, &path)
        .expect("old-format yaml must load")
        .clone();
    let node = index.node("crate::alpha").expect("node present");
    assert_eq!(node.tokens, 10);
    assert!(node.symbol_kind.is_none());
    assert!(node.parent_id.is_none());
    assert!(node.line_range.is_none());
}
