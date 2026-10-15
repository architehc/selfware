//! AST syntax pre-validation for evolution edits.
//!
//! Syntax pre-validation with source coordinates so a candidate's source is
//! checked as Rust rather than as lines of text. `syn` decides whether a file
//! is valid Rust; tree-sitter locates the error node.

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

#[cfg(test)]
#[path = "../../tests/unit/evolve/ast_scaffold_test.rs"]
mod tests;
