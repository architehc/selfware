//! Auto-fitting the context tier to the model's window.
//!
//! `RequestedMode` is what config and the HTTP API ask for (`auto` or a pinned
//! tier). `fit_tier` resolves `auto` to the richest concrete tier whose
//! *measured* token size fits the usable budget. Reduced tiers are measured by
//! really projecting the sources (strip comments / skeleton / component map),
//! never by fixed fractions — the fractions remain only as per-node fallbacks
//! when a file cannot be read.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::Path;

use super::context_reduce::reduce_source;
use super::map::build_map;
use super::skeleton::extract_rust_skeleton;
use super::{ContextMode, Graph, NodeLayer};

/// What the operator asked for: automatic fitting or a pinned tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestedMode {
    Auto,
    Fixed(ContextMode),
}

impl RequestedMode {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "map" => Ok(Self::Fixed(ContextMode::Map)),
            "lite" => Ok(Self::Fixed(ContextMode::Lite)),
            "compact" | "skeleton" => Ok(Self::Fixed(ContextMode::Compact)),
            "full" => Ok(Self::Fixed(ContextMode::Full)),
            "full_extended" => Ok(Self::Fixed(ContextMode::FullExtended)),
            other => Err(format!(
                "unknown context mode '{other}' \
                 (expected auto|map|lite|compact|full|full_extended)"
            )),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Fixed(mode) => mode.name(),
        }
    }
}

/// Token budget for one composed context, derived from the model window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FitBudget {
    pub context_length: usize,
    pub output_reserve: usize,
    pub fit_ratio: f64,
}

impl FitBudget {
    /// `output_reserve` follows the server's existing convention
    /// (`evolve/server.rs:117`): `max_tokens.min(context_length / 4)`.
    pub fn new(context_length: usize, max_tokens: usize, fit_ratio: f64) -> Self {
        Self {
            context_length,
            output_reserve: max_tokens.min(context_length / 4),
            fit_ratio,
        }
    }

    /// Tokens the composed context may occupy.
    pub fn usable(&self) -> usize {
        (self.context_length.saturating_sub(self.output_reserve) as f64 * self.fit_ratio) as usize
    }
}

/// The tier ladder, richest first.
pub const TIER_LADDER: [ContextMode; 5] = [
    ContextMode::FullExtended,
    ContextMode::Full,
    ContextMode::Compact,
    ContextMode::Lite,
    ContextMode::Map,
];

/// Result of fitting the tier ladder to a budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FitOutcome {
    pub mode: ContextMode,
    pub measured_tokens: usize,
    pub budget_tokens: usize,
    /// False only when even the smallest tier (Map) exceeds the budget — the
    /// caller must surface that as a warning, never silently truncate.
    pub fits: bool,
}

/// Per-node fallback fractions (measured tree-wide) used only when a node's
/// source file cannot be read.
const SIGNATURE_FRACTION: f64 = 0.18;
const COMMENT_STRIPPED_FRACTION: f64 = 0.82;

/// Measures the real token cost of each concrete tier for one graph snapshot.
///
/// Full/FullExtended come from scan-time token counts (no I/O). Compact, Lite
/// and Map read and project sources on first request and cache the result for
/// the lifetime of the measurer (one graph revision).
pub struct TierMeasurer<'a> {
    graph: &'a Graph,
    root: &'a Path,
    cache: RefCell<HashMap<&'static str, usize>>,
    io_reads: Cell<usize>,
}

impl<'a> TierMeasurer<'a> {
    pub fn new(graph: &'a Graph, root: &'a Path) -> Self {
        Self {
            graph,
            root,
            cache: RefCell::new(HashMap::new()),
            io_reads: Cell::new(0),
        }
    }

    /// Number of source files actually read (observability for tests/logs).
    pub fn io_reads(&self) -> usize {
        self.io_reads.get()
    }

    pub fn measure(&self, mode: &ContextMode) -> usize {
        match mode {
            ContextMode::FullExtended => self.measured_full_extended(),
            ContextMode::Full => self.measured_full(),
            ContextMode::Compact => self.cached("compact", Self::measure_compact),
            ContextMode::Lite => self.cached("lite", Self::measure_lite),
            ContextMode::Map => self.cached("map", Self::measure_map),
            ContextMode::Preset(_) | ContextMode::Custom => self.measured_full(),
        }
    }

    fn cached(&self, key: &'static str, f: fn(&Self) -> usize) -> usize {
        if let Some(hit) = self.cache.borrow().get(key) {
            return *hit;
        }
        let value = f(self);
        self.cache.borrow_mut().insert(key, value);
        value
    }

    fn measured_full_extended(&self) -> usize {
        self.graph
            .nodes
            .iter()
            .filter(|n| matches!(n.layer, NodeLayer::Code | NodeLayer::Test))
            .map(|n| n.tokens)
            .sum()
    }

    fn measured_full(&self) -> usize {
        self.graph
            .nodes
            .iter()
            .filter(|n| n.layer == NodeLayer::Code)
            .map(|n| n.tokens.saturating_sub(n.inline_test_tokens))
            .sum()
    }

    fn read_node_source(&self, node: &super::Node) -> Option<(String, String)> {
        let rel = node.path.as_deref()?;
        let src = std::fs::read_to_string(self.root.join(rel)).ok()?;
        self.io_reads.set(self.io_reads.get() + 1);
        Some((rel.to_string(), src))
    }

    fn measure_compact(&self) -> usize {
        let mut total = 0usize;
        for node in self
            .graph
            .nodes
            .iter()
            .filter(|n| n.layer == NodeLayer::Code)
        {
            let code_tokens = node.tokens.saturating_sub(node.inline_test_tokens);
            total += match self.read_node_source(node) {
                Some((_, src)) => crate::token_count::estimate_content_tokens(&reduce_source(&src)),
                None => (code_tokens as f64 * COMMENT_STRIPPED_FRACTION).round() as usize,
            };
        }
        total
    }

    fn measure_lite(&self) -> usize {
        let mut total = 0usize;
        for node in self
            .graph
            .nodes
            .iter()
            .filter(|n| n.layer == NodeLayer::Code)
        {
            let code_tokens = node.tokens.saturating_sub(node.inline_test_tokens);
            total += match self.read_node_source(node) {
                Some((rel, src)) if rel.ends_with(".rs") => {
                    extract_rust_skeleton(Path::new(&rel), &src).token_count
                }
                _ => (code_tokens as f64 * SIGNATURE_FRACTION).round() as usize,
            };
        }
        total
    }

    fn measure_map(&self) -> usize {
        build_map(self.graph, self.root).map_tokens
    }
}

/// Resolve `auto`: the richest tier whose measured size fits `budget.usable()`.
pub fn fit_tier(measurer: &TierMeasurer, budget: &FitBudget) -> FitOutcome {
    let usable = budget.usable();
    for mode in TIER_LADDER {
        let measured = measurer.measure(&mode);
        if measured <= usable {
            return FitOutcome {
                mode,
                measured_tokens: measured,
                budget_tokens: usable,
                fits: true,
            };
        }
    }
    FitOutcome {
        mode: ContextMode::Map,
        measured_tokens: measurer.measure(&ContextMode::Map),
        budget_tokens: usable,
        fits: false,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/context_fit_test.rs"]
mod context_fit_test;
