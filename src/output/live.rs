//! Live text-mode rendering of streamed model output.
//!
//! Pure building blocks, tested without a terminal:
//! - [`ProseRenderer`]: line-buffered prose → terminal lines. Renders
//!   Markdown (headings, bold/italic, inline code, lists, quotes, fences) to
//!   ANSI styling when `styled`, keeps the raw text otherwise (non-tty,
//!   `--no-color`, `NO_COLOR`), and collapses blank-line runs to at most one
//!   in both modes.
//! - [`BlankCollapser`]: the same blank-line collapse for free-flowing
//!   (verbose) reasoning text.
//! - [`reasoning_indicator`]: the one-line "Thinking… (1.2k chars)" status
//!   shown instead of the full reasoning outside `--verbose`.
//! - [`AnswerLedger`] + [`EchoGate`]: what prose the user has already SEEN in
//!   this task, so the final answer is printed exactly once — whether it was
//!   streamed live, streamed twice (planning + execution), or never streamed
//!   at all (cache hit, non-streaming call).
//!
//! Final-answer approach (text mode): prose streams live, one complete line
//! at a time, already rendered. `output::final_answer` then prints only what
//! the ledger says was NOT shown yet (all of it for an unstreamed answer, a
//! tail such as an appended truncation note, or nothing).

use std::sync::Mutex;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use super::hyperlink::Linker;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const ITALIC: &str = "\x1b[3m";
const UNDERLINE: &str = "\x1b[4m";
const STRIKE: &str = "\x1b[9m";
const CODE: &str = "\x1b[36m";

/// Wrap `text` in `codes` + reset (nothing when there is no style).
fn paint(codes: &str, text: &str) -> String {
    if codes.is_empty() || text.is_empty() {
        text.to_string()
    } else {
        format!("{codes}{text}{RESET}")
    }
}

/// Render inline Markdown (bold, italic, strikethrough, `code`, links) of one
/// line. `base` is re-applied to every segment (e.g. a heading's bold), since
/// each styled segment ends in a full reset.
fn render_inline(text: &str, base: &str, linker: Option<&Linker>) -> String {
    let mut out = String::new();
    let (mut strong, mut emph, mut strike) = (0u32, 0u32, 0u32);
    let mut links: Vec<(String, usize)> = Vec::new();
    let mut produced_any = false;
    // Adjacent text events with the same style are painted as ONE segment:
    // the parser splits text at delimiter runs (`my_file.rs` can arrive in
    // pieces), and a citation must be matched whole to be linked.
    let mut pending: Option<(String, String)> = None;
    let flush =
        |pending: &mut Option<(String, String)>, out: &mut String, links: &[(String, usize)]| {
            if let Some((codes, t)) = pending.take() {
                out.push_str(&paint(&codes, &link_text(&t, linker, links)));
            }
        };
    for event in Parser::new_ext(text, Options::ENABLE_STRIKETHROUGH) {
        if let Event::Text(t) = &event {
            let mut codes = base.to_string();
            if strong > 0 {
                codes.push_str(BOLD);
            }
            if emph > 0 {
                codes.push_str(ITALIC);
            }
            if strike > 0 {
                codes.push_str(STRIKE);
            }
            match &mut pending {
                Some((c, buf)) if *c == codes => buf.push_str(t),
                _ => {
                    flush(&mut pending, &mut out, &links);
                    pending = Some((codes, t.to_string()));
                }
            }
            produced_any = true;
            continue;
        }
        flush(&mut pending, &mut out, &links);
        match event {
            Event::Code(c) => {
                out.push_str(&paint(
                    &format!("{base}{CODE}"),
                    &link_text(&c, linker, &links),
                ));
                produced_any = true;
            }
            Event::Html(h) | Event::InlineHtml(h) => {
                out.push_str(&paint(base, &h));
                produced_any = true;
            }
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            Event::Start(Tag::Strong) => strong += 1,
            Event::End(TagEnd::Strong) => strong = strong.saturating_sub(1),
            Event::Start(Tag::Emphasis) => emph += 1,
            Event::End(TagEnd::Emphasis) => emph = emph.saturating_sub(1),
            Event::Start(Tag::Strikethrough) => strike += 1,
            Event::End(TagEnd::Strikethrough) => strike = strike.saturating_sub(1),
            Event::Start(Tag::Link { dest_url, .. }) => {
                links.push((dest_url.to_string(), out.len()));
            }
            Event::End(TagEnd::Link) => {
                if let Some((url, start)) = links.pop() {
                    // Show the target unless the link text already is it.
                    let shown = strip_ansi(&out[start..]);
                    if !url.is_empty() && shown.trim() != url {
                        out.push_str(&paint(DIM, &format!(" ({url})")));
                    }
                }
            }
            _ => {}
        }
    }
    flush(&mut pending, &mut out, &links);
    if !produced_any && !text.trim().is_empty() {
        // Swallowed by the parser (e.g. a link reference definition):
        // never drop the model's words, show them as written.
        return paint(base, text);
    }
    out
}

/// Hyperlink the citations in one text segment — never inside a Markdown
/// link (an OSC 8 link cannot nest).
fn link_text(text: &str, linker: Option<&Linker>, open_links: &[(String, usize)]) -> String {
    match linker {
        Some(l) if open_links.is_empty() => l.link(text),
        _ => text.to_string(),
    }
}

/// Remove ANSI CSI sequences and OSC sequences (hyperlinks) — for tests and
/// link-text comparison.
pub(crate) fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else if c == '\x1b' && chars.peek() == Some(&']') {
            // OSC … terminated by BEL or ST (ESC \).
            chars.next();
            while let Some(c) = chars.next() {
                if c == '\x07' {
                    break;
                }
                if c == '\x1b' && chars.peek() == Some(&'\\') {
                    chars.next();
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Whether a line opens or closes a fenced code block.
fn is_fence(trimmed: &str) -> bool {
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

/// Ordered-list marker length (`12. ` / `3) `), when the line starts with one.
fn ordered_marker_len(s: &str) -> Option<usize> {
    let digits = s.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 9 {
        return None;
    }
    let rest = &s[digits..];
    (rest.starts_with(". ") || rest.starts_with(") ")).then_some(digits + 2)
}

/// Render one non-blank Markdown line (outside or inside a fence) to ANSI.
fn render_styled_line(line: &str, in_fence: &mut bool, linker: Option<&Linker>) -> String {
    let trimmed = line.trim_start();
    if is_fence(trimmed) {
        *in_fence = !*in_fence;
        return paint(DIM, line);
    }
    if *in_fence {
        return paint(CODE, line);
    }
    let indent = &line[..line.len() - trimmed.len()];
    // Heading: `#`..`######` + space.
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
        let text = trimmed[hashes..].trim().trim_end_matches('#').trim_end();
        let base = if hashes <= 2 {
            format!("{BOLD}{UNDERLINE}")
        } else {
            BOLD.to_string()
        };
        return format!("{indent}{}", render_inline(text, &base, linker));
    }
    // Horizontal rule.
    let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() >= 3
        && (compact.chars().all(|c| c == '-')
            || compact.chars().all(|c| c == '*')
            || compact.chars().all(|c| c == '_'))
    {
        return format!("{indent}{}", paint(DIM, &"─".repeat(40)));
    }
    // Bullet list item.
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(bullet) {
            let (mark, rest) = match rest
                .strip_prefix("[ ] ")
                .map(|r| ("☐ ", r))
                .or_else(|| rest.strip_prefix("[x] ").map(|r| ("☑ ", r)))
            {
                Some((m, r)) => (m, r),
                None => ("", rest),
            };
            return format!("{indent}• {mark}{}", render_inline(rest, "", linker));
        }
    }
    // Ordered list item: keep the number, render the text.
    if let Some(n) = ordered_marker_len(trimmed) {
        return format!(
            "{indent}{}{}",
            &trimmed[..n],
            render_inline(&trimmed[n..], "", linker)
        );
    }
    // Block quote.
    if let Some(rest) = trimmed.strip_prefix('>') {
        return format!(
            "{indent}{}{}",
            paint(DIM, "│ "),
            render_inline(rest.trim_start(), DIM, linker)
        );
    }
    format!("{indent}{}", render_inline(trimmed, "", linker))
}

/// Line-buffered prose renderer for one streamed response (or one final
/// answer). Output uses `\n` line ends; the caller adapts them for a raw
/// terminal.
#[derive(Debug)]
pub(crate) struct ProseRenderer {
    styled: bool,
    partial: String,
    in_fence: bool,
    started: bool,
    blank_pending: bool,
    /// Hyperlinks workspace citations (styled output only).
    linker: Option<Linker>,
}

impl ProseRenderer {
    pub(crate) fn new(styled: bool) -> Self {
        Self {
            styled,
            partial: String::new(),
            in_fence: false,
            started: false,
            blank_pending: false,
            linker: None,
        }
    }

    /// Hyperlink workspace citations with `linker` (ignored unless styled:
    /// plain output never carries escape sequences).
    pub(crate) fn with_linker(mut self, linker: Option<Linker>) -> Self {
        self.linker = linker.filter(|_| self.styled);
        self
    }

    /// Feed streamed text; returns the rendered COMPLETE lines (each ending
    /// in `\n`). An unfinished line is held until its newline arrives.
    pub(crate) fn push(&mut self, text: &str) -> String {
        self.partial.push_str(text);
        let mut out = String::new();
        while let Some(nl) = self.partial.find('\n') {
            let line: String = self.partial.drain(..=nl).collect();
            let line = line.trim_end_matches('\n').trim_end_matches('\r');
            out.push_str(&self.line(line));
        }
        out
    }

    /// End of the text: render the held last line (with a newline).
    /// Trailing blank lines are dropped.
    pub(crate) fn finish(&mut self) -> String {
        let rest = std::mem::take(&mut self.partial);
        let out = if rest.is_empty() {
            String::new()
        } else {
            self.line(rest.trim_end_matches('\r'))
        };
        self.blank_pending = false;
        out
    }

    /// Whether anything has been rendered yet.
    #[cfg(test)]
    pub(crate) fn started(&self) -> bool {
        self.started
    }

    fn line(&mut self, line: &str) -> String {
        let blank = line.trim().is_empty();
        if blank && !self.in_fence {
            // Collapse runs, drop leading blanks: at most ONE blank line,
            // and only between two visible lines.
            if self.started {
                self.blank_pending = true;
            }
            return String::new();
        }
        let mut out = String::new();
        if std::mem::take(&mut self.blank_pending) {
            out.push('\n');
        }
        self.started = true;
        if blank {
            out.push('\n'); // blank line inside a code block is content
            return out;
        }
        if self.styled {
            out.push_str(&render_styled_line(
                line,
                &mut self.in_fence,
                self.linker.as_ref(),
            ));
        } else {
            if is_fence(line.trim_start()) {
                self.in_fence = !self.in_fence;
            }
            out.push_str(line);
        }
        out.push('\n');
        out
    }
}

/// Render a whole Markdown text at once, without hyperlinks.
#[cfg(test)]
pub(crate) fn render_prose(text: &str, styled: bool) -> String {
    render_prose_linked(text, styled, None)
}

/// Render a whole Markdown text at once (the final answer), workspace
/// citations hyperlinked by `linker`.
pub(crate) fn render_prose_linked(text: &str, styled: bool, linker: Option<Linker>) -> String {
    let mut r = ProseRenderer::new(styled).with_linker(linker);
    let mut out = r.push(text);
    out.push_str(&r.finish());
    out
}

/// Streaming blank-line collapse for free-flowing text (verbose reasoning):
/// leading newlines are dropped, a run of newlines (with only spaces/tabs
/// between) emits at most two — one blank line — and only once a visible
/// character follows, so trailing newlines never leave a gap.
#[derive(Debug, Default)]
pub(crate) struct BlankCollapser {
    started: bool,
    newlines: usize,
    pending_ws: String,
}

impl BlankCollapser {
    pub(crate) fn push(&mut self, text: &str) -> String {
        let mut out = String::new();
        for ch in text.chars() {
            match ch {
                '\n' => {
                    self.pending_ws.clear();
                    if self.started {
                        self.newlines += 1;
                    }
                }
                ' ' | '\t' | '\r' => self.pending_ws.push(ch),
                _ => {
                    for _ in 0..self.newlines.min(2) {
                        out.push('\n');
                    }
                    self.newlines = 0;
                    if self.started {
                        out.push_str(&self.pending_ws);
                    }
                    self.pending_ws.clear();
                    out.push(ch);
                    self.started = true;
                }
            }
        }
        out
    }
}

/// Compact character count: `842`, `1.2k`, `12k`.
pub(crate) fn compact_count(n: usize) -> String {
    if n < 1000 {
        n.to_string()
    } else if n < 10_000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        format!("{}k", n / 1000)
    }
}

/// The one-line reasoning status shown outside `--verbose`.
pub(crate) fn reasoning_indicator(chars: usize) -> String {
    format!("Thinking… ({} chars)", compact_count(chars))
}

/// One-line summary of a finished reasoning block (non-streaming paths and
/// non-tty logs): its first non-empty line, capped, plus its size.
pub(crate) fn reasoning_summary_line(reasoning: &str) -> String {
    let first = reasoning
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let chars = reasoning.chars().count();
    let mut head: String = first
        .chars()
        .take(80)
        .collect::<String>()
        .trim_end()
        .to_string();
    if first.chars().count() > 80 || reasoning.trim() != first {
        head.push('…');
    }
    format!("{head} ({} chars)", compact_count(chars))
}

/// Whitespace-free form used to compare shown and final text: rendering,
/// wrapping and think-block stripping move whitespace, never words.
fn squash(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Byte offset in `text` right after its non-whitespace chars matched
/// `prefix` (a squashed string), or `None` when they differ.
fn offset_after_squashed_prefix(text: &str, prefix: &str) -> Option<usize> {
    let mut want = prefix.chars();
    let mut next = want.next();
    if next.is_none() {
        return Some(0);
    }
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            continue;
        }
        if Some(c) != next {
            return None;
        }
        next = want.next();
        if next.is_none() {
            return Some(i + c.len_utf8());
        }
    }
    None
}

/// Byte offset in `text` where its non-whitespace tail equals `suffix`.
fn offset_before_squashed_suffix(text: &str, suffix: &str) -> Option<usize> {
    let mut want = suffix.chars().rev();
    let mut next = want.next();
    if next.is_none() {
        return Some(text.len());
    }
    for (i, c) in text.char_indices().rev() {
        if c.is_whitespace() {
            continue;
        }
        if Some(c) != next {
            return None;
        }
        next = want.next();
        if next.is_none() {
            return Some(i);
        }
    }
    None
}

/// Prose the user has already seen during the current task (squashed), and
/// the last tool-free response — the one a later repeat would echo.
#[derive(Debug, Default)]
pub(crate) struct AnswerLedger {
    shown: Vec<String>,
    candidate: Option<String>,
    /// The most recent `shown` block is a tool-free response (an answer),
    /// not narration before a tool call.
    last_is_answer: bool,
}

/// Bounded history: a task's final answer is one of its last few responses.
const LEDGER_KEEP: usize = 16;

impl AnswerLedger {
    pub(crate) const fn new() -> Self {
        Self {
            shown: Vec::new(),
            candidate: None,
            last_is_answer: false,
        }
    }

    /// Record prose that reached the terminal. `answer_candidate`: the
    /// response carried no tool call, so a repeat of it is an echo.
    pub(crate) fn record(&mut self, text: &str, answer_candidate: bool) {
        let s = squash(text);
        if s.is_empty() {
            return;
        }
        if answer_candidate {
            self.candidate = Some(s.clone());
        }
        if self.shown.len() >= LEDGER_KEEP {
            self.shown.remove(0);
        }
        self.shown.push(s);
        self.last_is_answer = answer_candidate;
    }

    /// The last tool-free response shown (squashed), if any.
    pub(crate) fn candidate(&self) -> Option<&str> {
        self.candidate.as_deref()
    }

    /// The part of `text` the user has NOT seen yet: `None` when all of it
    /// was shown, the tail after a shown prefix (e.g. an appended note),
    /// the head before a shown suffix, or the whole text.
    ///
    /// "Shown" is decided against whole shown BLOCKS, never substrings of
    /// them: `text` is on screen when it equals a shown block (e.g. the
    /// banked best answer of an earlier turn), or when it is the END of the
    /// most recent block and that block was a tool-free response (the final
    /// answer is the tail of the answer that just streamed, after
    /// think-stripping trimmed its lead). Prefix/suffix note detection uses
    /// only that same most recent tool-free block. The old
    /// `seen.contains(text)` swallowed a short final answer ("done", "OK",
    /// "42") that merely occurred somewhere inside earlier streamed prose,
    /// and the old prefix/suffix loop over every block truncated an
    /// unstreamed answer that started or ended with an earlier short block.
    pub(crate) fn unshown<'a>(&self, text: &'a str) -> Option<&'a str> {
        let s = squash(text);
        if s.is_empty() || self.shown.contains(&s) {
            return None;
        }
        let Some(last) = self.shown.last().filter(|_| self.last_is_answer) else {
            return Some(text);
        };
        if last.ends_with(s.as_str()) {
            return None;
        }
        if s.starts_with(last.as_str()) {
            if let Some(at) = offset_after_squashed_prefix(text, last) {
                return Some(text[at..].trim_start_matches([' ', '\t']));
            }
        }
        if s.ends_with(last.as_str()) {
            if let Some(at) = offset_before_squashed_suffix(text, last) {
                return Some(&text[..at]);
            }
        }
        Some(text)
    }
}

/// Holds back the start of a response while it repeats, word for word, the
/// last tool-free response already on screen (planning answer, then the
/// execution turn saying the same thing). The first divergence releases
/// everything held; a response that turns out to be a complete repeat is
/// never printed again.
#[derive(Debug)]
pub(crate) struct EchoGate {
    target: Option<String>,
    held: String,
    diverged: bool,
}

impl EchoGate {
    /// `target`: the squashed last shown tool-free response (see
    /// [`AnswerLedger::candidate`]); `None` disables the gate.
    pub(crate) fn new(target: Option<String>) -> Self {
        Self {
            target,
            held: String::new(),
            diverged: false,
        }
    }

    /// Feed prose; returns what may be displayed now.
    pub(crate) fn push(&mut self, text: &str) -> String {
        let Some(target) = self.target.as_deref() else {
            return text.to_string();
        };
        if self.diverged {
            return text.to_string();
        }
        self.held.push_str(text);
        if target.starts_with(squash(&self.held).as_str()) {
            return String::new();
        }
        self.diverged = true;
        std::mem::take(&mut self.held)
    }

    /// End of the response: a complete repeat is dropped; a partial one
    /// (the model stopped early) is shown.
    pub(crate) fn finish(&mut self) -> String {
        let held = std::mem::take(&mut self.held);
        match self.target.as_deref() {
            Some(target) if !self.diverged && squash(&held) == target => String::new(),
            _ => held,
        }
    }
}

static ANSWER_LEDGER: Mutex<AnswerLedger> = Mutex::new(AnswerLedger::new());

fn with_ledger<T>(f: impl FnOnce(&mut AnswerLedger) -> T) -> T {
    let mut guard = ANSWER_LEDGER.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

/// Forget what was shown: a new task starts with a clean screen history.
pub(crate) fn reset_answer_ledger() {
    with_ledger(|l| *l = AnswerLedger::new());
}

/// Record prose that reached the terminal (see [`AnswerLedger::record`]).
pub(crate) fn record_shown_prose(text: &str, answer_candidate: bool) {
    with_ledger(|l| l.record(text, answer_candidate));
}

/// The echo target for the next streamed response.
pub(crate) fn echo_target() -> Option<String> {
    with_ledger(|l| l.candidate().map(str::to_string))
}

/// The part of `text` not shown yet (see [`AnswerLedger::unshown`]).
pub(crate) fn unshown_part(text: &str) -> Option<String> {
    with_ledger(|l| l.unshown(text).map(str::to_string))
}

#[cfg(test)]
#[path = "../../tests/unit/output/live_test.rs"]
mod tests;
