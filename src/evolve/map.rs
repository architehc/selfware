//! Component map: the smallest useful context tier.
//!
//! Instead of shipping every signature (the `Lite` tier, ~272K tokens), the map
//! emits one compact card per component — its module doc line and the *names* of
//! its public symbols. The model reads the map to decide what matters, then pulls
//! detail through [`expand`]. Nothing is lost: full signatures and code stay one
//! retrieval call away, so a whole tree fits a small model's window at a fraction
//! of the resident cost.

use std::path::Path;

use serde::Serialize;

use crate::evolve::ast::AstAnalyzer;
use crate::evolve::context_reduce::strip_comments;
use crate::evolve::{Graph, NodeLayer};

/// Cap on symbol names listed per component so one huge file cannot dominate the
/// map. Overflow is reported as a count, and the detail is available via expand.
const MAX_SYMBOLS_PER_COMPONENT: usize = 48;

/// One component's entry in the map: what it is and what it exposes.
#[derive(Debug, Clone, Serialize)]
pub struct ComponentCard {
    pub component: String,
    pub path: String,
    pub doc: Option<String>,
    pub fns: Vec<String>,
    pub types: Vec<String>,
    pub traits: Vec<String>,
    pub consts: Vec<String>,
    /// Public symbols beyond [`MAX_SYMBOLS_PER_COMPONENT`], reported not dropped.
    pub more: usize,
    /// Full-code tokens of the component (what expanding it in full would cost).
    pub tokens: usize,
    pub lines: usize,
}

/// The compiled map plus its cost and the compression it buys.
#[derive(Debug, Clone, Serialize)]
pub struct ContextMap {
    pub cards: Vec<ComponentCard>,
    pub rendered: String,
    /// Measured tokens of `rendered` — the resident cost of the whole map.
    pub map_tokens: usize,
    /// Sum of every component's full-code tokens — the cost the map replaces.
    pub full_tokens: usize,
    pub components: usize,
}

/// Build the component map from the graph, reading each component's source.
pub fn build_map(graph: &Graph, root: &Path) -> ContextMap {
    let mut cards = Vec::new();
    let mut full_tokens = 0usize;
    for node in graph.nodes.iter().filter(|n| n.layer == NodeLayer::Code) {
        let Some(rel) = node.path.as_deref() else {
            continue;
        };
        let Ok(src) = std::fs::read_to_string(root.join(rel)) else {
            continue;
        };
        full_tokens += node.tokens;
        cards.push(build_card(&node.id, rel, &src, node.tokens, node.lines));
    }
    cards.sort_by(|a, b| a.component.cmp(&b.component));
    let rendered = render(&cards, full_tokens);
    let map_tokens = crate::token_count::estimate_content_tokens(&rendered);
    ContextMap {
        components: cards.len(),
        full_tokens,
        map_tokens,
        rendered,
        cards,
    }
}

/// Assemble the assistant's workspace orientation: the architectural taxonomy
/// (always), plus the full component map when `include_map` is set. This is
/// background for navigation — it is not citeable evidence, so the model can see
/// the whole tree's structure while still grounding claims in the deep evidence
/// of the selected files.
pub fn orientation(graph: &Graph, root: &Path, include_map: bool) -> String {
    let mut out = crate::evolve::clusters::taxonomy_outline(graph);
    if include_map {
        out.push_str("\n\n");
        out.push_str(&build_map(graph, root).rendered);
    }
    out
}

/// Build map cards for only the top-level components named in `want` (e.g.
/// `{"agent", "tools"}`) — used to assemble a focused context for a component
/// pair without reading the whole tree.
pub fn component_cards(
    graph: &Graph,
    root: &Path,
    want: &std::collections::BTreeSet<String>,
) -> Vec<ComponentCard> {
    let mut cards = Vec::new();
    for node in graph.nodes.iter().filter(|n| n.layer == NodeLayer::Code) {
        let component = crate::evolve::clusters::component_of(&node.id);
        if !want.contains(&component) {
            continue;
        }
        let Some(rel) = node.path.as_deref() else {
            continue;
        };
        let Ok(src) = std::fs::read_to_string(root.join(rel)) else {
            continue;
        };
        cards.push(build_card(&node.id, rel, &src, node.tokens, node.lines));
    }
    cards.sort_by(|a, b| a.component.cmp(&b.component));
    cards
}

/// Render a single card to the same text shape used in the full map body.
pub fn render_card(card: &ComponentCard) -> String {
    let mut out = format!(
        "### {}  ({} · {} tok)\n",
        card.component, card.path, card.tokens
    );
    if let Some(doc) = &card.doc {
        out.push_str(doc);
        out.push('\n');
    }
    push_symbols(&mut out, "fns", &card.fns);
    push_symbols(&mut out, "types", &card.types);
    push_symbols(&mut out, "traits", &card.traits);
    push_symbols(&mut out, "consts", &card.consts);
    if card.more > 0 {
        out.push_str(&format!("(+{} more public symbols)\n", card.more));
    }
    out
}

/// Expand one component to real detail for the model. `full` returns the whole
/// source with comments stripped; otherwise just the interface signatures.
pub fn expand(graph: &Graph, root: &Path, component: &str, full: bool) -> Option<String> {
    let node = graph
        .nodes
        .iter()
        .find(|n| n.layer == NodeLayer::Code && n.id == component)?;
    let rel = node.path.as_deref()?;
    let src = std::fs::read_to_string(root.join(rel)).ok()?;
    if full {
        return Some(strip_comments(&src));
    }
    let ast = AstAnalyzer::new().parse_source(&src).ok()?;
    Some(crate::evolve::summary::compile_summary(&ast, &src))
}

fn build_card(id: &str, path: &str, src: &str, tokens: usize, lines: usize) -> ComponentCard {
    let doc = module_doc(src);
    let (mut fns, mut types, mut traits, mut consts) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for line in src.lines() {
        let Some((kind, name)) = public_symbol(line.trim()) else {
            continue;
        };
        match kind {
            SymKind::Fn => fns.push(name),
            SymKind::Type => types.push(name),
            SymKind::Trait => traits.push(name),
            SymKind::Const => consts.push(name),
        }
    }
    // Trim to the per-component cap, keeping the counted overflow honest.
    let total = fns.len() + types.len() + traits.len() + consts.len();
    let mut budget = MAX_SYMBOLS_PER_COMPONENT;
    for bucket in [&mut fns, &mut types, &mut traits, &mut consts] {
        if bucket.len() > budget {
            bucket.truncate(budget);
        }
        budget = budget.saturating_sub(bucket.len());
    }
    let kept = fns.len() + types.len() + traits.len() + consts.len();
    ComponentCard {
        component: id.to_string(),
        path: path.to_string(),
        doc,
        fns,
        types,
        traits,
        consts,
        more: total.saturating_sub(kept),
        tokens,
        lines,
    }
}

/// First non-empty line of the leading `//!` module doc — the component's own
/// one-line description of itself, if it has one. Leading blank lines, shebangs,
/// and inner attributes (`#![...]`) are skipped; real code ends the search.
fn module_doc(src: &str) -> Option<String> {
    for line in src.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("#!") {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("//!") else {
            // Reached something that is not module doc — stop looking.
            return None;
        };
        let text = rest.trim();
        if !text.is_empty() {
            return Some(text.to_string());
        }
    }
    None
}

enum SymKind {
    Fn,
    Type,
    Trait,
    Const,
}

/// Parse a public item declaration line into (kind, name). Only `pub ` items
/// count — `pub(crate)`/private are internal and excluded from the API map.
fn public_symbol(line: &str) -> Option<(SymKind, String)> {
    let rest = line.strip_prefix("pub ")?;
    // Skip item modifiers that can precede the keyword. `const` is deliberately
    // excluded here: it is both a modifier (`const fn`) and an item keyword
    // (`const NAME`), so it is unwrapped separately just below.
    let mut rest = rest.trim_start();
    for modifier in ["async ", "unsafe ", "extern ", "default "] {
        while let Some(stripped) = rest.strip_prefix(modifier) {
            rest = stripped.trim_start();
        }
    }
    // `extern "C" fn` leaves a dangling string literal.
    if rest.starts_with('"') {
        if let Some(idx) = rest[1..].find('"') {
            rest = rest[idx + 2..].trim_start();
        }
    }
    // A `const fn` is a function; unwrap the modifier so it isn't read as a const item.
    if let Some(stripped) = rest.strip_prefix("const fn ") {
        rest = stripped;
        return {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            (!name.is_empty()).then_some((SymKind::Fn, name))
        };
    }
    let (kind, after) = if let Some(a) = rest.strip_prefix("fn ") {
        (SymKind::Fn, a)
    } else if let Some(a) = rest.strip_prefix("struct ") {
        (SymKind::Type, a)
    } else if let Some(a) = rest.strip_prefix("enum ") {
        (SymKind::Type, a)
    } else if let Some(a) = rest.strip_prefix("union ") {
        (SymKind::Type, a)
    } else if let Some(a) = rest.strip_prefix("type ") {
        (SymKind::Type, a)
    } else if let Some(a) = rest.strip_prefix("trait ") {
        (SymKind::Trait, a)
    } else if let Some(a) = rest.strip_prefix("const ") {
        (SymKind::Const, a)
    } else if let Some(a) = rest.strip_prefix("static ") {
        (SymKind::Const, a)
    } else {
        return None;
    };
    let name: String = after
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some((kind, name))
}

fn render(cards: &[ComponentCard], full_tokens: usize) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Component map — {} components, ~{} tokens of source behind this map.\n",
        cards.len(),
        full_tokens
    ));
    out.push_str(
        "# Each card lists a component and its public symbols. To read detail, call\n\
         # expand(component) for interface signatures, or expand(component, full) for code.\n\n",
    );
    for card in cards {
        out.push_str(&format!(
            "### {}  ({} · {} tok)\n",
            card.component, card.path, card.tokens
        ));
        if let Some(doc) = &card.doc {
            out.push_str(doc);
            out.push('\n');
        }
        push_symbols(&mut out, "fns", &card.fns);
        push_symbols(&mut out, "types", &card.types);
        push_symbols(&mut out, "traits", &card.traits);
        push_symbols(&mut out, "consts", &card.consts);
        if card.more > 0 {
            out.push_str(&format!("(+{} more public symbols)\n", card.more));
        }
        out.push('\n');
    }
    out.trim_end().to_string()
}

fn push_symbols(out: &mut String, label: &str, names: &[String]) {
    if names.is_empty() {
        return;
    }
    out.push_str(label);
    out.push_str(": ");
    out.push_str(&names.join(", "));
    out.push('\n');
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/map_test.rs"]
mod map_test;
