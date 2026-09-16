//! Prefix-Observable Search Policy Engine
//!
//! Implements search policies that govern attempt allocation across exploration
//! roots, branch refinements, and failure recoveries.
//!
//! Adheres strictly to the Dream-RSI prefix-observability invariant:
//! policies make decisions based SOLELY on historical observations revealed so far,
//! without leaking future scores, unrevealed tree nodes, or task outcomes.

use super::tree_log::{AttemptStatus, FailureClass};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// A single revealed observation visible to the search policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrefixObservation {
    /// Attempt ID.
    pub id: String,
    /// Semantic branch ID.
    pub branch_id: String,
    /// Depth within this branch (0 for root).
    pub attempt_depth: usize,
    /// Parent attempt ID, if any.
    pub parent_id: Option<String>,
    /// Measured composite score, if successfully evaluated.
    pub score: Option<f64>,
    /// Status of the attempt.
    pub status: AttemptStatus,
    /// Failure class, if not Evaluated.
    pub failure_class: Option<FailureClass>,
    /// Failure message or error snippet, if any.
    pub failure_reason: Option<String>,
    /// Improvement delta vs the baseline score.
    pub delta_vs_baseline: Option<f64>,
    /// Improvement delta vs the direct parent node.
    pub delta_vs_parent: Option<f64>,
    /// Tokens consumed, if measured.
    pub tokens_used: Option<u64>,
    /// Wall-clock evaluation time in ms.
    pub wall_time_ms: u64,
}

impl PrefixObservation {
    /// Returns true if this observation represents a successful evaluation.
    pub fn is_successful(&self) -> bool {
        self.status == AttemptStatus::Evaluated && self.score.is_some()
    }

    /// Returns true if this observation represents a repairable failure.
    pub fn is_repairable(&self) -> bool {
        self.failure_class
            .as_ref()
            .is_some_and(|fc| fc.is_repairable())
    }
}

/// Reconstructed trajectory of a single branch over its revealed prefix.
#[derive(Debug, Clone)]
pub struct BranchTrajectory {
    pub branch_id: String,
    pub observations: Vec<PrefixObservation>,
    /// Best historical score from a successful evaluation on this branch.
    pub successful_anchor: Option<f64>,
    /// Number of consecutive failures since the last successful evaluation.
    pub consecutive_failures: usize,
    /// Whether the latest frontier node suffered a repairable failure.
    pub has_repairable_frontier: bool,
    /// Whether this branch suffered a hard, unrecoverable failure.
    pub is_hard_failed: bool,
}

impl BranchTrajectory {
    /// Compute branch trajectory statistics from its ordered observations.
    pub fn from_observations(branch_id: String, obs: Vec<PrefixObservation>) -> Self {
        let mut anchor: Option<f64> = None;
        let mut consecutive_failures = 0;
        let mut has_repairable_frontier = false;
        let mut is_hard_failed = false;

        for o in &obs {
            if o.is_successful() {
                if let Some(s) = o.score {
                    anchor = Some(anchor.map_or(s, |a| a.max(s)));
                }
                consecutive_failures = 0;
                has_repairable_frontier = false;
                is_hard_failed = false;
            } else {
                consecutive_failures += 1;
                has_repairable_frontier = o.is_repairable();
                is_hard_failed = !o.is_repairable();
            }
        }

        Self {
            branch_id,
            observations: obs,
            successful_anchor: anchor,
            consecutive_failures,
            has_repairable_frontier,
            is_hard_failed,
        }
    }

    /// Depth of the branch (number of attempts revealed so far).
    pub fn depth(&self) -> usize {
        self.observations.len()
    }

    /// Latest observation on this branch.
    pub fn latest(&self) -> Option<&PrefixObservation> {
        self.observations.last()
    }
}

/// Read-only snapshot of all revealed observations available at a decision point.
#[derive(Debug, Clone)]
pub struct PrefixView {
    pub observations: Vec<PrefixObservation>,
    pub baseline_score: f64,
    pub max_parallelism: usize,
}

impl PrefixView {
    /// Create a new prefix view.
    pub fn new(
        observations: Vec<PrefixObservation>,
        baseline_score: f64,
        max_parallelism: usize,
    ) -> Self {
        Self {
            observations,
            baseline_score,
            max_parallelism,
        }
    }

    /// Total probes revealed so far.
    pub fn total_probes(&self) -> usize {
        self.observations.len()
    }

    /// Highest composite score observed among all revealed nodes.
    pub fn best_score_so_far(&self) -> f64 {
        self.observations
            .iter()
            .filter_map(|o| o.score)
            .fold(self.baseline_score, f64::max)
    }

    /// Reconstruct the trajectory for a specific branch.
    pub fn branch_trajectory(&self, branch_id: &str) -> BranchTrajectory {
        let branch_obs: Vec<PrefixObservation> = self
            .observations
            .iter()
            .filter(|o| o.branch_id == branch_id)
            .cloned()
            .collect();
        BranchTrajectory::from_observations(branch_id.to_string(), branch_obs)
    }

    /// Reconstruct trajectories for all opened branches.
    pub fn all_branch_trajectories(&self) -> HashMap<String, BranchTrajectory> {
        let mut grouped: HashMap<String, Vec<PrefixObservation>> = HashMap::new();
        for o in &self.observations {
            grouped
                .entry(o.branch_id.clone())
                .or_default()
                .push(o.clone());
        }

        grouped
            .into_iter()
            .map(|(bid, obs)| {
                let traj = BranchTrajectory::from_observations(bid.clone(), obs);
                (bid, traj)
            })
            .collect()
    }

    /// Best historical score on a specific branch.
    pub fn successful_anchor(&self, branch_id: &str) -> Option<f64> {
        self.observations
            .iter()
            .filter(|o| o.branch_id == branch_id && o.is_successful())
            .filter_map(|o| o.score)
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Whether a branch is promising (has anchor > baseline or is underexplored).
    pub fn is_branch_promising(&self, branch_id: &str) -> bool {
        let traj = self.branch_trajectory(branch_id);
        if traj.is_hard_failed {
            return false;
        }
        match traj.successful_anchor {
            Some(anchor) => anchor >= self.baseline_score && traj.consecutive_failures < 3,
            None => traj.depth() < 2, // Still underexplored
        }
    }
}

/// A legal action candidate presented to the search policy.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LegalAction {
    /// Open a new unparented root exploration candidate.
    OpenRoot { branch_id: String, node_id: String },
    /// Refine an existing revealed attempt frontier.
    RefineFrontier {
        branch_id: String,
        parent_id: String,
        node_id: String,
    },
}

impl LegalAction {
    /// Target node ID of this action.
    pub fn node_id(&self) -> &str {
        match self {
            LegalAction::OpenRoot { node_id, .. } => node_id,
            LegalAction::RefineFrontier { node_id, .. } => node_id,
        }
    }

    /// Target branch ID of this action.
    pub fn branch_id(&self) -> &str {
        match self {
            LegalAction::OpenRoot { branch_id, .. } => branch_id,
            LegalAction::RefineFrontier { branch_id, .. } => branch_id,
        }
    }

    /// Whether this action opens a new root.
    pub fn is_root(&self) -> bool {
        matches!(self, LegalAction::OpenRoot { .. })
    }
}

/// Decision made by a search policy at each round.
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyDecision {
    /// Select a batch of independent actions (at most `max_parallelism`, no duplicates).
    SelectBatch(Vec<LegalAction>),
    /// Terminate exploration with an explicit reason.
    Stop { reason: String },
}

/// Trait implemented by attempt allocation and search policies.
pub trait SearchPolicy: std::fmt::Debug + Send + Sync {
    /// Unique policy name.
    fn name(&self) -> &str;

    /// Select the next batch of actions or decide to stop, based strictly on the prefix view.
    fn decide(
        &mut self,
        prefix: &PrefixView,
        legal_actions: &[LegalAction],
        beta: f64,
    ) -> PolicyDecision;
}

// ─────────────────────────────────────────────────────────────────────────────
// BASELINE POLICIES
// ─────────────────────────────────────────────────────────────────────────────

/// Breadth-First Policy:
/// Exhaustively opens roots first up to `max_parallelism`, then expands shallow
/// single-step refinements across each branch up to `max_depth`.
#[derive(Debug, Clone)]
pub struct BreadthFirstPolicy {
    pub max_depth: usize,
}

impl BreadthFirstPolicy {
    pub fn new(max_depth: usize) -> Self {
        Self { max_depth }
    }
}

impl Default for BreadthFirstPolicy {
    fn default() -> Self {
        Self { max_depth: 2 }
    }
}

impl SearchPolicy for BreadthFirstPolicy {
    fn name(&self) -> &str {
        "BreadthFirstPolicy"
    }

    fn decide(
        &mut self,
        prefix: &PrefixView,
        legal_actions: &[LegalAction],
        _beta: f64,
    ) -> PolicyDecision {
        if legal_actions.is_empty() {
            return PolicyDecision::Stop {
                reason: "No legal actions available".into(),
            };
        }

        // Prefer unopened roots first
        let roots: Vec<LegalAction> = legal_actions
            .iter()
            .filter(|a| a.is_root())
            .cloned()
            .collect();

        if !roots.is_empty() {
            let mut batch = Vec::new();
            let mut selected_branches = HashSet::new();

            for action in roots {
                if batch.len() >= prefix.max_parallelism {
                    break;
                }
                if selected_branches.insert(action.branch_id().to_string()) {
                    batch.push(action);
                }
            }

            if !batch.is_empty() {
                return PolicyDecision::SelectBatch(batch);
            }
        }

        // Otherwise select shallow refinements
        let mut batch = Vec::new();
        let mut selected_branches = HashSet::new();

        for action in legal_actions {
            if batch.len() >= prefix.max_parallelism {
                break;
            }
            let traj = prefix.branch_trajectory(action.branch_id());
            if traj.depth() < self.max_depth
                && selected_branches.insert(action.branch_id().to_string())
            {
                batch.push(action.clone());
            }
        }

        if batch.is_empty() {
            PolicyDecision::Stop {
                reason: format!("All branches reached maximum depth {}", self.max_depth),
            }
        } else {
            PolicyDecision::SelectBatch(batch)
        }
    }
}

/// Refine-Top1 (Greedy Depth) Policy:
/// Evaluates roots, then locks onto the highest scoring branch and greedily
/// refines it until stagnation or completion.
#[derive(Debug, Clone)]
pub struct RefineTop1Policy {
    pub patience: usize,
    target_branch: Option<String>,
    stagnation_rounds: usize,
    last_best_score: f64,
}

impl RefineTop1Policy {
    pub fn new(patience: usize) -> Self {
        Self {
            patience,
            target_branch: None,
            stagnation_rounds: 0,
            last_best_score: 0.0,
        }
    }
}

impl Default for RefineTop1Policy {
    fn default() -> Self {
        Self::new(3)
    }
}

impl SearchPolicy for RefineTop1Policy {
    fn name(&self) -> &str {
        "RefineTop1Policy"
    }

    fn decide(
        &mut self,
        prefix: &PrefixView,
        legal_actions: &[LegalAction],
        _beta: f64,
    ) -> PolicyDecision {
        if legal_actions.is_empty() {
            return PolicyDecision::Stop {
                reason: "No legal actions available".into(),
            };
        }

        // If target branch is not yet selected, explore roots
        if self.target_branch.is_none() {
            let roots: Vec<LegalAction> = legal_actions
                .iter()
                .filter(|a| a.is_root())
                .cloned()
                .collect();

            if !roots.is_empty() {
                let batch: Vec<LegalAction> =
                    roots.into_iter().take(prefix.max_parallelism).collect();
                return PolicyDecision::SelectBatch(batch);
            }

            // Roots are done: find the top branch by successful anchor
            let trajs = prefix.all_branch_trajectories();
            let best_branch = trajs
                .into_iter()
                .filter_map(|(bid, t)| t.successful_anchor.map(|a| (bid, a)))
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

            match best_branch {
                Some((bid, score)) => {
                    self.target_branch = Some(bid.clone());
                    self.last_best_score = score;
                }
                None => {
                    return PolicyDecision::Stop {
                        reason: "No branch achieved a successful evaluation".into(),
                    };
                }
            }
        }

        // Refine the chosen target branch
        let target_bid = self.target_branch.as_ref().unwrap();
        let target_action = legal_actions.iter().find(|a| a.branch_id() == target_bid);

        match target_action {
            Some(act) => {
                let current_score = prefix.successful_anchor(target_bid).unwrap_or(0.0);
                if current_score > self.last_best_score + 1e-4 {
                    self.last_best_score = current_score;
                    self.stagnation_rounds = 0;
                } else {
                    self.stagnation_rounds += 1;
                }

                if self.stagnation_rounds > self.patience {
                    PolicyDecision::Stop {
                        reason: format!(
                            "Target branch {} stagnated for {} rounds",
                            target_bid, self.stagnation_rounds
                        ),
                    }
                } else {
                    PolicyDecision::SelectBatch(vec![act.clone()])
                }
            }
            None => PolicyDecision::Stop {
                reason: format!("Target branch {} reached terminal depth", target_bid),
            },
        }
    }
}

/// Pareto-Adaptive Policy (Dream-RSI):
/// Dynamically composes a parallel portfolio combining:
/// 1. Exploitation: refining strong active branches with high anchors and positive deltas.
/// 2. Exploration: opening new roots or underexplored branches.
/// 3. Bounded Recovery: allocating at most 1 slot to repairable implementation failures
///    (anti-over-closing: syntax slips or minor test failures do not kill a promising branch).
///
/// Behavior is modulated by `beta` (higher beta = deeper patience, broader width).
#[derive(Debug, Clone)]
pub struct ParetoAdaptivePolicy {
    pub max_patience: usize,
    stagnant_cycles: usize,
    best_observed: f64,
}

impl ParetoAdaptivePolicy {
    pub fn new() -> Self {
        Self {
            max_patience: 4,
            stagnant_cycles: 0,
            best_observed: 0.0,
        }
    }
}

impl Default for ParetoAdaptivePolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchPolicy for ParetoAdaptivePolicy {
    fn name(&self) -> &str {
        "ParetoAdaptivePolicy"
    }

    fn decide(
        &mut self,
        prefix: &PrefixView,
        legal_actions: &[LegalAction],
        beta: f64,
    ) -> PolicyDecision {
        if legal_actions.is_empty() {
            return PolicyDecision::Stop {
                reason: "No legal actions available".into(),
            };
        }

        let current_best = prefix.best_score_so_far();
        if current_best > self.best_observed + 1e-4 {
            self.best_observed = current_best;
            self.stagnant_cycles = 0;
        } else if prefix.total_probes() > 0 {
            self.stagnant_cycles += 1;
        }

        // Stagnation budget scaled by beta
        let allowed_stagnation = if beta >= 0.7 {
            self.max_patience + 2
        } else if beta <= 0.3 {
            self.max_patience.saturating_sub(1).max(1)
        } else {
            self.max_patience
        };

        if self.stagnant_cycles >= allowed_stagnation {
            return PolicyDecision::Stop {
                reason: format!(
                    "Global plateau reached after {} stagnant cycles (beta: {:.2})",
                    self.stagnant_cycles, beta
                ),
            };
        }

        let max_failures_allowed = if beta >= 0.7 {
            3
        } else if beta <= 0.3 {
            1
        } else {
            2
        };

        let trajs = prefix.all_branch_trajectories();
        let mut batch: Vec<LegalAction> = Vec::new();
        let mut selected_branches: HashSet<String> = HashSet::new();

        // ── 1. Recovery candidate (at most 1 slot) ──
        // Check for branches with repairable failures that had a promising anchor
        let mut recovery_action: Option<LegalAction> = None;
        for action in legal_actions {
            let bid = action.branch_id();
            if let Some(t) = trajs.get(bid) {
                if t.has_repairable_frontier
                    && t.consecutive_failures <= max_failures_allowed
                    && (t.successful_anchor.is_some() || t.depth() <= 2)
                {
                    recovery_action = Some(action.clone());
                    break;
                }
            }
        }

        if let Some(act) = recovery_action {
            selected_branches.insert(act.branch_id().to_string());
            batch.push(act);
        }

        // ── 2. Exploitation candidates (high anchor, positive trajectory) ──
        let mut exploit_candidates: Vec<(&LegalAction, f64)> = Vec::new();
        for action in legal_actions {
            let bid = action.branch_id();
            if selected_branches.contains(bid) {
                continue;
            }
            if let Some(t) = trajs.get(bid) {
                if !t.is_hard_failed && t.consecutive_failures < max_failures_allowed {
                    if let Some(anchor) = t.successful_anchor {
                        exploit_candidates.push((action, anchor));
                    }
                }
            }
        }

        // Sort by anchor descending
        exploit_candidates
            .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        for (action, _) in exploit_candidates {
            if batch.len() >= prefix.max_parallelism {
                break;
            }
            if selected_branches.insert(action.branch_id().to_string()) {
                batch.push(action.clone());
            }
        }

        // ── 3. Exploration candidates (unopened roots or underexplored branches) ──
        let mut explore_candidates: Vec<&LegalAction> = legal_actions
            .iter()
            .filter(|a| !selected_branches.contains(a.branch_id()))
            .filter(|a| {
                if a.is_root() {
                    true
                } else {
                    let depth = trajs.get(a.branch_id()).map_or(0, |t| t.depth());
                    depth < 2
                }
            })
            .collect();

        // Sort roots first
        explore_candidates.sort_by_key(|a| if a.is_root() { 0 } else { 1 });

        for action in explore_candidates {
            if batch.len() >= prefix.max_parallelism {
                break;
            }
            if selected_branches.insert(action.branch_id().to_string()) {
                batch.push(action.clone());
            }
        }

        if batch.is_empty() {
            PolicyDecision::Stop {
                reason: "No eligible exploitation, exploration, or recovery actions remaining"
                    .into(),
            }
        } else {
            PolicyDecision::SelectBatch(batch)
        }
    }
}

/// Early-Stop Plateau Policy Decorator:
/// Wraps an underlying policy and automatically terminates if the best score
/// has not improved by at least `min_delta` over `patience` rounds.
#[derive(Debug)]
pub struct EarlyStopPlateauPolicy {
    pub inner: Box<dyn SearchPolicy>,
    pub patience: usize,
    pub min_delta: f64,
    stagnant_count: usize,
    last_best: f64,
}

impl EarlyStopPlateauPolicy {
    pub fn new(inner: Box<dyn SearchPolicy>, patience: usize, min_delta: f64) -> Self {
        Self {
            inner,
            patience,
            min_delta,
            stagnant_count: 0,
            last_best: 0.0,
        }
    }
}

impl SearchPolicy for EarlyStopPlateauPolicy {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn decide(
        &mut self,
        prefix: &PrefixView,
        legal_actions: &[LegalAction],
        beta: f64,
    ) -> PolicyDecision {
        let current_best = prefix.best_score_so_far();
        if current_best > self.last_best + self.min_delta {
            self.last_best = current_best;
            self.stagnant_count = 0;
        } else if prefix.total_probes() > 0 {
            self.stagnant_count += 1;
        }

        if self.stagnant_count >= self.patience {
            PolicyDecision::Stop {
                reason: format!(
                    "Terminating early: no improvement > {:.4} for {} rounds",
                    self.min_delta, self.stagnant_count
                ),
            }
        } else {
            self.inner.decide(prefix, legal_actions, beta)
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/evolution/policy/policy_test.rs"]
mod tests;
