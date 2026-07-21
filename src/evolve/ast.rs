//! AST analyzer built on tree-sitter for parsing Rust source files.
//!
//! Produces a language-agnostic [`AstNode`] tree (kind, byte range, children)
//! that higher-level analyzers can walk without depending on tree-sitter types.

use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};
use syn::parse::Parser as SynParser;
use syn::punctuated::Punctuated;
use syn::{Meta, Token};
use tree_sitter::{Node as TsNode, Parser};

/// A language-agnostic node in a parsed syntax tree.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AstNode {
    /// Tree-sitter node kind, e.g. `source_file`, `function_item`.
    pub kind: String,
    /// Byte offset where the node starts in the source.
    pub start_byte: usize,
    /// Byte offset where the node ends in the source.
    pub end_byte: usize,
    /// One-based source positions for direct editor navigation.
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
    /// Identifier attached to declarations when tree-sitter exposes a `name`
    /// field. This is evidence extracted from source, not inferred.
    pub label: Option<String>,
    pub named: bool,
    pub has_error: bool,
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
        self.parse_source(&content)
            .with_context(|| format!("failed to parse {path}"))
    }

    /// Parse an in-memory editor buffer without writing it to disk first.
    pub fn parse_source(&self, content: &str) -> Result<AstNode> {
        let mut parser = self
            .parser
            .lock()
            .map_err(|_| anyhow!("ast parser mutex poisoned"))?;
        let tree = parser
            .parse(content, None)
            .ok_or_else(|| anyhow!("tree-sitter failed to parse source"))?;
        Ok(Self::convert_node(tree.root_node(), content.as_bytes()))
    }

    /// Return one-based, inclusive line ranges for items whose `cfg` predicate
    /// requires test compilation.
    ///
    /// Each range starts at the first attribute in the item's attached
    /// attribute group, so adjacent attributes and comments are kept with the
    /// item. Ranges are maximal and disjoint: once a test-only container is
    /// selected, separately attributed items inside it are not returned.
    /// A conjunction such as `cfg(all(test, feature = "slow"))` qualifies,
    /// while `cfg(any(test, feature = "slow"))` and `cfg_attr` do not because
    /// those items can exist outside a test build.
    pub fn cfg_test_ranges(&self, content: &str) -> Result<Vec<(usize, usize)>> {
        self.cfg_test_ranges_internal(content, true)
    }

    /// Test-only ranges that contain implementation bodies rather than only
    /// an external `mod name;` link to a separately stored test file.
    pub fn cfg_test_body_ranges(&self, content: &str) -> Result<Vec<(usize, usize)>> {
        self.cfg_test_ranges_internal(content, false)
    }

    fn cfg_test_ranges_internal(
        &self,
        content: &str,
        include_external_modules: bool,
    ) -> Result<Vec<(usize, usize)>> {
        let mut parser = self
            .parser
            .lock()
            .map_err(|_| anyhow!("ast parser mutex poisoned"))?;
        let tree = parser
            .parse(content, None)
            .ok_or_else(|| anyhow!("tree-sitter failed to parse source"))?;

        let mut ranges = Vec::new();
        Self::collect_cfg_test_ranges(
            tree.root_node(),
            content.as_bytes(),
            include_external_modules,
            &mut ranges,
        );
        Ok(ranges)
    }

    fn collect_cfg_test_ranges(
        parent: TsNode<'_>,
        source: &[u8],
        include_external_modules: bool,
        ranges: &mut Vec<(usize, usize)>,
    ) {
        let mut cursor = parent.walk();
        let children: Vec<_> = parent.named_children(&mut cursor).collect();
        let mut index = 0;

        while index < children.len() {
            let child = children[index];
            if child.kind() != "attribute_item" {
                if !Self::is_comment(child) {
                    Self::collect_cfg_test_ranges(child, source, include_external_modules, ranges);
                }
                index += 1;
                continue;
            }

            let first_attribute = child;
            let mut has_test_only_cfg = false;
            let mut target_index = index;

            while target_index < children.len() {
                let candidate = children[target_index];
                if candidate.kind() == "attribute_item" {
                    has_test_only_cfg |= Self::is_test_only_cfg(candidate, source);
                    target_index += 1;
                } else if Self::is_comment(candidate) {
                    target_index += 1;
                } else {
                    break;
                }
            }

            let Some(target) = children.get(target_index).copied() else {
                break;
            };

            let external_module = target.kind() == "mod_item"
                && target
                    .utf8_text(source)
                    .is_ok_and(|text| text.trim_end().ends_with(';'));
            if has_test_only_cfg
                && !matches!(target.kind(), "ERROR" | "inner_attribute_item")
                && (include_external_modules || !external_module)
            {
                let start_line = first_attribute.start_position().row + 1;
                let end = target.end_position();
                let end_line = if end.column == 0 && end.row > target.start_position().row {
                    end.row
                } else {
                    end.row + 1
                };
                ranges.push((start_line, end_line));
            } else {
                Self::collect_cfg_test_ranges(target, source, include_external_modules, ranges);
            }

            index = target_index + 1;
        }
    }

    fn is_test_only_cfg(attribute_item: TsNode<'_>, source: &[u8]) -> bool {
        let Ok(text) = attribute_item.utf8_text(source) else {
            return false;
        };
        let Ok(attributes) = syn::Attribute::parse_outer.parse_str(text) else {
            return false;
        };
        cfg_attributes_require_test(&attributes)
    }

    fn is_comment(node: TsNode<'_>) -> bool {
        matches!(node.kind(), "line_comment" | "block_comment")
    }

    fn convert_node(node: TsNode, source: &[u8]) -> AstNode {
        let mut cursor = node.walk();
        let start = node.start_position();
        let end = node.end_position();
        let label = node
            .child_by_field_name("name")
            .and_then(|name| name.utf8_text(source).ok())
            .map(str::to_string);
        AstNode {
            kind: node.kind().to_string(),
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            start_line: start.row + 1,
            start_column: start.column + 1,
            end_line: end.row + 1,
            end_column: end.column + 1,
            label,
            named: node.is_named(),
            has_error: node.has_error(),
            children: node
                .children(&mut cursor)
                .map(|child| Self::convert_node(child, source))
                .collect(),
        }
    }
}

/// True only when at least one `cfg` attribute logically requires `test`.
/// Multiple `cfg` attributes are conjunctive, so one test-only predicate is
/// sufficient. Unsupported predicates stay in production context.
pub(crate) fn cfg_attributes_require_test(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        if !attribute.path().is_ident("cfg") {
            return false;
        }
        let Meta::List(list) = &attribute.meta else {
            return false;
        };
        let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
        let Ok(predicates) = parser.parse2(list.tokens.clone()) else {
            return false;
        };
        predicates.len() == 1 && predicates.first().is_some_and(cfg_predicate_requires_test)
    })
}

fn cfg_predicate_requires_test(predicate: &Meta) -> bool {
    match predicate {
        Meta::Path(path) => path.is_ident("test"),
        Meta::List(list) if list.path.is_ident("all") || list.path.is_ident("any") => {
            let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
            let Ok(children) = parser.parse2(list.tokens.clone()) else {
                return false;
            };
            if list.path.is_ident("all") {
                children.iter().any(cfg_predicate_requires_test)
            } else {
                !children.is_empty() && children.iter().all(cfg_predicate_requires_test)
            }
        }
        Meta::List(_) | Meta::NameValue(_) => false,
    }
}

impl Default for AstAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
