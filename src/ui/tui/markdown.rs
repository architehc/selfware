//! Streaming Markdown Renderer
//!
//! Renders markdown content with syntax highlighting, collapsible sections,
//! and rich formatting for terminal display.

// Feature-gated module - dead_code lint disabled at crate level

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

use crate::safety::dry_run::truncate_str;

use super::TuiPalette;

/// Convert HeadingLevel to usize for repeat operations
fn heading_level_to_usize(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Markdown renderer with streaming support
pub struct MarkdownRenderer {
    /// Syntax highlighting set
    syntax_set: SyntaxSet,
    /// Theme for syntax highlighting
    theme: Theme,
    /// Options for markdown parsing
    options: Options,
}

impl MarkdownRenderer {
    /// Create a new markdown renderer
    pub fn new() -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();
        let theme = theme_set.themes["base16-ocean.dark"].clone();

        let mut options = Options::empty();
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_TASKLISTS);

        Self {
            syntax_set,
            theme,
            options,
        }
    }

    /// Render markdown to styled text
    pub fn render(&self, markdown: &str, width: u16) -> Text<'static> {
        let parser = Parser::new_ext(markdown, self.options);
        let mut renderer = RenderState::new(width as usize, &self.syntax_set, &self.theme);

        for event in parser {
            renderer.process_event(event);
        }

        renderer.finish()
    }

    /// Render a code block with syntax highlighting
    pub fn render_code_block(&self, lang: &str, code: &str) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Find syntax for language
        let syntax = self
            .syntax_set
            .find_syntax_by_token(lang)
            .or_else(|| self.syntax_set.find_syntax_by_extension(lang))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, &self.theme);

        // Add top border
        lines.push(Line::from(vec![
            Span::styled(
                format!("╭─ {} ", if lang.is_empty() { "code" } else { lang }),
                Style::default()
                    .fg(TuiPalette::COPPER)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("─".repeat(40), Style::default().fg(TuiPalette::STONE)),
        ]));

        // Highlight each line
        for line in code.lines() {
            let ranges = highlighter
                .highlight_line(line, &self.syntax_set)
                .unwrap_or_default();

            let spans: Vec<Span> = ranges
                .iter()
                .map(|(style, text)| {
                    let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                    Span::styled(text.to_string(), Style::default().fg(fg))
                })
                .collect();

            let mut line_spans = vec![Span::styled("│ ", Style::default().fg(TuiPalette::STONE))];
            line_spans.extend(spans);
            lines.push(Line::from(line_spans));
        }

        // Add bottom border
        lines.push(Line::from(Span::styled(
            format!("╰{}╯", "─".repeat(44)),
            Style::default().fg(TuiPalette::STONE),
        )));

        lines
    }

    /// Render a diff with side-by-side comparison
    pub fn render_diff(&self, old: &str, new: &str) -> Vec<Line<'static>> {
        use similar::{ChangeTag, TextDiff};

        let diff = TextDiff::from_lines(old, new);
        let mut lines = Vec::new();

        lines.push(Line::from(Span::styled(
            "╭─ Diff ─────────────────────────────────╮",
            Style::default().fg(TuiPalette::COPPER),
        )));

        for change in diff.iter_all_changes() {
            let (prefix, style) = match change.tag() {
                ChangeTag::Delete => (
                    "- ",
                    Style::default()
                        .fg(Color::Red)
                        .add_modifier(Modifier::CROSSED_OUT),
                ),
                ChangeTag::Insert => ("+ ", Style::default().fg(Color::Green)),
                ChangeTag::Equal => ("  ", Style::default().fg(TuiPalette::STONE)),
            };

            let text = change.to_string();
            let text = text.trim_end_matches('\n');
            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(TuiPalette::STONE)),
                Span::styled(prefix, style),
                Span::styled(text.to_string(), style),
            ]));
        }

        lines.push(Line::from(Span::styled(
            "╰─────────────────────────────────────────╯",
            Style::default().fg(TuiPalette::COPPER),
        )));

        lines
    }

    /// Render a tool call card
    pub fn render_tool_card(
        &self,
        name: &str,
        args: &str,
        result: Option<&str>,
    ) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Header
        lines.push(Line::from(vec![
            Span::styled("┌─ 🔧 ", Style::default().fg(TuiPalette::COPPER)),
            Span::styled(
                name.to_string(),
                Style::default()
                    .fg(TuiPalette::AMBER)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ", Style::default()),
            Span::styled("─".repeat(30), Style::default().fg(TuiPalette::STONE)),
        ]));

        // Arguments
        for line in args.lines().take(5) {
            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(TuiPalette::STONE)),
                Span::styled(
                    truncate_str(line, 50),
                    Style::default().fg(TuiPalette::SAGE),
                ),
            ]));
        }

        // Result if provided
        if let Some(res) = result {
            lines.push(Line::from(Span::styled(
                "├─────────────────────────────────────────",
                Style::default().fg(TuiPalette::STONE),
            )));

            let status_icon = if res.contains("error") || res.contains("failed") {
                "✗"
            } else {
                "✓"
            };
            let status_color = if res.contains("error") || res.contains("failed") {
                TuiPalette::RUST
            } else {
                TuiPalette::GARDEN_GREEN
            };

            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(TuiPalette::STONE)),
                Span::styled(
                    format!("{} ", status_icon),
                    Style::default().fg(status_color),
                ),
                Span::styled(
                    truncate_str(res, 45),
                    Style::default().fg(TuiPalette::STONE),
                ),
            ]));
        }

        // Footer
        lines.push(Line::from(Span::styled(
            "└─────────────────────────────────────────",
            Style::default().fg(TuiPalette::STONE),
        )));

        lines
    }

    /// Render a thinking block with spinner
    pub fn render_thinking(&self, content: &str, elapsed_secs: u64) -> Vec<Line<'static>> {
        let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let frame = spinner_frames[(elapsed_secs as usize) % spinner_frames.len()];

        let mut lines = Vec::new();

        lines.push(Line::from(vec![
            Span::styled(
                format!("╭─ {} Thinking ", frame),
                Style::default()
                    .fg(TuiPalette::AMBER)
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::styled(
                format!("({}s) ", elapsed_secs),
                Style::default().fg(TuiPalette::STONE),
            ),
            Span::styled("─".repeat(25), Style::default().fg(TuiPalette::STONE)),
        ]));

        // Show truncated thinking content
        let lines_count = content.lines().count();
        for (i, line) in content.lines().enumerate() {
            if i < 3 || i >= lines_count.saturating_sub(2) {
                lines.push(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(TuiPalette::STONE)),
                    Span::styled(
                        truncate_str(line, 50),
                        Style::default()
                            .fg(TuiPalette::STONE)
                            .add_modifier(Modifier::ITALIC),
                    ),
                ]));
            } else if i == 3 {
                lines.push(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(TuiPalette::STONE)),
                    Span::styled(
                        format!("... ({} more lines)", lines_count - 5),
                        Style::default()
                            .fg(TuiPalette::STONE)
                            .add_modifier(Modifier::DIM),
                    ),
                ]));
            }
        }

        lines.push(Line::from(Span::styled(
            "╰─────────────────────────────────────────",
            Style::default().fg(TuiPalette::STONE),
        )));

        lines
    }
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal state for markdown rendering
struct RenderState<'a> {
    lines: Vec<Line<'static>>,
    current_line: Vec<Span<'static>>,
    width: usize,
    in_code_block: bool,
    code_lang: String,
    code_content: String,
    list_depth: usize,
    emphasis: bool,
    strong: bool,
    syntax_set: &'a SyntaxSet,
    theme: &'a Theme,
}

impl<'a> RenderState<'a> {
    fn new(width: usize, syntax_set: &'a SyntaxSet, theme: &'a Theme) -> Self {
        Self {
            lines: Vec::new(),
            current_line: Vec::new(),
            width,
            in_code_block: false,
            code_lang: String::new(),
            code_content: String::new(),
            list_depth: 0,
            emphasis: false,
            strong: false,
            syntax_set,
            theme,
        }
    }

    fn process_event(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.start_tag(tag.clone()),
            Event::End(tag_end) => self.end_tag(tag_end),
            Event::Text(text) => self.add_text(&text),
            Event::Code(code) => self.add_inline_code(&code),
            Event::SoftBreak | Event::HardBreak => self.line_break(),
            Event::Rule => self.add_rule(),
            _ => {}
        }
    }

    fn start_tag(&mut self, tag: Tag) {
        match tag {
            Tag::Heading { level, .. } => {
                self.flush_line();
                let prefix = "#".repeat(heading_level_to_usize(level));
                self.current_line.push(Span::styled(
                    format!("{} ", prefix),
                    Style::default()
                        .fg(TuiPalette::AMBER)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            Tag::CodeBlock(kind) => {
                self.in_code_block = true;
                self.code_lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code_content.clear();
            }
            Tag::List(_) => {
                self.list_depth += 1;
            }
            Tag::Item => {
                self.flush_line();
                let indent = "  ".repeat(self.list_depth.saturating_sub(1));
                self.current_line.push(Span::styled(
                    format!("{}• ", indent),
                    Style::default().fg(TuiPalette::GARDEN_GREEN),
                ));
            }
            Tag::Emphasis => {
                self.emphasis = true;
            }
            Tag::Strong => {
                self.strong = true;
            }
            Tag::BlockQuote(_) => {
                self.flush_line();
                self.current_line
                    .push(Span::styled("▌ ", Style::default().fg(TuiPalette::SAGE)));
            }
            Tag::Link { .. } => {
                // Links will be styled differently
            }
            _ => {}
        }
    }

    fn end_tag(&mut self, tag_end: TagEnd) {
        match tag_end {
            TagEnd::Heading(_) => {
                self.flush_line();
                self.lines.push(Line::default()); // Add blank line after heading
            }
            TagEnd::CodeBlock if self.in_code_block => {
                self.render_code_block();
                self.in_code_block = false;
                self.code_content.clear();
            }
            TagEnd::List(_) => {
                self.list_depth = self.list_depth.saturating_sub(1);
                if self.list_depth == 0 {
                    self.flush_line();
                }
            }
            TagEnd::Item => {
                self.flush_line();
            }
            TagEnd::Emphasis => {
                self.emphasis = false;
            }
            TagEnd::Strong => {
                self.strong = false;
            }
            TagEnd::Paragraph => {
                self.flush_line();
                self.lines.push(Line::default()); // Blank line after paragraph
            }
            TagEnd::BlockQuote(_) => {
                self.flush_line();
            }
            _ => {}
        }
    }

    fn add_text(&mut self, text: &str) {
        if self.in_code_block {
            self.code_content.push_str(text);
            return;
        }

        let mut style = Style::default();
        if self.emphasis {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if self.strong {
            style = style.add_modifier(Modifier::BOLD);
        }

        self.current_line
            .push(Span::styled(text.to_string(), style));
    }

    fn add_inline_code(&mut self, code: &str) {
        self.current_line.push(Span::styled(
            format!("`{}`", code),
            Style::default()
                .fg(TuiPalette::COPPER)
                .add_modifier(Modifier::BOLD),
        ));
    }

    fn line_break(&mut self) {
        self.flush_line();
    }

    fn add_rule(&mut self) {
        self.flush_line();
        self.lines.push(Line::from(Span::styled(
            "─".repeat(self.width.min(50)),
            Style::default().fg(TuiPalette::STONE),
        )));
    }

    fn flush_line(&mut self) {
        if !self.current_line.is_empty() {
            self.lines
                .push(Line::from(std::mem::take(&mut self.current_line)));
        }
    }

    fn render_code_block(&mut self) {
        // Find syntax for language
        let syntax = self
            .syntax_set
            .find_syntax_by_token(&self.code_lang)
            .or_else(|| self.syntax_set.find_syntax_by_extension(&self.code_lang))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, self.theme);

        // Top border
        self.lines.push(Line::from(vec![
            Span::styled(
                format!(
                    "╭─ {} ",
                    if self.code_lang.is_empty() {
                        "code"
                    } else {
                        &self.code_lang
                    }
                ),
                Style::default()
                    .fg(TuiPalette::COPPER)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("─".repeat(35), Style::default().fg(TuiPalette::STONE)),
        ]));

        // Highlight each line
        for line in self.code_content.lines() {
            let ranges = highlighter
                .highlight_line(line, self.syntax_set)
                .unwrap_or_default();

            let spans: Vec<Span> = ranges
                .iter()
                .map(|(style, text)| {
                    let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                    Span::styled(text.to_string(), Style::default().fg(fg))
                })
                .collect();

            let mut line_spans = vec![Span::styled("│ ", Style::default().fg(TuiPalette::STONE))];
            line_spans.extend(spans);
            self.lines.push(Line::from(line_spans));
        }

        // Bottom border
        self.lines.push(Line::from(Span::styled(
            format!("╰{}╯", "─".repeat(40)),
            Style::default().fg(TuiPalette::STONE),
        )));
    }

    fn finish(mut self) -> Text<'static> {
        self.flush_line();
        Text::from(self.lines)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/ui/tui/markdown/markdown_test.rs"]
mod tests;
