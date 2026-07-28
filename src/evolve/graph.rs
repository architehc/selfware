//! Deterministic file-level graph for production code, tests, and examples.
//!
//! Two-graph model: this is the CANONICAL repository graph — structurally
//! validated, consumed by the evolve composer and the web UI. New graph
//! features go here. `analysis::code_graph` is a separate, CLI/visualization
//! graph that powers `selfware graph` output formats (DOT/Mermaid/ASCII); do
//! not merge or cross-wire the two.

use anyhow::Result;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::ast::cfg_attributes_require_test;
use super::dedup::{DeduplicationAnalyzer, DuplicateKind};
use super::quality::QualityAnalyzer;
use super::{AstAnalyzer, Edge, EdgeType, Graph, Node};

pub struct GraphBuilder {
    src_root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepositoryFileClass {
    Production,
    Test,
    Example,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepositorySourceFile {
    pub path: PathBuf,
    pub classification: RepositoryFileClass,
}

impl GraphBuilder {
    pub fn new(src_root: impl AsRef<Path>) -> Self {
        Self {
            src_root: src_root.as_ref().to_path_buf(),
        }
    }

    /// Build one node per supported source file. IDs are derived from
    /// repository-relative paths and remain stable across scan order and
    /// machine paths.
    pub fn scan_src(&self) -> Result<Graph> {
        let project_root = self
            .src_root
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let analyzer = QualityAnalyzer::static_only();
        let partitioner = AstAnalyzer::new();
        let mut nodes = Vec::new();

        let inventory = repository_source_inventory(project_root)?;
        let source_files = inventory
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        let test_partition = discover_test_sources(project_root, &source_files)?;
        for entry in inventory {
            let path = entry.path;
            let relative = path.strip_prefix(project_root).unwrap_or(&path);
            let classification = if test_partition.files.contains(&path) {
                RepositoryFileClass::Test
            } else {
                entry.classification
            };
            let source_set = match classification {
                RepositoryFileClass::Test => SourceSet::Test,
                RepositoryFileClass::Example => SourceSet::Example,
                RepositoryFileClass::Production if relative.starts_with("src") => SourceSet::Code,
                RepositoryFileClass::Production => SourceSet::Tooling,
            };
            let id = file_node_id(relative, source_set);
            let file_class = classify_file(relative);
            // Implementation sources under `src` count as Code: Rust plus the
            // other common implementation languages (Python, JS/TS, Go), so a
            // non-Rust project still gets Code nodes and the context tiers
            // ship something. Web assets, data, config, scripts, and vendored
            // files — even under `src` — become Auxiliary so they don't
            // inflate the code token tiers.
            // Benchmark tooling under src/ is tooling, not product code:
            // excluding it from the code tiers saves ~77k full / ~10k lite
            // tokens of context that never describes the product
            // (DEBLOAT_STATE.md).
            let is_tooling_module = {
                let module = relative
                    .components()
                    .nth(1)
                    .and_then(|c| c.as_os_str().to_str());
                matches!(module, Some("bench_harness") | Some("vlm_bench"))
            };
            let is_code = matches!(source_set, SourceSet::Code)
                && is_production_source_class(file_class)
                && !is_tooling_module;
            let mut node = match source_set {
                SourceSet::Test | SourceSet::Example => {
                    let mut n = Node::test(&id, &path_string(relative));
                    n.classification = file_class.to_string();
                    n
                }
                SourceSet::Code if is_code => {
                    let mut n = Node::code(&id, &path_string(relative));
                    n.classification = file_class.to_string();
                    n
                }
                // Non-source files under src (web assets, markup, shell
                // scripts) and everything outside src (data, scripts, config,
                // vendored) stay out of the code tiers.
                SourceSet::Code | SourceSet::Tooling => {
                    Node::auxiliary(&id, &path_string(relative), file_class)
                }
            };
            populate_metrics(&mut node, &path, &partitioner)?;
            if is_rust_source(&path) {
                analyze_quality(&analyzer, &mut node, &id);
            }
            nodes.push(node);
        }
        let hierarchy_edges = add_repository_hierarchy(&mut nodes, project_root);
        nodes.sort_by(|left, right| left.id.cmp(&right.id));

        let node_ids: HashSet<String> = nodes.iter().map(|node| node.id.clone()).collect();
        let mut edges = hierarchy_edges;
        edges.extend(containment_edges(&node_ids));
        let path_to_id: HashMap<PathBuf, String> = nodes
            .iter()
            .filter_map(|node| {
                node.path
                    .as_deref()
                    .map(|path| (PathBuf::from(path), node.id.clone()))
            })
            .collect();
        for (test_path, owner_path) in test_partition.owners {
            let (Some(test_id), Some(owner_id)) =
                (path_to_id.get(&test_path), path_to_id.get(&owner_path))
            else {
                continue;
            };
            if test_id != owner_id {
                edges.push(Edge {
                    from: owner_id.clone(),
                    to: test_id.clone(),
                    edge_type: EdgeType::Contains,
                });
            }
        }

        for node in &nodes {
            let Some(path) = node.path.as_deref() else {
                continue;
            };
            let full_path = project_root.join(path);
            for imported in imported_modules(&full_path, &node.id) {
                if let Some(target) = resolve_import(&imported, &node_ids) {
                    if target != node.id {
                        edges.push(Edge {
                            from: node.id.clone(),
                            to: target,
                            edge_type: EdgeType::DependsOn,
                        });
                    }
                }
            }
        }

        let mut graph = Graph { nodes, edges };
        let dedup = DeduplicationAnalyzer::new();
        match dedup.find_duplicates(&graph) {
            Ok(duplicates) => {
                for pair in duplicates {
                    graph.edges.push(Edge {
                        from: pair.first,
                        to: pair.second,
                        edge_type: match pair.kind {
                            DuplicateKind::Exact => EdgeType::DuplicateOf,
                            DuplicateKind::Near => EdgeType::SimilarTo,
                        },
                    });
                }
            }
            Err(error) => eprintln!("dedup analysis skipped: {error}"),
        }
        graph.edges.sort_by(|left, right| {
            left.from
                .cmp(&right.from)
                .then_with(|| left.to.cmp(&right.to))
                .then_with(|| {
                    edge_type_order(&left.edge_type).cmp(&edge_type_order(&right.edge_type))
                })
        });
        graph.edges.dedup_by(|left, right| {
            left.from == right.from && left.to == right.to && left.edge_type == right.edge_type
        });
        Ok(graph)
    }
}

/// Enumerate the repository text files that the graph and IDE are allowed to
/// expose. The allowlist is intentionally format-based and prunes generated,
/// dependency, private-state, and credential locations before descent.
pub(crate) fn repository_source_inventory(
    project_root: &Path,
) -> Result<Vec<RepositorySourceFile>> {
    if !project_root.exists() {
        return Ok(Vec::new());
    }
    let root_metadata = std::fs::symlink_metadata(project_root)?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        anyhow::bail!("repository root must be a real directory");
    }

    let mut paths = Vec::new();
    let walker = walkdir::WalkDir::new(project_root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || !entry.file_type().is_dir()
                || !is_excluded_repository_directory(entry.file_name())
        });
    for entry in walker {
        let entry = entry?;
        if !entry.file_type().is_file() || entry.file_type().is_symlink() {
            continue;
        }
        let relative = entry.path().strip_prefix(project_root)?;
        if !repository_relative_path_supported(relative) || !is_utf8_text(entry.path()) {
            continue;
        }
        paths.push(entry.into_path());
    }
    paths.sort();

    let test_partition = discover_test_sources(project_root, &paths)?;
    Ok(paths
        .into_iter()
        .map(|path| {
            let relative = path.strip_prefix(project_root).unwrap_or(&path);
            let classification =
                if test_partition.files.contains(&path) || structurally_test_path(relative) {
                    RepositoryFileClass::Test
                } else if structurally_example_path(relative) {
                    RepositoryFileClass::Example
                } else {
                    RepositoryFileClass::Production
                };
            RepositorySourceFile {
                path,
                classification,
            }
        })
        .collect())
}

/// Validate a repository-relative document name without opening it. Existing
/// files are additionally checked for regular-file, UTF-8, and symlink safety
/// by the IDE snapshot path.
pub(crate) fn repository_relative_path_supported(path: &Path) -> bool {
    let components = path
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => Some(value),
            _ => None,
        })
        .collect::<Vec<_>>();
    if components.is_empty()
        || components
            .iter()
            .take(components.len().saturating_sub(1))
            .any(|component| is_excluded_repository_directory(component))
    {
        return false;
    }

    let file_name = components
        .last()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if is_private_repository_file(path, file_name) {
        return false;
    }
    is_graph_source(path)
}

fn is_excluded_repository_directory(name: &std::ffi::OsStr) -> bool {
    matches!(
        name.to_str().unwrap_or_default(),
        ".git"
            | ".selfware"
            | ".worktrees"
            | "target"
            | "node_modules"
            | "vendor"
            | "dist"
            | "build"
            | "out"
            | "coverage"
            | "artifacts"
            | "__pycache__"
            | ".next"
            | ".nuxt"
            | ".svelte-kit"
            | ".turbo"
            | ".cache"
            | ".pytest_cache"
            | ".mypy_cache"
            | ".ruff_cache"
            | ".venv"
            | "venv"
            | ".ssh"
            | ".aws"
            | ".gnupg"
    )
}

fn is_private_repository_file(path: &Path, file_name: &str) -> bool {
    if file_name == "codegraph.json"
        || file_name == "selfware.toml"
        || file_name == ".env"
        || file_name.starts_with(".env.")
        || matches!(
            file_name,
            ".npmrc" | ".pypirc" | ".netrc" | ".authinfo" | "package-lock.json" | "composer.lock"
        )
    {
        return true;
    }

    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "toml" | "yaml" | "yml" | "json" | "jsonc"
    ) && matches!(
        stem.as_str(),
        "secret" | "secrets" | "credential" | "credentials" | "auth" | "tokens"
    )
}

fn is_utf8_text(path: &Path) -> bool {
    std::fs::read(path)
        .is_ok_and(|bytes| !bytes.contains(&0) && std::str::from_utf8(&bytes).is_ok())
}

fn structurally_test_path(relative: &Path) -> bool {
    let components = relative
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();
    components.first().is_some_and(|component| {
        matches!(*component, "tests" | "system_tests" | "benches" | "fuzz")
    }) || components
        .iter()
        .take(components.len().saturating_sub(1))
        .any(|component| matches!(*component, "test" | "tests" | "__tests__"))
        || extracted_test_name(relative)
}

fn structurally_example_path(relative: &Path) -> bool {
    relative.components().any(|component| {
        matches!(
            component,
            std::path::Component::Normal(value) if value == "examples"
        )
    })
}

struct TestSourcePartition {
    files: HashSet<PathBuf>,
    owners: HashMap<PathBuf, PathBuf>,
}

fn discover_test_sources(src_root: &Path, source_files: &[PathBuf]) -> Result<TestSourcePartition> {
    let source_set: HashSet<PathBuf> = source_files.iter().cloned().collect();
    let mut test_only: HashSet<PathBuf> = source_files
        .iter()
        .filter(|path| extracted_test_name(path))
        .cloned()
        .collect();
    let mut module_edges: Vec<(PathBuf, PathBuf, bool)> = Vec::new();

    for source in source_files {
        if !is_rust_source(source) {
            continue;
        }
        let content = std::fs::read_to_string(source)?;
        let Ok(file) = syn::parse_file(&content) else {
            continue;
        };
        collect_external_modules(source, &file.items, &source_set, &mut module_edges);
    }

    for (_, target, requires_test) in &module_edges {
        if *requires_test {
            test_only.insert(target.clone());
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for (parent, target, _) in &module_edges {
            if test_only.contains(parent) && test_only.insert(target.clone()) {
                changed = true;
            }
        }
    }

    test_only.retain(|path| path.starts_with(src_root) && source_set.contains(path));
    let owners = module_edges
        .into_iter()
        .filter(|(_, target, _)| test_only.contains(target))
        .map(|(owner, target, _)| (target, owner))
        .collect();
    Ok(TestSourcePartition {
        files: test_only,
        owners,
    })
}

fn extracted_test_name(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    name.ends_with("_test.rs")
        || name.ends_with("_tests.rs")
        || name.contains(".test.")
        || name.contains(".spec.")
        || (extension == "py" && (name.starts_with("test_") || name.ends_with("_test.py")))
        || (extension == "go" && name.ends_with("_test.go"))
        || (extension == "rb" && name.ends_with("_spec.rb"))
}

fn collect_external_modules(
    source: &Path,
    items: &[syn::Item],
    source_files: &HashSet<PathBuf>,
    output: &mut Vec<(PathBuf, PathBuf, bool)>,
) {
    for item in items {
        let syn::Item::Mod(module) = item else {
            continue;
        };
        if module.content.is_some() {
            continue;
        }

        let requires_test = cfg_attributes_require_test(&module.attrs);
        for target in external_module_candidates(source, module) {
            if source_files.contains(&target) {
                output.push((source.to_path_buf(), target, requires_test));
            }
        }
    }
}

fn external_module_candidates(source: &Path, module: &syn::ItemMod) -> Vec<PathBuf> {
    let source_dir = source.parent().unwrap_or_else(|| Path::new(""));
    if let Some(path) = module.attrs.iter().find_map(path_attribute) {
        return vec![source_dir.join(path)];
    }

    let file_name = source.file_name().and_then(|name| name.to_str());
    let module_dir = if matches!(file_name, Some("lib.rs" | "main.rs" | "mod.rs")) {
        source_dir.to_path_buf()
    } else {
        source_dir.join(source.file_stem().unwrap_or_default())
    };
    let name = module.ident.to_string();
    vec![
        module_dir.join(format!("{name}.rs")),
        module_dir.join(name).join("mod.rs"),
    ]
}

fn path_attribute(attribute: &syn::Attribute) -> Option<PathBuf> {
    if !attribute.path().is_ident("path") {
        return None;
    }
    let syn::Meta::NameValue(value) = &attribute.meta else {
        return None;
    };
    let syn::Expr::Lit(expression) = &value.value else {
        return None;
    };
    let syn::Lit::Str(path) = &expression.lit else {
        return None;
    };
    Some(PathBuf::from(path.value()))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SourceSet {
    Code,
    Test,
    Example,
    Tooling,
}

fn file_node_id(relative: &Path, source_set: SourceSet) -> String {
    let mut parts = relative
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if let Some(last) = parts.last_mut() {
        if last.ends_with(".rs") {
            *last = last.strip_suffix(".rs").unwrap_or(last).to_string();
        }
    }

    match source_set {
        SourceSet::Code => {
            if parts.first().is_some_and(|part| part == "src") {
                parts.remove(0);
            }
            if parts.first().is_some_and(|part| part == "bin") {
                parts.remove(0);
                return format!("bin::{}", normalized_module_parts(parts).join("::"));
            }
            if parts.as_slice() == ["lib"] {
                return "crate".to_string();
            }
            format!("crate::{}", normalized_module_parts(parts).join("::"))
        }
        SourceSet::Test => {
            if parts
                .first()
                .is_some_and(|part| matches!(part.as_str(), "tests" | "src"))
            {
                parts.remove(0);
            }
            format!("test::{}", normalized_module_parts(parts).join("::"))
        }
        SourceSet::Example => {
            if parts.first().is_some_and(|part| part == "examples") {
                parts.remove(0);
            }
            format!("example::{}", normalized_module_parts(parts).join("::"))
        }
        SourceSet::Tooling => {
            format!("tool::{}", normalized_module_parts(parts).join("::"))
        }
    }
}

fn normalized_module_parts(mut parts: Vec<String>) -> Vec<String> {
    if parts.last().is_some_and(|part| part == "mod") {
        parts.pop();
    }
    if parts.is_empty() {
        parts.push("root".to_string());
    }
    parts
}

const REPOSITORY_STRUCTURE_ID: &str = "structure::repository";

fn add_repository_hierarchy(nodes: &mut Vec<Node>, project_root: &Path) -> Vec<Edge> {
    let files = nodes
        .iter()
        .filter_map(|node| {
            let path = Path::new(node.path.as_deref()?);
            let relative = path
                .strip_prefix(project_root)
                .unwrap_or(path)
                .to_path_buf();
            Some((node.id.clone(), relative))
        })
        .collect::<Vec<_>>();
    if files.is_empty() {
        return Vec::new();
    }

    let mut directories = BTreeSet::new();
    for (_, relative) in &files {
        let Some(parent) = relative.parent() else {
            continue;
        };
        let mut current = PathBuf::new();
        for component in parent.components() {
            if let std::path::Component::Normal(component) = component {
                current.push(component);
                directories.insert(current.clone());
            }
        }
    }

    nodes.push(Node::structure(REPOSITORY_STRUCTURE_ID));
    nodes.extend(
        directories
            .iter()
            .map(|directory| Node::structure(&directory_structure_id(directory))),
    );

    let mut edges = Vec::new();
    for directory in &directories {
        let parent = directory
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(directory_structure_id)
            .unwrap_or_else(|| REPOSITORY_STRUCTURE_ID.to_string());
        edges.push(Edge {
            from: parent,
            to: directory_structure_id(directory),
            edge_type: EdgeType::Contains,
        });
    }
    for (node_id, relative) in files {
        let parent = relative
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(directory_structure_id)
            .unwrap_or_else(|| REPOSITORY_STRUCTURE_ID.to_string());
        edges.push(Edge {
            from: parent,
            to: node_id,
            edge_type: EdgeType::Contains,
        });
    }
    edges
}

fn directory_structure_id(path: &Path) -> String {
    let suffix = path
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(component) => {
                Some(component.to_string_lossy().replace("::", "%3A%3A"))
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("::");
    format!("structure::directory::{suffix}")
}

fn containment_edges(node_ids: &HashSet<String>) -> Vec<Edge> {
    let mut edges = Vec::new();
    for id in node_ids {
        let parent = parent_node_id(id, node_ids);
        if let Some(parent) = parent {
            edges.push(Edge {
                from: parent,
                to: id.clone(),
                edge_type: EdgeType::Contains,
            });
        }
    }
    edges
}

fn parent_node_id(id: &str, node_ids: &HashSet<String>) -> Option<String> {
    let mut parts = id.split("::").collect::<Vec<_>>();
    while parts.len() > 1 {
        parts.pop();
        let candidate = parts.join("::");
        if node_ids.contains(&candidate) {
            return Some(candidate);
        }
        if let Some(stripped) = candidate.strip_prefix("test::") {
            let crate_candidate = format!("crate::{}", stripped);
            if node_ids.contains(&crate_candidate) {
                return Some(crate_candidate);
            }
        }
    }
    (id.starts_with("crate::") && id != "crate" && node_ids.contains("crate"))
        .then(|| "crate".to_string())
}

fn imported_modules(path: &Path, current_id: &str) -> Vec<Vec<String>> {
    if !is_rust_source(path) {
        return Vec::new();
    }
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(file) = syn::parse_file(&content) else {
        return fallback_imports(&content)
            .into_iter()
            .filter_map(|parts| normalize_import(parts, current_id))
            .collect();
    };
    let mut imports = Vec::new();
    for item in file.items {
        if let syn::Item::Use(item_use) = item {
            flatten_use_tree(&item_use.tree, Vec::new(), &mut imports);
        }
    }
    imports
        .into_iter()
        .filter_map(|parts| normalize_import(parts, current_id))
        .collect()
}

fn flatten_use_tree(tree: &syn::UseTree, prefix: Vec<String>, output: &mut Vec<Vec<String>>) {
    match tree {
        syn::UseTree::Path(path) => {
            let mut next = prefix;
            next.push(path.ident.to_string());
            flatten_use_tree(&path.tree, next, output);
        }
        syn::UseTree::Name(name) => {
            let mut complete = prefix;
            complete.push(name.ident.to_string());
            output.push(complete);
        }
        syn::UseTree::Rename(rename) => {
            let mut complete = prefix;
            complete.push(rename.ident.to_string());
            output.push(complete);
        }
        syn::UseTree::Glob(_) => output.push(prefix),
        syn::UseTree::Group(group) => {
            for item in &group.items {
                flatten_use_tree(item, prefix.clone(), output);
            }
        }
    }
}

fn normalize_import(mut parts: Vec<String>, current_id: &str) -> Option<Vec<String>> {
    let first = parts.first()?.as_str();
    match first {
        "crate" | "selfware" => {
            parts.remove(0);
            let mut normalized = vec!["crate".to_string()];
            normalized.extend(parts);
            Some(normalized)
        }
        "self" => {
            parts.remove(0);
            let mut normalized = current_id
                .split("::")
                .map(str::to_string)
                .collect::<Vec<_>>();
            normalized.extend(parts);
            Some(normalized)
        }
        "super" => {
            let mut normalized = current_id
                .split("::")
                .map(str::to_string)
                .collect::<Vec<_>>();
            while parts.first().is_some_and(|part| part == "super") {
                parts.remove(0);
                normalized.pop();
            }
            normalized.extend(parts);
            Some(normalized)
        }
        _ => None,
    }
}

fn fallback_imports(content: &str) -> Vec<Vec<String>> {
    content
        .lines()
        .filter_map(|line| {
            let import = line
                .trim_start()
                .strip_prefix("use ")
                .or_else(|| line.trim_start().strip_prefix("pub use "))?;
            let path = import.split([';', '{']).next()?.trim();
            Some(path.split("::").map(str::to_string).collect())
        })
        .collect()
}

fn resolve_import(parts: &[String], node_ids: &HashSet<String>) -> Option<String> {
    let mut candidate = parts.to_vec();
    while !candidate.is_empty() {
        let id = candidate.join("::");
        if node_ids.contains(&id) {
            return Some(id);
        }
        candidate.pop();
    }
    None
}

fn analyze_quality(analyzer: &QualityAnalyzer, node: &mut Node, label: &str) {
    if let Err(error) = analyzer.analyze_node(node) {
        eprintln!("quality analysis skipped for {label}: {error}");
    }
}

fn populate_metrics(node: &mut Node, path: &Path, partitioner: &AstAnalyzer) -> Result<()> {
    let content = std::fs::read_to_string(path)?;
    node.lines = content.lines().count();
    // Real tokenizer (HF model-matched → tiktoken cl100k_base), cached per
    // content hash, instead of the bytes/4 heuristic — precise context sizes.
    node.tokens = crate::token_count::estimate_content_tokens(&content);
    node.files = 1;
    let ranges = if is_rust_source(path) {
        partitioner.cfg_test_body_ranges(&content)?
    } else {
        Vec::new()
    };
    node.inline_test_ranges = ranges.len();
    node.inline_test_lines = ranges
        .iter()
        .map(|(start, end)| end.saturating_sub(*start) + 1)
        .sum();
    node.inline_test_tokens = ranges
        .iter()
        .map(|(start, end)| {
            let text: String = content
                .split_inclusive('\n')
                .enumerate()
                .filter(|(index, _)| {
                    let line = index + 1;
                    line >= *start && line <= *end
                })
                .map(|(_, line)| line)
                .collect();
            crate::token_count::estimate_content_tokens(&text)
        })
        .sum();
    Ok(())
}

fn is_rust_source(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "rs")
}

/// Implementation-source classes admitted to the Code layer (and therefore to
/// the code token tiers). Data, config, markup, shell scripts, and vendored
/// files stay Auxiliary even under `src`.
fn is_production_source_class(file_class: &str) -> bool {
    matches!(
        file_class,
        "rust_source" | "python_source" | "javascript_source" | "typescript_source" | "go_source"
    )
}

/// Fine-grained content classification for a repository file, used both to keep
/// non-source content out of the code token tiers and to drive graph
/// perspectives.
fn classify_file(relative: &Path) -> &'static str {
    let p = relative.to_string_lossy().replace('\\', "/");
    if p.contains("vendor/")
        || p.contains("node_modules/")
        || p.starts_with("vscode-selfware")
        || p.starts_with("zed-extension")
    {
        return "vendored";
    }
    let ext = relative
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "rs" => "rust_source",
        "py" | "pyw" => "python_source",
        "js" | "mjs" | "cjs" | "jsx" => "javascript_source",
        "ts" | "tsx" => "typescript_source",
        "go" => "go_source",
        "json" | "jsonl" | "ndjson" | "csv" | "tsv" | "parquet" => "data",
        "toml" | "yaml" | "yml" | "lock" | "cfg" | "ini" | "conf" | "editorconfig" => "config",
        "sh" | "bash" | "zsh" | "ps1" | "rb" | "pl" => "script",
        "html" | "htm" | "css" | "scss" | "sass" | "md" | "markdown" | "txt" | "svg" | "rst" => {
            "markup"
        }
        _ => {
            if p.contains("results/") || p.contains("fixtures/") || p.contains("snapshots/") {
                "data"
            } else {
                "other"
            }
        }
    }
}

fn is_graph_source(path: &Path) -> bool {
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            matches!(
                name,
                "Makefile"
                    | "Dockerfile"
                    | "Jenkinsfile"
                    | "Procfile"
                    | ".gitignore"
                    | ".dockerignore"
                    | ".prettierrc"
            )
        })
    {
        return true;
    }
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "rs" | "js"
                    | "jsx"
                    | "mjs"
                    | "cjs"
                    | "ts"
                    | "tsx"
                    | "html"
                    | "css"
                    | "scss"
                    | "less"
                    | "vue"
                    | "svelte"
                    | "py"
                    | "pyw"
                    | "go"
                    | "java"
                    | "kt"
                    | "kts"
                    | "c"
                    | "h"
                    | "cc"
                    | "cpp"
                    | "cxx"
                    | "hpp"
                    | "hh"
                    | "cs"
                    | "rb"
                    | "php"
                    | "swift"
                    | "sh"
                    | "bash"
                    | "zsh"
                    | "sql"
                    | "proto"
                    | "graphql"
                    | "gql"
                    | "lua"
                    | "swl"
                    | "toml"
                    | "yaml"
                    | "yml"
                    | "json"
                    | "jsonc"
                    | "xml"
            )
        })
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn edge_type_order(edge_type: &EdgeType) -> u8 {
    match edge_type {
        EdgeType::Contains => 0,
        EdgeType::DependsOn => 1,
        EdgeType::Influences => 2,
        EdgeType::Feedback => 3,
        EdgeType::ContextIncluded => 4,
        EdgeType::DuplicateOf => 5,
        EdgeType::SimilarTo => 6,
    }
}
