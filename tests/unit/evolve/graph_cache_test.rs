//! Unit tests for the process-wide evolve graph cache (`evolve::graph_cache`).
//!
//! The cache-hit logic is driven through a local slot (`cached_or_load`) so
//! tests stay deterministic under parallel execution; only the missing-file
//! error path goes through the public `shared_graph_index`.

use super::*;
use crate::evolve::{Graph, Node};

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
