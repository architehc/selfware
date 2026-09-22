#[derive(Debug, Clone)]
pub enum AgentState {
    Planning,
    Executing { step: usize },
    ErrorRecovery { error: String },
    Completed,
    Failed { reason: String },
}

impl AgentState {
    fn label(&self) -> &'static str {
        match self {
            AgentState::Planning => "planning",
            AgentState::Executing { .. } => "executing",
            AgentState::ErrorRecovery { .. } => "error_recovery",
            AgentState::Completed => "completed",
            AgentState::Failed { .. } => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidStateTransition {
    from: &'static str,
    to: &'static str,
}

/// Maximum number of adaptive budget extensions a task may earn in place.
/// Each grant is +25% of the ORIGINAL cap (4 × +25% = +100% ceiling); a run
/// that is still productive after the ceiling falls to the auto-continue
/// chain (`maybe_auto_continue` in task_runner.rs).
const MAX_GRANTS: usize = 4;

/// Maximum number of auto-continuations ("chains") a single task may take
/// past its iteration cap. Bounded so a task that keeps reporting progress
/// forever gets a typed stop (`AUTO_CONTINUE_LIMIT`) instead of a
/// pathological infinite chain (USER-APPROVED long-task caps policy).
pub const MAX_AUTO_CONTINUES: usize = 3;

/// The exact terminal reason [`AgentLoop::next_state`] produces when the
/// iteration cap trips. The `--autocontinue` resume policy
/// (`CheckpointManager::latest_autoresumable_task`) matches on this string
/// to distinguish a budget stop (chainable) from every other failure (never
/// auto-chained), so it must stay byte-identical with what the Failed state
/// carries — keep it the single source of truth for that reason.
pub const MAX_ITERATIONS_STOP_REASON: &str = "Max iterations exceeded";

impl std::fmt::Display for InvalidStateTransition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid agent state transition from '{}' to '{}'",
            self.from, self.to
        )
    }
}

impl std::error::Error for InvalidStateTransition {}

pub struct AgentLoop {
    state: AgentState,
    max_iterations: usize,
    /// The cap the task started with — the adaptive extension adds a
    /// fraction of THIS, never of an already-extended budget.
    original_max: usize,
    /// Adaptive extensions granted so far this task. Each grant is +25% of
    /// the ORIGINAL cap; total extension is capped at +100% of the original
    /// (at most 4 grants). Long-horizon tasks (TB4: 5/12 trials died at the
    /// cap while still productive, 2 died even after the old one-shot +50%)
    /// need sustained progress to keep earning budget, not one blind bump.
    extensions_granted: usize,
    /// In-process auto-continuations ("chains") taken on this task, bounded
    /// by [`MAX_AUTO_CONTINUES`]. Each chain resets the iteration budget via
    /// `reset_budget_for_resume` and reuses the exact manual-`resume` path
    /// (`continue_execution`); this counter is the guard that turns a run
    /// which keeps "making progress" forever into a typed stop rather than
    /// an unbounded chain.
    auto_continue_count: usize,
    /// Iterations consumed by EARLIER segments of this task chain. The
    /// per-segment `iteration` counter resets on resume/continuation for
    /// budget fairness; this accumulator keeps the chain-wide total so the
    /// checkpoint (`cumulative_iterations`) and the end-of-run summary
    /// report the whole task's work, not just the final segment.
    prior_iterations: usize,
    current_step: usize,
    iteration: usize,
}

/// One executed tool batch distilled to its progress signal, for the
/// adaptive iteration-budget check (`productive_streak`).
#[derive(Debug, Clone)]
pub struct TurnProgress {
    /// At least one tool result in the turn was not an error.
    pub had_success: bool,
    /// (tool name, args hash) of every attempted call in the turn.
    pub signatures: Vec<(String, u64)>,
}

/// Whether a tool name witnesses a mutation for the [`productive_streak`]
/// duplicate excusal. Name-only on purpose: the progress window stores
/// (name, args HASH), so argument-classified tools (`shell_exec`/`pty_shell`
/// — a `cargo test` vs a `cat > file` are indistinguishable once hashed)
/// conservatively do NOT count as mutation witnesses. Reuses the canonical
/// dispatcher classifier with empty args rather than duplicating its name
/// list; the file- and git-mutation branches are name-only there.
fn signature_is_mutating(tool_name: &str) -> bool {
    super::tool_dispatch::tool_call_is_mutating(tool_name, &serde_json::Value::Null)
}

/// Conservative forward-progress test for the adaptive iteration budget:
/// the last `window` turns must EACH contain at least one non-error tool
/// result, and no tool call (same tool, same args) may repeat without an
/// intervening mutation. Anything less — an error-only turn, a bare re-run
/// of the same call, missing evidence — is NOT progress, and the run dies
/// at the cap.
///
/// Duplicate rule: re-running the SAME verification after an intervening
/// successful mutation is the normal edit→test rhythm of a long task, not
/// a stall — the old "no identical call anywhere in the window" rule
/// rejected every such window, so productive refactors (cargo_test after
/// each edit) never earned an adaptive grant or an auto-continue chain and
/// died at the cap with a typed MAX_ITERATIONS (2026-09-22 long-horizon
/// e2e finding). A repeat is now excused iff a MUTATING call with a
/// different signature appears in the span from the previous occurrence to
/// this one (inclusive of both turns: a batched [edit, test] turn mutates
/// and re-verifies in one step). A repeat with no intervening mutation —
/// the identical probe hammered N times, two reads alternated, the same
/// edit re-applied verbatim — still breaks the streak (fail-closed on true
/// stall/retry loops).
pub fn productive_streak(turns: &std::collections::VecDeque<TurnProgress>, window: usize) -> bool {
    if turns.len() < window {
        return false;
    }
    let window_turns: Vec<&TurnProgress> = turns.iter().skip(turns.len() - window).collect();
    for turn in &window_turns {
        if !turn.had_success || turn.signatures.is_empty() {
            return false;
        }
    }
    for (i, turn) in window_turns.iter().enumerate() {
        for (pos, signature) in turn.signatures.iter().enumerate() {
            // The most recent earlier occurrence of this exact call: in a
            // prior window turn, or earlier in this same turn (a duplicated
            // call inside one batch).
            let previous_turn = (0..i)
                .rev()
                .find(|&j| window_turns[j].signatures.contains(signature));
            let same_turn_repeat = turn.signatures[..pos].contains(signature);
            let Some(anchor) = previous_turn.or(same_turn_repeat.then_some(i)) else {
                continue;
            };
            // Excused only when some turn from the previous occurrence up to
            // this one carried a mutation OTHER than the repeated call
            // itself — the edit that makes re-running the verification new
            // work instead of a retry. The repeated call is excluded as its
            // own witness, so a cycle re-applying the SAME edit args (an
            // edit that changes nothing on re-run) still breaks.
            let excused = (anchor..=i).any(|k| {
                window_turns[k]
                    .signatures
                    .iter()
                    .any(|other| other != signature && signature_is_mutating(&other.0))
            });
            if !excused {
                return false;
            }
        }
    }
    true
}

impl AgentLoop {
    pub fn new(max_iterations: usize) -> Self {
        Self {
            state: AgentState::Planning,
            max_iterations,
            original_max: max_iterations,
            extensions_granted: 0,
            auto_continue_count: 0,
            prior_iterations: 0,
            current_step: 0,
            iteration: 0,
        }
    }

    /// Grant one adaptive budget extension: +25% of the ORIGINAL cap (at
    /// least 1). Extensions may fire multiple times per task — sustained
    /// productivity keeps earning budget — but the total extension never
    /// exceeds +100% of the original cap. Returns the added iterations, or
    /// `None` once the extension ceiling is reached.
    pub fn extend_budget_once(&mut self) -> Option<usize> {
        if self.extensions_granted >= MAX_GRANTS {
            return None;
        }
        self.extensions_granted += 1;
        let added = (self.original_max / 4).max(1);
        self.max_iterations += added;
        Some(added)
    }

    /// Whether the +25%×4 adaptive-extension ceiling has been reached (no
    /// further in-place grant is owed).
    pub fn extension_ceiling_reached(&self) -> bool {
        self.extensions_granted >= MAX_GRANTS
    }

    /// How many adaptive grants this task has consumed so far. Persisted in
    /// the checkpoint so a resume keeps the +25%×4 ceiling accounting
    /// instead of re-earning grants already spent.
    pub fn extensions_granted(&self) -> usize {
        self.extensions_granted
    }

    /// Restore the persisted adaptive-budget state on resume: the effective
    /// cap the task had earned at checkpoint time and the grants already
    /// consumed. Without this a resume rebuilt the loop at the CONFIGURED
    /// cap and silently dropped the earned extension (2026-09-22
    /// long-horizon finding). The cap restores as max(configured,
    /// persisted): an earned extension only ever GROWS the cap, and an
    /// operator re-passing a larger `--max-turns` still wins. The grant
    /// count is clamped to the ceiling so a hand-edited or corrupt
    /// checkpoint cannot mint extra grants.
    pub fn restore_budget_extension(&mut self, persisted_cap: usize, grants: usize) {
        self.max_iterations = self.original_max.max(persisted_cap);
        self.extensions_granted = grants.min(MAX_GRANTS);
    }

    /// Chain-wide iteration count: iterations consumed by earlier segments
    /// of this task (restored on resume, folded at each in-process
    /// continuation) plus the current segment's counter.
    pub fn accumulated_iterations(&self) -> usize {
        self.prior_iterations + self.iteration
    }

    /// Set the iteration total earlier segments of this task chain consumed
    /// (the `Agent::resume` restore path reads it from the checkpoint's
    /// `cumulative_iterations`).
    pub fn set_prior_iterations(&mut self, prior: usize) {
        self.prior_iterations = prior;
    }

    /// How many auto-continuations ("chains") have fired on the current task.
    pub fn auto_continue_count(&self) -> usize {
        self.auto_continue_count
    }

    /// Record one more auto-continuation on this task; returns the new count.
    pub fn register_auto_continue(&mut self) -> usize {
        self.auto_continue_count += 1;
        self.auto_continue_count
    }

    /// Undo a failed auto-continuation registration. Called only when the
    /// chain's boundary checkpoint could not be persisted — the chain never
    /// ran, so it must not consume the per-task chain budget.
    pub fn unregister_auto_continue(&mut self) {
        self.auto_continue_count = self.auto_continue_count.saturating_sub(1);
    }

    /// Restore the auto-continue chain count persisted at checkpoint time
    /// (the `Agent::resume` path) so the per-task chain bound survives a
    /// process restart instead of granting a fresh budget of chains.
    pub fn set_auto_continue_count(&mut self, count: usize) {
        self.auto_continue_count = count;
    }

    /// Reset the iteration budget exactly as a manual `Agent::resume` would
    /// re-create it (`AgentLoop::new` + `set_prior_iterations` +
    /// `restore_progress(step, 0)`): iteration returns to 0, the cap returns
    /// to the ORIGINAL configured value, and the adaptive-extension budget is
    /// fresh — while the step counter keeps counting, the chain-wide
    /// iteration total folds the closing segment in, and the state returns
    /// to `Executing`. The in-process auto-continue chain mirrors this, so a
    /// chained segment is indistinguishable from a resumed one. The
    /// auto-continue counter is deliberately NOT reset: it bounds the whole
    /// task, not one segment.
    pub fn reset_budget_for_resume(&mut self) {
        self.prior_iterations += self.iteration;
        self.iteration = 0;
        self.max_iterations = self.original_max;
        self.extensions_granted = 0;
        self.state = AgentState::Executing {
            step: self.current_step,
        };
    }

    pub fn next_state(&mut self) -> Option<AgentState> {
        // Planning does not consume an iteration slot — only non-Planning
        // states increment the counter. This gives the caller
        // `max_iterations` execution turns in addition to the initial
        // Planning turn.
        if !matches!(self.state, AgentState::Planning) {
            self.iteration += 1;
        }
        if self.iteration > self.max_iterations {
            self.state = AgentState::Failed {
                reason: MAX_ITERATIONS_STOP_REASON.to_string(),
            };
            return Some(self.state.clone());
        }
        Some(self.state.clone())
    }

    /// Returns a warning message when the loop is approaching the iteration
    /// limit.  Intended to be injected as a system message so the LLM can
    /// wrap up gracefully instead of being cut off abruptly.
    ///
    /// Returns `Some` at 80% and 90%+ of `max_iterations`, `None` otherwise.
    pub fn approaching_limit_warning(&self) -> Option<String> {
        if self.max_iterations == 0 {
            return None;
        }
        let remaining = self.max_iterations.saturating_sub(self.iteration);
        let pct_used = (self.iteration * 100) / self.max_iterations;

        if pct_used >= 90 {
            Some(format!(
                "[SYSTEM] Only {} iteration(s) remaining out of {}. Wrap up your current work and provide a final answer now.",
                remaining, self.max_iterations
            ))
        } else if pct_used >= 80 {
            Some(format!(
                "[SYSTEM] Approaching iteration limit: {} of {} iterations used ({} remaining). Start wrapping up.",
                self.iteration, self.max_iterations, remaining
            ))
        } else {
            None
        }
    }

    /// The iteration-limit warning band currently reached: 0 (none), 80, or 90.
    /// Lets the loop push each threshold warning exactly once instead of every
    /// iteration past 80% (which accumulated duplicate system messages —
    /// found by GLM-5.2 reviewing task_runner.rs).
    pub fn approaching_limit_band(&self) -> u8 {
        if self.max_iterations == 0 {
            return 0;
        }
        let pct = (self.iteration * 100) / self.max_iterations;
        if pct >= 90 {
            90
        } else if pct >= 80 {
            80
        } else {
            0
        }
    }

    fn is_valid_transition(current: &AgentState, next: &AgentState) -> bool {
        matches!(
            (current, next),
            (AgentState::Planning, AgentState::Executing { .. })
                | (AgentState::Planning, AgentState::ErrorRecovery { .. })
                | (AgentState::Planning, AgentState::Failed { .. })
                | (AgentState::Executing { .. }, AgentState::Executing { .. })
                | (
                    AgentState::Executing { .. },
                    AgentState::ErrorRecovery { .. }
                )
                | (AgentState::Executing { .. }, AgentState::Completed)
                | (AgentState::Executing { .. }, AgentState::Failed { .. })
                | (
                    AgentState::ErrorRecovery { .. },
                    AgentState::Executing { .. }
                )
                | (AgentState::ErrorRecovery { .. }, AgentState::Failed { .. })
        )
    }

    pub fn transition_to(
        &mut self,
        state: AgentState,
    ) -> std::result::Result<(), InvalidStateTransition> {
        if !Self::is_valid_transition(&self.state, &state) {
            return Err(InvalidStateTransition {
                from: self.state.label(),
                to: state.label(),
            });
        }
        self.state = state;
        Ok(())
    }

    pub fn set_state(&mut self, state: AgentState) {
        self.transition_to(state)
            .expect("invalid agent state transition");
    }

    pub fn increment_step(&mut self) -> std::result::Result<(), InvalidStateTransition> {
        self.current_step += 1;
        self.transition_to(AgentState::Executing {
            step: self.current_step,
        })
    }

    pub fn current_step(&self) -> usize {
        self.current_step
    }

    pub fn current_iteration(&self) -> usize {
        self.iteration
    }

    /// The current iteration cap, including any adaptive extension.
    pub fn max_iterations(&self) -> usize {
        self.max_iterations
    }

    /// Whether any adaptive budget extension fired this task.
    pub fn extension_was_used(&self) -> bool {
        self.extensions_granted > 0
    }

    pub fn current_state_label(&self) -> &'static str {
        self.state.label()
    }

    /// Restore loop progress from persisted state.
    pub fn restore_progress(&mut self, step: usize, iteration: usize) {
        self.current_step = step;
        self.iteration = iteration;
        self.state = AgentState::Executing { step };
    }

    /// Reset loop state for a new task, preserving max_iterations.
    ///
    /// Without this, queued tasks share the iteration counter from the previous
    /// task and may hit the max-iterations limit prematurely.
    pub fn reset_for_task(&mut self) {
        self.state = AgentState::Planning;
        self.current_step = 0;
        self.iteration = 0;
        self.prior_iterations = 0;
        // A new task gets a fresh budget: extensions are per-task.
        self.max_iterations = self.original_max;
        self.extensions_granted = 0;
        // And a fresh auto-continue chain budget (bounded chains are per-task).
        self.auto_continue_count = 0;
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/loop_control/loop_control_test.rs"]
mod tests;
