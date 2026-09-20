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
use super::tree_log::{AttemptStatus, AttemptTree};
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
    #[error("Validation set is empty or unreachable (no legal starting actions): {0}")]
    ValidationSetEmptyOrUnreachable(String),
    #[error("Evaluation set is empty: {0}")]
    EvaluationEmpty(String),
}

/// Factory producing boxed search policies for replay simulation.
pub type SearchPolicyFactory = Box<dyn Fn() -> Box<dyn SearchPolicy>>;

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

/// Comprehensive outcome report from an offline replay evaluation across multiple attempt trees.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MultiTreeEvaluation {
    /// Policy name.
    pub policy_name: String,
    /// Exploration trade-off knob (beta).
    pub beta: f64,
    /// Number of trees evaluated.
    pub tree_count: usize,
    /// Average terminal score across trees.
    pub mean_terminal_score: f64,
    /// Average score improvement over baseline.
    pub mean_improvement: f64,
    /// Average probe count per tree.
    pub mean_probes: f64,
    /// Average objective value J(π) across trees.
    pub mean_objective_value: f64,
    /// Average Pareto reward across trees.
    pub mean_pareto_reward: f64,
    /// Cumulative tokens consumed across all trees.
    pub cumulative_tokens: u64,
    /// Cumulative simulated wall time in ms across all trees.
    pub cumulative_wall_time_ms: u64,
    /// Per-tree replay reports.
    pub per_tree_reports: Vec<ReplayReport>,
}

/// Outcome report comparing candidate policies on held-out validation data vs discovery cost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyValidationSummary {
    /// Name of the search policy.
    pub policy_name: String,
    /// Cumulative probes expended during discovery.
    pub discovery_probes: usize,
    /// Cumulative tokens expended during discovery.
    pub discovery_tokens: u64,
    /// Discovery objective value J.
    pub discovery_objective: f64,
    /// Held-out validation mean terminal score.
    pub validation_terminal_score: f64,
    /// Held-out validation objective value J.
    pub validation_objective: f64,
    /// Whether this policy outperformed the incumbent baseline on held-out validation.
    pub beats_incumbent: bool,
}

/// Measures the ranking concordance of search policies across multiple attempt trees.
///
/// Uses Kendall's coefficient of concordance (W) to determine whether the relative
/// ranking of policies is mathematically consistent across diverse attempt trees
/// (W in [0, 1]). W >= 0.70 indicates high concordance; W < 0.70 indicates
/// tree-dependent volatility where policy promotion should be held.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankingStability {
    /// Kendall's W coefficient of concordance [0.0, 1.0].
    pub kendall_w: f64,
    /// Whether Kendall's W meets or exceeds the stability threshold (0.70).
    pub is_stable: bool,
    /// Number of attempt trees evaluated.
    pub tree_count: usize,
    /// Number of search policies compared.
    pub policy_count: usize,
    /// Per-tree rankings: for each tree index, a list of (policy_name, rank, objective_value).
    pub per_tree_rankings: Vec<Vec<(String, usize, f64)>>,
}

fn default_beta() -> f64 {
    0.2
}

/// Outcome of candidate evaluation across discovery and validation trees,
/// including cross-tree ranking stability analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayValidationOutcome {
    /// Candidate summaries ranked descending by validation objective.
    pub summaries: Vec<PolicyValidationSummary>,
    /// Ranking stability across discovery trees, if >= 2 discovery trees.
    pub discovery_stability: Option<RankingStability>,
    /// Ranking stability across validation trees, if >= 2 validation trees.
    pub validation_stability: Option<RankingStability>,
    /// Exploration/exploitation trade-off factor beta used during evaluation.
    #[serde(default = "default_beta")]
    pub beta: f64,
}

/// Computes the canonical SHA-256 evidence hash binding a promoted policy to its replay evaluation outcome
/// and the provenance of its inputs (attempt-tree file digests and replay report digest).
pub fn compute_policy_evidence_hash(
    winner_name: &str,
    validation_objective: f64,
    incumbent_objective: f64,
    kendall_w: f64,
    beta: f64,
    tree_digests: &[String],
    report_digest: &str,
) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(winner_name.trim().as_bytes());
    hasher.update(
        format!(
            ":{:.6}:{:.6}:{:.6}:{:.6}",
            validation_objective, incumbent_objective, kendall_w, beta
        )
        .as_bytes(),
    );
    for digest in tree_digests {
        hasher.update(b":tree:");
        hasher.update(digest.as_bytes());
    }
    if !report_digest.is_empty() {
        hasher.update(b":report:");
        hasher.update(report_digest.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

/// Explicit promotion readiness decision incorporating required evidence,
/// ranking stability across validation trees, and objective improvement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PromotionReadiness {
    /// Candidate has demonstrated verified improvement, satisfies required evidence,
    /// and exhibits ranking stability across validation trees (Kendall's W >= 0.70).
    Ready {
        winner_name: String,
        validation_objective: f64,
        incumbent_objective: f64,
        kendall_w: f64,
        #[serde(default = "default_beta")]
        beta: f64,
    },
    /// Candidate has higher mean objective than incumbent, but ranking across validation
    /// trees is volatile (Kendall's W < 0.70). Promotion is strictly blocked.
    BlockedByInstability {
        winner_name: String,
        validation_objective: f64,
        incumbent_objective: f64,
        kendall_w: f64,
    },
    /// Incumbent policy remains optimal; top candidate did not beat incumbent baseline.
    RetainIncumbent {
        incumbent_objective: f64,
        best_candidate_name: Option<String>,
        best_candidate_objective: Option<f64>,
    },
    /// Insufficient evidence to decide (e.g. fewer than 2 validation trees to assess stability).
    InsufficientEvidence { reason: String },
}

impl ReplayValidationOutcome {
    /// Computes a canonical SHA-256 digest over all replay evaluation outcome metrics,
    /// ensuring cryptographic provenance for policy promotion.
    pub fn report_digest(&self) -> String {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        for s in &self.summaries {
            hasher.update(
                format!(
                    "{}:{}:{}:{:.6}:{:.6}:{};",
                    s.policy_name,
                    s.discovery_probes,
                    s.discovery_tokens,
                    s.validation_terminal_score,
                    s.validation_objective,
                    s.beats_incumbent
                )
                .as_bytes(),
            );
        }
        if let Some(ref d) = self.discovery_stability {
            hasher.update(
                format!("disc:{:.6}:{}:{};", d.kendall_w, d.tree_count, d.is_stable).as_bytes(),
            );
        }
        if let Some(ref v) = self.validation_stability {
            hasher.update(
                format!("val:{:.6}:{}:{};", v.kendall_w, v.tree_count, v.is_stable).as_bytes(),
            );
        }
        hasher.update(format!("beta:{:.6}", self.beta).as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Explicitly determines whether the top candidate policy qualifies for promotion.
    pub fn promotion_readiness(&self) -> PromotionReadiness {
        let incumbent_summary = self
            .summaries
            .iter()
            .find(|s| s.policy_name.contains("Incumbent"));
        let incumbent_obj = incumbent_summary
            .map(|s| s.validation_objective)
            .unwrap_or(0.0);

        let top_candidate = self
            .summaries
            .iter()
            .find(|s| !s.policy_name.contains("Incumbent"));

        let top = match top_candidate {
            Some(t) => t,
            None => {
                return PromotionReadiness::RetainIncumbent {
                    incumbent_objective: incumbent_obj,
                    best_candidate_name: None,
                    best_candidate_objective: None,
                };
            }
        };

        if !top.beats_incumbent || top.validation_objective <= incumbent_obj {
            return PromotionReadiness::RetainIncumbent {
                incumbent_objective: incumbent_obj,
                best_candidate_name: Some(top.policy_name.clone()),
                best_candidate_objective: Some(top.validation_objective),
            };
        }

        // Top candidate beats incumbent. Check ranking stability evidence.
        match &self.validation_stability {
            Some(stability) => {
                if stability.is_stable {
                    PromotionReadiness::Ready {
                        winner_name: top.policy_name.clone(),
                        validation_objective: top.validation_objective,
                        incumbent_objective: incumbent_obj,
                        kendall_w: stability.kendall_w,
                        beta: self.beta,
                    }
                } else {
                    PromotionReadiness::BlockedByInstability {
                        winner_name: top.policy_name.clone(),
                        validation_objective: top.validation_objective,
                        incumbent_objective: incumbent_obj,
                        kendall_w: stability.kendall_w,
                    }
                }
            }
            None => PromotionReadiness::InsufficientEvidence {
                reason: "ranking stability evaluation requires at least 2 validation trees"
                    .to_string(),
            },
        }
    }
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
        if let Err(e) = self.tree.validate_ancestry() {
            return Err(ReplayError::ValidationSetEmptyOrUnreachable(format!(
                "tree has invalid ancestry: {e}"
            )));
        }

        // Pre-discover baseline node if present; treated as already known at round 0
        let effective_baseline_score = if self.baseline_score > 0.0 {
            self.baseline_score
        } else {
            self.tree
                .nodes()
                .iter()
                .find(|n| n.status == AttemptStatus::Baseline || n.id == "att-baseline")
                .and_then(|n| n.composite_score)
                .unwrap_or(self.baseline_score)
        };

        let mut revealed_ids: HashSet<String> = HashSet::new();
        let mut baseline_order: Vec<String> = Vec::new();
        for node in self.tree.nodes() {
            if node.status == AttemptStatus::Baseline || node.id == "att-baseline" {
                revealed_ids.insert(node.id.clone());
                baseline_order.push(node.id.clone());
            }
        }

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
                if decision_rounds == 0 {
                    return Err(ReplayError::ValidationSetEmptyOrUnreachable(
                        "tree has zero reachable root nodes (all attempts depend on unrevealed parents)"
                            .into(),
                    ));
                }
                stop_reason = "No legal actions remaining in tree".into();
                break;
            }

            // Construct prefix view with revealed observations (including baseline)
            let all_revealed: Vec<String> = baseline_order
                .iter()
                .chain(revealed_order.iter())
                .cloned()
                .collect();
            let prefix_obs =
                self.build_prefix_observations(&all_revealed, effective_baseline_score);
            let prefix =
                PrefixView::new(prefix_obs, effective_baseline_score, self.max_parallelism);

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
            .fold(effective_baseline_score, f64::max);

        let score_improvement = terminal_score - effective_baseline_score;
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
            baseline_score: effective_baseline_score,
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

    /// Evaluates a policy across multiple attempt trees using a policy factory closure.
    pub fn evaluate_policy_across_trees<F>(
        trees: &[AttemptTree],
        baseline_score: f64,
        max_parallelism: usize,
        make_policy: &F,
        beta: f64,
    ) -> Result<MultiTreeEvaluation, ReplayError>
    where
        F: Fn() -> Box<dyn SearchPolicy>,
    {
        let mut reports = Vec::new();
        let mut cumulative_tokens = 0;
        let mut cumulative_wall_time_ms = 0;
        let mut policy_name = String::new();

        for tree in trees {
            let sim = ReplaySimulator::new(tree.clone(), baseline_score)
                .with_max_parallelism(max_parallelism);
            let mut policy = make_policy();
            policy_name = policy.name().to_string();
            let report = match sim.evaluate_policy(policy.as_mut(), beta) {
                Ok(r) => r,
                Err(ReplayError::ValidationSetEmptyOrUnreachable(msg)) => {
                    tracing::warn!("Replay tree has no reachable actions ({msg}); skipping tree");
                    continue;
                }
                Err(e) => return Err(e),
            };
            cumulative_tokens += report.total_tokens;
            cumulative_wall_time_ms += report.total_wall_time_ms;
            reports.push(report);
        }

        let n = reports.len().max(1) as f64;
        let mean_terminal_score = reports.iter().map(|r| r.terminal_score).sum::<f64>() / n;
        let mean_improvement = reports.iter().map(|r| r.score_improvement).sum::<f64>() / n;
        let mean_probes = reports.iter().map(|r| r.total_probes as f64).sum::<f64>() / n;
        let mean_objective_value = reports.iter().map(|r| r.objective_value).sum::<f64>() / n;
        let mean_pareto_reward = reports.iter().map(|r| r.pareto_reward).sum::<f64>() / n;

        Ok(MultiTreeEvaluation {
            policy_name,
            beta,
            tree_count: reports.len(),
            mean_terminal_score,
            mean_improvement,
            mean_probes,
            mean_objective_value,
            mean_pareto_reward,
            cumulative_tokens,
            cumulative_wall_time_ms,
            per_tree_reports: reports,
        })
    }

    /// Evaluates candidate policies against discovery trees and held-out validation trees,
    /// measuring discovery costs vs validation quality, cross-tree ranking stability, and judging candidates against the incumbent.
    pub fn evaluate_candidates_full(
        discovery_trees: &[AttemptTree],
        validation_trees: &[AttemptTree],
        baseline_score: f64,
        max_parallelism: usize,
        candidate_factories: &[(&'static str, SearchPolicyFactory)],
        beta: f64,
    ) -> Result<ReplayValidationOutcome, ReplayError> {
        let mut summaries = Vec::new();
        let mut incumbent_validation_obj = 0.0;
        let mut found_incumbent = false;
        let mut disc_evals = Vec::new();
        let mut val_evals = Vec::new();

        // First pass: evaluate on discovery and validation
        for (label, make_policy) in candidate_factories {
            let disc_eval = Self::evaluate_policy_across_trees(
                discovery_trees,
                baseline_score,
                max_parallelism,
                make_policy,
                beta,
            )?;
            let val_eval = Self::evaluate_policy_across_trees(
                validation_trees,
                baseline_score,
                max_parallelism,
                make_policy,
                beta,
            )?;

            let is_incumbent =
                label.contains("Incumbent") || disc_eval.policy_name.contains("Incumbent");
            if is_incumbent {
                incumbent_validation_obj = val_eval.mean_objective_value;
                found_incumbent = true;
            }

            let disc_probes: usize = disc_eval
                .per_tree_reports
                .iter()
                .map(|r| r.total_probes)
                .sum();

            summaries.push(PolicyValidationSummary {
                policy_name: disc_eval.policy_name.clone(),
                discovery_probes: disc_probes,
                discovery_tokens: disc_eval.cumulative_tokens,
                discovery_objective: disc_eval.mean_objective_value,
                validation_terminal_score: val_eval.mean_terminal_score,
                validation_objective: val_eval.mean_objective_value,
                beats_incumbent: false,
            });

            disc_evals.push(disc_eval);
            val_evals.push(val_eval);
        }

        // Second pass: mark beats_incumbent
        if found_incumbent {
            for summary in &mut summaries {
                if !summary.policy_name.contains("Incumbent")
                    && summary.validation_objective > incumbent_validation_obj
                {
                    summary.beats_incumbent = true;
                }
            }
        }

        // Sort by validation objective descending
        summaries.sort_by(|a, b| {
            b.validation_objective
                .partial_cmp(&a.validation_objective)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let discovery_stability = if discovery_trees.len() > 1 && candidate_factories.len() > 1 {
            match Self::compute_ranking_stability(&disc_evals) {
                Ok(stab) => Some(stab),
                Err(err) => {
                    tracing::warn!("Failed to compute discovery ranking stability: {err}");
                    None
                }
            }
        } else {
            None
        };

        let validation_stability = if validation_trees.len() > 1 && candidate_factories.len() > 1 {
            match Self::compute_ranking_stability(&val_evals) {
                Ok(stab) => Some(stab),
                Err(err) => {
                    tracing::warn!("Failed to compute validation ranking stability: {err}");
                    None
                }
            }
        } else {
            None
        };

        Ok(ReplayValidationOutcome {
            summaries,
            discovery_stability,
            validation_stability,
            beta,
        })
    }

    /// Evaluates candidate policies against discovery trees and held-out validation trees,
    /// returning only the ranked summaries for backwards compatibility.
    pub fn evaluate_candidates_with_validation(
        discovery_trees: &[AttemptTree],
        validation_trees: &[AttemptTree],
        baseline_score: f64,
        max_parallelism: usize,
        candidate_factories: &[(&'static str, SearchPolicyFactory)],
        beta: f64,
    ) -> Result<Vec<PolicyValidationSummary>, ReplayError> {
        Self::evaluate_candidates_full(
            discovery_trees,
            validation_trees,
            baseline_score,
            max_parallelism,
            candidate_factories,
            beta,
        )
        .map(|outcome| outcome.summaries)
    }

    /// Computes Kendall's W coefficient of concordance across multiple attempt trees for a set of policies.
    pub fn compute_ranking_stability(
        evaluations: &[MultiTreeEvaluation],
    ) -> Result<RankingStability, ReplayError> {
        let policy_count = evaluations.len();
        if policy_count == 0 {
            return Err(ReplayError::EvaluationEmpty(
                "no policy evaluations provided".into(),
            ));
        }

        let tree_count = evaluations[0].tree_count;
        if tree_count == 0 {
            return Err(ReplayError::EvaluationEmpty("tree count is zero".into()));
        }

        for eval in evaluations {
            if eval.tree_count != tree_count || eval.per_tree_reports.len() != tree_count {
                return Err(ReplayError::EvaluationEmpty(
                    "mismatched tree counts across evaluations".into(),
                ));
            }
        }

        if tree_count < 2 || policy_count < 2 {
            let mut per_tree_rankings = Vec::with_capacity(tree_count);
            for t in 0..tree_count {
                let mut tree_ranks = Vec::with_capacity(policy_count);
                for (p_idx, eval) in evaluations.iter().enumerate() {
                    tree_ranks.push((
                        eval.policy_name.clone(),
                        p_idx + 1,
                        eval.per_tree_reports[t].objective_value,
                    ));
                }
                per_tree_rankings.push(tree_ranks);
            }
            // Rule 3: Honest status over optimistic success.
            // With fewer than 2 trees or 2 policies, concordance has zero degrees
            // of freedom; claiming W = 1.0 and is_stable: true is a false green badge.
            return Ok(RankingStability {
                kendall_w: 0.0,
                is_stable: false,
                tree_count,
                policy_count,
                per_tree_rankings,
            });
        }

        let m = policy_count as f64;
        let n = tree_count as f64;

        let mut per_tree_rankings: Vec<Vec<(String, usize, f64)>> = Vec::with_capacity(tree_count);
        let mut rank_sums = vec![0.0; policy_count];

        let mut total_tie_correction = 0.0;

        for t in 0..tree_count {
            let mut scored_policies: Vec<(usize, String, f64)> = evaluations
                .iter()
                .enumerate()
                .map(|(idx, eval)| {
                    (
                        idx,
                        eval.policy_name.clone(),
                        eval.per_tree_reports[t].objective_value,
                    )
                })
                .collect();

            // Sort by score descending. For presentation order, use policy name as secondary,
            // but assign IDENTICAL fractional mid-ranks to tied policies so ties do not manufacture
            // artificial variance or stability.
            scored_policies.sort_by(|a, b| {
                b.2.partial_cmp(&a.2)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.1.cmp(&b.1))
            });

            let mut tree_ranking = Vec::with_capacity(policy_count);
            let mut tree_tie_correction = 0.0;
            let mut i = 0;

            while i < policy_count {
                let mut j = i + 1;
                while j < policy_count && (scored_policies[j].2 - scored_policies[i].2).abs() < 1e-9
                {
                    j += 1;
                }
                let tie_size = (j - i) as f64;
                if tie_size > 1.0 {
                    tree_tie_correction += tie_size.powi(3) - tie_size;
                }

                // Fractional mid-rank: arithmetic mean of positions [i + 1, ..., j]
                let mid_rank = (i + 1 + j) as f64 / 2.0;

                for &(p_idx, ref name, score) in &scored_policies[i..j] {
                    rank_sums[p_idx] += mid_rank;
                    tree_ranking.push((name.clone(), mid_rank.round() as usize, score));
                }
                i = j;
            }

            total_tie_correction += tree_tie_correction;
            per_tree_rankings.push(tree_ranking);
        }

        let r_bar = n * (m + 1.0) / 2.0;
        let s: f64 = rank_sums.iter().map(|r| (r - r_bar).powi(2)).sum();
        let max_s = ((n.powi(2) * (m.powi(3) - m)) - (n * total_tie_correction)) / 12.0;

        let kendall_w = if max_s > 1e-9 {
            (s / max_s).clamp(0.0, 1.0)
        } else {
            // When all policies have identical scores on every tree, there is zero
            // evidence of ranking preference (distinguishable variance = 0). W is 0.0.
            0.0
        };

        let is_stable = kendall_w >= 0.70;

        Ok(RankingStability {
            kendall_w,
            is_stable,
            tree_count,
            policy_count,
            per_tree_rankings,
        })
    }

    /// Helper to find legal roots and legal frontiers given the set of revealed node IDs.
    fn compute_legal_actions(&self, revealed_ids: &HashSet<String>) -> Vec<LegalAction> {
        let mut legal = Vec::new();

        for node in self.tree.nodes() {
            if revealed_ids.contains(&node.id) {
                continue;
            }
            if node.branch_id == "control" {
                // Control anchors are shadow environment verification tests, not exploration actions
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
                        let is_root = node.is_open_root_action();

                        if is_root {
                            legal.push(LegalAction::OpenRoot {
                                branch_id: node.branch_id.clone(),
                                node_id: node.id.clone(),
                            });
                        } else {
                            legal.push(LegalAction::RefineFrontier {
                                branch_id: node.branch_id.clone(),
                                parent_id: pid.clone(),
                                node_id: node.id.clone(),
                            });
                        }
                    }
                }
            }
        }

        legal
    }

    /// Helper to construct PrefixObservation sequence in chronological reveal order.
    fn build_prefix_observations(
        &self,
        revealed_order: &[String],
        baseline_score: f64,
    ) -> Vec<PrefixObservation> {
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

                let delta_vs_baseline = node.composite_score.map(|s| s - baseline_score);

                // Depth within the branch, via the shared guarded walker: a cyclic
                // parent chain must terminate rather than spin.
                let depth = crate::evolution::tree_log::guarded_ancestry_depth(&node.id, |pid| {
                    self.tree.get(pid).and_then(|p| p.parent_id.clone())
                });

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
