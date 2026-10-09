use super::*;
use crate::evolve::{Edge, EdgeType};
use crate::orchestration::primitives::types::MAX_ARTIFACTS;

fn digest(byte: char) -> String {
    std::iter::repeat_n(byte, 64).collect()
}

fn assignment(key: &str, node: &str, path: &str, hash: char) -> PrimitiveAssignment {
    PrimitiveAssignment {
        key: key.to_string(),
        graph_node_id: Some(node.to_string()),
        path: path.to_string(),
        classification: "rust_source".to_string(),
        base_sha256: digest(hash),
        focus_symbol: None,
        reasons: vec!["test".to_string()],
        read_attestation: ReadAttestationSummary {
            attestation_id: format!("read-{key}"),
            exact_snapshot_read: true,
            write: false,
            execute: false,
            network: false,
        },
    }
}

fn plan(assignments: Vec<PrimitiveAssignment>) -> ActivationPlan {
    ActivationPlan {
        run_id: "run-test".to_string(),
        task_sha256: digest('1'),
        selection_mode: "test".to_string(),
        max_rounds: 3,
        graph: GraphStatus::Missing,
        binding: RepositoryBinding {
            repository_id: digest('2'),
            head_tree: None,
            graph_sha256: None,
            activation_census_sha256: digest('3'),
            activation_census_complete: true,
            activation_ranking_complete: true,
            working_set_sha256: working_set_digest(&assignments),
            plan_sha256: digest('4'),
        },
        assignments,
        allowed_routes: BTreeSet::new(),
        warnings: Vec::new(),
    }
}

fn permissive_safety() -> SafetyConfig {
    SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        ..SafetyConfig::default()
    }
}

fn write_graph(root: &Path, graph: &Graph) {
    std::fs::create_dir_all(root.join(".selfware")).unwrap();
    std::fs::write(
        root.join(".selfware/evolve-graph.yaml"),
        serde_yaml::to_string(graph).unwrap(),
    )
    .unwrap();
}

#[test]
fn routes_are_derived_from_the_active_set_instead_of_graph_edges() {
    let graph = Graph {
        nodes: vec![
            Node::code("file-a", "src/a.rs"),
            Node::code("file-b", "src/b.rs"),
            Node::symbol(
                "file-a::stable_interface",
                "file-a",
                "function",
                "src/a.rs",
                (5, 9),
                12,
            ),
        ],
        edges: vec![Edge {
            from: "file-a::stable_interface".to_string(),
            to: "file-b".to_string(),
            edge_type: EdgeType::DependsOn,
        }],
    };
    let index = GraphIndex::from_graph(Arc::new(graph), &digest('f'));
    assert_eq!(
        owning_file_node(&index, "file-a::stable_interface").map(|node| node.id.as_str()),
        Some("file-a")
    );

    let assignments = vec![
        assignment("artifact-001", "file-a", "src/a.rs", 'a'),
        assignment("artifact-002", "file-b", "src/b.rs", 'b'),
    ];
    let routes = trusted_routes_for_assignments(&assignments, 3);
    assert_eq!(
        routes,
        BTreeSet::from([
            AllowedRoute {
                from: "artifact-001".to_string(),
                to: "artifact-002".to_string(),
            },
            AllowedRoute {
                from: "artifact-002".to_string(),
                to: "artifact-001".to_string(),
            },
        ])
    );
    assert!(trusted_routes_for_assignments(&assignments, 1).is_empty());

    let graph_without_edges = GraphIndex::from_graph(
        Arc::new(Graph {
            nodes: index.graph.nodes.clone(),
            edges: Vec::new(),
        }),
        &digest('e'),
    );
    assert!(graph_without_edges.graph.edges.is_empty());
    assert_eq!(
        trusted_routes_for_assignments(&assignments, 3),
        routes,
        "advisory edges must not grant or remove communication authority"
    );
}

#[test]
fn oversized_localizer_focus_symbol_is_dropped_before_plan_construction() {
    let localized = vec![crate::tools::localize_issue::Candidate {
        file: "src/lib.rs".to_string(),
        function: "x".repeat(super::super::types::MAX_FOCUS_SYMBOL_BYTES + 1),
        score: 1.0,
        reason: "test".to_string(),
    }];
    let eligible = BTreeSet::from(["src/lib.rs".to_string()]);

    let ordered = candidate_order(
        "inspect x",
        Path::new("/repo"),
        &localized,
        &eligible,
        None,
        1,
    );

    assert_eq!(ordered.len(), 1);
    assert_eq!(ordered[0].path, "src/lib.rs");
    assert!(ordered[0].focus_symbol.is_none());
}

#[test]
fn binding_digests_change_when_an_artifact_binding_changes() {
    let original = vec![assignment("artifact-001", "file-a", "src/a.rs", 'a')];
    let mut changed = original.clone();
    changed[0].base_sha256 = digest('b');

    assert_ne!(working_set_digest(&original), working_set_digest(&changed));
    let original_plan = plan(original);
    let changed_plan = plan(changed);
    assert_ne!(
        plan_digest(&original_plan).unwrap(),
        plan_digest(&changed_plan).unwrap()
    );

    let mut changed_census = original_plan.clone();
    changed_census.binding.activation_census_sha256 = digest('9');
    assert_ne!(
        plan_digest(&original_plan).unwrap(),
        plan_digest(&changed_census).unwrap()
    );

    let mut incomplete_census = original_plan.clone();
    incomplete_census.binding.activation_census_complete = false;
    assert_ne!(
        plan_digest(&original_plan).unwrap(),
        plan_digest(&incomplete_census).unwrap()
    );

    let mut incomplete_ranking = original_plan.clone();
    incomplete_ranking.binding.activation_ranking_complete = false;
    assert_ne!(
        plan_digest(&original_plan).unwrap(),
        plan_digest(&incomplete_ranking).unwrap()
    );

    let mut changed_routes = plan(vec![
        assignment("artifact-001", "file-a", "src/a.rs", 'a'),
        assignment("artifact-002", "file-b", "src/b.rs", 'b'),
    ]);
    let without_routes = plan_digest(&changed_routes).unwrap();
    changed_routes.allowed_routes =
        trusted_routes_for_assignments(&changed_routes.assignments, changed_routes.max_rounds);
    assert_ne!(without_routes, plan_digest(&changed_routes).unwrap());
}

#[cfg(unix)]
#[tokio::test]
async fn graph_candidates_cannot_escape_the_independent_source_census() {
    let temp = tempfile::Builder::new()
        .prefix(".primitive-census-graph-")
        .tempdir_in(".")
        .unwrap();
    let outside_temp = tempfile::Builder::new()
        .prefix(".primitive-census-outside-")
        .tempdir_in(".")
        .unwrap();
    let root = std::fs::canonicalize(temp.path()).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join(".aws")).unwrap();
    std::fs::create_dir_all(root.join(".ssh")).unwrap();
    std::fs::create_dir_all(root.join("secrets")).unwrap();
    std::fs::write(
        root.join("src/allowed.rs"),
        "pub fn activate_target() -> bool { true }\n",
    )
    .unwrap();
    std::fs::write(root.join(".env"), "API_KEY=do-not-read\n").unwrap();
    std::fs::write(root.join(".aws/credentials"), "aws_secret=do-not-read\n").unwrap();
    std::fs::write(root.join(".ssh/config"), "IdentityFile do-not-read\n").unwrap();
    std::fs::write(
        root.join("secrets/credential.rs"),
        "const SECRET: &str = \"do-not-read\";\n",
    )
    .unwrap();
    let outside = outside_temp.path().join("outside.rs");
    std::fs::write(&outside, "pub fn activate_target() {}\n").unwrap();

    let outside_relative = format!(
        "../{}/outside.rs",
        outside_temp.path().file_name().unwrap().to_string_lossy()
    );

    let malicious_paths = [
        ".env".to_string(),
        ".aws/credentials".to_string(),
        ".ssh/config".to_string(),
        "secrets/credential.rs".to_string(),
        "missing.rs".to_string(),
        outside_relative,
    ];
    let mut nodes = vec![Node::code("allowed", "src/allowed.rs")];
    let mut edges = Vec::new();
    for (position, path) in malicious_paths.iter().enumerate() {
        let id = format!("activate-malicious-{position}");
        nodes.push(Node::code(&id, path));
        edges.push(Edge {
            from: "allowed".to_string(),
            to: id,
            edge_type: EdgeType::DependsOn,
        });
    }
    write_graph(&root, &Graph { nodes, edges });

    let prepared = build_activation_plan(
        "inspect activate_target and its dependencies",
        &root,
        &permissive_safety(),
        2,
        MAX_ARTIFACTS,
    )
    .await
    .unwrap();

    assert!(matches!(prepared.plan.graph, GraphStatus::Loaded { .. }));
    assert_eq!(
        prepared.plan.selection_mode,
        "trusted_census_plus_frozen_graph"
    );
    assert_eq!(
        prepared
            .plan
            .assignments
            .iter()
            .map(|assignment| assignment.path.as_str())
            .collect::<Vec<_>>(),
        vec!["src/allowed.rs"]
    );
    assert_eq!(prepared.artifacts.len(), 1);
}

#[cfg(unix)]
#[tokio::test]
async fn private_graph_path_is_rejected_without_expanding_activation() {
    let temp = tempfile::Builder::new()
        .prefix(".primitive-private-graph-")
        .tempdir_in(".")
        .unwrap();
    let root = std::fs::canonicalize(temp.path()).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join(".claude")).unwrap();
    std::fs::write(
        root.join("src/allowed.rs"),
        "pub fn private_graph_marker() {}\n",
    )
    .unwrap();
    std::fs::write(
        root.join(".claude/private.rs"),
        "pub fn private_graph_marker() { expose_secret(); }\n",
    )
    .unwrap();
    write_graph(
        &root,
        &Graph {
            nodes: vec![
                Node::code("allowed", "src/allowed.rs"),
                Node::code("private", ".claude/private.rs"),
            ],
            edges: vec![Edge {
                from: "allowed".to_string(),
                to: "private".to_string(),
                edge_type: EdgeType::DependsOn,
            }],
        },
    );

    let prepared = build_activation_plan(
        "inspect private_graph_marker",
        &root,
        &permissive_safety(),
        2,
        MAX_ARTIFACTS,
    )
    .await
    .unwrap();

    assert!(matches!(
        prepared.plan.graph,
        GraphStatus::Rejected { ref reason, .. }
            if reason.contains("private coding-agent state")
    ));
    assert_eq!(
        prepared.plan.selection_mode,
        "trusted_census_graph_rejected"
    );
    assert_eq!(prepared.plan.assignments.len(), 1);
    assert_eq!(prepared.plan.assignments[0].path, "src/allowed.rs");
}

#[cfg(unix)]
#[tokio::test]
async fn missing_graph_is_reported_and_unselected_census_mutation_invalidates_binding() {
    let temp = tempfile::Builder::new()
        .prefix(".primitive-census-binding-")
        .tempdir_in(".")
        .unwrap();
    let root = std::fs::canonicalize(temp.path()).unwrap();
    std::fs::write(
        root.join("selected.rs"),
        "pub fn selected_marker() -> bool { true }\n",
    )
    .unwrap();
    std::fs::write(root.join("unselected.rs"), "pub fn unrelated() {}\n").unwrap();
    let safety = permissive_safety();

    let prepared = build_activation_plan("inspect selected_marker", &root, &safety, 2, 1)
        .await
        .unwrap();
    assert!(matches!(prepared.plan.graph, GraphStatus::Missing));
    assert_eq!(prepared.plan.selection_mode, "trusted_census_graph_missing");
    assert_eq!(prepared.plan.assignments[0].path, "selected.rs");
    assert!(binding_changes(&prepared, &safety).await.is_empty());

    std::fs::write(
        root.join("unselected.rs"),
        "pub fn unrelated_changed() {}\n",
    )
    .unwrap();
    assert_eq!(
        binding_changes(&prepared, &safety).await,
        vec!["activation census changed".to_string()]
    );
}

#[cfg(unix)]
#[tokio::test]
async fn binding_check_detects_artifact_bytes_changed_after_activation() {
    let temp = tempfile::Builder::new()
        .prefix(".primitive-binding-")
        .tempdir_in(".")
        .unwrap();
    let root = std::fs::canonicalize(temp.path()).unwrap();
    let path = root.join("artifact.rs");
    std::fs::write(&path, "pub fn original() {}\n").unwrap();
    let safety = SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        ..SafetyConfig::default()
    };

    let owner = assignment("artifact-001", "file-a", "artifact.rs", 'a');
    let mut prepared_plan = plan(vec![PrimitiveAssignment {
        base_sha256: sha256(b"pub fn original() {}\n"),
        ..owner
    }]);
    prepared_plan.binding.head_tree = git_head_tree(&root);
    prepared_plan.binding.activation_census_sha256 = recomputed_localization_census(&root, &safety)
        .await
        .unwrap()
        .manifest_sha256;
    prepared_plan.binding.working_set_sha256 = working_set_digest(&prepared_plan.assignments);
    prepared_plan.binding.plan_sha256 = plan_digest(&prepared_plan).unwrap();
    let prepared = PreparedActivation {
        root,
        plan: prepared_plan,
        artifacts: BTreeMap::new(),
    };

    assert!(binding_changes(&prepared, &safety).await.is_empty());
    std::fs::write(&path, "pub fn changed() {}\n").unwrap();
    let changes = binding_changes(&prepared, &safety).await;
    assert!(changes.contains(&"activation census changed".to_string()));
    assert!(changes.contains(&"artifact.rs content changed".to_string()));
}

#[cfg(unix)]
#[cfg(unix)]
#[tokio::test]
async fn binding_check_rejects_artifact_replaced_by_symlink() {
    use std::os::unix::fs::symlink;

    let root_temp = tempfile::Builder::new()
        .prefix(".primitive-binding-root-")
        .tempdir_in(".")
        .unwrap();
    let outside_temp = tempfile::Builder::new()
        .prefix(".primitive-binding-outside-")
        .tempdir_in(".")
        .unwrap();
    let root = std::fs::canonicalize(root_temp.path()).unwrap();
    let path = root.join("artifact.rs");
    std::fs::write(&path, "pub fn original() {}\n").unwrap();
    let safety = SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        ..SafetyConfig::default()
    };

    let owner = assignment("artifact-001", "file-a", "artifact.rs", 'a');
    let mut prepared_plan = plan(vec![PrimitiveAssignment {
        base_sha256: sha256(b"pub fn original() {}\n"),
        ..owner
    }]);
    prepared_plan.binding.head_tree = git_head_tree(&root);
    prepared_plan.binding.activation_census_sha256 = recomputed_localization_census(&root, &safety)
        .await
        .unwrap()
        .manifest_sha256;
    prepared_plan.binding.working_set_sha256 = working_set_digest(&prepared_plan.assignments);
    prepared_plan.binding.plan_sha256 = plan_digest(&prepared_plan).unwrap();
    let prepared = PreparedActivation {
        root: root.clone(),
        plan: prepared_plan,
        artifacts: BTreeMap::new(),
    };

    let outside = outside_temp.path().join("artifact.rs");
    std::fs::write(&outside, "pub fn original() {}\n").unwrap();
    std::fs::remove_file(&path).unwrap();
    symlink(&outside, &path).unwrap();

    let changes = binding_changes(&prepared, &safety).await;
    assert!(changes.contains(&"activation census changed".to_string()));
    assert!(changes
        .iter()
        .any(|change| change.starts_with("artifact.rs is no longer readable:")));
}

#[cfg(unix)]
#[cfg(unix)]
#[tokio::test]
async fn advisory_graph_symlink_is_rejected() {
    use std::os::unix::fs::symlink;

    let root_temp = tempfile::Builder::new()
        .prefix(".primitive-graph-root-")
        .tempdir_in(".")
        .unwrap();
    let outside_temp = tempfile::Builder::new()
        .prefix(".primitive-graph-outside-")
        .tempdir_in(".")
        .unwrap();
    let root = std::fs::canonicalize(root_temp.path()).unwrap();
    std::fs::create_dir(root.join(".selfware")).unwrap();
    let outside_graph = outside_temp.path().join("evolve-graph.yaml");
    std::fs::write(&outside_graph, b"nodes: []\nedges: []\n").unwrap();
    symlink(&outside_graph, root.join(".selfware/evolve-graph.yaml")).unwrap();

    let graph = load_advisory_graph(
        &root,
        &SafetyConfig {
            allowed_paths: vec!["/**".to_string()],
            ..SafetyConfig::default()
        },
    )
    .await;

    assert!(matches!(graph.status, GraphStatus::Rejected { reason, .. }
        if reason.contains("graph read rejected")));
    assert!(graph.digest.is_none());
    assert!(graph.index.is_none());
}

#[test]
fn requested_private_state_root_is_rejected() {
    let temp = tempfile::Builder::new()
        .prefix(".primitive-private-root-")
        .tempdir_in(".")
        .unwrap();
    let root = temp.path().join(".selfware/project");
    std::fs::create_dir_all(&root).unwrap();

    let error = resolve_root(
        &root,
        &SafetyConfig {
            allowed_paths: vec!["/**".to_string()],
            ..SafetyConfig::default()
        },
    )
    .unwrap_err();

    assert!(error
        .to_string()
        .contains("requested root contains forbidden private-state component '.selfware'"));
}

#[cfg(unix)]
#[test]
fn canonical_private_state_root_is_rejected_through_parent_symlink() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::Builder::new()
        .prefix(".primitive-private-root-")
        .tempdir_in(".")
        .unwrap();
    let private_parent = temp.path().join(".claude");
    let private_root = private_parent.join("project");
    std::fs::create_dir_all(&private_root).unwrap();
    symlink(&private_parent, temp.path().join("alias")).unwrap();
    let requested = temp.path().join("alias/project");

    let error = resolve_root(
        &requested,
        &SafetyConfig {
            allowed_paths: vec!["/**".to_string()],
            ..SafetyConfig::default()
        },
    )
    .unwrap_err();

    assert!(error
        .to_string()
        .contains("canonical root contains forbidden private-state component '.claude'"));
}
