//! Unit tests for symbol extraction (`evolve::symbols`).

use super::*;

#[test]
fn extracts_each_pub_item_kind_with_span_and_measured_tokens() {
    let content = "pub fn do_thing(x: i32) -> i32 {\n    x + 1\n}\n\n\
                   pub async fn fetch() {\n    let _ = 1;\n}\n\n\
                   pub struct Thing {\n    pub field: i32,\n}\n\n\
                   pub enum Choice {\n    A,\n    B,\n}\n\n\
                   pub trait Worker {\n    fn work(&self);\n}\n";
    let decls = extract_pub_symbols(content);
    let by_name: std::collections::HashMap<&str, &SymbolDecl> =
        decls.iter().map(|d| (d.name.as_str(), d)).collect();
    for (name, kind) in [
        ("do_thing", "fn"),
        ("fetch", "fn"),
        ("Thing", "struct"),
        ("Choice", "enum"),
        ("Worker", "trait"),
    ] {
        let decl = by_name
            .get(name)
            .unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(decl.kind, kind, "{name}");
        let (start, end) = decl.line_range;
        assert!(start >= 1 && end >= start, "{name} range sane: {decl:?}");
        let span: String = content
            .lines()
            .skip(start - 1)
            .take(end - start + 1)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            span.contains(name),
            "{name} range must cover its declaration: {span:?}"
        );
        // Rule 4: tokens are measured, never a byte fraction.
        assert_eq!(
            decl.tokens,
            crate::token_count::estimate_content_tokens(&span),
            "{name} tokens must be the measured span cost"
        );
    }
}

#[test]
fn non_pub_and_impl_items_are_excluded() {
    let content = "fn private_fn() {}\n\
                   pub(crate) fn crate_fn() {}\n\
                   struct PrivateStruct;\n\
                   impl Thing {\n    pub fn method(&self) {}\n}\n";
    let decls = extract_pub_symbols(content);
    assert!(
        decls.is_empty(),
        "only top-level pub items become symbols: {decls:?}"
    );
}

#[test]
fn unparseable_source_yields_no_symbols_not_guesses() {
    assert!(extract_pub_symbols("pub fn broken( {{{").is_empty());
}

#[test]
fn identifiers_split_on_non_identifier_chars() {
    let ids = identifiers("let _ = foo(bar::Baz, qux_1);");
    assert!(ids.contains("foo"));
    assert!(ids.contains("bar"));
    assert!(ids.contains("Baz"));
    assert!(ids.contains("qux_1"));
    assert!(!ids.contains("let _"));
    assert!(identifiers("").is_empty());
}

// ---------------------------------------------------------------------------
// GraphBuilder symbol pass (tempdir fixture projects)
// ---------------------------------------------------------------------------

use crate::evolve::{EdgeType, Graph, NodeLayer};

/// Fixture: alpha imports Shared from beta; Shared is ALSO defined in gamma
/// (ambiguous), LocalWidget defined in alpha AND gamma (ambiguous), beta is
/// a directory module whose `shared` fn collides with the `beta/shared.rs`
/// file node id.
fn fixture_project() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("tempdir");
    let src = temp.path().join("src");
    std::fs::create_dir_all(src.join("beta")).expect("dirs");
    std::fs::write(
        src.join("lib.rs"),
        "pub mod alpha;\npub mod beta;\npub mod gamma;\n",
    )
    .expect("lib");
    std::fs::write(
        src.join("alpha.rs"),
        "use crate::beta::Shared;\n\n\
         pub fn caller() {\n    let _ = Shared;\n    let _ = helper();\n}\n\n\
         fn helper() {}\n\n\
         pub struct LocalWidget;\n\n\
         pub fn uses_widget(w: LocalWidget) -> LocalWidget {\n    w\n}\n",
    )
    .expect("alpha");
    std::fs::write(
        src.join("beta/mod.rs"),
        "pub struct Shared;\n\npub fn shared() {}\n",
    )
    .expect("beta mod");
    std::fs::write(src.join("beta/shared.rs"), "pub fn lonely() {}\n").expect("beta shared");
    std::fs::write(
        src.join("gamma.rs"),
        "pub struct Shared;\n\npub struct LocalWidget;\n",
    )
    .expect("gamma");
    temp
}

fn build(temp: &tempfile::TempDir, symbols: bool) -> Graph {
    crate::evolve::GraphBuilder::new(temp.path().join("src"))
        .with_symbols(symbols)
        .scan_src()
        .expect("scan_src")
}

fn has_dep_edge(graph: &Graph, from: &str, to: &str) -> bool {
    graph
        .edges
        .iter()
        .any(|e| e.from == from && e.to == to && matches!(e.edge_type, EdgeType::DependsOn))
}

fn has_contains_edge(graph: &Graph, from: &str, to: &str) -> bool {
    graph
        .edges
        .iter()
        .any(|e| e.from == from && e.to == to && matches!(e.edge_type, EdgeType::Contains))
}

#[test]
fn builder_emits_symbol_nodes_with_v2_fields() {
    let temp = fixture_project();
    let graph = build(&temp, true);

    for id in [
        "crate::alpha::caller",
        "crate::alpha::LocalWidget",
        "crate::alpha::uses_widget",
        "crate::beta::Shared",
        "crate::gamma::Shared",
        "crate::gamma::LocalWidget",
    ] {
        let node = graph
            .nodes
            .iter()
            .find(|n| n.id == id)
            .unwrap_or_else(|| panic!("missing symbol node {id}"));
        assert_eq!(node.layer, NodeLayer::Symbol, "{id}");
        assert_eq!(node.classification, "symbol", "{id}");
        assert!(node.parent_id.is_some(), "{id} parent");
        assert!(node.symbol_kind.is_some(), "{id} kind");
        assert!(node.line_range.is_some(), "{id} range");
        assert!(node.tokens > 0, "{id} measured tokens");
    }
    // Non-pub fn never becomes a symbol.
    assert!(graph.nodes.iter().all(|n| n.id != "crate::alpha::helper"));
    // Contains edges link files to their symbols.
    assert!(has_contains_edge(
        &graph,
        "crate::alpha",
        "crate::alpha::caller"
    ));
    assert!(has_contains_edge(
        &graph,
        "crate::beta",
        "crate::beta::Shared"
    ));
}

#[test]
fn builder_links_intra_file_and_confident_cross_file_only() {
    let temp = fixture_project();
    let graph = build(&temp, true);

    // Intra-file mention.
    assert!(has_dep_edge(
        &graph,
        "crate::alpha::uses_widget",
        "crate::alpha::LocalWidget"
    ));
    // Ambiguous name (alpha + gamma) never links cross-file.
    assert!(!has_dep_edge(
        &graph,
        "crate::alpha::uses_widget",
        "crate::gamma::LocalWidget"
    ));
    // Ambiguous name (beta + gamma) resolved CONFIDENTLY by the import.
    assert!(has_dep_edge(
        &graph,
        "crate::alpha::caller",
        "crate::beta::Shared"
    ));
    // ...but not to the file that was NOT imported.
    assert!(!has_dep_edge(
        &graph,
        "crate::alpha::caller",
        "crate::gamma::Shared"
    ));
    // Non-pub target: no edge to `helper` (no such symbol node).
    assert!(graph.edges.iter().all(|e| e.to != "crate::alpha::helper"));
    // File-level edges are unchanged.
    assert!(has_dep_edge(&graph, "crate::alpha", "crate::beta"));
}

#[test]
fn builder_skips_symbol_ids_colliding_with_file_nodes() {
    let temp = fixture_project();
    let graph = build(&temp, true);

    // crate::beta::shared is the FILE node of src/beta/shared.rs; the `shared`
    // fn in beta/mod.rs would share that id and must be skipped.
    let node = graph
        .nodes
        .iter()
        .find(|n| n.id == "crate::beta::shared")
        .expect("file node present");
    assert_eq!(node.layer, NodeLayer::Code);
    assert!(node.parent_id.is_none(), "the id stayed the file node's");
    assert!(
        graph
            .nodes
            .iter()
            .all(|n| !(n.layer == NodeLayer::Symbol && n.id == "crate::beta::shared")),
        "no symbol node may claim the file node's id"
    );
}

#[test]
fn builder_flag_off_keeps_file_level_graph() {
    let temp = fixture_project();
    let graph = build(&temp, false);

    assert!(graph.nodes.iter().all(|n| n.layer != NodeLayer::Symbol));
    assert!(graph.nodes.iter().all(|n| n.parent_id.is_none()));
    assert!(has_dep_edge(&graph, "crate::alpha", "crate::beta"));
}

/// Manual repo-scale measurement for the symbol pass (not part of the
/// default gate run): `cargo test --lib measure_symbol_graph_size -- --ignored --nocapture`.
#[test]
#[ignore = "manual repo-scale measurement"]
fn measure_symbol_graph_size() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let report = |symbols: bool, label: &str| {
        let graph = crate::evolve::GraphBuilder::new(root.join("src"))
            .with_symbols(symbols)
            .scan_src()
            .expect("scan_src");
        let yaml = serde_yaml::to_string(&graph).expect("serialize");
        let symbol_nodes = graph
            .nodes
            .iter()
            .filter(|n| n.layer == NodeLayer::Symbol)
            .count();
        eprintln!(
            "{label}: nodes={} (symbols={}) edges={} yaml_bytes={}",
            graph.nodes.len(),
            symbol_nodes,
            graph.edges.len(),
            yaml.len()
        );
        (graph.nodes.len(), graph.edges.len(), yaml.len())
    };
    let before = report(false, "file-level only");
    let after = report(true, "with symbols");
    eprintln!(
        "delta: nodes {:.2}x edges {:.2}x yaml {:.2}x",
        after.0 as f64 / before.0 as f64,
        after.1 as f64 / before.1 as f64,
        after.2 as f64 / before.2 as f64,
    );
}
