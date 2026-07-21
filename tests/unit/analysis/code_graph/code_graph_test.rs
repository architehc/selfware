use super::*;

#[test]
fn test_node_type_as_str() {
    assert_eq!(NodeType::Function.as_str(), "function");
    assert_eq!(NodeType::Struct.as_str(), "struct");
    assert_eq!(NodeType::Module.as_str(), "module");
}

#[test]
fn test_node_type_color() {
    assert!(!NodeType::Function.color().is_empty());
    assert!(!NodeType::Struct.color().is_empty());
}

#[test]
fn test_edge_type_label() {
    assert_eq!(EdgeType::Calls.label(), "calls");
    assert_eq!(EdgeType::Imports.label(), "imports");
    assert_eq!(EdgeType::Implements.label(), "implements");
}

#[test]
fn test_graph_node_new() {
    let node = GraphNode::new("my_function", NodeType::Function);
    assert_eq!(node.name, "my_function");
    assert_eq!(node.node_type, NodeType::Function);
    assert!(node.id.starts_with("node_"));
}

#[test]
fn test_graph_node_builder() {
    let node = GraphNode::new("test", NodeType::Struct)
        .in_file("src/lib.rs")
        .at_line(42)
        .with_visibility("pub")
        .with_meta("key", "value");

    assert_eq!(node.file_path, Some("src/lib.rs".to_string()));
    assert_eq!(node.line_number, Some(42));
    assert_eq!(node.visibility, Some("pub".to_string()));
    assert_eq!(node.metadata.get("key"), Some(&"value".to_string()));
}

#[test]
fn test_graph_edge_new() {
    let edge = GraphEdge::new("n1", "n2", EdgeType::Calls);
    assert_eq!(edge.source, "n1");
    assert_eq!(edge.target, "n2");
    assert!(edge.id.starts_with("edge_"));
}

#[test]
fn test_graph_edge_with_weight() {
    let edge = GraphEdge::new("a", "b", EdgeType::Uses).with_weight(0.5);
    assert_eq!(edge.weight, 0.5);
}

#[test]
fn test_code_graph_new() {
    let graph = CodeGraph::new("test_graph");
    assert_eq!(graph.name, "test_graph");
    assert_eq!(graph.node_count(), 0);
    assert_eq!(graph.edge_count(), 0);
}

#[test]
fn test_code_graph_add_node() {
    let mut graph = CodeGraph::new("test");
    let node = GraphNode::new("func1", NodeType::Function);
    let id = graph.add_node(node);

    assert_eq!(graph.node_count(), 1);
    assert!(graph.get_node("func1").is_some());
    assert!(graph.get_node_by_id(&id).is_some());
}

#[test]
fn test_code_graph_connect() {
    let mut graph = CodeGraph::new("test");
    graph.add_node(GraphNode::new("a", NodeType::Function));
    graph.add_node(GraphNode::new("b", NodeType::Function));

    assert!(graph.connect("a", "b", EdgeType::Calls));
    assert_eq!(graph.edge_count(), 1);
}

#[test]
fn test_code_graph_dependencies() {
    let mut graph = CodeGraph::new("test");
    let a_id = graph.add_node(GraphNode::new("a", NodeType::Function));
    graph.add_node(GraphNode::new("b", NodeType::Function));
    graph.add_node(GraphNode::new("c", NodeType::Function));

    graph.connect("a", "b", EdgeType::Calls);
    graph.connect("a", "c", EdgeType::Calls);

    let deps = graph.dependencies(&a_id);
    assert_eq!(deps.len(), 2);
}

#[test]
fn test_code_graph_dependents() {
    let mut graph = CodeGraph::new("test");
    graph.add_node(GraphNode::new("a", NodeType::Function));
    graph.add_node(GraphNode::new("b", NodeType::Function));
    let c_id = graph.add_node(GraphNode::new("c", NodeType::Function));

    graph.connect("a", "c", EdgeType::Calls);
    graph.connect("b", "c", EdgeType::Calls);

    let depnts = graph.dependents(&c_id);
    assert_eq!(depnts.len(), 2);
}

#[test]
fn test_code_graph_nodes_by_type() {
    let mut graph = CodeGraph::new("test");
    graph.add_node(GraphNode::new("f1", NodeType::Function));
    graph.add_node(GraphNode::new("f2", NodeType::Function));
    graph.add_node(GraphNode::new("s1", NodeType::Struct));

    let functions = graph.nodes_by_type(NodeType::Function);
    assert_eq!(functions.len(), 2);

    let structs = graph.nodes_by_type(NodeType::Struct);
    assert_eq!(structs.len(), 1);
}

#[test]
fn test_code_graph_find_path() {
    let mut graph = CodeGraph::new("test");
    let a = graph.add_node(GraphNode::new("a", NodeType::Function));
    let _b = graph.add_node(GraphNode::new("b", NodeType::Function));
    let c = graph.add_node(GraphNode::new("c", NodeType::Function));

    graph.connect("a", "b", EdgeType::Calls);
    graph.connect("b", "c", EdgeType::Calls);

    let path = graph.find_path(&a, &c);
    assert!(path.is_some());
    assert_eq!(path.unwrap().len(), 3);
}

#[test]
fn test_code_graph_find_cycles() {
    let mut graph = CodeGraph::new("test");
    graph.add_node(GraphNode::new("a", NodeType::Function));
    graph.add_node(GraphNode::new("b", NodeType::Function));
    graph.add_node(GraphNode::new("c", NodeType::Function));

    graph.connect("a", "b", EdgeType::Calls);
    graph.connect("b", "c", EdgeType::Calls);
    graph.connect("c", "a", EdgeType::Calls);

    let cycles = graph.find_cycles();
    assert!(!cycles.is_empty());
}

#[test]
fn test_code_graph_node_metrics() {
    let mut graph = CodeGraph::new("test");
    let a = graph.add_node(GraphNode::new("a", NodeType::Function));
    graph.add_node(GraphNode::new("b", NodeType::Function));
    graph.add_node(GraphNode::new("c", NodeType::Function));

    graph.connect("b", "a", EdgeType::Calls);
    graph.connect("c", "a", EdgeType::Calls);
    graph.connect("a", "c", EdgeType::Calls);

    let metrics = graph.node_metrics(&a);
    assert_eq!(metrics.in_degree, 2);
    assert_eq!(metrics.out_degree, 1);
    assert_eq!(metrics.total_degree, 3);
}

#[test]
fn test_code_graph_find_hubs() {
    let mut graph = CodeGraph::new("test");
    let hub = graph.add_node(GraphNode::new("hub", NodeType::Function));
    for i in 0..5 {
        let id = graph.add_node(GraphNode::new(&format!("n{}", i), NodeType::Function));
        graph.add_edge(GraphEdge::new(&id, &hub, EdgeType::Calls));
    }

    let hubs = graph.find_hubs(3);
    assert!(!hubs.is_empty());
    assert_eq!(hubs[0].0.name, "hub");
}

#[test]
fn test_code_graph_subgraph() {
    let mut graph = CodeGraph::new("test");
    let a = graph.add_node(GraphNode::new("a", NodeType::Function));
    let b = graph.add_node(GraphNode::new("b", NodeType::Function));
    let c = graph.add_node(GraphNode::new("c", NodeType::Function));

    graph.add_edge(GraphEdge::new(&a, &b, EdgeType::Calls));
    graph.add_edge(GraphEdge::new(&b, &c, EdgeType::Calls));

    let sub = graph.subgraph(&[a.clone(), b.clone()]);
    assert_eq!(sub.node_count(), 2);
    assert_eq!(sub.edge_count(), 1);
}

#[test]
fn test_graph_renderer_to_dot() {
    let mut graph = CodeGraph::new("test");
    graph.add_node(GraphNode::new("a", NodeType::Function));
    graph.add_node(GraphNode::new("b", NodeType::Function));
    graph.connect("a", "b", EdgeType::Calls);

    let renderer = GraphRenderer::new();
    let dot = renderer.render(&graph, OutputFormat::Dot);

    assert!(dot.contains("digraph test"));
    assert!(dot.contains("->"));
}

#[test]
fn test_graph_renderer_to_mermaid() {
    let mut graph = CodeGraph::new("test");
    graph.add_node(GraphNode::new("a", NodeType::Function));
    graph.add_node(GraphNode::new("b", NodeType::Struct));
    graph.connect("a", "b", EdgeType::Uses);

    let renderer = GraphRenderer::new();
    let mermaid = renderer.render(&graph, OutputFormat::Mermaid);

    assert!(mermaid.contains("graph"));
    assert!(mermaid.contains("-->"));
}

#[test]
fn test_graph_renderer_to_ascii() {
    let mut graph = CodeGraph::new("test");
    graph.add_node(GraphNode::new("func", NodeType::Function).in_file("test.rs"));

    let renderer = GraphRenderer::new();
    let ascii = renderer.render(&graph, OutputFormat::Ascii);

    assert!(ascii.contains("=== test ==="));
    assert!(ascii.contains("[function]"));
}

#[test]
fn test_graph_renderer_to_json() {
    let mut graph = CodeGraph::new("test");
    graph.add_node(GraphNode::new("a", NodeType::Function));

    let renderer = GraphRenderer::new();
    let json = renderer.render(&graph, OutputFormat::Json);

    assert!(json.contains("\"name\": \"test\""));
    assert!(json.contains("\"nodes\""));
    assert!(json.contains("\"edges\""));
}

#[test]
fn test_graph_renderer_to_plantuml() {
    let mut graph = CodeGraph::new("test");
    graph.add_node(GraphNode::new("MyClass", NodeType::Struct));
    graph.add_node(GraphNode::new("MyInterface", NodeType::Trait));
    graph.connect("MyClass", "MyInterface", EdgeType::Implements);

    let renderer = GraphRenderer::new();
    let puml = renderer.render(&graph, OutputFormat::PlantUml);

    assert!(puml.contains("@startuml"));
    assert!(puml.contains("@enduml"));
    assert!(puml.contains("..|>"));
}

#[test]
fn test_graph_builder_basic() {
    let mut builder = GraphBuilder::new("project");
    builder.add_file("src/main.rs");
    builder.add_module("main", Some("src/main.rs"));
    builder.add_function("run", Some("src/main.rs"), Some(10));

    let graph = builder.build();
    assert_eq!(graph.node_count(), 3);
}

#[test]
fn test_graph_builder_connections() {
    let mut builder = GraphBuilder::new("project");
    builder.add_function("caller", None, None);
    builder.add_function("callee", None, None);
    builder.add_call("caller", "callee");

    let graph = builder.build();
    assert_eq!(graph.edge_count(), 1);
}

#[test]
fn test_graph_builder_hierarchy() {
    let mut builder = GraphBuilder::new("project");
    let mod_id = builder.add_module("mymod", None);
    builder.enter(&mod_id);
    builder.add_child("func1", NodeType::Function);
    builder.add_child("func2", NodeType::Function);
    builder.exit();

    let graph = builder.build();
    assert_eq!(graph.node_count(), 3);
    assert_eq!(graph.edge_count(), 2); // Contains edges
}

#[test]
fn test_sanitize_id() {
    assert_eq!(sanitize_id("hello-world"), "hello_world");
    assert_eq!(sanitize_id("foo::bar"), "foo__bar");
    assert_eq!(sanitize_id("test123"), "test123");
}

#[test]
fn test_renderer_with_direction() {
    let renderer = GraphRenderer::new().with_direction("LR");
    assert_eq!(renderer.direction, "LR");
}

#[test]
fn test_renderer_cluster() {
    let renderer = GraphRenderer::new().cluster();
    assert!(renderer.cluster_by_file);
}

#[test]
fn test_graph_merge() {
    let mut g1 = CodeGraph::new("g1");
    g1.add_node(GraphNode::new("a", NodeType::Function));

    let mut g2 = CodeGraph::new("g2");
    g2.add_node(GraphNode::new("b", NodeType::Function));

    g1.merge(&g2);
    assert_eq!(g1.node_count(), 2);
}

#[test]
fn test_node_label() {
    let node = GraphNode::new("func", NodeType::Function).with_qualified_name("module::func");
    assert_eq!(node.label(), "module::func");

    let simple = GraphNode::new("simple", NodeType::Function);
    assert_eq!(simple.label(), "simple");
}

#[test]
fn test_edge_display_label() {
    let edge = GraphEdge::new("a", "b", EdgeType::Calls).with_label("custom");
    assert_eq!(edge.display_label(), "custom");

    let default = GraphEdge::new("a", "b", EdgeType::Calls);
    assert_eq!(default.display_label(), "calls");
}

#[test]
fn test_node_type_all_variants_as_str() {
    assert_eq!(NodeType::File.as_str(), "file");
    assert_eq!(NodeType::Enum.as_str(), "enum");
    assert_eq!(NodeType::Trait.as_str(), "trait");
    assert_eq!(NodeType::Impl.as_str(), "impl");
    assert_eq!(NodeType::Const.as_str(), "const");
    assert_eq!(NodeType::TypeAlias.as_str(), "type");
    assert_eq!(NodeType::Macro.as_str(), "macro");
    assert_eq!(NodeType::Package.as_str(), "package");
}

#[test]
fn test_node_type_all_variants_color() {
    // All variants should return non-empty colors
    let types = [
        NodeType::File,
        NodeType::Module,
        NodeType::Function,
        NodeType::Struct,
        NodeType::Enum,
        NodeType::Trait,
        NodeType::Impl,
        NodeType::Const,
        NodeType::TypeAlias,
        NodeType::Macro,
        NodeType::Package,
    ];
    for t in types {
        assert!(!t.color().is_empty());
    }
}

#[test]
fn test_node_type_all_variants_dot_shape() {
    assert_eq!(NodeType::File.dot_shape(), "folder");
    assert_eq!(NodeType::Module.dot_shape(), "component");
    assert_eq!(NodeType::Struct.dot_shape(), "box");
    assert_eq!(NodeType::Enum.dot_shape(), "box");
    assert_eq!(NodeType::Trait.dot_shape(), "hexagon");
    assert_eq!(NodeType::Impl.dot_shape(), "parallelogram");
    assert_eq!(NodeType::Package.dot_shape(), "box3d");
    assert_eq!(NodeType::Const.dot_shape(), "ellipse"); // Default
}

#[test]
fn test_node_type_clone() {
    let nt = NodeType::Function;
    let cloned = nt;
    assert_eq!(nt, cloned);
}

#[test]
fn test_node_type_hash() {
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(NodeType::Function);
    set.insert(NodeType::Struct);
    set.insert(NodeType::Function); // Duplicate
    assert_eq!(set.len(), 2);
}

#[test]
fn test_edge_type_all_variants_label() {
    assert_eq!(EdgeType::Uses.label(), "uses");
    assert_eq!(EdgeType::Contains.label(), "contains");
    assert_eq!(EdgeType::Extends.label(), "extends");
    assert_eq!(EdgeType::TypeDependency.label(), "depends on");
    assert_eq!(EdgeType::References.label(), "references");
    assert_eq!(EdgeType::Implements.label(), "implements");
}

#[test]
fn test_edge_type_clone() {
    let et = EdgeType::Calls;
    let cloned = et;
    assert_eq!(et, cloned);
}

#[test]
fn test_edge_type_hash() {
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(EdgeType::Calls);
    set.insert(EdgeType::Imports);
    set.insert(EdgeType::Calls); // Duplicate
    assert_eq!(set.len(), 2);
}

#[test]
fn test_graph_node_debug() {
    let node = GraphNode::new("test", NodeType::Function);
    let debug = format!("{:?}", node);
    assert!(debug.contains("GraphNode"));
}

#[test]
fn test_graph_node_clone() {
    let node = GraphNode::new("test", NodeType::Function).with_meta("key", "value");
    let cloned = node.clone();
    assert_eq!(node.name, cloned.name);
    assert_eq!(node.metadata.get("key"), cloned.metadata.get("key"));
}

#[test]
fn test_graph_edge_debug() {
    let edge = GraphEdge::new("a", "b", EdgeType::Calls);
    let debug = format!("{:?}", edge);
    assert!(debug.contains("GraphEdge"));
}

#[test]
fn test_graph_edge_clone() {
    let edge = GraphEdge::new("a", "b", EdgeType::Calls)
        .with_weight(0.5)
        .with_label("custom");
    let cloned = edge.clone();
    assert_eq!(edge.source, cloned.source);
    assert_eq!(edge.weight, cloned.weight);
}

#[test]
fn test_code_graph_connect_nonexistent() {
    let mut graph = CodeGraph::new("test");
    graph.add_node(GraphNode::new("a", NodeType::Function));

    // Connecting to non-existent node should fail
    assert!(!graph.connect("a", "nonexistent", EdgeType::Calls));
    assert!(!graph.connect("nonexistent", "a", EdgeType::Calls));
}

#[test]
fn test_code_graph_no_path() {
    let mut graph = CodeGraph::new("test");
    let a = graph.add_node(GraphNode::new("a", NodeType::Function));
    let b = graph.add_node(GraphNode::new("b", NodeType::Function));
    // No edges

    let path = graph.find_path(&a, &b);
    assert!(path.is_none());
}

#[test]
fn test_code_graph_no_cycles() {
    let mut graph = CodeGraph::new("test");
    graph.add_node(GraphNode::new("a", NodeType::Function));
    graph.add_node(GraphNode::new("b", NodeType::Function));
    graph.connect("a", "b", EdgeType::Calls);

    let cycles = graph.find_cycles();
    assert!(cycles.is_empty());
}

#[test]
fn test_code_graph_empty_metrics() {
    let graph = CodeGraph::new("test");
    let metrics = graph.node_metrics("nonexistent");
    assert_eq!(metrics.in_degree, 0);
    assert_eq!(metrics.out_degree, 0);
}

#[test]
fn test_graph_builder_add_struct() {
    let mut builder = GraphBuilder::new("test");
    builder.add_struct("MyStruct", Some("src/lib.rs"));

    let graph = builder.build();
    let structs = graph.nodes_by_type(NodeType::Struct);
    assert_eq!(structs.len(), 1);
}

#[test]
fn test_graph_builder_add_trait() {
    let mut builder = GraphBuilder::new("test");
    builder.add_trait("MyTrait");

    let graph = builder.build();
    let traits = graph.nodes_by_type(NodeType::Trait);
    assert_eq!(traits.len(), 1);
}

#[test]
fn test_graph_builder_add_type_dependency() {
    let mut builder = GraphBuilder::new("test");
    builder.add_function("func", None, None);
    builder.add_struct("Data", None);
    builder.add_type_dependency("func", "Data");

    let graph = builder.build();
    assert_eq!(graph.edge_count(), 1);
}

#[test]
fn test_graph_builder_add_import() {
    let mut builder = GraphBuilder::new("test");
    builder.add_module("mod_a", None);
    builder.add_module("mod_b", None);
    builder.add_import("mod_a", "mod_b");

    let graph = builder.build();
    assert_eq!(graph.edge_count(), 1);
}

#[test]
fn test_graph_builder_add_implements() {
    let mut builder = GraphBuilder::new("test");
    builder.add_struct("MyStruct", None);
    builder.add_trait("MyTrait");
    builder.add_implements("MyStruct", "MyTrait");

    let graph = builder.build();
    assert_eq!(graph.edge_count(), 1);
}

#[test]
fn test_graph_builder_add_contains() {
    let mut builder = GraphBuilder::new("test");
    builder.add_module("mymod", None);
    builder.add_function("myfunc", None, None);
    builder.add_contains("mymod", "myfunc");

    let graph = builder.build();
    assert_eq!(graph.edge_count(), 1);
}

#[test]
fn test_output_format_debug() {
    let format = OutputFormat::Dot;
    let debug = format!("{:?}", format);
    assert!(debug.contains("Dot"));
}

#[test]
fn test_output_format_all_variants() {
    let formats = [
        OutputFormat::Dot,
        OutputFormat::Mermaid,
        OutputFormat::Ascii,
        OutputFormat::Json,
        OutputFormat::PlantUml,
    ];
    // Just verify we can use all formats
    for f in formats {
        let _ = format!("{:?}", f);
    }
}

#[test]
fn test_node_metrics_debug() {
    let metrics = NodeMetrics {
        in_degree: 2,
        out_degree: 3,
        total_degree: 5,
        centrality: 0.5,
    };
    let debug = format!("{:?}", metrics);
    assert!(debug.contains("NodeMetrics"));
}

#[test]
fn test_graph_renderer_builder_pattern() {
    let renderer = GraphRenderer::new().with_direction("TB").cluster();

    assert_eq!(renderer.direction, "TB");
    assert!(renderer.cluster_by_file);
}

#[test]
fn test_graph_builder_default() {
    let builder = GraphBuilder::default();
    let graph = builder.build();
    assert_eq!(graph.node_count(), 0);
}

#[test]
fn test_code_graph_default() {
    let graph = CodeGraph::default();
    assert_eq!(graph.node_count(), 0);
}

#[test]
fn test_code_graph_empty() {
    let graph = CodeGraph::new("test");
    assert_eq!(graph.node_count(), 0);
    assert_eq!(graph.edge_count(), 0);
}

#[test]
fn test_edge_type_dot_style() {
    assert_eq!(EdgeType::Calls.dot_style(), "solid");
    assert_eq!(EdgeType::Imports.dot_style(), "dashed");
    assert_eq!(EdgeType::Contains.dot_style(), "dotted");
    assert_eq!(EdgeType::Implements.dot_style(), "bold");
}

#[test]
fn test_edge_type_mermaid_arrow() {
    assert_eq!(EdgeType::Calls.mermaid_arrow(), "-->");
    assert_eq!(EdgeType::Imports.mermaid_arrow(), "-.->");
    assert_eq!(EdgeType::Implements.mermaid_arrow(), "-.->");
}

#[test]
fn test_graph_builder_graph() {
    let mut builder = GraphBuilder::new("test");
    builder.add_function("func", None, None);

    // Access graph without consuming builder
    let graph = builder.graph();
    assert_eq!(graph.node_count(), 1);

    // Can still use builder
    builder.add_struct("Data", None);
    let final_graph = builder.build();
    assert_eq!(final_graph.node_count(), 2);
}

// ---- Streaming render tests ----

fn sample_graph() -> CodeGraph {
    let mut graph = CodeGraph::new("streaming_test");
    graph.add_node(GraphNode::new("main", NodeType::Function).in_file("src/main.rs"));
    graph.add_node(GraphNode::new("Config", NodeType::Struct).in_file("src/config.rs"));
    graph.connect("main", "Config", EdgeType::Uses);
    graph
}

#[test]
fn test_render_to_matches_render_dot() {
    let graph = sample_graph();
    let renderer = GraphRenderer::new();
    let rendered = renderer.render(&graph, OutputFormat::Dot);
    let mut buf = Vec::new();
    renderer
        .render_to(&graph, OutputFormat::Dot, &mut buf)
        .unwrap();
    assert_eq!(rendered, String::from_utf8(buf).unwrap());
}

#[test]
fn test_render_to_matches_render_mermaid() {
    let graph = sample_graph();
    let renderer = GraphRenderer::new();
    let rendered = renderer.render(&graph, OutputFormat::Mermaid);
    let mut buf = Vec::new();
    renderer
        .render_to(&graph, OutputFormat::Mermaid, &mut buf)
        .unwrap();
    assert_eq!(rendered, String::from_utf8(buf).unwrap());
}

#[test]
fn test_render_to_matches_render_ascii() {
    let graph = sample_graph();
    let renderer = GraphRenderer::new();
    let rendered = renderer.render(&graph, OutputFormat::Ascii);
    let mut buf = Vec::new();
    renderer
        .render_to(&graph, OutputFormat::Ascii, &mut buf)
        .unwrap();
    assert_eq!(rendered, String::from_utf8(buf).unwrap());
}

#[test]
fn test_render_to_matches_render_json() {
    let graph = sample_graph();
    let renderer = GraphRenderer::new();
    let rendered = renderer.render(&graph, OutputFormat::Json);
    let mut buf = Vec::new();
    renderer
        .render_to(&graph, OutputFormat::Json, &mut buf)
        .unwrap();
    assert_eq!(rendered, String::from_utf8(buf).unwrap());
}

#[test]
fn test_render_to_matches_render_plantuml() {
    let graph = sample_graph();
    let renderer = GraphRenderer::new();
    let rendered = renderer.render(&graph, OutputFormat::PlantUml);
    let mut buf = Vec::new();
    renderer
        .render_to(&graph, OutputFormat::PlantUml, &mut buf)
        .unwrap();
    assert_eq!(rendered, String::from_utf8(buf).unwrap());
}
