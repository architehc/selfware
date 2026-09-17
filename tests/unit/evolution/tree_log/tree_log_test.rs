use super::*;
use tempfile::tempdir;

fn sample_metrics(sab_score: f64) -> FitnessMetrics {
    FitnessMetrics {
        sab_score,
        tokens_used: Some(1200),
        token_budget: 16384,
        wall_clock_secs: 14.2,
        timeout_secs: 60.0,
        full_evaluation_secs: Some(18.5),
        test_pass_pct: 88.5,
        binary_size_mb: 12.4,
        max_binary_size_mb: 50.0,
        tests_passed: 120,
        tests_total: 120,
        visual_score: 0.0,
    }
}

fn sample_node(
    id: &str,
    parent_id: Option<&str>,
    branch_id: &str,
    score: Option<f64>,
    status: AttemptStatus,
) -> AttemptNode {
    AttemptNode {
        id: id.to_string(),
        parent_id: parent_id.map(|s| s.to_string()),
        generation: 1,
        branch_id: branch_id.to_string(),
        hypothesis_id: format!("hyp-{}", id),
        description: format!("Mutation {}", id),
        diff_sha256: compute_sha256(id.as_bytes()),
        patch: Some(format!("diff --git a/src/lib.rs b/src/lib.rs\n// {}", id)),
        sab_report_path: None,
        metrics: score.map(sample_metrics),
        composite_score: score,
        tokens_used: Some(1200),
        wall_time_ms: 14200,
        status,
        failure_class: if status == AttemptStatus::Evaluated {
            None
        } else {
            Some(FailureClass::RepairableSyntax)
        },
        failure_reason: if status == AttemptStatus::Evaluated {
            None
        } else {
            Some("expected semicolon".into())
        },
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        created_at: "2026-09-16T12:00:00Z".to_string(),
    }
}

#[test]
fn test_tree_construction_and_queries() {
    let mut tree = AttemptTree::new();

    let root_a = sample_node(
        "node-1",
        None,
        "branch-A",
        Some(0.65),
        AttemptStatus::Evaluated,
    );
    let child_a1 = sample_node(
        "node-2",
        Some("node-1"),
        "branch-A",
        Some(0.72),
        AttemptStatus::Evaluated,
    );
    let root_b = sample_node(
        "node-3",
        None,
        "branch-B",
        Some(0.50),
        AttemptStatus::Evaluated,
    );

    tree.add_node(root_a).expect("add root_a");
    tree.add_node(child_a1).expect("add child_a1");
    tree.add_node(root_b).expect("add root_b");

    assert_eq!(tree.len(), 3);
    assert!(!tree.is_empty());

    // Check roots
    let roots = tree.roots();
    assert_eq!(roots.len(), 2);
    let root_ids: Vec<&str> = roots.iter().map(|n| n.id.as_str()).collect();
    assert!(root_ids.contains(&"node-1"));
    assert!(root_ids.contains(&"node-3"));

    // Check children
    let children_1 = tree.children("node-1");
    assert_eq!(children_1.len(), 1);
    assert_eq!(children_1[0].id, "node-2");

    let children_2 = tree.children("node-2");
    assert!(children_2.is_empty());

    // Check branches
    let branches = tree.branches();
    assert_eq!(branches, vec!["branch-A", "branch-B"]);

    let nodes_a = tree.branch_nodes("branch-A");
    assert_eq!(nodes_a.len(), 2);

    // Check best node
    let best = tree.best_node().expect("best node");
    assert_eq!(best.id, "node-2");
    assert_eq!(best.composite_score, Some(0.72));

    let best_b = tree.best_in_branch("branch-B").expect("best in branch B");
    assert_eq!(best_b.id, "node-3");
    assert_eq!(best_b.composite_score, Some(0.50));
}

#[test]
fn test_duplicate_id_rejected() {
    let mut tree = AttemptTree::new();
    let node1 = sample_node("dup-1", None, "b1", Some(0.5), AttemptStatus::Evaluated);
    let node2 = sample_node("dup-1", None, "b2", Some(0.8), AttemptStatus::Evaluated);

    tree.add_node(node1).unwrap();
    let err = tree.add_node(node2).unwrap_err();
    match err {
        TreeLogError::DuplicateId(id) => assert_eq!(id, "dup-1"),
        other => panic!("Unexpected error: {:?}", other),
    }
}

#[test]
fn test_save_and_load_jsonl() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("attempts.jsonl");

    let mut tree = AttemptTree::new();
    tree.add_node(sample_node(
        "n1",
        None,
        "b1",
        Some(0.55),
        AttemptStatus::Evaluated,
    ))
    .unwrap();
    tree.add_node(sample_node(
        "n2",
        Some("n1"),
        "b1",
        None,
        AttemptStatus::CompileFailed,
    ))
    .unwrap();

    tree.save_to_jsonl(&file_path).unwrap();
    assert!(file_path.exists());

    let loaded = AttemptTree::load_from_jsonl(&file_path).unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded.get("n1").unwrap().composite_score, Some(0.55));
    assert_eq!(
        loaded.get("n2").unwrap().status,
        AttemptStatus::CompileFailed
    );
    assert_eq!(loaded.children("n1").len(), 1);
}

#[test]
fn test_append_node_to_jsonl() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("attempts_append.jsonl");

    let n1 = sample_node("app-1", None, "b1", Some(0.60), AttemptStatus::Evaluated);
    let n2 = sample_node(
        "app-2",
        Some("app-1"),
        "b1",
        Some(0.75),
        AttemptStatus::Evaluated,
    );

    AttemptTree::append_node_to_jsonl(&file_path, &n1).unwrap();
    AttemptTree::append_node_to_jsonl(&file_path, &n2).unwrap();

    let loaded = AttemptTree::load_from_jsonl(&file_path).unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded.get("app-2").unwrap().composite_score, Some(0.75));
}

#[test]
fn test_failure_class_repairability() {
    assert!(FailureClass::RepairableSyntax.is_repairable());
    assert!(FailureClass::RepairableTypeError.is_repairable());
    assert!(FailureClass::RepairableTestFailure.is_repairable());
    assert!(FailureClass::RepairableClippy.is_repairable());

    assert!(!FailureClass::UnrecoverableResource.is_repairable());
    assert!(!FailureClass::SafetyViolation.is_repairable());
    assert!(!FailureClass::EnvironmentError.is_repairable());
    assert!(!FailureClass::Unclassified.is_repairable());
}

#[test]
fn test_compute_sha256() {
    let hash = compute_sha256(b"hello world");
    assert_eq!(
        hash,
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
}

#[test]
fn test_corrupt_line_handling() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("corrupt.jsonl");

    std::fs::write(&file_path, "not a valid json\n").unwrap();
    let res = AttemptTree::load_from_jsonl(&file_path);
    assert!(res.is_err());
    match res.unwrap_err() {
        TreeLogError::CorruptLine(msg) => assert!(msg.contains("line 1")),
        other => panic!("Unexpected error: {:?}", other),
    }
}

#[test]
fn test_tail_lines_preserves_trailing_lines() {
    let empty = "";
    assert_eq!(tail_lines(empty, 5), "");

    let short = "line1\nline2\nline3";
    assert_eq!(tail_lines(short, 5), short);
    assert_eq!(tail_lines(short, 2), "line2\nline3");
    assert_eq!(tail_lines(short, 1), "line3");

    let exact = "a\nb\nc";
    assert_eq!(tail_lines(exact, 3), exact);

    let many: String = (0..100).map(|i| format!("line {i}\n")).collect();
    let tail = tail_lines(&many, 5);
    let tail_lines_vec: Vec<&str> = tail.lines().collect();
    assert_eq!(tail_lines_vec.len(), 5);
    assert_eq!(tail_lines_vec[0], "line 95");
    assert_eq!(tail_lines_vec[4], "line 99");
}

#[test]
fn test_output_tail_serde_backwards_compatible() {
    // Older attempt node JSON without output_tail should deserialize with None
    let json_legacy = r#"{
        "id": "att-legacy",
        "parent_id": null,
        "generation": 1,
        "branch_id": "b1",
        "hypothesis_id": "h1",
        "description": "legacy attempt",
        "diff_sha256": "abcdef",
        "wall_time_ms": 100,
        "status": "internal_error",
        "created_at": "2026-09-17T00:00:00Z"
    }"#;
    let node: AttemptNode = serde_json::from_str(json_legacy).expect("deserialize legacy node");
    assert!(node.output_tail.is_none());

    // New node with output_tail roundtrips
    let mut with_tail = node.clone();
    with_tail.output_tail = Some("cargo test error: timeout".to_string());
    let serialized = serde_json::to_string(&with_tail).expect("serialize with tail");
    let roundtripped: AttemptNode =
        serde_json::from_str(&serialized).expect("deserialize with tail");
    assert_eq!(
        roundtripped.output_tail.as_deref(),
        Some("cargo test error: timeout")
    );
}

#[test]
fn test_ancestry_groups_partitions_connected_components() {
    let mut tree = AttemptTree::new();
    // Tree A: root-a -> child-a1 -> child-a2
    tree.add_node(sample_node(
        "root-a",
        None,
        "branch-a",
        Some(50.0),
        AttemptStatus::Evaluated,
    ))
    .unwrap();
    tree.add_node(sample_node(
        "child-a1",
        Some("root-a"),
        "branch-a",
        Some(55.0),
        AttemptStatus::Evaluated,
    ))
    .unwrap();
    tree.add_node(sample_node(
        "child-a2",
        Some("child-a1"),
        "branch-a",
        Some(60.0),
        AttemptStatus::Evaluated,
    ))
    .unwrap();

    // Tree B: root-b -> child-b1
    tree.add_node(sample_node(
        "root-b",
        None,
        "branch-b",
        Some(40.0),
        AttemptStatus::Evaluated,
    ))
    .unwrap();
    tree.add_node(sample_node(
        "child-b1",
        Some("root-b"),
        "branch-b",
        Some(45.0),
        AttemptStatus::Evaluated,
    ))
    .unwrap();

    let groups = tree.ancestry_groups();
    assert_eq!(
        groups.len(),
        2,
        "Must identify exactly 2 connected components"
    );

    let group_a = groups
        .iter()
        .find(|g| g.contains(&"root-a".to_string()))
        .unwrap();
    assert!(group_a.contains(&"child-a1".to_string()));
    assert!(group_a.contains(&"child-a2".to_string()));
    assert_eq!(group_a.len(), 3);

    let group_b = groups
        .iter()
        .find(|g| g.contains(&"root-b".to_string()))
        .unwrap();
    assert!(group_b.contains(&"child-b1".to_string()));
    assert_eq!(group_b.len(), 2);
}

#[test]
fn test_split_held_out_excludes_control_anchors() {
    let mut tree = AttemptTree::new();
    // Common baseline root
    tree.add_node(sample_node(
        "baseline",
        None,
        "baseline",
        Some(50.0),
        AttemptStatus::Baseline,
    ))
    .unwrap();
    // Exploratory branch 1
    tree.add_node(sample_node(
        "hyp0",
        Some("baseline"),
        "branch-g1-hyp0",
        Some(60.0),
        AttemptStatus::Evaluated,
    ))
    .unwrap();
    // Exploratory branch 2
    tree.add_node(sample_node(
        "hyp1",
        Some("baseline"),
        "branch-g1-hyp1",
        Some(70.0),
        AttemptStatus::Evaluated,
    ))
    .unwrap();
    // Control anchor branch
    tree.add_node(sample_node(
        "ctrl",
        Some("baseline"),
        "control",
        Some(50.0),
        AttemptStatus::Evaluated,
    ))
    .unwrap();

    let (disc, val) = tree.split_held_out(0.40).expect("split must succeed");

    // Neither partition must contain the control anchor
    assert!(
        !disc.has_node("ctrl"),
        "Discovery tree must not contain control anchor"
    );
    assert!(
        !val.has_node("ctrl"),
        "Validation tree must not contain control anchor"
    );

    // Both partitions must contain the common baseline root
    assert!(disc.has_node("baseline"));
    assert!(val.has_node("baseline"));

    // Validation tree must contain an exploratory branch, not be empty or control-only
    let val_exploratory = val
        .nodes
        .iter()
        .any(|n| n.branch_id == "branch-g1-hyp0" || n.branch_id == "branch-g1-hyp1");
    assert!(
        val_exploratory,
        "Validation tree must contain an exploratory branch"
    );
}
