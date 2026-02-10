//! Knowledge Graph Tools
//!
//! Tools for building and querying a knowledge graph of code entities,
//! relationships, and facts discovered during analysis.


use anyhow::{Context, Result};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use tokio::sync::RwLock;

use super::Tool;

// ============================================================================
// Knowledge Graph Data Structures
// ============================================================================

/// A node in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeNode {
    pub id: String,
    pub node_type: NodeType,
    pub name: String,
    pub description: Option<String>,
    pub properties: HashMap<String, String>,
    pub file_path: Option<String>,
    pub line_number: Option<u32>,
    pub created_at: String,
}

/// Types of nodes in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    Function,
    Struct,
    Enum,
    Trait,
    Module,
    File,
    Crate,
    Test,
    Concept,
    Fact,
    Todo,
    Bug,
    Feature,
    Custom(String),
}

impl std::fmt::Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeType::Custom(s) => write!(f, "{}", s),
            _ => write!(f, "{:?}", self).map(|_| ()).map(|_| ()),
        }
    }
}

/// A relationship between nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEdge {
    pub from_id: String,
    pub to_id: String,
    pub relation: RelationType,
    pub properties: HashMap<String, String>,
    pub created_at: String,
}

/// Types of relationships
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    // Code relationships
    Calls,
    CalledBy,
    Uses,
    UsedBy,
    Implements,
    ImplementedBy,
    Extends,
    ExtendedBy,
    Contains,
    ContainedIn,
    Imports,
    ImportedBy,
    DependsOn,
    DependencyOf,
    Tests,
    TestedBy,

    // Semantic relationships
    RelatedTo,
    SimilarTo,
    Explains,
    ExplainedBy,
    FixedBy,
    Fixes,
    CausedBy,
    Causes,

    // Custom
    Custom(String),
}

/// The in-memory knowledge graph
#[derive(Debug, Default)]
pub struct KnowledgeGraph {
    nodes: HashMap<String, KnowledgeNode>,
    edges: Vec<KnowledgeEdge>,
    node_index_by_type: HashMap<NodeType, HashSet<String>>,
    node_index_by_name: HashMap<String, HashSet<String>>,
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a node to the graph
    pub fn add_node(&mut self, node: KnowledgeNode) -> String {
        let id = node.id.clone();

        // Update indexes
        self.node_index_by_type
            .entry(node.node_type.clone())
            .or_default()
            .insert(id.clone());

        self.node_index_by_name
            .entry(node.name.to_lowercase())
            .or_default()
            .insert(id.clone());

        self.nodes.insert(id.clone(), node);
        id
    }

    /// Add an edge to the graph
    pub fn add_edge(&mut self, edge: KnowledgeEdge) {
        self.edges.push(edge);
    }

    /// Get a node by ID
    pub fn get_node(&self, id: &str) -> Option<&KnowledgeNode> {
        self.nodes.get(id)
    }

    /// Find nodes by type
    pub fn find_by_type(&self, node_type: &NodeType) -> Vec<&KnowledgeNode> {
        self.node_index_by_type
            .get(node_type)
            .map(|ids| ids.iter().filter_map(|id| self.nodes.get(id)).collect())
            .unwrap_or_default()
    }

    /// Find nodes by name (case-insensitive partial match)
    pub fn find_by_name(&self, name: &str) -> Vec<&KnowledgeNode> {
        let name_lower = name.to_lowercase();
        self.nodes
            .values()
            .filter(|node| node.name.to_lowercase().contains(&name_lower))
            .collect()
    }

    /// Find edges from a node
    pub fn edges_from(&self, node_id: &str) -> Vec<&KnowledgeEdge> {
        self.edges.iter().filter(|e| e.from_id == node_id).collect()
    }

    /// Find edges to a node
    pub fn edges_to(&self, node_id: &str) -> Vec<&KnowledgeEdge> {
        self.edges.iter().filter(|e| e.to_id == node_id).collect()
    }

    /// Get all nodes
    pub fn all_nodes(&self) -> Vec<&KnowledgeNode> {
        self.nodes.values().collect()
    }

    /// Get statistics
    pub fn stats(&self) -> GraphStats {
        let mut type_counts: HashMap<String, usize> = HashMap::new();
        for node in self.nodes.values() {
            *type_counts
                .entry(format!("{:?}", node.node_type))
                .or_default() += 1;
        }

        GraphStats {
            total_nodes: self.nodes.len(),
            total_edges: self.edges.len(),
            nodes_by_type: type_counts,
        }
    }

    /// Clear the graph
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.edges.clear();
        self.node_index_by_type.clear();
        self.node_index_by_name.clear();
    }

    /// Remove a node and its edges
    pub fn remove_node(&mut self, id: &str) -> Option<KnowledgeNode> {
        if let Some(node) = self.nodes.remove(id) {
            // Remove from indexes
            if let Some(ids) = self.node_index_by_type.get_mut(&node.node_type) {
                ids.remove(id);
            }
            if let Some(ids) = self.node_index_by_name.get_mut(&node.name.to_lowercase()) {
                ids.remove(id);
            }

            // Remove edges
            self.edges.retain(|e| e.from_id != id && e.to_id != id);

            Some(node)
        } else {
            None
        }
    }
}

#[derive(Debug, Serialize)]
pub struct GraphStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub nodes_by_type: HashMap<String, usize>,
}

// Global knowledge graph instance
static KNOWLEDGE_GRAPH: Lazy<RwLock<KnowledgeGraph>> =
    Lazy::new(|| RwLock::new(KnowledgeGraph::new()));

// ============================================================================
// Knowledge Add Tool
// ============================================================================

/// Add a node to the knowledge graph
pub struct KnowledgeAdd;

#[async_trait]
impl Tool for KnowledgeAdd {
    fn name(&self) -> &str {
        "knowledge_add"
    }

    fn description(&self) -> &str {
        "Add a node (entity, fact, concept) to the knowledge graph"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name of the entity"
                },
                "node_type": {
                    "type": "string",
                    "enum": ["function", "struct", "enum", "trait", "module", "file", "crate", "test", "concept", "fact", "todo", "bug", "feature"],
                    "description": "Type of node"
                },
                "description": {
                    "type": "string",
                    "description": "Description of the entity"
                },
                "properties": {
                    "type": "object",
                    "description": "Additional properties (key-value pairs)"
                },
                "file_path": {
                    "type": "string",
                    "description": "Source file path (for code entities)"
                },
                "line_number": {
                    "type": "integer",
                    "description": "Line number in source file"
                }
            },
            "required": ["name", "node_type"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("name is required"))?;

        let node_type_str = args
            .get("node_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("node_type is required"))?;

        let node_type = parse_node_type(node_type_str);

        let properties: HashMap<String, String> = args
            .get("properties")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        let id = generate_id(name, &node_type);

        let node = KnowledgeNode {
            id: id.clone(),
            node_type: node_type.clone(),
            name: name.to_string(),
            description: args
                .get("description")
                .and_then(|v| v.as_str())
                .map(String::from),
            properties,
            file_path: args
                .get("file_path")
                .and_then(|v| v.as_str())
                .map(String::from),
            line_number: args
                .get("line_number")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32),
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        let mut graph = KNOWLEDGE_GRAPH.write().await;
        graph.add_node(node.clone());

        Ok(json!({
            "success": true,
            "id": id,
            "name": name,
            "node_type": format!("{:?}", node_type),
            "message": format!("Added {} '{}' to knowledge graph", node_type_str, name)
        }))
    }
}

// ============================================================================
// Knowledge Relate Tool
// ============================================================================

/// Create a relationship between nodes
pub struct KnowledgeRelate;

#[async_trait]
impl Tool for KnowledgeRelate {
    fn name(&self) -> &str {
        "knowledge_relate"
    }

    fn description(&self) -> &str {
        "Create a relationship between two nodes in the knowledge graph"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "from": {
                    "type": "string",
                    "description": "ID or name of the source node"
                },
                "to": {
                    "type": "string",
                    "description": "ID or name of the target node"
                },
                "relation": {
                    "type": "string",
                    "enum": ["calls", "called_by", "uses", "used_by", "implements", "implemented_by",
                             "extends", "extended_by", "contains", "contained_in", "imports", "imported_by",
                             "depends_on", "dependency_of", "tests", "tested_by", "related_to", "similar_to",
                             "explains", "explained_by", "fixed_by", "fixes", "caused_by", "causes"],
                    "description": "Type of relationship"
                },
                "properties": {
                    "type": "object",
                    "description": "Additional properties for the relationship"
                }
            },
            "required": ["from", "to", "relation"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let from = args
            .get("from")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("from is required"))?;

        let to = args
            .get("to")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("to is required"))?;

        let relation_str = args
            .get("relation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("relation is required"))?;

        let relation = parse_relation_type(relation_str);

        let properties: HashMap<String, String> = args
            .get("properties")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        let mut graph = KNOWLEDGE_GRAPH.write().await;

        // Resolve node IDs (by ID or by name)
        let from_id = resolve_node_id(&graph, from)?;
        let to_id = resolve_node_id(&graph, to)?;

        let edge = KnowledgeEdge {
            from_id: from_id.clone(),
            to_id: to_id.clone(),
            relation: relation.clone(),
            properties,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        graph.add_edge(edge);

        Ok(json!({
            "success": true,
            "from_id": from_id,
            "to_id": to_id,
            "relation": format!("{:?}", relation),
            "message": format!("Created relationship: {} --[{}]--> {}", from, relation_str, to)
        }))
    }
}

// ============================================================================
// Knowledge Query Tool
// ============================================================================

/// Query the knowledge graph
pub struct KnowledgeQuery;

#[async_trait]
impl Tool for KnowledgeQuery {
    fn name(&self) -> &str {
        "knowledge_query"
    }

    fn description(&self) -> &str {
        "Query the knowledge graph for nodes and relationships"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "node_id": {
                    "type": "string",
                    "description": "Get a specific node by ID"
                },
                "name": {
                    "type": "string",
                    "description": "Search nodes by name (partial match)"
                },
                "node_type": {
                    "type": "string",
                    "description": "Filter by node type"
                },
                "include_edges": {
                    "type": "boolean",
                    "description": "Include related edges (default: true)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum results to return (default: 50)"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let graph = KNOWLEDGE_GRAPH.read().await;

        let include_edges = args
            .get("include_edges")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

        // Query by ID
        if let Some(node_id) = args.get("node_id").and_then(|v| v.as_str()) {
            if let Some(node) = graph.get_node(node_id) {
                let edges_from: Vec<_> = if include_edges {
                    graph
                        .edges_from(node_id)
                        .into_iter()
                        .map(|e| {
                            json!({
                                "to": e.to_id,
                                "relation": format!("{:?}", e.relation)
                            })
                        })
                        .collect()
                } else {
                    vec![]
                };

                let edges_to: Vec<_> = if include_edges {
                    graph
                        .edges_to(node_id)
                        .into_iter()
                        .map(|e| {
                            json!({
                                "from": e.from_id,
                                "relation": format!("{:?}", e.relation)
                            })
                        })
                        .collect()
                } else {
                    vec![]
                };

                return Ok(json!({
                    "success": true,
                    "node": node,
                    "outgoing_edges": edges_from,
                    "incoming_edges": edges_to
                }));
            } else {
                return Ok(json!({
                    "success": false,
                    "error": format!("Node not found: {}", node_id)
                }));
            }
        }

        // Query by name
        let mut results: Vec<&KnowledgeNode> =
            if let Some(name) = args.get("name").and_then(|v| v.as_str()) {
                graph.find_by_name(name)
            } else {
                graph.all_nodes()
            };

        // Filter by type
        if let Some(type_str) = args.get("node_type").and_then(|v| v.as_str()) {
            let node_type = parse_node_type(type_str);
            results.retain(|n| n.node_type == node_type);
        }

        // Limit results
        results.truncate(limit);

        let nodes_json: Vec<Value> = results.iter().map(|n| json!(n)).collect();

        Ok(json!({
            "success": true,
            "nodes": nodes_json,
            "count": nodes_json.len(),
            "total_in_graph": graph.all_nodes().len()
        }))
    }
}

// ============================================================================
// Knowledge Stats Tool
// ============================================================================

/// Get statistics about the knowledge graph
pub struct KnowledgeStats;

#[async_trait]
impl Tool for KnowledgeStats {
    fn name(&self) -> &str {
        "knowledge_stats"
    }

    fn description(&self) -> &str {
        "Get statistics about the knowledge graph"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(&self, _args: Value) -> Result<Value> {
        let graph = KNOWLEDGE_GRAPH.read().await;
        let stats = graph.stats();

        Ok(json!({
            "success": true,
            "total_nodes": stats.total_nodes,
            "total_edges": stats.total_edges,
            "nodes_by_type": stats.nodes_by_type
        }))
    }
}

// ============================================================================
// Knowledge Clear Tool
// ============================================================================

/// Clear the knowledge graph
pub struct KnowledgeClear;

#[async_trait]
impl Tool for KnowledgeClear {
    fn name(&self) -> &str {
        "knowledge_clear"
    }

    fn description(&self) -> &str {
        "Clear all nodes and edges from the knowledge graph"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "confirm": {
                    "type": "boolean",
                    "description": "Must be true to confirm clearing"
                }
            },
            "required": ["confirm"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let confirm = args
            .get("confirm")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !confirm {
            return Ok(json!({
                "success": false,
                "error": "Must set confirm: true to clear the knowledge graph"
            }));
        }

        let mut graph = KNOWLEDGE_GRAPH.write().await;
        let old_stats = graph.stats();
        graph.clear();

        Ok(json!({
            "success": true,
            "message": "Knowledge graph cleared",
            "cleared_nodes": old_stats.total_nodes,
            "cleared_edges": old_stats.total_edges
        }))
    }
}

// ============================================================================
// Knowledge Remove Tool
// ============================================================================

/// Remove a node from the knowledge graph
pub struct KnowledgeRemove;

#[async_trait]
impl Tool for KnowledgeRemove {
    fn name(&self) -> &str {
        "knowledge_remove"
    }

    fn description(&self) -> &str {
        "Remove a node and its edges from the knowledge graph"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "node_id": {
                    "type": "string",
                    "description": "ID of the node to remove"
                }
            },
            "required": ["node_id"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let node_id = args
            .get("node_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("node_id is required"))?;

        let mut graph = KNOWLEDGE_GRAPH.write().await;

        if let Some(removed) = graph.remove_node(node_id) {
            Ok(json!({
                "success": true,
                "removed": removed,
                "message": format!("Removed node: {}", node_id)
            }))
        } else {
            Ok(json!({
                "success": false,
                "error": format!("Node not found: {}", node_id)
            }))
        }
    }
}

// ============================================================================
// Knowledge Export Tool
// ============================================================================

/// Export the knowledge graph to JSON
pub struct KnowledgeExport;

#[async_trait]
impl Tool for KnowledgeExport {
    fn name(&self) -> &str {
        "knowledge_export"
    }

    fn description(&self) -> &str {
        "Export the knowledge graph to a JSON file"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "output_path": {
                    "type": "string",
                    "description": "Path to save the JSON file"
                }
            },
            "required": ["output_path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let output_path = args
            .get("output_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("output_path is required"))?;

        let graph = KNOWLEDGE_GRAPH.read().await;

        let export_data = json!({
            "nodes": graph.all_nodes(),
            "edges": graph.edges.iter().collect::<Vec<_>>(),
            "exported_at": chrono::Utc::now().to_rfc3339()
        });

        let json_str = serde_json::to_string_pretty(&export_data)?;
        tokio::fs::write(output_path, &json_str)
            .await
            .context("Failed to write export file")?;

        Ok(json!({
            "success": true,
            "output_path": output_path,
            "nodes_exported": graph.all_nodes().len(),
            "edges_exported": graph.edges.len()
        }))
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn generate_id(name: &str, node_type: &NodeType) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(format!("{:?}:{}", node_type, name));
    let hash = hasher.finalize();
    format!("{:x}", hash)[..12].to_string()
}

fn parse_node_type(s: &str) -> NodeType {
    match s.to_lowercase().as_str() {
        "function" => NodeType::Function,
        "struct" => NodeType::Struct,
        "enum" => NodeType::Enum,
        "trait" => NodeType::Trait,
        "module" => NodeType::Module,
        "file" => NodeType::File,
        "crate" => NodeType::Crate,
        "test" => NodeType::Test,
        "concept" => NodeType::Concept,
        "fact" => NodeType::Fact,
        "todo" => NodeType::Todo,
        "bug" => NodeType::Bug,
        "feature" => NodeType::Feature,
        other => NodeType::Custom(other.to_string()),
    }
}

fn parse_relation_type(s: &str) -> RelationType {
    match s.to_lowercase().as_str() {
        "calls" => RelationType::Calls,
        "called_by" => RelationType::CalledBy,
        "uses" => RelationType::Uses,
        "used_by" => RelationType::UsedBy,
        "implements" => RelationType::Implements,
        "implemented_by" => RelationType::ImplementedBy,
        "extends" => RelationType::Extends,
        "extended_by" => RelationType::ExtendedBy,
        "contains" => RelationType::Contains,
        "contained_in" => RelationType::ContainedIn,
        "imports" => RelationType::Imports,
        "imported_by" => RelationType::ImportedBy,
        "depends_on" => RelationType::DependsOn,
        "dependency_of" => RelationType::DependencyOf,
        "tests" => RelationType::Tests,
        "tested_by" => RelationType::TestedBy,
        "related_to" => RelationType::RelatedTo,
        "similar_to" => RelationType::SimilarTo,
        "explains" => RelationType::Explains,
        "explained_by" => RelationType::ExplainedBy,
        "fixed_by" => RelationType::FixedBy,
        "fixes" => RelationType::Fixes,
        "caused_by" => RelationType::CausedBy,
        "causes" => RelationType::Causes,
        other => RelationType::Custom(other.to_string()),
    }
}

fn resolve_node_id(graph: &KnowledgeGraph, id_or_name: &str) -> Result<String> {
    // First try as ID
    if graph.get_node(id_or_name).is_some() {
        return Ok(id_or_name.to_string());
    }

    // Try to find by name
    let matches = graph.find_by_name(id_or_name);
    if matches.len() == 1 {
        return Ok(matches[0].id.clone());
    } else if matches.len() > 1 {
        return Err(anyhow::anyhow!(
            "Ambiguous name '{}': found {} matches. Use node ID instead.",
            id_or_name,
            matches.len()
        ));
    }

    Err(anyhow::anyhow!("Node not found: {}", id_or_name))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
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
        let result = tool.execute(json!({})).await.unwrap();
        assert_eq!(result["success"], false);
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
}
