//! OSC 8 terminal hyperlinks for workspace citations.
//!
//! A rendered answer or run summary that cites `src/lib.rs:42` (or
//! `src/lib.rs:10-20`, `src/lib.rs#L42`) gets the citation wrapped in an
//! OSC 8 hyperlink to the file, so a terminal that supports them makes it
//! clickable. Terminals without OSC 8 support ignore the sequence and show
//! the text unchanged.
//!
//! Rules:
//! - only on a styled terminal (stdout is a TTY, colour allowed): never in
//!   plain / non-tty / `--no-color` / `NO_COLOR` / JSON output, never while
//!   the TUI owns the screen;
//! - `SELFWARE_HYPERLINKS=0` turns links off, `=1` turns them on even on a
//!   terminal we would otherwise skip (`TERM=dumb`/`linux`) — it never adds
//!   links to plain or JSON output;
//! - only paths that exist inside the workspace (the current directory,
//!   which the CLI sets to the workspace) are linked;
//! - the target is a plain `file://` URL of the absolute path: the line
//!   number stays in the visible text only, because no `#L` fragment
//!   convention for `file://` URLs is honoured widely across terminals and
//!   the file handlers they launch.

use std::path::{Path, PathBuf};

/// Environment override: `0`/`false`/`off` disables, `1`/`true`/`on` forces
/// links on a styled terminal.
pub(crate) const HYPERLINKS_ENV: &str = "SELFWARE_HYPERLINKS";

/// Whether citations are hyperlinked, from the facts that decide it.
///
/// `styled`: prose is rendered with terminal styling (stdout is a TTY and
/// colour is allowed — see [`super::markdown_styled`]). `json`: machine
/// output. `env_override`: the [`HYPERLINKS_ENV`] value. `term`: `$TERM`.
pub(crate) fn hyperlinks_wanted(
    styled: bool,
    json: bool,
    env_override: Option<&str>,
    term: Option<&str>,
) -> bool {
    if !styled || json {
        return false;
    }
    match env_override.map(|v| v.trim().to_ascii_lowercase()) {
        Some(v) if matches!(v.as_str(), "0" | "false" | "off" | "no") => return false,
        Some(v) if matches!(v.as_str(), "1" | "true" | "on" | "yes") => return true,
        _ => {}
    }
    // The Linux console and dumb terminals have no OSC 8 support.
    !matches!(term, Some("dumb") | Some("linux"))
}

/// Whether hyperlinks are on for this process's stdout right now.
pub(crate) fn enabled() -> bool {
    !super::is_tui_active()
        && hyperlinks_wanted(
            super::markdown_styled(),
            super::is_json_mode(),
            std::env::var(HYPERLINKS_ENV).ok().as_deref(),
            std::env::var("TERM").ok().as_deref(),
        )
}

/// `file://` URL for an absolute path, or `None` when it cannot be encoded
/// safely inside an OSC 8 sequence.
pub(crate) fn file_url(path: &Path) -> Option<String> {
    let url = url::Url::from_file_path(path).ok()?;
    // `;` separates OSC 8 fields in some parsers; control characters would
    // end the sequence early. `Url` percent-encodes controls already; this
    // is the belt to that brace.
    let s = url.as_str().replace(';', "%3B");
    if s.chars().any(|c| c.is_control()) {
        return None;
    }
    Some(s)
}

/// `text` wrapped in an OSC 8 hyperlink to `url` (ST-terminated).
pub(crate) fn osc8(url: &str, text: &str) -> String {
    format!("\x1b]8;;{url}\x1b\\{text}\x1b]8;;\x1b\\")
}

/// Links citations to files inside one workspace root.
#[derive(Debug, Clone)]
pub(crate) struct Linker {
    root: PathBuf,
}

impl Linker {
    /// A linker for `root` (canonicalized); `None` when it does not exist.
    pub(crate) fn new(root: &Path) -> Option<Self> {
        Some(Self {
            root: root.canonicalize().ok()?,
        })
    }

    /// The linker for this process's terminal: `Some` only when
    /// hyperlinks are [`enabled`], rooted at the workspace (current dir).
    pub(crate) fn for_terminal() -> Option<Self> {
        if !enabled() {
            return None;
        }
        Self::new(&std::env::current_dir().ok()?)
    }

    /// The file a cited path names, when it exists inside the workspace.
    fn resolve(&self, cited: &str) -> Option<PathBuf> {
        let joined = self.root.join(cited);
        let real = joined.canonicalize().ok()?;
        (real.starts_with(&self.root) && real.is_file()).then_some(real)
    }

    /// `text` with every citation of an existing workspace file wrapped in
    /// an OSC 8 link. Text without citations comes back unchanged.
    pub(crate) fn link(&self, text: &str) -> String {
        let spans = crate::agent::citation_check::citation_spans(text);
        if spans.is_empty() {
            return text.to_string();
        }
        let mut out = String::with_capacity(text.len() + 64);
        let mut at = 0usize;
        for (range, path) in spans {
            let Some(url) = self.resolve(&path).and_then(|p| file_url(&p)) else {
                continue;
            };
            out.push_str(&text[at..range.start]);
            out.push_str(&osc8(&url, &text[range.clone()]));
            at = range.end;
        }
        out.push_str(&text[at..]);
        out
    }
}

/// Hyperlink the citations of text about to be printed to stdout (run
/// summary, citation lines); unchanged when hyperlinks are off.
pub(crate) fn linkify_for_terminal(text: &str) -> String {
    match Linker::for_terminal() {
        Some(linker) => linker.link(text),
        None => text.to_string(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/output/hyperlink_test.rs"]
mod tests;
