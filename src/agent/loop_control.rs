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
    /// The one-shot adaptive extension has been granted.
    extension_used: bool,
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

/// Conservative forward-progress test for the adaptive iteration budget:
/// the last `window` turns must EACH contain at least one non-error tool
/// result, and no identical tool call (same tool, same args) may repeat
/// anywhere in the window. Anything less — an error-only turn, a repeated
/// call, missing evidence — is NOT progress, and the run dies at the cap.
pub fn productive_streak(turns: &std::collections::VecDeque<TurnProgress>, window: usize) -> bool {
    if turns.len() < window {
        return false;
    }
    let mut seen: std::collections::HashSet<&(String, u64)> = std::collections::HashSet::new();
    for turn in turns.iter().rev().take(window) {
        if !turn.had_success || turn.signatures.is_empty() {
            return false;
        }
        for signature in &turn.signatures {
            if !seen.insert(signature) {
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
            extension_used: false,
            current_step: 0,
            iteration: 0,
        }
    }

    /// Grant the one-shot adaptive budget extension: +50% of the ORIGINAL
    /// cap (at least 1), at most once per task. Returns the added
    /// iterations, or `None` when the extension was already used.
    pub fn extend_budget_once(&mut self) -> Option<usize> {
        if self.extension_used {
            return None;
        }
        self.extension_used = true;
        let added = (self.original_max / 2).max(1);
        self.max_iterations += added;
        Some(added)
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
                reason: "Max iterations exceeded".to_string(),
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
        // A new task gets a fresh budget: the extension is per-task.
        self.max_iterations = self.original_max;
        self.extension_used = false;
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/loop_control/loop_control_test.rs"]
mod tests;
