//! AST analyzer built on tree-sitter for parsing Rust source files.
//!
//! Produces a language-agnostic [`AstNode`] tree (kind, byte range, children)
//! that higher-level analyzers can walk without depending on tree-sitter types.

use std::sync::Mutex;

use anyhow::{anyhow, Result};
use tree_sitter::{Node as TsNode, Parser};

/// A language-agnostic node in a parsed syntax tree.
#[derive(Debug, Clone)]
pub struct AstNode {
    /// Tree-sitter node kind, e.g. `source_file`, `function_item`.
    pub kind: String,
    /// Byte offset where the node starts in the source.
    pub start_byte: usize,
    /// Byte offset where the node ends in the source.
    pub end_byte: usize,
    /// Child nodes, in source order.
    pub children: Vec<AstNode>,
}

/// Parses Rust files into [`AstNode`] trees using tree-sitter.
pub struct AstAnalyzer {
    // `Parser::parse` requires `&mut self`; the mutex keeps the public
    // `parse_file(&self)` interface while allowing reuse of the parser.
    parser: Mutex<Parser>,
}

impl AstAnalyzer {
    pub fn new() -> Self {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::language())
            .expect("tree-sitter-rust language is always valid");
        Self {
            parser: Mutex::new(parser),
        }
    }

    /// Read `path` and parse it into an [`AstNode`] tree rooted at the file.
    pub fn parse_file(&self, path: &str) -> Result<AstNode> {
        let content = std::fs::read_to_string(path)?;
        let mut parser = self
            .parser
            .lock()
            .map_err(|_| anyhow!("ast parser mutex poisoned"))?;
        let tree = parser
            .parse(&content, None)
            .ok_or_else(|| anyhow!("tree-sitter failed to parse {path}"))?;
        Ok(Self::convert_node(tree.root_node()))
    }

    fn convert_node(node: TsNode) -> AstNode {
        let mut cursor = node.walk();
        AstNode {
            kind: node.kind().to_string(),
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            children: node.children(&mut cursor).map(Self::convert_node).collect(),
        }
    }
}

impl Default for AstAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
