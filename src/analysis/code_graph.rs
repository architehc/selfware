//! Code Graph Visualization System
//!
//! Interactive dependency graphs, call flow visualization, architecture
//! diagrams from code, live updating during edits.
//!
//! Two-graph model: this is the CLI/visualization graph behind `selfware
//! graph` (DOT/Mermaid/ASCII output). The CANONICAL, structurally validated
//! repository graph is `evolve::graph`; new graph features belong there, not
//! here. Do not merge or cross-wire the two.
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
#[path = "../../tests/unit/analysis/code_graph/code_graph_test.rs"]
mod tests;
