use super::*;
use crate::evolution::policy::{
    BreadthFirstPolicy, FixedPopulationPolicy, ParetoAdaptivePolicy, RefineTop1Policy,
};
use crate::evolution::tree_log::{compute_sha256, AttemptNode, AttemptStatus, FailureClass};

fn make_node(
    id: &str,
    parent_id: Option<&str>,
    branch_id: &str,
    score: Option<f64>,
    status: AttemptStatus,
    failure_class: Option<FailureClass>,
) -> AttemptNode {
    AttemptNode {
        id: id.to_string(),
        parent_id: parent_id.map(|s| s.to_string()),
        generation: 1,
        branch_id: branch_id.to_string(),
        hypothesis_id: format!("hyp-{}", id),
        description: format!("Desc {}", id),
        diff_sha256: compute_sha256(id.as_bytes()),
        patch: None,
        sab_report_path: None,
        metrics: None,
        composite_score: score,
        tokens_used: Some(1000),
        wall_time_ms: 5000,
        status,
        failure_class,
        failure_reason: None,
        binary_sha256: None,
        created_at: "2026-09-16T12:00:00Z".into(),
    }
}

fn build_test_tree() -> AttemptTree {
    let mut tree = AttemptTree::new();

    // Branch A: root 0.55 -> child 0.60
    tree.add_node(make_node(
        "a0",
        None,
        "A",
        Some(0.55),
        AttemptStatus::Evaluated,
        None,
    ))
    .unwrap();
    tree.add_node(make_node(
        "a1",
        Some("a0"),
        "A",
        Some(0.60),
        AttemptStatus::Evaluated,
        None,
    ))
    .unwrap();

    // Branch B: root 0.70 -> child 0.85 (the optimal branch)
    tree.add_node(make_node(
        "b0",
        None,
        "B",
        Some(0.70),
        AttemptStatus::Evaluated,
        None,
    ))
    .unwrap();
    tree.add_node(make_node(
        "b1",
        Some("b0"),
        "B",
        Some(0.85),
        AttemptStatus::Evaluated,
        None,
    ))
    .unwrap();

    // Branch C: root 0.40 -> unpromising
    tree.add_node(make_node(
        "c0",
        None,
        "C",
        Some(0.40),
        AttemptStatus::Evaluated,
        None,
    ))
    .unwrap();

    tree
}

#[test]
fn test_replay_breadth_first_execution() {
    let tree = build_test_tree();
    let sim = ReplaySimulator::new(tree, 0.50).with_max_parallelism(2);
    let mut policy = BreadthFirstPolicy::new(2);

    let report = sim
        .evaluate_policy(&mut policy, 0.1)
        .expect("replay evaluation");

    assert_eq!(report.terminal_score, 0.85);
    assert_eq!(report.score_improvement, 0.35);
    assert!(report.total_probes >= 3);
    assert!(report.decision_rounds >= 2);
    assert!(report.revealed_node_ids.contains(&"b1".to_string()));
}

#[test]
fn test_replay_refine_top1_focuses_on_winner() {
    let tree = build_test_tree();
    let sim = ReplaySimulator::new(tree, 0.50).with_max_parallelism(3);
    let mut policy = RefineTop1Policy::new(2);

    let report = sim
        .evaluate_policy(&mut policy, 0.1)
        .expect("replay evaluation");

    assert_eq!(report.terminal_score, 0.85);
    // RefineTop1 should explore roots, then only refine branch B, skipping branch A's child!
    assert!(report.revealed_node_ids.contains(&"b1".to_string()));
    assert!(!report.revealed_node_ids.contains(&"a1".to_string()));
}

#[test]
fn test_pareto_adaptive_recovers_repairable_branch() {
    let mut tree = AttemptTree::new();

    // Branch A: root 0.65 -> compile failure (syntax) -> fix gets 0.92!
    tree.add_node(make_node(
        "a0",
        None,
        "A",
        Some(0.65),
        AttemptStatus::Evaluated,
        None,
    ))
    .unwrap();
    tree.add_node(make_node(
        "a1",
        Some("a0"),
        "A",
        None,
        AttemptStatus::CompileFailed,
        Some(FailureClass::RepairableSyntax),
    ))
    .unwrap();
    tree.add_node(make_node(
        "a2",
        Some("a1"),
        "A",
        Some(0.92),
        AttemptStatus::Evaluated,
        None,
    ))
    .unwrap();

    // Branch B: root 0.60 -> child 0.62
    tree.add_node(make_node(
        "b0",
        None,
        "B",
        Some(0.60),
        AttemptStatus::Evaluated,
        None,
    ))
    .unwrap();
    tree.add_node(make_node(
        "b1",
        Some("b0"),
        "B",
        Some(0.62),
        AttemptStatus::Evaluated,
        None,
    ))
    .unwrap();

    let sim = ReplaySimulator::new(tree, 0.50).with_max_parallelism(2);
    let mut policy = ParetoAdaptivePolicy::new();

    let report = sim
        .evaluate_policy(&mut policy, 0.5)
        .expect("replay evaluation");

    // ParetoAdaptivePolicy must not abandon branch A after a1's syntax error:
    // it should allocate a recovery probe and reach a2 (0.92)!
    assert_eq!(report.terminal_score, 0.92);
    assert!(report.revealed_node_ids.contains(&"a2".to_string()));
}

#[test]
fn test_sweep_beta() {
    let tree = build_test_tree();
    let sim = ReplaySimulator::new(tree, 0.50);

    let betas = vec![0.1, 0.5, 0.9];
    let reports = sim
        .sweep_beta(|| Box::new(BreadthFirstPolicy::new(2)), &betas)
        .expect("sweep beta");

    assert_eq!(reports.len(), 3);
    assert_eq!(reports[0].beta, 0.1);
    assert_eq!(reports[1].beta, 0.5);
    assert_eq!(reports[2].beta, 0.9);
    // Higher beta penalizes probes more, lowering objective value
    assert!(reports[0].objective_value >= reports[2].objective_value);
}

#[test]
fn test_compare_policies_ranks_by_objective() {
    let tree = build_test_tree();
    let sim = ReplaySimulator::new(tree, 0.50).with_max_parallelism(3);

    let mut policies: Vec<Box<dyn SearchPolicy>> = vec![
        Box::new(BreadthFirstPolicy::new(2)),
        Box::new(RefineTop1Policy::new(2)),
        Box::new(ParetoAdaptivePolicy::new()),
    ];

    let rankings = sim
        .compare_policies(&mut policies, 0.2)
        .expect("compare policies");
    assert_eq!(rankings.len(), 3);
    // Verify sorted descending
    assert!(rankings[0].objective_value >= rankings[1].objective_value);
    assert!(rankings[1].objective_value >= rankings[2].objective_value);
}

#[test]
fn test_batch_exceeds_parallelism_error() {
    #[derive(Debug)]
    struct GreedyOverParallelPolicy;
    impl SearchPolicy for GreedyOverParallelPolicy {
        fn name(&self) -> &str {
            "OverParallel"
        }
        fn decide(&mut self, _p: &PrefixView, legal: &[LegalAction], _beta: f64) -> PolicyDecision {
            PolicyDecision::SelectBatch(legal.to_vec()) // returns all legal actions at once!
        }
    }

    let tree = build_test_tree(); // has 3 roots
    let sim = ReplaySimulator::new(tree, 0.50).with_max_parallelism(2); // capacity is only 2
    let mut policy = GreedyOverParallelPolicy;

    let res = sim.evaluate_policy(&mut policy, 0.1);
    assert!(matches!(
        res,
        Err(ReplayError::BatchExceedsParallelism { size: 3, max: 2 })
    ));
}

#[test]
fn test_evaluate_across_multiple_trees() {
    let tree1 = build_test_tree();
    let tree2 = build_test_tree();
    let trees = vec![tree1, tree2];

    let eval = ReplaySimulator::evaluate_policy_across_trees(
        &trees,
        0.50,
        2,
        &|| Box::new(BreadthFirstPolicy::new(2)),
        0.2,
    )
    .expect("multi-tree evaluation");

    assert_eq!(eval.tree_count, 2);
    assert_eq!(eval.per_tree_reports.len(), 2);
    assert!(eval.mean_terminal_score >= 0.80);
    assert!(eval.cumulative_tokens > 0);
}

#[test]
fn test_evaluate_candidates_with_held_out_validation() {
    let tree = build_test_tree();
    let (disc_tree, val_tree) = tree.split_held_out(0.33);

    let candidate_factories: Vec<(&'static str, SearchPolicyFactory)> = vec![
        (
            "FixedPopulation (Incumbent)",
            Box::new(|| Box::new(FixedPopulationPolicy::new(2))),
        ),
        (
            "RefineTop1Policy",
            Box::new(|| Box::new(RefineTop1Policy::new(2))),
        ),
        (
            "ParetoAdaptivePolicy",
            Box::new(|| Box::new(ParetoAdaptivePolicy::new())),
        ),
    ];

    let summaries = ReplaySimulator::evaluate_candidates_with_validation(
        &[disc_tree],
        &[val_tree],
        0.50,
        2,
        &candidate_factories,
        0.2,
    )
    .expect("validation comparison");

    assert_eq!(summaries.len(), 3);
    // Verification: ranked descending by validation objective
    assert!(summaries[0].validation_objective >= summaries[1].validation_objective);
    assert!(summaries[1].validation_objective >= summaries[2].validation_objective);
}
