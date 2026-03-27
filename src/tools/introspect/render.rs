//! Output Rendering for Code Introspection
//!
//! Formats introspection results in various output formats (tree, flat, graph)
//! with token-aware truncation.

use anyhow::Result;
use std::collections::HashMap;

use super::parser::Symbol;
use super::FileInfo;

/// Output format types
#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    /// Hierarchical tree view
    Tree,
    /// Flat list view
    Flat,
    /// Graph representation
    Graph,
}

impl OutputFormat {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "tree" => Ok(Self::Tree),
            "flat" => Ok(Self::Flat),
            "graph" => Ok(Self::Graph),
            _ => anyhow::bail!("Unknown output format: {}", s),
        }
    }
}

/// Renders introspection results
pub struct OutputRenderer {
    format: OutputFormat,
}

impl OutputRenderer {
    /// Create a new renderer
    pub fn new(format: &str) -> Self {
        let format = OutputFormat::parse(format).unwrap_or(OutputFormat::Tree);
        Self { format }
    }

    /// Render files and symbols
    pub fn render(&self, files: &[FileInfo], symbols: &[Symbol]) -> Result<String> {
        match self.format {
            OutputFormat::Tree => self.render_tree(files, symbols),
            OutputFormat::Flat => self.render_flat(files, symbols),
            OutputFormat::Graph => self.render_graph(files, symbols),
        }
    }

    /// Render as hierarchical tree
    fn render_tree(&self, files: &[FileInfo], _symbols: &[Symbol]) -> Result<String> {
        let mut output = String::new();
        output.push_str("📁 Code Introspection Results\n");
        output.push_str("═════════════════════════════\n\n");

        // Group files by directory
        let mut dir_groups: HashMap<String, Vec<&FileInfo>> = HashMap::new();

        for file in files {
            let path = std::path::Path::new(&file.path);
            let dir = path
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string());

            dir_groups.entry(dir).or_default().push(file);
        }

        // Sort directories
        let mut dirs: Vec<_> = dir_groups.keys().cloned().collect();
        dirs.sort();

        for dir in dirs {
            output.push_str(&format!("📂 {}\n", dir));

            if let Some(files) = dir_groups.get(&dir) {
                let mut sorted_files = files.clone();
                sorted_files.sort_by(|a, b| a.path.cmp(&b.path));

                for (i, file) in sorted_files.iter().enumerate() {
                    let is_last = i == sorted_files.len() - 1;
                    let branch = if is_last { "└──" } else { "├──" };
                    let file_name = std::path::Path::new(&file.path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| file.path.clone());

                    output.push_str(&format!(
                        "    {} 📄 {} [{}]\n",
                        branch, file_name, file.depth
                    ));

                    // Show symbols for this file
                    if !file.symbols.is_empty() {
                        for (j, symbol) in file.symbols.iter().enumerate() {
                            let sym_last = j == file.symbols.len() - 1;
                            let sym_branch = if sym_last {
                                "    └──"
                            } else {
                                "    ├──"
                            };
                            output.push_str(&format!("    │   {} ◆ {}\n", sym_branch, symbol));
                        }
                    }
                }
            }

            output.push('\n');
        }

        // Summary
        let total_tokens: usize = files.iter().map(|f| f.tokens).sum();
        output.push_str(&format!(
            "Summary: {} files, ~{} tokens, {} symbols\n",
            files.len(),
            total_tokens,
            files.iter().map(|f| f.symbols.len()).sum::<usize>()
        ));

        Ok(output)
    }

    /// Render as flat list
    fn render_flat(&self, files: &[FileInfo], _symbols: &[Symbol]) -> Result<String> {
        let mut output = String::new();
        output.push_str("📄 Files (Flat View)\n");
        output.push_str("════════════════════\n\n");

        for file in files {
            let file_name = std::path::Path::new(&file.path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| file.path.clone());

            output.push_str(&format!(
                "{} [{}] - {} tokens\n",
                file_name, file.depth, file.tokens
            ));

            // List symbols
            for symbol in &file.symbols {
                output.push_str(&format!("  • {}\n", symbol));
            }

            output.push('\n');
        }

        Ok(output)
    }

    /// Render as dependency graph (Mermaid format)
    fn render_graph(&self, files: &[FileInfo], _symbols: &[Symbol]) -> Result<String> {
        let mut output = String::new();
        output.push_str("```mermaid\n");
        output.push_str("graph TD\n");

        // Create nodes for files
        for (i, file) in files.iter().enumerate() {
            let file_name = std::path::Path::new(&file.path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| file.path.clone());

            let node_id = format!("F{}", i);
            output.push_str(&format!("    {}[\"{}\"]\n", node_id, file_name));
        }

        // Add edges based on common directory structure
        for (i, file) in files.iter().enumerate() {
            let path = std::path::Path::new(&file.path);
            if let Some(parent) = path.parent() {
                let parent_str = parent.to_string_lossy().to_string();

                // Find parent file (mod.rs, lib.rs, etc.)
                for (j, other) in files.iter().enumerate() {
                    if i != j {
                        let other_path = std::path::Path::new(&other.path);
                        let other_parent = other_path
                            .parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default();

                        if other_parent == parent_str {
                            let other_name = other_path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default();

                            if other_name == "mod.rs"
                                || other_name == "lib.rs"
                                || other_name == "__init__.py"
                            {
                                output.push_str(&format!("    F{} --> F{}\n", j, i));
                            }
                        }
                    }
                }
            }
        }

        output.push_str("```\n");
        Ok(output)
    }

    /// Render a summary view
    pub fn render_summary(
        &self,
        files_total: usize,
        files_included: usize,
        tokens_used: usize,
        tokens_remaining: usize,
    ) -> String {
        let coverage = if files_total > 0 {
            (files_included as f64 / files_total as f64) * 100.0
        } else {
            0.0
        };

        format!(
            "📊 Introspection Summary\n\
             ───────────────────────\n\
             Files: {}/{} ({:.1}%)\n\
             Tokens: {} used, {} remaining\n",
            files_included, files_total, coverage, tokens_used, tokens_remaining
        )
    }

    /// Render suggestions
    pub fn render_suggestions(&self, suggestions: &[String]) -> String {
        if suggestions.is_empty() {
            return String::new();
        }

        let mut output = String::new();
        output.push_str("💡 Suggestions\n");
        output.push_str("─────────────\n");

        for suggestion in suggestions {
            output.push_str(&format!("  • {}\n", suggestion));
        }

        output.push('\n');
        output
    }
}

/// Render symbols in a compact format
pub fn render_symbols_compact(symbols: &[Symbol]) -> String {
    let mut output = String::new();

    for symbol in symbols {
        let vis = match symbol.visibility {
            super::parser::Visibility::Public => "pub ",
            super::parser::Visibility::PublicCrate => "pub(crate) ",
            _ => "",
        };

        let kind_emoji = match symbol.kind {
            super::parser::SymbolKind::Function | super::parser::SymbolKind::Method => "🔧",
            super::parser::SymbolKind::Struct | super::parser::SymbolKind::Class => "📦",
            super::parser::SymbolKind::Trait => "🔷",
            super::parser::SymbolKind::Enum => "🔢",
            super::parser::SymbolKind::Module => "📁",
            super::parser::SymbolKind::Const | super::parser::SymbolKind::Static => "📌",
            _ => "•",
        };

        output.push_str(&format!(
            "{} {}{}{}\n",
            kind_emoji,
            vis,
            symbol.name,
            if symbol.signature.contains('(') {
                "(...)"
            } else {
                ""
            }
        ));
    }

    output
}

/// Truncate output to fit within token budget
pub fn truncate_output(output: &str, max_tokens: usize) -> String {
    // Rough estimate: 4 chars per token
    let max_chars = max_tokens * 4;

    if output.len() <= max_chars {
        output.to_string()
    } else {
        let truncated = &output[..max_chars];
        format!(
            "{}\n\n[... truncated, {} total characters]",
            truncated,
            output.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_tree() {
        let renderer = OutputRenderer::new("tree");

        let files = vec![FileInfo {
            path: "src/main.rs".to_string(),
            depth: "signatures".to_string(),
            tokens: 500,
            symbols: vec!["main".to_string(), "helper".to_string()],
        }];

        let result = renderer.render_tree(&files, &[]).unwrap();
        assert!(result.contains("main.rs"));
        assert!(result.contains("signatures"));
    }

    #[test]
    fn test_render_flat() {
        let renderer = OutputRenderer::new("flat");

        let files = vec![FileInfo {
            path: "src/lib.rs".to_string(),
            depth: "full".to_string(),
            tokens: 1000,
            symbols: vec!["foo".to_string()],
        }];

        let result = renderer.render_flat(&files, &[]).unwrap();
        assert!(result.contains("lib.rs"));
        assert!(result.contains("1000 tokens"));
    }

    #[test]
    fn test_truncate_output() {
        let long_text = "a".repeat(10000);
        let truncated = truncate_output(&long_text, 100); // ~400 chars

        assert!(truncated.len() < long_text.len());
        assert!(truncated.contains("truncated"));
    }
}
