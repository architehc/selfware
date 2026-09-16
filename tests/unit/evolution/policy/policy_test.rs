use super::*;

fn make_obs(
    id: &str,
    branch_id: &str,
    depth: usize,
    parent_id: Option<&str>,
    score: Option<f64>,
    status: AttemptStatus,
    failure_class: Option<FailureClass>,
) -> PrefixObservation {
    PrefixObservation {
        id: id.to_string(),
        branch_id: branch_id.to_string(),
        attempt_depth: depth,
        parent_id: parent_id.map(|s| s.to_string()),
        score,
        status,
        failure_class,
        failure_reason: None,
        delta_vs_baseline: score.map(|s| s - 0.5),
        delta_vs_parent: None,
        tokens_used: Some(1000),
        wall_time_ms: 10000,
    }
}

#[test]
fn test_branch_trajectory_computation() {
    let obs = vec![
        make_obs(
            "o1",
            "b1",
            0,
            None,
            Some(0.6),
            AttemptStatus::Evaluated,
            None,
        ),
        make_obs(
            "o2",
            "b1",
            1,
            Some("o1"),
            None,
            AttemptStatus::CompileFailed,
            Some(FailureClass::RepairableSyntax),
        ),
    ];

    let traj = BranchTrajectory::from_observations("b1".to_string(), obs);
    assert_eq!(traj.depth(), 2);
    assert_eq!(traj.successful_anchor, Some(0.6));
    assert_eq!(traj.consecutive_failures, 1);
    assert!(traj.has_repairable_frontier);
    assert!(!traj.is_hard_failed);
}

#[test]
fn test_breadth_first_policy_exploration() {
    let mut policy = BreadthFirstPolicy::new(2);
    let prefix = PrefixView::new(Vec::new(), 0.5, 2);

    let actions = vec![
        LegalAction::OpenRoot {
            branch_id: "b1".into(),
            node_id: "n1".into(),
        },
        LegalAction::OpenRoot {
            branch_id: "b2".into(),
            node_id: "n2".into(),
        },
        LegalAction::OpenRoot {
            branch_id: "b3".into(),
            node_id: "n3".into(),
        },
    ];

    let dec = policy.decide(&prefix, &actions, 0.5);
    match dec {
        PolicyDecision::SelectBatch(batch) => {
            assert_eq!(batch.len(), 2);
            assert_eq!(batch[0].branch_id(), "b1");
            assert_eq!(batch[1].branch_id(), "b2");
        }
        _ => panic!("Expected SelectBatch"),
    }
}

#[test]
fn test_refine_top1_policy_selection() {
    let mut policy = RefineTop1Policy::new(2);

    // Initial state: 2 roots revealed, b2 is best
    let obs = vec![
        make_obs(
            "r1",
            "b1",
            0,
            None,
            Some(0.55),
            AttemptStatus::Evaluated,
            None,
        ),
        make_obs(
            "r2",
            "b2",
            0,
            None,
            Some(0.75),
            AttemptStatus::Evaluated,
            None,
        ),
    ];
    let prefix = PrefixView::new(obs, 0.5, 2);

    let actions = vec![
        LegalAction::RefineFrontier {
            branch_id: "b1".into(),
            parent_id: "r1".into(),
            node_id: "c1".into(),
        },
        LegalAction::RefineFrontier {
            branch_id: "b2".into(),
            parent_id: "r2".into(),
            node_id: "c2".into(),
        },
    ];

    let dec = policy.decide(&prefix, &actions, 0.5);
    match dec {
        PolicyDecision::SelectBatch(batch) => {
            assert_eq!(batch.len(), 1);
            assert_eq!(batch[0].branch_id(), "b2");
            assert_eq!(batch[0].node_id(), "c2");
        }
        _ => panic!("Expected SelectBatch refining b2"),
    }
}

#[test]
fn test_pareto_adaptive_recovery_anti_over_closing() {
    let mut policy = ParetoAdaptivePolicy::new();

    // b1 had a good anchor (0.7) then a repairable syntax error
    // b2 is an active branch with anchor 0.65
    let obs = vec![
        make_obs(
            "r1",
            "b1",
            0,
            None,
            Some(0.70),
            AttemptStatus::Evaluated,
            None,
        ),
        make_obs(
            "f1",
            "b1",
            1,
            Some("r1"),
            None,
            AttemptStatus::CompileFailed,
            Some(FailureClass::RepairableSyntax),
        ),
        make_obs(
            "r2",
            "b2",
            0,
            None,
            Some(0.65),
            AttemptStatus::Evaluated,
            None,
        ),
    ];
    let prefix = PrefixView::new(obs, 0.5, 2);

    let actions = vec![
        LegalAction::RefineFrontier {
            branch_id: "b1".into(),
            parent_id: "f1".into(),
            node_id: "fix1".into(),
        },
        LegalAction::RefineFrontier {
            branch_id: "b2".into(),
            parent_id: "r2".into(),
            node_id: "ref2".into(),
        },
    ];

    let dec = policy.decide(&prefix, &actions, 0.5);
    match dec {
        PolicyDecision::SelectBatch(batch) => {
            // Batch should contain both: recovery for b1 and exploit for b2
            assert_eq!(batch.len(), 2);
            let bids: HashSet<&str> = batch.iter().map(|a| a.branch_id()).collect();
            assert!(bids.contains("b1"), "Recovery for b1 must be included");
            assert!(bids.contains("b2"), "Exploit for b2 must be included");
        }
        _ => panic!("Expected SelectBatch"),
    }
}

#[test]
fn test_early_stop_plateau() {
    let inner = Box::new(BreadthFirstPolicy::new(5));
    let mut policy = EarlyStopPlateauPolicy::new(inner, 1, 0.01);

    // Initial obs with score 0.6
    let obs = vec![make_obs(
        "o1",
        "b1",
        0,
        None,
        Some(0.60),
        AttemptStatus::Evaluated,
        None,
    )];
    let prefix = PrefixView::new(obs, 0.5, 2);
    let actions = vec![LegalAction::OpenRoot {
        branch_id: "b2".into(),
        node_id: "n2".into(),
    }];

    // Round 1: normal decision
    let dec1 = policy.decide(&prefix, &actions, 0.5);
    assert!(matches!(dec1, PolicyDecision::SelectBatch(_)));

    // Round 2 without improvement triggers stop
    let dec2 = policy.decide(&prefix, &actions, 0.5);
    match dec2 {
        PolicyDecision::Stop { reason } => assert!(reason.contains("Terminating early")),
        _ => panic!("Expected early stop"),
    }
}
