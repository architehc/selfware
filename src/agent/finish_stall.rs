//! Finish stall: the work is done and verified, the model keeps re-reading.
//!
//! c24 (0.9.3 integration run, qwen38-flash-next, 24k window): by turn 38
//! every deliverable was written and `cargo_check` had passed on the final
//! tree. Turns 39–51 were ONLY `file_read` / `grep_search` calls over the
//! same unchanged files, every thinking block opening "Let me finalize",
//! and no final answer came. The adaptive budget extension then granted
//! 40→50 (the reads had distinct argument hashes, so `productive_streak`
//! took them for progress), the run died at MAX_ITERATIONS with "raise
//! max_iterations" advice, and the summary said it had "restored" two files
//! that were byte-identical to the snapshot.
//!
//! Detection is structural — nothing reads the model's prose. A dispatched
//! turn is a *finish-stall turn* when all of these hold:
//!
//! - the tree was verified green at the start of the turn and still is at
//!   its end, with no mutation in between (`mutation_sequence` unchanged;
//!   green = a credited pass covers the current tree and no in-scope failure
//!   blocks it — the completion gate's own freshness,
//!   `verification_pass_covers_current_tree`);
//! - every call in it was read-only (no mutating tool was even attempted);
//! - no call returned anything the task had not already been given: a
//!   `file_read` line or `grep_search` match counts as seen when the same
//!   (file, line number, text) was delivered before — an edit that shifts or
//!   changes lines makes them new again — and any other read-only result
//!   when the identical output was returned before. Re-running a check on a
//!   tree that is already green and unchanged adds nothing either.
//!
//! Target-level novelty (`seen_read_targets`, the progress guard's notion)
//! is not enough here: the same path read again after an edit IS new
//! content, and a different grep pattern over unchanged files can return
//! only lines already shown.
//!
//! After [`FINISH_STALL_TURNS`] consecutive finish-stall turns the model gets
//! ONE directive naming what is done and verified and asking for the final
//! answer. After another [`FINISH_STALL_TURNS`] such turns, a read-only call
//! that would return only already-seen content is answered with a short
//! refusal instead of the content. A novel read resets the streak (the
//! model is gathering something new) and a mutation resets everything (the
//! tree is no longer the verified one).
//!
//! Why 3: it is the smallest window that still lets a model verify each of
//! two or three deliverables once after the passing check without any
//! pressure, and it matches the existing unchanged-reread block
//! (`maybe_block_redundant_reread`, 3 unchanged reads). Measured on c24:
//! the post-green phase had 13 read-only turns and ZERO mutations, while the
//! legitimate post-edit re-reads (turns 31–34, reading the just-edited file
//! and the new notes file) all delivered new lines and are never counted.
//!
//! The same state withholds the adaptive iteration-budget extension and the
//! auto-continue chain (`maybe_extend_iteration_budget`): turns that only
//! re-read a verified, unchanged tree are not forward progress, so they must
//! not earn more budget (AGENTS.md rule 3/4 — the run summary says why).

use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use serde_json::Value;

/// Consecutive finish-stall turns before the directive, and again before
/// re-reads are refused (see the module docs for the choice).
pub(crate) const FINISH_STALL_TURNS: usize = 3;

/// Leading marker of the finish directive (tests and logs key on it).
pub(crate) const FINISH_STALL_DIRECTIVE_MARKER: &str = "FINISH NOW";

/// JSON key of the refusal result that replaces an already-seen re-read.
pub(crate) const FINISH_STALL_REFUSAL_KEY: &str = "finish_stall_refused";

/// Where the tree was last verified green.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GreenPoint {
    /// Turn at the end of which the tree was first seen verified green at
    /// this mutation sequence.
    pub turn: usize,
    /// Mutation sequence the green verdict covers.
    pub mutation_sequence: usize,
    /// The passing check that turn ran (tool name), when it ran one; `None`
    /// when the credit came from the post-edit check or a doc-only proof.
    pub check: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Watching,
    Nudged,
    Refusing,
}

#[derive(Debug, Default)]
struct OpenTurn {
    turn: usize,
    green_at_start: bool,
    mutation_sequence: usize,
    calls: usize,
    read_only: bool,
    novel: bool,
    passing_check: Option<String>,
}

/// What the end of a turn asks the dispatcher to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TurnVerdict {
    Continue,
    /// Push the finish directive once.
    Nudge,
    /// From the next turn on, already-seen read-only calls are refused.
    StartRefusing,
}

/// Per-task finish-stall state.
#[derive(Debug, Default)]
pub(crate) struct FinishStall {
    /// Hashes of every observation delivered this task (see module docs).
    seen: HashSet<u64>,
    open: Option<OpenTurn>,
    green: Option<GreenPoint>,
    phase: Phase,
    /// Consecutive finish-stall turns in the current phase.
    streak: usize,
    /// Finish-stall turns since the current green point.
    stall_turns: usize,
    /// Dispatched turns (any kind) since the current green point.
    turns_since_green: usize,
    last_turn_stalled: bool,
    /// Number of the last closed turn.
    last_turn: usize,
    nudged_at: Option<usize>,
    refusing_from: Option<usize>,
    refused: usize,
    /// Set when an adaptive extension / auto-continue was withheld because
    /// of the stall (the turn it was withheld at).
    extension_withheld_at: Option<usize>,
}

fn hash_of<T: Hash>(value: &T) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn line_key(path: &str, line: u64, text: &str) -> u64 {
    hash_of(&("line", path, line, text.trim_end_matches('\r')))
}

/// The observations one answered call delivers, as hashes. `path_key` maps
/// a path as the tool reported it to the canonical key used for every tool
/// (so `grep_search` matches and `file_read` lines of one file agree).
pub(crate) fn observation_keys(
    tool: &str,
    args_str: &str,
    result: &str,
    success: bool,
    path_key: &dyn Fn(&str) -> String,
) -> Vec<u64> {
    let generic = || vec![hash_of(&(tool, success, result))];
    if !success {
        return generic();
    }
    let Ok(value) = serde_json::from_str::<Value>(result) else {
        return generic();
    };
    match tool {
        "file_read" => {
            let Some(content) = value.get("content").and_then(Value::as_str) else {
                return generic();
            };
            let args: Value = serde_json::from_str(args_str).unwrap_or_default();
            let Some(raw_path) = ["path", "file_path", "file", "filepath"]
                .iter()
                .find_map(|k| args.get(*k).and_then(Value::as_str))
            else {
                return generic();
            };
            let path = path_key(raw_path);
            if content.is_empty() {
                return vec![hash_of(&("empty", path.as_str()))];
            }
            let numbered = value.get(crate::tools::line_numbers::LINE_NUMBERS_KEY)
                == Some(&Value::Bool(true))
                || crate::tools::line_numbers::is_numbered_text(content);
            let start = args
                .get("line_range")
                .and_then(Value::as_array)
                .and_then(|r| r.first())
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .max(1);
            content
                .lines()
                .enumerate()
                .map(|(i, l)| {
                    if numbered {
                        if let Some(n) = crate::tools::line_numbers::numbered_prefix_len(l) {
                            let number = l[..n].trim().parse::<u64>().unwrap_or(0);
                            return line_key(&path, number, &l[n..]);
                        }
                    }
                    line_key(&path, start + i as u64, l)
                })
                .collect()
        }
        "grep_search" => {
            let Some(matches) = value.get("matches").and_then(Value::as_array) else {
                return generic();
            };
            if matches.is_empty() {
                return generic();
            }
            let mut keys = Vec::new();
            for m in matches {
                let (Some(file), Some(line), Some(text)) = (
                    m.get("file").and_then(Value::as_str),
                    m.get("line").and_then(Value::as_u64),
                    m.get("content").and_then(Value::as_str),
                ) else {
                    return generic();
                };
                let path = path_key(file);
                keys.push(line_key(&path, line, text));
                let strings = |k: &str| -> Vec<String> {
                    m.get(k)
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default()
                };
                let before = strings("context_before");
                let first = line.saturating_sub(before.len() as u64);
                for (i, text) in before.iter().enumerate() {
                    keys.push(line_key(&path, first + i as u64, text));
                }
                for (i, text) in strings("context_after").iter().enumerate() {
                    keys.push(line_key(&path, line + 1 + i as u64, text));
                }
            }
            keys
        }
        _ => generic(),
    }
}

impl FinishStall {
    /// Forget everything (new task).
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    /// Open a dispatched model turn.
    pub(crate) fn begin_turn(&mut self, turn: usize, green: bool, mutation_sequence: usize) {
        self.open = Some(OpenTurn {
            turn,
            green_at_start: green,
            mutation_sequence,
            read_only: true,
            ..OpenTurn::default()
        });
    }

    /// Whether a model turn is open (results outside one are not tracked).
    pub(crate) fn turn_open(&self) -> bool {
        self.open.is_some()
    }

    /// Whether `keys` carry anything not delivered before. A call with no
    /// observation at all counts as new (nothing to compare).
    pub(crate) fn is_novel(&self, keys: &[u64]) -> bool {
        keys.is_empty() || keys.iter().any(|k| !self.seen.contains(k))
    }

    /// Whether this answered call must be refused instead of delivered:
    /// refusal phase, the turn began on the verified tree, the call is
    /// read-only, and it returns nothing new (a check re-run on the green,
    /// unchanged tree included).
    pub(crate) fn should_refuse(&self, mutating: bool, verification: bool, novel: bool) -> bool {
        self.phase == Phase::Refusing
            && !mutating
            && (!novel || verification)
            && self.open.as_ref().is_some_and(|t| t.green_at_start)
    }

    /// Count one refused call.
    pub(crate) fn record_refusal(&mut self) {
        self.refused += 1;
        if let Some(turn) = self.open.as_mut() {
            turn.calls += 1;
        }
    }

    /// Record one answered call of the open turn: its observations become
    /// seen. `passing_check` is the tool name of a verification call that
    /// passed.
    pub(crate) fn record_call(
        &mut self,
        mutating: bool,
        verification: bool,
        passing_check: Option<&str>,
        keys: &[u64],
    ) {
        let novel = self.is_novel(keys);
        self.seen.extend(keys.iter().copied());
        let Some(turn) = self.open.as_mut() else {
            return;
        };
        turn.calls += 1;
        if mutating {
            turn.read_only = false;
        }
        // A check re-run on a tree that was already green and unchanged
        // tells nothing new about it, whatever its output looks like.
        if novel && !(verification && turn.green_at_start) {
            turn.novel = true;
        }
        if let Some(check) = passing_check {
            turn.passing_check = Some(check.to_string());
        }
    }

    /// Close the open turn. `green` / `mutation_sequence` describe the tree
    /// after it.
    pub(crate) fn end_turn(&mut self, green: bool, mutation_sequence: usize) -> TurnVerdict {
        let Some(turn) = self.open.take() else {
            return TurnVerdict::Continue;
        };
        self.last_turn = turn.turn;
        if turn.calls == 0 {
            self.last_turn_stalled = false;
            return TurnVerdict::Continue;
        }
        let tree_moved = mutation_sequence != turn.mutation_sequence;
        if !green {
            // Not (or no longer) verified: nothing to finish yet.
            self.reset_watch();
            self.green = None;
            return TurnVerdict::Continue;
        }
        if tree_moved || !turn.green_at_start || self.green.is_none() {
            // A new verified state: this turn is where it was reached.
            self.reset_watch();
            self.green = Some(GreenPoint {
                turn: turn.turn,
                mutation_sequence,
                check: turn.passing_check,
            });
            return TurnVerdict::Continue;
        }
        self.turns_since_green += 1;
        if !turn.read_only || turn.novel {
            // Attempted a change, or gathered something new: not a stall.
            self.streak = 0;
            self.last_turn_stalled = false;
            return TurnVerdict::Continue;
        }
        self.streak += 1;
        self.stall_turns += 1;
        self.last_turn_stalled = true;
        match self.phase {
            Phase::Watching if self.streak >= FINISH_STALL_TURNS => {
                self.phase = Phase::Nudged;
                self.nudged_at = Some(turn.turn);
                self.streak = 0;
                TurnVerdict::Nudge
            }
            Phase::Nudged if self.streak >= FINISH_STALL_TURNS => {
                self.phase = Phase::Refusing;
                self.refusing_from = Some(turn.turn + 1);
                self.streak = 0;
                TurnVerdict::StartRefusing
            }
            _ => TurnVerdict::Continue,
        }
    }

    fn reset_watch(&mut self) {
        self.phase = Phase::Watching;
        self.streak = 0;
        self.stall_turns = 0;
        self.turns_since_green = 0;
        self.last_turn_stalled = false;
        self.nudged_at = None;
        self.refusing_from = None;
        self.refused = 0;
    }

    /// The current verified point, if the tree is verified and unchanged.
    #[cfg(test)]
    pub(crate) fn green_point(&self) -> Option<&GreenPoint> {
        self.green.as_ref()
    }

    /// True when the budget must not be extended: the tree is verified and
    /// unchanged, and the latest turn only re-read seen content (or the
    /// finish directive was already given).
    pub(crate) fn blocks_budget_extension(&self) -> bool {
        self.green.is_some() && (self.last_turn_stalled || self.phase != Phase::Watching)
    }

    /// Remember that an extension was withheld after the last closed turn;
    /// true the first time (so the caller reports it once).
    pub(crate) fn note_extension_withheld(&mut self) -> bool {
        let first = self.extension_withheld_at.is_none();
        self.extension_withheld_at.get_or_insert(self.last_turn);
        first
    }

    /// Consecutive stall turns in the current phase (tests, logs).
    #[cfg(test)]
    pub(crate) fn streak(&self) -> usize {
        self.streak
    }

    /// Whether the finish directive has been given for this green point.
    #[cfg(test)]
    pub(crate) fn nudged(&self) -> bool {
        self.phase != Phase::Watching
    }

    /// Whether already-seen re-reads are being refused.
    #[cfg(test)]
    pub(crate) fn refusing(&self) -> bool {
        self.phase == Phase::Refusing
    }

    /// Calls refused so far for this green point.
    #[cfg(test)]
    pub(crate) fn refused(&self) -> usize {
        self.refused
    }

    fn check_phrase(green: &GreenPoint) -> String {
        match &green.check {
            Some(check) => format!("{check} passed at turn {}", green.turn),
            None => format!("verification passed at turn {}", green.turn),
        }
    }

    /// One-line outcome clause for a run that ended without a final answer
    /// on a verified, unchanged tree after finish-stall turns; `None`
    /// otherwise (the clause would not be true).
    pub(crate) fn outcome_clause(&self) -> Option<String> {
        let green = self.green.as_ref()?;
        if self.stall_turns == 0 {
            return None;
        }
        let how = match &green.check {
            Some(check) => format!("{check} passed; nothing changed since"),
            None => "nothing changed since".to_string(),
        };
        Some(format!(
            "the work was done and verified at turn {} ({how}) but no final answer was given",
            green.turn
        ))
    }

    /// Detail line for the run summary: what the stall handling did, or
    /// `None` when it never engaged and withheld nothing.
    pub(crate) fn summary_detail(&self) -> Option<String> {
        self.green.as_ref()?;
        // One or two re-reads before answering are normal; the line is for
        // runs where the handling acted (live c24 run 2 printed it for a
        // single re-read before a clean answer).
        if self.nudged_at.is_none() && self.extension_withheld_at.is_none() {
            return None;
        }
        let mut parts = vec![format!(
            "{} of the {} turn(s) after the verified state only re-read content already returned",
            self.stall_turns, self.turns_since_green
        )];
        if let Some(turn) = self.nudged_at {
            parts.push(format!("told to give the final answer after turn {turn}"));
        }
        if let Some(turn) = self.refusing_from {
            parts.push(format!(
                "{} re-read(s) refused from turn {turn}",
                self.refused
            ));
        }
        if let Some(turn) = self.extension_withheld_at {
            parts.push(format!(
                "iteration budget NOT extended at turn {turn} (re-reading a verified, unchanged tree is not progress)"
            ));
        }
        Some(format!("finish stall: {}", parts.join("; ")))
    }

    /// The directive pushed once per green point.
    pub(crate) fn directive(&self, files: &[String]) -> String {
        let green = self.green.as_ref();
        let verified = green
            .map(|g| format!("{} and nothing has changed since", Self::check_phrase(g)))
            .unwrap_or_else(|| "the current tree is verified".to_string());
        let files = if files.is_empty() {
            "your changes are in place".to_string()
        } else {
            format!("changed: {}", files.join(", "))
        };
        format!(
            "<selfware_system_directive>\n\
             {FINISH_STALL_DIRECTIVE_MARKER}: the task's work is done and verified — {files}; \
             {verified}. Your last {FINISH_STALL_TURNS} turns only re-read content you had \
             already been given, so they cannot change the result. Your NEXT reply must be the \
             final answer, with no tool calls: report what you did from what you have already \
             seen. Re-reading unchanged content from here on will be refused.\n\
             </selfware_system_directive>"
        )
    }

    /// The tool result that replaces a refused re-read.
    pub(crate) fn refusal(&self) -> String {
        let verified = self
            .green
            .as_ref()
            .map(|g| {
                format!(
                    "{}; nothing has changed since turn {}",
                    Self::check_phrase(g),
                    g.turn
                )
            })
            .unwrap_or_else(|| "the current tree is verified".to_string());
        serde_json::json!({
            FINISH_STALL_REFUSAL_KEY: true,
            "content_returned": false,
            "note": format!(
                "Not run: this call would only return content you were already given, and the \
                 work is already verified ({verified}). Give the final answer now, with no tool \
                 calls."
            ),
        })
        .to_string()
    }
}

impl super::Agent {
    /// See [`FinishStall::outcome_clause`].
    pub(crate) fn finish_stall_outcome_clause(&self) -> Option<String> {
        self.finish_stall.outcome_clause()
    }

    /// Whether the tree is verified green right now: a mutation happened,
    /// a credited pass covers the current tree (the completion gate's own
    /// freshness), and no in-scope failure blocks it.
    pub(super) fn finish_stall_tree_green(&self) -> bool {
        self.mutation_sequence > 0
            && self.verification_pass_covers_current_tree()
            && self
                .verification_failures
                .blocking(&self.verification_task_root(), self.mutation_sequence)
                .is_none()
    }

    /// Screen one answered call of the open model turn. Returns the refusal
    /// text that replaces the result when the call must be refused, else
    /// records what it delivered and returns `None`.
    ///
    /// `delivered` is the text the model will actually receive for a
    /// successful `file_read` (the first chunk of a whole read that does not
    /// fit), so lines it never received do not count as seen.
    pub(super) fn finish_stall_screen(
        &mut self,
        tool_name: &str,
        args_str: &str,
        success: bool,
        result: &str,
        delivered: Option<&str>,
    ) -> Option<String> {
        if !self.finish_stall.turn_open() {
            return None;
        }
        let args: Value = serde_json::from_str(args_str).unwrap_or(Value::Null);
        let mutating = super::tool_dispatch::tool_call_is_mutating(tool_name, &args);
        let verification = super::tool_dispatch::tool_call_is_verification(tool_name, args_str);
        let text = delivered.unwrap_or(result);
        let keys = observation_keys(tool_name, args_str, text, success, &|p| {
            self.canonical_path_key(p)
        });
        let novel = self.finish_stall.is_novel(&keys);
        if self
            .finish_stall
            .should_refuse(mutating, verification, novel)
        {
            self.finish_stall.record_refusal();
            return Some(self.finish_stall.refusal());
        }
        let passing = (verification && success).then_some(tool_name);
        self.finish_stall
            .record_call(mutating, verification, passing, &keys);
        None
    }

    /// Close the model turn: push the finish directive (once per verified
    /// state) or arm the refusal, as the stall state decides.
    pub(super) fn finish_stall_end_turn(&mut self) {
        if !self.finish_stall.turn_open() {
            return;
        }
        let green = self.finish_stall_tree_green();
        match self.finish_stall.end_turn(green, self.mutation_sequence) {
            TurnVerdict::Continue => {}
            TurnVerdict::Nudge => {
                let files: Vec<String> = self
                    .written_paths()
                    .iter()
                    .take(6)
                    .map(|p| {
                        let root = crate::tools::workspace_root::current_path();
                        p.strip_prefix(&root).unwrap_or(p).display().to_string()
                    })
                    .collect();
                let directive = self.finish_stall.directive(&files);
                tracing::info!(
                    "finish stall: {FINISH_STALL_TURNS} turns re-read seen content on a verified tree — pushing the finish directive"
                );
                self.emit_progress(super::progress::ProgressEvent::GuardFired {
                    kind: "finish_stall_nudge".to_string(),
                    count: 1,
                });
                self.messages
                    .push(crate::api::types::Message::user(directive));
            }
            TurnVerdict::StartRefusing => {
                tracing::info!(
                    "finish stall: no final answer {FINISH_STALL_TURNS} turns after the directive — refusing re-reads of seen content"
                );
                self.emit_progress(super::progress::ProgressEvent::GuardFired {
                    kind: "finish_stall_refusal".to_string(),
                    count: 1,
                });
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/finish_stall/finish_stall_test.rs"]
mod tests;
