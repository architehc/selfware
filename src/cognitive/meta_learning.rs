//! Meta-Learning
//!
//! Tracks which improvement strategies are most effective and adjusts priorities.
//! Uses exponential moving averages to weight category priorities based on
//! historical success rates.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::self_edit::{ImprovementCategory, ImprovementRecord};

/// Score tracking for an improvement strategy/category
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyScore {
    pub category: ImprovementCategory,
    pub attempts: usize,
    pub successes: usize,
    pub avg_effectiveness: f64,
    pub last_attempted: u64,
    /// Cooldown: skip this category until this timestamp
    pub cooldown_until: u64,
}

impl StrategyScore {
    pub fn new(category: ImprovementCategory) -> Self {
        Self {
            category,
            attempts: 0,
            successes: 0,
            avg_effectiveness: 0.0,
            last_attempted: 0,
            cooldown_until: 0,
        }
    }

    /// Success rate (0.0 - 1.0)
    pub fn success_rate(&self) -> f64 {
        if self.attempts == 0 {
            0.5 // prior: assume moderate success for untried categories
        } else {
            self.successes as f64 / self.attempts as f64
        }
    }

    /// Whether this category is in cooldown
    pub fn in_cooldown(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now < self.cooldown_until
    }

    /// Priority weight for this category (combines success rate and effectiveness)
    pub fn priority_weight(&self) -> f64 {
        if self.in_cooldown() {
            return 0.0;
        }
        // Blend success rate with effectiveness, with a prior toward exploration
        let exploration_bonus = if self.attempts < 3 { 0.2 } else { 0.0 };
        0.5 * self.success_rate() + 0.5 * self.avg_effectiveness.max(0.0) + exploration_bonus
    }
}

/// Meta-learner that tracks strategy effectiveness
pub struct MetaLearner {
    scores: HashMap<ImprovementCategory, StrategyScore>,
    /// EMA alpha for effectiveness updates
    alpha: f64,
    /// Cooldown duration in seconds after a failure
    cooldown_secs: u64,
    /// Path for persistence
    persist_path: PathBuf,
}

impl MetaLearner {
    pub fn new() -> Self {
        let persist_path = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("selfware")
            .join("improvements")
            .join("meta_learner.json");

        let scores = Self::load_scores(&persist_path).unwrap_or_default();

        Self {
            scores,
            alpha: 0.3,
            cooldown_secs: 3600, // 1 hour cooldown after failure
            persist_path,
        }
    }

    /// Update weights from an improvement record
    pub fn update_weights(&mut self, record: &ImprovementRecord) {
        let score = self
            .scores
            .entry(record.category.clone())
            .or_insert_with(|| StrategyScore::new(record.category.clone()));

        score.attempts += 1;
        score.last_attempted = record.completed_at;

        if record.verified && !record.rolled_back && record.effectiveness_score > 0.0 {
            score.successes += 1;
        }

        // Exponential moving average for effectiveness
        if score.attempts == 1 {
            score.avg_effectiveness = record.effectiveness_score;
        } else {
            score.avg_effectiveness = self.alpha * record.effectiveness_score
                + (1.0 - self.alpha) * score.avg_effectiveness;
        }

        // Apply cooldown on failure
        if record.rolled_back || record.effectiveness_score < 0.0 {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            score.cooldown_until = now + self.cooldown_secs;
        }

        // Persist after update
        if let Err(e) = self.save() {
            tracing::warn!("Failed to persist meta-learner state: {}", e);
        }
    }

    /// Get ranked strategy recommendations
    pub fn analyze_strategies(&self) -> Vec<(ImprovementCategory, f64)> {
        let mut ranked: Vec<_> = self
            .scores
            .iter()
            .map(|(cat, score)| (cat.clone(), score.priority_weight()))
            .collect();

        // Add default scores for categories not yet tracked
        let all_categories = vec![
            ImprovementCategory::PromptTemplate,
            ImprovementCategory::ToolPipeline,
            ImprovementCategory::ErrorHandling,
            ImprovementCategory::VerificationLogic,
            ImprovementCategory::ContextManagement,
            ImprovementCategory::CodeQuality,
            ImprovementCategory::NewCapability,
        ];

        for cat in all_categories {
            if !self.scores.contains_key(&cat) {
                // Untried categories get an exploration bonus
                ranked.push((cat, 0.7));
            }
        }

        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked
    }

    /// Apply category weight to an improvement target's priority
    pub fn weight_priority(&self, category: &ImprovementCategory, base_priority: f64) -> f64 {
        let weight = self
            .scores
            .get(category)
            .map(|s| s.priority_weight())
            .unwrap_or(0.7); // exploration prior for unknown categories
        base_priority * weight
    }

    /// Get score for a specific category
    pub fn get_score(&self, category: &ImprovementCategory) -> Option<&StrategyScore> {
        self.scores.get(category)
    }

    fn save(&self) -> Result<()> {
        if let Some(parent) = self.persist_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(&self.scores)?;
        std::fs::write(&self.persist_path, content)?;
        Ok(())
    }

    fn load_scores(path: &Path) -> Result<HashMap<ImprovementCategory, StrategyScore>> {
        if !path.exists() {
            return Ok(HashMap::new());
        }
        let content = std::fs::read_to_string(path)?;
        let scores = serde_json::from_str(&content)?;
        Ok(scores)
    }
}

impl Default for MetaLearner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/cognitive/meta_learning/meta_learning_test.rs"]
mod tests;
