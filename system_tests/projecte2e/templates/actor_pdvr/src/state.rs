//! PDVR state machine with transition validation.

use std::fmt;

/// The four phases of the PDVR cognitive cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    Plan,
    Do,
    Verify,
    Reflect,
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Phase::Plan => write!(f, "Plan"),
            Phase::Do => write!(f, "Do"),
            Phase::Verify => write!(f, "Verify"),
            Phase::Reflect => write!(f, "Reflect"),
        }
    }
}

/// Outcome of a phase — determines the next transition.
#[derive(Debug, Clone, PartialEq)]
pub enum PhaseOutcome {
    /// Phase completed successfully, move to next.
    Success(String),
    /// Phase failed, may retry or transition to error handling.
    Failure(String),
    /// Phase needs to go back to a previous phase.
    Retry,
}

/// A record of a completed phase.
#[derive(Debug, Clone)]
pub struct PhaseRecord {
    pub phase: Phase,
    pub outcome: PhaseOutcome,
    pub iteration: usize,
}

/// The state machine that manages PDVR transitions.
pub struct StateMachine {
    current: Phase,
    iteration: usize,
    max_iterations: usize,
    history: Vec<PhaseRecord>,
}

impl StateMachine {
    pub fn new(max_iterations: usize) -> Self {
        StateMachine {
            current: Phase::Plan,
            iteration: 0,
            max_iterations,
            history: Vec::new(),
        }
    }

    /// Get the current phase.
    pub fn current_phase(&self) -> Phase {
        self.current
    }

    /// Get the current iteration number.
    pub fn iteration(&self) -> usize {
        self.iteration
    }

    /// Get the phase transition history.
    pub fn history(&self) -> &[PhaseRecord] {
        &self.history
    }

    /// Check if the state machine has exceeded the max iterations.
    pub fn is_exhausted(&self) -> bool {
        self.iteration >= self.max_iterations
    }

    /// Validate whether a transition from the current phase to `next` is legal.
    ///
    /// Legal transitions in PDVR:
    ///   Plan -> Do
    ///   Do -> Verify
    ///   Verify -> Reflect  (on success)
    ///   Verify -> Do       (on failure — retry the action)
    ///   Reflect -> Plan    (start new cycle)
    ///
    /// BUG 1: Allows Plan -> Verify (skipping Do), which is invalid.
    /// BUG 2: Disallows Verify -> Do (retry on failure), which should be valid.
    pub fn is_valid_transition(&self, next: Phase) -> bool {
        matches!(
            (self.current, next),
            (Phase::Plan, Phase::Do)
                | (Phase::Plan, Phase::Verify) // BUG 1: This should NOT be allowed
                | (Phase::Do, Phase::Verify)
                // BUG 2: Missing (Phase::Verify, Phase::Do) for retry
                | (Phase::Verify, Phase::Reflect)
                | (Phase::Reflect, Phase::Plan)
        )
    }

    /// Attempt to transition to the next phase based on the outcome.
    ///
    /// Returns Ok(new_phase) on success, Err(message) on invalid transition.
    pub fn transition(&mut self, outcome: PhaseOutcome) -> Result<Phase, String> {
        if self.is_exhausted() {
            return Err("max iterations exceeded".to_string());
        }

        let next = match (&self.current, &outcome) {
            (Phase::Plan, PhaseOutcome::Success(_)) => Phase::Do,
            (Phase::Plan, PhaseOutcome::Failure(_)) => Phase::Plan, // Retry planning
            (Phase::Plan, PhaseOutcome::Retry) => Phase::Plan,

            (Phase::Do, PhaseOutcome::Success(_)) => Phase::Verify,
            (Phase::Do, PhaseOutcome::Failure(_)) => Phase::Plan, // Go back to plan
            (Phase::Do, PhaseOutcome::Retry) => Phase::Do,

            (Phase::Verify, PhaseOutcome::Success(_)) => Phase::Reflect,
            (Phase::Verify, PhaseOutcome::Failure(_)) => Phase::Do, // Retry action
            (Phase::Verify, PhaseOutcome::Retry) => Phase::Verify,

            (Phase::Reflect, PhaseOutcome::Success(_)) => Phase::Plan,
            (Phase::Reflect, PhaseOutcome::Failure(_)) => Phase::Plan,
            (Phase::Reflect, PhaseOutcome::Retry) => Phase::Reflect,
        };

        if !self.is_valid_transition(next) {
            return Err(format!(
                "invalid transition: {} -> {} (outcome: {:?})",
                self.current, next, outcome
            ));
        }

        self.history.push(PhaseRecord {
            phase: self.current,
            outcome,
            iteration: self.iteration,
        });

        self.current = next;
        // BUG 3: Iteration only increments on Plan transitions.
        // Should increment on every full cycle (Plan -> Do -> Verify -> Reflect -> Plan).
        // As written, a Verify-failure loop (Do -> Verify -> Do -> Verify) never increments,
        // so max_iterations is never reached.
        if next == Phase::Plan {
            self.iteration += 1;
        }

        Ok(next)
    }

    /// Reset the state machine for a new task.
    pub fn reset(&mut self) {
        self.current = Phase::Plan;
        // BUG 4: Does not reset iteration counter.
        // After reset(), the machine may already be exhausted from a previous task.
        // self.iteration = 0;  // Missing!
        self.history.clear();
    }
}
