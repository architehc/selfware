//! AST scaffolding for evolution edits.
//!
//! Syntax pre-validation with source coordinates, a catalog of top-level
//! items, and structure-aware edits (derive injection, trait-impl stubs) so a
//! candidate's source is checked and edited as Rust rather than as lines of
//! text. `syn` decides whether a file is valid Rust; tree-sitter locates the
//! error and the items (byte-exact ranges, so edits keep every other byte of
//! the file).

use serde::{Deserialize, Serialize};
use std::fmt;
use tree_sitter::{Node, Parser, Tree};

/// A syntax error in candidate source. `line`/`column` are one-based and
/// present only when tree-sitter located an error node; `None` means syn
/// rejected the file but the position is unknown (never a made-up 1:1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AstSyntaxError {
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
}

impl fmt::Display for AstSyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.line, self.column) {
            (Some(line), Some(column)) => write!(
                f,
                "Rust syntax error at line {line}, column {column}: {}",
                self.message
            ),
            _ => write!(f, "Rust syntax error (position unknown): {}", self.message),
        }
    }
}

impl std::error::Error for AstSyntaxError {}

/// Kind of Rust item extracted from the AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AstItemKind {
    Struct,
    Enum,
    Union,
    Trait,
    Function,
    ImplBlock,
    TypeAlias,
    Module,
}

/// Catalog entry for a top-level item of a Rust source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AstItemSummary {
    pub name: String,
    pub kind: AstItemKind,
    /// One-based, inclusive.
    pub line_start: usize,
    pub line_end: usize,
    /// Plain `pub` only: `pub(crate)` / `pub(super)` are not public API.
    pub is_public: bool,
    pub doc_comment: Option<String>,
}

fn parse_tree(source: &str) -> Result<Tree, String> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::language())
        .map_err(|e| format!("tree-sitter-rust unavailable: {e}"))?;
    parser
        .parse(source, None)
        .ok_or_else(|| "tree-sitter failed to parse source".to_string())
}

/// Pre-validate Rust source before writing it or running the compiler.
pub fn validate_rust_syntax(source: &str) -> Result<(), AstSyntaxError> {
    let Err(syn_err) = syn::parse_file(source) else {
        return Ok(());
    };
    let located = parse_tree(source)
        .ok()
        .and_then(|tree| first_error_node(tree.root_node()).map(|n| n.start_position()));
    Err(AstSyntaxError {
        line: located.map(|p| p.row + 1),
        column: located.map(|p| p.column + 1),
        message: syn_err.to_string(),
    })
}

fn first_error_node(node: Node<'_>) -> Option<Node<'_>> {
    if node.is_error() || node.is_missing() {
        return Some(node);
    }
    if !node.has_error() {
        return None;
    }
    let mut cursor = node.walk();
    let children: Vec<Node<'_>> = node.children(&mut cursor).collect();
    children.into_iter().find_map(first_error_node)
}

fn text<'s>(node: Node<'_>, source: &'s str) -> &'s str {
    &source[node.byte_range()]
}

fn item_kind(kind: &str) -> Option<AstItemKind> {
    Some(match kind {
        "struct_item" => AstItemKind::Struct,
        "enum_item" => AstItemKind::Enum,
        "union_item" => AstItemKind::Union,
        "trait_item" => AstItemKind::Trait,
        "function_item" => AstItemKind::Function,
        "impl_item" => AstItemKind::ImplBlock,
        "type_item" => AstItemKind::TypeAlias,
        "mod_item" => AstItemKind::Module,
        _ => return None,
    })
}

/// Doc comments (`///`, `/** */`) directly above `node`, skipping attributes.
fn doc_comment_above(node: Node<'_>, source: &str) -> Option<String> {
    let mut docs = Vec::new();
    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        match p.kind() {
            "attribute_item" => {}
            "line_comment" => {
                let t = text(p, source);
                if t.starts_with("///") && !t.starts_with("////") {
                    docs.push(t.trim_start_matches("///").trim().to_string());
                } else {
                    break;
                }
            }
            "block_comment" => {
                let t = text(p, source);
                if t.starts_with("/**") && !t.starts_with("/***") {
                    docs.push(
                        t.trim_start_matches("/**")
                            .trim_end_matches("*/")
                            .trim()
                            .to_string(),
                    );
                } else {
                    break;
                }
            }
            _ => break,
        }
        prev = p.prev_sibling();
    }
    if docs.is_empty() {
        return None;
    }
    docs.reverse();
    Some(docs.join("\n"))
}

/// Catalog the top-level items of a Rust source file.
pub fn catalog_items(source: &str) -> Result<Vec<AstItemSummary>, AstSyntaxError> {
    validate_rust_syntax(source)?;
    let tree = parse_tree(source).map_err(|message| AstSyntaxError {
        line: None,
        column: None,
        message,
    })?;
    let root = tree.root_node();
    let mut items = Vec::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        let Some(kind) = item_kind(child.kind()) else {
            continue;
        };
        let name = if kind == AstItemKind::ImplBlock {
            // The header up to the body: `impl Executable for Worker`.
            let end = child
                .child_by_field_name("body")
                .map_or(child.end_byte(), |b| b.start_byte());
            source[child.start_byte()..end]
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            match child.child_by_field_name("name") {
                Some(n) => text(n, source).to_string(),
                None => continue,
            }
        };
        let mut vcur = child.walk();
        let is_public = child
            .children(&mut vcur)
            .any(|c| c.kind() == "visibility_modifier" && text(c, source) == "pub");
        items.push(AstItemSummary {
            name,
            kind,
            line_start: child.start_position().row + 1,
            line_end: child.end_position().row + 1,
            is_public,
            doc_comment: doc_comment_above(child, source),
        });
    }
    Ok(items)
}

/// Whether `s` is a Rust path usable in a derive list (`Debug`,
/// `serde::Serialize`).
fn is_derive_path(s: &str) -> bool {
    !s.is_empty()
        && s.split("::").all(|seg| {
            let mut chars = seg.chars();
            chars
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}

fn last_segment(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path).trim()
}

/// All struct/enum/union items (any nesting depth) named `type_name`.
fn find_type_items<'t>(node: Node<'t>, source: &str, type_name: &str, out: &mut Vec<Node<'t>>) {
    if matches!(node.kind(), "struct_item" | "enum_item" | "union_item")
        && node
            .child_by_field_name("name")
            .is_some_and(|n| text(n, source) == type_name)
    {
        out.push(node);
    }
    let mut cursor = node.walk();
    let children: Vec<Node<'t>> = node.named_children(&mut cursor).collect();
    for child in children {
        find_type_items(child, source, type_name, out);
    }
}

/// Add `new_derive` to the derives of the struct/enum/union `type_name`.
///
/// An existing `#[derive(…)]` gets the new path appended (a no-op when a
/// derive with the same final segment is already listed — `Serialize` is
/// not mistaken for `Deserialize`); otherwise a `#[derive(new_derive)]` is
/// inserted above the item's attributes (so derive helpers like
/// `#[serde(…)]` stay after it). Every other byte of `source` is kept. The
/// result is syntax-checked; errors: unparsable source, a non-path derive,
/// no or several items with that name.
pub fn add_or_update_derive(
    source: &str,
    type_name: &str,
    new_derive: &str,
) -> Result<String, String> {
    let new_derive = new_derive.trim();
    if !is_derive_path(new_derive) {
        return Err(format!("`{new_derive}` is not a derive path"));
    }
    validate_rust_syntax(source).map_err(|e| e.to_string())?;
    let tree = parse_tree(source)?;
    let mut found = Vec::new();
    find_type_items(tree.root_node(), source, type_name, &mut found);
    let item = match found.as_slice() {
        [] => return Err(format!("Type `{type_name}` not found in source")),
        [one] => *one,
        many => {
            return Err(format!(
                "Type `{type_name}` is defined {} times (lines {}); refusing to guess",
                many.len(),
                many.iter()
                    .map(|n| (n.start_position().row + 1).to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        }
    };

    // The attribute block above the item (comments may sit in between).
    let mut attrs = Vec::new();
    let mut prev = item.prev_sibling();
    while let Some(p) = prev {
        match p.kind() {
            "attribute_item" => attrs.push(p),
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        prev = p.prev_sibling();
    }
    attrs.reverse();

    let derives: Vec<Node<'_>> = attrs
        .iter()
        .copied()
        .filter(|a| {
            let t: String = text(*a, source).split_whitespace().collect();
            t.starts_with("#[derive(")
        })
        .collect();

    let wanted = last_segment(new_derive);
    for d in &derives {
        let t = text(*d, source);
        let (Some(open), Some(close)) = (t.find('('), t.rfind(')')) else {
            continue;
        };
        if t[open + 1..close]
            .split(',')
            .any(|p| last_segment(p.trim()) == wanted)
        {
            return Ok(source.to_string());
        }
    }

    let mut out = String::with_capacity(source.len() + new_derive.len() + 16);
    if let Some(last) = derives.last() {
        let t = text(*last, source);
        let close = t
            .rfind(')')
            .ok_or_else(|| "malformed derive attribute".to_string())?;
        let inner = &t[..close];
        let content_end = inner.trim_end().len();
        let (at, insert) = match inner[..content_end].chars().last() {
            Some('(') => (content_end, new_derive.to_string()),
            Some(',') => (content_end, format!(" {new_derive},")),
            _ => (content_end, format!(", {new_derive}")),
        };
        let abs = last.start_byte() + at;
        out.push_str(&source[..abs]);
        out.push_str(&insert);
        out.push_str(&source[abs..]);
    } else {
        let anchor = attrs.first().copied().unwrap_or(item);
        let start = anchor.start_byte();
        let line_start = source[..start].rfind('\n').map_or(0, |i| i + 1);
        let indent = &source[line_start..start];
        let indent = if indent.chars().all(char::is_whitespace) {
            indent
        } else {
            ""
        };
        out.push_str(&source[..start]);
        out.push_str(&format!("#[derive({new_derive})]\n{indent}"));
        out.push_str(&source[start..]);
    }

    validate_rust_syntax(&out)
        .map_err(|e| format!("derive injection produced invalid Rust: {e}"))?;
    Ok(out)
}

/// Scaffold a trait implementation stub (`todo!()` bodies). Each method is
/// `(name, arguments, return type)`; an empty or `()` return type is omitted.
pub fn scaffold_trait_impl(
    trait_name: &str,
    target_type: &str,
    methods: &[(&str, &str, &str)],
) -> String {
    let mut out = format!("impl {} for {} {{\n", trait_name, target_type);
    for (fn_name, sig_args, ret_type) in methods {
        let return_clause = if ret_type.is_empty() || *ret_type == "()" {
            String::new()
        } else {
            format!(" -> {}", ret_type)
        };
        out.push_str(&format!(
            "    fn {}({}){} {{\n        todo!(\"implement {}.{}\")\n    }}\n",
            fn_name, sig_args, return_clause, trait_name, fn_name
        ));
    }
    out.push_str("}\n");
    out
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/ast_scaffold_test.rs"]
mod tests;
