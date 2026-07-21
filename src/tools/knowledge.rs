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
                anyhow::bail!("Node not found: {}", node_id);
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
            anyhow::bail!("Must set confirm: true to clear the knowledge graph");
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

        let removed = graph
            .remove_node(node_id)
            .ok_or_else(|| anyhow::anyhow!("Node not found: {}", node_id))?;

        Ok(json!({
            "success": true,
            "removed": removed,
            "message": format!("Removed node: {}", node_id)
        }))
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

        if output_path.contains("..") || output_path.starts_with('/') {
            anyhow::bail!(
                "Invalid output_path: must be a relative path without traversal components"
            );
        }

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
// Auto-Extract Entities Tool
// ============================================================================

/// Auto-extract entities from a Rust file using LSP
pub struct KnowledgeAutoExtract;

#[async_trait]
impl Tool for KnowledgeAutoExtract {
    fn name(&self) -> &str {
        "knowledge_auto_extract"
    }

    fn description(&self) -> &str {
        "Automatically extract all entities (functions, structs, enums, traits) from a Rust source file and add them to the knowledge graph. Uses LSP to parse the file and detect all symbols."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the Rust source file to extract entities from"
                },
                "add_relations": {
                    "type": "boolean",
                    "description": "Whether to add inferred relationships between entities (default: true)",
                    "default": true
                },
                "dry_run": {
                    "type": "boolean",
                    "description": "If true, return what would be added without actually adding to the graph",
                    "default": false
                }
            },
            "required": ["file_path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("file_path is required"))?;

        let add_relations = args
            .get("add_relations")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let dry_run = args
            .get("dry_run")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Use LSP to get all symbols in the file
        let lsp_symbols = match lsp_document_symbols(file_path).await {
            Ok(symbols) => symbols,
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Failed to get document symbols from '{}': {}",
                    file_path,
                    e
                ));
            }
        };

        let mut added_nodes = Vec::new();
        let mut added_edges = Vec::new();
        let mut symbol_ids: HashMap<String, String> = HashMap::new();

        // Add all symbols as nodes
        for symbol in &lsp_symbols {
            let node_type = parse_symbol_kind(&symbol.kind);
            let node_id = generate_id(&symbol.name, &node_type);

            let node = KnowledgeNode {
                id: node_id.clone(),
                node_type: node_type.clone(),
                name: symbol.name.clone(),
                description: symbol
                    .documentation
                    .clone()
                    .or_else(|| Some(format!("{} at line {}", node_type, symbol.line_number))),
                properties: HashMap::new(),
                file_path: Some(file_path.to_string()),
                line_number: Some(symbol.line_number),
                created_at: chrono::Utc::now().to_rfc3339(),
            };

            if dry_run {
                added_nodes.push(node.clone());
            } else {
                let mut graph = KNOWLEDGE_GRAPH.write().await;
                graph.add_node(node);
                added_nodes.push(KnowledgeNode {
                    id: node_id.clone(),
                    node_type: node_type.clone(),
                    name: symbol.name.clone(),
                    description: None,
                    properties: HashMap::new(),
                    file_path: Some(file_path.to_string()),
                    line_number: Some(symbol.line_number),
                    created_at: chrono::Utc::now().to_rfc3339(),
                });
            }

            symbol_ids.insert(symbol.name.clone(), node_id);
        }

        // Optionally add inferred relationships
        if add_relations && !dry_run {
            // Add "contains" relationships for nested symbols
            for (i, symbol) in lsp_symbols.iter().enumerate() {
                if let Some(parent_symbol) = lsp_symbols.get(i.saturating_sub(1)) {
                    if symbol.indentation > parent_symbol.indentation {
                        let edge = KnowledgeEdge {
                            from_id: symbol_ids[&parent_symbol.name].clone(),
                            to_id: symbol_ids[&symbol.name].clone(),
                            relation: RelationType::Contains,
                            properties: HashMap::new(),
                            created_at: chrono::Utc::now().to_rfc3339(),
                        };

                        let mut graph = KNOWLEDGE_GRAPH.write().await;
                        graph.add_edge(edge);
                        added_edges.push(1);
                    }
                }
            }
        }

        Ok(json!({
            "success": true,
            "file_path": file_path,
            "entities_extracted": added_nodes.len(),
            "relations_added": added_edges.len(),
            "dry_run": dry_run,
            "entities": added_nodes
                .iter()
                .map(|n| json!({"name": n.name, "type": format!("{:?}", n.node_type), "line": n.line_number}))
                .collect::<Vec<_>>()
        }))
    }
}

/// Parse LSP symbol kind to our node type
fn parse_symbol_kind(kind: &str) -> NodeType {
    match kind.to_lowercase().as_str() {
        "function" | "method" => NodeType::Function,
        "struct" | "class" => NodeType::Struct,
        "enum" => NodeType::Enum,
        "trait" | "interface" => NodeType::Trait,
        "module" | "namespace" => NodeType::Module,
        "file" => NodeType::File,
        "test" => NodeType::Test,
        "type" | "type_alias" => NodeType::Custom("type".into()),
        "constant" | "const" => NodeType::Custom("const".into()),
        _ => NodeType::Custom(kind.to_lowercase()),
    }
}

/// LSP document symbols wrapper
async fn lsp_document_symbols(file_path: &str) -> Result<Vec<LSymbol>> {
    // For now, use a simple grep-based approach to find definitions
    // In a full implementation, this would call rust-analyzer or similar
    let content =
        std::fs::read_to_string(file_path).context(format!("Cannot read file: {}", file_path))?;

    let mut symbols = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    for (idx, line) in lines.iter().enumerate() {
        let line_num = (idx + 1) as u32;
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.starts_with("//") || trimmed.is_empty() {
            continue;
        }

        // Detect various Rust definitions
        if let Some(name) = extract_symbol_name(line, "pub fn") {
            symbols.push(LSymbol {
                name,
                kind: "function".to_string(),
                line_number: line_num,
                indentation: line.len() - line.trim_start().len(),
                documentation: None,
            });
        } else if let Some(name) = extract_symbol_name(line, "fn") {
            symbols.push(LSymbol {
                name,
                kind: "function".to_string(),
                line_number: line_num,
                indentation: line.len() - line.trim_start().len(),
                documentation: None,
            });
        } else if let Some(name) = extract_symbol_name(line, "pub struct") {
            symbols.push(LSymbol {
                name,
                kind: "struct".to_string(),
                line_number: line_num,
                indentation: line.len() - line.trim_start().len(),
                documentation: None,
            });
        } else if let Some(name) = extract_symbol_name(line, "struct") {
            symbols.push(LSymbol {
                name,
                kind: "struct".to_string(),
                line_number: line_num,
                indentation: line.len() - line.trim_start().len(),
                documentation: None,
            });
        } else if let Some(name) = extract_symbol_name(line, "pub enum") {
            symbols.push(LSymbol {
                name,
                kind: "enum".to_string(),
                line_number: line_num,
                indentation: line.len() - line.trim_start().len(),
                documentation: None,
            });
        } else if let Some(name) = extract_symbol_name(line, "enum") {
            symbols.push(LSymbol {
                name,
                kind: "enum".to_string(),
                line_number: line_num,
                indentation: line.len() - line.trim_start().len(),
                documentation: None,
            });
        } else if let Some(name) = extract_symbol_name(line, "pub trait") {
            symbols.push(LSymbol {
                name,
                kind: "trait".to_string(),
                line_number: line_num,
                indentation: line.len() - line.trim_start().len(),
                documentation: None,
            });
        } else if let Some(name) = extract_symbol_name(line, "trait") {
            symbols.push(LSymbol {
                name,
                kind: "trait".to_string(),
                line_number: line_num,
                indentation: line.len() - line.trim_start().len(),
                documentation: None,
            });
        } else if let Some(name) = extract_symbol_name(line, "pub mod") {
            symbols.push(LSymbol {
                name,
                kind: "module".to_string(),
                line_number: line_num,
                indentation: line.len() - line.trim_start().len(),
                documentation: None,
            });
        } else if let Some(name) = extract_symbol_name(line, "mod") {
            symbols.push(LSymbol {
                name,
                kind: "module".to_string(),
                line_number: line_num,
                indentation: line.len() - line.trim_start().len(),
                documentation: None,
            });
        } else if let Some(name) = extract_symbol_name(line, "impl") {
            symbols.push(LSymbol {
                name,
                kind: "impl".to_string(),
                line_number: line_num,
                indentation: line.len() - line.trim_start().len(),
                documentation: None,
            });
        }
    }

    Ok(symbols)
}

/// Extract symbol name from a definition line
fn extract_symbol_name(line: &str, prefix: &str) -> Option<String> {
    if let Some(pos) = line.find(prefix) {
        let after_prefix = &line[pos + prefix.len()..];
        let trimmed = after_prefix.trim_start();

        // Extract the identifier (alphanumeric + _)
        if let Some(name_end) = trimmed.find(|c: char| !c.is_alphanumeric() && c != '_') {
            let name = &trimmed[..name_end];
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// LSP symbol representation
#[derive(Debug, Clone)]
struct LSymbol {
    name: String,
    kind: String,
    line_number: u32,
    indentation: usize,
    documentation: Option<String>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[path = "../../tests/unit/tools/knowledge/knowledge_test.rs"]
mod tests;
