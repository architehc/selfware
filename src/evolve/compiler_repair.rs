//! Compiler-suggestion auto-repair.
//!
//! Reads structured `cargo … --message-format=json` diagnostics
//! ([`AnalysisReport`]), takes the suggestions rustc marks
//! `MachineApplicable` (`help: consider borrowing here: &`), applies them at
//! their byte offsets, and re-checks — a bounded loop
//! ([`DiagnosticRepairEngine::run_repair_loop`], at most `max_rounds` repair
//! passes; formal model: `formal/EvolutionBounds.lean`, E1
//! `repair_loop_bounded`). Errors without such a suggestion get plain-English
//! guidance for the model instead ([`DiagnosticRepairEngine::synthesize_repair_guidance`]).
//!
//! What is applied, and what is not:
//!
//! - only `MachineApplicable` suggestions (rustfix's rule): `MaybeIncorrect`,
//!   `HasPlaceholders` and `Unspecified` suggestions are never written;
//! - the spans of one `help:` child are one multipart suggestion, applied
//!   together or not at all; a suggestion overlapping one already accepted
//!   (an alternative fix for the same code) is skipped, and the identical
//!   suggestion reported twice (a crate compiled as lib and as test target)
//!   is applied once;
//! - only files inside the project root: rustc reports absolute paths for
//!   dependency sources (`~/.cargo/registry/…`) and the standard library,
//!   and a `..` or absolute path is never followed;
//! - every skipped fix is returned with its reason ([`RepairApplication`]),
//!   so a caller never reports "repaired" for edits that were not made.

use crate::evolve::diagnostics::{AnalysisReport, CompilerDiagnostic};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::future::Future;
use std::path::{Component, Path, PathBuf};

/// The applicability rustc gives a suggestion that is safe to apply unseen.
const MACHINE_APPLICABLE: &str = "MachineApplicable";

/// One span edit of a machine-applicable rustc suggestion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineApplicableFix {
    /// File as rustc reported it (relative to the project root for local code).
    pub file: String,
    pub line_start: usize,
    pub line_end: usize,
    /// One-based, in characters.
    pub column_start: usize,
    pub column_end: usize,
    /// Zero-based byte range; when rustc omitted it, it is resolved from the
    /// line/column pair against the file content at apply time.
    pub byte_start: Option<usize>,
    pub byte_end: Option<usize>,
    pub replacement: String,
    pub diagnostic_message: String,
    /// Edits sharing a `suggestion_id` are one multipart suggestion.
    pub suggestion_id: usize,
}

/// A fix that was not written, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedFix {
    pub fix: MachineApplicableFix,
    pub reason: String,
}

/// What [`DiagnosticRepairEngine::apply_machine_fixes`] actually did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairApplication {
    /// Span edits written to disk.
    pub applied: usize,
    /// Duplicate reports of an edit that was applied once.
    pub duplicates: usize,
    pub skipped: Vec<SkippedFix>,
    /// Project-relative files that were rewritten.
    pub files_changed: Vec<String>,
}

/// Why [`DiagnosticRepairEngine::run_repair_loop`] stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairStop {
    /// The last check succeeded.
    Compiles,
    /// The check failed and rustc offered no applicable fix (or none could
    /// be written): the remaining errors need a model.
    NoApplicableFix,
    /// `max_rounds` repair passes were made and the check still fails.
    FuelExhausted,
}

/// Result of a bounded repair loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairLoopOutcome {
    /// Repair passes that wrote at least one edit (≤ `max_rounds`).
    pub rounds: usize,
    /// Span edits written over all rounds.
    pub fixes_applied: usize,
    /// Checks run (initial check + one per round).
    pub checks_run: usize,
    pub stop: RepairStop,
    pub final_report: AnalysisReport,
    /// Fixes skipped in the rounds, with reasons.
    pub skipped: Vec<SkippedFix>,
}

/// Actionable, plain-English repair guidance for compiler errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionableRepairGuidance {
    pub file: String,
    pub line: usize,
    pub code: Option<String>,
    pub message: String,
    pub rendered: Option<String>,
    pub suggested_action: String,
}

/// Engine for extracting and applying compiler-suggested repairs.
#[derive(Debug, Default)]
pub struct DiagnosticRepairEngine;

impl DiagnosticRepairEngine {
    pub fn new() -> Self {
        Self
    }

    /// Every `MachineApplicable` suggestion edit in `report` (children
    /// included), each tagged with the suggestion it belongs to.
    pub fn extract_machine_fixes(&self, report: &AnalysisReport) -> Vec<MachineApplicableFix> {
        let mut fixes = Vec::new();
        let mut next_id = 0usize;
        for diag in &report.diagnostics {
            collect_fixes(diag, &diag.message, &mut next_id, &mut fixes);
        }
        fixes
    }

    /// Apply machine-applicable fixes to files under `root`.
    ///
    /// Per file, suggestions are accepted in order unless an edit falls
    /// outside the file, off a character boundary, or overlaps an accepted
    /// edit; accepted edits are then written from the highest byte offset
    /// down so earlier offsets stay valid.
    pub fn apply_machine_fixes(
        &self,
        root: &Path,
        fixes: &[MachineApplicableFix],
    ) -> Result<RepairApplication> {
        let mut out = RepairApplication::default();
        if fixes.is_empty() {
            return Ok(out);
        }
        let root_canon = fs::canonicalize(root)
            .with_context(|| format!("repair root {} is not accessible", root.display()))?;

        let mut by_file: BTreeMap<String, Vec<&MachineApplicableFix>> = BTreeMap::new();
        for fix in fixes {
            by_file.entry(fix.file.clone()).or_default().push(fix);
        }

        for (file, file_fixes) in by_file {
            let path = match resolve_inside(&root_canon, &file) {
                Ok(p) => p,
                Err(reason) => {
                    out.skipped
                        .extend(file_fixes.into_iter().map(|f| SkippedFix {
                            fix: f.clone(),
                            reason: reason.clone(),
                        }));
                    continue;
                }
            };
            let content = fs::read_to_string(&path).with_context(|| {
                format!("failed to read {} for compiler repair", path.display())
            })?;

            // Group the file's edits by suggestion, keeping first-seen order.
            let mut groups: Vec<(usize, Vec<&MachineApplicableFix>)> = Vec::new();
            for fix in file_fixes {
                match groups.iter_mut().find(|(id, _)| *id == fix.suggestion_id) {
                    Some((_, g)) => g.push(fix),
                    None => groups.push((fix.suggestion_id, vec![fix])),
                }
            }

            let mut accepted: Vec<(usize, usize, String)> = Vec::new();
            for (_, group) in groups {
                let mut edits = Vec::with_capacity(group.len());
                let mut reject: Option<String> = None;
                for fix in &group {
                    match resolve_range(&content, fix) {
                        Ok(range) => edits.push((range.0, range.1, fix.replacement.clone())),
                        Err(reason) => {
                            reject = Some(reason);
                            break;
                        }
                    }
                }
                if reject.is_none() {
                    // The same suggestion reported again (lib + test target).
                    if edits.iter().all(|e| accepted.contains(e)) {
                        out.duplicates += edits.len();
                        continue;
                    }
                    if edits
                        .iter()
                        .any(|e| !accepted.contains(e) && accepted.iter().any(|a| overlaps(a, e)))
                    {
                        reject =
                            Some("overlaps a fix already accepted (alternative suggestion)".into());
                    }
                }
                match reject {
                    Some(reason) => out.skipped.extend(group.into_iter().map(|f| SkippedFix {
                        fix: f.clone(),
                        reason: reason.clone(),
                    })),
                    None => {
                        for e in edits {
                            if accepted.contains(&e) {
                                out.duplicates += 1;
                            } else {
                                accepted.push(e);
                            }
                        }
                    }
                }
            }

            if accepted.is_empty() {
                continue;
            }
            // Highest start first; at equal start the wider edit first, so an
            // insertion at the same offset lands before the replaced text.
            accepted.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
            let mut new_content = content;
            for (start, end, replacement) in &accepted {
                new_content.replace_range(*start..*end, replacement);
            }
            fs::write(&path, new_content)
                .with_context(|| format!("failed to write repaired {}", path.display()))?;
            out.applied += accepted.len();
            out.files_changed.push(file);
        }

        Ok(out)
    }

    /// Check, apply machine-applicable fixes, re-check — at most
    /// `max_rounds` repair passes. `check` runs the compiler (e.g. `cargo
    /// check --message-format=json` in a quarantined arm) and returns its
    /// report; an `Err` from it ends the loop with that error.
    pub async fn run_repair_loop<F, Fut>(
        &self,
        root: &Path,
        max_rounds: usize,
        mut check: F,
    ) -> Result<RepairLoopOutcome>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<AnalysisReport>>,
    {
        let mut report = check().await?;
        let mut checks_run = 1usize;
        let mut rounds = 0usize;
        let mut fixes_applied = 0usize;
        let mut skipped = Vec::new();
        let stop = loop {
            if report.success {
                break RepairStop::Compiles;
            }
            if rounds >= max_rounds {
                break RepairStop::FuelExhausted;
            }
            let fixes = self.extract_machine_fixes(&report);
            if fixes.is_empty() {
                break RepairStop::NoApplicableFix;
            }
            let applied = self.apply_machine_fixes(root, &fixes)?;
            skipped.extend(applied.skipped);
            if applied.applied == 0 {
                break RepairStop::NoApplicableFix;
            }
            rounds += 1;
            fixes_applied += applied.applied;
            report = check().await?;
            checks_run += 1;
        };
        Ok(RepairLoopOutcome {
            rounds,
            fixes_applied,
            checks_run,
            stop,
            final_report: report,
            skipped,
        })
    }

    /// Synthesize actionable repair guidance for top-level errors.
    pub fn synthesize_repair_guidance(
        &self,
        report: &AnalysisReport,
    ) -> Vec<ActionableRepairGuidance> {
        let mut guidance = Vec::new();

        for diag in &report.diagnostics {
            if diag.level != "error" {
                continue;
            }

            let primary_span = diag
                .spans
                .iter()
                .find(|s| s.is_primary)
                .or_else(|| diag.spans.first());

            let (file, line) = match primary_span {
                Some(span) => (span.file.clone(), span.line_start),
                None => ("unknown".to_string(), 0),
            };

            let suggested_action = match diag.code.as_deref() {
                Some("E0382") => "Value moved here. Add `.clone()` at the move site, or update the function signature to take a borrowed reference `&`.",
                Some("E0308") => "Type mismatch. Verify expected vs actual types, or insert appropriate conversion (e.g. `.into()`, `&`, `as _`).",
                Some("E0425") => "Cannot find value or function in this scope. Check import declarations or verify identifier spelling.",
                Some("E0599") => "No method or associated item found for type. Check if required trait is imported (e.g. `use std::io::Write;`).",
                Some("E0277") => "Trait bound is not satisfied. Add `#[derive(...)]` to the struct, or implement the required trait.",
                _ => "Review the compiler diagnostic snippet and adjust the syntax or types to satisfy rustc.",
            };

            guidance.push(ActionableRepairGuidance {
                file,
                line,
                code: diag.code.clone(),
                message: diag.message.clone(),
                rendered: diag.rendered.clone(),
                suggested_action: suggested_action.to_string(),
            });
        }

        guidance
    }
}

/// Walk `diag` and its children; each diagnostic whose spans carry
/// machine-applicable replacements is one suggestion (one id).
fn collect_fixes(
    diag: &CompilerDiagnostic,
    top_message: &str,
    next_id: &mut usize,
    out: &mut Vec<MachineApplicableFix>,
) {
    let id = *next_id;
    let mut used = false;
    for span in &diag.spans {
        let (Some(replacement), Some(MACHINE_APPLICABLE)) = (
            span.suggested_replacement.as_ref(),
            span.suggestion_applicability.as_deref(),
        ) else {
            continue;
        };
        used = true;
        out.push(MachineApplicableFix {
            file: span.file.clone(),
            line_start: span.line_start,
            line_end: span.line_end,
            column_start: span.column_start,
            column_end: span.column_end,
            byte_start: span.byte_start,
            byte_end: span.byte_end,
            replacement: replacement.clone(),
            diagnostic_message: format!("{top_message}: {}", diag.message),
            suggestion_id: id,
        });
    }
    if used {
        *next_id += 1;
    }
    for child in &diag.children {
        collect_fixes(child, top_message, next_id, out);
    }
}

/// `file` (as rustc reported it) resolved inside `root_canon`, or why not.
fn resolve_inside(root_canon: &Path, file: &str) -> std::result::Result<PathBuf, String> {
    let rel = Path::new(file);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(format!(
            "`{file}` is not a project-relative path (dependency or std source)"
        ));
    }
    let joined = root_canon.join(rel);
    let canon = fs::canonicalize(&joined).map_err(|_| format!("`{file}` does not exist"))?;
    if !canon.starts_with(root_canon) {
        return Err(format!("`{file}` resolves outside the project root"));
    }
    if !canon.is_file() {
        return Err(format!("`{file}` is not a regular file"));
    }
    Ok(canon)
}

/// Byte range of `fix` in `content`, validated.
fn resolve_range(
    content: &str,
    fix: &MachineApplicableFix,
) -> std::result::Result<(usize, usize), String> {
    let (start, end) = match (fix.byte_start, fix.byte_end) {
        (Some(s), Some(e)) => (s, e),
        _ => (
            char_position_to_byte(content, fix.line_start, fix.column_start)
                .ok_or("line/column start outside the file")?,
            char_position_to_byte(content, fix.line_end, fix.column_end)
                .ok_or("line/column end outside the file")?,
        ),
    };
    if start > end || end > content.len() {
        return Err(format!(
            "byte range {start}..{end} outside the file ({} bytes)",
            content.len()
        ));
    }
    if !content.is_char_boundary(start) || !content.is_char_boundary(end) {
        return Err(format!(
            "byte range {start}..{end} splits a character (file changed since the check?)"
        ));
    }
    Ok((start, end))
}

/// Byte offset of a one-based (line, character column) position.
fn char_position_to_byte(content: &str, line: usize, column: usize) -> Option<usize> {
    if line == 0 || column == 0 {
        return None;
    }
    let mut offset = 0usize;
    for (idx, text) in content.split_inclusive('\n').enumerate() {
        if idx + 1 == line {
            let body = text.strip_suffix('\n').unwrap_or(text);
            let mut chars = body.char_indices().map(|(b, _)| b).chain([body.len()]);
            return chars.nth(column - 1).map(|b| offset + b);
        }
        offset += text.len();
    }
    // A position just past the last line (end of file).
    (line == content.split_inclusive('\n').count() + 1 && column == 1).then_some(content.len())
}

/// Whether two edits conflict: overlapping ranges, or two insertions at the
/// same offset (their order would be a guess).
fn overlaps(a: &(usize, usize, String), b: &(usize, usize, String)) -> bool {
    if a.0 == a.1 && b.0 == b.1 {
        return a.0 == b.0;
    }
    a.0 < b.1 && b.0 < a.1
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/compiler_repair_test.rs"]
mod tests;
