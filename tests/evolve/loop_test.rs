use selfware::evolve::r#loop::EvolutionLoop;
use selfware::evolve::Graph;

#[tokio::test]
async fn test_evolution_loop_reanalyzes_after_action() {
    let graph = Graph {
        nodes: vec![],
        edges: vec![],
    };
    let mut loop_ = EvolutionLoop::new(graph);
    let result = loop_.run_once().await.unwrap();
    // Honest minimal contract: run_once returns Ok and re-scans the `src/`
    // tree relative to the cwd (the crate root under `cargo test`), which
    // contains real .rs files — so a non-zero node count proves the re-scan
    // actually happened and the (empty) input graph was replaced.
    assert!(result.reanalyzed);
    assert!(
        result.updated_nodes > 0,
        "run_once must re-scan src/ and report the nodes it found"
    );
    // NOTE: `updated_nodes` currently counts EVERY node in the re-scanned
    // graph — run_once does not diff against the previous graph yet. A
    // meaningful "which nodes changed" assertion is pending real
    // incremental analysis in `EvolutionLoop::run_once`.
}
