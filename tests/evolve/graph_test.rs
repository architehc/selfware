use selfware::evolve::graph::GraphBuilder;
use selfware::evolve::{EdgeType, NodeLayer};

fn write_rust(path: &std::path::Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

#[test]
fn test_non_rust_implementation_sources_are_code_nodes_with_test_partition() {
    let project = tempfile::tempdir().unwrap();
    write_rust(&project.path().join("src/lib.rs"), "pub fn library() {}\n");
    for (path, content) in [
        ("src/worker.py", "def work():\n    return 1\n"),
        ("src/app.js", "export function boot() {}\n"),
        ("src/app.ts", "export function boot(): void {}\n"),
        ("src/component.jsx", "export const C = () => null;\n"),
        ("src/view.tsx", "export const V = () => null;\n"),
        ("src/main.go", "package main\nfunc main() {}\n"),
        // Ecosystem test naming conventions must partition to the Test layer.
        ("src/test_worker.py", "def test_work():\n    assert True\n"),
        ("src/worker_test.py", "def test_work():\n    assert True\n"),
        ("src/app.test.js", "test('boot', () => {});\n"),
        ("src/util.spec.ts", "describe('x', () => {});\n"),
        ("src/main_test.go", "package main\nfunc TestMain() {}\n"),
        // Shell scripts and markup stay Auxiliary even under src/.
        ("src/deploy.sh", "echo deploy\n"),
        ("src/index.html", "<html></html>\n"),
    ] {
        write_rust(&project.path().join(path), content);
    }

    let graph = GraphBuilder::new(project.path().join("src"))
        .scan_src()
        .unwrap();
    let by_id = |id: &str| {
        graph
            .nodes
            .iter()
            .find(|node| node.id == id)
            .unwrap_or_else(|| panic!("missing node {id}"))
    };

    for (id, class) in [
        ("crate::worker.py", "python_source"),
        ("crate::app.js", "javascript_source"),
        ("crate::app.ts", "typescript_source"),
        ("crate::component.jsx", "javascript_source"),
        ("crate::view.tsx", "typescript_source"),
        ("crate::main.go", "go_source"),
    ] {
        let node = by_id(id);
        assert_eq!(node.layer, NodeLayer::Code, "{id} must be a Code node");
        assert_eq!(node.classification, class, "{id} classification");
        assert!(node.tokens > 0, "{id} must carry measured tokens");
    }

    for id in [
        "test::test_worker.py",
        "test::worker_test.py",
        "test::app.test.js",
        "test::util.spec.ts",
        "test::main_test.go",
    ] {
        assert_eq!(
            by_id(id).layer,
            NodeLayer::Test,
            "{id} must partition to the Test layer"
        );
    }

    assert_eq!(by_id("crate::deploy.sh").layer, NodeLayer::Auxiliary);
    assert_eq!(by_id("crate::index.html").layer, NodeLayer::Auxiliary);
}

#[test]
fn test_graph_builder_scans_agent_component() {
    let builder = GraphBuilder::new("src");
    let graph = builder.scan_src().unwrap();
    let agent = graph.nodes.iter().find(|n| n.id == "crate::agent").unwrap();
    assert!(agent.tokens > 0);
    assert!(agent.files > 0);
}

#[test]
fn test_depends_on_edges_never_dangle() {
    let builder = GraphBuilder::new("src");
    let graph = builder.scan_src().unwrap();
    let ids: std::collections::HashSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
    // File modules must be normalized to bare names (no `.rs` suffix).
    assert!(ids.iter().all(|id| !id.ends_with(".rs")));
    for edge in &graph.edges {
        assert!(
            ids.contains(edge.from.as_str()),
            "dangling from: {}",
            edge.from
        );
        assert!(ids.contains(edge.to.as_str()), "dangling to: {}", edge.to);
    }
}

#[test]
fn test_scan_is_deterministic_and_separates_production_from_tests() {
    let project = tempfile::tempdir().unwrap();
    write_rust(
        &project.path().join("src/agent/mod.rs"),
        "use crate::agent::Agent;\nuse crate::config::Config;\npub struct Agent;\n",
    );
    write_rust(
        &project.path().join("src/config.rs"),
        "pub struct Config;\n#[cfg(test)]\nmod tests { #[test] fn config_works() {} }\n",
    );
    write_rust(
        &project.path().join("src/api/mod.rs"),
        "#[cfg(test)]\n#[path = \"tests.rs\"]\nmod tests;\npub fn request() {}\n",
    );
    write_rust(
        &project.path().join("src/api/tests.rs"),
        "use super::*;\n#[test]\nfn request_works() {}\n",
    );
    write_rust(
        &project.path().join("src/bin/tool.rs"),
        "use selfware::config::Config;\nfn main() {}\n",
    );
    std::fs::create_dir_all(project.path().join("src/web")).unwrap();
    std::fs::write(
        project.path().join("src/web/app.js"),
        "export function boot() {}\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(project.path().join("scripts")).unwrap();
    std::fs::write(
        project.path().join("scripts/generate.py"),
        "print('generate')\n",
    )
    .unwrap();
    std::fs::create_dir_all(project.path().join("system_tests")).unwrap();
    std::fs::write(project.path().join("system_tests/flow.sh"), "echo test\n").unwrap();
    write_rust(
        &project.path().join("tests/agent_test.rs"),
        "use selfware::agent::Agent;\n#[test]\nfn agent_works() {}\n",
    );
    write_rust(
        &project.path().join("examples/demo.rs"),
        "use selfware::config::Config;\nfn main() {}\n",
    );

    let builder = GraphBuilder::new(project.path().join("src"));
    let first = builder.scan_src().unwrap();
    let second = builder.scan_src().unwrap();

    let ids: Vec<_> = first
        .nodes
        .iter()
        .filter(|node| node.path.is_some())
        .map(|node| node.id.as_str())
        .collect();
    // Schema v2: Code-layer Rust files also emit symbol-level nodes for their
    // top-level `pub` items (parent file id + `::Name`), sorted with the rest.
    assert_eq!(
        ids,
        vec![
            "bin::tool",
            "crate::agent",
            "crate::agent::Agent",
            "crate::api",
            "crate::api::request",
            "crate::config",
            "crate::config::Config",
            "crate::web::app.js",
            "example::demo",
            "test::agent_test",
            "test::api::tests",
            "test::system_tests::flow.sh",
            "tool::Cargo.toml",
            "tool::scripts::generate.py"
        ]
    );
    assert!(first
        .nodes
        .iter()
        .filter(|node| node.layer == NodeLayer::Code)
        .all(|node| {
            node.path.as_deref().unwrap().contains("src/") || node.id.starts_with("tool::")
        }));
    assert!(first
        .nodes
        .iter()
        .filter(|node| node.layer == NodeLayer::Test)
        .all(|node| node.id.starts_with("test::") || node.id.starts_with("example::")));
    assert_eq!(
        first
            .nodes
            .iter()
            .find(|node| node.id == "test::api::tests")
            .unwrap()
            .layer,
        NodeLayer::Test
    );
    let config = first
        .nodes
        .iter()
        .find(|node| node.id == "crate::config")
        .unwrap();
    assert_eq!(config.inline_test_ranges, 1);
    assert_eq!(config.inline_test_lines, 2);

    assert!(first.edges.iter().any(|edge| {
        edge.from == "crate::api"
            && edge.to == "test::api::tests"
            && edge.edge_type == EdgeType::Contains
    }));

    let dependency_edges: Vec<_> = first
        .edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::DependsOn)
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect();
    assert_eq!(
        dependency_edges,
        vec![
            ("bin::tool", "crate::config"),
            ("crate::agent", "crate::config"),
            ("example::demo", "crate::config"),
            ("test::agent_test", "crate::agent")
        ]
    );
    assert!(first.edges.iter().all(|edge| edge.from != edge.to));

    let second_ids: Vec<_> = second.nodes.iter().map(|node| node.id.as_str()).collect();
    let first_ids: Vec<_> = first.nodes.iter().map(|node| node.id.as_str()).collect();
    let second_edges: Vec<_> = second
        .edges
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str(), edge.edge_type.clone()))
        .collect();
    let first_edges: Vec<_> = first
        .edges
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str(), edge.edge_type.clone()))
        .collect();
    assert_eq!(first_ids, second_ids);
    assert_eq!(first_edges, second_edges);
}

#[test]
fn test_repository_scan_covers_workspace_sources_and_prunes_unsafe_artifacts() {
    let project = tempfile::tempdir().unwrap();
    for (path, content) in [
        ("src/lib.rs", "pub fn library() {}\n"),
        (
            "vscode-selfware/src/extension.ts",
            "export function activate() {}\n",
        ),
        ("zed-extension/src/lib.rs", "pub fn extension() {}\n"),
        ("fuzz/fuzz_targets/parser.rs", "fn fuzz_target() {}\n"),
        ("workflows/code_review.swl", "workflow code_review {}\n"),
        ("rustfmt.toml", "edition = \"2021\"\n"),
        ("Makefile", "check:\n\tcargo check\n"),
        ("target/generated.rs", "pub fn generated() {}\n"),
        (
            "node_modules/pkg/index.ts",
            "export const dependency = 1;\n",
        ),
        ("vendor/copied.rs", "pub fn copied() {}\n"),
        ("build/bundle.js", "export const built = 1;\n"),
        (".selfware/private.json", "{}\n"),
        ("selfware.toml", "api_key = \"do-not-index\"\n"),
        ("credentials.json", "{}\n"),
        ("codegraph.json", "{}\n"),
    ] {
        let full = project.path().join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, content).unwrap();
    }
    let binary = project.path().join("src/binary.rs");
    std::fs::write(binary, [0, 159, 146, 150]).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(
            project.path().join("vscode-selfware/src/extension.ts"),
            project.path().join("src/symlink.ts"),
        )
        .unwrap();
    }

    let graph = GraphBuilder::new(project.path().join("src"))
        .scan_src()
        .unwrap();
    let by_id = |id: &str| graph.nodes.iter().find(|node| node.id == id).unwrap();

    // Vendored extensions and non-src tooling are covered as nodes but classified
    // Auxiliary so they stay out of the Rust code token tiers.
    assert_eq!(
        by_id("tool::vscode-selfware::src::extension.ts").layer,
        NodeLayer::Auxiliary
    );
    assert_eq!(
        by_id("tool::zed-extension::src::lib").layer,
        NodeLayer::Auxiliary
    );
    assert_eq!(
        by_id("test::fuzz::fuzz_targets::parser").layer,
        NodeLayer::Test
    );
    assert_eq!(
        by_id("tool::workflows::code_review.swl").layer,
        NodeLayer::Auxiliary
    );
    assert!(graph
        .nodes
        .iter()
        .any(|node| node.id == "tool::rustfmt.toml"));
    assert!(graph.nodes.iter().any(|node| node.id == "tool::Makefile"));

    let paths = graph
        .nodes
        .iter()
        .filter_map(|node| node.path.as_deref())
        .collect::<Vec<_>>();
    for excluded in [
        "/target/",
        "/node_modules/",
        "/vendor/",
        "/build/",
        "/.selfware/",
        "selfware.toml",
        "credentials.json",
        "codegraph.json",
        "binary.rs",
        "symlink.ts",
    ] {
        assert!(
            paths.iter().all(|path| !path.contains(excluded)),
            "{excluded}"
        );
    }
    // File nodes hang off structure:: nodes; schema-v2 symbol nodes hang off
    // their parent file node instead (also a Contains edge).
    for node in graph
        .nodes
        .iter()
        .filter(|node| node.path.is_some() && node.layer != NodeLayer::Symbol)
    {
        assert!(graph.edges.iter().any(|edge| {
            edge.edge_type == EdgeType::Contains
                && edge.to == node.id
                && edge.from.starts_with("structure::")
        }));
    }
    // Symbol nodes (schema v2): src/lib.rs declares `pub fn library`, so a
    // crate::library node exists and is contained by its parent file node.
    let library = by_id("crate::library");
    assert_eq!(library.layer, NodeLayer::Symbol);
    assert!(graph.edges.iter().any(|edge| {
        edge.edge_type == EdgeType::Contains && edge.from == "crate" && edge.to == "crate::library"
    }));
    assert!(selfware::evolve::validate_graph(&graph).valid);
}
