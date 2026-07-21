//! ContextComposer: manages which graph components are in the active context.
//!
//! Explicit loading modes:
//! - `Lite`: nothing loaded (default).
//! - `Full`: production code nodes only.
//! - `FullExtended`: production code plus test/example nodes.
//! - `Preset(name)`: a single named preset loaded.

use super::{Graph, Node, NodeLayer};

/// Rough tokens-per-line factor used when a node has no measured token count.
const TOKENS_PER_LINE: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextMode {
    Lite,
    Full,
    FullExtended,
    Preset(String),
}

impl ContextMode {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Lite => "lite",
            Self::Full => "full",
            Self::FullExtended => "full_extended",
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

    /// The node ids a given mode would include, without changing active state.
    fn included_for(&self, mode: &ContextMode) -> Vec<String> {
        match mode {
            ContextMode::Lite => Vec::new(),
            ContextMode::Full => self
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
        }
    }

    /// Node count and estimated token size of each selectable mode, computed
    /// without mutating the active selection — used to show the cost of each
    /// option in the context picker.
    pub fn mode_sizes(&self) -> Vec<ContextModeSize> {
        [ContextMode::Lite, ContextMode::Full, ContextMode::FullExtended]
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

fn estimate_context_node_tokens(node: &Node, mode: &ContextMode) -> usize {
    let total = estimate_node_tokens(node.tokens, node.lines);
    if node.layer == NodeLayer::Code && matches!(mode, ContextMode::Full) {
        total.saturating_sub(node.inline_test_tokens)
    } else {
        total
    }
}
