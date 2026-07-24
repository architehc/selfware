//! Skeleton (signature-level) extraction for Rust sources.
//!
//! Shared by the agent's L2 context level (`agent::context_map`) and the
//! evolve composer's Lite-tier measurement (`evolve::context_fit`). This is
//! intentionally fast and approximate — regex-style line scanning, not a full
//! AST parse.

use std::path::{Path, PathBuf};

use crate::token_count::estimate_content_tokens;

/// A single item extracted from a file's skeleton (L2).
#[derive(Debug, Clone)]
pub enum SkeletonItem {
    Function {
        name: String,
        signature: String,
        line: usize,
    },
    Struct {
        name: String,
        fields_summary: String,
        line: usize,
    },
    Enum {
        name: String,
        variants_summary: String,
        line: usize,
    },
    Trait {
        name: String,
        methods: Vec<String>,
        line: usize,
    },
    Impl {
        target: String,
        methods: Vec<String>,
        line: usize,
    },
    Module {
        name: String,
        line: usize,
    },
    Const {
        name: String,
        type_hint: String,
        line: usize,
    },
    Use {
        path: String,
        line: usize,
    },
}

/// Skeleton representation of a file (L2).
#[derive(Debug, Clone)]
pub struct FileSkeleton {
    pub path: PathBuf,
    pub items: Vec<SkeletonItem>,
    pub token_count: usize,
}

impl FileSkeleton {
    /// Render the skeleton as a compact string for context injection.
    pub fn render(&self) -> String {
        let mut out = format!("// {}\n", self.path.display());
        for item in &self.items {
            match item {
                SkeletonItem::Function {
                    name: _,
                    signature,
                    line,
                } => {
                    out.push_str(&format!("L{}: {}\n", line, signature));
                }
                SkeletonItem::Struct {
                    name,
                    fields_summary,
                    line,
                } => {
                    out.push_str(&format!(
                        "L{}: struct {} {{ {} }}\n",
                        line, name, fields_summary
                    ));
                }
                SkeletonItem::Enum {
                    name,
                    variants_summary,
                    line,
                } => {
                    out.push_str(&format!(
                        "L{}: enum {} {{ {} }}\n",
                        line, name, variants_summary
                    ));
                }
                SkeletonItem::Trait {
                    name,
                    methods,
                    line,
                } => {
                    out.push_str(&format!(
                        "L{}: trait {} {{ {} }}\n",
                        line,
                        name,
                        methods.join("; ")
                    ));
                }
                SkeletonItem::Impl {
                    target,
                    methods,
                    line,
                } => {
                    out.push_str(&format!(
                        "L{}: impl {} {{ {} }}\n",
                        line,
                        target,
                        methods.join("; ")
                    ));
                }
                SkeletonItem::Module { name, line } => {
                    out.push_str(&format!("L{}: mod {}\n", line, name));
                }
                SkeletonItem::Const {
                    name,
                    type_hint,
                    line,
                } => {
                    out.push_str(&format!("L{}: const {}: {}\n", line, name, type_hint));
                }
                SkeletonItem::Use { path, line } => {
                    // `path` already includes the `use`/`pub use` keyword (see
                    // extract_rust_skeleton), so print it verbatim. Prepending
                    // another `use ` here produced `use use ...`, which models
                    // mistook for a syntax error and chased as a phantom bug.
                    out.push_str(&format!("L{}: {}\n", line, path));
                }
            }
        }
        out
    }
}

// ─── Skeleton Extraction ────────────────────────────────────────────────────

/// Extract a skeleton (L2) from Rust source code using regex-based parsing.
/// This is intentionally fast and approximate — not a full AST parse.
pub fn extract_rust_skeleton(path: &Path, content: &str) -> FileSkeleton {
    let mut items = Vec::new();

    for (line_num_0, line) in content.lines().enumerate() {
        let line_num = line_num_0 + 1;
        let trimmed = line.trim();

        // Skip comments and empty lines.
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") {
            continue;
        }

        // `use` statements.
        if trimmed.starts_with("use ") || trimmed.starts_with("pub use ") {
            items.push(SkeletonItem::Use {
                path: trimmed.trim_end_matches(';').to_string(),
                line: line_num,
            });
            continue;
        }

        // `mod` declarations.
        if (trimmed.starts_with("mod ") || trimmed.starts_with("pub mod "))
            && (trimmed.ends_with(';') || trimmed.ends_with('{'))
        {
            let name = extract_name_after(trimmed, "mod ");
            items.push(SkeletonItem::Module {
                name,
                line: line_num,
            });
            continue;
        }

        // `const` / `static`.
        if trimmed.starts_with("const ")
            || trimmed.starts_with("pub const ")
            || trimmed.starts_with("pub(super) const ")
            || trimmed.starts_with("pub(crate) const ")
            || trimmed.starts_with("static ")
            || trimmed.starts_with("pub static ")
        {
            let (name, type_hint) = extract_const_parts(trimmed);
            items.push(SkeletonItem::Const {
                name,
                type_hint,
                line: line_num,
            });
            continue;
        }

        // `fn` declarations — capture the full signature line.
        if is_fn_line(trimmed) {
            let name = extract_fn_name(trimmed);
            items.push(SkeletonItem::Function {
                name,
                signature: trimmed.trim_end_matches('{').trim().to_string(),
                line: line_num,
            });
            continue;
        }

        // `struct` declarations.
        if is_struct_line(trimmed) {
            let name = extract_name_after(trimmed, "struct ");
            // Try to capture field names on subsequent lines (simplified).
            items.push(SkeletonItem::Struct {
                name,
                fields_summary: "...".to_string(),
                line: line_num,
            });
            continue;
        }

        // `enum` declarations.
        if is_enum_line(trimmed) {
            let name = extract_name_after(trimmed, "enum ");
            items.push(SkeletonItem::Enum {
                name,
                variants_summary: "...".to_string(),
                line: line_num,
            });
            continue;
        }

        // `trait` declarations.
        if is_trait_line(trimmed) {
            let name = extract_name_after(trimmed, "trait ");
            items.push(SkeletonItem::Trait {
                name,
                methods: vec![],
                line: line_num,
            });
            continue;
        }

        // `impl` blocks.
        if is_impl_line(trimmed) {
            let target = extract_impl_target(trimmed);
            items.push(SkeletonItem::Impl {
                target,
                methods: vec![],
                line: line_num,
            });
            continue;
        }
    }

    let rendered = {
        let skel = FileSkeleton {
            path: path.to_path_buf(),
            items: items.clone(),
            token_count: 0,
        };
        skel.render()
    };
    let token_count = estimate_content_tokens(&rendered);

    FileSkeleton {
        path: path.to_path_buf(),
        items,
        token_count,
    }
}

// ─── Skeleton helpers ───────────────────────────────────────────────────────

fn is_fn_line(line: &str) -> bool {
    let prefixes = [
        "fn ",
        "pub fn ",
        "pub(crate) fn ",
        "pub(super) fn ",
        "async fn ",
        "pub async fn ",
        "pub(crate) async fn ",
        "pub(super) async fn ",
        "unsafe fn ",
        "pub unsafe fn ",
        "const fn ",
        "pub const fn ",
    ];
    prefixes.iter().any(|p| line.starts_with(p))
}

fn extract_fn_name(line: &str) -> String {
    // Find "fn " and extract the name before '(' or '<'.
    if let Some(fn_idx) = line.find("fn ") {
        let after_fn = &line[fn_idx + 3..];
        let end = after_fn
            .find(|c: char| ['(', '<', ' '].contains(&c))
            .unwrap_or(after_fn.len());
        after_fn[..end].to_string()
    } else {
        "?".to_string()
    }
}

fn is_struct_line(line: &str) -> bool {
    (line.starts_with("struct ")
        || line.starts_with("pub struct ")
        || line.starts_with("pub(crate) struct ")
        || line.starts_with("pub(super) struct "))
        && !line.contains("impl")
}

fn is_enum_line(line: &str) -> bool {
    line.starts_with("enum ")
        || line.starts_with("pub enum ")
        || line.starts_with("pub(crate) enum ")
        || line.starts_with("pub(super) enum ")
}

fn is_trait_line(line: &str) -> bool {
    line.starts_with("trait ")
        || line.starts_with("pub trait ")
        || line.starts_with("pub(crate) trait ")
        || line.starts_with("pub(super) trait ")
}

fn is_impl_line(line: &str) -> bool {
    line.starts_with("impl ") || line.starts_with("impl<")
}

fn extract_name_after(line: &str, keyword: &str) -> String {
    if let Some(idx) = line.find(keyword) {
        let after = &line[idx + keyword.len()..];
        let end = after
            .find(|c: char| ['<', '{', '(', ';', ' '].contains(&c))
            .unwrap_or(after.len());
        after[..end].trim().to_string()
    } else {
        "?".to_string()
    }
}

fn extract_const_parts(line: &str) -> (String, String) {
    // "pub const FOO: usize = 42;" → ("FOO", "usize")
    let after_const = if let Some(idx) = line.find("const ") {
        &line[idx + 6..]
    } else if let Some(idx) = line.find("static ") {
        &line[idx + 7..]
    } else {
        return ("?".into(), "?".into());
    };

    let parts: Vec<&str> = after_const.splitn(2, ':').collect();
    let name = parts
        .first()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let type_hint = parts
        .get(1)
        .map(|s| {
            s.split('=')
                .next()
                .unwrap_or("")
                .trim()
                .trim_end_matches(';')
                .to_string()
        })
        .unwrap_or_default();

    (name, type_hint)
}

fn extract_impl_target(line: &str) -> String {
    // "impl<T> Agent for Foo {" → "Agent for Foo"
    // "impl Agent {" → "Agent"
    let after_impl = if let Some(rest) = line.strip_prefix("impl<") {
        // Skip generic params.
        if let Some(gt_pos) = rest.find('>') {
            &rest[gt_pos + 1..]
        } else {
            rest
        }
    } else if let Some(rest) = line.strip_prefix("impl ") {
        rest
    } else {
        &line[5..]
    };

    after_impl
        .trim_end_matches('{')
        .trim_end_matches("where")
        .trim()
        .to_string()
}
