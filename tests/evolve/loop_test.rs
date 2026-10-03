use selfware::evolve::r#loop::EvolutionLoop;
use selfware::evolve::Graph;

#[tokio::test]
async fn test_evolution_loop_reanalyzes_after_action() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(workspace.path().join("src")).unwrap();
    std::fs::write(
        workspace.path().join("src/lib.rs"),
        "pub fn current_workspace_source() {}\n",
    )
    .unwrap();
    std::fs::create_dir_all(workspace.path().join(".claude/worktrees/stale/src")).unwrap();
    std::fs::write(
        workspace
            .path()
            .join(".claude/worktrees/stale/src/private.rs"),
        "pub fn private_worktree_source() {}\n",
    )
    .unwrap();

    let root =
        selfware::tools::workspace_root::WorkspaceRoot::fixed(workspace.path().to_path_buf());
    let result = selfware::tools::workspace_root::scope(root, async {
        let graph = Graph {
            nodes: vec![],
            edges: vec![],
        };
        let mut loop_ = EvolutionLoop::new(graph);
        loop_.run_once().await.unwrap()
    })
    .await;

    assert!(result.reanalyzed);
    assert!(result.updated_nodes > 0, "run_once must re-scan src/");
    // NOTE: `updated_nodes` currently counts EVERY node in the re-scanned
    // graph — run_once does not diff against the previous graph yet. A
    // meaningful "which nodes changed" assertion is pending real
    // incremental analysis in `EvolutionLoop::run_once`.
}
