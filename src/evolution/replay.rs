//! Deterministic Offline Replay Simulator
//!
//! Evaluates search policies cheaply and deterministically against recorded attempt
//! trees without invoking compilers, test suites, sandboxes, or LLMs.
//!
//! Based on the Dream-RSI historical replay paradigm (arXiv:2609.14858):
//! - The simulator presents the search policy with revealed prefix observations only.
//! - The policy selects candidate batches from legal root and frontier actions.
//! - Replay measures terminal quality, work (probes / tokens / latency), and parallelism.
//! - Policies are ranked by objective: J(π) = V_terminal(π) - β * Cost(π).

use super::policy::{LegalAction, PolicyDecision, PrefixObservation, PrefixView, SearchPolicy};
use super::tree_log::AttemptTree;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Errors that can occur during offline replay simulation.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReplayError {
    #[error("Batch size {size} exceeds maximum parallelism {max}")]
    BatchExceedsParallelism { size: usize, max: usize },
    #[error("Policy selected illegal action: {0}")]
    IllegalAction(String),
    #[error("Policy returned duplicate action in batch: {0}")]
    DuplicateActionInBatch(String),
    #[error("Policy returned conflicting actions on the same branch in a single batch: {0}")]
    DuplicateBranchInBatch(String),
    #[error("Replay exceeded maximum round cap of {0}")]
    PolicyLoopExceeded(usize),
}

/// Comprehensive outcome report from an offline replay evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayReport {
    /// Name of the evaluated policy.
    pub policy_name: String,
    /// Exploration trade-off knob (beta) used during replay.
    pub beta: f64,
    /// Starting baseline score.
    pub baseline_score: f64,
    /// Highest composite score attained across revealed nodes.
    pub terminal_score: f64,
    /// Improvement delta (terminal - baseline).
    pub score_improvement: f64,
    /// Total probes (evaluations) spent by the policy.
    pub total_probes: usize,
    /// Total decision rounds taken by the policy.
    pub decision_rounds: usize,
    /// Effective sequential rounds: sum of ceil(batch_size / parallelism).
    pub effective_sequential_rounds: usize,
    /// Parallel penalty: mean sequential rounds per probe. Lower is better.
    pub parallel_penalty: f64,
    /// Simulated total wall-clock time in ms.
    pub total_wall_time_ms: u64,
    /// Total tokens consumed across probed attempts.
    pub total_tokens: u64,
    /// Objective function value: J(π) = V_terminal - β * (probes / total_nodes).
    pub objective_value: f64,
    /// Pareto reward: improvement - λ * parallel_penalty.
    pub pareto_reward: f64,
    /// Reason given for terminating exploration.
    pub stop_reason: String,
    /// Ordered list of node IDs revealed during the replay.
    pub revealed_node_ids: Vec<String>,
}

/// Deterministic replay simulator executing search policies over an `AttemptTree`.
#[derive(Debug, Clone)]
pub struct ReplaySimulator {
    pub tree: AttemptTree,
    pub baseline_score: f64,
    pub max_parallelism: usize,
    pub max_rounds: usize,
    pub lambda: f64,
}

impl ReplaySimulator {
    /// Create a new replay simulator for an attempt tree and baseline score.
    pub fn new(tree: AttemptTree, baseline_score: f64) -> Self {
        Self {
            tree,
            baseline_score,
            max_parallelism: 4,
            max_rounds: 1000,
            lambda: 0.1,
        }
    }

    /// Set maximum parallel worker capacity (W).
    pub fn with_max_parallelism(mut self, parallelism: usize) -> Self {
        self.max_parallelism = parallelism.max(1);
        self
    }

    /// Set safety cap on maximum decision rounds.
    pub fn with_max_rounds(mut self, max_rounds: usize) -> Self {
        self.max_rounds = max_rounds;
        self
    }

    /// Set lambda weight for parallel penalty in Pareto reward calculation.
    pub fn with_lambda(mut self, lambda: f64) -> Self {
        self.lambda = lambda;
        self
    }

    /// Run offline replay for a search policy under a specific beta knob.
    pub fn evaluate_policy(
        &self,
        policy: &mut dyn SearchPolicy,
        beta: f64,
    ) -> Result<ReplayReport, ReplayError> {
        let mut revealed_ids: HashSet<String> = HashSet::new();
        let mut revealed_order: Vec<String> = Vec::new();
        let mut decision_rounds = 0;
        let mut effective_sequential_rounds = 0;
        let mut total_wall_time_ms: u64 = 0;
        let mut total_tokens: u64 = 0;
        let mut stop_reason = String::from("Exhausted legal actions");

        while decision_rounds < self.max_rounds {
            // Determine legal actions strictly from revealed prefix
            let legal_actions = self.compute_legal_actions(&revealed_ids);
            if legal_actions.is_empty() {
                stop_reason = "No legal actions remaining in tree".into();
                break;
            }

            // Construct prefix view with revealed observations
            let prefix_obs = self.build_prefix_observations(&revealed_order);
            let prefix = PrefixView::new(prefix_obs, self.baseline_score, self.max_parallelism);

            // Policy decides next action batch
            let decision = policy.decide(&prefix, &legal_actions, beta);

            match decision {
                PolicyDecision::Stop { reason } => {
                    stop_reason = reason;
                    break;
                }
                PolicyDecision::SelectBatch(batch) => {
                    if batch.is_empty() {
                        stop_reason = "Policy returned empty batch".into();
                        break;
                    }

                    if batch.len() > self.max_parallelism {
                        return Err(ReplayError::BatchExceedsParallelism {
                            size: batch.len(),
                            max: self.max_parallelism,
                        });
                    }

                    // Validate batch validity and independence
                    let legal_set: HashSet<&str> =
                        legal_actions.iter().map(|a| a.node_id()).collect();
                    let mut batch_nodes = HashSet::new();
                    let mut batch_branches = HashSet::new();

                    for action in &batch {
                        let nid = action.node_id();
                        let bid = action.branch_id();

                        if !legal_set.contains(nid) {
                            return Err(ReplayError::IllegalAction(nid.to_string()));
                        }
                        if !batch_nodes.insert(nid) {
                            return Err(ReplayError::DuplicateActionInBatch(nid.to_string()));
                        }
                        if !batch_branches.insert(bid) {
                            return Err(ReplayError::DuplicateBranchInBatch(bid.to_string()));
                        }
                    }

                    // Reveal batch
                    let mut batch_max_wall_ms: u64 = 0;
                    for action in &batch {
                        let nid = action.node_id();
                        revealed_ids.insert(nid.to_string());
                        revealed_order.push(nid.to_string());

                        if let Some(node) = self.tree.get(nid) {
                            batch_max_wall_ms = batch_max_wall_ms.max(node.wall_time_ms);
                            if let Some(tok) = node.tokens_used {
                                total_tokens += tok;
                            }
                        }
                    }

                    total_wall_time_ms += batch_max_wall_ms;
                    decision_rounds += 1;
                    let seq_rounds = batch.len().div_ceil(self.max_parallelism);
                    effective_sequential_rounds += seq_rounds;
                }
            }
        }

        if decision_rounds >= self.max_rounds {
            return Err(ReplayError::PolicyLoopExceeded(self.max_rounds));
        }

        // Calculate outcomes
        let total_probes = revealed_order.len();
        let terminal_score = revealed_order
            .iter()
            .filter_map(|id| self.tree.get(id))
            .filter_map(|n| n.composite_score)
            .fold(self.baseline_score, f64::max);

        let score_improvement = terminal_score - self.baseline_score;
        let parallel_penalty = if total_probes > 0 {
            effective_sequential_rounds as f64 / total_probes as f64
        } else {
            1.0
        };

        let normalized_probe_cost = total_probes as f64 / self.tree.len().max(1) as f64;
        let objective_value = terminal_score - beta * normalized_probe_cost;
        let pareto_reward = score_improvement - self.lambda * parallel_penalty;

        Ok(ReplayReport {
            policy_name: policy.name().to_string(),
            beta,
            baseline_score: self.baseline_score,
            terminal_score,
            score_improvement,
            total_probes,
            decision_rounds,
            effective_sequential_rounds,
            parallel_penalty,
            total_wall_time_ms,
            total_tokens,
            objective_value,
            pareto_reward,
            stop_reason,
            revealed_node_ids: revealed_order,
        })
    }

    /// Sweep beta parameter over a set of values for a policy constructor.
    pub fn sweep_beta<F>(
        &self,
        make_policy: F,
        betas: &[f64],
    ) -> Result<Vec<ReplayReport>, ReplayError>
    where
        F: Fn() -> Box<dyn SearchPolicy>,
    {
        let mut reports = Vec::new();
        for &beta in betas {
            let mut policy = make_policy();
            let report = self.evaluate_policy(policy.as_mut(), beta)?;
            reports.push(report);
        }
        Ok(reports)
    }

    /// Compare multiple search policies side-by-side under the same attempt tree and beta.
    /// Results are returned ranked by objective value (highest first).
    pub fn compare_policies(
        &self,
        policies: &mut [Box<dyn SearchPolicy>],
        beta: f64,
    ) -> Result<Vec<ReplayReport>, ReplayError> {
        let mut reports = Vec::new();
        for policy in policies.iter_mut() {
            let report = self.evaluate_policy(policy.as_mut(), beta)?;
            reports.push(report);
        }

        // Sort by objective value descending
        reports.sort_by(|a, b| {
            b.objective_value
                .partial_cmp(&a.objective_value)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(reports)
    }

    /// Helper to find legal roots and legal frontiers given the set of revealed node IDs.
    fn compute_legal_actions(&self, revealed_ids: &HashSet<String>) -> Vec<LegalAction> {
        let mut legal = Vec::new();

        for node in self.tree.nodes() {
            if revealed_ids.contains(&node.id) {
                continue;
            }

            match &node.parent_id {
                None => {
                    // Unrevealed root is legal
                    legal.push(LegalAction::OpenRoot {
                        branch_id: node.branch_id.clone(),
                        node_id: node.id.clone(),
                    });
                }
                Some(pid) => {
                    // Refinement is legal only if parent has already been revealed
                    if revealed_ids.contains(pid) {
                        legal.push(LegalAction::RefineFrontier {
                            branch_id: node.branch_id.clone(),
                            parent_id: pid.clone(),
                            node_id: node.id.clone(),
                        });
                    }
                }
            }
        }

        legal
    }

    /// Helper to construct PrefixObservation sequence in chronological reveal order.
    fn build_prefix_observations(&self, revealed_order: &[String]) -> Vec<PrefixObservation> {
        let mut observations = Vec::new();

        for id in revealed_order {
            if let Some(node) = self.tree.get(id) {
                let parent_score = node
                    .parent_id
                    .as_ref()
                    .and_then(|pid| self.tree.get(pid))
                    .and_then(|p| p.composite_score);

                let delta_vs_parent = match (node.composite_score, parent_score) {
                    (Some(curr), Some(par)) => Some(curr - par),
                    _ => None,
                };

                let delta_vs_baseline = node.composite_score.map(|s| s - self.baseline_score);

                // Calculate depth within branch based on parent chain
                let mut depth = 0;
                let mut curr_pid = node.parent_id.clone();
                while let Some(pid) = curr_pid {
                    depth += 1;
                    curr_pid = self.tree.get(&pid).and_then(|p| p.parent_id.clone());
                }

                observations.push(PrefixObservation {
                    id: node.id.clone(),
                    branch_id: node.branch_id.clone(),
                    attempt_depth: depth,
                    parent_id: node.parent_id.clone(),
                    score: node.composite_score,
                    status: node.status,
                    failure_class: node.failure_class,
                    failure_reason: node.failure_reason.clone(),
                    delta_vs_baseline,
                    delta_vs_parent,
                    tokens_used: node.tokens_used,
                    wall_time_ms: node.wall_time_ms,
                });
            }
        }

        observations
    }
}

#[cfg(test)]
#[path = "../../tests/unit/evolution/replay/replay_test.rs"]
mod tests;
