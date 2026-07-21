//! Deterministic Rust source outline derived from the tree-sitter AST.
//!
//! This module only projects declarations, attributes, and comments that are
//! present in the supplied source. It does not infer behavior or call a model.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::evolve::ast::AstNode;

const MAX_OUTLINE_BYTES: usize = 256 * 1024;
const MAX_EVIDENCE_ITEMS: usize = 4096;
const MAX_NESTING_DEPTH: usize = 128;

/// A source range that directly supports one item in the structural outline.
#[derive(Debug, Clone, Serialize)]
pub struct SummaryEvidence {
    pub kind: String,
    pub label: Option<String>,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub projection: &'static str,
}

/// A bounded structural projection plus explicit completeness evidence.
#[derive(Debug, Clone, Serialize)]
pub struct StructuralSummary {
    pub outline: String,
    pub evidence: Vec<SummaryEvidence>,
    pub complete: bool,
    pub parse_has_error: bool,
    pub truncated: bool,
    pub depth_limit_reached: bool,
    pub invalid_ranges: usize,
    pub omitted_node_kinds: Vec<String>,
}

#[derive(Default)]
struct SummaryBuilder {
    output: String,
    evidence: Vec<SummaryEvidence>,
    omitted_node_kinds: BTreeSet<String>,
    truncated: bool,
    depth_limit_reached: bool,
    invalid_ranges: usize,
}

/// Compile a compact outline while retaining range-level grounding metadata.
pub fn compile_structural_summary(ast: &AstNode, source: &str) -> StructuralSummary {
    let mut builder = SummaryBuilder::default();
    walk(ast, source, &mut builder, 0);
    let omitted_node_kinds = builder.omitted_node_kinds.into_iter().collect::<Vec<_>>();
    let parse_has_error = ast.has_error;
    let complete = !parse_has_error
        && !builder.truncated
        && !builder.depth_limit_reached
        && builder.invalid_ranges == 0
        && omitted_node_kinds.is_empty();

    StructuralSummary {
        outline: builder.output.trim_end().to_string(),
        evidence: builder.evidence,
        complete,
        parse_has_error,
        truncated: builder.truncated,
        depth_limit_reached: builder.depth_limit_reached,
        invalid_ranges: builder.invalid_ranges,
        omitted_node_kinds,
    }
}

/// Compile only the outline text for callers that do not need evidence.
pub fn compile_summary(ast: &AstNode, source: &str) -> String {
    compile_structural_summary(ast, source).outline
}

fn walk(node: &AstNode, source: &str, builder: &mut SummaryBuilder, depth: usize) {
    if builder.truncated {
        return;
    }
    if depth > MAX_NESTING_DEPTH {
        builder.depth_limit_reached = true;
        builder.truncated = true;
        return;
    }

    match node.kind.as_str() {
        "source_file" | "declaration_list" => {
            for child in &node.children {
                walk(child, source, builder, depth);
            }
        }
        "impl_item" | "trait_item" | "mod_item" | "foreign_mod_item" => {
            emit_container(node, source, builder, depth);
        }
        "function_item" => emit_function(node, source, builder, depth),
        "attribute_item"
        | "inner_attribute_item"
        | "line_comment"
        | "block_comment"
        | "shebang"
        | "struct_item"
        | "union_item"
        | "enum_item"
        | "type_item"
        | "associated_type"
        | "function_signature_item"
        | "const_item"
        | "static_item"
        | "let_declaration"
        | "use_declaration"
        | "extern_crate_declaration"
        | "macro_definition"
        | "macro_invocation"
        | "expression_statement" => emit_verbatim(node, source, builder, depth),
        "empty_statement" => {}
        other if node.named => {
            builder.omitted_node_kinds.insert(other.to_string());
        }
        _ => {}
    }
}

fn emit_container(node: &AstNode, source: &str, builder: &mut SummaryBuilder, depth: usize) {
    let Some(body) = node
        .children
        .iter()
        .find(|child| child.kind == "declaration_list")
    else {
        emit_verbatim(node, source, builder, depth);
        return;
    };
    let Some(header) = source.get(node.start_byte..body.start_byte) else {
        builder.invalid_ranges += 1;
        return;
    };
    let fragment = format!("{} {{\n", indented(header.trim(), depth));
    if !emit_fragment(node, fragment, "container_header", builder) {
        return;
    }

    if depth == MAX_NESTING_DEPTH {
        builder.depth_limit_reached = true;
        builder.truncated = true;
    } else {
        walk(body, source, builder, depth + 1);
    }
    if builder.truncated {
        builder.output.push_str(&format!(
            "{}/* outline truncated */\n",
            "  ".repeat(depth + 1)
        ));
    }
    builder.output.push_str(&"  ".repeat(depth));
    builder.output.push_str("}\n\n");
}

fn emit_function(node: &AstNode, source: &str, builder: &mut SummaryBuilder, depth: usize) {
    let Some(body) = node.children.iter().find(|child| child.kind == "block") else {
        emit_verbatim(node, source, builder, depth);
        return;
    };
    let Some(header) = source.get(node.start_byte..body.start_byte) else {
        builder.invalid_ranges += 1;
        return;
    };
    let fragment = format!("{} {{ ... }}\n\n", indented(header.trim(), depth));
    emit_fragment(node, fragment, "body_elided", builder);
}

fn emit_verbatim(node: &AstNode, source: &str, builder: &mut SummaryBuilder, depth: usize) {
    let Some(text) = source.get(node.start_byte..node.end_byte) else {
        builder.invalid_ranges += 1;
        return;
    };
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let spacing = if matches!(
        node.kind.as_str(),
        "attribute_item" | "inner_attribute_item" | "line_comment" | "block_comment" | "shebang"
    ) {
        "\n"
    } else {
        "\n\n"
    };
    let fragment = format!("{}{spacing}", indented(text, depth));
    emit_fragment(node, fragment, "verbatim", builder);
}

fn emit_fragment(
    node: &AstNode,
    fragment: String,
    projection: &'static str,
    builder: &mut SummaryBuilder,
) -> bool {
    if builder.evidence.len() >= MAX_EVIDENCE_ITEMS
        || builder.output.len().saturating_add(fragment.len()) > MAX_OUTLINE_BYTES
    {
        builder.truncated = true;
        return false;
    }
    builder.output.push_str(&fragment);
    builder.evidence.push(SummaryEvidence {
        kind: node.kind.clone(),
        label: node.label.clone(),
        start_byte: node.start_byte,
        end_byte: node.end_byte,
        start_line: node.start_line,
        start_column: node.start_column,
        end_line: node.end_line,
        end_column: node.end_column,
        projection,
    });
    true
}

fn indented(text: &str, depth: usize) -> String {
    let indent = "  ".repeat(depth);
    text.lines()
        .map(|line| format!("{indent}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
