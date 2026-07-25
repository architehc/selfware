//! ContextComposer: manages which graph components are in the active context.
//!
//! Explicit loading tiers (smallest → largest), all over production code nodes:
//! - `Map`: a component index — doc line + public symbol names per component,
//!   with detail pulled on demand via `evolve::map::expand` (retrieval, not resident).
//! - `Lite`: interface signatures only (~18% of full) — the smallest whole-code tier.
//! - `Compact`: full code with comments stripped (~82%) — for smaller models.
//! - `Full`: full production code.
//! - `FullExtended`: production code plus test/example nodes.
//! - `Custom`: a hand-picked component selection, loaded at Lite/skeleton detail.
//! - `Preset(name)`: a single named preset loaded.

use super::{Graph, Node, NodeLayer};

/// Rough tokens-per-line factor used when a node has no measured token count.
const TOKENS_PER_LINE: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextMode {
    Map,
    Lite,
    Compact,
    Full,
    FullExtended,
    Custom,
    Preset(String),
}

impl ContextMode {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Map => "map",
            Self::Lite => "lite",
            Self::Compact => "compact",
            Self::Full => "full",
            Self::FullExtended => "full_extended",
            Self::Custom => "custom",
            Self::Preset(_) => "preset",
        }
    }
}

/// Node count and token size of one selectable context mode (for the picker).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContextModeSize {
    pub mode: String,
    pub nodes: usize,
    pub tokens: usize,
}

/// Aggregate cost of one node layer in the active context selection.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContextLayerSummary {
    pub layer: NodeLayer,
    pub nodes: usize,
    pub tokens: usize,
    pub files: usize,
}

/// Aggregate cost for one source category in the active context.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContextSourceSummary {
    pub nodes: usize,
    pub tokens: usize,
    pub files: usize,
}

/// Serializable snapshot of the composer's current selection and cost.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContextSummary {
    pub mode: ContextMode,
    pub included: Vec<String>,
    pub estimated_tokens: usize,
    pub production: ContextSourceSummary,
    pub tests: ContextSourceSummary,
    pub examples: ContextSourceSummary,
    pub file_partition_complete: bool,
    pub production_files_with_inline_tests: usize,
    pub inline_test_ranges: usize,
    pub inline_test_lines: usize,
    pub layers: Vec<ContextLayerSummary>,
}

pub struct ContextComposer {
    graph: Graph,
    mode: ContextMode,
    included: Vec<String>,
}

impl ContextComposer {
    pub fn new(graph: Graph) -> Self {
        Self {
            graph,
            mode: ContextMode::Lite,
            included: Vec::new(),
        }
    }

    pub fn set_mode(&mut self, mode: ContextMode) {
        self.included = self.included_for(&mode);
        self.mode = mode;
    }

    /// Apply a hand-picked component selection as the active context.
    /// Unknown ids are dropped; the mode becomes `Custom` (even when empty).
    pub fn set_custom(&mut self, ids: Vec<String>) {
        let known: std::collections::HashSet<&str> =
            self.graph.nodes.iter().map(|n| n.id.as_str()).collect();
        self.included = ids
            .into_iter()
            .filter(|id| known.contains(id.as_str()))
            .collect();
        self.mode = ContextMode::Custom;
    }

    /// The node ids a given mode would include, without changing active state.
    fn included_for(&self, mode: &ContextMode) -> Vec<String> {
        match mode {
            ContextMode::Map | ContextMode::Lite | ContextMode::Compact | ContextMode::Full => self
                .graph
                .nodes
                .iter()
                .filter(|node| node.layer == NodeLayer::Code)
                .map(|n| n.id.clone())
                .collect(),
            ContextMode::FullExtended => self
                .graph
                .nodes
                .iter()
                .filter(|node| matches!(node.layer, NodeLayer::Code | NodeLayer::Test))
                .map(|node| node.id.clone())
                .collect(),
            ContextMode::Preset(name) => vec![name.clone()],
            // Custom: the composer's current hand-picked list is the source of
            // truth — there is no rule to recompute it from.
            ContextMode::Custom => self.included.clone(),
        }
    }

    /// Node count and estimated token size of each selectable mode, computed
    /// without mutating the active selection — used to show the cost of each
    /// option in the context picker.
    pub fn mode_sizes(&self) -> Vec<ContextModeSize> {
        [
            ContextMode::Lite,
            ContextMode::Compact,
            ContextMode::Full,
            ContextMode::FullExtended,
        ]
        .into_iter()
        .map(|mode| {
            let included = self.included_for(&mode);
            let tokens = self
                .graph
                .nodes
                .iter()
                .filter(|node| included.contains(&node.id))
                .map(|node| estimate_context_node_tokens(node, &mode))
                .sum();
            ContextModeSize {
                mode: mode.name().to_string(),
                nodes: included.len(),
                tokens,
            }
        })
        .collect()
    }

    pub fn mode_name(&self) -> &'static str {
        self.mode.name()
    }

    pub fn mode(&self) -> &ContextMode {
        &self.mode
    }

    /// Estimated token cost of the currently included nodes.
    ///
    /// Uses the measured `tokens` count when available; otherwise falls back
    /// to a `lines`-based heuristic (with a minimum of 1 per included node),
    /// so included content never estimates to zero.
    pub fn estimate_tokens(&self) -> usize {
        self.graph
            .nodes
            .iter()
            .filter(|n| self.included.contains(&n.id))
            .map(|node| estimate_context_node_tokens(node, &self.mode))
            .sum()
    }

    pub fn included_nodes(&self) -> Vec<String> {
        self.included.clone()
    }

    /// Token and file totals for each layer represented in the active context.
    pub fn layer_summaries(&self) -> Vec<ContextLayerSummary> {
        [
            NodeLayer::Code,
            NodeLayer::Test,
            NodeLayer::Concept,
            NodeLayer::Preset,
        ]
        .into_iter()
        .filter_map(|layer| {
            let nodes: Vec<_> = self
                .graph
                .nodes
                .iter()
                .filter(|node| node.layer == layer && self.included.contains(&node.id))
                .collect();
            (!nodes.is_empty()).then(|| ContextLayerSummary {
                layer,
                nodes: nodes.len(),
                tokens: nodes
                    .iter()
                    .map(|node| estimate_context_node_tokens(node, &self.mode))
                    .sum(),
                files: nodes.iter().map(|node| node.files).sum(),
            })
        })
        .collect()
    }

    pub fn summary(&self) -> ContextSummary {
        let mut production = ContextSourceSummary::default();
        let mut tests = ContextSourceSummary::default();
        let mut examples = ContextSourceSummary::default();
        let mut production_files_with_inline_tests = 0usize;
        let mut inline_test_ranges = 0usize;
        let mut inline_test_lines = 0usize;

        for node in self
            .graph
            .nodes
            .iter()
            .filter(|node| self.included.contains(&node.id))
        {
            let bucket = if node.layer == NodeLayer::Code {
                &mut production
            } else if node.layer == NodeLayer::Test && node.id.starts_with("example::") {
                &mut examples
            } else if node.layer == NodeLayer::Test {
                &mut tests
            } else {
                continue;
            };
            bucket.nodes += 1;
            bucket.tokens += estimate_context_node_tokens(node, &self.mode);
            bucket.files += node.files;
            if node.layer == NodeLayer::Code && node.inline_test_ranges > 0 {
                production_files_with_inline_tests += 1;
                inline_test_ranges += node.inline_test_ranges;
                inline_test_lines += node.inline_test_lines;
            }
        }

        ContextSummary {
            mode: self.mode.clone(),
            included: self.included.clone(),
            estimated_tokens: self.estimate_tokens(),
            production,
            tests,
            examples,
            file_partition_complete: production_files_with_inline_tests == 0,
            production_files_with_inline_tests,
            inline_test_ranges,
            inline_test_lines,
            layers: self.layer_summaries(),
        }
    }
}

fn estimate_node_tokens(tokens: usize, lines: usize) -> usize {
    if tokens > 0 {
        tokens
    } else {
        (lines * TOKENS_PER_LINE).max(1)
    }
}

/// Fraction of a file's implementation tokens that its public-signature skeleton
/// occupies — measured at ~18% across the tree (pub fn/struct/trait lines + doc).
const SIGNATURE_FRACTION: f64 = 0.18;
/// Fraction remaining after stripping comments — measured at ~82%.
const COMMENT_STRIPPED_FRACTION: f64 = 0.82;

fn estimate_context_node_tokens(node: &Node, mode: &ContextMode) -> usize {
    let total = estimate_node_tokens(node.tokens, node.lines);
    let code_tokens = total.saturating_sub(node.inline_test_tokens);
    match mode {
        // Map: a compiled component-index artifact, not a per-node projection.
        // Its real resident cost is measured by `evolve::map` and reported by the
        // server; per node it contributes nothing to the composer estimate.
        ContextMode::Map => 0,
        // Lite: interface signatures only — the smallest useful tier.
        // Custom selections also load at skeleton detail, so they cost the same.
        ContextMode::Lite | ContextMode::Custom => {
            ((code_tokens as f64) * SIGNATURE_FRACTION).round() as usize
        }
        // Compact: full code with comments stripped, for smaller models.
        ContextMode::Compact => ((code_tokens as f64) * COMMENT_STRIPPED_FRACTION).round() as usize,
        ContextMode::Full => code_tokens,
        _ => total,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/context_custom_test.rs"]
mod context_custom_test;
