//! Review coverage: the repository inventory at review start, a per-file
//! line-coverage ledger, recorded findings, and the review completion gate.
//!
//! Live failure this answers (0.9.3): "can you review the selfware core do
//! not code" read `src/lib.rs`, `Cargo.toml` and `src/main.rs` — 3 of ~480
//! files — and a confident architecture "review" with 0 citations was
//! accepted as `✅ Completed`. A review must first know what the repository
//! contains, then read it, and say honestly how much it read.
//!
//! - **Start** (`Agent::begin_review_session`): for a task
//!   `task_policy::task_is_code_review` classifies as a review, the
//!   deterministic [`RepoInventory`] is built, shown to the user, and a
//!   compact, token-measured version (the scope and reading plan) is injected
//!   into the model's context.
//! - **Coverage**: every `file_read` result actually delivered to the model
//!   (after chunking; spilled summaries, outlines and "unchanged" notes do
//!   not count) adds its line range. Coverage lives on the agent, not in the
//!   message history, so compaction cannot erase it; it is persisted in the
//!   checkpoint's guard counters for resume.
//! - **Findings**: `FINDING: path:line — …` lines the model writes (and the
//!   cited lines of a refused draft) are kept here and restated every turn,
//!   so compaction cannot lose them either.
//! - **Gate** (`Agent::review_coverage_gate`): a final answer is refused
//!   while relevant files remain unread, naming the next ones in plan order.
//!   Bounded: two refusals without new coverage, the iteration reserve, or
//!   the deadline/budget step-aside end the gating, and the run then reports
//!   PARTIAL coverage — never a completed review.
//! - **Phase** ([`Agent::review_phase`]): `Reading` while files remain,
//!   `Synthesis` once the next answer is the one that counts — the signal a
//!   per-phase reasoning-effort policy keys on.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;

use super::task_policy::{policy_envelope, PolicyKind};
use super::Agent;
use crate::analysis::repo_inventory::{group, human_bytes, PlanEntry, RepoInventory};
use crate::api::types::Message;
use crate::token_count::estimate_content_tokens;

/// Refusals in a row that brought no new coverage before the gate steps
/// aside (the model keeps answering instead of reading).
pub(crate) const REVIEW_GATE_NO_PROGRESS_BOUND: usize = 2;

/// Iterations kept for the final answer: with this few left the gate stops
/// refusing and the turn note asks for the answer now.
pub(crate) const REVIEW_ITERATION_RESERVE: usize = 3;

/// Evidence / banner marker of a review that ended with files unread.
pub(crate) const REVIEW_COVERAGE_PARTIAL_NOTE: &str = "review coverage: PARTIAL";

/// Measured cap of the inventory context note injected at review start.
const INVENTORY_CONTEXT_TOKENS: usize = 1_500;

/// Unread files named per gate refusal / turn note.
const NEXT_FILES_SHOWN: usize = 8;

const MAX_FINDINGS: usize = 400;
const FINDING_MAX_CHARS: usize = 300;

/// Which answer the next model turn is expected to produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewPhase {
    /// Relevant files remain unread: the turn should read.
    Reading,
    /// Coverage is complete, or the budget/gate bound ended the reading: the
    /// next turn writes the final review.
    Synthesis,
}

impl ReviewPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            ReviewPhase::Reading => "reading",
            ReviewPhase::Synthesis => "synthesis",
        }
    }
}

/// The run's review coverage, for the run summary and the JSON result.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct ReviewCoverageReport {
    pub scope: String,
    /// One-line inventory header ("37 files, 71,342 lines, 3.7 MB; …").
    pub inventory: String,
    pub relevant_files: usize,
    /// Relevant files whose every line was delivered by `file_read`.
    pub read_files: usize,
    pub partially_read_files: usize,
    pub relevant_lines: usize,
    pub read_lines: usize,
    /// Floor of read/relevant lines; never 100 unless complete.
    pub percent_lines: usize,
    pub complete: bool,
    /// Unread / partially read files in reading-plan order (first 20).
    pub not_read: Vec<String>,
    pub not_read_count: usize,
    pub findings_recorded: usize,
    /// Why reading stopped short, when it did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stopped: Option<String>,
    /// The summary line (`coverage: …`).
    pub line: String,
}

/// The per-task review state held by the agent.
#[derive(Debug, Default)]
pub(crate) struct ReviewState {
    session: Option<ReviewSession>,
    /// Coverage + findings restored from a checkpoint, applied when the
    /// resumed task rebuilds its session.
    pending_restore: Option<serde_json::Value>,
}

#[derive(Debug)]
pub(crate) struct ReviewSession {
    /// Canonical root the inventory was taken at.
    root: PathBuf,
    inventory_line: String,
    scope_label: String,
    plan: Vec<PlanEntry>,
    relevant_lines: usize,
    /// Relevant path → merged, 1-based inclusive line ranges delivered.
    coverage: HashMap<String, Vec<(usize, usize)>>,
    findings: Vec<String>,
    finding_keys: HashSet<String>,
    seen_messages: HashSet<u64>,
    refusals: usize,
    no_progress_refusals: usize,
    covered_at_last_refusal: Option<usize>,
    /// (step, answer fingerprint) → decision: repeated gate evaluations of
    /// the same answer in one step are one refusal, not several.
    last_eval: Option<((usize, u64), Option<String>)>,
    citation_nudged: bool,
    stopped: Option<String>,
    announced_phase: Option<ReviewPhase>,
}

static FINDING_LINE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)^\s*(?:[-*•]\s*)?(?:\*\*)?finding(?:\*\*)?\s*(?:\d+\s*)?[:\-–—]\s*(?:\*\*)?\s*(.+)$",
    )
    .expect("valid regex")
});
static CITATION: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:[A-Za-z0-9_.\-]+/)*[A-Za-z0-9_\-]+\.[A-Za-z0-9]{1,6}:\d+").expect("valid regex")
});

fn fnv(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Merge `range` into a sorted, non-overlapping, 1-based inclusive list.
fn merge_range(ranges: &mut Vec<(usize, usize)>, range: (usize, usize)) {
    let (a, b) = if range.0 <= range.1 {
        range
    } else {
        (range.1, range.0)
    };
    ranges.push((a.max(1), b.max(1)));
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for (s, e) in ranges.drain(..) {
        match merged.last_mut() {
            Some(last) if s <= last.1.saturating_add(1) => last.1 = last.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    *ranges = merged;
}

/// Lines of `1..=total` covered by `ranges`.
fn covered_within(ranges: &[(usize, usize)], total: usize) -> usize {
    ranges
        .iter()
        .map(|&(s, e)| {
            let e = e.min(total);
            if s > e {
                0
            } else {
                e - s + 1
            }
        })
        .sum()
}

/// The line range a `file_read` result DELIVERED, from the payload the
/// model received: a chunked whole read's `shown_line_range`, a ranged
/// read's start plus `lines_returned`, or a whole read's `total_lines`.
/// `None` for payloads that carry no file content (outline / unchanged
/// notes, spilled summaries, errors) and for empty files.
pub(crate) fn delivered_range(args: &serde_json::Value, payload: &str) -> Option<(usize, usize)> {
    let v: serde_json::Value = serde_json::from_str(payload).ok()?;
    if v.get(super::context::UNCHANGED_REREAD_NOTE_KEY).is_some()
        || v.get(super::result_compaction::OUTLINE_REREAD_KEY)
            .is_some()
        || v.get("content").is_none()
    {
        return None;
    }
    let pair = |value: &serde_json::Value| -> Option<(usize, usize)> {
        let arr = value.as_array()?;
        Some((
            arr.first()?.as_u64()? as usize,
            arr.get(1)?.as_u64()? as usize,
        ))
    };
    if let Some(shown) = v.get("shown_line_range").and_then(pair) {
        return Some(shown);
    }
    if let Some((start, _)) = args
        .get("line_range")
        .filter(|r| !r.is_null())
        .and_then(pair)
    {
        let returned = v.get("lines_returned").and_then(|n| n.as_u64())? as usize;
        return (returned > 0).then(|| (start.max(1), start.max(1) + returned - 1));
    }
    let total = v.get("total_lines").and_then(|n| n.as_u64())? as usize;
    (total > 0).then_some((1, total))
}

impl ReviewSession {
    pub(crate) fn new(
        inventory: &RepoInventory,
        plan: crate::analysis::repo_inventory::ReviewPlan,
    ) -> Self {
        let plan_entries: Vec<PlanEntry> = plan.plan.into_iter().filter(|e| e.lines > 0).collect();
        let relevant_lines = plan_entries.iter().map(|e| e.lines).sum();
        let inventory_line = format!(
            "{} files ({} code), {} lines, {}; scope: {} → {} relevant files, {} lines",
            group(inventory.totals.files),
            group(inventory.code.files),
            group(inventory.totals.lines),
            human_bytes(inventory.totals.bytes),
            plan.scope.label,
            group(plan_entries.len()),
            group(relevant_lines),
        );
        Self {
            root: PathBuf::from(&inventory.root),
            inventory_line,
            scope_label: plan.scope.label,
            plan: plan_entries,
            relevant_lines,
            coverage: HashMap::new(),
            findings: Vec::new(),
            finding_keys: HashSet::new(),
            seen_messages: HashSet::new(),
            refusals: 0,
            no_progress_refusals: 0,
            covered_at_last_refusal: None,
            last_eval: None,
            citation_nudged: false,
            stopped: None,
            announced_phase: None,
        }
    }

    fn entry(&self, rel: &str) -> Option<&PlanEntry> {
        self.plan.iter().find(|e| e.path == rel)
    }

    /// Record a delivered range of `rel` (repository-relative); ignored for
    /// files outside the review scope.
    pub(crate) fn record(&mut self, rel: &str, range: (usize, usize)) {
        if self.entry(rel).is_none() {
            return;
        }
        merge_range(self.coverage.entry(rel.to_string()).or_default(), range);
    }

    fn covered(&self, e: &PlanEntry) -> usize {
        self.coverage
            .get(&e.path)
            .map(|r| covered_within(r, e.lines))
            .unwrap_or(0)
    }

    pub(crate) fn covered_lines(&self) -> usize {
        self.plan.iter().map(|e| self.covered(e)).sum()
    }

    pub(crate) fn complete(&self) -> bool {
        self.plan.iter().all(|e| self.covered(e) >= e.lines)
    }

    /// Unread or partially read files in plan order, with where to go on.
    fn next_unread(&self, n: usize) -> Vec<String> {
        self.plan
            .iter()
            .filter(|e| self.covered(e) < e.lines)
            .take(n)
            .map(|e| match self.coverage.get(&e.path) {
                Some(ranges) if !ranges.is_empty() => {
                    let first_gap = first_gap(ranges, e.lines);
                    format!(
                        "{} — {} of {} lines read; continue at line {}",
                        e.path,
                        self.covered(e),
                        e.lines,
                        first_gap
                    )
                }
                _ => format!("{} ({} lines)", e.path, e.lines),
            })
            .collect()
    }

    pub(crate) fn report(&self) -> ReviewCoverageReport {
        let read_files = self
            .plan
            .iter()
            .filter(|e| self.covered(e) >= e.lines)
            .count();
        let partially = self
            .plan
            .iter()
            .filter(|e| {
                let c = self.covered(e);
                c > 0 && c < e.lines
            })
            .count();
        let read_lines = self.covered_lines();
        let complete = read_files == self.plan.len();
        let mut percent = (read_lines * 100)
            .checked_div(self.relevant_lines)
            .unwrap_or(100);
        if !complete && percent >= 100 {
            percent = 99;
        }
        let unread: Vec<&PlanEntry> = self
            .plan
            .iter()
            .filter(|e| self.covered(e) < e.lines)
            .collect();
        let not_read: Vec<String> = unread.iter().take(20).map(|e| e.path.clone()).collect();
        let line = if complete {
            format!(
                "coverage: read all {} of {} relevant files (100% of {} relevant lines) — complete",
                group(read_files),
                group(self.plan.len()),
                group(self.relevant_lines)
            )
        } else {
            let shown: Vec<&str> = unread.iter().take(5).map(|e| e.path.as_str()).collect();
            let more = unread.len().saturating_sub(shown.len());
            format!(
                "⚠️ coverage: PARTIAL — read {} of {} relevant files ({}% of {} relevant lines{}); not read: {}{}{}",
                group(read_files),
                group(self.plan.len()),
                percent,
                group(self.relevant_lines),
                if partially > 0 {
                    format!(", {partially} partially")
                } else {
                    String::new()
                },
                shown.join(", "),
                if more > 0 {
                    format!(", … (+{more})")
                } else {
                    String::new()
                },
                self.stopped
                    .as_ref()
                    .map(|s| format!("; stopped: {s}"))
                    .unwrap_or_default()
            )
        };
        ReviewCoverageReport {
            scope: self.scope_label.clone(),
            inventory: self.inventory_line.clone(),
            relevant_files: self.plan.len(),
            read_files,
            partially_read_files: partially,
            relevant_lines: self.relevant_lines,
            read_lines,
            percent_lines: percent,
            complete,
            not_read,
            not_read_count: unread.len(),
            findings_recorded: self.findings.len(),
            stopped: self.stopped.clone(),
            line,
        }
    }

    /// Absorb findings from model text: explicit `FINDING:` lines always;
    /// with `draft`, also every line that cites `path:line` (a refused
    /// final answer is otherwise lost to the next compaction).
    pub(crate) fn absorb_text(&mut self, text: &str, draft: bool) {
        for line in text.lines() {
            let finding = if let Some(c) = FINDING_LINE.captures(line) {
                Some(c[1].trim().to_string())
            } else if draft && CITATION.is_match(line) {
                Some(
                    line.trim()
                        .trim_start_matches(['-', '*', '•', ' '])
                        .trim()
                        .to_string(),
                )
            } else {
                None
            };
            let Some(finding) = finding.filter(|f| f.chars().count() >= 12) else {
                continue;
            };
            let finding: String = if finding.chars().count() > FINDING_MAX_CHARS {
                let cut: String = finding.chars().take(FINDING_MAX_CHARS).collect();
                format!("{cut}…")
            } else {
                finding
            };
            let key = finding
                .to_lowercase()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if self.findings.len() < MAX_FINDINGS && self.finding_keys.insert(key) {
                self.findings.push(finding);
            }
        }
    }

    /// The findings, newest last, within `budget` tokens (oldest elided with
    /// a count when they do not fit).
    fn render_findings(&self, budget: usize) -> String {
        let mut lines: Vec<String> = Vec::new();
        let mut used = 0;
        for f in self.findings.iter().rev() {
            let line = format!("- {f}");
            let cost = estimate_content_tokens(&line) + 1;
            if used + cost > budget {
                break;
            }
            used += cost;
            lines.push(line);
        }
        let elided = self.findings.len() - lines.len();
        lines.reverse();
        if elided > 0 {
            lines.insert(
                0,
                format!(
                    "- (+{elided} earlier findings not shown here for space — they are recorded)"
                ),
            );
        }
        lines.join("\n")
    }

    /// The per-turn status note (reading or synthesis), within `budget`
    /// tokens (measured).
    pub(crate) fn turn_note(
        &self,
        phase: ReviewPhase,
        wrap_up: Option<&str>,
        budget: usize,
    ) -> String {
        let report = self.report();
        let mut out = vec!["<review_status>".to_string()];
        match phase {
            ReviewPhase::Reading => {
                out.push(format!(
                    "Review coverage so far: read {} of {} relevant files ({}% of {} relevant lines). Scope: {}.",
                    report.read_files,
                    report.relevant_files,
                    report.percent_lines,
                    group(report.relevant_lines),
                    self.scope_label
                ));
                out.push(
                    "Read next, in reading-plan order (file_read; several per turn is fine; use \
                     line_range for files over ~600 lines — outlines and grep hits do not count as read):"
                        .to_string(),
                );
                for next in self.next_unread(NEXT_FILES_SHOWN) {
                    out.push(format!("- {next}"));
                }
                if report.not_read_count > NEXT_FILES_SHOWN {
                    out.push(format!(
                        "- … +{} more",
                        report.not_read_count - NEXT_FILES_SHOWN
                    ));
                }
                out.push(
                    "Record each finding as soon as you see it, one per line in your message: \
                     `FINDING: path:line — what is wrong and why`. Recorded findings are kept across \
                     context compaction; unrecorded ones may be lost."
                        .to_string(),
                );
            }
            ReviewPhase::Synthesis => {
                if report.complete {
                    out.push(format!(
                        "Coverage complete: all {} relevant files read ({} lines). Write the final review now.",
                        report.relevant_files,
                        group(report.relevant_lines)
                    ));
                } else {
                    out.push(format!(
                        "Reading has ended{}: {} of {} relevant files read ({}% of lines). Write the final \
                         review now from what you read, and state plainly that coverage is partial and \
                         which areas were not read.",
                        wrap_up
                            .or(self.stopped.as_deref())
                            .map(|w| format!(" ({w})"))
                            .unwrap_or_default(),
                        report.read_files,
                        report.relevant_files,
                        report.percent_lines
                    ));
                }
                out.push(
                    "Every finding needs a path:line citation to a line you read. Group by severity; \
                     no findings in an area is a valid result — say so."
                        .to_string(),
                );
            }
        }
        let header = out.join("\n");
        let close = "</review_status>";
        let room = budget
            .saturating_sub(estimate_content_tokens(&header) + estimate_content_tokens(close) + 16);
        if self.findings.is_empty() {
            return format!("{header}\nFindings recorded so far: none.\n{close}");
        }
        format!(
            "{header}\nFindings recorded so far ({}):\n{}\n{close}",
            self.findings.len(),
            self.render_findings(room)
        )
    }

    /// Decide on a final answer (see [`Agent::review_coverage_gate`]).
    /// `limit` is why the budget no longer allows another reading round.
    pub(crate) fn gate(
        &mut self,
        step: usize,
        answer: &str,
        limit: Option<String>,
    ) -> Option<String> {
        let key = (step, fnv(answer));
        if let Some((prev, result)) = &self.last_eval {
            if *prev == key {
                return result.clone();
            }
        }
        let result = self.decide(answer, limit);
        self.last_eval = Some((key, result.clone()));
        result
    }

    fn decide(&mut self, answer: &str, limit: Option<String>) -> Option<String> {
        self.absorb_text(answer, true);
        if let Some(why) = limit {
            // The budget always wins: no further round, of any kind.
            if !self.complete() && self.stopped.is_none() {
                self.stopped = Some(why);
            }
            return None;
        }
        let covered = self.covered_lines();
        if !self.complete() && self.stopped.is_none() {
            if self
                .covered_at_last_refusal
                .is_some_and(|before| covered <= before)
            {
                self.no_progress_refusals += 1;
            } else {
                self.no_progress_refusals = 0;
            }
            if self.no_progress_refusals >= REVIEW_GATE_NO_PROGRESS_BOUND {
                self.stopped = Some(format!(
                    "the model answered {} times in a row without reading further",
                    self.no_progress_refusals + 1
                ));
            }
        }
        if self.complete() || self.stopped.is_some() {
            // Reading is over. A review answer with no `path:line` at all is
            // refused ONCE (the citation gate alone would only warn about a
            // code report without citations).
            if !self.citation_nudged && !CITATION.is_match(answer) {
                self.citation_nudged = true;
                return Some(policy_envelope(
                    PolicyKind::Gate,
                    true,
                    "review answer cites no path:line",
                    "This review cites no `path:line`. Give the final review again with a \
                     `path:line` citation for each finding (to lines you read). If you found no \
                     problems in an area, say so explicitly.",
                ));
            }
            return None;
        }
        self.refusals += 1;
        self.covered_at_last_refusal = Some(covered);
        let report = self.report();
        let mut body = vec![format!(
            "REVIEW COVERAGE INCOMPLETE — this is not accepted as the final review yet. You have \
             read {} of {} relevant files ({}% of {} relevant lines; scope: {}). A review must read \
             every relevant file before it concludes.",
            report.read_files,
            report.relevant_files,
            report.percent_lines,
            group(report.relevant_lines),
            self.scope_label
        )];
        body.push("Read next, in reading-plan order, with file_read:".to_string());
        for next in self.next_unread(NEXT_FILES_SHOWN) {
            body.push(format!("- {next}"));
        }
        if report.not_read_count > NEXT_FILES_SHOWN {
            body.push(format!(
                "- … +{} more",
                report.not_read_count - NEXT_FILES_SHOWN
            ));
        }
        body.push(format!(
            "Your draft's cited findings are recorded ({} so far) and restated every turn; keep \
             adding `FINDING: path:line — …` lines as you read. Answer again when the plan is read.",
            self.findings.len()
        ));
        Some(policy_envelope(
            PolicyKind::Gate,
            true,
            "review coverage incomplete",
            &body.join("\n"),
        ))
    }

    pub(crate) fn snapshot(&self) -> serde_json::Value {
        let coverage: serde_json::Map<String, serde_json::Value> = self
            .coverage
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::json!(v)))
            .collect();
        serde_json::json!({
            "coverage": coverage,
            "findings": self.findings,
            "stopped": self.stopped,
        })
    }

    pub(crate) fn restore(&mut self, snapshot: &serde_json::Value) {
        if let Some(map) = snapshot.get("coverage").and_then(|c| c.as_object()) {
            for (path, ranges) in map {
                for r in ranges.as_array().into_iter().flatten() {
                    if let (Some(a), Some(b)) = (
                        r.get(0).and_then(|v| v.as_u64()),
                        r.get(1).and_then(|v| v.as_u64()),
                    ) {
                        self.record(path, (a as usize, b as usize));
                    }
                }
            }
        }
        for f in snapshot
            .get("findings")
            .and_then(|f| f.as_array())
            .into_iter()
            .flatten()
            .filter_map(|f| f.as_str())
        {
            self.absorb_text(&format!("FINDING: {f}"), false);
        }
    }
}

/// First line of `1..=total` not covered by `ranges`.
fn first_gap(ranges: &[(usize, usize)], total: usize) -> usize {
    let mut next = 1;
    for &(s, e) in ranges {
        if s > next {
            break;
        }
        next = next.max(e + 1);
    }
    next.min(total.max(1))
}

/// Repository-relative key of a path the model passed to `file_read`.
fn relative_key(root: &Path, workspace: &Path, arg: &str) -> Option<String> {
    let abs = super::context::canonical_absolute_path(arg, Some(workspace));
    let abs = std::fs::canonicalize(&abs).unwrap_or_else(|_| PathBuf::from(abs));
    let rel = abs.strip_prefix(root).ok()?;
    Some(
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

impl Agent {
    fn with_review<R>(&self, f: impl FnOnce(&mut ReviewSession) -> R) -> Option<R> {
        let mut state = self.review.lock().unwrap_or_else(|e| e.into_inner());
        state.session.as_mut().map(f)
    }

    /// Whether this task runs under the review coverage machinery.
    pub(crate) fn review_session_active(&self) -> bool {
        self.review
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .session
            .is_some()
    }

    /// Start (or clear) the review session for the current task: build the
    /// inventory, show it, inject the compact version into the context.
    /// No-op (session cleared) for any task that is not a code review.
    pub(super) async fn begin_review_session(&mut self) {
        let pending = {
            let mut state = self.review.lock().unwrap_or_else(|e| e.into_inner());
            state.session = None;
            state.pending_restore.take()
        };
        let task = self.task_context_for_classification().to_string();
        if !super::task_policy::task_is_code_review(&task) {
            return;
        }
        let root = self.tools.workspace_root().path();
        let scan_root = root.clone();
        let inventory =
            match tokio::task::spawn_blocking(move || RepoInventory::scan(&scan_root)).await {
                Ok(Ok(inventory)) => inventory,
                Ok(Err(e)) => {
                    tracing::warn!("review inventory failed: {e}");
                    self.emit_progress(super::progress::ProgressEvent::TurnDecision {
                        decision: "review_inventory".to_string(),
                        detail: format!("NOT PERFORMED — {e}"),
                    });
                    return;
                }
                Err(e) => {
                    tracing::warn!("review inventory task failed: {e}");
                    return;
                }
            };
        let scope = crate::analysis::repo_inventory::resolve_review_scope(&task, &inventory);
        let plan = inventory.review_plan(scope);
        crate::output::review_inventory(&inventory.render_text(Some(&plan)));
        let compact = inventory.render_compact(&plan, INVENTORY_CONTEXT_TOKENS);
        let mut session = ReviewSession::new(&inventory, plan);
        if let Some(snapshot) = pending.as_ref() {
            session.restore(snapshot);
        }
        self.emit_progress(super::progress::ProgressEvent::TurnDecision {
            decision: "review_inventory".to_string(),
            detail: session.inventory_line.clone(),
        });
        if session.plan.is_empty() {
            // Nothing in scope to read: no gate, and the summary says so.
            tracing::info!("review scope has no relevant code files — coverage gate off");
        }
        self.messages.push(Message::user(format!(
            "<selfware_context_note kind=review_inventory>\n\
             This is a REVIEW. The harness inventoried the repository deterministically:\n{compact}\n\n\
             How this review works: read every file in the reading plan with file_read (whole file, \
             or consecutive line_range chunks for large files); outlines, directory listings and grep \
             hits do not count as reading. Record each finding as soon as you see it, as its own line \
             `FINDING: path:line — what is wrong and why` — recorded findings survive context \
             compaction. A final answer is refused while planned files remain unread (unless the \
             budget runs out; the review is then reported as PARTIAL). The final review cites \
             path:line for every finding.\n\
             </selfware_context_note>"
        )));
        let mut state = self.review.lock().unwrap_or_else(|e| e.into_inner());
        state.session = (!session.plan.is_empty()).then_some(session);
    }

    /// Record the line range a delivered `file_read` result showed the model.
    /// `delivered` is the payload that went into the message (post-chunk).
    pub(super) fn review_record_file_read(&self, args_str: &str, delivered: &str) {
        let workspace = self.tools.workspace_root().path();
        self.with_review(|session| {
            let args: serde_json::Value = serde_json::from_str(args_str).unwrap_or_default();
            let Some(arg_path) = ["path", "file_path", "file"]
                .iter()
                .find_map(|k| args.get(*k).and_then(|v| v.as_str()))
            else {
                return;
            };
            let Some(range) = delivered_range(&args, delivered) else {
                return;
            };
            if let Some(rel) = relative_key(&session.root, &workspace, arg_path) {
                session.record(&rel, range);
            }
        });
    }

    /// Absorb `FINDING:` lines from assistant messages not seen before
    /// (idempotent; runs each turn before trim/compaction).
    pub(super) fn observe_review_messages(&self) {
        let messages = &self.messages;
        self.with_review(|session| {
            for m in messages.iter().filter(|m| m.role == "assistant") {
                let text = m.content.text();
                let reasoning = m.reasoning_content.as_deref().unwrap_or_default();
                let fp = fnv(&format!("{text}\u{1}{reasoning}"));
                if session.seen_messages.insert(fp) {
                    session.absorb_text(text, false);
                    session.absorb_text(reasoning, false);
                }
            }
        });
    }

    /// Why the budget no longer allows another reading round, if it does not.
    fn review_limit_reached(&self) -> Option<String> {
        if let Some(step_aside) = self.completion_gate_step_aside() {
            return Some(step_aside.to_string());
        }
        let used = self.loop_control.current_iteration();
        let max = self.loop_control.max_iterations();
        (max.saturating_sub(used) <= REVIEW_ITERATION_RESERVE)
            .then(|| format!("iteration budget ({used} of {max} used)"))
    }

    /// The phase the next model turn is in, `None` outside a review.
    /// `Synthesis` once coverage is complete, the gate stepped aside, or the
    /// budget leaves only room for the answer; `Reading` otherwise. The
    /// signal per-phase request settings (reasoning effort, max_tokens) key
    /// on.
    pub fn review_phase(&self) -> Option<ReviewPhase> {
        let limit = self.review_limit_reached();
        self.with_review(|session| {
            if session.complete() || session.stopped.is_some() || limit.is_some() {
                ReviewPhase::Synthesis
            } else {
                ReviewPhase::Reading
            }
        })
    }

    /// The per-turn review status for the request tail, `None` outside a
    /// review. Emits a `review_phase` progress event on each phase change.
    pub(super) fn review_turn_note(&self) -> Option<String> {
        let phase = self.review_phase()?;
        let limit = self.review_limit_reached();
        let budget = (self.max_context_tokens / 12).clamp(400, 3_000);
        let (note, changed) = self.with_review(|session| {
            let changed = session.announced_phase != Some(phase);
            session.announced_phase = Some(phase);
            (session.turn_note(phase, limit.as_deref(), budget), changed)
        })?;
        if changed {
            let report = self.review_coverage();
            self.emit_progress(super::progress::ProgressEvent::TurnDecision {
                decision: "review_phase".to_string(),
                detail: format!(
                    "{}{}",
                    phase.as_str(),
                    report
                        .map(|r| format!(
                            " — read {} of {} relevant files",
                            r.read_files, r.relevant_files
                        ))
                        .unwrap_or_default()
                ),
            });
        }
        Some(note)
    }

    /// The review completion gate: `Some(refusal)` while relevant files
    /// remain unread (bounded, see [`ReviewSession::gate`]); `None` outside a
    /// review, once coverage is complete, or once reading has been ended by
    /// the budget or the no-progress bound (reported as PARTIAL).
    pub(super) fn review_coverage_gate(&self) -> Option<String> {
        if !self.review_session_active() {
            return None;
        }
        let answer = self.citation_candidate_answer();
        let step = self.loop_control.current_step();
        let limit = self.review_limit_reached();
        self.with_review(|session| session.gate(step, &answer, limit))
            .flatten()
    }

    /// The run's review coverage, `None` outside a review.
    pub fn review_coverage(&self) -> Option<ReviewCoverageReport> {
        self.with_review(|session| session.report())
    }

    /// Coverage + findings for the checkpoint (`None` outside a review).
    pub(super) fn review_snapshot(&self) -> Option<serde_json::Value> {
        self.with_review(|session| session.snapshot())
    }

    /// Queue a checkpointed snapshot for the resumed task's session.
    pub(super) fn queue_review_restore(&self, snapshot: Option<serde_json::Value>) {
        self.review
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pending_restore = snapshot;
    }
}

/// Fold PARTIAL review coverage into a non-failure verdict's evidence (the
/// banner then withholds ✅; see `FailureMode::banner_header`). Complete
/// coverage and failure verdicts pass through unchanged.
pub(crate) fn with_review_coverage(
    base: super::failure_mode::FailureMode,
    coverage: Option<&ReviewCoverageReport>,
) -> super::failure_mode::FailureMode {
    match coverage {
        Some(c) if !c.complete && base.kind.is_nonfailure() => super::failure_mode::FailureMode {
            evidence: format!(
                "{}; {REVIEW_COVERAGE_PARTIAL_NOTE} — read {} of {} relevant files ({}% of lines)",
                base.evidence, c.read_files, c.relevant_files, c.percent_lines
            ),
            ..base
        },
        _ => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::repo_inventory::{PlanReason, ReviewPlan, ReviewScope, Totals};

    fn session(files: &[(&str, usize)]) -> ReviewSession {
        let plan = ReviewPlan {
            scope: ReviewScope::whole_repository(),
            relevant: Totals::default(),
            plan: files
                .iter()
                .map(|(p, l)| PlanEntry {
                    path: p.to_string(),
                    lines: *l,
                    in_degree: 0,
                    reason: PlanReason::Remaining,
                })
                .collect(),
        };
        let inventory = RepoInventory {
            root: "/repo".to_string(),
            gitignore: crate::analysis::repo_inventory::GitignoreStatus::Applied,
            totals: Totals::default(),
            binary_files: 0,
            code: Totals::default(),
            languages: Vec::new(),
            largest_code_files: Vec::new(),
            centrality_metric: crate::analysis::repo_inventory::CENTRALITY_METRIC,
            import_edges: 0,
            most_central: Vec::new(),
            entry_points: Vec::new(),
            files: Vec::new(),
        };
        ReviewSession::new(&inventory, plan)
    }

    #[test]
    fn coverage_is_the_union_of_delivered_ranges() {
        let mut s = session(&[("a.rs", 100), ("b.rs", 10), ("empty.rs", 0)]);
        // Zero-line files carry nothing to read and are not in the plan.
        assert_eq!(s.plan.len(), 2);
        s.record("a.rs", (1, 40));
        s.record("a.rs", (30, 60)); // overlap
        s.record("a.rs", (90, 200)); // clipped to the file
        s.record("outside.rs", (1, 5)); // not relevant: ignored
        assert_eq!(s.covered_lines(), 60 + 11);
        let r = s.report();
        assert_eq!(r.read_files, 0);
        assert_eq!(r.partially_read_files, 1);
        assert!(!r.complete);
        assert_eq!(r.percent_lines, 71 * 100 / 110);
        assert_eq!(
            s.next_unread(5)[0],
            "a.rs — 71 of 100 lines read; continue at line 61"
        );
        s.record("a.rs", (61, 89));
        s.record("b.rs", (1, 10));
        assert!(s.complete());
        let r = s.report();
        assert_eq!((r.read_files, r.percent_lines), (2, 100));
        assert!(
            r.line.starts_with("coverage: read all 2 of 2"),
            "{}",
            r.line
        );
    }

    #[test]
    fn partial_percent_never_rounds_to_complete() {
        let mut s = session(&[("a.rs", 1000)]);
        s.record("a.rs", (1, 999));
        let r = s.report();
        assert_eq!(r.percent_lines, 99);
        assert!(r.line.contains("PARTIAL"), "{}", r.line);
    }

    #[test]
    fn coverage_and_findings_survive_a_snapshot_round_trip() {
        // Compaction never touches the session; resume goes through the
        // checkpoint snapshot — both must keep coverage and findings.
        let mut s = session(&[("a.rs", 50), ("b.rs", 50)]);
        s.record("a.rs", (1, 50));
        s.record("b.rs", (10, 20));
        s.absorb_text("FINDING: a.rs:12 — unwrap on user input panics", false);
        let snap = s.snapshot();
        let mut resumed = session(&[("a.rs", 50), ("b.rs", 50)]);
        resumed.restore(&snap);
        assert_eq!(resumed.covered_lines(), 61);
        assert_eq!(
            resumed.findings,
            vec!["a.rs:12 — unwrap on user input panics"]
        );
    }

    #[test]
    fn delivered_range_reads_what_the_model_received() {
        let whole = serde_json::json!({});
        assert_eq!(
            delivered_range(&whole, r#"{"content":"x","total_lines":12}"#),
            Some((1, 12))
        );
        // A chunked whole read counts only the chunk shown.
        assert_eq!(
            delivered_range(
                &whole,
                r#"{"content":"x","total_lines":900,"shown_line_range":[1,300]}"#
            ),
            Some((1, 300))
        );
        let ranged = serde_json::json!({"line_range": [100, 400]});
        assert_eq!(
            delivered_range(
                &ranged,
                r#"{"content":"x","lines_returned":51,"total_lines":150}"#
            ),
            Some((100, 150))
        );
        // No content (outline / unchanged notes, spilled summaries): nothing.
        assert_eq!(
            delivered_range(&whole, r#"{"unchanged_since_turn":3,"content":"x"}"#),
            None
        );
        assert_eq!(
            delivered_range(&whole, "summary with a disk reference"),
            None
        );
        assert_eq!(
            delivered_range(&whole, r#"{"content":"","total_lines":0}"#),
            None
        );
    }

    #[test]
    fn gate_refuses_until_covered_then_accepts() {
        let mut s = session(&[("a.rs", 10), ("b.rs", 10)]);
        let draft = "Review: - a.rs:3 — off-by-one in the loop bound";
        let first = s.gate(1, draft, None).expect("refused");
        assert!(first.contains("REVIEW COVERAGE INCOMPLETE"), "{first}");
        assert!(first.contains("- a.rs (10 lines)"), "{first}");
        // The refused draft's cited line is kept as a finding.
        assert_eq!(s.findings.len(), 1);
        // Same step, same answer: one refusal, not two.
        assert_eq!(s.gate(1, draft, None), Some(first));
        assert_eq!(s.refusals, 1);
        s.record("a.rs", (1, 10));
        let second = s.gate(2, draft, None).expect("still refused");
        assert!(second.contains("- b.rs (10 lines)"), "{second}");
        s.record("b.rs", (1, 10));
        assert_eq!(s.gate(3, "Final review: a.rs:3 off-by-one.", None), None);
        assert!(s.report().complete);
    }

    #[test]
    fn complete_review_without_citations_is_nudged_once() {
        let mut s = session(&[("a.rs", 5)]);
        s.record("a.rs", (1, 5));
        let nudge = s.gate(1, "Looks fine overall.", None).expect("nudged");
        assert!(nudge.contains("cites no `path:line`"), "{nudge}");
        assert_eq!(s.gate(2, "Looks fine overall, no issues.", None), None);
    }

    #[test]
    fn gate_steps_aside_after_refusals_without_progress() {
        let mut s = session(&[("a.rs", 10), ("b.rs", 10)]);
        assert!(s.gate(1, "answer one", None).is_some());
        assert!(
            s.gate(2, "answer two", None).is_some(),
            "one no-progress refusal"
        );
        // Bound reached: gating ends; an uncited answer gets one citation
        // round, then is accepted.
        let nudge = s.gate(3, "answer three", None).expect("citation nudge");
        assert!(nudge.contains("cites no `path:line`"), "{nudge}");
        assert_eq!(s.gate(4, "answer four", None), None);
        let r = s.report();
        assert!(!r.complete);
        assert!(r
            .stopped
            .as_deref()
            .unwrap()
            .contains("without reading further"));
        assert!(r.line.starts_with("⚠️ coverage: PARTIAL"), "{}", r.line);
    }

    #[test]
    fn budget_exhaustion_ends_with_an_honest_partial_outcome() {
        let mut s = session(&[("a.rs", 10), ("b.rs", 10)]);
        s.record("a.rs", (1, 10));
        let limit = Some("iteration budget (398 of 400 used)".to_string());
        assert_eq!(s.gate(7, "partial review a.rs:2 bad", limit), None);
        let r = s.report();
        assert!(!r.complete);
        assert_eq!((r.read_files, r.relevant_files), (1, 2));
        assert!(r.line.contains("stopped: iteration budget"), "{}", r.line);
        assert!(r.line.contains("not read: b.rs"), "{}", r.line);
        // The verdict keeps its kind but gains the PARTIAL note.
        let base = super::super::failure_mode::FailureMode {
            restored_files: Vec::new(),
            kind: super::super::failure_mode::FailureKind::NoChange,
            evidence: "completed naturally".to_string(),
            advice: "-".to_string(),
        };
        let folded = with_review_coverage(base, Some(&r));
        assert!(folded.evidence.contains(REVIEW_COVERAGE_PARTIAL_NOTE));
        assert!(!folded.is_clean_success(), "{}", folded.evidence);
    }

    #[test]
    fn findings_are_parsed_deduplicated_and_budgeted() {
        let mut s = session(&[("a.rs", 10)]);
        s.absorb_text(
            "Reading on.\nFINDING: a.rs:4 — integer overflow on large input\n- **Finding:** a.rs:9 — error swallowed silently\nfinding: a.rs:4 — integer overflow on large input",
            false,
        );
        assert_eq!(s.findings.len(), 2);
        // Plain prose with a citation is not a finding outside a draft.
        s.absorb_text("I looked at a.rs:5 and it seems fine so far.", false);
        assert_eq!(s.findings.len(), 2);
        let note = s.turn_note(ReviewPhase::Reading, None, 2_000);
        assert!(note.contains("Findings recorded so far (2)"), "{note}");
        assert!(note.contains("- a.rs (10 lines)"), "{note}");
        for i in 0..200 {
            s.absorb_text(
                &format!("FINDING: a.rs:{i} — a distinct problem number {i}"),
                false,
            );
        }
        let tight = s.turn_note(ReviewPhase::Synthesis, None, 400);
        assert!(
            estimate_content_tokens(&tight) <= 420,
            "{}",
            estimate_content_tokens(&tight)
        );
        assert!(tight.contains("earlier findings not shown"), "{tight}");
    }

    fn xml_read(path: &str) -> String {
        format!("<tool>\n<name>file_read</name>\n<arguments>{{\"path\":\"{path}\"}}</arguments>\n</tool>")
    }

    /// The live failure shape, against a scripted model: it reads two of the
    /// three relevant files and answers. The gate sends it back naming the
    /// unread file; it reads it, answers again, and the run completes with
    /// full coverage and the finding recorded.
    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
    )]
    async fn agent_is_sent_back_until_the_reading_plan_is_covered() {
        use crate::testing::mock_api::MockLlmServer;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("pkg")).unwrap();
        std::fs::write(
            root.join("pkg/__init__.py"),
            "from .a import ratio\nfrom .b import show\n",
        )
        .unwrap();
        std::fs::write(
            root.join("pkg/a.py"),
            "def ratio(x, n):\n    return x / n\n",
        )
        .unwrap();
        std::fs::write(root.join("pkg/b.py"), "def show(v):\n    print(v)\n").unwrap();
        let _cwd = crate::test_support::CwdGuard::enter(root);

        let draft = "Review of the repository: FINDING: pkg/a.py:2 — ratio divides by n without \
                     checking for zero, so ratio(1, 0) raises ZeroDivisionError.";
        let server = MockLlmServer::builder()
            .with_response(xml_read("pkg/__init__.py"))
            .with_response(xml_read("pkg/a.py"))
            .with_response(draft)
            .with_response(xml_read("pkg/b.py"))
            .with_response(
                "Final review. pkg/a.py:2 — ratio divides by n without a zero check \
                 (ZeroDivisionError on n=0). pkg/b.py:2 — show prints and returns None; fine.",
            )
            .build()
            .await;
        let config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();
        let result = agent
            .run_task("review this repository for bugs, cite file:line")
            .await;
        server.stop().await;
        assert!(result.is_ok(), "{:?}", result.err());

        let refusal = agent
            .messages
            .iter()
            .map(|m| m.content.text().to_string())
            .find(|t| t.contains("REVIEW COVERAGE INCOMPLETE"))
            .expect("the early answer was refused");
        assert!(refusal.contains("pkg/b.py (2 lines)"), "{refusal}");
        let coverage = agent.review_coverage().expect("review session");
        assert!(coverage.complete, "{coverage:?}");
        assert_eq!((coverage.read_files, coverage.relevant_files), (3, 3));
        assert!(coverage.findings_recorded >= 1, "{coverage:?}");
        assert!(
            coverage.line.starts_with("coverage: read all 3 of 3"),
            "{}",
            coverage.line
        );
        assert_eq!(agent.review_phase(), Some(ReviewPhase::Synthesis));
        assert!(agent.last_assistant_response.contains("Final review"));
    }
}
