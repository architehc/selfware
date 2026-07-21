use super::*;

#[test]
fn test_knowledge_graph_new() {
    let graph = KnowledgeGraph::new();
    assert_eq!(graph.all_nodes().len(), 0);
}

#[test]
fn test_knowledge_graph_add_node() {
    let mut graph = KnowledgeGraph::new();
    let node = KnowledgeNode {
        id: "test1".to_string(),
        node_type: NodeType::Function,
        name: "test_function".to_string(),
        description: Some("A test function".to_string()),
        properties: HashMap::new(),
        file_path: Some("src/lib.rs".to_string()),
        line_number: Some(42),
        created_at: "2024-01-01".to_string(),
    };

    graph.add_node(node);
    assert_eq!(graph.all_nodes().len(), 1);
    assert!(graph.get_node("test1").is_some());
}

#[test]
fn test_knowledge_graph_find_by_type() {
    let mut graph = KnowledgeGraph::new();

    graph.add_node(KnowledgeNode {
        id: "f1".to_string(),
        node_type: NodeType::Function,
        name: "func1".to_string(),
        description: None,
        properties: HashMap::new(),
        file_path: None,
        line_number: None,
        created_at: "2024-01-01".to_string(),
    });

    graph.add_node(KnowledgeNode {
        id: "s1".to_string(),
        node_type: NodeType::Struct,
        name: "struct1".to_string(),
        description: None,
        properties: HashMap::new(),
        file_path: None,
        line_number: None,
        created_at: "2024-01-01".to_string(),
    });

    let functions = graph.find_by_type(&NodeType::Function);
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0].name, "func1");
}

#[test]
fn test_knowledge_graph_find_by_name() {
    let mut graph = KnowledgeGraph::new();

    graph.add_node(KnowledgeNode {
        id: "f1".to_string(),
        node_type: NodeType::Function,
        name: "my_function".to_string(),
        description: None,
        properties: HashMap::new(),
        file_path: None,
        line_number: None,
        created_at: "2024-01-01".to_string(),
    });

    let results = graph.find_by_name("function");
    assert_eq!(results.len(), 1);

    let results = graph.find_by_name("MY_FUNC");
    assert_eq!(results.len(), 1);

    let results = graph.find_by_name("nonexistent");
    assert_eq!(results.len(), 0);
}

#[test]
fn test_knowledge_graph_edges() {
    let mut graph = KnowledgeGraph::new();

    graph.add_node(KnowledgeNode {
        id: "f1".to_string(),
        node_type: NodeType::Function,
        name: "caller".to_string(),
        description: None,
        properties: HashMap::new(),
        file_path: None,
        line_number: None,
        created_at: "2024-01-01".to_string(),
    });

    graph.add_node(KnowledgeNode {
        id: "f2".to_string(),
        node_type: NodeType::Function,
        name: "callee".to_string(),
        description: None,
        properties: HashMap::new(),
        file_path: None,
        line_number: None,
        created_at: "2024-01-01".to_string(),
    });

    graph.add_edge(KnowledgeEdge {
        from_id: "f1".to_string(),
        to_id: "f2".to_string(),
        relation: RelationType::Calls,
        properties: HashMap::new(),
        created_at: "2024-01-01".to_string(),
    });

    let edges_from_f1 = graph.edges_from("f1");
    assert_eq!(edges_from_f1.len(), 1);

    let edges_to_f2 = graph.edges_to("f2");
    assert_eq!(edges_to_f2.len(), 1);
}

#[test]
fn test_knowledge_graph_remove_node() {
    let mut graph = KnowledgeGraph::new();

    graph.add_node(KnowledgeNode {
        id: "f1".to_string(),
        node_type: NodeType::Function,
        name: "test".to_string(),
        description: None,
        properties: HashMap::new(),
        file_path: None,
        line_number: None,
        created_at: "2024-01-01".to_string(),
    });

    assert!(graph.remove_node("f1").is_some());
    assert!(graph.get_node("f1").is_none());
    assert!(graph.remove_node("f1").is_none());
}

#[test]
fn test_tool_names() {
    assert_eq!(KnowledgeAdd.name(), "knowledge_add");
    assert_eq!(KnowledgeRelate.name(), "knowledge_relate");
    assert_eq!(KnowledgeQuery.name(), "knowledge_query");
    assert_eq!(KnowledgeStats.name(), "knowledge_stats");
    assert_eq!(KnowledgeClear.name(), "knowledge_clear");
    assert_eq!(KnowledgeRemove.name(), "knowledge_remove");
    assert_eq!(KnowledgeExport.name(), "knowledge_export");
}

#[test]
fn test_parse_node_type() {
    assert_eq!(parse_node_type("function"), NodeType::Function);
    assert_eq!(parse_node_type("STRUCT"), NodeType::Struct);
    assert_eq!(
        parse_node_type("custom_type"),
        NodeType::Custom("custom_type".to_string())
    );
}

#[test]
fn test_parse_relation_type() {
    assert_eq!(parse_relation_type("calls"), RelationType::Calls);
    assert_eq!(parse_relation_type("USES"), RelationType::Uses);
    assert_eq!(
        parse_relation_type("custom_rel"),
        RelationType::Custom("custom_rel".to_string())
    );
}

#[test]
fn test_generate_id() {
    let id1 = generate_id("test", &NodeType::Function);
    let id2 = generate_id("test", &NodeType::Function);
    assert_eq!(id1, id2);

    let id3 = generate_id("test", &NodeType::Struct);
    assert_ne!(id1, id3);
}

#[tokio::test]
async fn test_knowledge_add_no_name() {
    let tool = KnowledgeAdd;
    let result = tool.execute(json!({"node_type": "function"})).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_knowledge_clear_no_confirm() {
    let tool = KnowledgeClear;
    let result = tool.execute(json!({})).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Must set confirm"));
}

// Additional comprehensive tests

#[test]
fn test_node_type_display() {
    assert!(!format!("{}", NodeType::Function).is_empty());
    assert!(!format!("{}", NodeType::Struct).is_empty());
    assert!(!format!("{}", NodeType::Custom("MyType".into())).is_empty());
}

#[test]
fn test_node_type_equality() {
    assert_eq!(NodeType::Function, NodeType::Function);
    assert_ne!(NodeType::Function, NodeType::Struct);
    assert_eq!(
        NodeType::Custom("test".into()),
        NodeType::Custom("test".into())
    );
}

#[test]
fn test_relation_type_equality() {
    assert_eq!(RelationType::Calls, RelationType::Calls);
    assert_ne!(RelationType::Calls, RelationType::CalledBy);
    assert_eq!(
        RelationType::Custom("rel".into()),
        RelationType::Custom("rel".into())
    );
}

#[test]
fn test_knowledge_node_clone() {
    let node = KnowledgeNode {
        id: "n1".into(),
        node_type: NodeType::Function,
        name: "test".into(),
        description: Some("desc".into()),
        properties: HashMap::new(),
        file_path: Some("src/lib.rs".into()),
        line_number: Some(10),
        created_at: "2024-01-01".into(),
    };
    let cloned = node.clone();
    assert_eq!(node.id, cloned.id);
    assert_eq!(node.name, cloned.name);
}

#[test]
fn test_knowledge_edge_clone() {
    let edge = KnowledgeEdge {
        from_id: "a".into(),
        to_id: "b".into(),
        relation: RelationType::Calls,
        properties: HashMap::new(),
        created_at: "2024-01-01".into(),
    };
    let cloned = edge.clone();
    assert_eq!(edge.from_id, cloned.from_id);
    assert_eq!(edge.to_id, cloned.to_id);
}

#[test]
fn test_graph_stats() {
    let mut graph = KnowledgeGraph::new();

    graph.add_node(KnowledgeNode {
        id: "f1".into(),
        node_type: NodeType::Function,
        name: "func1".into(),
        description: None,
        properties: HashMap::new(),
        file_path: None,
        line_number: None,
        created_at: "2024-01-01".into(),
    });

    graph.add_node(KnowledgeNode {
        id: "f2".into(),
        node_type: NodeType::Function,
        name: "func2".into(),
        description: None,
        properties: HashMap::new(),
        file_path: None,
        line_number: None,
        created_at: "2024-01-01".into(),
    });

    graph.add_edge(KnowledgeEdge {
        from_id: "f1".into(),
        to_id: "f2".into(),
        relation: RelationType::Calls,
        properties: HashMap::new(),
        created_at: "2024-01-01".into(),
    });

    let stats = graph.stats();
    assert_eq!(stats.total_nodes, 2);
    assert_eq!(stats.total_edges, 1);
}

#[test]
fn test_graph_clear() {
    let mut graph = KnowledgeGraph::new();

    graph.add_node(KnowledgeNode {
        id: "n1".into(),
        node_type: NodeType::Concept,
        name: "concept".into(),
        description: None,
        properties: HashMap::new(),
        file_path: None,
        line_number: None,
        created_at: "2024-01-01".into(),
    });

    assert_eq!(graph.all_nodes().len(), 1);

    graph.clear();

    assert_eq!(graph.all_nodes().len(), 0);
}

#[test]
fn test_remove_node_with_edges() {
    let mut graph = KnowledgeGraph::new();

    graph.add_node(KnowledgeNode {
        id: "a".into(),
        node_type: NodeType::Function,
        name: "a".into(),
        description: None,
        properties: HashMap::new(),
        file_path: None,
        line_number: None,
        created_at: "2024-01-01".into(),
    });

    graph.add_node(KnowledgeNode {
        id: "b".into(),
        node_type: NodeType::Function,
        name: "b".into(),
        description: None,
        properties: HashMap::new(),
        file_path: None,
        line_number: None,
        created_at: "2024-01-01".into(),
    });

    graph.add_edge(KnowledgeEdge {
        from_id: "a".into(),
        to_id: "b".into(),
        relation: RelationType::Calls,
        properties: HashMap::new(),
        created_at: "2024-01-01".into(),
    });

    // Remove node A, should also remove the edge
    graph.remove_node("a");

    assert!(graph.get_node("a").is_none());
    assert!(graph.edges_from("a").is_empty());
    assert!(graph.edges_to("b").is_empty());
}

#[test]
fn test_all_node_types() {
    let types = vec![
        ("function", NodeType::Function),
        ("struct", NodeType::Struct),
        ("enum", NodeType::Enum),
        ("trait", NodeType::Trait),
        ("module", NodeType::Module),
        ("file", NodeType::File),
        ("crate", NodeType::Crate),
        ("test", NodeType::Test),
        ("concept", NodeType::Concept),
        ("fact", NodeType::Fact),
        ("todo", NodeType::Todo),
        ("bug", NodeType::Bug),
        ("feature", NodeType::Feature),
    ];

    for (s, expected) in types {
        assert_eq!(parse_node_type(s), expected);
    }
}

#[test]
fn test_all_relation_types() {
    let types = vec![
        ("calls", RelationType::Calls),
        ("called_by", RelationType::CalledBy),
        ("uses", RelationType::Uses),
        ("used_by", RelationType::UsedBy),
        ("implements", RelationType::Implements),
        ("implemented_by", RelationType::ImplementedBy),
        ("extends", RelationType::Extends),
        ("extended_by", RelationType::ExtendedBy),
        ("contains", RelationType::Contains),
        ("contained_in", RelationType::ContainedIn),
        ("imports", RelationType::Imports),
        ("imported_by", RelationType::ImportedBy),
        ("depends_on", RelationType::DependsOn),
        ("dependency_of", RelationType::DependencyOf),
        ("tests", RelationType::Tests),
        ("tested_by", RelationType::TestedBy),
        ("related_to", RelationType::RelatedTo),
        ("similar_to", RelationType::SimilarTo),
        ("explains", RelationType::Explains),
        ("explained_by", RelationType::ExplainedBy),
        ("fixed_by", RelationType::FixedBy),
        ("fixes", RelationType::Fixes),
        ("caused_by", RelationType::CausedBy),
        ("causes", RelationType::Causes),
    ];

    for (s, expected) in types {
        assert_eq!(parse_relation_type(s), expected);
    }
}

#[test]
fn test_generate_id_consistency() {
    // Same inputs should produce same ID
    let id1 = generate_id("myFunc", &NodeType::Function);
    let id2 = generate_id("myFunc", &NodeType::Function);
    assert_eq!(id1, id2);

    // Different inputs should produce different IDs
    let id3 = generate_id("myFunc", &NodeType::Struct);
    assert_ne!(id1, id3);

    let id4 = generate_id("otherFunc", &NodeType::Function);
    assert_ne!(id1, id4);
}

#[test]
fn test_graph_default() {
    let graph = KnowledgeGraph::default();
    assert!(graph.all_nodes().is_empty());
}

#[test]
fn test_tool_descriptions() {
    assert!(!KnowledgeAdd.description().is_empty());
    assert!(!KnowledgeRelate.description().is_empty());
    assert!(!KnowledgeQuery.description().is_empty());
    assert!(!KnowledgeStats.description().is_empty());
    assert!(!KnowledgeClear.description().is_empty());
    assert!(!KnowledgeRemove.description().is_empty());
    assert!(!KnowledgeExport.description().is_empty());
}

#[test]
fn test_tool_schemas() {
    let add_schema = KnowledgeAdd.schema();
    assert!(add_schema.is_object());

    let relate_schema = KnowledgeRelate.schema();
    assert!(relate_schema.is_object());

    let query_schema = KnowledgeQuery.schema();
    assert!(query_schema.is_object());
}

#[test]
fn test_node_type_serialization() {
    let node_type = NodeType::Function;
    let json = serde_json::to_string(&node_type).unwrap();
    let deserialized: NodeType = serde_json::from_str(&json).unwrap();
    assert_eq!(node_type, deserialized);
}

#[test]
fn test_relation_type_serialization() {
    let relation = RelationType::Calls;
    let json = serde_json::to_string(&relation).unwrap();
    let deserialized: RelationType = serde_json::from_str(&json).unwrap();
    assert_eq!(relation, deserialized);
}

#[test]
fn test_knowledge_node_with_properties() {
    let mut props = HashMap::new();
    props.insert("visibility".into(), "public".into());
    props.insert("async".into(), "true".into());

    let node = KnowledgeNode {
        id: "n1".into(),
        node_type: NodeType::Function,
        name: "async_fn".into(),
        description: Some("An async function".into()),
        properties: props,
        file_path: Some("src/lib.rs".into()),
        line_number: Some(42),
        created_at: "2024-01-01".into(),
    };

    assert_eq!(
        node.properties.get("visibility"),
        Some(&"public".to_string())
    );
    assert_eq!(node.properties.len(), 2);
}

#[test]
fn test_edge_with_properties() {
    let mut props = HashMap::new();
    props.insert("weight".into(), "1.0".into());

    let edge = KnowledgeEdge {
        from_id: "a".into(),
        to_id: "b".into(),
        relation: RelationType::DependsOn,
        properties: props,
        created_at: "2024-01-01".into(),
    };

    assert_eq!(edge.properties.get("weight"), Some(&"1.0".to_string()));
}

#[test]
fn test_graph_stats_serialization() {
    let mut nodes_by_type = HashMap::new();
    nodes_by_type.insert("Function".into(), 5);
    nodes_by_type.insert("Struct".into(), 3);

    let stats = GraphStats {
        total_nodes: 8,
        total_edges: 10,
        nodes_by_type,
    };

    let json = serde_json::to_string(&stats).unwrap();
    assert!(json.contains("total_nodes"));
    assert!(json.contains("8"));
}

#[test]
fn test_find_by_name_case_insensitive() {
    let mut graph = KnowledgeGraph::new();

    graph.add_node(KnowledgeNode {
        id: "n1".into(),
        node_type: NodeType::Function,
        name: "MyFunction".into(),
        description: None,
        properties: HashMap::new(),
        file_path: None,
        line_number: None,
        created_at: "2024-01-01".into(),
    });

    // Should find with different cases
    assert_eq!(graph.find_by_name("myfunction").len(), 1);
    assert_eq!(graph.find_by_name("MYFUNCTION").len(), 1);
    assert_eq!(graph.find_by_name("MyFunction").len(), 1);
    assert_eq!(graph.find_by_name("myfunc").len(), 1);
}

#[test]
fn test_find_by_type_empty() {
    let graph = KnowledgeGraph::new();
    let results = graph.find_by_type(&NodeType::Function);
    assert!(results.is_empty());
}

#[test]
fn test_edges_from_nonexistent() {
    let graph = KnowledgeGraph::new();
    let edges = graph.edges_from("nonexistent");
    assert!(edges.is_empty());
}

#[test]
fn test_edges_to_nonexistent() {
    let graph = KnowledgeGraph::new();
    let edges = graph.edges_to("nonexistent");
    assert!(edges.is_empty());
}

#[test]
fn test_remove_nonexistent_node() {
    let mut graph = KnowledgeGraph::new();
    let result = graph.remove_node("nonexistent");
    assert!(result.is_none());
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
fn test_relation_type_hash() {
    use std::collections::HashSet;

    let mut set = HashSet::new();
    set.insert(RelationType::Calls);
    set.insert(RelationType::Uses);
    set.insert(RelationType::Calls); // Duplicate

    assert_eq!(set.len(), 2);
}

#[tokio::test]
async fn test_knowledge_add_no_type() {
    let tool = KnowledgeAdd;
    let result = tool.execute(json!({"name": "test"})).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_knowledge_remove_no_id() {
    let tool = KnowledgeRemove;
    let result = tool.execute(json!({})).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_knowledge_export_no_path() {
    let tool = KnowledgeExport;
    let result = tool.execute(json!({})).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_knowledge_relate_missing_from() {
    let tool = KnowledgeRelate;
    let result = tool.execute(json!({"to": "b", "relation": "calls"})).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_knowledge_relate_missing_to() {
    let tool = KnowledgeRelate;
    let result = tool
        .execute(json!({"from": "a", "relation": "calls"}))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_knowledge_relate_missing_relation() {
    let tool = KnowledgeRelate;
    let result = tool.execute(json!({"from": "a", "to": "b"})).await;
    assert!(result.is_err());
}

#[test]
fn test_custom_node_type() {
    // parse_node_type lowercases the input for custom types
    let parsed = parse_node_type("MyCustomType");
    assert!(matches!(parsed, NodeType::Custom(_)));
    if let NodeType::Custom(s) = parsed {
        assert_eq!(s, "mycustomtype");
    }
}

#[test]
fn test_custom_relation_type() {
    let custom = RelationType::Custom("my_custom_rel".into());
    let parsed = parse_relation_type("my_custom_rel");
    assert_eq!(custom, parsed);
}
