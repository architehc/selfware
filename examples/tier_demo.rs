//! Demo: Run the tier allocator against the real selfware codebase.
//!
//! Usage: cargo run --example tier_demo [focus_node] [context_window]
//!
//! Examples:
//!   cargo run --example tier_demo                    # defaults: "agent" focus, 131072 context
//!   cargo run --example tier_demo tools 32768        # focus on tools, 9B model budget
//!   cargo run --example tier_demo config 32768       # focus on config, 9B model budget

use anyhow::Result;
use selfware::analysis::code_graph::{CodeGraph, EdgeType, GraphNode, NodeType};
use selfware::analysis::tier_allocator::{allocate_tiers, ContextTier, TierAllocatorConfig};
use selfware::analysis::workspace_graph::{build_workspace_graph, WorkspaceGraphOptions};
use std::collections::{HashMap, HashSet};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let focus_name = args.get(1).map(|s| s.as_str()).unwrap_or("agent");
    let context_window: usize = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(131_072);

    println!("Building workspace graph from src/...");
    let options = WorkspaceGraphOptions {
        root: std::path::PathBuf::from("."),
        focus: None,
        neighborhood_depth: 100,
        max_nodes: 5000,
        include_imports: true,
        include_type_dependencies: true,
    };

    let full_graph = build_workspace_graph(&options)?;
    println!(
        "Full graph: {} nodes, {} edges",
        full_graph.node_count(),
        full_graph.edge_count()
    );

    // Build file-level DAG by collapsing symbol edges to file edges.
    // For every edge between two symbols, create an edge between their parent files.
    let graph = {
        let mut file_graph = CodeGraph::new("file_dag");

        // Collect all file nodes
        let file_nodes: Vec<&selfware::analysis::code_graph::GraphNode> = full_graph
            .nodes_by_type(NodeType::File)
            .into_iter()
            .collect();

        // Map: file_path -> node_id in file_graph
        let mut path_to_id: HashMap<String, String> = HashMap::new();
        for node in &file_nodes {
            if let Some(ref path) = node.file_path {
                let short_name = path
                    .strip_prefix("src/")
                    .unwrap_or(path)
                    .to_string();
                let new_node = GraphNode::new(&short_name, NodeType::File)
                    .in_file(path);
                let id = file_graph.add_node(new_node);
                path_to_id.insert(path.clone(), id);
            }
        }

        // Map: symbol node_id -> file_path (via the symbol's file_path field)
        let mut symbol_to_file: HashMap<String, String> = HashMap::new();
        for (node_id, node) in &full_graph.nodes {
            if let Some(ref path) = node.file_path {
                symbol_to_file.insert(node_id.clone(), path.clone());
            }
        }

        // Collapse symbol edges to file edges (deduplicated)
        let mut seen_edges: HashSet<(String, String)> = HashSet::new();
        for edge in &full_graph.edges {
            let src_file = symbol_to_file.get(&edge.source);
            let tgt_file = symbol_to_file.get(&edge.target);
            if let (Some(sf), Some(tf)) = (src_file, tgt_file) {
                if sf == tf {
                    continue; // Skip intra-file edges
                }
                let src_id = path_to_id.get(sf.as_str());
                let tgt_id = path_to_id.get(tf.as_str());
                if let (Some(sid), Some(tid)) = (src_id, tgt_id) {
                    let key = (sid.clone(), tid.clone());
                    if seen_edges.insert(key) {
                        file_graph.add_edge(
                            selfware::analysis::code_graph::GraphEdge::new(
                                sid, tid, EdgeType::Imports,
                            ),
                        );
                    }
                }
            }
        }

        println!(
            "File-level DAG: {} files, {} cross-file edges\n",
            file_graph.node_count(),
            file_graph.edge_count()
        );
        file_graph
    };

    // Find the focus node — match against file paths
    let focus_id = {
        // First try exact name match
        if let Some(node) = graph.get_node(focus_name) {
            node.id.clone()
        } else {
            // Try partial path match against file_path or name
            let candidates: Vec<_> = graph
                .nodes_by_type(NodeType::File)
                .into_iter()
                .chain(graph.nodes_by_type(NodeType::Module))
                .filter(|n| {
                    n.name.contains(focus_name)
                        || n.file_path
                            .as_deref()
                            .map(|p| p.contains(focus_name))
                            .unwrap_or(false)
                })
                .collect();

            if candidates.is_empty() {
                eprintln!("No node matching '{}'. Available files:", focus_name);
                let mut files: Vec<_> = graph
                    .nodes_by_type(NodeType::File)
                    .into_iter()
                    .map(|n| n.name.as_str())
                    .collect();
                files.sort();
                for name in files.iter().take(30) {
                    eprintln!("  - {}", name);
                }
                if files.len() > 30 {
                    eprintln!("  ... and {} more", files.len() - 30);
                }
                std::process::exit(1);
            }
            println!("Matched '{}' -> {}", focus_name, candidates[0].name);
            candidates[0].id.clone()
        }
    };

    // Run the allocator
    let config = TierAllocatorConfig::for_context_window(context_window);
    println!(
        "Context window: {} tokens",
        config.context_window
    );
    println!(
        "Content budget: {} tokens (system:{}, output:{})\n",
        config.content_budget(),
        config.system_reserve,
        config.output_reserve
    );

    let allocation = allocate_tiers(&graph, &focus_id, &config);

    // Print summary
    println!("{}\n", allocation.summary());

    // Print by tier
    for tier in [
        ContextTier::Edit,
        ContextTier::Integrate,
        ContextTier::Work,
        ContextTier::Describe,
    ] {
        let nodes = allocation.at_tier(tier);
        if nodes.is_empty() {
            continue;
        }
        println!(
            "── {} ({} nodes, {} tok/node) ──",
            tier,
            nodes.len(),
            tier.default_tokens()
        );
        for a in &nodes {
            let path = a.file_path.as_deref().unwrap_or("-");
            println!(
                "  {:3} hops  {:>5} tok  {}  ({})",
                a.hops, a.estimated_tokens, a.name, path
            );
        }
        println!();
    }

    Ok(())
}
