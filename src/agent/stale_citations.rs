//! Line references an edit made stale (no model call).
//!
//! Evidence (live harness, c24 on 0.9.3–0.9.5): the task writes
//! `docs/CONTEXT_NOTES.md` listing every `pub fn` as "`name` (line N)" and
//! then adds `///` doc comments above those functions. Every wrong citation
//! in those runs was a +4/+5 shift: the notes were right when written, the
//! doc comments moved the functions, and the model never refreshed the
//! notes. The completion gate caught it only on the final answer — twice on
//! the very last iteration, with no turn left to correct anything.
//!
//! This watch tells the model right after the batch that moved them, once
//! per moved reference, exactly which references in files it wrote this
//! task now point at the wrong line and where the named code is now. The
//! mapping is structural: the content of each edited file right before the
//! edit is kept ([`StaleCitationWatch::record_pre_mutation`]), and a
//! citation that anchored on its line in an earlier version is carried
//! through the line diff to the current file
//! ([`citation_check::moved_citation_range`]). Nothing is inferred from
//! prose; a citation that was never right is left to the completion gate.
//!
//! Scope: citations in doc-like files the agent wrote this task (the same
//! set the completion gate checks). The final answer is judged by the
//! completion gate against the final tree, so it is not watched here.
//! Mutations through tools without a path list (shell, VCS, formatters)
//! snapshot every file the written deliverables cite before they run.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::citation_check::{self, CitationResolver, MovedCitation};

/// Earlier contents kept per file. The oldest (the file as first edited
/// this task) is always kept; the rest are the newest pre-edit states.
const MAX_VERSIONS_PER_FILE: usize = 8;
/// Files larger than this are not snapshotted (their references are then
/// not tracked; the completion gate still checks them).
const MAX_SNAPSHOT_BYTES: u64 = 2 * 1024 * 1024;
/// Distinct files snapshotted per task.
const MAX_TRACKED_FILES: usize = 64;
/// Moved references listed in one notice.
const MAX_LISTED: usize = 24;

/// Marker of the notice (and its turn-decision name).
pub(crate) const STALE_CITATIONS_REASON: &str = "an edit moved lines that a file you wrote cites";

/// Per-task state: earlier file contents and the references already reported.
#[derive(Debug, Default)]
pub(crate) struct StaleCitationWatch {
    /// Canonical path -> contents right before each edit this task, oldest first.
    versions: HashMap<PathBuf, Vec<Arc<Vec<String>>>>,
    /// (source, cited path, start, end, symbol, new start) already reported.
    reported: HashSet<(String, String, usize, usize, Option<String>, usize)>,
}

impl StaleCitationWatch {
    /// Keep `path`'s current content as an earlier version (called right
    /// before a tool mutates it). Missing, non-UTF-8 or oversized files and
    /// content equal to the newest kept version are skipped.
    pub(crate) fn record_pre_mutation(&mut self, path: &Path) {
        let Ok(canonical) = std::fs::canonicalize(path) else {
            return;
        };
        if !self.versions.contains_key(&canonical) && self.versions.len() >= MAX_TRACKED_FILES {
            return;
        }
        match std::fs::metadata(&canonical) {
            Ok(m) if m.is_file() && m.len() <= MAX_SNAPSHOT_BYTES => {}
            _ => return,
        }
        let Ok(text) = std::fs::read_to_string(&canonical) else {
            return;
        };
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        let kept = self.versions.entry(canonical).or_default();
        if kept.last().is_some_and(|last| **last == lines) {
            return;
        }
        kept.push(Arc::new(lines));
        if kept.len() > MAX_VERSIONS_PER_FILE {
            // Keep the first (the file as the task first saw it edited).
            kept.remove(1);
        }
    }

    /// Earlier versions of `canonical`, oldest first.
    pub(crate) fn versions(&self, canonical: &Path) -> Vec<Arc<Vec<String>>> {
        self.versions.get(canonical).cloned().unwrap_or_default()
    }

    /// Whether any file was snapshotted this task.
    pub(crate) fn is_tracking(&self) -> bool {
        !self.versions.is_empty()
    }

    /// The moves not reported before; marks them reported.
    pub(crate) fn take_unreported(&mut self, moved: Vec<MovedCitation>) -> Vec<MovedCitation> {
        moved
            .into_iter()
            .filter(|m| {
                self.reported.insert((
                    m.source.clone(),
                    m.citation.path.clone(),
                    m.citation.start,
                    m.citation.end,
                    m.citation.symbol.clone(),
                    m.new_start,
                ))
            })
            .collect()
    }
}

/// Moved references in `text` (a file the agent wrote, `source`), given the
/// earlier versions the watch kept.
pub(crate) fn moved_citations_in(
    resolver: &mut CitationResolver,
    watch: &StaleCitationWatch,
    source: &str,
    text: &str,
) -> Vec<MovedCitation> {
    let mut out = Vec::new();
    for c in citation_check::parse_citations(text) {
        let Some((canonical, file, current)) = resolver.cited_file(text, &c.path) else {
            continue;
        };
        let versions = watch.versions(&canonical);
        if versions.is_empty() {
            continue;
        }
        let refs: Vec<&[String]> = versions.iter().map(|v| v.as_slice()).collect();
        if let Some((new_start, new_end)) =
            citation_check::moved_citation_range(&c, &refs, &current)
        {
            out.push(MovedCitation {
                source: source.to_string(),
                citation: c,
                file,
                new_start,
                new_end,
            });
        }
    }
    out
}

/// The notice for one batch's moved references.
pub(crate) fn stale_citations_notice(moved: &[MovedCitation]) -> String {
    let mut lines: Vec<String> = moved
        .iter()
        .take(MAX_LISTED)
        .map(|m| format!("- {}", m.describe()))
        .collect();
    let more = moved.len().saturating_sub(MAX_LISTED);
    if more > 0 {
        lines.push(format!("- ... and {more} more"));
    }
    let sources: Vec<&str> = {
        let mut s: Vec<&str> = moved.iter().map(|m| m.source.as_str()).collect();
        s.sort_unstable();
        s.dedup();
        s
    };
    super::task_policy::policy_envelope(
        super::task_policy::PolicyKind::StaleCitations,
        true,
        STALE_CITATIONS_REASON,
        &format!(
            "STALE LINE REFERENCES — your edit moved code that {} cites. The new \
             positions below come from the edit's line diff (checked, not estimated):\n{}\n\
             Update these line numbers in {} (no need to re-read the files for this), \
             then continue with the task.",
            sources.join(", "),
            lines.join("\n"),
            sources.join(", ")
        ),
    )
}

impl super::Agent {
    /// Snapshot what a mutating call is about to change (see the module
    /// docs). A call with a path list snapshots those paths; one without
    /// (shell, VCS, formatters) snapshots every file the task's written
    /// deliverables cite.
    pub(super) fn stale_citations_before_mutation(&mut self, name: &str, args: &serde_json::Value) {
        use super::tool_dispatch::helpers::{
            tool_call_is_mutating, tool_call_is_opaque_mutation, written_paths_for_tool_call,
        };
        if !tool_call_is_mutating(name, args) {
            return;
        }
        let root = self.tools.workspace_root();
        let mut paths: Vec<PathBuf> = written_paths_for_tool_call(name, args)
            .into_iter()
            .map(|p| root.anchor_path(&p))
            .collect();
        if paths.is_empty() || tool_call_is_opaque_mutation(name, args) {
            paths.extend(self.deliverable_cited_files());
        }
        for path in paths {
            self.stale_citations.record_pre_mutation(&path);
        }
    }

    /// Canonical paths of the files the written deliverables cite.
    fn deliverable_cited_files(&self) -> Vec<PathBuf> {
        let root = self.tools.workspace_root().path();
        let mut resolver = CitationResolver::with_policy(&root, &self.config.safety);
        let mut out: Vec<PathBuf> = Vec::new();
        for source in self.written_deliverables() {
            let Some(text) = resolver.read_confined(&source) else {
                continue;
            };
            for c in citation_check::parse_citations(&text) {
                if let Some((canonical, _, _)) = resolver.cited_file(&text, &c.path) {
                    if !out.contains(&canonical) {
                        out.push(canonical);
                    }
                }
            }
        }
        out
    }

    /// After a batch that mutated the tree: tell the model, once, which
    /// references in the files it wrote this task the batch moved, and
    /// where they point now. Returns whether a notice was pushed.
    pub(super) fn push_stale_citations_notice(&mut self) -> bool {
        if !self.stale_citations.is_tracking() {
            return false;
        }
        let root = self.tools.workspace_root().path();
        let mut resolver = CitationResolver::with_policy(&root, &self.config.safety);
        let mut moved = Vec::new();
        for source in self.written_deliverables() {
            let Some(text) = resolver.read_confined(&source) else {
                continue;
            };
            moved.extend(moved_citations_in(
                &mut resolver,
                &self.stale_citations,
                &source,
                &text,
            ));
        }
        let fresh = self.stale_citations.take_unreported(moved);
        if fresh.is_empty() {
            return false;
        }
        let detail = format!(
            "{} reference(s) moved by an edit: {}",
            fresh.len(),
            fresh
                .iter()
                .take(3)
                .map(MovedCitation::describe)
                .collect::<Vec<_>>()
                .join("; ")
        );
        tracing::info!("stale citations: {detail}");
        self.emit_progress(super::progress::ProgressEvent::TurnDecision {
            decision: "stale_citations".to_string(),
            detail,
        });
        self.messages
            .push(crate::api::types::Message::user(stale_citations_notice(
                &fresh,
            )));
        true
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/stale_citations/stale_citations_test.rs"]
mod tests;
