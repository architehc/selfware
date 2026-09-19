use super::*;
use crate::evolution::policy::{
    BreadthFirstPolicy, FixedPopulationPolicy, ParetoAdaptivePolicy, RefineTop1Policy,
};
use crate::evolution::tree_log::{
    compute_sha256, AttemptNode, AttemptStatus, FailureClass, TreeLogError,
};

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
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        action_type: if parent_id.is_none() {
            Some(crate::evolution::ActionType::OpenRoot)
        } else {
            Some(crate::evolution::ActionType::RefineFrontier)
        },
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
    let (disc_tree, val_tree) = tree.split_held_out(0.33).expect("split held out");

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

#[test]
fn test_evaluate_candidates_full_with_stability() {
    let tree1 = build_test_tree();
    let tree2 = build_test_tree();
    let val_tree1 = build_test_tree();
    let val_tree2 = build_test_tree();

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

    let outcome = ReplaySimulator::evaluate_candidates_full(
        &[tree1, tree2],
        &[val_tree1, val_tree2],
        0.50,
        2,
        &candidate_factories,
        0.2,
    )
    .expect("full evaluation with stability");

    assert_eq!(outcome.summaries.len(), 3);
    assert!(outcome.discovery_stability.is_some());
    let disc_stab = outcome.discovery_stability.unwrap();
    assert_eq!(disc_stab.tree_count, 2);
    assert_eq!(disc_stab.policy_count, 3);
    // When trees are identical, concordance is perfect (W = 1.0)
    assert_eq!(disc_stab.kendall_w, 1.0);
    assert!(disc_stab.is_stable);

    assert!(outcome.validation_stability.is_some());
    let val_stab = outcome.validation_stability.unwrap();
    assert_eq!(val_stab.tree_count, 2);
    assert_eq!(val_stab.kendall_w, 1.0);
    assert!(val_stab.is_stable);
}

#[test]
fn test_single_chain_held_out_fails_closed() {
    let mut tree = AttemptTree::new();
    tree.add_node(make_node(
        "root",
        None,
        "single_branch",
        Some(0.5),
        AttemptStatus::Evaluated,
        None,
    ))
    .unwrap();
    tree.add_node(make_node(
        "child",
        Some("root"),
        "single_branch",
        Some(0.6),
        AttemptStatus::Evaluated,
        None,
    ))
    .unwrap();

    // 1. split_held_out rejects single branch
    let split_res = tree.split_held_out(0.5);
    assert!(matches!(
        split_res,
        Err(TreeLogError::InsufficientBranchesForHeldOut(1))
    ));

    // 2. An un-rooted tree with orphaned children fails closed in replay
    let mut orphaned_tree = AttemptTree::new();
    orphaned_tree
        .add_node(make_node(
            "child_orphan",
            Some("nonexistent_parent"),
            "orphan_branch",
            Some(0.6),
            AttemptStatus::Evaluated,
            None,
        ))
        .unwrap();

    let sim = ReplaySimulator::new(orphaned_tree, 0.5);
    let mut policy = BreadthFirstPolicy::new(2);
    let replay_res = sim.evaluate_policy(&mut policy, 0.1);
    assert!(matches!(
        replay_res,
        Err(ReplayError::ValidationSetEmptyOrUnreachable(_))
    ));
}

#[test]
fn test_dependent_attempts_cannot_be_reached_before_parent() {
    let mut tree = AttemptTree::new();
    // Gen 1 attempts (roots)
    tree.add_node(make_node(
        "gen1_winner",
        None,
        "branch_a",
        Some(0.70),
        AttemptStatus::Evaluated,
        None,
    ))
    .unwrap();
    tree.add_node(make_node(
        "gen1_loser",
        None,
        "branch_b",
        Some(0.40),
        AttemptStatus::Evaluated,
        None,
    ))
    .unwrap();

    // Gen 2 attempt depending on gen1_winner
    tree.add_node(make_node(
        "gen2_child",
        Some("gen1_winner"),
        "branch_a",
        Some(0.85),
        AttemptStatus::Evaluated,
        None,
    ))
    .unwrap();

    let sim = ReplaySimulator::new(tree, 0.5);

    // Initial state (step 0): only roots are legal actions
    let mut revealed = std::collections::HashSet::new();
    let actions_init = sim.compute_legal_actions(&revealed);
    assert_eq!(actions_init.len(), 2);
    assert!(actions_init.iter().all(|a| match a {
        crate::evolution::policy::LegalAction::OpenRoot { node_id, .. } => {
            node_id == "gen1_winner" || node_id == "gen1_loser"
        }
        _ => false,
    }));

    // If only gen1_loser is revealed, gen2_child is still NOT legal
    revealed.insert("gen1_loser".to_string());
    let actions_step1 = sim.compute_legal_actions(&revealed);
    assert_eq!(actions_step1.len(), 1);
    assert!(matches!(
        &actions_step1[0],
        crate::evolution::policy::LegalAction::OpenRoot { node_id, .. } if node_id == "gen1_winner"
    ));

    // Only once gen1_winner is revealed does gen2_child become legal as RefineFrontier
    revealed.insert("gen1_winner".to_string());
    let actions_step2 = sim.compute_legal_actions(&revealed);
    assert_eq!(actions_step2.len(), 1);
    assert!(matches!(
        &actions_step2[0],
        crate::evolution::policy::LegalAction::RefineFrontier { node_id, parent_id, .. }
            if node_id == "gen2_child" && parent_id == "gen1_winner"
    ));
}

#[test]
fn test_validation_set_with_reachable_root_and_orphaned_child_fails_closed() {
    let mut tree = AttemptTree::new();
    // Valid reachable root
    tree.add_node(make_node(
        "reachable_root",
        None,
        "branch_a",
        Some(0.70),
        AttemptStatus::Evaluated,
        None,
    ))
    .unwrap();

    // Orphaned child with missing parent
    tree.add_node(make_node(
        "orphaned_child",
        Some("missing_parent_id"),
        "branch_b",
        Some(0.85),
        AttemptStatus::Evaluated,
        None,
    ))
    .unwrap();

    // 1. validate_ancestry must explicitly fail closed
    let val_res = tree.validate_ancestry();
    assert!(matches!(
        val_res,
        Err(TreeLogError::OrphanedNode { ref node_id, ref parent_id })
            if node_id == "orphaned_child" && parent_id == "missing_parent_id"
    ));

    // 2. Replay evaluation must fail closed rather than silently ignoring the orphan
    let sim = ReplaySimulator::new(tree, 0.5);
    let mut policy = BreadthFirstPolicy::new(2);
    let res = sim.evaluate_policy(&mut policy, 0.1);
    assert!(matches!(
        res,
        Err(ReplayError::ValidationSetEmptyOrUnreachable(msg)) if msg.contains("missing_parent_id")
    ));
}

fn dummy_eval(name: &str, obj1: f64, obj2: f64) -> MultiTreeEvaluation {
    let make_rep = |obj: f64| ReplayReport {
        policy_name: name.to_string(),
        beta: 0.1,
        baseline_score: 0.5,
        terminal_score: 0.5,
        score_improvement: 0.0,
        total_probes: 1,
        decision_rounds: 1,
        effective_sequential_rounds: 1,
        parallel_penalty: 1.0,
        total_wall_time_ms: 100,
        total_tokens: 500,
        objective_value: obj,
        pareto_reward: 0.0,
        stop_reason: "done".to_string(),
        revealed_node_ids: vec!["n".to_string()],
    };

    MultiTreeEvaluation {
        policy_name: name.to_string(),
        beta: 0.1,
        tree_count: 2,
        mean_terminal_score: 0.5,
        mean_improvement: 0.0,
        mean_probes: 1.0,
        mean_objective_value: (obj1 + obj2) / 2.0,
        mean_pareto_reward: 0.0,
        cumulative_tokens: 1000,
        cumulative_wall_time_ms: 200,
        per_tree_reports: vec![make_rep(obj1), make_rep(obj2)],
    }
}

#[test]
fn test_ranking_stability_tied_scores_and_renaming_invariance() {
    // Two policies with identical scores on every tree (Tree 1: 0.50, 0.50; Tree 2: 0.50, 0.50)
    let evals_orig = vec![
        dummy_eval("AlphaPolicy", 0.50, 0.50),
        dummy_eval("BetaPolicy", 0.50, 0.50),
    ];

    let stab1 = ReplaySimulator::compute_ranking_stability(&evals_orig).expect("stability");
    // Tied scores must NOT manufacture a "stable" W = 1.0 ranking; W must be 0.0!
    assert_eq!(
        stab1.kendall_w, 0.0,
        "Tied scores across all trees must evaluate to W = 0.0 (no evidence of preference)"
    );
    assert!(!stab1.is_stable);

    // Renaming AlphaPolicy to ZetaPolicy (swapping alphabetical tie-break order) must NOT change W
    let evals_renamed = vec![
        dummy_eval("ZetaPolicy", 0.50, 0.50),
        dummy_eval("BetaPolicy", 0.50, 0.50),
    ];
    let stab2 = ReplaySimulator::compute_ranking_stability(&evals_renamed).expect("stability");
    assert_eq!(
        stab2.kendall_w, 0.0,
        "Policy renaming must leave Kendall's W invariant at 0.0"
    );
}

#[test]
fn test_higher_mean_score_with_volatile_rankings_blocks_promotion() {
    let outcome = ReplayValidationOutcome {
        summaries: vec![
            PolicyValidationSummary {
                policy_name: "VolatileWinner".to_string(),
                discovery_probes: 2,
                discovery_tokens: 500,
                discovery_objective: 0.90,
                validation_terminal_score: 0.85,
                validation_objective: 0.85,
                beats_incumbent: true,
            },
            PolicyValidationSummary {
                policy_name: "FixedPopulation (Incumbent)".to_string(),
                discovery_probes: 2,
                discovery_tokens: 500,
                discovery_objective: 0.70,
                validation_terminal_score: 0.70,
                validation_objective: 0.70,
                beats_incumbent: false,
            },
        ],
        discovery_stability: None,
        validation_stability: Some(RankingStability {
            kendall_w: 0.35, // Volatile! Below 0.70
            is_stable: false,
            tree_count: 3,
            policy_count: 2,
            per_tree_rankings: vec![],
        }),
        beta: 0.2,
    };

    let readiness = outcome.promotion_readiness();
    assert!(
        matches!(
            readiness,
            PromotionReadiness::BlockedByInstability {
                ref winner_name,
                kendall_w,
                ..
            } if winner_name == "VolatileWinner" && (kendall_w - 0.35).abs() < 1e-6
        ),
        "Promotion must be explicitly blocked when rankings are volatile (W < 0.70)"
    );
}

#[test]
fn test_ranking_stability_degenerate_counts_not_falsely_green() {
    // Single tree evaluation: concordance cannot be established across 1 tree (Rule 3)
    let single_tree_evals = vec![
        MultiTreeEvaluation {
            policy_name: "P1".into(),
            beta: 0.1,
            tree_count: 1,
            mean_terminal_score: 0.8,
            mean_improvement: 0.0,
            mean_probes: 1.0,
            mean_objective_value: 0.8,
            mean_pareto_reward: 0.0,
            cumulative_tokens: 100,
            cumulative_wall_time_ms: 10,
            per_tree_reports: vec![dummy_eval("P1", 0.8, 0.8).per_tree_reports[0].clone()],
        },
        MultiTreeEvaluation {
            policy_name: "P2".into(),
            beta: 0.1,
            tree_count: 1,
            mean_objective_value: 0.5,
            mean_terminal_score: 0.5,
            mean_improvement: 0.0,
            mean_probes: 1.0,
            mean_pareto_reward: 0.0,
            cumulative_tokens: 100,
            cumulative_wall_time_ms: 10,
            per_tree_reports: vec![dummy_eval("P2", 0.5, 0.5).per_tree_reports[0].clone()],
        },
    ];
    let stab = ReplaySimulator::compute_ranking_stability(&single_tree_evals).expect("stability");
    assert_eq!(
        stab.kendall_w, 0.0,
        "Kendall's W must be 0.0 for tree_count < 2"
    );
    assert!(
        !stab.is_stable,
        "is_stable must be false for tree_count < 2"
    );

    // Single policy evaluation: cannot rank a single policy against itself (Rule 3)
    let single_policy_evals = vec![dummy_eval("OnlyPolicy", 0.9, 0.85)];
    let stab_single_p =
        ReplaySimulator::compute_ranking_stability(&single_policy_evals).expect("stability");
    assert_eq!(
        stab_single_p.kendall_w, 0.0,
        "Kendall's W must be 0.0 for policy_count < 2"
    );
    assert!(
        !stab_single_p.is_stable,
        "is_stable must be false for policy_count < 2"
    );
}

#[test]
fn test_replay_daemon_tree_shape_with_baseline_root_and_control_anchors() {
    let mut tree = AttemptTree::new();

    // att-baseline is the sole root in daemon-generated attempt trees
    let baseline_node = AttemptNode {
        id: "att-baseline".to_string(),
        parent_id: None,
        generation: 0,
        branch_id: "baseline".to_string(),
        hypothesis_id: "baseline".to_string(),
        description: "Initial baseline capability measurement".to_string(),
        diff_sha256: "sha_base".to_string(),
        patch: None,
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.75),
        tokens_used: Some(500),
        wall_time_ms: 1000,
        status: AttemptStatus::Baseline,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        action_type: None,
        created_at: "2026-09-17T00:00:00Z".to_string(),
    };
    tree.add_node(baseline_node).unwrap();

    // Control anchor node from generation 1
    let ctrl_node = AttemptNode {
        id: "att-g1-control".to_string(),
        parent_id: Some("att-baseline".to_string()),
        generation: 1,
        branch_id: "control".to_string(),
        hypothesis_id: "control".to_string(),
        description: "Unpatched control anchor".to_string(),
        diff_sha256: "sha_ctrl".to_string(),
        patch: None,
        sab_report_path: None,
        metrics: None,
        composite_score: None,
        tokens_used: None,
        wall_time_ms: 500,
        status: AttemptStatus::InternalError,
        failure_class: Some(FailureClass::EnvironmentError),
        failure_reason: Some("Control clean check".to_string()),
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        action_type: None,
        created_at: "2026-09-17T00:01:00Z".to_string(),
    };
    tree.add_node(ctrl_node).unwrap();

    // Actual mutation attempts beneath att-baseline
    let hyp1_node = AttemptNode {
        id: "att-g1-hyp-1".to_string(),
        parent_id: Some("att-baseline".to_string()),
        generation: 1,
        branch_id: "hyp-1".to_string(),
        hypothesis_id: "hyp-1".to_string(),
        description: "Hypothesis 1".to_string(),
        diff_sha256: "sha_hyp1".to_string(),
        patch: Some("diff1".to_string()),
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.85),
        tokens_used: Some(1000),
        wall_time_ms: 2000,
        status: AttemptStatus::Evaluated,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        action_type: Some(crate::evolution::ActionType::OpenRoot),
        created_at: "2026-09-17T00:02:00Z".to_string(),
    };
    tree.add_node(hyp1_node).unwrap();

    let hyp2_node = AttemptNode {
        id: "att-g1-hyp-2".to_string(),
        parent_id: Some("att-baseline".to_string()),
        generation: 1,
        branch_id: "hyp-2".to_string(),
        hypothesis_id: "hyp-2".to_string(),
        description: "Hypothesis 2".to_string(),
        diff_sha256: "sha_hyp2".to_string(),
        patch: Some("diff2".to_string()),
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.80),
        tokens_used: Some(1000),
        wall_time_ms: 2000,
        status: AttemptStatus::Evaluated,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        action_type: Some(crate::evolution::ActionType::OpenRoot),
        created_at: "2026-09-17T00:03:00Z".to_string(),
    };
    tree.add_node(hyp2_node).unwrap();

    // FixedPopulationPolicy with population size 2
    let mut policy = FixedPopulationPolicy::new(2);

    // Baseline score 0.0 passed (as CLI does by default); simulator should pick up 0.75 from att-baseline
    let sim = ReplaySimulator::new(tree, 0.0);
    let report = sim
        .evaluate_policy(&mut policy, 0.1)
        .expect("replay report");

    // 1. Baseline node must NOT be counted as a probe
    assert_eq!(
        report.total_probes, 2,
        "probes must only count actual mutations, not att-baseline"
    );
    assert!(report
        .revealed_node_ids
        .contains(&"att-g1-hyp-1".to_string()));
    assert!(report
        .revealed_node_ids
        .contains(&"att-g1-hyp-2".to_string()));
    assert!(
        !report
            .revealed_node_ids
            .contains(&"att-g1-control".to_string()),
        "control anchor must be excluded"
    );
    assert!(
        !report
            .revealed_node_ids
            .contains(&"att-baseline".to_string()),
        "baseline must not be in probe sequence"
    );

    // 2. Baseline score must be resolved from att-baseline
    assert_eq!(report.baseline_score, 0.75);
    assert_eq!(report.terminal_score, 0.85);
    assert!((report.score_improvement - 0.10).abs() < 1e-9);
}

#[test]
fn test_replay_retains_open_root_for_promoted_descendant() {
    let mut tree = AttemptTree::new();

    let baseline = AttemptNode {
        id: "att-baseline".to_string(),
        parent_id: None,
        generation: 0,
        branch_id: "baseline".to_string(),
        hypothesis_id: "baseline".to_string(),
        description: "Baseline".to_string(),
        diff_sha256: "0".to_string(),
        patch: None,
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.50),
        tokens_used: None,
        wall_time_ms: 0,
        status: AttemptStatus::Baseline,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        action_type: None,
        created_at: "2026-09-17T00:00:00Z".to_string(),
    };
    tree.add_node(baseline).unwrap();

    // Promoted attempt from baseline
    let cand1 = AttemptNode {
        id: "att-cand1".to_string(),
        parent_id: Some("att-baseline".to_string()),
        generation: 0,
        branch_id: "branch-0".to_string(),
        hypothesis_id: "hyp-0".to_string(),
        description: "Candidate 1 (Gen 0 winner)".to_string(),
        diff_sha256: "sha1".to_string(),
        patch: Some("diff1".to_string()),
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.80),
        tokens_used: Some(1000),
        wall_time_ms: 100,
        status: AttemptStatus::Evaluated,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: Some("c1-commit".to_string()),
        action_type: Some(crate::evolution::ActionType::OpenRoot),
        created_at: "2026-09-17T00:01:00Z".to_string(),
    };
    tree.add_node(cand1).unwrap();

    // Gen 1 OpenRoot action exploring from the promoted candidate att-cand1
    let gen1_open_root = AttemptNode {
        id: "att-gen1-root".to_string(),
        parent_id: Some("att-cand1".to_string()),
        generation: 1,
        branch_id: "branch-1".to_string(),
        hypothesis_id: "hyp-1".to_string(),
        description: "Gen 1 OpenRoot exploring from incumbent".to_string(),
        diff_sha256: "sha2".to_string(),
        patch: Some("diff2".to_string()),
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.85),
        tokens_used: Some(1000),
        wall_time_ms: 100,
        status: AttemptStatus::Evaluated,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        action_type: Some(crate::evolution::ActionType::OpenRoot),
        created_at: "2026-09-17T00:02:00Z".to_string(),
    };
    tree.add_node(gen1_open_root).unwrap();

    // Gen 1 RefineFrontier action refining the same parent att-cand1
    let gen1_refine = AttemptNode {
        id: "att-gen1-refine".to_string(),
        parent_id: Some("att-cand1".to_string()),
        generation: 1,
        branch_id: "branch-0".to_string(),
        hypothesis_id: "hyp-refine".to_string(),
        description: "Gen 1 RefineFrontier refining incumbent".to_string(),
        diff_sha256: "sha3".to_string(),
        patch: Some("diff3".to_string()),
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.82),
        tokens_used: Some(1000),
        wall_time_ms: 100,
        status: AttemptStatus::Evaluated,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        action_type: Some(crate::evolution::ActionType::RefineFrontier),
        created_at: "2026-09-17T00:03:00Z".to_string(),
    };
    tree.add_node(gen1_refine).unwrap();

    let sim = ReplaySimulator::new(tree, 0.50);
    let mut revealed = std::collections::HashSet::new();
    revealed.insert("att-baseline".to_string());
    revealed.insert("att-cand1".to_string());

    let legal = sim.compute_legal_actions(&revealed);

    // Check that att-gen1-root is classified as OpenRoot, NOT RefineFrontier
    let root_action = legal
        .iter()
        .find(|a| a.node_id() == "att-gen1-root")
        .expect("att-gen1-root must be legal");
    assert!(
        matches!(root_action, LegalAction::OpenRoot { branch_id, node_id } if branch_id == "branch-1" && node_id == "att-gen1-root"),
        "att-gen1-root must be LegalAction::OpenRoot, got {:?}",
        root_action
    );

    // Check that att-gen1-refine is classified as RefineFrontier
    let refine_action = legal
        .iter()
        .find(|a| a.node_id() == "att-gen1-refine")
        .expect("att-gen1-refine must be legal");
    assert!(
        matches!(refine_action, LegalAction::RefineFrontier { branch_id, parent_id, node_id } if branch_id == "branch-0" && parent_id == "att-cand1" && node_id == "att-gen1-refine"),
        "att-gen1-refine must be LegalAction::RefineFrontier, got {:?}",
        refine_action
    );
}
