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
    let tree = parse_tree(source).ok();
    if let Some(ref t) = tree {
        if super::ast::AstAnalyzer::deeper_than(t.root_node(), super::ast::MAX_SYN_BODY_DEPTH * 2) {
            return Err(AstSyntaxError {
                line: Some(1),
                column: Some(1),
                message: format!(
                    "syntax tree exceeds maximum safe nesting depth ({})",
                    super::ast::MAX_SYN_BODY_DEPTH * 2
                ),
            });
        }
    }
    let Err(syn_err) = syn::parse_file(source) else {
        return Ok(());
    };
    let located = tree
        .as_ref()
        .and_then(|t| first_error_node(t.root_node()).map(|n| n.start_position()));
    Err(AstSyntaxError {
        line: located.map(|p| p.row + 1),
        column: located.map(|p| p.column + 1),
        message: syn_err.to_string(),
    })
}

fn first_error_node(root: Node<'_>) -> Option<Node<'_>> {
    if !root.has_error() {
        return None;
    }
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            return Some(node);
        }
        if node.has_error() && cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return None;
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/ast_scaffold_test.rs"]
mod tests;
