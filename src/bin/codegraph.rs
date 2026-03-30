//! Generates a comprehensive JSON graph of the selfware codebase.
//!
//! Usage: cargo run --bin codegraph
//! Output: codegraph.json in the current directory

use regex::Regex;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

fn main() {
    let src_dir = PathBuf::from("src");
    if !src_dir.exists() {
        eprintln!("Error: src/ directory not found. Run from project root.");
        std::process::exit(1);
    }

    let mut nodes: Vec<Value> = Vec::new();
    let mut edges: Vec<Value> = Vec::new();
    let mut total_lines: usize = 0;
    let mut total_files: usize = 0;

    // Collect all .rs files
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(&src_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path().to_path_buf();
        if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }

    // Regex patterns for extracting items
    let fn_re = Regex::new(r"(?m)^[ \t]*(pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)").unwrap();
    let struct_re = Regex::new(r"(?m)^[ \t]*(pub(?:\([^)]*\))?\s+)?struct\s+(\w+)").unwrap();
    let enum_re = Regex::new(r"(?m)^[ \t]*(pub(?:\([^)]*\))?\s+)?enum\s+(\w+)").unwrap();
    let trait_re = Regex::new(r"(?m)^[ \t]*(pub(?:\([^)]*\))?\s+)?trait\s+(\w+)").unwrap();
    let impl_re = Regex::new(r"(?m)^[ \t]*impl(?:<[^>]*>)?\s+(?:(\w+)\s+for\s+)?(\w+)").unwrap();
    let use_re = Regex::new(r"(?m)^[ \t]*use\s+(?:crate|super)::(\w+)").unwrap();
    let mod_re = Regex::new(r"(?m)^[ \t]*(pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;").unwrap();

    // Map: module_id -> children ids
    let mut module_children: HashMap<String, Vec<String>> = HashMap::new();
    // Map: file module_id -> set of dependency module_ids
    let mut file_deps: HashMap<String, HashSet<String>> = HashMap::new();
    // All node ids for lookup
    let mut all_node_ids: HashSet<String> = HashSet::new();
    // Store node data temporarily
    struct NodeData {
        id: String,
        label: String,
        kind: String,
        path: String,
        lines: usize,
        children: Vec<String>,
    }
    let mut node_data_list: Vec<NodeData> = Vec::new();

    for file_path in &files {
        let rel_path = file_path.strip_prefix(".").unwrap_or(file_path);
        let rel_str = rel_path.to_string_lossy().replace('\\', "/");

        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let line_count = content.lines().count();
        total_lines += line_count;
        total_files += 1;

        // Determine module path from file path
        // e.g., src/agent/mod.rs -> agent::mod
        // e.g., src/agent/execution.rs -> agent::execution
        // e.g., src/cache.rs -> cache
        let module_id = path_to_module_id(rel_path);
        let _parent_module = parent_module_id(&module_id);

        // Determine if this is a module file (mod.rs or directory-based)
        let file_name = file_path.file_name().unwrap().to_string_lossy();
        let is_mod_file = file_name == "mod.rs";
        let is_main_or_lib = file_name == "main.rs" || file_name == "lib.rs";

        let kind = if is_mod_file || is_main_or_lib {
            "module"
        } else {
            "file"
        };

        let label = if is_mod_file {
            // Use parent directory name
            file_path
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| module_id.clone())
        } else if is_main_or_lib {
            file_name.replace(".rs", "")
        } else {
            file_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| module_id.clone())
        };

        // Extract child items
        let mut children: Vec<String> = Vec::new();

        // Functions
        for cap in fn_re.captures_iter(&content) {
            let vis = cap.get(1).map_or("", |m| m.as_str());
            if vis.starts_with("pub") {
                let name = &cap[2];
                let child_id = format!("{}::{}", module_id, name);
                children.push(child_id.clone());
                all_node_ids.insert(child_id.clone());

                node_data_list.push(NodeData {
                    id: child_id,
                    label: name.to_string(),
                    kind: "function".to_string(),
                    path: rel_str.clone(),
                    lines: 0, // child items don't track lines individually
                    children: vec![],
                });
            }
        }

        // Structs
        for cap in struct_re.captures_iter(&content) {
            let vis = cap.get(1).map_or("", |m| m.as_str());
            if vis.starts_with("pub") {
                let name = &cap[2];
                let child_id = format!("{}::{}", module_id, name);
                children.push(child_id.clone());
                all_node_ids.insert(child_id.clone());

                node_data_list.push(NodeData {
                    id: child_id,
                    label: name.to_string(),
                    kind: "struct".to_string(),
                    path: rel_str.clone(),
                    lines: 0,
                    children: vec![],
                });
            }
        }

        // Enums
        for cap in enum_re.captures_iter(&content) {
            let vis = cap.get(1).map_or("", |m| m.as_str());
            if vis.starts_with("pub") {
                let name = &cap[2];
                let child_id = format!("{}::{}", module_id, name);
                children.push(child_id.clone());
                all_node_ids.insert(child_id.clone());

                node_data_list.push(NodeData {
                    id: child_id,
                    label: name.to_string(),
                    kind: "enum".to_string(),
                    path: rel_str.clone(),
                    lines: 0,
                    children: vec![],
                });
            }
        }

        // Traits
        for cap in trait_re.captures_iter(&content) {
            let vis = cap.get(1).map_or("", |m| m.as_str());
            if vis.starts_with("pub") {
                let name = &cap[2];
                let child_id = format!("{}::{}", module_id, name);
                children.push(child_id.clone());
                all_node_ids.insert(child_id.clone());

                node_data_list.push(NodeData {
                    id: child_id,
                    label: name.to_string(),
                    kind: "trait".to_string(),
                    path: rel_str.clone(),
                    lines: 0,
                    children: vec![],
                });
            }
        }

        // Impl blocks (extract as edges, not nodes)
        for cap in impl_re.captures_iter(&content) {
            if let Some(trait_name) = cap.get(1) {
                let _target = &cap[2];
                let trait_id = trait_name.as_str().to_string();
                // We'll add "implements" edges later if the trait is in our graph
                let dep_set = file_deps.entry(module_id.clone()).or_default();
                dep_set.insert(format!("impl:{}", trait_id));
            }
        }

        // Use statements (crate-internal dependencies)
        for cap in use_re.captures_iter(&content) {
            let dep_module = cap[1].to_string();
            let dep_set = file_deps.entry(module_id.clone()).or_default();
            dep_set.insert(dep_module);
        }

        // Mod declarations (contains edges)
        for cap in mod_re.captures_iter(&content) {
            let child_mod = &cap[2];
            let child_mod_id = format!("{}::{}", module_id, child_mod);
            module_children
                .entry(module_id.clone())
                .or_default()
                .push(child_mod_id);
        }

        all_node_ids.insert(module_id.clone());

        node_data_list.push(NodeData {
            id: module_id,
            label,
            kind: kind.to_string(),
            path: rel_str,
            lines: line_count,
            children,
        });
    }

    // Build the nodes JSON
    for nd in &node_data_list {
        let tokens_full = (nd.lines as f64 * 7.5) as u64;
        let complexity = if nd.lines < 200 {
            "low"
        } else if nd.lines < 800 {
            "medium"
        } else {
            "high"
        };

        let throughput = 3000.0_f64; // tokens/sec

        let inspect_tokens = (tokens_full as f64 * 0.06) as u64;
        let skeleton_tokens = (tokens_full as f64 * 0.10) as u64;
        let alter_tokens = (tokens_full as f64 * 1.50) as u64;
        let build_tokens = (tokens_full as f64 * 2.00) as u64;

        let time_ms = |t: u64| ((t as f64 / throughput) * 1000.0) as u64;

        let context_actions = json!({
            "inspect": { "tokens": inspect_tokens, "time_estimate_ms": time_ms(inspect_tokens) },
            "read_full": { "tokens": tokens_full, "time_estimate_ms": time_ms(tokens_full) },
            "read_skeleton": { "tokens": skeleton_tokens, "time_estimate_ms": time_ms(skeleton_tokens) },
            "alter": { "tokens": alter_tokens, "time_estimate_ms": time_ms(alter_tokens) },
            "build_new": { "tokens": build_tokens, "time_estimate_ms": time_ms(build_tokens) },
        });

        // Merge module_children into children list
        let mut all_children = nd.children.clone();
        if let Some(mod_kids) = module_children.get(&nd.id) {
            all_children.extend(mod_kids.iter().cloned());
        }

        nodes.push(json!({
            "id": nd.id,
            "label": nd.label,
            "kind": nd.kind,
            "path": nd.path,
            "lines": nd.lines,
            "tokens_estimate": tokens_full,
            "complexity": complexity,
            "children": all_children,
            "context_actions": context_actions,
        }));
    }

    // Build top-level module set for resolving "use crate::X"
    let top_modules: HashSet<String> = node_data_list
        .iter()
        .filter(|nd| nd.kind == "module" || nd.kind == "file")
        .filter_map(|nd| {
            let parts: Vec<&str> = nd.id.split("::").collect();
            if !parts.is_empty() {
                Some(parts[0].to_string())
            } else {
                None
            }
        })
        .collect();

    // Build edges from use statements
    let mut edge_set: HashSet<(String, String, String)> = HashSet::new();
    for (source_id, deps) in &file_deps {
        for dep in deps {
            if let Some(trait_name) = dep.strip_prefix("impl:") {
                // implements edge — find the trait in nodes
                // Try to find matching trait node
                for nd in &node_data_list {
                    if nd.kind == "trait" && nd.label == trait_name {
                        let key = (source_id.clone(), nd.id.clone(), "implements".to_string());
                        if edge_set.insert(key) {
                            edges.push(json!({
                                "source": source_id,
                                "target": nd.id,
                                "kind": "implements",
                                "weight": 1.0,
                            }));
                        }
                    }
                }
            } else if top_modules.contains(dep) {
                // "uses" edge to a top-level module
                // Find the mod.rs or single-file module
                let target_id = if all_node_ids.contains(&format!("{}::mod", dep)) {
                    format!("{}::mod", dep)
                } else {
                    dep.clone()
                };
                // Don't add self-edges
                let source_top = source_id.split("::").next().unwrap_or(source_id);
                if source_top != dep {
                    let key = (source_id.clone(), target_id.clone(), "uses".to_string());
                    if edge_set.insert(key) {
                        edges.push(json!({
                            "source": source_id,
                            "target": target_id,
                            "kind": "uses",
                            "weight": 1.0,
                        }));
                    }
                }
            }
        }
    }

    // Add "contains" edges from modules to their children
    for (parent, kids) in &module_children {
        for kid in kids {
            let key = (parent.clone(), kid.clone(), "contains".to_string());
            if edge_set.insert(key) {
                edges.push(json!({
                    "source": parent,
                    "target": kid,
                    "kind": "contains",
                    "weight": 1.0,
                }));
            }
        }
    }

    // Fusion detection
    let fusion_groups = detect_fusions(&edges);

    let total_tokens = (total_lines as f64 * 7.5) as u64;

    let output = json!({
        "nodes": nodes,
        "edges": edges,
        "fusion_groups": fusion_groups,
        "meta": {
            "total_files": total_files,
            "total_lines": total_lines,
            "total_tokens": total_tokens,
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "fusion_levels": {
                "binary": "Two components connected by a single edge",
                "trinary": "Three components forming a triangle dependency",
                "quaternary": "Four components forming a diamond or square pattern",
            },
        },
    });

    let output_str = serde_json::to_string_pretty(&output).expect("Failed to serialize JSON");
    fs::write("codegraph.json", &output_str).expect("Failed to write codegraph.json");

    println!("codegraph.json generated successfully.");
    println!("  Nodes: {}", nodes.len());
    println!("  Edges: {}", edges.len());
    println!("  Files: {}", total_files);
    println!("  Lines: {}", total_lines);
    println!("  Tokens (est): {}", total_tokens);
    println!("  Fusion groups: {}", fusion_groups.len());
}

fn path_to_module_id(path: &Path) -> String {
    let path_str = path.to_string_lossy().replace('\\', "/");
    // Strip leading "src/"
    let stripped = path_str
        .strip_prefix("src/")
        .unwrap_or(&path_str);
    // Strip trailing ".rs"
    let stripped = stripped
        .strip_suffix(".rs")
        .unwrap_or(stripped);
    // Replace "/" with "::"
    stripped.replace('/', "::")
}

fn parent_module_id(module_id: &str) -> Option<String> {
    let parts: Vec<&str> = module_id.split("::").collect();
    if parts.len() > 1 {
        Some(parts[..parts.len() - 1].join("::"))
    } else {
        None
    }
}

/// Detect fusion patterns in the dependency graph.
/// Binary: A->B (simple dependency pair)
/// Trinary: A->B, B->C, C->A (circular triangle)
/// Quaternary: A->B, A->C, B->D, C->D (diamond pattern)
fn detect_fusions(edges: &[Value]) -> Vec<Value> {
    // Build adjacency from "uses" edges only (skip "contains")
    let mut adj: HashMap<String, HashSet<String>> = HashMap::new();
    for edge in edges {
        let kind = edge["kind"].as_str().unwrap_or("");
        if kind != "uses" {
            continue;
        }
        let src = edge["source"].as_str().unwrap_or("").to_string();
        let tgt = edge["target"].as_str().unwrap_or("").to_string();
        if !src.is_empty() && !tgt.is_empty() {
            adj.entry(src).or_default().insert(tgt);
        }
    }

    // Normalize to top-level modules for cleaner fusion detection
    let mut mod_adj: HashMap<String, HashSet<String>> = HashMap::new();
    for (src, targets) in &adj {
        let src_mod = src.split("::").next().unwrap_or(src).to_string();
        for tgt in targets {
            let tgt_mod = tgt.split("::").next().unwrap_or(tgt).to_string();
            if src_mod != tgt_mod {
                mod_adj.entry(src_mod.clone()).or_default().insert(tgt_mod);
            }
        }
    }

    let all_mods: Vec<String> = mod_adj.keys().cloned().collect();
    let mut fusions: Vec<Value> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Binary fusions (A->B where B->A too, i.e., mutual dependency)
    for (a, a_targets) in &mod_adj {
        for b in a_targets {
            if let Some(b_targets) = mod_adj.get(b) {
                if b_targets.contains(a) {
                    let mut pair = vec![a.clone(), b.clone()];
                    pair.sort();
                    let key = format!("binary:{}", pair.join(","));
                    if seen.insert(key) {
                        fusions.push(json!({
                            "level": "binary",
                            "members": pair,
                            "description": format!("Mutual dependency between {} and {}", pair[0], pair[1]),
                        }));
                    }
                }
            }
        }
    }

    // Trinary fusions (A->B, B->C, C->A)
    for a in &all_mods {
        if let Some(a_targets) = mod_adj.get(a) {
            for b in a_targets {
                if let Some(b_targets) = mod_adj.get(b) {
                    for c in b_targets {
                        if c != a && c != b {
                            if let Some(c_targets) = mod_adj.get(c) {
                                if c_targets.contains(a) {
                                    let mut tri = vec![a.clone(), b.clone(), c.clone()];
                                    tri.sort();
                                    let key = format!("trinary:{}", tri.join(","));
                                    if seen.insert(key) {
                                        fusions.push(json!({
                                            "level": "trinary",
                                            "members": tri,
                                            "description": format!(
                                                "Circular dependency: {} -> {} -> {} -> {}",
                                                a, b, c, a
                                            ),
                                        }));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Quaternary fusions (diamond: A->B, A->C, B->D, C->D)
    for a in &all_mods {
        if let Some(a_targets) = mod_adj.get(a) {
            let a_list: Vec<&String> = a_targets.iter().collect();
            for i in 0..a_list.len() {
                for j in (i + 1)..a_list.len() {
                    let b = a_list[i];
                    let c = a_list[j];
                    if let (Some(b_targets), Some(c_targets)) =
                        (mod_adj.get(b), mod_adj.get(c))
                    {
                        let shared: Vec<&String> = b_targets
                            .intersection(c_targets)
                            .filter(|d| *d != a && *d != b && *d != c)
                            .collect();
                        for d in shared {
                            let mut quad = vec![a.clone(), b.clone(), c.clone(), d.clone()];
                            quad.sort();
                            let key = format!("quaternary:{}", quad.join(","));
                            if seen.insert(key) {
                                fusions.push(json!({
                                    "level": "quaternary",
                                    "members": quad,
                                    "description": format!(
                                        "Diamond pattern: {} -> {{{}, {}}} -> {}",
                                        a, b, c, d
                                    ),
                                }));
                            }
                        }
                    }
                }
            }
        }
    }

    fusions
}
