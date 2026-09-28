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
//! Three tiers, never conflated (AGENTS.md rule 3):
//! - **verified** — the named symbol was found within the cited range;
//! - **location-only** — the file exists and the cited line/range lies inside
//!   it, but nothing next to the citation names content to check, so the
//!   content was NOT checked (a plain `(src/lib.rs:715)` next to prose);
//! - **wrong** — missing file, range outside the file, or the named content
//!   is elsewhere / nowhere in the file.
//!
//! Citations that cannot even be located (ambiguous bare name, unreadable
//! file, outside the workspace/policy) are **not checkable**.
//!
//! A citation followed by an inline code quote ("src/lib.rs:795
//! `self.line_buf[0]`") names content too: the quote (whitespace ignored,
//! `..` / `...` / `…` as wildcards between fragments) must occur, fragments
//! in order, within the cited range (± `LINE_TOLERANCE`) to verify. A quote
//! that is not there is wrong only when it is confidently absent from the
//! whole file (it looks like code and none of its identifiers occur in the
//! file); otherwise the citation stays location-only — a paraphrase or a
//! near miss is not evidence either way (see `QuotePattern`).
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
    /// Inline code quoted right after the citation ("p:1 `code`"), if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
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
    /// The named symbol, or the quoted code, appears within the cited range
    /// (± tolerance).
    Verified { file: String },
    /// The symbol is elsewhere in the file; `actual_line` is where.
    WrongLine { file: String, actual_line: usize },
    /// The symbol does not occur anywhere in the resolved file.
    SymbolNotFound { file: String },
    /// The file exists and the cited range lies inside it, but nothing next
    /// to the citation names content to check: the LOCATION was checked,
    /// the content was not. Never counted as verified.
    LocationVerified { file: String },
    /// No file matches the cited path in the workspace.
    MissingFile,
    /// The cited range lies outside the file (or is malformed).
    OutOfRange { file: String, line_count: usize },
    /// The citation could not be located at all: the path is ambiguous,
    /// unreadable or outside the workspace/policy. Neither confirmed nor
    /// refuted.
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
        let what = match (&c.symbol, &c.quote) {
            (Some(sym), _) => format!("`{sym}` cited at {}", c.display()),
            (None, Some(quote)) => format!("`{}` quoting `{quote}`", c.display()),
            (None, None) => format!("`{}`", c.display()),
        };
        let body = match &self.verdict {
            CitationVerdict::WrongLine { file, actual_line } => {
                format!("{what} but found at {file}:{actual_line}")
            }
            CitationVerdict::SymbolNotFound { file } if c.symbol.is_none() => {
                format!("{what} but none of the quoted code's names occur anywhere in {file}")
            }
            CitationVerdict::SymbolNotFound { file } => {
                format!("{what} but the name does not occur anywhere in {file}")
            }
            CitationVerdict::MissingFile => format!("{what} — no such file in the workspace"),
            CitationVerdict::OutOfRange { file, line_count } => {
                format!("{what} — outside {file}, which has {line_count} lines")
            }
            CitationVerdict::Verified { file } => format!("{what} — verified in {file}"),
            CitationVerdict::LocationVerified { file } => {
                format!("{what} — line exists in {file}, content not checked")
            }
            CitationVerdict::Unverifiable { reason } => {
                format!("{what} — not checkable ({reason})")
            }
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

/// Marker of the informational (ℹ️) note on an uncited, unrequested answer
/// (see `GroundingStatus::info_note`).
pub(crate) const CITATIONS_NOT_CHECKED_INFO: &str = "answer not checked against the files";

/// Source label for citations taken from the final answer text.
pub(crate) const ANSWER_SOURCE: &str = "final answer";

/// Aggregate of every checked citation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CitationReport {
    pub total: usize,
    /// Named content confirmed inside the cited range.
    pub verified: usize,
    /// File and range exist; content not checked (nothing named).
    pub location_verified: usize,
    pub wrong_line: Vec<CheckedCitation>,
    pub symbol_not_found: Vec<CheckedCitation>,
    pub missing_file: Vec<CheckedCitation>,
    pub out_of_range: Vec<CheckedCitation>,
    /// Could not be located: ambiguous, unreadable or outside-policy path.
    pub unverifiable: usize,
}

impl CitationReport {
    fn push(&mut self, checked: CheckedCitation) {
        self.total += 1;
        match &checked.verdict {
            CitationVerdict::Verified { .. } => self.verified += 1,
            CitationVerdict::LocationVerified { .. } => self.location_verified += 1,
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
        self.location_verified += other.location_verified;
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
    /// Named content (symbol or quoted code) confirmed inside the cited
    /// range.
    pub verified: usize,
    /// File exists and the cited range lies inside it, but nothing named
    /// content to check: location checked, content NOT checked.
    pub location_verified: usize,
    /// Could not be located at all (ambiguous, unreadable or outside-policy
    /// path): neither location nor content checked.
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
    /// The answer is a report about THIS workspace's code: a read-only task
    /// that either cites workspace paths or asks about the workspace's code
    /// (`task_policy::task_references_project_code`, the same predicate the
    /// planning-path grounding gate uses). Only such answers are held to
    /// `none_checkable` / `mostly_uncheckable`: an exact-response task
    /// ("Reply with exactly ...") or general Q&A has nothing to cite, and a
    /// missing citation there is not an ungrounded report (0.9.1 live: that
    /// rendered "⚠️ … citations: none checkable"). Not serialized.
    #[serde(skip)]
    pub code_report: bool,
    /// The task asked for citations or a review/audit ("cite file:line",
    /// "review …"): an uncited answer then fails that request (⚠️). When
    /// nobody asked, an uncited answer about the workspace is only an
    /// informational note (ℹ️, `info_note`) — maintainer decision, 0.9.2.
    /// Not serialized.
    #[serde(skip)]
    pub citations_requested: bool,
    /// The gate stepped aside for a limit (`"deadline"` or `"budget"`): the
    /// wrong citations above were accepted WITHOUT a correction round (see
    /// [`super::deadline`]). Serialized only when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_corrected: Option<String>,
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
            location_verified: report.location_verified,
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
            code_report: false,
            citations_requested: false,
            not_corrected: None,
        }
    }

    /// Citations shown to be wrong.
    pub fn problem_count(&self) -> usize {
        self.wrong_line + self.symbol_not_found + self.missing_file + self.out_of_range
    }

    /// Citations that failed even the location check or were shown wrong
    /// (wrong + not checkable). Location-only citations are NOT counted:
    /// their line exists, they are reported separately as location-only.
    pub fn unverified_count(&self) -> usize {
        self.total
            .saturating_sub(self.verified)
            .saturating_sub(self.location_verified)
    }

    /// `citations: N of M could not be verified (W wrong, K not checkable)`
    /// plus `; L location-only (line exists, content not checked)` when any
    /// — the note the banner, run summary, JSON result and failure-mode
    /// evidence carry when a warning applies. N is
    /// [`Self::unverified_count`], so the count and the words agree, and the
    /// breakdown matches [`Self::grounding_line`].
    pub fn unverified_note(&self) -> String {
        let mut note = format!(
            "citations: {} of {} could not be verified{}",
            self.unverified_count(),
            self.total,
            self.unverified_breakdown()
        );
        if self.location_verified > 0 {
            note.push_str(&format!(
                "; {} location-only (line exists, content not checked)",
                self.location_verified
            ));
        }
        note
    }

    /// ` (W wrong, K not checkable)`, parts omitted when zero; empty when
    /// nothing failed.
    fn unverified_breakdown(&self) -> String {
        let mut parts = Vec::new();
        if self.problem_count() > 0 {
            parts.push(format!("{} wrong", self.problem_count()));
        }
        if self.unverifiable > 0 {
            parts.push(format!("{} not checkable", self.unverifiable));
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!(" ({})", parts.join(", "))
        }
    }

    /// Citations that passed at least the location check or were shown
    /// wrong: verified + location-only + wrong. Only not-checkable
    /// citations (ambiguous / unreadable / outside policy) are excluded.
    pub fn checkable_count(&self) -> usize {
        self.verified + self.location_verified + self.problem_count()
    }

    /// A code report (see `code_report`) with no checkable citation at all:
    /// no citations, or none that could even be located. Nothing in
    /// it was checked against the files — not even that a cited line
    /// exists — so it must not read as grounded (0.8.2 live validation D4).
    /// Whether the mid-run `[citations] …` marker line says anything: not
    /// for an answer with no citations that is not a report about this
    /// workspace's code ("hi" printed "0 checked: 0 verified, …"). The
    /// status itself is still recorded and the progress event still sent.
    pub fn marker_is_informative(&self) -> bool {
        self.total > 0 || self.code_report
    }

    pub fn none_checkable(&self) -> bool {
        self.code_report
            && self.checkable_count() == 0
            && (self.citations_requested || self.total > 0)
    }

    /// A code-report answer with no path:line citations when the task did
    /// not ask for any: nothing was checked against the files, which is
    /// stated (ℹ️) but is not a warning (see `citations_requested`).
    pub fn uncited_unrequested(&self) -> bool {
        self.code_report && self.total == 0 && !self.citations_requested
    }

    /// The informational note for `uncited_unrequested`, or `None`.
    pub fn info_note(&self) -> Option<String> {
        self.uncited_unrequested().then(|| {
            format!(
                "{CITATIONS_NOT_CHECKED_INFO}: no path:line citations in the answer (none were requested)"
            )
        })
    }

    /// `citations: none checkable: ...` — see [`Self::none_checkable`].
    pub fn none_checkable_note(&self) -> String {
        let why = if self.total == 0 {
            "no path:line citations in the answer".to_string()
        } else {
            format!(
                "{} of {} not checkable (ambiguous, unreadable or outside-policy path)",
                self.unverifiable, self.total
            )
        };
        format!("{CITATIONS_NONE_CHECKABLE}: {why}")
    }

    /// The warning note the banner, run summary, JSON `grounding.note` and
    /// failure-mode evidence carry, or `None` for a grounded answer: wrong
    /// citations first ([`Self::unverified_note`]), else "none checkable"
    /// for a review/report answer ([`Self::none_checkable_note`]), else the
    /// unverified note when most citations could not even be located
    /// ([`Self::mostly_uncheckable`]).
    pub fn warning_note(&self) -> Option<String> {
        if self.problem_count() > 0 {
            let mut note = self.unverified_note();
            if let Some(word) = &self.not_corrected {
                note.push_str("; citations not corrected: ");
                note.push_str(word);
            }
            Some(note)
        } else if self.none_checkable() {
            Some(self.none_checkable_note())
        } else if self.mostly_uncheckable() {
            Some(self.unverified_note())
        } else {
            None
        }
    }

    /// A review/report answer where MORE citations failed even the location
    /// check (not checkable: ambiguous / unreadable / outside-policy path)
    /// than passed it (verified + location-only). One located citation next
    /// to nineteen that could not be found is not a grounded answer (review,
    /// 0.9.1).
    ///
    /// Location-only citations count on the passing side: their file exists
    /// and the cited line is inside it. The 0.9.1 rule counted them against
    /// the answer and put ⚠️ on an accurate 73-citation architecture
    /// explanation (24 symbol-verified, 49 plain `file:line` next to prose,
    /// 0 wrong). The Grounding line still names them location-only, never
    /// verified. Only code reports (see `code_report`) are held to this
    /// rule, as with [`Self::none_checkable`].
    pub fn mostly_uncheckable(&self) -> bool {
        self.code_report && self.unverifiable > self.verified + self.location_verified
    }

    /// `N checked: V verified, L location-only (line exists, content not
    /// checked), W wrong[, K not checkable]` — the counts the gate marker and
    /// the Grounding line share.
    pub fn counts_line(&self) -> String {
        let mut line = format!(
            "{} checked: {} verified, {} location-only (line exists, content not checked), {} wrong",
            self.total,
            self.verified,
            self.location_verified,
            self.problem_count()
        );
        if self.unverifiable > 0 {
            line.push_str(&format!(", {} not checkable", self.unverifiable));
        }
        line
    }

    /// The summary's "Grounding:" line. Names what was actually checked:
    /// `verified` means the named content was found inside the cited range,
    /// `location-only` means only that the cited line exists, never more
    /// (AGENTS.md rule 3).
    pub fn grounding_line(&self) -> String {
        if self.total == 0 {
            let lead = if self.none_checkable() {
                format!("Grounding: {CITATIONS_NONE_CHECKABLE} — ")
            } else {
                "Grounding: ".to_string()
            };
            return format!(
                "{lead}no path:line citations in the answer (nothing checked against files)"
            );
        }
        format!("Grounding: {}", self.counts_line())
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
        let quote = quote_after(&text[whole.end()..], before.ends_with('`'));
        let key = (path.clone(), start, end, symbol.clone(), quote.clone());
        if !seen.insert(key) {
            continue;
        }
        out.push(Citation {
            path,
            start,
            end,
            symbol,
            quote,
        });
        if out.len() >= MAX_CITATIONS_PER_SOURCE {
            break;
        }
    }
    for c in prose_citations(text) {
        if out.len() >= MAX_CITATIONS_PER_SOURCE {
            break;
        }
        let key = (
            c.path.clone(),
            c.start,
            c.end,
            c.symbol.clone(),
            c.quote.clone(),
        );
        if seen.insert(key) {
            out.push(c);
        }
    }
    out
}

/// Byte ranges of the `path:line` / `path:a-b` / `path#Lnn` citations in
/// `text` (the whole token, path through line numbers) with the cited path,
/// in order. Same token rules as [`parse_citations`] (a path glued to a URL
/// or a longer identifier is not a citation); used by the terminal renderer
/// to turn citations into clickable links.
pub(crate) fn citation_spans(text: &str) -> Vec<(std::ops::Range<usize>, String)> {
    let mut out = Vec::new();
    for caps in citation_regex().captures_iter(text) {
        let (Some(m), Some(whole)) = (caps.name("path"), caps.get(0)) else {
            continue;
        };
        let before = &text[..m.start()];
        if before.ends_with("//")
            || before.ends_with(':')
            || before.chars().next_back().is_some_and(is_path_char)
        {
            continue;
        }
        if text[whole.end()..]
            .chars()
            .next()
            .is_some_and(|c| is_ident_char(c) && !c.is_ascii_digit())
        {
            continue;
        }
        out.push((
            m.start()..whole.end(),
            m.as_str().trim_start_matches("./").to_string(),
        ));
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
            quote: None,
        });
    }
    out
}

/// The inline code span quoted right after a citation, on the same line:
/// "p:12 `code`", "p:12: `code`", "p:12 — `code`", "**p:12** `code`",
/// "`p:12` `code`". `in_span` says the citation itself sits in a code span,
/// whose closing backtick comes first. A span that is itself a citation, or
/// anything else between the citation and the span (";", ")", a word), is
/// not a quote of the cited line.
fn quote_after(after: &str, in_span: bool) -> Option<String> {
    let line = &after[..after.find('\n').unwrap_or(after.len())];
    let mut rest = line;
    if in_span {
        rest = rest.strip_prefix('`')?;
    }
    rest = rest.trim_start_matches('*').trim_start();
    for sep in [":", "—", "–", "-", "→", "=>"] {
        if let Some(r) = rest.strip_prefix(sep) {
            rest = r.trim_start();
            break;
        }
    }
    let inner = rest.strip_prefix('`')?;
    if inner.starts_with('`') {
        return None;
    }
    let close = inner.find('`')?;
    let quote = inner[..close].trim();
    if quote.is_empty() || citation_regex().is_match(quote) {
        return None;
    }
    Some(quote.to_string())
}

/// A code quote prepared for matching (see the module docs): whitespace
/// removed, split into ordered fragments at `..` / `...` / `…`.
struct QuotePattern {
    fragments: Vec<String>,
    /// Identifiers (3+ chars, not starting with a digit) in the quote.
    identifiers: Vec<String>,
    /// Shaped like code (call, path, index, assignment, snake_case ...),
    /// not a flag or a prose phrase — required before an absent quote is
    /// judged wrong.
    code_shaped: bool,
}

impl QuotePattern {
    /// `None` when the quote is too weak to check (fewer than 3 identifier
    /// characters): `x`, `0`, `->` confirm nothing.
    fn new(quote: &str) -> Option<Self> {
        let squashed: String = quote.chars().filter(|c| !c.is_whitespace()).collect();
        let normalized = squashed.replace('…', "..").replace("...", "..");
        let fragments: Vec<String> = normalized
            .split("..")
            .filter(|f| !f.is_empty())
            .map(str::to_string)
            .collect();
        let ident_chars = fragments
            .iter()
            .flat_map(|f| f.chars())
            .filter(|c| is_ident_char(*c))
            .count();
        if ident_chars < 3 {
            return None;
        }
        let identifiers: Vec<String> = quote
            .split(|c: char| !is_ident_char(c))
            .filter(|w| w.len() >= 3 && !w.starts_with(|c: char| c.is_ascii_digit()))
            .map(str::to_string)
            .collect();
        let code_shaped = quote.contains(['(', '[', '{', '=', ';', '.', '<'])
            || quote.contains("::")
            || identifiers.iter().any(|w| w.contains('_'));
        Some(QuotePattern {
            fragments,
            identifiers,
            code_shaped,
        })
    }

    /// The fragments occur, in order, in `hay` (whitespace already removed).
    fn matches(&self, hay: &str) -> bool {
        let mut from = 0;
        for frag in &self.fragments {
            match hay[from..].find(frag.as_str()) {
                Some(at) => from += at + frag.len(),
                None => return false,
            }
        }
        true
    }

    /// Whether the quote confidently does not come from `lines`: it is
    /// code-shaped, names at least one 4+ character identifier, and none
    /// of its identifiers occurs anywhere in the file.
    fn absent_from(&self, lines: &[String]) -> bool {
        self.code_shaped
            && self.identifiers.iter().any(|w| w.len() >= 4)
            && !self
                .identifiers
                .iter()
                .any(|w| lines.iter().any(|l| contains_word(l, w)))
    }
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
/// `OUTSIDE_POLICY_REASON` and is never read; it is never reported as a
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
        let lo = c.start.saturating_sub(LINE_TOLERANCE).max(1);
        let hi = (c.end + LINE_TOLERANCE).min(line_count);
        let quote = c.quote.as_deref().and_then(QuotePattern::new);
        let quote_in_range = || {
            quote.as_ref().is_some_and(|q| {
                let window: String = lines[lo - 1..hi]
                    .iter()
                    .flat_map(|l| l.chars())
                    .filter(|ch| !ch.is_whitespace())
                    .collect();
                q.matches(&window)
            })
        };
        if let Some(sym) = &c.symbol {
            if (lo..=hi).any(|n| contains_word(&lines[n - 1], sym)) || quote_in_range() {
                return CitationVerdict::Verified { file };
            }
            return match locate_symbol(lines, sym) {
                Some(actual_line) => CitationVerdict::WrongLine { file, actual_line },
                None => CitationVerdict::SymbolNotFound { file },
            };
        }
        match &quote {
            Some(_) if quote_in_range() => CitationVerdict::Verified { file },
            Some(q) if q.absent_from(lines) => CitationVerdict::SymbolNotFound { file },
            // The line exists; nothing checkable names content there (or the
            // quote is a paraphrase / near miss — not evidence either way).
            _ => CitationVerdict::LocationVerified { file },
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

/// The line (1-based) of `lines` that a citation's named content anchors
/// to: the line within the cited range (± [`LINE_TOLERANCE`]) carrying the
/// named symbol, nearest the cited start line (the cited line itself when it
/// carries it); for a quote-only citation, the cited start line when the
/// quote occurs in that window. `None` when nothing named anchors there (or
/// the range lies outside `lines`).
pub(crate) fn citation_anchor(lines: &[String], c: &Citation) -> Option<usize> {
    if c.start == 0 || c.end < c.start || c.end > lines.len() {
        return None;
    }
    let lo = c.start.saturating_sub(LINE_TOLERANCE).max(1);
    let hi = (c.end + LINE_TOLERANCE).min(lines.len());
    if let Some(sym) = &c.symbol {
        return (lo..=hi)
            .filter(|&n| contains_word(&lines[n - 1], sym))
            .min_by_key(|&n| (n.abs_diff(c.start), n));
    }
    let quote = c.quote.as_deref().and_then(QuotePattern::new)?;
    let window: String = lines[lo - 1..hi]
        .iter()
        .flat_map(|l| l.chars())
        .filter(|ch| !ch.is_whitespace())
        .collect();
    quote.matches(&window).then_some(c.start)
}

/// For every line of `old` (0-based) that an edit left unchanged, its index
/// in `new`; `None` for lines the edit changed or deleted. Computed from a
/// line diff (Myers, bounded by a deadline: a diff cut short maps fewer
/// lines, never a wrong one).
pub(crate) fn unchanged_line_map(old: &[String], new: &[String]) -> Vec<Option<usize>> {
    let mut map = vec![None; old.len()];
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(250);
    for op in
        similar::capture_diff_slices_deadline(similar::Algorithm::Myers, old, new, Some(deadline))
    {
        if let similar::DiffOp::Equal {
            old_index,
            new_index,
            len,
        } = op
        {
            for k in 0..len {
                map[old_index + k] = Some(new_index + k);
            }
        }
    }
    map
}

/// Where a citation's cited range moved to after edits, or `None` when it
/// did not move (or cannot be mapped exactly).
///
/// `versions` are earlier contents of the cited file this task (oldest
/// first: each the file as it was right before an edit), `current` the file
/// now. A citation that anchors exactly on its cited line now is current.
/// Otherwise the newest earlier version where it anchored EXACTLY is the
/// one it was written against (a version where it only anchors within the
/// tolerance is used when none is exact); the anchor line is carried
/// through the diff from that version to `current`, and the citation moves
/// by the same offset. A cited line the edit itself changed or deleted
/// maps to nothing and is not reported (the completion gate still checks
/// it). A citation that anchors in no version was never right and is left
/// to the completion gate.
pub(crate) fn moved_citation_range(
    c: &Citation,
    versions: &[&[String]],
    current: &[String],
) -> Option<(usize, usize)> {
    if citation_anchor(current, c) == Some(c.start) {
        return None;
    }
    let exact = versions.iter().rev().find_map(|v| {
        citation_anchor(v, c)
            .filter(|&a| a == c.start)
            .map(|a| (v, a))
    });
    let (version, anchor) = exact.or_else(|| {
        versions
            .iter()
            .rev()
            .find_map(|v| citation_anchor(v, c).map(|a| (v, a)))
    })?;
    let moved_to = unchanged_line_map(version, current)
        .get(anchor - 1)
        .copied()
        .flatten()?
        + 1;
    if moved_to == anchor {
        return None;
    }
    let delta = moved_to as isize - anchor as isize;
    let start = usize::try_from(c.start as isize + delta).ok()?;
    let end = usize::try_from(c.end as isize + delta).ok()?;
    (start >= 1 && end <= current.len()).then_some((start, end))
}

/// A citation in a file the agent wrote whose cited lines an edit moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MovedCitation {
    /// The written file carrying the citation (`docs/NOTES.md`).
    pub source: String,
    pub citation: Citation,
    /// The cited file, workspace-relative.
    pub file: String,
    pub new_start: usize,
    pub new_end: usize,
}

impl MovedCitation {
    /// "docs/NOTES.md cites src/a.rs:96 `name` — now at :100 after your edit"
    pub(crate) fn describe(&self) -> String {
        let c = &self.citation;
        let what = match (&c.symbol, &c.quote) {
            (Some(sym), _) => format!(" `{sym}`"),
            (None, Some(quote)) => format!(" `{quote}`"),
            (None, None) => String::new(),
        };
        let (old, new) = if c.start == c.end {
            (format!("{}", c.start), format!(":{}", self.new_start))
        } else {
            (
                format!("{}-{}", c.start, c.end),
                format!(":{}-{}", self.new_start, self.new_end),
            )
        };
        format!(
            "{} cites {}:{old}{what} — now at {new} after your edit",
            self.source, self.file
        )
    }
}

impl CitationResolver {
    /// The lines of the file a citation names, with its canonical path and
    /// workspace-relative display, under the same confinement as a check.
    /// `None` when the path is missing, ambiguous, refused or unreadable.
    /// `context` is the text the citation came from (full paths named there
    /// disambiguate a bare file name, as in [`Self::verify_text`]).
    pub(crate) fn cited_file(
        &mut self,
        context: &str,
        cited: &str,
    ) -> Option<(PathBuf, String, Vec<String>)> {
        self.mentioned = mentioned_paths(context);
        let Resolution::Found(path) = self.resolve(cited) else {
            return None;
        };
        let file = self.relative_display(&path);
        let lines = match self.lines(&path) {
            Loaded::Lines(lines) => lines.clone(),
            Loaded::Refused | Loaded::Unreadable => return None,
        };
        let canonical = std::fs::canonicalize(&path).unwrap_or(path);
        Some((canonical, file, lines))
    }
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
         Fix each citation (re-read the file if you are unsure of the line: ranged reads \
         return numbered lines (N<TAB>code; the number is metadata, not file content) — take \
         the number from a ranged read, or pass line_numbers: true; do not count lines) or remove it, and \
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
    /// The latest answer the gate rejected, with its grounding outcome and
    /// the mutation sequence it was judged at; cleared when a later answer
    /// is accepted. The deadline path accepts it
    /// when a correction round can no longer fit, and the timeout partial
    /// carries it (see [`super::deadline`]).
    pub rejected_draft: Option<RejectedDraft>,
}

/// An answer the citation gate rejected (see [`CitationGateState`]).
#[derive(Debug, Clone)]
pub(crate) struct RejectedDraft {
    /// The answer text as judged (think blocks stripped).
    pub text: String,
    /// Grounding outcome of that evaluation.
    pub status: GroundingStatus,
    /// `Agent::mutation_sequence` at the rejection: a draft judged before
    /// later edits no longer describes the tree and is never accepted.
    pub mutation_sequence: usize,
}

impl super::Agent {
    /// The answer text the completion gate is judging: the latest assistant
    /// message (pushed to history before any gate probe runs), falling back
    /// to `last_assistant_response`.
    pub(super) fn citation_candidate_answer(&self) -> String {
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
    pub(super) fn written_deliverables(&self) -> Vec<String> {
        let Some(cp) = self.current_checkpoint.as_ref() else {
            return Vec::new();
        };
        let mut out: Vec<String> = Vec::new();
        for call in cp.tool_calls.iter().filter(|c| c.success) {
            // Same write-tool set and path extraction as every other write
            // consumer (patch_apply embeds its targets in the diff).
            if !super::tool_dispatch::helpers::tool_call_writes_file(&call.tool_name) {
                continue;
            }
            let Ok(args) = serde_json::from_str::<serde_json::Value>(&call.arguments) else {
                continue;
            };
            let paths: Vec<String> =
                super::tool_dispatch::helpers::written_paths_for_tool_call(&call.tool_name, &args)
                    .into_iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
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
            // This answer supersedes any draft rejected earlier.
            state.rejected_draft = None;
            state.status = None;
            state.last_eval = Some((key, None));
            return None;
        }

        let problems = report.problem_count();
        // Limit step-aside: inside the deadline or budget reserve (or when
        // one more turn no longer fits), a correction round would end in a
        // TIMEOUT / BUDGET_EXHAUSTED with no answer at all. Accept this draft
        // with the wrong count and the "citations not corrected: deadline" /
        // "…: budget" note instead (rule 3).
        let limit_step_aside = if problems > 0 && state.last_rejected_step != Some(step) {
            self.completion_gate_step_aside()
        } else {
            None
        };
        let (result, marker) = if problems == 0 {
            (None, None)
        } else if state.last_rejected_step == Some(step) {
            // Same turn, re-evaluated content: same round, no new spend.
            let round = state.rejections;
            (Some(correction_directive(&report, round)), None)
        } else if let Some(why) = &limit_step_aside {
            tracing::warn!(
                "citation check: {problems} of {} citations wrong, but a correction round no \
                 longer fits ({why}) — accepting the draft with the count reported",
                report.total
            );
            (
                None,
                Some(format!(
                    "{problems} of {} wrong — {} ({}); completing with this warning",
                    report.total,
                    super::deadline::citations_not_corrected_note(why.cause),
                    why.detail
                )),
            )
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
        // Held to the report standard only when the task is a report about
        // this workspace's code (see `GroundingStatus::code_report`).
        status.citations_requested =
            super::task_policy::task_requests_citations(self.task_context_for_classification());
        status.code_report = is_read_only
            && (report.total > 0
                || super::task_policy::task_is_code_review(self.task_context_for_classification())
                || {
                    let project_name = root
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default();
                    super::task_policy::task_references_project_code(
                        self.task_context_for_classification(),
                        project_name,
                    )
                });
        status.not_corrected = limit_step_aside
            .as_ref()
            .map(|w| w.cause.note_word().to_string());
        // The kept draft is always the LATEST judged answer: a rejection
        // replaces it, and an accepted answer (clean, or accepted with the
        // count reported) retires it — otherwise the limit path and the
        // timeout partial delivered a stale rejected v1 over an accepted v2.
        state.rejected_draft = result.is_some().then(|| RejectedDraft {
            text: answer.clone(),
            status: status.clone(),
            mutation_sequence: self.mutation_sequence,
        });
        // Counts come from the status, never a literal: a same-step
        // re-evaluation of a still-wrong answer has no round marker and used
        // to report "0 wrong" next to "N wrong" (0.8.2 live validation D13).
        let marker = marker.unwrap_or_else(|| status.counts_line());
        state.status = Some(status.clone());
        state.last_eval = Some((key, result.clone()));
        drop(state);

        if status.marker_is_informative() {
            crate::output::citation_check(&marker);
        }
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
