//! Level-1 pair evolution suggestions.
//!
//! For each directly connected pair of production components in the graph, build
//! a focused combined context — both components' public surface (map cards) plus
//! the dependency relationship between them — and ask the model for concrete ways
//! the two could evolve together: merges, shared abstractions, boundary fixes,
//! deduplication. The graph supplies the pairs deterministically; the model only
//! reasons over the supplied context.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;

use crate::evolve::clusters::{cluster_of, component_of};
use crate::evolve::map::{component_cards, render_card};
use crate::evolve::{Graph, NodeLayer};

/// An undirected level-1 pair of directly connected components.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ComponentPair {
    pub a: String,
    pub b: String,
    pub cluster_a: String,
    pub cluster_b: String,
    /// True when the two sit in different architectural clusters — usually the
    /// more interesting boundaries to reconsider.
    pub cross_cluster: bool,
    /// The edge kinds observed between them (DependsOn, SimilarTo, DuplicateOf…).
    pub relations: Vec<String>,
    /// Number of underlying node-to-node edges collapsed into this pair.
    pub weight: usize,
}

/// The graph edge kinds that count as a "connection" worth suggesting evolution
/// for. Structural containment and context-inclusion edges are excluded.
fn is_evolution_edge(kind: &str) -> bool {
    matches!(
        kind,
        "DependsOn" | "SimilarTo" | "DuplicateOf" | "Influences" | "Feedback"
    )
}

/// All directly connected (level-1) component pairs, undirected and deduped, from
/// the production graph. Same-component and non-code edges are excluded; edge
/// kinds are aggregated per pair.
pub fn connected_pairs(graph: &Graph) -> Vec<ComponentPair> {
    let code: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter(|n| n.layer == NodeLayer::Code)
        .map(|n| n.id.as_str())
        .collect();
    let mut rels: BTreeMap<(String, String), (BTreeSet<String>, usize)> = BTreeMap::new();
    for edge in &graph.edges {
        if !code.contains(edge.from.as_str()) || !code.contains(edge.to.as_str()) {
            continue;
        }
        let kind = format!("{:?}", edge.edge_type);
        if !is_evolution_edge(&kind) {
            continue;
        }
        let a = component_of(&edge.from);
        let b = component_of(&edge.to);
        if a == b {
            continue;
        }
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        let entry = rels.entry((lo, hi)).or_default();
        entry.0.insert(kind);
        entry.1 += 1;
    }
    let mut pairs: Vec<ComponentPair> = rels
        .into_iter()
        .map(|((a, b), (kinds, weight))| {
            let cluster_a = cluster_of(&a).to_string();
            let cluster_b = cluster_of(&b).to_string();
            ComponentPair {
                cross_cluster: cluster_a != cluster_b,
                cluster_a,
                cluster_b,
                relations: kinds.into_iter().collect(),
                weight,
                a,
                b,
            }
        })
        .collect();
    // Heaviest, cross-cluster connections first — the richest evolution targets.
    pairs.sort_by(|x, y| {
        y.cross_cluster
            .cmp(&x.cross_cluster)
            .then(y.weight.cmp(&x.weight))
            .then(x.a.cmp(&y.a))
            .then(x.b.cmp(&y.b))
    });
    pairs
}

/// Build the combined context for a component pair: a header naming the two and
/// their relationship, followed by each component's public surface (map cards).
pub fn pair_context(graph: &Graph, root: &Path, pair: &ComponentPair) -> String {
    let want: BTreeSet<String> = [pair.a.clone(), pair.b.clone()].into_iter().collect();
    let cards = component_cards(graph, root, &want);
    let mut out = format!(
        "# Component pair: {} <-> {}\n\
         # {} ({}), {} ({}). Relationship: {}.\n\n",
        pair.a,
        pair.b,
        pair.a,
        pair.cluster_a,
        pair.b,
        pair.cluster_b,
        if pair.relations.is_empty() {
            "connected".to_string()
        } else {
            pair.relations.join(", ")
        },
    );
    for card in &cards {
        out.push_str(&render_card(card));
        out.push('\n');
    }
    out.trim_end().to_string()
}

/// The system instruction for pair-evolution suggestions: force a compact JSON
/// list so results are machine-usable and comparable across pairs and models.
pub const SUGGEST_SYSTEM: &str = "You are a software-architecture assistant. \
    Given the public surface of two connected components, propose concrete ways \
    they could evolve together. Use only the supplied context. Return one JSON \
    object and no markdown: {\"suggestions\":[{\"title\":string,\"kind\":\
    \"merge\"|\"extract-shared\"|\"fix-boundary\"|\"dedup\"|\"decouple\",\
    \"rationale\":string,\"steps\":[string],\"effort\":\"S\"|\"M\"|\"L\",\
    \"risk\":\"low\"|\"medium\"|\"high\"}]}. Give 2-5 suggestions ordered by \
    value. If the two are unrelated, return an empty suggestions array.";

/// Build the user message for a pair: its combined context plus the task.
pub fn suggest_prompt(pair_context: &str) -> String {
    format!(
        "{pair_context}\n\n\
         Task: propose how these two components should evolve together.",
    )
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/pair_suggest_test.rs"]
mod pair_suggest_test;
