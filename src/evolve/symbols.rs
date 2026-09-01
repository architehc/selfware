//! Symbol-level extraction for Rust sources (schema v2).
//!
//! Conservative by design: only top-level `pub` items (`fn`, `async fn`,
//! `struct`, `enum`, `trait`) become symbol nodes. Detection uses `syn`
//! (accurate visibility + names); line ranges come from the skeleton
//! machinery's brace-matched span finder (`extract_symbol_source`), and
//! range tokens are MEASURED with `estimate_content_tokens` (AGENTS.md
//! rule 4). False negatives are fine; false positives are not.

use super::skeleton::extract_symbol_source;

/// One extracted public symbol with its source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolDecl {
    pub name: String,
    /// `fn` | `struct` | `enum` | `trait`
    pub kind: &'static str,
    /// 1-based inclusive (start, end) lines.
    pub line_range: (usize, usize),
    /// Measured tokens of the source span.
    pub tokens: usize,
}

/// Extract the top-level `pub` symbols of a Rust source file. Unparseable
/// files and symbols whose span can't be located are skipped (a false
/// negative, never a guess).
pub fn extract_pub_symbols(content: &str) -> Vec<SymbolDecl> {
    let Ok(file) = syn::parse_file(content) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in file.items {
        let (name, kind, vis) = match &item {
            syn::Item::Fn(item_fn) => (item_fn.sig.ident.to_string(), "fn", &item_fn.vis),
            syn::Item::Struct(item_struct) => {
                (item_struct.ident.to_string(), "struct", &item_struct.vis)
            }
            syn::Item::Enum(item_enum) => (item_enum.ident.to_string(), "enum", &item_enum.vis),
            syn::Item::Trait(item_trait) => {
                (item_trait.ident.to_string(), "trait", &item_trait.vis)
            }
            _ => continue,
        };
        if !matches!(vis, syn::Visibility::Public(_)) {
            continue;
        }
        let Some(line_range) = extract_symbol_source(content, &name) else {
            continue;
        };
        let span_text: String = content
            .lines()
            .skip(line_range.0.saturating_sub(1))
            .take(line_range.1.saturating_sub(line_range.0) + 1)
            .collect::<Vec<_>>()
            .join("\n");
        out.push(SymbolDecl {
            name,
            kind,
            line_range,
            tokens: crate::token_count::estimate_content_tokens(&span_text),
        });
    }
    out
}

/// Identifier-shaped tokens of a source span (the body text used for
/// mention detection): split on anything that is not `[A-Za-z0-9_]`, drop
/// empties. Exact-set membership decides mentions — no substring guesses.
pub fn identifiers(text: &str) -> std::collections::HashSet<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/symbols_test.rs"]
mod symbols_test;
