//! GraphIndex: a read-only, derived query index over an `Arc<Graph>`.
//!
//! Built once per graph revision (see `evolve::graph_cache`), the index
//! precomputes the adjacency maps the graph query tools need so per-call
//! work is a hash lookup instead of an O(edges) scan. It never mutates the
//! underlying graph.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;

use super::clusters::component_of;
use super::{EdgeType, Graph, Node, NodeLayer};

/// Hotspot ranking metric for [`GraphIndex::hotspots`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    /// Raw measured token size of the node.
    Tokens,
    /// Complexity score (nodes without one rank last).
    Complexity,
    /// Complexity per line; falls back to tokens per line when the node
    /// carries no complexity measurement.
    Density,
}

impl Metric {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "tokens" => Some(Self::Tokens),
            "complexity" => Some(Self::Complexity),
            "density" => Some(Self::Density),
            _ => None,
        }
    }
}

/// Edge direction relative to a queried node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Incoming edges (`from` is the other endpoint).
    In,
    /// Outgoing edges (`to` is the other endpoint).
    Out,
    /// Both directions.
    Both,
}

impl Direction {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "in" => Some(Self::In),
            "out" => Some(Self::Out),
            "both" => Some(Self::Both),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::In => "in",
            Self::Out => "out",
            Self::Both => "both",
        }
    }

    fn allows(&self, direction: Direction) -> bool {
        *self == Direction::Both || *self == direction
    }
}

/// Snake-case name of an edge type, as tool schemas and rows spell it.
pub fn edge_type_name(edge_type: &EdgeType) -> &'static str {
    match edge_type {
        EdgeType::Contains => "contains",
        EdgeType::DependsOn => "depends_on",
        EdgeType::Influences => "influences",
        EdgeType::Feedback => "feedback",
        EdgeType::ContextIncluded => "context_included",
        EdgeType::DuplicateOf => "duplicate_of",
        EdgeType::SimilarTo => "similar_to",
    }
}

/// Parse a tool-schema edge kind (`depends_on`, `contains`, `duplicate_of`,
/// `similar_to`); `"all"` and unknown kinds return `None`.
pub fn parse_edge_kind(s: &str) -> Option<EdgeType> {
    match s.trim().to_lowercase().as_str() {
        "depends_on" => Some(EdgeType::DependsOn),
        "contains" => Some(EdgeType::Contains),
        "duplicate_of" => Some(EdgeType::DuplicateOf),
        "similar_to" => Some(EdgeType::SimilarTo),
        _ => None,
    }
}

/// Split a query into GraphRag-style terms: lowercase alphanumeric runs of
/// 3+ characters (`evolve::graphrag::query` uses the same rule).
pub fn split_terms(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .map(|w| w.to_lowercase())
        .collect()
}

/// Count of `terms` appearing as substrings of the node's id or path
/// (the GraphRag candidate rule, without the source read).
pub fn lexical_hits(node: &Node, terms: &[String]) -> usize {
    let id = node.id.to_lowercase();
    let path = node.path.as_deref().unwrap_or("").to_lowercase();
    terms
        .iter()
        .filter(|term| id.contains(term.as_str()) || path.contains(term.as_str()))
        .count()
}

/// Read-only derived index over one revision of the evolve graph.
#[derive(Debug)]
pub struct GraphIndex {
    pub graph: Arc<Graph>,
    /// First 12 hex chars of the SHA-256 of the YAML the graph was parsed
    /// from — cheap staleness identity for tool responses.
    pub revision: String,
    by_id: HashMap<String, usize>,
    /// `DependsOn`, forward: id → ids it depends on.
    deps_out: HashMap<String, Vec<String>>,
    /// `DependsOn`, reverse: id → ids that depend on it (the blast radius).
    deps_in: HashMap<String, Vec<String>>,
    /// `Contains`, forward: id → ids contained in it (test files, dir members).
    contains_in: HashMap<String, Vec<String>>,
    by_component: BTreeMap<String, Vec<String>>,
}

impl GraphIndex {
    /// Derive the index from a graph; `yaml_hash` is the full SHA-256 hex of
    /// the YAML bytes and is truncated to the 12-char revision.
    pub fn from_graph(graph: Arc<Graph>, yaml_hash: &str) -> Self {
        let revision: String = yaml_hash.chars().take(12).collect();
        let mut by_id = HashMap::with_capacity(graph.nodes.len());
        for (position, node) in graph.nodes.iter().enumerate() {
            by_id.insert(node.id.clone(), position);
        }
        let mut deps_out: HashMap<String, Vec<String>> = HashMap::new();
        let mut deps_in: HashMap<String, Vec<String>> = HashMap::new();
        let mut contains_in: HashMap<String, Vec<String>> = HashMap::new();
        for edge in &graph.edges {
            match edge.edge_type {
                EdgeType::DependsOn => {
                    deps_out
                        .entry(edge.from.clone())
                        .or_default()
                        .push(edge.to.clone());
                    deps_in
                        .entry(edge.to.clone())
                        .or_default()
                        .push(edge.from.clone());
                }
                EdgeType::Contains => {
                    contains_in
                        .entry(edge.from.clone())
                        .or_default()
                        .push(edge.to.clone());
                }
                _ => {}
            }
        }
        for values in deps_out
            .values_mut()
            .chain(deps_in.values_mut())
            .chain(contains_in.values_mut())
        {
            values.sort();
            values.dedup();
        }
        let mut by_component: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for node in &graph.nodes {
            by_component
                .entry(component_of(&node.id))
                .or_default()
                .push(node.id.clone());
        }
        Self {
            graph,
            revision,
            by_id,
            deps_out,
            deps_in,
            contains_in,
            by_component,
        }
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.by_id
            .get(id)
            .map(|&position| &self.graph.nodes[position])
    }

    /// Ids that depend on `id` (reverse `DependsOn`) — the impact set.
    pub fn dependents(&self, id: &str) -> &[String] {
        self.deps_in.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Ids that `id` depends on (forward `DependsOn`).
    pub fn dependencies(&self, id: &str) -> &[String] {
        self.deps_out.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Dependency cycles over `DependsOn` edges, as node-id paths that close
    /// on themselves (last element repeats the first). At most `k` cycles,
    /// ordered deterministically by first id.
    ///
    /// This is deliberately NOT `ontology::validate_graph`: the validator
    /// excludes `DependsOn` edges from cycle detection (dependency cycles
    /// are structurally valid), but they are exactly the cycles worth
    /// surfacing for cleanup. Iterative Tarjan SCC — dependency chains can
    /// be deep enough to blow a recursive stack.
    pub fn dependency_cycles(&self, k: usize) -> Vec<Vec<String>> {
        let mut index_of: HashMap<&str, usize> = HashMap::new();
        let mut lowlink: HashMap<&str, usize> = HashMap::new();
        let mut on_stack: HashSet<&str> = HashSet::new();
        let mut stack: Vec<&str> = Vec::new();
        let mut next_index = 0usize;
        let mut sccs: Vec<Vec<&str>> = Vec::new();

        for node in &self.graph.nodes {
            let root = node.id.as_str();
            if index_of.contains_key(root) {
                continue;
            }
            index_of.insert(root, next_index);
            lowlink.insert(root, next_index);
            next_index += 1;
            stack.push(root);
            on_stack.insert(root);
            // Work stack: (node, next successor position).
            let mut work: Vec<(&str, usize)> = vec![(root, 0)];
            while let Some(&(v, position)) = work.last() {
                let successors: &[String] = self.deps_out.get(v).map(Vec::as_slice).unwrap_or(&[]);
                if position < successors.len() {
                    work.last_mut().expect("just read").1 += 1;
                    let w = successors[position].as_str();
                    if !self.by_id.contains_key(w) {
                        // Dangling edge target — not a real node, skip.
                        continue;
                    }
                    if !index_of.contains_key(w) {
                        index_of.insert(w, next_index);
                        lowlink.insert(w, next_index);
                        next_index += 1;
                        stack.push(w);
                        on_stack.insert(w);
                        work.push((w, 0));
                    } else if on_stack.contains(w) {
                        let low = lowlink[v].min(index_of[w]);
                        lowlink.insert(v, low);
                    }
                } else {
                    if lowlink[v] == index_of[v] {
                        let mut scc = Vec::new();
                        loop {
                            let w = stack.pop().expect("Tarjan stack underflow");
                            on_stack.remove(w);
                            scc.push(w);
                            if w == v {
                                break;
                            }
                        }
                        sccs.push(scc);
                    }
                    work.pop();
                    if let Some(&(parent, _)) = work.last() {
                        let low = lowlink[parent].min(lowlink[v]);
                        lowlink.insert(parent, low);
                    }
                }
            }
        }

        let mut cycles: Vec<Vec<String>> = Vec::new();
        for scc in &sccs {
            if scc.len() > 1 {
                cycles.push(self.witness_cycle(scc));
            } else {
                // Self-loop: node depends on itself.
                let v = scc[0];
                if self
                    .dependencies(v)
                    .iter()
                    .any(|target| target.as_str() == v)
                {
                    cycles.push(vec![v.to_string(), v.to_string()]);
                }
            }
        }
        cycles.sort();
        cycles.truncate(k);
        cycles
    }

    /// One concrete cycle through an SCC: greedy walk over member successors
    /// (strong connectivity guarantees one always exists) until a node
    /// repeats; the loop from the repeated node is the witness.
    fn witness_cycle(&self, scc: &[&str]) -> Vec<String> {
        let members: HashSet<&str> = scc.iter().copied().collect();
        let start = *scc.iter().min().expect("non-empty SCC");
        let mut path: Vec<&str> = vec![start];
        let mut visited: HashSet<&str> = HashSet::from([start]);
        let mut current = start;
        loop {
            let successors = self.dependencies(current);
            let next = successors
                .iter()
                .find(|s| members.contains(s.as_str()) && !visited.contains(s.as_str()))
                .or_else(|| successors.iter().find(|s| members.contains(s.as_str())))
                .expect("SCC member always has a member successor");
            path.push(next);
            if let Some(pos) = path[..path.len() - 1].iter().position(|x| x == next) {
                return path[pos..].iter().map(|s| s.to_string()).collect();
            }
            visited.insert(next);
            current = next;
        }
    }

    /// Ids directly contained in `id` via `Contains` edges.
    pub fn contained(&self, id: &str) -> &[String] {
        self.contains_in.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Component → member ids, via `clusters::component_of`.
    pub fn by_component(&self) -> &BTreeMap<String, Vec<String>> {
        &self.by_component
    }

    /// Every edge touching `id`, as (other endpoint, edge type); outgoing
    /// edges report `to`, incoming report `from`. Filtered by `kind` when set.
    pub fn neighbors(&self, id: &str, kind: Option<EdgeType>) -> Vec<(String, EdgeType)> {
        self.directed_neighbors(id, kind.as_ref(), Direction::Both)
            .into_iter()
            .map(|(other, edge_type, _)| (other, edge_type))
            .collect()
    }

    /// Every edge touching `id`, as (other endpoint, edge type, direction
    /// relative to `id`). Filtered by `kind` and `direction`; sorted by the
    /// other endpoint's id so results are deterministic.
    pub fn directed_neighbors(
        &self,
        id: &str,
        kind: Option<&EdgeType>,
        direction: Direction,
    ) -> Vec<(String, EdgeType, Direction)> {
        let wanted = |edge_type: &EdgeType| kind.is_none_or(|k| k == edge_type);
        let mut out = Vec::new();
        for edge in &self.graph.edges {
            if !wanted(&edge.edge_type) {
                continue;
            }
            if edge.from == id && direction.allows(Direction::Out) {
                out.push((edge.to.clone(), edge.edge_type.clone(), Direction::Out));
            } else if edge.to == id && direction.allows(Direction::In) {
                out.push((edge.from.clone(), edge.edge_type.clone(), Direction::In));
            }
        }
        out.sort_by(|left, right| left.0.cmp(&right.0));
        out
    }

    /// Reverse-`DependsOn` BFS from `id`, ordered by depth then id, capped at
    /// `max_depth` hops. The seed itself is never included.
    pub fn impact_closure(&self, id: &str, max_depth: usize) -> Vec<String> {
        self.impact_frontier(id, max_depth)
            .into_iter()
            .map(|(id, _, _)| id)
            .collect()
    }

    /// [`impact_closure`](Self::impact_closure) with provenance: each row is
    /// (dependent id, depth, id it depends through), so callers can rank by
    /// depth and name the connecting edge.
    pub fn impact_frontier(&self, id: &str, max_depth: usize) -> Vec<(String, usize, String)> {
        let mut seen: HashSet<&str> = HashSet::from([id]);
        let mut queue: VecDeque<&str> = VecDeque::from([id]);
        let mut out = Vec::new();
        let mut depth = 0;
        while !queue.is_empty() && depth < max_depth {
            for _ in 0..queue.len() {
                let Some(current) = queue.pop_front() else {
                    break;
                };
                for dependent in self.dependents(current) {
                    if seen.insert(dependent.as_str()) {
                        out.push((dependent.clone(), depth + 1, current.to_string()));
                        queue.push_back(dependent);
                    }
                }
            }
            depth += 1;
        }
        out
    }

    /// Test-layer nodes `Contains`-linked from `id` (the owner → test edges
    /// written by `GraphBuilder`).
    pub fn tests_for(&self, id: &str) -> Vec<String> {
        self.contained(id)
            .iter()
            .filter(|child| {
                self.node(child)
                    .is_some_and(|node| node.layer == NodeLayer::Test)
            })
            .cloned()
            .collect()
    }

    /// Top-`k` nodes by `metric`, optionally restricted to one layer and
    /// optionally excluding nodes whose id or path starts with
    /// `exclude_prefix` (the filter is applied before ranking, so `k` counts
    /// post-filter rows). Ties break by id so results are deterministic.
    pub fn hotspots(
        &self,
        metric: Metric,
        layer: Option<NodeLayer>,
        exclude_prefix: Option<&str>,
        k: usize,
    ) -> Vec<&Node> {
        let score = |node: &Node| -> f64 {
            match metric {
                Metric::Tokens => node.tokens as f64,
                Metric::Complexity => node.complexity.unwrap_or(0.0),
                Metric::Density => {
                    let lines = node.lines.max(1) as f64;
                    match node.complexity {
                        Some(complexity) => complexity / lines,
                        None => node.tokens as f64 / lines,
                    }
                }
            }
        };
        let excluded = |node: &Node| {
            exclude_prefix.is_some_and(|prefix| {
                node.id.starts_with(prefix)
                    || node
                        .path
                        .as_deref()
                        .is_some_and(|path| path.starts_with(prefix))
            })
        };
        let mut nodes: Vec<&Node> = self
            .graph
            .nodes
            .iter()
            .filter(|node| layer.is_none_or(|wanted| node.layer == wanted))
            // Symbol nodes are opt-in (`layer: "symbol"`): the default
            // all-layers view stays file-level so file queries don't change.
            .filter(|node| layer.is_some() || node.layer != NodeLayer::Symbol)
            .filter(|node| !excluded(node))
            .collect();
        nodes.sort_by(|left, right| {
            score(right)
                .total_cmp(&score(left))
                .then_with(|| left.id.cmp(&right.id))
        });
        nodes.truncate(k);
        nodes
    }

    /// GraphRag-style lexical matching: nodes whose id or path contains at
    /// least one term, scored by hit count, sorted by hits then id.
    pub fn lexical_match(&self, terms: &[String]) -> Vec<(&Node, usize)> {
        let mut out: Vec<(&Node, usize)> = self
            .graph
            .nodes
            .iter()
            .map(|node| (node, lexical_hits(node, terms)))
            .filter(|(_, hits)| *hits > 0)
            .collect();
        out.sort_by(|left, right| {
            right
                .1
                .cmp(&left.1)
                .then_with(|| left.0.id.cmp(&right.0.id))
        });
        out
    }

    /// Up to `k` node ids closest to `id` — substring hits first, then by
    /// Levenshtein distance — so tools can answer unknown ids with
    /// actionable suggestions instead of a bare "unknown id".
    pub fn nearest_matches(&self, id: &str, k: usize) -> Vec<String> {
        let needle = id.to_lowercase();
        let mut scored: Vec<(&str, bool, usize)> = self
            .graph
            .nodes
            .iter()
            .map(|node| {
                let candidate = node.id.to_lowercase();
                (
                    node.id.as_str(),
                    candidate.contains(needle.as_str()),
                    levenshtein(&needle, &candidate),
                )
            })
            .collect();
        scored.sort_by(|left, right| {
            right
                .1
                .cmp(&left.1)
                .then_with(|| left.2.cmp(&right.2))
                .then_with(|| left.0.cmp(right.0))
        });
        scored
            .into_iter()
            .take(k)
            .map(|(id, _, _)| id.to_string())
            .collect()
    }
}

/// Classic Levenshtein over chars; node ids are short, so O(n·m) is fine.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut current = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            current[j + 1] = (previous[j] + usize::from(ca != cb))
                .min(previous[j + 1] + 1)
                .min(current[j] + 1);
        }
        previous = current;
    }
    previous[b.len()]
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/graph_index_test.rs"]
mod graph_index_test;
