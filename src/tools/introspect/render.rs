//! Output Rendering for Code Introspection
//!
//! Formats introspection results in various output formats (tree, flat, graph)
//! with token-aware truncation.

use anyhow::Result;
use std::collections::HashMap;

use super::budget::Depth;
use super::parser::Symbol;
use super::FileInfo;

/// Directory part of a path, as grouped in the tree view.
fn dir_of(path: &str) -> String {
    std::path::Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string())
}

/// File-name part of a path.
fn file_name_of(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

/// The lines a file entry lists: one per rendered symbol, plus a marker
/// naming how many were left out for budget.
fn entry_lines(file: &FileInfo) -> Vec<String> {
    let mut items = file.rendered_lines.clone();
    if file.symbols_omitted > 0 {
        items.push(format!(
            "… {} more symbols omitted (token budget)",
            file.symbols_omitted
        ));
    }
    items
}

/// The single line a symbol renders as at `depth`: its `Kind: name` brief
/// (or `use …` import) at overview/dependencies, its visibility-prefixed
/// signature otherwise. Whitespace is collapsed so every symbol is one line.
pub fn symbol_line(symbol: &Symbol, depth: &Depth) -> String {
    let text = match depth {
        Depth::Overview | Depth::Dependencies => symbol.signature.clone(),
        Depth::Signatures | Depth::Full => {
            format!("{}{}", symbol.visibility.prefix(), symbol.signature)
        }
    };
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

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

    /// Render the included files. Every file's entry is exactly
    /// [`file_block`](Self::file_block) for that file, so a per-file token
    /// count measured on its block is a count of what this output carries.
    pub fn render(&self, files: &[FileInfo]) -> Result<String> {
        match self.format {
            OutputFormat::Tree => self.render_tree(files),
            OutputFormat::Flat => self.render_flat(files),
            OutputFormat::Graph => self.render_graph(files),
        }
    }

    /// For each file (in input order), whether it is the last entry of its
    /// directory group in the tree view — the only positional input
    /// [`file_block`](Self::file_block) needs besides the index.
    pub fn last_in_group_flags(&self, files: &[FileInfo]) -> Vec<bool> {
        let mut last_path_per_dir: HashMap<String, &str> = HashMap::new();
        for file in files {
            let dir = dir_of(&file.path);
            let entry = last_path_per_dir.entry(dir).or_insert(file.path.as_str());
            if file.path.as_str() > *entry {
                *entry = file.path.as_str();
            }
        }
        files
            .iter()
            .map(|f| last_path_per_dir.get(&dir_of(&f.path)).copied() == Some(f.path.as_str()))
            .collect()
    }

    /// The text one file contributes to the output. `index` is the file's
    /// position in the rendered list (graph node ids) and `last_in_group`
    /// selects the tree branch glyph.
    pub fn file_block(&self, file: &FileInfo, index: usize, last_in_group: bool) -> String {
        let file_name = file_name_of(&file.path);
        let mut out = String::new();
        match self.format {
            OutputFormat::Tree => {
                let branch = if last_in_group {
                    "└──"
                } else {
                    "├──"
                };
                out.push_str(&format!(
                    "    {} 📄 {} [{}]\n",
                    branch, file_name, file.depth
                ));
                let items = entry_lines(file);
                for (j, item) in items.iter().enumerate() {
                    let sym_branch = if j == items.len() - 1 {
                        "    └──"
                    } else {
                        "    ├──"
                    };
                    out.push_str(&format!("    │   {} ◆ {}\n", sym_branch, item));
                }
            }
            OutputFormat::Flat => {
                out.push_str(&format!(
                    "{} [{}] - {} tokens\n",
                    file_name, file.depth, file.tokens
                ));
                for item in entry_lines(file) {
                    out.push_str(&format!("  • {}\n", item));
                }
                out.push('\n');
            }
            OutputFormat::Graph => {
                out.push_str(&format!("    F{}[\"{}\"]\n", index, file_name));
            }
        }
        out
    }

    /// Text the output adds once per directory group (tree view only):
    /// charged when a file opens a new group.
    pub fn group_header(&self, file: &FileInfo) -> Option<String> {
        match self.format {
            OutputFormat::Tree => Some(format!("📂 {}\n\n", dir_of(&file.path))),
            OutputFormat::Flat | OutputFormat::Graph => None,
        }
    }

    /// Render as hierarchical tree
    fn render_tree(&self, files: &[FileInfo]) -> Result<String> {
        let mut output = String::new();
        output.push_str("📁 Code Introspection Results\n");
        output.push_str("═════════════════════════════\n\n");

        let flags = self.last_in_group_flags(files);

        // Group files by directory, remembering each file's index.
        let mut dir_groups: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, file) in files.iter().enumerate() {
            dir_groups.entry(dir_of(&file.path)).or_default().push(i);
        }

        let mut dirs: Vec<_> = dir_groups.keys().cloned().collect();
        dirs.sort();

        for dir in dirs {
            output.push_str(&format!("📂 {}\n", dir));
            if let Some(indices) = dir_groups.get(&dir) {
                let mut sorted = indices.clone();
                sorted.sort_by(|a, b| files[*a].path.cmp(&files[*b].path));
                for i in sorted {
                    output.push_str(&self.file_block(&files[i], i, flags[i]));
                }
            }
            output.push('\n');
        }

        // Summary
        let total_tokens: usize = files.iter().map(|f| f.tokens).sum();
        output.push_str(&format!(
            "Summary: {} files, {} tokens in file entries, {} symbols\n",
            files.len(),
            total_tokens,
            files.iter().map(|f| f.symbols.len()).sum::<usize>()
        ));

        Ok(output)
    }

    /// Render as flat list
    fn render_flat(&self, files: &[FileInfo]) -> Result<String> {
        let mut output = String::new();
        output.push_str("📄 Files (Flat View)\n");
        output.push_str("════════════════════\n\n");

        for (i, file) in files.iter().enumerate() {
            output.push_str(&self.file_block(file, i, false));
        }

        Ok(output)
    }

    /// Render as dependency graph (Mermaid format)
    fn render_graph(&self, files: &[FileInfo]) -> Result<String> {
        let mut output = String::new();
        output.push_str("```mermaid\n");
        output.push_str("graph TD\n");

        // Create nodes for files
        for (i, file) in files.iter().enumerate() {
            output.push_str(&self.file_block(file, i, false));
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
#[path = "../../../tests/unit/tools/introspect/render/render_test.rs"]
mod tests;
