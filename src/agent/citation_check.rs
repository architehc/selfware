//! Deterministic citation verification for final answers (no model call).
//!
//! Evidence (context-validation run, 2026-09-24): a read-only review finished
//! with exit 0 and a "no P1/P2 defects" sign-off, yet it cited
//! `check_id_preserves_test_selectors_and_drops_flags` at
//! `verification_scope.rs:1046-1078` when the test starts at line 493 (and two
//! more tests hundreds of lines off). Exit 0 must not imply a grounded answer
//! (AGENTS.md rule 3).
//!
//! This module parses `path:line`, `path:line-line` (also `–`/`—`),
//! `path#Lnn[-Lmm]` citations, prose `line N` / `(line N)` references tied
//! to an unambiguous path (see `prose_citations`), associates a code-span symbol written right
//! before a citation ("`sym` (`path:l`)", "`sym` at path:l"), and checks each
//! against the workspace: the file exists, the range is inside it, and the
//! named symbol appears within the cited range (± `LINE_TOLERANCE`). When it
//! does not, the file is searched and the actual line is recorded as the
//! suggested correction.
//!
//! The completion gate (`Agent::citation_gate`) feeds wrong citations back to
//! the model for at most `CITATION_GATE_REJECTION_BOUND` correction rounds
//! (the audit ledger's bounded step-aside pattern), then lets the run complete
//! with an explicit "citations: N of M could not be verified (W wrong, ...)" in the run
//! summary, the banner, stream-json and the JSON result.

use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::safety::path_validator::{lexical_normalize_path, PathValidator};

/// A named symbol counts as "in the cited range" within this many lines of it.
pub(crate) const LINE_TOLERANCE: usize = 3;

/// Correction rounds fed back to the model before the gate steps aside and
/// the run completes with an explicit unverified-citations warning.
pub(crate) const CITATION_GATE_REJECTION_BOUND: usize = 2;

/// Cap on distinct citations checked per source (answer or written file).
const MAX_CITATIONS_PER_SOURCE: usize = 400;
/// Files larger than this are not read (reported as unverifiable).
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// Bound on the workspace walk used to resolve bare file names.
const MAX_INDEX_ENTRIES: usize = 50_000;
/// Problem lines listed in a correction directive / structured result.
const MAX_LISTED_PROBLEMS: usize = 12;

/// Extensions a citation path may carry. A closed list keeps host:port
/// (`example.com:8080`) and prose (`e.g.:`) from parsing as citations.
const CITABLE_EXTENSIONS: &str = "rs|py|pyi|js|mjs|cjs|ts|tsx|jsx|go|java|kt|kts|c|h|cc|cpp|cxx|hpp|hh|cs|rb|php|swift|scala|sh|bash|zsh|sql|html|css|scss|vue|svelte|md|markdown|rst|txt|toml|yaml|yml|json|lua|ex|exs|erl|zig|dart|proto|gradle|cmake|mk|adoc";

/// Doc-like deliverables whose citations are verified when the agent wrote
/// them this task (REVIEW.md, report.txt, ...).
const DELIVERABLE_EXTENSIONS: &[&str] = &["md", "markdown", "txt", "rst", "adoc"];

/// One citation parsed from text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Citation {
    /// The path exactly as written.
    pub path: String,
    /// First cited line (1-based).
    pub start: usize,
    /// Last cited line (== `start` for a single line).
    pub end: usize,
    /// Identifier named right before the citation, if any.
    pub symbol: Option<String>,
}

impl Citation {
    fn display(&self) -> String {
        if self.start == self.end {
            format!("{}:{}", self.path, self.start)
        } else {
            format!("{}:{}-{}", self.path, self.start, self.end)
        }
    }
}

/// Outcome of checking one citation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum CitationVerdict {
    /// The named symbol appears within the cited range (± tolerance).
    Verified { file: String },
    /// The symbol is elsewhere in the file; `actual_line` is where.
    WrongLine { file: String, actual_line: usize },
    /// The symbol does not occur anywhere in the resolved file.
    SymbolNotFound { file: String },
    /// No file matches the cited path in the workspace.
    MissingFile,
    /// The cited range lies outside the file (or is malformed).
    OutOfRange { file: String, line_count: usize },
    /// The file and range exist but nothing names a checkable symbol (or the
    /// path is ambiguous / unreadable): neither confirmed nor refuted.
    Unverifiable { reason: String },
}

impl CitationVerdict {
    /// Whether the citation was shown to be wrong.
    pub fn is_problem(&self) -> bool {
        matches!(
            self,
            CitationVerdict::WrongLine { .. }
                | CitationVerdict::SymbolNotFound { .. }
                | CitationVerdict::MissingFile
                | CitationVerdict::OutOfRange { .. }
        )
    }
}

/// A citation and its verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckedCitation {
    pub citation: Citation,
    pub verdict: CitationVerdict,
    /// Where the citation was found: `final answer` or a written file path.
    pub source: String,
}

impl CheckedCitation {
    /// One-line human description of a problem (for directives/summary).
    pub fn describe(&self) -> String {
        let c = &self.citation;
        let what = match &c.symbol {
            Some(sym) => format!("`{sym}` cited at {}", c.display()),
            None => format!("`{}`", c.display()),
        };
        let body = match &self.verdict {
            CitationVerdict::WrongLine { file, actual_line } => {
                format!("{what} but found at {file}:{actual_line}")
            }
            CitationVerdict::SymbolNotFound { file } => {
                format!("{what} but the name does not occur anywhere in {file}")
            }
            CitationVerdict::MissingFile => format!("{what} — no such file in the workspace"),
            CitationVerdict::OutOfRange { file, line_count } => {
                format!("{what} — outside {file}, which has {line_count} lines")
            }
            CitationVerdict::Verified { file } => format!("{what} — verified in {file}"),
            CitationVerdict::Unverifiable { reason } => format!("{what} — unverifiable ({reason})"),
        };
        if self.source == ANSWER_SOURCE {
            body
        } else {
            format!("{body} [in {}]", self.source)
        }
    }
}

/// Wording of the "no checkable citation" warning (banner, summary, JSON).
pub(crate) const CITATIONS_NONE_CHECKABLE: &str = "citations: none checkable";

/// Source label for citations taken from the final answer text.
pub(crate) const ANSWER_SOURCE: &str = "final answer";

/// Aggregate of every checked citation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CitationReport {
    pub total: usize,
    pub verified: usize,
    pub wrong_line: Vec<CheckedCitation>,
    pub symbol_not_found: Vec<CheckedCitation>,
    pub missing_file: Vec<CheckedCitation>,
    pub out_of_range: Vec<CheckedCitation>,
    /// File and range exist but no symbol was named (or path ambiguous).
    pub unverifiable: usize,
}

impl CitationReport {
    fn push(&mut self, checked: CheckedCitation) {
        self.total += 1;
        match &checked.verdict {
            CitationVerdict::Verified { .. } => self.verified += 1,
            CitationVerdict::Unverifiable { .. } => self.unverifiable += 1,
            CitationVerdict::WrongLine { .. } => self.wrong_line.push(checked),
            CitationVerdict::SymbolNotFound { .. } => self.symbol_not_found.push(checked),
            CitationVerdict::MissingFile => self.missing_file.push(checked),
            CitationVerdict::OutOfRange { .. } => self.out_of_range.push(checked),
        }
    }

    /// Fold another report (e.g. a written deliverable's) into this one.
    pub fn merge(&mut self, other: CitationReport) {
        self.total += other.total;
        self.verified += other.verified;
        self.unverifiable += other.unverifiable;
        self.wrong_line.extend(other.wrong_line);
        self.symbol_not_found.extend(other.symbol_not_found);
        self.missing_file.extend(other.missing_file);
        self.out_of_range.extend(other.out_of_range);
    }

    /// Citations shown to be wrong (wrong line, missing name/file, bad range).
    pub fn problem_count(&self) -> usize {
        self.wrong_line.len()
            + self.symbol_not_found.len()
            + self.missing_file.len()
            + self.out_of_range.len()
    }

    /// Every problem, in a stable order.
    pub fn problems(&self) -> impl Iterator<Item = &CheckedCitation> {
        self.wrong_line
            .iter()
            .chain(self.symbol_not_found.iter())
            .chain(self.missing_file.iter())
            .chain(self.out_of_range.iter())
    }
}

/// Run-level grounding outcome for the summary, banner and JSON result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GroundingStatus {
    /// Distinct citations checked (final answer + written deliverables).
    pub total: usize,
    /// Symbol confirmed inside the cited range.
    pub verified: usize,
    /// File and range exist, but no named symbol could be checked.
    pub unverifiable: usize,
    pub wrong_line: usize,
    pub symbol_not_found: usize,
    pub missing_file: usize,
    pub out_of_range: usize,
    /// Correction rounds the gate fed back to the model.
    pub correction_rounds: usize,
    /// Written deliverables whose citations were checked too.
    pub checked_files: Vec<String>,
    /// Up to a dozen problem descriptions (wrong-line entries name the
    /// actual location).
    pub problems: Vec<String>,
    /// The answer belongs to a read-only (review/report) task, whose claims
    /// are expected to be grounded in checkable citations. Not serialized.
    #[serde(skip)]
    pub read_only: bool,
}

impl GroundingStatus {
    pub fn from_report(
        report: &CitationReport,
        correction_rounds: usize,
        checked_files: Vec<String>,
    ) -> Self {
        GroundingStatus {
            total: report.total,
            verified: report.verified,
            unverifiable: report.unverifiable,
            wrong_line: report.wrong_line.len(),
            symbol_not_found: report.symbol_not_found.len(),
            missing_file: report.missing_file.len(),
            out_of_range: report.out_of_range.len(),
            correction_rounds,
            checked_files,
            problems: report
                .problems()
                .take(MAX_LISTED_PROBLEMS)
                .map(CheckedCitation::describe)
                .collect(),
            read_only: false,
        }
    }

    /// Citations shown to be wrong.
    pub fn problem_count(&self) -> usize {
        self.wrong_line + self.symbol_not_found + self.missing_file + self.out_of_range
    }

    /// Everything not positively verified (wrong + unverifiable).
    pub fn unverified_count(&self) -> usize {
        self.total.saturating_sub(self.verified)
    }

    /// `citations: N of M could not be verified (W wrong, K without a
    /// checkable symbol)` — the note the banner, run summary, JSON result
    /// and failure-mode evidence carry when problems remain. N is
    /// [`Self::unverified_count`] (everything not positively verified), so
    /// the count and the words agree, and the breakdown matches
    /// [`Self::grounding_line`].
    pub fn unverified_note(&self) -> String {
        format!(
            "citations: {} of {} could not be verified{}",
            self.unverified_count(),
            self.total,
            self.unverified_breakdown()
        )
    }

    /// ` (W wrong, K without a checkable symbol)`, parts omitted when zero;
    /// empty when everything verified.
    fn unverified_breakdown(&self) -> String {
        let mut parts = Vec::new();
        if self.problem_count() > 0 {
            parts.push(format!("{} wrong", self.problem_count()));
        }
        if self.unverifiable > 0 {
            parts.push(format!("{} without a checkable symbol", self.unverifiable));
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!(" ({})", parts.join(", "))
        }
    }

    /// Citations actually checked against a named symbol or a range: the
    /// verified ones plus the ones shown wrong.
    pub fn checkable_count(&self) -> usize {
        self.verified + self.problem_count()
    }

    /// A review/report answer with no checkable citation at all: nothing in
    /// it was checked against the files, so it must not read as grounded
    /// (0.8.2 live validation D4).
    pub fn none_checkable(&self) -> bool {
        self.read_only && self.checkable_count() == 0
    }

    /// `citations: none checkable: ...` — see [`Self::none_checkable`].
    pub fn none_checkable_note(&self) -> String {
        let why = if self.total == 0 {
            "no path:line citations in the answer".to_string()
        } else {
            format!(
                "{} of {} without a checkable symbol",
                self.unverifiable, self.total
            )
        };
        format!("{CITATIONS_NONE_CHECKABLE}: {why}")
    }

    /// The warning note the banner, run summary, JSON `grounding.note` and
    /// failure-mode evidence carry, or `None` for a grounded answer: wrong
    /// citations first ([`Self::unverified_note`]), else "none checkable"
    /// for a review/report answer ([`Self::none_checkable_note`]).
    pub fn warning_note(&self) -> Option<String> {
        if self.problem_count() > 0 {
            Some(self.unverified_note())
        } else if self.none_checkable() {
            Some(self.none_checkable_note())
        } else {
            None
        }
    }

    /// The summary's "Grounding:" line. Names what was actually checked:
    /// `verified` means the named symbol was found inside the cited range,
    /// never more (AGENTS.md rule 3).
    pub fn grounding_line(&self) -> String {
        if self.total == 0 {
            let lead = if self.read_only {
                format!("Grounding: {CITATIONS_NONE_CHECKABLE} — ")
            } else {
                "Grounding: ".to_string()
            };
            return format!(
                "{lead}no path:line citations in the answer (nothing checked against files)"
            );
        }
        let detail = self.unverified_breakdown();
        format!(
            "Grounding: {} verified citations, {} unverified{detail}",
            self.verified,
            self.unverified_count()
        )
    }
}

fn citation_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let pattern = format!(
            r"(?P<path>[A-Za-z0-9_./\-]*[A-Za-z0-9_\-]\.(?:{CITABLE_EXTENSIONS}))(?:#L(?P<hs>\d{{1,7}})(?:-L?(?P<he>\d{{1,7}}))?|:(?P<s>\d{{1,7}})(?:\s?[-–—]\s?L?(?P<e>\d{{1,7}}))?)"
        );
        Regex::new(&pattern).expect("citation regex compiles")
    })
}

fn is_path_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '-')
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Parse every citation in `text`, in order, de-duplicated.
pub fn parse_citations(text: &str) -> Vec<Citation> {
    let mut out: Vec<Citation> = Vec::new();
    let mut seen = BTreeSet::new();
    for caps in citation_regex().captures_iter(text) {
        let m = caps.name("path").expect("path group");
        let whole = caps.get(0).expect("whole match");
        // Not a citation when glued to a longer token (`x.com/a.rs` inside a
        // URL, `v1.2.rs` noise) or followed by more digits/identifier.
        let before = &text[..m.start()];
        if before.ends_with("//") || before.ends_with(':') {
            continue;
        }
        if before.chars().next_back().is_some_and(is_path_char) {
            continue;
        }
        if text[whole.end()..]
            .chars()
            .next()
            .is_some_and(|c| is_ident_char(c) && !c.is_ascii_digit())
        {
            continue;
        }
        let num = |name: &str| {
            caps.name(name)
                .and_then(|g| g.as_str().parse::<usize>().ok())
        };
        let Some(start) = num("s").or_else(|| num("hs")) else {
            continue;
        };
        let end = num("e").or_else(|| num("he")).unwrap_or(start);
        let path = m.as_str().trim_start_matches("./").to_string();
        let symbol = symbol_before(before);
        let key = (path.clone(), start, end, symbol.clone());
        if !seen.insert(key) {
            continue;
        }
        out.push(Citation {
            path,
            start,
            end,
            symbol,
        });
        if out.len() >= MAX_CITATIONS_PER_SOURCE {
            break;
        }
    }
    for c in prose_citations(text) {
        if out.len() >= MAX_CITATIONS_PER_SOURCE {
            break;
        }
        let key = (c.path.clone(), c.start, c.end, c.symbol.clone());
        if seen.insert(key) {
            out.push(c);
        }
    }
    out
}

/// A path mention without a `:line` suffix (prose citations tie to these).
fn bare_path_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r"(?P<path>[A-Za-z0-9_./\-]*[A-Za-z0-9_\-]\.(?:{CITABLE_EXTENSIONS}))\b"
        ))
        .expect("bare path regex compiles")
    })
}

/// A prose line reference: `line N`, `lines N-M`, `(line N)`,
/// `(test file line N)`.
fn prose_line_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?P<open>\(\s*)?(?P<tf>test\s+file\s+)?\blines?\s+(?P<s>\d{1,7})(?:\s*(?:-|–|—|to)\s*(?P<e>\d{1,7}))?\b",
        )
        .expect("prose line regex compiles")
    })
}

/// Bare path mentions in `text`: (start, end, path), glued tokens (URLs,
/// longer identifiers) excluded.
fn path_mentions(text: &str) -> Vec<(usize, usize, String)> {
    bare_path_regex()
        .captures_iter(text)
        .filter_map(|c| {
            let m = c.name("path")?;
            let before = &text[..m.start()];
            if before.ends_with("//") || before.chars().next_back().is_some_and(is_path_char) {
                return None;
            }
            Some((
                m.start(),
                m.end(),
                m.as_str().trim_start_matches("./").to_string(),
            ))
        })
        .collect()
}

fn is_test_path(path: &str) -> bool {
    path.to_ascii_lowercase().contains("test")
}

fn unique_paths<'a>(it: impl Iterator<Item = &'a String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in it {
        if !out.contains(p) {
            out.push(p.clone());
        }
    }
    out
}

/// Prose citations (0.8.2 live validation D4: a review cited everything as
/// "`sym` (line 22)" under a heading naming the file, so the gate saw 0
/// citations while 17 of 20 hand-checked were wrong). A `line N` /
/// `lines N-M` / `(line N)` reference is tied to a path, in this order:
/// 1. the path right after it (`line N of p`, `lines N-M in p`);
/// 2. the path right before it (`p, line N`, `p (line N)`, `p at line N`);
/// 3. the only path named in the same clause (bounded by `.`/`;`/newline);
/// 4. the nearest preceding markdown heading, when it names one path — or,
///    naming several, exactly one test path for `test file line N` and
///    exactly one non-test path otherwise.
///
/// A reference with no unambiguous path is not a citation (never guessed).
fn prose_citations(text: &str) -> Vec<Citation> {
    let mentions = path_mentions(text);
    let mut out = Vec::new();
    for caps in prose_line_regex().captures_iter(text) {
        let whole = caps.get(0).expect("whole match");
        let num = |name: &str| {
            caps.name(name)
                .and_then(|g| g.as_str().parse::<usize>().ok())
        };
        let Some(start) = num("s") else { continue };
        let end = num("e").unwrap_or(start);
        let ref_start = whole.start();
        let test_file = caps.name("tf").is_some();

        // 1. `line N of p` / `line N in p`.
        let after = &text[whole.end()..];
        let after_trim = after.trim_start();
        let lead = after.len() - after_trim.len();
        let path_after = ["of ", "in "].iter().find_map(|w| {
            let rest = after_trim.strip_prefix(w)?;
            let skip = rest.len() - rest.trim_start_matches(['`', '*', ' ']).len();
            let at = whole.end() + lead + w.len() + skip;
            mentions.iter().find(|(s, _, _)| *s == at)
        });
        // 2. `p, line N` / `p (line N)` / `p at line N`.
        let path_before = || {
            let mut b = text[..ref_start].trim_end();
            for word in ["at", "on"] {
                if let Some(stripped) = b.strip_suffix(word) {
                    if stripped.ends_with(char::is_whitespace) {
                        b = stripped.trim_end();
                    }
                }
            }
            let b = b.trim_end_matches(['`', '*', ',', ':', '—', '(', ' ']);
            mentions.iter().find(|(_, e, _)| *e == b.len())
        };
        let (path, via_before) = if let Some((_, _, p)) = path_after {
            (Some(p.clone()), None)
        } else if let Some((s, _, p)) = path_before() {
            (Some(p.clone()), Some(*s))
        } else {
            // 3. The only path in the same clause.
            let clause_start = text[..ref_start]
                .rfind(['\n', ';'])
                .into_iter()
                .chain(text[..ref_start].rfind(". ").map(|i| i + 1))
                .max()
                .map(|i| i + 1)
                .unwrap_or(0);
            let clause_end = text[whole.end()..]
                .find(['\n', ';'])
                .into_iter()
                .chain(text[whole.end()..].find(". "))
                .min()
                .map(|i| whole.end() + i)
                .unwrap_or(text.len());
            let in_clause = unique_paths(
                mentions
                    .iter()
                    .filter(|(s, e, _)| *s >= clause_start && *e <= clause_end)
                    .map(|(_, _, p)| p),
            );
            if in_clause.len() == 1 {
                (in_clause.into_iter().next(), None)
            } else if in_clause.is_empty() {
                // 4. The nearest preceding heading.
                let heading = text[..ref_start]
                    .rsplit('\n')
                    .find(|l| l.trim_start().starts_with('#'))
                    .map(|l| {
                        let off = l.as_ptr() as usize - text.as_ptr() as usize;
                        (off, off + l.len())
                    });
                let named = heading
                    .map(|(hs, he)| {
                        unique_paths(
                            mentions
                                .iter()
                                .filter(|(s, e, _)| *s >= hs && *e <= he)
                                .map(|(_, _, p)| p),
                        )
                    })
                    .unwrap_or_default();
                let pick = if named.len() == 1 {
                    named.into_iter().next()
                } else {
                    let (tests, others): (Vec<String>, Vec<String>) =
                        named.into_iter().partition(|p| is_test_path(p));
                    let pool = if test_file { tests } else { others };
                    (pool.len() == 1).then(|| pool.into_iter().next().expect("one"))
                };
                (pick, None)
            } else {
                (None, None)
            }
        };
        let Some(path) = path else { continue };
        // `symbol_before` expects the text right before a citation, whose
        // trailing backtick opens the citation's own code span; a prose
        // reference is not in one, so end the prefix with a neutral "(".
        let prefix_symbol = |end: usize| symbol_before(&format!("{} (", text[..end].trim_end()));
        let symbol = prefix_symbol(ref_start).or_else(|| via_before.and_then(prefix_symbol));
        out.push(Citation {
            path,
            start,
            end,
            symbol,
        });
    }
    out
}

/// Words allowed between a code-span symbol and its citation:
/// "`X` (`p:1`)", "`X` at p:1", "`X` enum (`p:1`)", "`X` defined at p:1".
const CONNECTOR_WORDS: &[&str] = &[
    "at",
    "in",
    "see",
    "defined",
    "line",
    "lines",
    "enum",
    "struct",
    "fn",
    "function",
    "method",
    "test",
    "trait",
    "const",
    "constant",
    "static",
    "type",
    "macro",
    "module",
    "impl",
    "field",
    "variant",
    "class",
    "def",
    // "`X` is defined at p:l", "`X` lives at", "`X` is declared in", ...
    // (0.8.2 live validation D4).
    "is",
    "are",
    "was",
    "lives",
    "located",
    "declared",
    "implemented",
    "found",
    "helper",
    "on",
];

/// The identifier in the code span immediately preceding a citation, if the
/// text between them is only punctuation/connector words on the same line.
fn symbol_before(before: &str) -> Option<String> {
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let mut rest = &before[line_start..];
    // Markdown emphasis around the citation: "`X` is defined at **p:1**".
    rest = rest.trim_end().trim_end_matches(['*', '_']);
    // The citation itself may sit in a code span: "`X` (`p:1`)".
    rest = rest.trim_end().strip_suffix('`').unwrap_or(rest);
    rest = rest.trim_end().trim_end_matches('*');
    for _ in 0..6 {
        let trimmed = rest.trim_end();
        let trimmed = trimmed
            .strip_suffix('(')
            .or_else(|| trimmed.strip_suffix(':'))
            .or_else(|| trimmed.strip_suffix(','))
            .or_else(|| trimmed.strip_suffix('—'))
            .or_else(|| trimmed.strip_suffix('-'))
            .unwrap_or(trimmed)
            .trim_end();
        let word_start = trimmed
            .char_indices()
            .rev()
            .find(|(_, c)| !c.is_ascii_alphabetic())
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        let word = &trimmed[word_start..];
        if !word.is_empty()
            && CONNECTOR_WORDS.iter().any(|w| w.eq_ignore_ascii_case(word))
            && (word_start == 0 || trimmed[..word_start].ends_with(char::is_whitespace))
        {
            rest = &trimmed[..word_start];
            continue;
        }
        rest = trimmed;
        break;
    }
    // A bold symbol: "**`X`** at p:1".
    let rest = rest.trim_end().trim_end_matches('*');
    let inner_end = rest.strip_suffix('`')?;
    let open = inner_end.rfind('`')?;
    // "`render_kv`/`_no_detail`" — a suffix shorthand of the previous span,
    // not a name that occurs in the file.
    if inner_end[..open].ends_with('/') {
        return None;
    }
    normalize_symbol(&inner_end[open + 1..])
}

/// Reduce a code span to one checkable identifier: `Type::name()` → `name`,
/// `MAX_LEN = 16_384` → `MAX_LEN`, `render()` → `render`. Spans that do not
/// start with an identifier (or whose identifier is < 3 chars) yield `None`.
fn normalize_symbol(span: &str) -> Option<String> {
    let span = span.trim();
    let lead: String = span
        .chars()
        .take_while(|c| is_ident_char(*c) || *c == ':' || *c == '.')
        .collect();
    // Only identifier-shaped spans: `name`, `name()`, `NAME = 1`, `Type<T>`.
    // A phrase (`cargo test --lib`), expression (`a>=2`) or format string
    // (`turn_{step:04}.json`) names no symbol.
    let rem = span[lead.len()..].trim_start();
    if !(rem.is_empty() || rem.starts_with(['(', '=', '<', '!', '[', ';', ','])) {
        return None;
    }
    let last = lead.rsplit([':', '.']).find(|s| !s.is_empty())?.to_string();
    let first = last.chars().next()?;
    if last.len() < 3 || !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    Some(last)
}

/// Reason recorded for a citation whose path fails workspace confinement or
/// the file-tool path policy. Deliberately uniform: it never says whether
/// the target exists, how long it is, or where a symbol sits in it.
pub(crate) const OUTSIDE_POLICY_REASON: &str = "outside workspace/policy";

/// Resolves cited paths against the workspace and caches file lines.
///
/// Confinement: the cited path is MODEL OUTPUT, and verdicts ("found at
/// line M", "no such file") are fed back to the model, so citation checks
/// hold the same boundary the file tools hold. Every candidate
/// 1. must lexically stay inside the workspace root (absolute paths outside
///    it and `..` escapes are refused before the filesystem is touched),
/// 2. must pass the file tools' own [`PathValidator`] policy (allowed,
///    denied and protected-system paths) anchored at the canonical root,
/// 3. must resolve — symlinks followed — to a location inside the canonical
///    root, and
/// 4. is read only through [`PathValidator::open_regular_file`], which
///    re-validates the OPENED descriptor's real path (no check-then-open
///    race); that real path is checked against the root once more.
///
/// A candidate failing any step is `Unverifiable` with
/// [`OUTSIDE_POLICY_REASON`] and is never read; it is never reported as a
/// missing file or a wrong line, which would leak its existence or content.
pub struct CitationResolver {
    /// Canonical workspace root (symlinks resolved).
    root: PathBuf,
    /// The root as given, when it differs from the canonical form (macOS
    /// `/var` → `/private/var`): absolute citations written against it are
    /// mapped onto the canonical root.
    given_root: PathBuf,
    validator: PathValidator,
    /// Full paths mentioned anywhere in the checked text (disambiguation).
    mentioned: Vec<String>,
    index: Option<Vec<PathBuf>>,
    files: HashMap<PathBuf, Loaded>,
}

/// A candidate file after confinement and (maybe) reading.
enum Loaded {
    Lines(Vec<String>),
    /// Failed confinement or policy: never read.
    Refused,
    /// Inside the workspace and allowed, but could not be read (too large,
    /// not a regular file, vanished).
    Unreadable,
}

enum Resolution {
    Found(PathBuf),
    Ambiguous(Vec<PathBuf>),
    Missing,
    /// Fails workspace confinement or path policy.
    Refused,
}

/// Outcome of confining one path (see [`CitationResolver`]).
enum Confined {
    /// Inside the root, allowed, exists: the absolute (lexical) path.
    Existing(PathBuf),
    /// Inside the root and allowed, but nothing is there.
    Absent,
    Refused,
}

impl CitationResolver {
    /// Resolver confined to `root` under the default file-tool policy
    /// (`allowed_paths = ["./**"]` anchored at `root`, default denied paths).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self::with_policy(root, &crate::config::SafetyConfig::default())
    }

    /// Resolver confined to `root` AND the given file-tool policy — the
    /// agent passes its own `[safety]` config, the one its file tools use.
    pub fn with_policy(root: impl Into<PathBuf>, policy: &crate::config::SafetyConfig) -> Self {
        let given_root = root.into();
        let root = std::fs::canonicalize(&given_root).unwrap_or_else(|_| given_root.clone());
        CitationResolver {
            validator: PathValidator::new(policy, root.clone()),
            root,
            given_root,
            mentioned: Vec::new(),
            index: None,
            files: HashMap::new(),
        }
    }

    fn relative_display(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }

    /// Steps 1–3 of the confinement contract (see the type docs). Touches
    /// the filesystem only after the lexical check has passed.
    fn confine(&self, cited: &str) -> Confined {
        if cited.contains('\0') {
            return Confined::Refused;
        }
        let raw = Path::new(cited);
        let joined = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            self.root.join(raw)
        };
        let lexical = lexical_normalize_path(&joined);
        // 1. Lexical containment (absolute paths and `..` escapes).
        let lexical = if lexical.starts_with(&self.root) {
            lexical
        } else if let Ok(rest) = lexical.strip_prefix(&self.given_root) {
            self.root.join(rest)
        } else {
            return Confined::Refused;
        };
        // 2. The file tools' own policy (allowed/denied/protected paths).
        if self.validator.validate(&lexical.to_string_lossy()).is_err() {
            return Confined::Refused;
        }
        // 3. Real location (symlinks followed) must stay inside the root.
        //    A missing path is judged by its deepest existing ancestor, so a
        //    symlinked directory pointing outside cannot answer "missing".
        match std::fs::canonicalize(&lexical) {
            Ok(real) if real.starts_with(&self.root) => Confined::Existing(lexical),
            Ok(_) => Confined::Refused,
            Err(_) => {
                let inside = lexical
                    .ancestors()
                    .skip(1)
                    .find_map(|a| std::fs::canonicalize(a).ok())
                    .is_some_and(|real| real.starts_with(&self.root));
                if inside && std::fs::symlink_metadata(&lexical).is_err() {
                    Confined::Absent
                } else {
                    // A dangling symlink (target unknown) or an ancestor
                    // outside the root: refuse rather than say "missing".
                    Confined::Refused
                }
            }
        }
    }

    fn index(&mut self) -> &[PathBuf] {
        if self.index.is_none() {
            let mut entries = Vec::new();
            // `follow_links(false)`: a symlink is reported as a symlink, not
            // as its target's type, so the file filter below drops every
            // symlink — one pointing outside the root never enters the
            // suffix index (and each candidate is re-confined on read).
            let walker = walkdir::WalkDir::new(&self.root)
                .follow_links(false)
                .into_iter()
                .filter_entry(|e| {
                    let name = e.file_name().to_string_lossy();
                    e.depth() == 0
                        || !(name.starts_with('.')
                            || name == "target"
                            || name == "node_modules"
                            || name == "__pycache__")
                });
            for entry in walker.flatten() {
                if entry.file_type().is_file() && !entry.path_is_symlink() {
                    entries.push(entry.into_path());
                    if entries.len() >= MAX_INDEX_ENTRIES {
                        break;
                    }
                }
            }
            self.index = Some(entries);
        }
        self.index.as_deref().unwrap_or(&[])
    }

    fn resolve(&mut self, cited: &str) -> Resolution {
        match self.confine(cited) {
            Confined::Existing(path) => return Resolution::Found(path),
            Confined::Refused => return Resolution::Refused,
            Confined::Absent => {}
        }
        // Bare or partial path (`verification_scope.rs`): match by suffix.
        // Only in-root, non-symlink index entries can match (see `index`).
        let suffix: Vec<&str> = cited.split('/').filter(|s| !s.is_empty()).collect();
        if suffix.is_empty() {
            return Resolution::Missing;
        }
        let root = self.root.clone();
        let candidates: Vec<PathBuf> = self
            .index()
            .iter()
            .filter(|p| {
                let rel = p.strip_prefix(&root).unwrap_or(p);
                let comps: Vec<String> = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                comps.len() >= suffix.len()
                    && comps[comps.len() - suffix.len()..]
                        .iter()
                        .zip(suffix.iter())
                        .all(|(a, b)| a == b)
            })
            .cloned()
            .collect();
        match candidates.len() {
            0 => Resolution::Missing,
            1 => Resolution::Found(candidates.into_iter().next().expect("one candidate")),
            _ => {
                // Prefer a candidate whose full relative path the text names.
                let named: Vec<&PathBuf> = candidates
                    .iter()
                    .filter(|p| {
                        let rel = self.relative_display(p);
                        self.mentioned.iter().any(|m| m == &rel)
                    })
                    .collect();
                if named.len() == 1 {
                    Resolution::Found(named[0].clone())
                } else {
                    Resolution::Ambiguous(candidates)
                }
            }
        }
    }

    /// Step 4: open through the validated descriptor and read from it.
    fn load(&self, path: &Path) -> Loaded {
        use std::io::Read;
        let path_str = path.to_string_lossy();
        if !matches!(self.confine(&path_str), Confined::Existing(_)) {
            return Loaded::Refused;
        }
        let Ok(opened) = self.validator.open_regular_file(&path_str) else {
            // Policy refusal or not a regular file: either way, never read.
            return Loaded::Refused;
        };
        if !opened.real_path.starts_with(&self.root) {
            return Loaded::Refused;
        }
        let mut file = opened.file;
        match file.metadata() {
            Ok(m) if m.len() <= MAX_FILE_BYTES => {}
            _ => return Loaded::Unreadable,
        }
        let mut bytes = Vec::new();
        if file.read_to_end(&mut bytes).is_err() {
            return Loaded::Unreadable;
        }
        Loaded::Lines(
            String::from_utf8_lossy(&bytes)
                .lines()
                .map(str::to_string)
                .collect(),
        )
    }

    fn lines(&mut self, path: &Path) -> &Loaded {
        if !self.files.contains_key(path) {
            let loaded = self.load(path);
            self.files.insert(path.to_path_buf(), loaded);
        }
        self.files.get(path).expect("just inserted")
    }

    /// Read a file named by model output (a written deliverable) under the
    /// same confinement as citations. `None` when it fails confinement or
    /// policy, is missing, or cannot be read — callers skip it silently.
    pub(crate) fn read_confined(&mut self, cited: &str) -> Option<String> {
        let Confined::Existing(path) = self.confine(cited) else {
            return None;
        };
        match self.lines(&path) {
            Loaded::Lines(lines) => Some(lines.join("\n")),
            Loaded::Refused | Loaded::Unreadable => None,
        }
    }

    fn check_in_file(&mut self, path: &Path, c: &Citation) -> CitationVerdict {
        let file = self.relative_display(path);
        let lines = match self.lines(path) {
            Loaded::Lines(lines) => lines,
            Loaded::Refused => {
                return CitationVerdict::Unverifiable {
                    reason: OUTSIDE_POLICY_REASON.to_string(),
                }
            }
            Loaded::Unreadable => {
                return CitationVerdict::Unverifiable {
                    reason: format!("{file} could not be read"),
                }
            }
        };
        let line_count = lines.len();
        if c.start == 0 || c.end < c.start || c.end > line_count {
            return CitationVerdict::OutOfRange { file, line_count };
        }
        let Some(sym) = &c.symbol else {
            return CitationVerdict::Unverifiable {
                reason: "no symbol named next to the citation".to_string(),
            };
        };
        let lo = c.start.saturating_sub(LINE_TOLERANCE).max(1);
        let hi = (c.end + LINE_TOLERANCE).min(line_count);
        if (lo..=hi).any(|n| contains_word(&lines[n - 1], sym)) {
            return CitationVerdict::Verified { file };
        }
        match locate_symbol(lines, sym) {
            Some(actual_line) => CitationVerdict::WrongLine { file, actual_line },
            None => CitationVerdict::SymbolNotFound { file },
        }
    }

    /// Check one citation.
    pub fn check(&mut self, c: &Citation) -> CitationVerdict {
        match self.resolve(&c.path) {
            Resolution::Missing => CitationVerdict::MissingFile,
            Resolution::Refused => CitationVerdict::Unverifiable {
                reason: OUTSIDE_POLICY_REASON.to_string(),
            },
            Resolution::Found(path) => self.check_in_file(&path, c),
            Resolution::Ambiguous(candidates) => {
                // Verified when exactly one same-named file confirms it;
                // otherwise we cannot tell which file was meant.
                let verified: Vec<CitationVerdict> = candidates
                    .iter()
                    .map(|p| self.check_in_file(p, c))
                    .filter(|v| matches!(v, CitationVerdict::Verified { .. }))
                    .collect();
                if verified.len() == 1 {
                    verified.into_iter().next().expect("one verdict")
                } else {
                    CitationVerdict::Unverifiable {
                        reason: format!(
                            "ambiguous path: {} files named {}",
                            candidates.len(),
                            c.path
                        ),
                    }
                }
            }
        }
    }

    /// Verify every citation in `text`, attributing them to `source`.
    pub fn verify_text(&mut self, text: &str, source: &str) -> CitationReport {
        self.mentioned = mentioned_paths(text);
        let mut report = CitationReport::default();
        for citation in parse_citations(text) {
            let verdict = self.check(&citation);
            report.push(CheckedCitation {
                citation,
                verdict,
                source: source.to_string(),
            });
        }
        report
    }
}

/// Full relative paths (with a `/`) mentioned anywhere in the text.
fn mentioned_paths(text: &str) -> Vec<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(&format!(
            r"[A-Za-z0-9_\-.]+(?:/[A-Za-z0-9_\-.]+)+\.(?:{CITABLE_EXTENSIONS})\b"
        ))
        .expect("mentioned-path regex compiles")
    });
    re.find_iter(text)
        .map(|m| m.as_str().trim_start_matches("./").to_string())
        .collect()
}

/// Whole-word occurrence of `word` in `line`.
fn contains_word(line: &str, word: &str) -> bool {
    let mut from = 0;
    while let Some(pos) = line[from..].find(word) {
        let at = from + pos;
        let before_ok = line[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !is_ident_char(c));
        let after_ok = line[at + word.len()..]
            .chars()
            .next()
            .is_none_or(|c| !is_ident_char(c));
        if before_ok && after_ok {
            return true;
        }
        from = at + word.len();
    }
    false
}

/// Where `sym` actually lives: its definition line when one is recognisable
/// (`fn sym`, `struct sym`, `def sym`, `const sym`, ...), else its first
/// whole-word occurrence. 1-based.
pub(crate) fn locate_symbol(lines: &[String], sym: &str) -> Option<usize> {
    const DEF_KEYWORDS: &[&str] = &[
        "fn",
        "struct",
        "enum",
        "trait",
        "type",
        "const",
        "static",
        "mod",
        "class",
        "def",
        "func",
        "function",
        "interface",
        "let",
        "var",
        "macro_rules!",
    ];
    let is_definition = |line: &str| {
        let tokens: Vec<&str> = line
            .split(|c: char| c.is_whitespace() || c == '(' || c == '<' || c == ':' || c == '{')
            .filter(|t| !t.is_empty())
            .collect();
        tokens
            .windows(2)
            .any(|w| DEF_KEYWORDS.contains(&w[0]) && w[1] == sym)
    };
    lines
        .iter()
        .position(|l| contains_word(l, sym) && is_definition(l))
        .or_else(|| lines.iter().position(|l| contains_word(l, sym)))
        .map(|i| i + 1)
}

/// Whether a written path is a doc-like deliverable whose citations are
/// checked (REVIEW.md, notes.txt, ...).
pub(crate) fn is_deliverable_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| {
            DELIVERABLE_EXTENSIONS
                .iter()
                .any(|d| d.eq_ignore_ascii_case(ext))
        })
}

/// The directive fed back to the model for one correction round.
pub(crate) fn correction_directive(report: &CitationReport, round: usize) -> String {
    let listed: Vec<String> = report
        .problems()
        .take(MAX_LISTED_PROBLEMS)
        .map(|p| format!("- {}", p.describe()))
        .collect();
    let more = report.problem_count().saturating_sub(listed.len());
    let more_note = if more > 0 {
        format!("\n- ... and {more} more")
    } else {
        String::new()
    };
    format!(
        "CITATION CHECK — completion blocked (correction round {round} of \
         {CITATION_GATE_REJECTION_BOUND}). {} of {} citations do not match the files in the \
         workspace:\n{}{more_note}\n\
         Fix each citation (re-read the file if you are unsure of the line: file_read shows \
         each line's number before a tab — cite that number, do not count lines) or remove it, and \
         remove any claim that rested only on it; then give your final answer again. \
         Do not add citations you have not checked.",
        report.problem_count(),
        report.total,
        listed.join("\n")
    )
}

/// Per-task state of the citation completion gate.
#[derive(Debug, Default)]
pub(crate) struct CitationGateState {
    /// Correction rounds already fed back (bounded).
    pub rejections: usize,
    /// Loop step of the latest rejection: repeat gate calls within one turn
    /// return the same directive without spending another round.
    pub last_rejected_step: Option<usize>,
    /// (step, content hash) of the last evaluation and its result — gate
    /// probes run several times per turn; the files are read once.
    pub last_eval: Option<((usize, u64), Option<String>)>,
    /// Outcome of the latest evaluation, read by the summary/banner/JSON.
    pub status: Option<GroundingStatus>,
}

impl super::Agent {
    /// The answer text the completion gate is judging: the latest assistant
    /// message (pushed to history before any gate probe runs), falling back
    /// to `last_assistant_response`.
    fn citation_candidate_answer(&self) -> String {
        let latest = self
            .messages
            .iter()
            .rev()
            .find(|m| m.role == "assistant")
            .map(|m| super::recovery::strip_think_blocks(&m.content.text_all()))
            .unwrap_or_default();
        if latest.trim().is_empty() {
            self.last_assistant_response.clone()
        } else {
            latest
        }
    }

    /// Doc-like files this task wrote (REVIEW.md, ...), deduplicated, in
    /// write order.
    fn written_deliverables(&self) -> Vec<String> {
        let Some(cp) = self.current_checkpoint.as_ref() else {
            return Vec::new();
        };
        let mut out: Vec<String> = Vec::new();
        for call in cp.tool_calls.iter().filter(|c| c.success) {
            if !matches!(
                call.tool_name.as_str(),
                "file_write" | "file_edit" | "file_fim_edit" | "file_multi_edit"
            ) {
                continue;
            }
            let Ok(args) = serde_json::from_str::<serde_json::Value>(&call.arguments) else {
                continue;
            };
            let mut paths: Vec<String> = args
                .get("path")
                .and_then(|p| p.as_str())
                .map(|p| vec![p.to_string()])
                .unwrap_or_default();
            if let Some(edits) = args.get("edits").and_then(|e| e.as_array()) {
                paths.extend(
                    edits
                        .iter()
                        .filter_map(|e| e.get("path").and_then(|p| p.as_str()))
                        .map(str::to_string),
                );
            }
            for p in paths {
                if is_deliverable_path(&p) && !out.contains(&p) {
                    out.push(p);
                }
            }
        }
        out
    }

    /// Deterministic citation gate (no model call). Runs for read-only /
    /// review / report tasks and for any final answer or written deliverable
    /// that contains citations. Wrong citations are fed back for at most
    /// [`CITATION_GATE_REJECTION_BOUND`] correction rounds; after that the
    /// gate steps aside and the run completes with the unverified count
    /// recorded (summary, banner, stream-json, JSON result).
    pub(super) fn citation_gate(&self, is_read_only: bool) -> Option<String> {
        let root = self.tools.workspace_root().path();
        let answer = self.citation_candidate_answer();
        // Deliverable paths come from the model's tool arguments: read them
        // under the same workspace + file-tool policy confinement as the
        // citations themselves (a file written then swapped for a symlink
        // is skipped, never followed).
        let mut resolver = CitationResolver::with_policy(&root, &self.config.safety);
        let deliverables: Vec<(String, String)> = self
            .written_deliverables()
            .into_iter()
            .filter_map(|p| resolver.read_confined(&p).map(|text| (p, text)))
            .collect();

        let step = self.loop_control.current_step();
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            answer.hash(&mut h);
            deliverables.hash(&mut h);
            (step, h.finish())
        };
        let mut state = self.citation_gate.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((prev, result)) = &state.last_eval {
            if *prev == key {
                return result.clone();
            }
        }

        let mut report = resolver.verify_text(&answer, ANSWER_SOURCE);
        let mut checked_files = Vec::new();
        for (path, text) in &deliverables {
            let file_report = resolver.verify_text(text, path);
            if file_report.total > 0 {
                checked_files.push(path.clone());
            }
            report.merge(file_report);
        }

        if report.total == 0 && !is_read_only {
            // Nothing cited and not a review/report task: nothing to say.
            state.status = None;
            state.last_eval = Some((key, None));
            return None;
        }

        let problems = report.problem_count();
        let (result, marker) = if problems == 0 {
            (None, None)
        } else if state.last_rejected_step == Some(step) {
            // Same turn, re-evaluated content: same round, no new spend.
            let round = state.rejections;
            (Some(correction_directive(&report, round)), None)
        } else if state.rejections < CITATION_GATE_REJECTION_BOUND {
            state.rejections += 1;
            state.last_rejected_step = Some(step);
            let round = state.rejections;
            (
                Some(correction_directive(&report, round)),
                Some(format!(
                    "{problems} of {} wrong — correction round {round}/{CITATION_GATE_REJECTION_BOUND}",
                    report.total
                )),
            )
        } else {
            tracing::warn!(
                "citation check: {problems} of {} citations still wrong after {} correction \
                 round(s) — stepping aside; the run completes with the count reported",
                report.total,
                state.rejections
            );
            (
                None,
                Some(format!(
                    "{problems} of {} still wrong after {} correction round(s) — completing with this warning",
                    report.total, state.rejections
                )),
            )
        };
        let mut status = GroundingStatus::from_report(&report, state.rejections, checked_files);
        status.read_only = is_read_only;
        // Counts come from the status, never a literal: a same-step
        // re-evaluation of a still-wrong answer has no round marker and used
        // to report "0 wrong" next to "N wrong" (0.8.2 live validation D13).
        let marker = marker.unwrap_or_else(|| {
            format!(
                "{} checked: {} verified, {} without a checkable symbol, {} wrong",
                status.total,
                status.verified,
                status.unverifiable,
                status.problem_count()
            )
        });
        state.status = Some(status.clone());
        state.last_eval = Some((key, result.clone()));
        drop(state);

        crate::output::citation_check(&marker);
        self.emit_progress(super::progress::ProgressEvent::TurnDecision {
            decision: "citation_check".to_string(),
            detail: format!("{} — {}", status.grounding_line(), marker),
        });
        result
    }

    /// Grounding outcome of the latest citation check this task, if any.
    pub fn grounding_status(&self) -> Option<GroundingStatus> {
        self.citation_gate
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .status
            .clone()
    }

    /// Reset the citation gate for a new task.
    pub(super) fn reset_citation_gate(&self) {
        *self.citation_gate.lock().unwrap_or_else(|e| e.into_inner()) =
            CitationGateState::default();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/citation_check/citation_check_test.rs"]
mod tests;
