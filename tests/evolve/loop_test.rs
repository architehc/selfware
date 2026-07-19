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
    assert!(result.reanalyzed);
}
