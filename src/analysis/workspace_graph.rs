//! Workspace-level code graph construction and rendering.
//!
//! Builds a navigable graph from Rust source files using the existing project
//! intelligence index, then adds lightweight dependency edges for imports and
//! type references.

use crate::analysis::code_graph::{
    CodeGraph, EdgeType, GraphEdge, GraphNode, GraphRenderer, NodeType, OutputFormat,
};
use crate::cognitive::intelligence::{ProjectIntelligence, Symbol, SymbolKind};
use crate::cognitive::knowledge_graph::SemanticLinker;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

static IDENTIFIER_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b[A-Za-z_][A-Za-z0-9_]*\b").expect("valid identifier regex"));

#[derive(Debug, Clone)]
pub struct WorkspaceGraphOptions {
    pub root: PathBuf,
    pub focus: Option<String>,
    pub neighborhood_depth: usize,
    pub max_nodes: usize,
    pub include_imports: bool,
    pub include_type_dependencies: bool,
}

impl WorkspaceGraphOptions {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            focus: None,
            neighborhood_depth: 2,
            max_nodes: 48,
            include_imports: true,
            include_type_dependencies: true,
        }
    }
}

impl Default for WorkspaceGraphOptions {
    fn default() -> Self {
        Self::new(".")
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceGraphSummary {
    pub node_count: usize,
    pub edge_count: usize,
    pub file_count: usize,
    pub function_count: usize,
    pub type_count: usize,
    pub import_edges: usize,
    pub dependency_edges: usize,
    pub contains_edges: usize,
    pub cycle_count: usize,
}

#[derive(Debug, Clone)]
struct IndexedSymbol {
    symbol: Symbol,
    node_id: String,
}

pub fn build_workspace_graph(options: &WorkspaceGraphOptions) -> Result<CodeGraph> {
    let full_graph = build_full_graph(options)?;

    match options
        .focus
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(focus) => Ok(filter_graph(
            &full_graph,
            focus,
            options.neighborhood_depth,
            options.max_nodes,
        )),
        None => Ok(full_graph),
    }
}

pub fn render_workspace_graph(
    options: &WorkspaceGraphOptions,
    format: OutputFormat,
) -> Result<String> {
    let graph = build_workspace_graph(options)?;
    let renderer = GraphRenderer::new().with_direction("LR").cluster();
    Ok(renderer.render(&graph, format))
}

pub fn summarize_graph(graph: &CodeGraph) -> WorkspaceGraphSummary {
    let mut file_count = 0;
    let mut function_count = 0;
    let mut type_count = 0;
    let mut import_edges = 0;
    let mut dependency_edges = 0;
    let mut contains_edges = 0;

    for node in graph.nodes.values() {
        match node.node_type {
            NodeType::File => file_count += 1,
            NodeType::Function => function_count += 1,
            NodeType::Struct | NodeType::Enum | NodeType::Trait | NodeType::TypeAlias => {
                type_count += 1
            }
            _ => {}
        }
    }

    for edge in &graph.edges {
        match edge.edge_type {
            EdgeType::Imports => import_edges += 1,
            EdgeType::TypeDependency => dependency_edges += 1,
            EdgeType::Contains => contains_edges += 1,
            _ => {}
        }
    }

    WorkspaceGraphSummary {
        node_count: graph.node_count(),
        edge_count: graph.edge_count(),
        file_count,
        function_count,
        type_count,
        import_edges,
        dependency_edges,
        contains_edges,
        cycle_count: graph.find_cycles().len(),
    }
}

fn build_full_graph(options: &WorkspaceGraphOptions) -> Result<CodeGraph> {
    let root = canonicalize_root(&options.root)?;
    let workspace_name = root
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "workspace".to_string());

    let mut intelligence = ProjectIntelligence::new(root.clone());
    intelligence.refresh()?;

    let rust_files = {
        let files = intelligence
            .files()
            .read()
            .map_err(|_| anyhow::anyhow!("failed to acquire file index"))?;
        files
            .by_language("Rust")
            .into_iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>()
    };

    let mut graph = CodeGraph::new(&workspace_name);
    let package_id = graph.add_node(
        GraphNode::new(&format!("pkg::{workspace_name}"), NodeType::Package)
            .with_qualified_name(&workspace_name)
            .in_file(&root.to_string_lossy()),
    );

    let mut file_nodes = HashMap::<PathBuf, String>::new();
    let mut indexed_symbols = Vec::<IndexedSymbol>::new();
    let mut symbols_by_name = HashMap::<String, Vec<String>>::new();
    let mut symbol_file_by_id = HashMap::<String, PathBuf>::new();
    let mut edge_keys = HashSet::<(String, String, EdgeType)>::new();

    {
        let symbols = intelligence
            .symbols()
            .read()
            .map_err(|_| anyhow::anyhow!("failed to acquire symbol index"))?;

        for file in &rust_files {
            let relative = relative_display(file, &root);
            let file_name = format!("file::{relative}");
            let file_id = graph.add_node(
                GraphNode::new(&file_name, NodeType::File)
                    .with_qualified_name(&relative)
                    .in_file(&relative),
            );
            graph.add_edge(GraphEdge::new(&package_id, &file_id, EdgeType::Contains));
            edge_keys.insert((package_id.clone(), file_id.clone(), EdgeType::Contains));
            file_nodes.insert(file.clone(), file_id.clone());

            if let Some(file_symbols) = symbols.in_file(file) {
                for symbol in file_symbols {
                    let unique_name = symbol_unique_name(symbol, &root);
                    let qualified = symbol_qualified_name(symbol, &root);
                    let mut node =
                        GraphNode::new(&unique_name, symbol_kind_to_node_type(&symbol.kind))
                            .with_qualified_name(&qualified)
                            .in_file(&relative)
                            .at_line(symbol.line as u32)
                            .with_meta("symbol_name", &symbol.name)
                            .with_meta("symbol_kind", &format!("{:?}", symbol.kind))
                            .with_meta("signature", &symbol.signature);

                    if let Some(doc) = &symbol.doc {
                        node = node.with_doc(doc);
                    }

                    node = node.with_visibility(&format!("{:?}", symbol.visibility));

                    let node_id = graph.add_node(node);
                    graph.add_edge(GraphEdge::new(&file_id, &node_id, EdgeType::Contains));
                    edge_keys.insert((file_id.clone(), node_id.clone(), EdgeType::Contains));

                    indexed_symbols.push(IndexedSymbol {
                        symbol: symbol.clone(),
                        node_id: node_id.clone(),
                    });
                    symbol_file_by_id.insert(node_id.clone(), symbol.file.clone());
                    symbols_by_name
                        .entry(symbol.name.clone())
                        .or_default()
                        .push(node_id);
                }
            }
        }
    }

    if options.include_imports || options.include_type_dependencies {
        let linker = SemanticLinker::new();

        for file in &rust_files {
            let content = std::fs::read_to_string(file)
                .with_context(|| format!("failed to read Rust source '{}'", file.display()))?;

            if options.include_imports {
                if let Some(source_id) = file_nodes.get(file) {
                    add_import_edges(
                        &mut graph,
                        &mut edge_keys,
                        source_id,
                        linker.extract_imports(&content, "rust"),
                        &symbols_by_name,
                    );
                }
            }
        }
    }

    if options.include_type_dependencies {
        add_type_dependency_edges(
            &mut graph,
            &mut edge_keys,
            &indexed_symbols,
            &symbols_by_name,
            &symbol_file_by_id,
        );
    }

    Ok(graph)
}

fn canonicalize_root(root: &Path) -> Result<PathBuf> {
    let root = if root.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        root.to_path_buf()
    };
    std::fs::canonicalize(&root)
        .with_context(|| format!("failed to resolve workspace root '{}'", root.display()))
}

fn relative_display(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

fn symbol_unique_name(symbol: &Symbol, root: &Path) -> String {
    format!(
        "symbol::{}::{}:{}",
        relative_display(&symbol.file, root),
        symbol.name,
        symbol.line
    )
}

fn symbol_qualified_name(symbol: &Symbol, root: &Path) -> String {
    format!("{}::{}", relative_display(&symbol.file, root), symbol.name)
}

fn symbol_kind_to_node_type(kind: &SymbolKind) -> NodeType {
    match kind {
        SymbolKind::Function => NodeType::Function,
        SymbolKind::Struct => NodeType::Struct,
        SymbolKind::Enum => NodeType::Enum,
        SymbolKind::Trait => NodeType::Trait,
        SymbolKind::Impl => NodeType::Impl,
        SymbolKind::Const | SymbolKind::Static => NodeType::Const,
        SymbolKind::Type => NodeType::TypeAlias,
        SymbolKind::Macro => NodeType::Macro,
        SymbolKind::Module => NodeType::Module,
    }
}

fn add_import_edges(
    graph: &mut CodeGraph,
    edge_keys: &mut HashSet<(String, String, EdgeType)>,
    source_id: &str,
    imports: Vec<String>,
    symbols_by_name: &HashMap<String, Vec<String>>,
) {
    for import in imports {
        let short_name = import.rsplit("::").next().unwrap_or(&import);
        let Some(targets) = symbols_by_name.get(short_name) else {
            continue;
        };

        for target_id in targets.iter().take(3) {
            let key = (source_id.to_string(), target_id.clone(), EdgeType::Imports);
            if edge_keys.insert(key) {
                graph.add_edge(
                    GraphEdge::new(source_id, target_id, EdgeType::Imports).with_label(&import),
                );
            }
        }
    }
}

fn add_type_dependency_edges(
    graph: &mut CodeGraph,
    edge_keys: &mut HashSet<(String, String, EdgeType)>,
    indexed_symbols: &[IndexedSymbol],
    symbols_by_name: &HashMap<String, Vec<String>>,
    symbol_file_by_id: &HashMap<String, PathBuf>,
) {
    for entry in indexed_symbols {
        let tokens = IDENTIFIER_REGEX
            .find_iter(&entry.symbol.signature)
            .map(|m| m.as_str())
            .collect::<HashSet<_>>();

        for token in tokens {
            if token == entry.symbol.name {
                continue;
            }

            let Some(targets) = symbols_by_name.get(token) else {
                continue;
            };

            for target_id in targets {
                if target_id == &entry.node_id {
                    continue;
                }

                // Prefer dependencies that resolve within the same file to reduce noise.
                let same_file = symbol_file_by_id
                    .get(target_id)
                    .map(|path| path == &entry.symbol.file)
                    .unwrap_or(false);
                if !same_file && targets.len() > 1 {
                    continue;
                }

                let key = (
                    entry.node_id.clone(),
                    target_id.clone(),
                    EdgeType::TypeDependency,
                );
                if edge_keys.insert(key) {
                    graph.add_edge(GraphEdge::new(
                        &entry.node_id,
                        target_id,
                        EdgeType::TypeDependency,
                    ));
                }
            }
        }
    }
}

fn filter_graph(graph: &CodeGraph, focus: &str, depth: usize, max_nodes: usize) -> CodeGraph {
    let focus = focus.to_lowercase();
    let mut queue = VecDeque::<(String, usize)>::new();
    let mut visited = HashSet::<String>::new();

    let mut matches = graph
        .nodes
        .iter()
        .filter(|(_, node)| node_matches_focus(node, &focus))
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();

    if matches.is_empty() {
        return graph.clone();
    }

    matches.sort();

    for id in matches {
        if visited.insert(id.clone()) {
            queue.push_back((id, 0));
        }
    }

    while let Some((node_id, current_depth)) = queue.pop_front() {
        if visited.len() >= max_nodes {
            break;
        }
        if current_depth >= depth {
            continue;
        }

        for edge in graph.outgoing_edges(&node_id) {
            if visited.len() >= max_nodes {
                break;
            }

            if visited.insert(edge.target.clone()) {
                queue.push_back((edge.target.clone(), current_depth + 1));
            }
        }

        for edge in graph.incoming_edges(&node_id) {
            if visited.len() >= max_nodes {
                break;
            }

            if visited.insert(edge.source.clone()) {
                queue.push_back((edge.source.clone(), current_depth + 1));
            }
        }
    }

    let mut node_ids = visited.into_iter().collect::<Vec<_>>();
    node_ids.sort();
    node_ids.truncate(max_nodes);
    graph.subgraph(&node_ids)
}

fn node_matches_focus(node: &GraphNode, focus: &str) -> bool {
    let file_path = node.file_path.as_deref().unwrap_or_default().to_lowercase();
    node.name.to_lowercase().contains(focus)
        || node.qualified_name.to_lowercase().contains(focus)
        || file_path.contains(focus)
        || node
            .metadata
            .get("symbol_name")
            .map(|value| value.to_lowercase().contains(focus))
            .unwrap_or(false)
}

#[cfg(test)]
#[path = "../../tests/unit/analysis/workspace_graph/workspace_graph_test.rs"]
mod tests;
