use super::*;
use std::fs;
use tempfile::tempdir;

fn sample_workspace() -> tempfile::TempDir {
    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"graph-test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("cargo");
    fs::create_dir_all(dir.path().join("src")).expect("src dir");
    fs::write(
            dir.path().join("src/lib.rs"),
            "pub mod model;\nuse crate::model::Widget;\n\npub fn build(widget: Widget) -> Widget { widget }\n",
        )
        .expect("lib");
    fs::write(dir.path().join("src/model.rs"), "pub struct Widget;\n").expect("model");
    dir
}

#[test]
fn builds_workspace_graph_with_structure_and_imports() {
    let dir = sample_workspace();
    let graph = build_workspace_graph(&WorkspaceGraphOptions::new(dir.path())).expect("graph");
    let summary = summarize_graph(&graph);

    assert!(summary.file_count >= 2);
    assert!(summary.function_count >= 1);
    assert!(summary.type_count >= 1);
    assert!(summary.contains_edges >= 3);
    assert!(summary.import_edges >= 1);
}

#[test]
fn filters_graph_to_focus_neighborhood() {
    let dir = sample_workspace();
    let mut options = WorkspaceGraphOptions::new(dir.path());
    options.focus = Some("Widget".to_string());
    options.max_nodes = 6;
    let graph = build_workspace_graph(&options).expect("graph");

    assert!(graph.node_count() <= 6);
    assert!(graph
        .nodes
        .values()
        .any(|node| node.qualified_name.contains("Widget")));
}

#[test]
fn renders_ascii_graph() {
    let dir = sample_workspace();
    let rendered =
        render_workspace_graph(&WorkspaceGraphOptions::new(dir.path()), OutputFormat::Ascii)
            .expect("render");

    assert!(rendered.contains("Widget"));
}
