//! Inline Diff Viewer for Selfware
//!
//! Terminal diff rendering with colored output, line numbers, word-level
//! highlighting, and hunk-based display. Designed to show proposed file
//! changes before applying edits, similar to how Claude Code previews diffs.

use std::fmt::Write as FmtWrite;

use anyhow::Result;
use colored::Colorize;
use similar::{ChangeTag, TextDiff};

/// Default number of context lines around each change hunk.
const DEFAULT_CONTEXT_LINES: usize = 3;

/// Maximum number of changed lines before truncation kicks in.
const MAX_CHANGED_LINES: usize = 100;

/// Statistics about a diff between two texts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffStats {
    pub additions: usize,
    pub deletions: usize,
    pub modified_lines: usize,
    pub file_path: String,
}

impl std::fmt::Display for DiffStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "+{} -{} (~{} modified)",
            self.additions, self.deletions, self.modified_lines
        )
    }
}

/// Compute diff statistics between old and new content.
pub fn compute_stats(file_path: &str, old: &str, new: &str) -> DiffStats {
    let diff = TextDiff::from_lines(old, new);
    let mut additions = 0usize;
    let mut deletions = 0usize;

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Insert => additions += 1,
            ChangeTag::Delete => deletions += 1,
            ChangeTag::Equal => {}
        }
    }

    // Modified lines = min(additions, deletions) since those are likely
    // changed lines rather than purely new/removed ones.
    let modified_lines = additions.min(deletions);

    DiffStats {
        additions,
        deletions,
        modified_lines,
        file_path: file_path.to_string(),
    }
}

/// Inline diff viewer for terminal output.
///
/// Renders colored unified and side-by-side diffs with line numbers,
/// word-level highlighting, and hunk markers.
pub struct DiffViewer {
    /// Number of context lines around each hunk.
    pub context_lines: usize,
    /// Maximum changed lines before truncation.
    pub max_changed_lines: usize,
}

impl Default for DiffViewer {
    fn default() -> Self {
        Self {
            context_lines: DEFAULT_CONTEXT_LINES,
            max_changed_lines: MAX_CHANGED_LINES,
        }
    }
}

impl DiffViewer {
    /// Create a new DiffViewer with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a DiffViewer with custom context lines.
    pub fn with_context(mut self, lines: usize) -> Self {
        self.context_lines = lines;
        self
    }

    /// Create a DiffViewer with a custom truncation threshold.
    pub fn with_max_changed(mut self, max: usize) -> Self {
        self.max_changed_lines = max;
        self
    }

    // ── Rendering Methods ──────────────────────────────────────────────

    /// Render a colored unified diff.
    pub fn show_diff(&self, file_path: &str, old_content: &str, new_content: &str) -> String {
        if old_content == new_content {
            return format!("{} {}", "No changes in".dimmed(), file_path.dimmed());
        }

        let stats = compute_stats(file_path, old_content, new_content);
        let mut out = String::new();

        // Header
        self.write_header(&mut out, file_path, &stats);

        let diff = TextDiff::from_lines(old_content, new_content);

        let total_changed = stats.additions + stats.deletions;
        if total_changed > self.max_changed_lines {
            writeln!(
                out,
                "{}",
                format!(
                    "  ... diff truncated ({} changed lines, showing summary) ...",
                    total_changed
                )
                .yellow()
            )
            .ok();
            writeln!(
                out,
                "  {} {} {}",
                format!("+{}", stats.additions).green(),
                format!("-{}", stats.deletions).red(),
                format!("(~{} modified)", stats.modified_lines).dimmed(),
            )
            .ok();
            self.write_footer(&mut out);
            return out;
        }

        // Render hunks with context
        let grouped = diff.grouped_ops(self.context_lines);
        for (group_idx, group) in grouped.iter().enumerate() {
            if group_idx > 0 {
                writeln!(out, "{}", "  ···".dimmed()).ok();
            }

            // Compute hunk range for the @@ marker
            let (old_start, old_end, new_start, new_end) = hunk_range(group);
            writeln!(
                out,
                "{}",
                format!(
                    "  @@ -{},{} +{},{} @@",
                    old_start + 1,
                    old_end - old_start,
                    new_start + 1,
                    new_end - new_start,
                )
                .cyan()
            )
            .ok();

            for op in group {
                for change in diff.iter_changes(op) {
                    let line_text = change.value().trim_end_matches('\n');
                    match change.tag() {
                        ChangeTag::Equal => {
                            let line_no = change.old_index().unwrap_or(0) + 1;
                            writeln!(
                                out,
                                "  {} {}",
                                format!("{:>4}", line_no).dimmed(),
                                line_text.dimmed(),
                            )
                            .ok();
                        }
                        ChangeTag::Delete => {
                            let line_no = change.old_index().unwrap_or(0) + 1;
                            writeln!(
                                out,
                                "  {} {} {}",
                                format!("{:>4}", line_no).red(),
                                "-".red().bold(),
                                line_text.red(),
                            )
                            .ok();
                        }
                        ChangeTag::Insert => {
                            let line_no = change.new_index().unwrap_or(0) + 1;
                            writeln!(
                                out,
                                "  {} {} {}",
                                format!("{:>4}", line_no).green(),
                                "+".green().bold(),
                                line_text.green(),
                            )
                            .ok();
                        }
                    }
                }
            }
        }

        self.write_footer(&mut out);
        out
    }

    /// Render a side-by-side inline diff with line numbers on both sides.
    pub fn show_inline_diff(
        &self,
        file_path: &str,
        old_content: &str,
        new_content: &str,
    ) -> String {
        if old_content == new_content {
            return format!("{} {}", "No changes in".dimmed(), file_path.dimmed());
        }

        let stats = compute_stats(file_path, old_content, new_content);
        let mut out = String::new();

        self.write_header(&mut out, file_path, &stats);

        let diff = TextDiff::from_lines(old_content, new_content);

        let total_changed = stats.additions + stats.deletions;
        if total_changed > self.max_changed_lines {
            writeln!(
                out,
                "{}",
                format!(
                    "  ... diff truncated ({} changed lines, showing summary) ...",
                    total_changed
                )
                .yellow()
            )
            .ok();
            self.write_footer(&mut out);
            return out;
        }

        let grouped = diff.grouped_ops(self.context_lines);
        for (group_idx, group) in grouped.iter().enumerate() {
            if group_idx > 0 {
                writeln!(out, "{}", "  ···".dimmed()).ok();
            }

            let (old_start, old_end, new_start, new_end) = hunk_range(group);
            writeln!(
                out,
                "{}",
                format!(
                    "  @@ -{},{} +{},{} @@",
                    old_start + 1,
                    old_end - old_start,
                    new_start + 1,
                    new_end - new_start,
                )
                .cyan()
            )
            .ok();

            for op in group {
                // Collect consecutive delete/insert pairs for word-level diff
                let changes: Vec<_> = diff.iter_changes(op).collect();
                let mut i = 0;
                while i < changes.len() {
                    let change = &changes[i];
                    match change.tag() {
                        ChangeTag::Equal => {
                            let old_no = change.old_index().unwrap_or(0) + 1;
                            let new_no = change.new_index().unwrap_or(0) + 1;
                            let text = change.value().trim_end_matches('\n');
                            writeln!(
                                out,
                                "  {} {} {} {}",
                                format!("{:>4}", old_no).dimmed(),
                                format!("{:>4}", new_no).dimmed(),
                                " ".dimmed(),
                                text.dimmed(),
                            )
                            .ok();
                            i += 1;
                        }
                        ChangeTag::Delete => {
                            // Look ahead for a paired insert (word-level diff)
                            if i + 1 < changes.len() && changes[i + 1].tag() == ChangeTag::Insert {
                                let old_line = change.value().trim_end_matches('\n');
                                let new_line = changes[i + 1].value().trim_end_matches('\n');
                                let old_no = change.old_index().unwrap_or(0) + 1;
                                let new_no = changes[i + 1].new_index().unwrap_or(0) + 1;

                                let (old_hl, new_hl) = word_level_highlight(old_line, new_line);

                                writeln!(
                                    out,
                                    "  {}      {} {}",
                                    format!("{:>4}", old_no).red(),
                                    "-".red().bold(),
                                    old_hl,
                                )
                                .ok();
                                writeln!(
                                    out,
                                    "        {} {} {}",
                                    format!("{:>4}", new_no).green(),
                                    "+".green().bold(),
                                    new_hl,
                                )
                                .ok();
                                i += 2;
                            } else {
                                let old_no = change.old_index().unwrap_or(0) + 1;
                                let text = change.value().trim_end_matches('\n');
                                writeln!(
                                    out,
                                    "  {}      {} {}",
                                    format!("{:>4}", old_no).red(),
                                    "-".red().bold(),
                                    text.red(),
                                )
                                .ok();
                                i += 1;
                            }
                        }
                        ChangeTag::Insert => {
                            let new_no = change.new_index().unwrap_or(0) + 1;
                            let text = change.value().trim_end_matches('\n');
                            writeln!(
                                out,
                                "        {} {} {}",
                                format!("{:>4}", new_no).green(),
                                "+".green().bold(),
                                text.green(),
                            )
                            .ok();
                            i += 1;
                        }
                    }
                }
            }
        }

        self.write_footer(&mut out);
        out
    }

    /// Render a new file creation diff (all lines green).
    pub fn show_creation(&self, file_path: &str, content: &str) -> String {
        let lines: Vec<&str> = content.lines().collect();
        let mut out = String::new();

        writeln!(
            out,
            "{}",
            format!("╭─ {} (new file, +{} lines) ─╮", file_path, lines.len())
                .green()
                .bold()
        )
        .ok();

        let display_lines = if lines.len() > self.max_changed_lines {
            writeln!(
                out,
                "{}",
                format!(
                    "  ... showing first {} of {} lines ...",
                    self.max_changed_lines,
                    lines.len()
                )
                .yellow()
            )
            .ok();
            &lines[..self.max_changed_lines]
        } else {
            &lines[..]
        };

        for (idx, line) in display_lines.iter().enumerate() {
            writeln!(
                out,
                "  {} {} {}",
                format!("{:>4}", idx + 1).green(),
                "+".green().bold(),
                line.green(),
            )
            .ok();
        }

        if lines.len() > self.max_changed_lines {
            writeln!(
                out,
                "{}",
                format!(
                    "  ... {} more lines ...",
                    lines.len() - self.max_changed_lines
                )
                .yellow()
            )
            .ok();
        }

        writeln!(out, "{}", "╰────────────────────────────╯".green().bold()).ok();
        out
    }

    /// Render a file deletion (all lines red).
    pub fn show_deletion(&self, file_path: &str) -> String {
        let mut out = String::new();
        writeln!(
            out,
            "{}",
            format!("╭─ {} (deleted) ─╮", file_path).red().bold()
        )
        .ok();
        writeln!(out, "  {}", "File will be deleted".red()).ok();
        writeln!(out, "{}", "╰────────────────────────────╯".red().bold()).ok();
        out
    }

    /// One-line compact diff summary.
    pub fn show_compact_diff(&self, file_path: &str, old: &str, new: &str) -> String {
        if old == new {
            return format!("{}: {}", file_path, "no changes".dimmed());
        }

        let stats = compute_stats(file_path, old, new);
        let change_desc = guess_change_description(old, new);

        format!(
            "{}: {} {} lines{}",
            file_path,
            format!("+{}", stats.additions).green(),
            format!("-{}", stats.deletions).red(),
            if change_desc.is_empty() {
                String::new()
            } else {
                format!(" ({})", change_desc)
            },
        )
    }

    // ── Integration Helpers ────────────────────────────────────────────

    /// Read an existing file, compute diff against new content, return rendered diff.
    pub fn diff_before_write(&self, path: &str, new_content: &str) -> Result<String> {
        let old_content = if std::path::Path::new(path).exists() {
            std::fs::read_to_string(path)?
        } else {
            return Ok(self.show_creation(path, new_content));
        };

        Ok(self.show_diff(path, &old_content, new_content))
    }

    /// Compute diff for a string replacement within a file.
    pub fn diff_before_edit(&self, path: &str, old_str: &str, new_str: &str) -> Result<String> {
        let content = std::fs::read_to_string(path)?;

        if !content.contains(old_str) {
            anyhow::bail!(
                "old_str not found in {}: {:?}",
                path,
                truncate_str(old_str, 60)
            );
        }

        let new_content = content.replacen(old_str, new_str, 1);
        Ok(self.show_diff(path, &content, &new_content))
    }

    // ── Private Helpers ────────────────────────────────────────────────

    fn write_header(&self, out: &mut String, file_path: &str, stats: &DiffStats) {
        writeln!(
            out,
            "{}",
            format!(
                "╭─ {} (+{} -{} lines) ─╮",
                file_path, stats.additions, stats.deletions
            )
            .bold()
        )
        .ok();
    }

    fn write_footer(&self, out: &mut String) {
        writeln!(out, "{}", "╰────────────────────────────╯".dimmed()).ok();
    }
}

// ── Word-Level Highlighting ────────────────────────────────────────────

/// Produce word-level highlighted strings for a delete/insert pair.
/// Changed words get inverse/underline styling for visibility.
fn word_level_highlight(old_line: &str, new_line: &str) -> (String, String) {
    let word_diff = TextDiff::from_words(old_line, new_line);

    let mut old_highlighted = String::new();
    let mut new_highlighted = String::new();

    for change in word_diff.iter_all_changes() {
        let word = change.value();
        match change.tag() {
            ChangeTag::Equal => {
                write!(old_highlighted, "{}", word.red()).ok();
                write!(new_highlighted, "{}", word.green()).ok();
            }
            ChangeTag::Delete => {
                write!(old_highlighted, "{}", word.red().underline().bold()).ok();
            }
            ChangeTag::Insert => {
                write!(new_highlighted, "{}", word.green().underline().bold()).ok();
            }
        }
    }

    (old_highlighted, new_highlighted)
}

// ── Hunk Range Calculation ─────────────────────────────────────────────

/// Compute (old_start, old_end, new_start, new_end) for a group of diff ops.
///
/// Uses `DiffOp::as_tag_tuple()` which returns `(tag, i1, i2, j1, j2)` where
/// `i1..i2` is the old range and `j1..j2` is the new range.
fn hunk_range(group: &[similar::DiffOp]) -> (usize, usize, usize, usize) {
    let first = group.first().expect("group should not be empty");
    let last = group.last().expect("group should not be empty");

    let (_, old_range_first, new_range_first) = first.as_tag_tuple();
    let (_, old_range_last, new_range_last) = last.as_tag_tuple();

    (
        old_range_first.start,
        old_range_last.end,
        new_range_first.start,
        new_range_last.end,
    )
}

/// Try to guess a brief description of what changed (e.g., "function foo modified").
fn guess_change_description(old: &str, new: &str) -> String {
    let diff = TextDiff::from_lines(old, new);

    // Look at changed lines for function/struct/impl signatures
    for change in diff.iter_all_changes() {
        if change.tag() == ChangeTag::Delete || change.tag() == ChangeTag::Insert {
            let line = change.value().trim();
            // Check for function definitions
            if let Some(name) = extract_identifier(line, "fn ") {
                return format!("function {} modified", name);
            }
            if let Some(name) = extract_identifier(line, "struct ") {
                return format!("struct {} modified", name);
            }
            if let Some(name) = extract_identifier(line, "impl ") {
                return format!("impl {} modified", name);
            }
            if let Some(name) = extract_identifier(line, "pub fn ") {
                return format!("function {} modified", name);
            }
            if let Some(name) = extract_identifier(line, "def ") {
                return format!("function {} modified", name);
            }
            if let Some(name) = extract_identifier(line, "class ") {
                return format!("class {} modified", name);
            }
        }
    }

    String::new()
}

/// Extract an identifier following a keyword prefix.
fn extract_identifier(line: &str, prefix: &str) -> Option<String> {
    let rest = line.strip_prefix(prefix).or_else(|| {
        // Also try after "pub " or similar visibility modifiers
        line.find(prefix).map(|pos| &line[pos + prefix.len()..])
    })?;

    let ident: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();

    if ident.is_empty() {
        None
    } else {
        Some(ident)
    }
}

/// Truncate a string for display in error messages.
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_ansi(s: &str) -> String {
        // Remove ANSI escape sequences for assertion comparisons
        let re = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
        re.replace_all(s, "").to_string()
    }

    #[test]
    fn test_simple_addition() {
        let viewer = DiffViewer::new();
        let old = "line1\nline2\n";
        let new = "line1\nline2\nline3\n";
        let result = viewer.show_diff("test.rs", old, new);
        let plain = strip_ansi(&result);

        assert!(plain.contains("+1"));
        assert!(plain.contains("-0"));
        assert!(plain.contains("test.rs"));
        assert!(plain.contains("line3"));
    }

    #[test]
    fn test_simple_deletion() {
        let viewer = DiffViewer::new();
        let old = "line1\nline2\nline3\n";
        let new = "line1\nline3\n";
        let result = viewer.show_diff("test.rs", old, new);
        let plain = strip_ansi(&result);

        assert!(plain.contains("-1"));
        assert!(plain.contains("line2"));
    }

    #[test]
    fn test_modification_with_context() {
        let viewer = DiffViewer::new();
        let old = "aaa\nbbb\nccc\nddd\neee\n";
        let new = "aaa\nbbb\nCCC\nddd\neee\n";
        let result = viewer.show_diff("test.rs", old, new);
        let plain = strip_ansi(&result);

        // Should have the changed line
        assert!(plain.contains("CCC"));
        assert!(plain.contains("ccc"));
        // Should have context lines
        assert!(plain.contains("bbb"));
        assert!(plain.contains("ddd"));
    }

    #[test]
    fn test_multi_hunk_diff() {
        let viewer = DiffViewer::with_context(DiffViewer::new(), 1);
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";
        let new = "a\nB\nc\nd\ne\nf\ng\nH\ni\nj\n";
        let result = viewer.show_diff("test.rs", old, new);
        let plain = strip_ansi(&result);

        // Should have two @@ markers for two separate hunks
        let hunk_count = plain.matches("@@").count();
        // Each @@ marker appears as a pair (opening), so count pairs
        assert!(
            hunk_count >= 2,
            "Expected at least 2 hunk markers, got {}",
            hunk_count
        );
    }

    #[test]
    fn test_empty_to_content() {
        let viewer = DiffViewer::new();
        let old = "";
        let new = "hello\nworld\n";
        let result = viewer.show_diff("new.rs", old, new);
        let plain = strip_ansi(&result);

        assert!(plain.contains("new.rs"));
        assert!(plain.contains("hello"));
        assert!(plain.contains("world"));
    }

    #[test]
    fn test_content_to_empty() {
        let viewer = DiffViewer::new();
        let old = "hello\nworld\n";
        let new = "";
        let result = viewer.show_diff("old.rs", old, new);
        let plain = strip_ansi(&result);

        assert!(plain.contains("old.rs"));
        assert!(plain.contains("hello"));
        assert!(plain.contains("world"));
    }

    #[test]
    fn test_no_changes() {
        let viewer = DiffViewer::new();
        let content = "same\ncontent\n";
        let result = viewer.show_diff("file.rs", content, content);
        let plain = strip_ansi(&result);

        assert!(plain.contains("No changes"));
    }

    #[test]
    fn test_very_long_diff_truncation() {
        let viewer = DiffViewer::new().with_max_changed(10);
        let old = (0..50)
            .map(|i| format!("old_line_{}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let new = (0..50)
            .map(|i| format!("new_line_{}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let result = viewer.show_diff("big.rs", &old, &new);
        let plain = strip_ansi(&result);

        assert!(plain.contains("truncated"));
    }

    #[test]
    fn test_word_level_highlighting() {
        let (old_hl, new_hl) = word_level_highlight("let x = foo(bar);", "let x = baz(bar);");

        // The highlighted strings should contain the text
        let old_plain = strip_ansi(&old_hl);
        let new_plain = strip_ansi(&new_hl);
        assert!(old_plain.contains("foo"));
        assert!(new_plain.contains("baz"));
        // Common parts should also be present
        assert!(old_plain.contains("let x = "));
        assert!(new_plain.contains("let x = "));
    }

    #[test]
    fn test_stats_computation() {
        let stats = compute_stats("test.rs", "aaa\nbbb\nccc\n", "aaa\nBBB\nccc\nddd\n");

        assert_eq!(stats.file_path, "test.rs");
        assert_eq!(stats.additions, 2); // BBB + ddd
        assert_eq!(stats.deletions, 1); // bbb
        assert_eq!(stats.modified_lines, 1); // min(2, 1)
    }

    #[test]
    fn test_show_creation() {
        let viewer = DiffViewer::new();
        let result =
            viewer.show_creation("new_file.rs", "fn main() {\n    println!(\"hello\");\n}\n");
        let plain = strip_ansi(&result);

        assert!(plain.contains("new_file.rs"));
        assert!(plain.contains("new file"));
        assert!(plain.contains("fn main()"));
        assert!(plain.contains("+3 lines"));
    }

    #[test]
    fn test_show_deletion() {
        let viewer = DiffViewer::new();
        let result = viewer.show_deletion("old_file.rs");
        let plain = strip_ansi(&result);

        assert!(plain.contains("old_file.rs"));
        assert!(plain.contains("deleted"));
    }

    #[test]
    fn test_compact_diff() {
        let viewer = DiffViewer::new();
        let old = "fn add(a: i32) -> i32 { a }\n";
        let new = "fn add(a: i32, b: i32) -> i32 { a + b }\n";
        let result = viewer.show_compact_diff("math.rs", old, new);
        let plain = strip_ansi(&result);

        assert!(plain.contains("math.rs"));
        assert!(plain.contains("+1"));
        assert!(plain.contains("-1"));
        assert!(plain.contains("function add modified"));
    }

    #[test]
    fn test_compact_diff_no_changes() {
        let viewer = DiffViewer::new();
        let result = viewer.show_compact_diff("file.rs", "same\n", "same\n");
        let plain = strip_ansi(&result);

        assert!(plain.contains("no changes"));
    }

    #[test]
    fn test_inline_diff_with_word_highlight() {
        let viewer = DiffViewer::new();
        let old = "let value = 42;\n";
        let new = "let value = 99;\n";
        let result = viewer.show_inline_diff("test.rs", old, new);
        let plain = strip_ansi(&result);

        assert!(plain.contains("42"));
        assert!(plain.contains("99"));
        assert!(plain.contains("test.rs"));
    }

    #[test]
    fn test_diff_stats_display() {
        let stats = DiffStats {
            additions: 10,
            deletions: 3,
            modified_lines: 3,
            file_path: "test.rs".to_string(),
        };

        let display = format!("{}", stats);
        assert_eq!(display, "+10 -3 (~3 modified)");
    }

    #[test]
    fn test_show_creation_truncation() {
        let viewer = DiffViewer::new().with_max_changed(5);
        let content = (0..20)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let result = viewer.show_creation("big.rs", &content);
        let plain = strip_ansi(&result);

        assert!(plain.contains("more lines"));
    }

    #[test]
    fn test_extract_identifier() {
        assert_eq!(
            extract_identifier("fn hello()", "fn "),
            Some("hello".to_string())
        );
        assert_eq!(
            extract_identifier("pub fn world()", "pub fn "),
            Some("world".to_string())
        );
        assert_eq!(
            extract_identifier("struct Foo {", "struct "),
            Some("Foo".to_string())
        );
        assert_eq!(extract_identifier("no match here", "fn "), None);
    }
}
