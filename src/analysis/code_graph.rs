//! Code Graph Visualization System
//!
//! Interactive dependency graphs, call flow visualization, architecture
//! diagrams from code, live updating during edits.
//!
//! # Features
//!
//! - Dependency graphs at module and function level
//! - Call flow visualization
//! - Architecture diagram generation
//! - Multiple output formats (DOT, Mermaid, ASCII)

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{self, Write as IoWrite};
use std::sync::atomic::{AtomicU64, Ordering};

static NODE_COUNTER: AtomicU64 = AtomicU64::new(0);
static EDGE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Type of code entity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeType {
    /// Source file
    File,
    /// Module
    Module,
    /// Function or method
    Function,
    /// Struct definition
    Struct,
    /// Enum definition
    Enum,
    /// Trait definition
    Trait,
    /// Impl block
    Impl,
    /// Constant
    Const,
    /// Type alias
    TypeAlias,
    /// Macro
    Macro,
    /// Package/crate
    Package,
}

impl NodeType {
    /// Get display string
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeType::File => "file",
            NodeType::Module => "module",
            NodeType::Function => "function",
            NodeType::Struct => "struct",
            NodeType::Enum => "enum",
            NodeType::Trait => "trait",
            NodeType::Impl => "impl",
            NodeType::Const => "const",
            NodeType::TypeAlias => "type",
            NodeType::Macro => "macro",
            NodeType::Package => "package",
        }
    }

    /// Get color for visualization
    pub fn color(&self) -> &'static str {
        match self {
            NodeType::File => "#e8e8e8",
            NodeType::Module => "#b8d4e3",
            NodeType::Function => "#98d8c8",
            NodeType::Struct => "#f7dc6f",
            NodeType::Enum => "#f5b7b1",
            NodeType::Trait => "#d7bde2",
            NodeType::Impl => "#abebc6",
            NodeType::Const => "#fadbd8",
            NodeType::TypeAlias => "#d5dbdb",
            NodeType::Macro => "#f9e79f",
            NodeType::Package => "#85c1e9",
        }
    }

    /// Get shape for DOT
    pub fn dot_shape(&self) -> &'static str {
        match self {
            NodeType::File => "folder",
            NodeType::Module => "component",
            NodeType::Function => "ellipse",
            NodeType::Struct | NodeType::Enum => "box",
            NodeType::Trait => "hexagon",
            NodeType::Impl => "parallelogram",
            NodeType::Package => "box3d",
            _ => "ellipse",
        }
    }
}

/// Type of relationship
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeType {
    /// Function calls another function
    Calls,
    /// Module imports/uses another
    Imports,
    /// Contains (module contains function, etc.)
    Contains,
    /// Implements trait
    Implements,
    /// Extends/inherits
    Extends,
    /// Type dependency (field type, parameter type)
    TypeDependency,
    /// Uses (generic usage)
    Uses,
    /// References
    References,
}

impl EdgeType {
    /// Get label for edge
    pub fn label(&self) -> &'static str {
        match self {
            EdgeType::Calls => "calls",
            EdgeType::Imports => "imports",
            EdgeType::Contains => "contains",
            EdgeType::Implements => "implements",
            EdgeType::Extends => "extends",
            EdgeType::TypeDependency => "depends on",
            EdgeType::Uses => "uses",
            EdgeType::References => "references",
        }
    }

    /// Get line style for DOT
    pub fn dot_style(&self) -> &'static str {
        match self {
            EdgeType::Calls => "solid",
            EdgeType::Imports => "dashed",
            EdgeType::Contains => "dotted",
            EdgeType::Implements => "bold",
            EdgeType::Extends => "bold",
            EdgeType::TypeDependency => "dashed",
            EdgeType::Uses => "solid",
            EdgeType::References => "dotted",
        }
    }

    /// Get arrow type for Mermaid
    pub fn mermaid_arrow(&self) -> &'static str {
        match self {
            EdgeType::Calls => "-->",
            EdgeType::Imports => "-.->",
            EdgeType::Contains => "-->",
            EdgeType::Implements => "-.->",
            EdgeType::Extends => "-->|extends|",
            EdgeType::TypeDependency => "-.->",
            EdgeType::Uses => "-->",
            EdgeType::References => "-.->",
        }
    }
}

/// A node in the code graph
#[derive(Debug, Clone)]
pub struct GraphNode {
    /// Unique node ID
    pub id: String,
    /// Node name
    pub name: String,
    /// Full qualified name
    pub qualified_name: String,
    /// Node type
    pub node_type: NodeType,
    /// File path (if applicable)
    pub file_path: Option<String>,
    /// Line number (if applicable)
    pub line_number: Option<u32>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
    /// Visibility (pub, pub(crate), private)
    pub visibility: Option<String>,
    /// Documentation
    pub documentation: Option<String>,
}

impl GraphNode {
    /// Create a new node
    pub fn new(name: &str, node_type: NodeType) -> Self {
        Self {
            id: format!("node_{}", NODE_COUNTER.fetch_add(1, Ordering::SeqCst)),
            name: name.to_string(),
            qualified_name: name.to_string(),
            node_type,
            file_path: None,
            line_number: None,
            metadata: HashMap::new(),
            visibility: None,
            documentation: None,
        }
    }

    /// Set qualified name
    pub fn with_qualified_name(mut self, name: &str) -> Self {
        self.qualified_name = name.to_string();
        self
    }

    /// Set file path
    pub fn in_file(mut self, path: &str) -> Self {
        self.file_path = Some(path.to_string());
        self
    }

    /// Set line number
    pub fn at_line(mut self, line: u32) -> Self {
        self.line_number = Some(line);
        self
    }

    /// Add metadata
    pub fn with_meta(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }

    /// Set visibility
    pub fn with_visibility(mut self, vis: &str) -> Self {
        self.visibility = Some(vis.to_string());
        self
    }

    /// Set documentation
    pub fn with_doc(mut self, doc: &str) -> Self {
        self.documentation = Some(doc.to_string());
        self
    }

    /// Get display label
    pub fn label(&self) -> String {
        if self.qualified_name != self.name {
            self.qualified_name.clone()
        } else {
            self.name.clone()
        }
    }
}

/// An edge in the code graph
#[derive(Debug, Clone)]
pub struct GraphEdge {
    /// Unique edge ID
    pub id: String,
    /// Source node ID
    pub source: String,
    /// Target node ID
    pub target: String,
    /// Edge type
    pub edge_type: EdgeType,
    /// Edge weight (for importance/frequency)
    pub weight: f32,
    /// Additional label
    pub label: Option<String>,
    /// Metadata
    pub metadata: HashMap<String, String>,
}

impl GraphEdge {
    /// Create a new edge
    pub fn new(source: &str, target: &str, edge_type: EdgeType) -> Self {
        Self {
            id: format!("edge_{}", EDGE_COUNTER.fetch_add(1, Ordering::SeqCst)),
            source: source.to_string(),
            target: target.to_string(),
            edge_type,
            weight: 1.0,
            label: None,
            metadata: HashMap::new(),
        }
    }

    /// Set weight
    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }

    /// Set label
    pub fn with_label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }

    /// Get display label
    pub fn display_label(&self) -> String {
        self.label
            .clone()
            .unwrap_or_else(|| self.edge_type.label().to_string())
    }
}

/// The code graph
#[derive(Debug, Clone, Default)]
pub struct CodeGraph {
    /// Graph name
    pub name: String,
    /// All nodes indexed by ID
    pub nodes: HashMap<String, GraphNode>,
    /// All edges
    pub edges: Vec<GraphEdge>,
    /// Index: node name -> node ID
    name_index: HashMap<String, String>,
    /// Index: source node -> outgoing edges
    outgoing: HashMap<String, Vec<usize>>,
    /// Index: target node -> incoming edges
    incoming: HashMap<String, Vec<usize>>,
}

impl CodeGraph {
    /// Create a new empty graph
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            nodes: HashMap::new(),
            edges: Vec::new(),
            name_index: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
        }
    }

    /// Add a node
    pub fn add_node(&mut self, node: GraphNode) -> String {
        let id = node.id.clone();
        self.name_index.insert(node.name.clone(), id.clone());
        if node.qualified_name != node.name {
            self.name_index
                .insert(node.qualified_name.clone(), id.clone());
        }
        self.nodes.insert(id.clone(), node);
        id
    }

    /// Add an edge
    pub fn add_edge(&mut self, edge: GraphEdge) {
        let idx = self.edges.len();
        self.outgoing
            .entry(edge.source.clone())
            .or_default()
            .push(idx);
        self.incoming
            .entry(edge.target.clone())
            .or_default()
            .push(idx);
        self.edges.push(edge);
    }

    /// Connect two nodes by name
    pub fn connect(&mut self, source_name: &str, target_name: &str, edge_type: EdgeType) -> bool {
        let source_id = self.name_index.get(source_name).cloned();
        let target_id = self.name_index.get(target_name).cloned();

        if let (Some(src), Some(tgt)) = (source_id, target_id) {
            self.add_edge(GraphEdge::new(&src, &tgt, edge_type));
            true
        } else {
            false
        }
    }

    /// Get node by name
    pub fn get_node(&self, name: &str) -> Option<&GraphNode> {
        self.name_index.get(name).and_then(|id| self.nodes.get(id))
    }

    /// Get node by ID
    pub fn get_node_by_id(&self, id: &str) -> Option<&GraphNode> {
        self.nodes.get(id)
    }

    /// Get outgoing edges for a node
    pub fn outgoing_edges(&self, node_id: &str) -> Vec<&GraphEdge> {
        self.outgoing
            .get(node_id)
            .map(|indices| indices.iter().map(|&i| &self.edges[i]).collect())
            .unwrap_or_default()
    }

    /// Get incoming edges for a node
    pub fn incoming_edges(&self, node_id: &str) -> Vec<&GraphEdge> {
        self.incoming
            .get(node_id)
            .map(|indices| indices.iter().map(|&i| &self.edges[i]).collect())
            .unwrap_or_default()
    }

    /// Get nodes that a given node calls/depends on
    pub fn dependencies(&self, node_id: &str) -> Vec<&GraphNode> {
        self.outgoing_edges(node_id)
            .iter()
            .filter_map(|e| self.nodes.get(&e.target))
            .collect()
    }

    /// Get nodes that depend on a given node
    pub fn dependents(&self, node_id: &str) -> Vec<&GraphNode> {
        self.incoming_edges(node_id)
            .iter()
            .filter_map(|e| self.nodes.get(&e.source))
            .collect()
    }

    /// Count nodes
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Count edges
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Get nodes by type
    pub fn nodes_by_type(&self, node_type: NodeType) -> Vec<&GraphNode> {
        self.nodes
            .values()
            .filter(|n| n.node_type == node_type)
            .collect()
    }

    /// Find path between two nodes (BFS)
    pub fn find_path(&self, from_id: &str, to_id: &str) -> Option<Vec<String>> {
        if from_id == to_id {
            return Some(vec![from_id.to_string()]);
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut parent: HashMap<String, String> = HashMap::new();

        visited.insert(from_id.to_string());
        queue.push_back(from_id.to_string());

        while let Some(current) = queue.pop_front() {
            for edge in self.outgoing_edges(&current) {
                if !visited.contains(&edge.target) {
                    visited.insert(edge.target.clone());
                    parent.insert(edge.target.clone(), current.clone());

                    if edge.target == to_id {
                        // Reconstruct path
                        let mut path = vec![to_id.to_string()];
                        let mut curr = to_id.to_string();
                        while let Some(p) = parent.get(&curr) {
                            path.push(p.clone());
                            curr = p.clone();
                        }
                        path.reverse();
                        return Some(path);
                    }

                    queue.push_back(edge.target.clone());
                }
            }
        }

        None
    }

    /// Detect cycles in the graph
    pub fn find_cycles(&self) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for node_id in self.nodes.keys() {
            if !visited.contains(node_id) {
                let mut path = Vec::new();
                self.detect_cycle(
                    node_id,
                    &mut visited,
                    &mut rec_stack,
                    &mut path,
                    &mut cycles,
                );
            }
        }

        cycles
    }

    fn detect_cycle(
        &self,
        node_id: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        visited.insert(node_id.to_string());
        rec_stack.insert(node_id.to_string());
        path.push(node_id.to_string());

        for edge in self.outgoing_edges(node_id) {
            if !visited.contains(&edge.target) {
                self.detect_cycle(&edge.target, visited, rec_stack, path, cycles);
            } else if rec_stack.contains(&edge.target) {
                // Found cycle
                let cycle_start = path.iter().position(|x| x == &edge.target).unwrap();
                let mut cycle: Vec<String> = path[cycle_start..].to_vec();
                cycle.push(edge.target.clone());
                cycles.push(cycle);
            }
        }

        path.pop();
        rec_stack.remove(node_id);
    }

    /// Calculate metrics for a node
    pub fn node_metrics(&self, node_id: &str) -> NodeMetrics {
        let in_degree = self.incoming_edges(node_id).len();
        let out_degree = self.outgoing_edges(node_id).len();

        NodeMetrics {
            in_degree,
            out_degree,
            total_degree: in_degree + out_degree,
            // Simple centrality: ratio of connections to total nodes
            centrality: if self.nodes.len() > 1 {
                (in_degree + out_degree) as f32 / (self.nodes.len() - 1) as f32
            } else {
                0.0
            },
        }
    }

    /// Get highly connected nodes (hubs)
    pub fn find_hubs(&self, threshold: usize) -> Vec<(&GraphNode, NodeMetrics)> {
        self.nodes
            .iter()
            .map(|(id, node)| (node, self.node_metrics(id)))
            .filter(|(_, m)| m.total_degree >= threshold)
            .collect()
    }

    /// Merge another graph into this one
    pub fn merge(&mut self, other: &CodeGraph) {
        for node in other.nodes.values() {
            if !self.name_index.contains_key(&node.name) {
                self.add_node(node.clone());
            }
        }

        for edge in &other.edges {
            // Check if both nodes exist
            if self.nodes.contains_key(&edge.source) && self.nodes.contains_key(&edge.target) {
                self.add_edge(edge.clone());
            }
        }
    }

    /// Create subgraph with only specified nodes
    pub fn subgraph(&self, node_ids: &[String]) -> CodeGraph {
        let node_set: HashSet<_> = node_ids.iter().collect();
        let mut sub = CodeGraph::new(&format!("{}_subgraph", self.name));

        for id in node_ids {
            if let Some(node) = self.nodes.get(id) {
                sub.add_node(node.clone());
            }
        }

        for edge in &self.edges {
            if node_set.contains(&edge.source) && node_set.contains(&edge.target) {
                sub.add_edge(edge.clone());
            }
        }

        sub
    }
}

/// Metrics for a node
#[derive(Debug, Clone)]
pub struct NodeMetrics {
    /// Number of incoming edges
    pub in_degree: usize,
    /// Number of outgoing edges
    pub out_degree: usize,
    /// Total edges
    pub total_degree: usize,
    /// Centrality score
    pub centrality: f32,
}

/// Output format for graph rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// DOT format for Graphviz
    Dot,
    /// Mermaid diagram
    Mermaid,
    /// ASCII art
    Ascii,
    /// JSON
    Json,
    /// PlantUML
    PlantUml,
}

/// Graph renderer
#[derive(Debug, Default)]
pub struct GraphRenderer {
    /// Include node types in labels
    pub include_types: bool,
    /// Include edge labels
    pub include_edge_labels: bool,
    /// Direction for layout (TB, LR, BT, RL)
    pub direction: String,
    /// Cluster by file/module
    pub cluster_by_file: bool,
}

impl GraphRenderer {
    /// Create a new renderer
    pub fn new() -> Self {
        Self {
            include_types: true,
            include_edge_labels: true,
            direction: "TB".to_string(),
            cluster_by_file: false,
        }
    }

    /// Set direction
    pub fn with_direction(mut self, dir: &str) -> Self {
        self.direction = dir.to_string();
        self
    }

    /// Enable clustering
    pub fn cluster(mut self) -> Self {
        self.cluster_by_file = true;
        self
    }

    /// Render graph to specified format as a `String`.
    pub fn render(&self, graph: &CodeGraph, format: OutputFormat) -> String {
        let mut buf = Vec::new();
        self.render_to(graph, format, &mut buf)
            .expect("writing to Vec<u8> should not fail");
        String::from_utf8(buf).expect("render output is valid UTF-8")
    }

    /// Render graph to the specified format, streaming into `w`.
    pub fn render_to(
        &self,
        graph: &CodeGraph,
        format: OutputFormat,
        w: &mut dyn IoWrite,
    ) -> io::Result<()> {
        match format {
            OutputFormat::Dot => self.write_dot(graph, w),
            OutputFormat::Mermaid => self.write_mermaid(graph, w),
            OutputFormat::Ascii => self.write_ascii(graph, w),
            OutputFormat::Json => self.write_json(graph, w),
            OutputFormat::PlantUml => self.write_plantuml(graph, w),
        }
    }

    /// Write DOT format to `w`.
    fn write_dot(&self, graph: &CodeGraph, w: &mut dyn IoWrite) -> io::Result<()> {
        writeln!(w, "digraph {} {{", sanitize_id(&graph.name))?;
        writeln!(w, "  rankdir={};", self.direction)?;
        writeln!(w, "  node [fontname=\"Arial\"];")?;
        writeln!(w, "  edge [fontname=\"Arial\", fontsize=10];")?;
        writeln!(w)?;

        if self.cluster_by_file {
            let mut by_file: HashMap<String, Vec<&GraphNode>> = HashMap::new();
            for node in graph.nodes.values() {
                let file = node
                    .file_path
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string());
                by_file.entry(file).or_default().push(node);
            }

            for (file, nodes) in by_file {
                writeln!(w, "  subgraph cluster_{} {{", sanitize_id(&file))?;
                writeln!(w, "    label=\"{}\";", file)?;
                for node in nodes {
                    writeln!(w, "    {};", self.node_to_dot(node))?;
                }
                writeln!(w, "  }}")?;
                writeln!(w)?;
            }
        } else {
            for node in graph.nodes.values() {
                writeln!(w, "  {};", self.node_to_dot(node))?;
            }
        }

        writeln!(w)?;

        for edge in &graph.edges {
            writeln!(w, "  {};", self.edge_to_dot(edge))?;
        }

        writeln!(w, "}}")?;
        Ok(())
    }

    fn node_to_dot(&self, node: &GraphNode) -> String {
        let label = if self.include_types {
            format!("{}\\n[{}]", node.name, node.node_type.as_str())
        } else {
            node.name.clone()
        };

        format!(
            "{} [label=\"{}\", shape={}, fillcolor=\"{}\", style=filled]",
            sanitize_id(&node.id),
            label,
            node.node_type.dot_shape(),
            node.node_type.color()
        )
    }

    fn edge_to_dot(&self, edge: &GraphEdge) -> String {
        let label = if self.include_edge_labels {
            format!(
                " [label=\"{}\", style={}]",
                edge.display_label(),
                edge.edge_type.dot_style()
            )
        } else {
            format!(" [style={}]", edge.edge_type.dot_style())
        };

        format!(
            "{} -> {}{}",
            sanitize_id(&edge.source),
            sanitize_id(&edge.target),
            label
        )
    }

    /// Write Mermaid format to `w`.
    fn write_mermaid(&self, graph: &CodeGraph, w: &mut dyn IoWrite) -> io::Result<()> {
        writeln!(w, "graph {}", self.direction)?;

        for node in graph.nodes.values() {
            let shape = match node.node_type {
                NodeType::Function => format!("{}(({}))", sanitize_mermaid(&node.id), node.name),
                NodeType::Struct | NodeType::Enum => {
                    format!("{}[{}]", sanitize_mermaid(&node.id), node.name)
                }
                NodeType::Trait => format!("{}{{{{{}}}}} ", sanitize_mermaid(&node.id), node.name),
                NodeType::Module | NodeType::Package => {
                    format!("{}[[{}]]", sanitize_mermaid(&node.id), node.name)
                }
                _ => format!("{}[{}]", sanitize_mermaid(&node.id), node.name),
            };
            writeln!(w, "    {}", shape)?;
        }

        writeln!(w)?;

        for edge in &graph.edges {
            let label = if let (true, Some(lbl)) = (self.include_edge_labels, edge.label.as_ref()) {
                format!("|{}|", lbl)
            } else {
                String::new()
            };
            writeln!(
                w,
                "    {}{}{}{}",
                sanitize_mermaid(&edge.source),
                edge.edge_type.mermaid_arrow(),
                label,
                sanitize_mermaid(&edge.target)
            )?;
        }

        Ok(())
    }

    /// Write ASCII art to `w`.
    fn write_ascii(&self, graph: &CodeGraph, w: &mut dyn IoWrite) -> io::Result<()> {
        writeln!(w, "=== {} ===\n", graph.name)?;
        writeln!(w, "Nodes: {}", graph.node_count())?;
        writeln!(w, "Edges: {}\n", graph.edge_count())?;

        for node in graph.nodes.values() {
            let deps = graph.dependencies(&node.id);
            let depnts = graph.dependents(&node.id);

            writeln!(
                w,
                "[{}] {} ({})",
                node.node_type.as_str(),
                node.name,
                if let Some(ref path) = node.file_path {
                    path
                } else {
                    "?"
                }
            )?;

            if !deps.is_empty() {
                write!(w, "  -> depends on: ")?;
                writeln!(
                    w,
                    "{}",
                    deps.iter()
                        .map(|n| n.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )?;
            }

            if !depnts.is_empty() {
                write!(w, "  <- used by: ")?;
                writeln!(
                    w,
                    "{}",
                    depnts
                        .iter()
                        .map(|n| n.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )?;
            }

            writeln!(w)?;
        }

        Ok(())
    }

    /// Write JSON to `w`.
    fn write_json(&self, graph: &CodeGraph, w: &mut dyn IoWrite) -> io::Result<()> {
        writeln!(w, "{{")?;
        writeln!(w, "  \"name\": \"{}\",", graph.name)?;
        writeln!(w, "  \"nodes\": [")?;

        let nodes: Vec<String> = graph
            .nodes
            .values()
            .map(|n| {
                format!(
                    "    {{\"id\": \"{}\", \"name\": \"{}\", \"type\": \"{}\", \"file\": {}}}",
                    n.id,
                    n.name,
                    n.node_type.as_str(),
                    n.file_path
                        .as_ref()
                        .map(|f| format!("\"{}\"", f))
                        .unwrap_or_else(|| "null".to_string())
                )
            })
            .collect();
        write!(w, "{}", nodes.join(",\n"))?;

        writeln!(w, "\n  ],")?;
        writeln!(w, "  \"edges\": [")?;

        let edges: Vec<String> = graph
            .edges
            .iter()
            .map(|e| {
                format!(
                    "    {{\"source\": \"{}\", \"target\": \"{}\", \"type\": \"{}\"}}",
                    e.source,
                    e.target,
                    e.edge_type.label()
                )
            })
            .collect();
        write!(w, "{}", edges.join(",\n"))?;

        writeln!(w, "\n  ]")?;
        writeln!(w, "}}")?;
        Ok(())
    }

    /// Write PlantUML to `w`.
    fn write_plantuml(&self, graph: &CodeGraph, w: &mut dyn IoWrite) -> io::Result<()> {
        writeln!(w, "@startuml\n")?;

        for node in graph.nodes.values() {
            let uml_type = match node.node_type {
                NodeType::Package => "package",
                NodeType::Module => "package",
                NodeType::Struct => "class",
                NodeType::Trait => "interface",
                NodeType::Enum => "enum",
                _ => "class",
            };
            writeln!(w, "{} {} {{\n}}", uml_type, sanitize_id(&node.name))?;
        }

        writeln!(w)?;

        for edge in &graph.edges {
            let source = sanitize_id(
                &graph
                    .nodes
                    .get(&edge.source)
                    .map(|n| n.name.clone())
                    .unwrap_or_default(),
            );
            let target = sanitize_id(
                &graph
                    .nodes
                    .get(&edge.target)
                    .map(|n| n.name.clone())
                    .unwrap_or_default(),
            );

            let arrow = match edge.edge_type {
                EdgeType::Implements => "..|>",
                EdgeType::Extends => "--|>",
                EdgeType::Contains => "*--",
                EdgeType::Uses => "-->",
                _ => "-->",
            };

            writeln!(w, "{} {} {}", source, arrow, target)?;
        }

        writeln!(w, "\n@enduml")?;
        Ok(())
    }
}

/// Code graph builder
#[derive(Debug, Default)]
pub struct GraphBuilder {
    /// Current graph being built
    graph: CodeGraph,
    /// Stack of parent nodes (for hierarchical building)
    parent_stack: Vec<String>,
}

impl GraphBuilder {
    /// Create a new builder
    pub fn new(name: &str) -> Self {
        Self {
            graph: CodeGraph::new(name),
            parent_stack: Vec::new(),
        }
    }

    /// Add a file node
    pub fn add_file(&mut self, path: &str) -> String {
        let name = path.rsplit('/').next().unwrap_or(path);
        let node = GraphNode::new(name, NodeType::File)
            .in_file(path)
            .with_qualified_name(path);
        self.graph.add_node(node)
    }

    /// Add a module
    pub fn add_module(&mut self, name: &str, file: Option<&str>) -> String {
        let mut node = GraphNode::new(name, NodeType::Module);
        if let Some(f) = file {
            node = node.in_file(f);
        }
        self.graph.add_node(node)
    }

    /// Add a function
    pub fn add_function(&mut self, name: &str, file: Option<&str>, line: Option<u32>) -> String {
        let mut node = GraphNode::new(name, NodeType::Function);
        if let Some(f) = file {
            node = node.in_file(f);
        }
        if let Some(l) = line {
            node = node.at_line(l);
        }
        self.graph.add_node(node)
    }

    /// Add a struct
    pub fn add_struct(&mut self, name: &str, file: Option<&str>) -> String {
        let mut node = GraphNode::new(name, NodeType::Struct);
        if let Some(f) = file {
            node = node.in_file(f);
        }
        self.graph.add_node(node)
    }

    /// Add a trait
    pub fn add_trait(&mut self, name: &str) -> String {
        let node = GraphNode::new(name, NodeType::Trait);
        self.graph.add_node(node)
    }

    /// Add a call edge
    pub fn add_call(&mut self, caller: &str, callee: &str) {
        self.graph.connect(caller, callee, EdgeType::Calls);
    }

    /// Add an import edge
    pub fn add_import(&mut self, importer: &str, imported: &str) {
        self.graph.connect(importer, imported, EdgeType::Imports);
    }

    /// Add a type dependency
    pub fn add_type_dependency(&mut self, dependent: &str, dependency: &str) {
        self.graph
            .connect(dependent, dependency, EdgeType::TypeDependency);
    }

    /// Add an implements edge
    pub fn add_implements(&mut self, implementor: &str, trait_name: &str) {
        self.graph
            .connect(implementor, trait_name, EdgeType::Implements);
    }

    /// Add a contains edge
    pub fn add_contains(&mut self, container: &str, contained: &str) {
        self.graph.connect(container, contained, EdgeType::Contains);
    }

    /// Enter a parent context (for building hierarchies)
    pub fn enter(&mut self, node_id: &str) {
        self.parent_stack.push(node_id.to_string());
    }

    /// Exit current parent context
    pub fn exit(&mut self) -> Option<String> {
        self.parent_stack.pop()
    }

    /// Add node as child of current parent
    pub fn add_child(&mut self, name: &str, node_type: NodeType) -> String {
        let node = GraphNode::new(name, node_type);
        let id = self.graph.add_node(node);

        if let Some(parent) = self.parent_stack.last() {
            let edge = GraphEdge::new(parent, &id, EdgeType::Contains);
            self.graph.add_edge(edge);
        }

        id
    }

    /// Build and return the graph
    pub fn build(self) -> CodeGraph {
        self.graph
    }

    /// Get current graph (borrowing)
    pub fn graph(&self) -> &CodeGraph {
        &self.graph
    }
}

// Helper functions

/// Sanitize ID for DOT format
fn sanitize_id(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Sanitize ID for Mermaid format
fn sanitize_mermaid(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
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
        renderer.render_to(&graph, OutputFormat::Dot, &mut buf).unwrap();
        assert_eq!(rendered, String::from_utf8(buf).unwrap());
    }

    #[test]
    fn test_render_to_matches_render_mermaid() {
        let graph = sample_graph();
        let renderer = GraphRenderer::new();
        let rendered = renderer.render(&graph, OutputFormat::Mermaid);
        let mut buf = Vec::new();
        renderer.render_to(&graph, OutputFormat::Mermaid, &mut buf).unwrap();
        assert_eq!(rendered, String::from_utf8(buf).unwrap());
    }

    #[test]
    fn test_render_to_matches_render_ascii() {
        let graph = sample_graph();
        let renderer = GraphRenderer::new();
        let rendered = renderer.render(&graph, OutputFormat::Ascii);
        let mut buf = Vec::new();
        renderer.render_to(&graph, OutputFormat::Ascii, &mut buf).unwrap();
        assert_eq!(rendered, String::from_utf8(buf).unwrap());
    }

    #[test]
    fn test_render_to_matches_render_json() {
        let graph = sample_graph();
        let renderer = GraphRenderer::new();
        let rendered = renderer.render(&graph, OutputFormat::Json);
        let mut buf = Vec::new();
        renderer.render_to(&graph, OutputFormat::Json, &mut buf).unwrap();
        assert_eq!(rendered, String::from_utf8(buf).unwrap());
    }

    #[test]
    fn test_render_to_matches_render_plantuml() {
        let graph = sample_graph();
        let renderer = GraphRenderer::new();
        let rendered = renderer.render(&graph, OutputFormat::PlantUml);
        let mut buf = Vec::new();
        renderer.render_to(&graph, OutputFormat::PlantUml, &mut buf).unwrap();
        assert_eq!(rendered, String::from_utf8(buf).unwrap());
    }
}
