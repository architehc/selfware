//! Syntax highlighting for fenced code blocks in text-mode answers.
//!
//! Uses `syntect`, which the TUI's Markdown renderer already depends on, so
//! text mode gains highlighting with no new dependency (cargo-deny policy,
//! binary size unchanged). The bundled grammars cover Rust, Python,
//! JavaScript, JSON, shell, YAML, Markdown, C/C++, Go, Java, … — TypeScript
//! is highlighted with the JavaScript grammar; a language with no grammar
//! (TOML, an unknown tag, no tag) keeps the plain code colour.
//!
//! Only used for styled output (TTY + colour allowed): the caller never
//! highlights plain / non-tty / `--no-color` / JSON output. Colours are
//! 24-bit when `COLORTERM` says the terminal supports it, otherwise mapped
//! to the xterm 256-colour palette. Tokens in the theme's default
//! foreground are left uncoloured so they follow the terminal's own
//! foreground (readable on light and dark backgrounds).

use std::sync::OnceLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{Color, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

const RESET: &str = "\x1b[0m";

fn syntax_set() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| {
        let mut set = ThemeSet::load_defaults();
        set.themes.remove("base16-ocean.dark").unwrap_or_default()
    })
}

/// The language tag of a fence opening line: "```rust" → "rust",
/// "~~~ py title=x" → "py", "```{.python}" → "python"; `None` without one.
pub(crate) fn fence_language(fence_line: &str) -> Option<&str> {
    let rest = fence_line
        .trim_start()
        .trim_start_matches(['`', '~'])
        .trim_start();
    let token = rest
        .split(|c: char| c.is_whitespace() || c == ',' || c == '}')
        .next()?
        .trim_start_matches('{')
        .trim_start_matches('.');
    (!token.is_empty()).then_some(token)
}

/// The grammar for a fence language tag, if one is bundled.
fn syntax_for(lang: &str) -> Option<&'static SyntaxReference> {
    let lower = lang.to_ascii_lowercase();
    let key = match lower.as_str() {
        "ts" | "typescript" | "tsx" | "jsx" | "mjs" | "cjs" | "javascript" | "node" => "js",
        "shell" | "bash" | "zsh" | "console" | "shellscript" => "sh",
        "python3" | "py3" => "py",
        "rust" => "rs",
        other => other,
    };
    let set = syntax_set();
    set.find_syntax_by_token(key)
        .or_else(|| set.find_syntax_by_extension(key))
        .filter(|s| s.name != "Plain Text")
}

/// Whether the terminal advertises 24-bit colour.
pub(crate) fn truecolor_from(colorterm: Option<&str>) -> bool {
    colorterm.is_some_and(|v| {
        let v = v.to_ascii_lowercase();
        v.contains("truecolor") || v.contains("24bit")
    })
}

/// Nearest xterm 256-colour index for an RGB colour (6×6×6 cube or the
/// grey ramp, whichever is closer).
pub(crate) fn ansi256(r: u8, g: u8, b: u8) -> u8 {
    fn cube(v: u8) -> u8 {
        if v < 48 {
            0
        } else if v < 115 {
            1
        } else {
            (v - 35) / 40
        }
    }
    const LEVELS: [i32; 6] = [0, 95, 135, 175, 215, 255];
    let (cr, cg, cb) = (cube(r), cube(g), cube(b));
    let cube_rgb = (
        LEVELS[cr as usize],
        LEVELS[cg as usize],
        LEVELS[cb as usize],
    );
    let avg = (r as i32 + g as i32 + b as i32) / 3;
    let grey_idx = if avg > 238 { 23 } else { (avg - 3).max(0) / 10 };
    let grey = 8 + grey_idx * 10;
    let dist = |(x, y, z): (i32, i32, i32)| {
        (x - r as i32).pow(2) + (y - g as i32).pow(2) + (z - b as i32).pow(2)
    };
    if dist((grey, grey, grey)) < dist(cube_rgb) {
        (232 + grey_idx) as u8
    } else {
        16 + 36 * cr + 6 * cg + cb
    }
}

fn fg_code(c: Color, truecolor: bool) -> String {
    if truecolor {
        format!("\x1b[38;2;{};{};{}m", c.r, c.g, c.b)
    } else {
        format!("\x1b[38;5;{}m", ansi256(c.r, c.g, c.b))
    }
}

/// Highlighter for one fenced block, fed line by line as the block streams.
///
/// syntect keeps per-line parse state (so a multi-line string or comment
/// keeps its colour), but that state holds oniguruma regions, which are not
/// `Send` — and the streaming renderer lives across `.await`s. So the state
/// lives on a small worker thread owned by this value: each line is sent
/// over a channel and the painted line comes back. The thread ends when the
/// block closes (this value is dropped). Rebuilding the state by replaying
/// earlier lines instead was measured at ~170 ms per line (debug build,
/// 64-line window) — far too slow for a live stream.
#[derive(Debug)]
pub(crate) struct CodeHighlighter {
    to_worker: std::sync::mpsc::Sender<(u64, String)>,
    from_worker: std::sync::mpsc::Receiver<(u64, String)>,
    /// Sequence number of the last line sent: a late answer for an earlier
    /// line (one that timed out) is discarded, never shown for this one.
    seq: u64,
}

/// How long one line may take before it is shown uncoloured instead.
#[cfg(not(test))]
const LINE_DEADLINE: std::time::Duration = std::time::Duration::from_millis(250);
/// Tests assert on colours: a loaded CI machine must not turn a slow line
/// into an uncoloured one.
#[cfg(test)]
const LINE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(20);

impl CodeHighlighter {
    /// A highlighter for the fence language `lang`; `None` when no bundled
    /// grammar matches (the caller keeps the plain code colour) or the
    /// worker thread cannot start.
    pub(crate) fn for_language(lang: &str, truecolor: bool) -> Option<Self> {
        let syntax = syntax_for(lang)?;
        let (to_worker, lines) = std::sync::mpsc::channel::<(u64, String)>();
        let (painted, from_worker) = std::sync::mpsc::channel::<(u64, String)>();
        std::thread::Builder::new()
            .name("code-highlight".into())
            .spawn(move || {
                let theme = theme();
                let default_fg = theme.settings.foreground;
                let mut hl = HighlightLines::new(syntax, theme);
                for (seq, line) in lines {
                    let with_nl = format!("{line}\n");
                    let out = match hl.highlight_line(&with_nl, syntax_set()) {
                        Ok(ranges) => paint(&ranges, default_fg, truecolor),
                        Err(_) => line,
                    };
                    if painted.send((seq, out)).is_err() {
                        break;
                    }
                }
            })
            .ok()?;
        Some(Self {
            to_worker,
            from_worker,
            seq: 0,
        })
    }

    /// [`Self::for_language`] with the colour depth of this terminal.
    pub(crate) fn for_terminal(lang: &str) -> Option<Self> {
        Self::for_language(
            lang,
            truecolor_from(std::env::var("COLORTERM").ok().as_deref()),
        )
    }

    /// Highlight the next line of the block (without its newline). The
    /// visible text is unchanged; only SGR colour codes are added. A line
    /// the worker does not answer in time is returned uncoloured.
    pub(crate) fn line(&mut self, line: &str) -> String {
        self.seq += 1;
        if self.to_worker.send((self.seq, line.to_string())).is_err() {
            return line.to_string();
        }
        let deadline = std::time::Instant::now() + LINE_DEADLINE;
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            match self.from_worker.recv_timeout(left) {
                Ok((seq, painted)) if seq == self.seq => return painted,
                Ok(_) => continue, // a late answer for an earlier line
                Err(_) => return line.to_string(),
            }
        }
    }
}

/// SGR-colour the highlighted pieces of one line (newline dropped). Pieces
/// in the theme's default foreground stay uncoloured.
fn paint(
    ranges: &[(syntect::highlighting::Style, &str)],
    default_fg: Option<Color>,
    truecolor: bool,
) -> String {
    let mut out = String::new();
    let mut coloured = false;
    for (style, text) in ranges {
        let text = text.trim_end_matches(['\n', '\r']);
        if text.is_empty() {
            continue;
        }
        if Some(style.foreground) == default_fg || text.trim().is_empty() {
            if coloured {
                out.push_str(RESET);
                coloured = false;
            }
            out.push_str(text);
        } else {
            out.push_str(&fg_code(style.foreground, truecolor));
            out.push_str(text);
            coloured = true;
        }
    }
    if coloured {
        out.push_str(RESET);
    }
    out
}

#[cfg(test)]
#[path = "../../tests/unit/output/highlight_test.rs"]
mod tests;
