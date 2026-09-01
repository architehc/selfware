use super::*;

#[test]
fn test_action_cost_multipliers() {
    assert!((ContextAction::Inspect.cost_multiplier() - 0.06).abs() < f64::EPSILON);
    assert!((ContextAction::ReadFull.cost_multiplier() - 1.0).abs() < f64::EPSILON);
    assert!((ContextAction::Alter.cost_multiplier() - 1.5).abs() < f64::EPSILON);
    assert!((ContextAction::BuildNew.cost_multiplier() - 2.0).abs() < f64::EPSILON);
}

#[test]
fn test_fusion_multipliers() {
    assert!((FusionLevel::Binary.multiplier() - 1.0).abs() < f64::EPSILON);
    assert!((FusionLevel::Trinary.multiplier() - 2.5).abs() < f64::EPSILON);
    assert!((FusionLevel::Quaternary.multiplier() - 4.0).abs() < f64::EPSILON);
}

#[test]
fn test_action_from_str_loose() {
    assert_eq!(
        ContextAction::from_str_loose("inspect"),
        Some(ContextAction::Inspect)
    );
    assert_eq!(
        ContextAction::from_str_loose("READ"),
        Some(ContextAction::ReadFull)
    );
    assert_eq!(
        ContextAction::from_str_loose("edit"),
        Some(ContextAction::Alter)
    );
    assert_eq!(
        ContextAction::from_str_loose("commit"),
        Some(ContextAction::Ship)
    );
    assert_eq!(ContextAction::from_str_loose("unknown"), None);
}

#[test]
fn test_is_cargo_op() {
    assert!(ContextAction::Verify.is_cargo_op());
    assert!(ContextAction::Test.is_cargo_op());
    assert!(!ContextAction::Inspect.is_cargo_op());
    assert!(!ContextAction::Alter.is_cargo_op());
}

#[test]
fn test_estimate_action_cost_nonexistent() {
    // fits_in_budget reads the global budget atomics; hold+reset them so a
    // concurrent budget-mutating test can't perturb the result.
    let _budget = crate::test_support::BudgetGuard::hold();
    let est = estimate_action_cost(
        ContextAction::ReadFull,
        Path::new("/nonexistent/file.rs"),
        FusionLevel::Binary,
    );
    // Should use fallback of 500 tokens
    assert_eq!(est.estimated_tokens, 500);
    assert!(est.fits_in_budget);
}

#[test]
fn test_estimate_action_cost_fusion_scaling() {
    let _budget = crate::test_support::BudgetGuard::hold();
    let binary = estimate_action_cost(
        ContextAction::ReadFull,
        Path::new("/nonexistent/file.rs"),
        FusionLevel::Binary,
    );
    let trinary = estimate_action_cost(
        ContextAction::ReadFull,
        Path::new("/nonexistent/file.rs"),
        FusionLevel::Trinary,
    );
    let quaternary = estimate_action_cost(
        ContextAction::ReadFull,
        Path::new("/nonexistent/file.rs"),
        FusionLevel::Quaternary,
    );

    assert!(trinary.estimated_tokens > binary.estimated_tokens);
    assert!(quaternary.estimated_tokens > trinary.estimated_tokens);
}

#[test]
fn test_cargo_op_adds_overhead() {
    let verify = estimate_action_cost(
        ContextAction::Verify,
        Path::new("/nonexistent/file.rs"),
        FusionLevel::Binary,
    );
    let inspect = estimate_action_cost(
        ContextAction::Inspect,
        Path::new("/nonexistent/file.rs"),
        FusionLevel::Binary,
    );
    // Verify should have more time due to cargo overhead
    assert!(verify.estimated_time_ms > inspect.estimated_time_ms);
}

#[test]
fn test_update_and_read_budget() {
    let _budget = crate::test_support::BudgetGuard::hold();
    update_budget(50_000, 200_000, 10);
    let (used, total, files) = read_budget();
    assert_eq!(used, 50_000);
    assert_eq!(total, 200_000);
    assert_eq!(files, 10);
}

#[test]
fn test_extract_use_deps_empty() {
    assert!(extract_use_deps("").is_empty());
    assert!(extract_use_deps("fn main() {}\n").is_empty());
    assert_eq!(
        extract_use_deps("use crate::tools::graph;\nuse crate::evolve::{Graph, Node};\n"),
        vec!["tools::graph".to_string(), "evolve::".to_string()]
    );
}

#[test]
fn test_measure_file_tokens_uses_measured_count_not_len_div_4() {
    // Repeated-character content is where a real tokenizer diverges most
    // from len/4: BPE merges the runs instead of charging one token per
    // four bytes.
    let content = "a".repeat(400);
    let mut file = tempfile::NamedTempFile::new().expect("tempfile");
    use std::io::Write;
    write!(file, "{content}").expect("write");

    let measured = measure_file_tokens(file.path());
    let expected = crate::token_count::estimate_content_tokens(&content);
    assert_eq!(measured, expected, "must be the measured token count");
    assert_ne!(
        measured,
        content.len() / 4,
        "must not be the old bytes÷4 heuristic"
    );
}

#[test]
fn test_measure_file_tokens_fallback_when_unreadable() {
    assert_eq!(
        measure_file_tokens(Path::new("/nonexistent/definitely-missing.rs")),
        UNREADABLE_FILE_FALLBACK_TOKENS
    );
}

#[test]
fn test_build_code_map_reports_measured_file_tokens() {
    // No graph on disk: every file takes the live fallback — measured
    // tokens and use-line deps from held content, flagged live.
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    let content = "use crate::alpha;\npub fn widget() {}\n";
    std::fs::create_dir_all(root.join("src/widgets")).expect("src dir");
    std::fs::write(root.join("src/widgets/widget.rs"), content).expect("write widget");

    let nodes = build_code_map(root, None, 2);
    let file_node = nodes
        .get("src/widgets/widget.rs")
        .expect("widget.rs node present");
    assert_eq!(
        file_node.token_estimate,
        crate::token_count::estimate_content_tokens(content),
        "per-file tokens must be measured, not len/4 ({})",
        content.len() / 4
    );
    assert_eq!(file_node.dependencies, vec!["alpha".to_string()]);
    assert!(file_node.live, "no graph ⇒ live fallback must be flagged");
    // The module rollup sums the same measured numbers.
    let module = nodes.get("src/widgets").expect("widgets module node");
    assert_eq!(module.token_estimate, file_node.token_estimate);
}

/// Fixture: two source files on disk plus an evolve graph covering them,
/// with a DependsOn edge widget → alpha.
fn fixture_project_with_graph() -> (tempfile::TempDir, usize) {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    std::fs::create_dir_all(root.join("src/widgets")).expect("src dir");
    // Files are written BEFORE the graph is saved, so the graph is fresh.
    std::fs::write(root.join("src/widgets/widget.rs"), "pub fn widget() {}\n")
        .expect("write widget");
    std::fs::write(root.join("src/alpha.rs"), "pub fn alpha() {}\n").expect("write alpha");

    let mut widget = crate::evolve::Node::code("crate::widgets::widget", "src/widgets/widget.rs");
    widget.tokens = 111;
    let mut alpha = crate::evolve::Node::code("crate::alpha", "src/alpha.rs");
    alpha.tokens = 222;
    let graph = crate::evolve::Graph {
        nodes: vec![widget, alpha],
        edges: vec![crate::evolve::Edge {
            from: "crate::widgets::widget".into(),
            to: "crate::alpha".into(),
            edge_type: crate::evolve::EdgeType::DependsOn,
        }],
    };
    crate::evolve::OntologyStore::new(root.join(".selfware/evolve-graph.yaml"))
        .save(&graph)
        .expect("save graph");
    (temp, 111)
}

#[test]
fn test_build_code_map_serves_graph_tokens_and_edges_when_fresh() {
    let (temp, widget_graph_tokens) = fixture_project_with_graph();
    let root = temp.path();

    let nodes = build_code_map(root, None, 2);
    let widget = nodes
        .get("src/widgets/widget.rs")
        .expect("widget node present");
    assert_eq!(
        widget.token_estimate, widget_graph_tokens,
        "fresh graph node tokens are served as-is"
    );
    assert!(!widget.live, "graph-served node must not be flagged live");
    assert_eq!(
        widget.dependencies,
        vec!["alpha".to_string()],
        "deps come from DependsOn edges (crate:: prefix stripped)"
    );
    // Module rollup sums the graph-served tokens.
    let module = nodes.get("src/widgets").expect("widgets module node");
    assert_eq!(module.token_estimate, widget_graph_tokens);
}

#[test]
fn test_build_code_map_falls_back_live_for_missing_and_stale_files() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    std::fs::create_dir_all(root.join("src")).expect("src dir");

    // Save the graph FIRST, then write the file: file mtime > graph mtime,
    // so even a covered path would be stale — and this file has no node.
    crate::evolve::OntologyStore::new(root.join(".selfware/evolve-graph.yaml"))
        .save(&crate::evolve::Graph::default())
        .expect("save empty graph");
    let content = "use crate::alpha;\npub fn fresh() {}\n";
    std::fs::write(root.join("src/fresh.rs"), content).expect("write fresh");

    let nodes = build_code_map(root, None, 1);
    let fresh = nodes.get("src/fresh.rs").expect("fresh.rs node present");
    assert!(fresh.live, "missing-from-graph file must be flagged live");
    assert_eq!(
        fresh.token_estimate,
        crate::token_count::estimate_content_tokens(content),
        "live fallback measures the current disk content"
    );
    assert_eq!(fresh.dependencies, vec!["alpha".to_string()]);
}

#[test]
fn test_build_code_map_focus_and_depth_filters() {
    let (temp, _) = fixture_project_with_graph();
    let root = temp.path();

    // Directory focus: only the widgets subtree.
    let nodes = build_code_map(root, Some("widgets"), 2);
    assert!(nodes.contains_key("src/widgets/widget.rs"));
    assert!(!nodes.contains_key("src/alpha.rs"));

    // File focus: exactly one node.
    let nodes = build_code_map(root, Some("alpha"), 2);
    assert_eq!(nodes.len(), 1);
    assert!(nodes.contains_key("src/alpha.rs"));

    // Depth 0: files directly under src only — the nested widget is cut.
    let nodes = build_code_map(root, None, 0);
    assert!(nodes.contains_key("src/alpha.rs"));
    assert!(!nodes.contains_key("src/widgets/widget.rs"));
}

#[test]
fn test_file_token_base_prefers_graph_when_fresh() {
    let (temp, widget_graph_tokens) = fixture_project_with_graph();
    let root = temp.path();
    let target = root.join("src/widgets/widget.rs");
    let (tokens, source) = file_token_base(root, &target);
    assert_eq!(tokens, widget_graph_tokens);
    assert_eq!(source, "graph");

    // A path outside the project falls back to the live measure.
    let (tokens, source) = file_token_base(root, std::path::Path::new("/nonexistent/x.rs"));
    assert_eq!(tokens, UNREADABLE_FILE_FALLBACK_TOKENS);
    assert_eq!(source, "live");
}
