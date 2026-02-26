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
mod tests {
    use super::*;

    #[test]
    fn test_strategy_score_new() {
        let score = StrategyScore::new(ImprovementCategory::ErrorHandling);
        assert_eq!(score.attempts, 0);
        assert_eq!(score.success_rate(), 0.5); // prior
    }

    #[test]
    fn test_strategy_score_success_rate() {
        let mut score = StrategyScore::new(ImprovementCategory::CodeQuality);
        score.attempts = 10;
        score.successes = 8;
        assert!((score.success_rate() - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_strategy_score_priority_weight() {
        let mut score = StrategyScore::new(ImprovementCategory::CodeQuality);
        score.attempts = 10;
        score.successes = 8;
        score.avg_effectiveness = 0.6;
        let weight = score.priority_weight();
        // 0.5 * 0.8 + 0.5 * 0.6 = 0.7
        assert!((weight - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_meta_learner_update_weights() {
        let mut learner = MetaLearner {
            scores: HashMap::new(),
            alpha: 0.3,
            cooldown_secs: 3600,
            persist_path: std::env::temp_dir().join("selfware_test_meta.json"),
        };

        let record = ImprovementRecord {
            target_id: "imp-1".to_string(),
            category: ImprovementCategory::ErrorHandling,
            description: "test".to_string(),
            before_metrics: None,
            after_metrics: None,
            git_commits: vec![],
            verified: true,
            rolled_back: false,
            effectiveness_score: 0.8,
            completed_at: 0,
        };

        learner.update_weights(&record);
        let score = learner
            .get_score(&ImprovementCategory::ErrorHandling)
            .unwrap();
        assert_eq!(score.attempts, 1);
        assert_eq!(score.successes, 1);
        assert!((score.avg_effectiveness - 0.8).abs() < 0.001);

        std::fs::remove_file(&learner.persist_path).ok();
    }

    #[test]
    fn test_analyze_strategies() {
        let learner = MetaLearner {
            scores: HashMap::new(),
            alpha: 0.3,
            cooldown_secs: 3600,
            persist_path: std::env::temp_dir().join("selfware_test_meta2.json"),
        };

        let strategies = learner.analyze_strategies();
        // Should return all 7 categories with exploration bonus
        assert_eq!(strategies.len(), 7);
        // All should have the default exploration weight of 0.7
        for (_, weight) in &strategies {
            assert!((*weight - 0.7).abs() < 0.001);
        }
    }

    #[test]
    fn test_in_cooldown_not_active() {
        let score = StrategyScore::new(ImprovementCategory::CodeQuality);
        // Default cooldown_until is 0, which is always in the past
        assert!(!score.in_cooldown());
    }

    #[test]
    fn test_in_cooldown_active() {
        let mut score = StrategyScore::new(ImprovementCategory::CodeQuality);
        // Set cooldown far in the future
        score.cooldown_until = u64::MAX;
        assert!(score.in_cooldown());
    }

    #[test]
    fn test_priority_weight_during_cooldown() {
        let mut score = StrategyScore::new(ImprovementCategory::CodeQuality);
        score.attempts = 10;
        score.successes = 8;
        score.avg_effectiveness = 0.9;
        score.cooldown_until = u64::MAX; // in cooldown
        assert_eq!(score.priority_weight(), 0.0);
    }

    #[test]
    fn test_priority_weight_exploration_bonus() {
        let mut score = StrategyScore::new(ImprovementCategory::CodeQuality);
        score.attempts = 1; // < 3 → exploration bonus
        score.successes = 1;
        score.avg_effectiveness = 0.5;
        let weight = score.priority_weight();
        // 0.5 * 1.0 + 0.5 * 0.5 + 0.2 = 0.95
        assert!((weight - 0.95).abs() < 0.001);
    }

    #[test]
    fn test_weight_priority_known_category() {
        let mut learner = MetaLearner {
            scores: HashMap::new(),
            alpha: 0.3,
            cooldown_secs: 3600,
            persist_path: std::env::temp_dir().join("selfware_test_meta_wp.json"),
        };

        // Add a known category score
        let mut score = StrategyScore::new(ImprovementCategory::ErrorHandling);
        score.attempts = 10;
        score.successes = 8;
        score.avg_effectiveness = 0.6;
        learner
            .scores
            .insert(ImprovementCategory::ErrorHandling, score);

        let weighted = learner.weight_priority(&ImprovementCategory::ErrorHandling, 1.0);
        // weight = 0.5 * 0.8 + 0.5 * 0.6 = 0.7, so 1.0 * 0.7 = 0.7
        assert!((weighted - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_weight_priority_unknown_category() {
        let learner = MetaLearner {
            scores: HashMap::new(),
            alpha: 0.3,
            cooldown_secs: 3600,
            persist_path: std::env::temp_dir().join("selfware_test_meta_wp2.json"),
        };

        // Unknown category gets exploration prior of 0.7
        let weighted = learner.weight_priority(&ImprovementCategory::NewCapability, 0.8);
        assert!((weighted - 0.56).abs() < 0.001); // 0.8 * 0.7
    }

    #[test]
    fn test_get_score_none_for_untracked() {
        let learner = MetaLearner {
            scores: HashMap::new(),
            alpha: 0.3,
            cooldown_secs: 3600,
            persist_path: std::env::temp_dir().join("selfware_test_meta_gs.json"),
        };

        assert!(learner
            .get_score(&ImprovementCategory::ToolPipeline)
            .is_none());
    }

    #[test]
    fn test_update_weights_failure_applies_cooldown() {
        let mut learner = MetaLearner {
            scores: HashMap::new(),
            alpha: 0.3,
            cooldown_secs: 3600,
            persist_path: std::env::temp_dir().join("selfware_test_meta_fail.json"),
        };

        let record = ImprovementRecord {
            target_id: "imp-fail".to_string(),
            category: ImprovementCategory::CodeQuality,
            description: "bad".to_string(),
            before_metrics: None,
            after_metrics: None,
            git_commits: vec![],
            verified: false,
            rolled_back: true,
            effectiveness_score: -0.5,
            completed_at: 0,
        };

        learner.update_weights(&record);
        let score = learner
            .get_score(&ImprovementCategory::CodeQuality)
            .unwrap();
        assert_eq!(score.attempts, 1);
        assert_eq!(score.successes, 0);
        assert!(score.in_cooldown(), "Should be in cooldown after failure");
        assert_eq!(score.priority_weight(), 0.0); // cooldown blocks priority

        std::fs::remove_file(&learner.persist_path).ok();
    }

    #[test]
    fn test_update_weights_ema() {
        let mut learner = MetaLearner {
            scores: HashMap::new(),
            alpha: 0.3,
            cooldown_secs: 3600,
            persist_path: std::env::temp_dir().join("selfware_test_meta_ema.json"),
        };

        let make_record = |score: f64| ImprovementRecord {
            target_id: "imp-x".to_string(),
            category: ImprovementCategory::ErrorHandling,
            description: "test".to_string(),
            before_metrics: None,
            after_metrics: None,
            git_commits: vec![],
            verified: true,
            rolled_back: false,
            effectiveness_score: score,
            completed_at: 0,
        };

        // First record: avg = 1.0
        learner.update_weights(&make_record(1.0));
        let s = learner
            .get_score(&ImprovementCategory::ErrorHandling)
            .unwrap();
        assert!((s.avg_effectiveness - 1.0).abs() < 0.001);

        // Second record (0.0): avg = 0.3 * 0.0 + 0.7 * 1.0 = 0.7
        learner.update_weights(&make_record(0.0));
        let s = learner
            .get_score(&ImprovementCategory::ErrorHandling)
            .unwrap();
        assert!((s.avg_effectiveness - 0.7).abs() < 0.001);

        // Third record (0.5): avg = 0.3 * 0.5 + 0.7 * 0.7 = 0.64
        learner.update_weights(&make_record(0.5));
        let s = learner
            .get_score(&ImprovementCategory::ErrorHandling)
            .unwrap();
        assert!((s.avg_effectiveness - 0.64).abs() < 0.001);

        std::fs::remove_file(&learner.persist_path).ok();
    }

    #[test]
    fn test_analyze_strategies_with_mixed_scores() {
        let mut learner = MetaLearner {
            scores: HashMap::new(),
            alpha: 0.3,
            cooldown_secs: 3600,
            persist_path: std::env::temp_dir().join("selfware_test_meta_mixed.json"),
        };

        // Add a high-performing category
        let mut good = StrategyScore::new(ImprovementCategory::ErrorHandling);
        good.attempts = 10;
        good.successes = 9;
        good.avg_effectiveness = 0.8;
        learner
            .scores
            .insert(ImprovementCategory::ErrorHandling, good);

        // Add a poor-performing category in cooldown
        let mut bad = StrategyScore::new(ImprovementCategory::CodeQuality);
        bad.attempts = 5;
        bad.successes = 1;
        bad.avg_effectiveness = -0.2;
        bad.cooldown_until = u64::MAX;
        learner.scores.insert(ImprovementCategory::CodeQuality, bad);

        let strategies = learner.analyze_strategies();
        // ErrorHandling should be ranked above CodeQuality (which is 0.0 due to cooldown)
        let eh_pos = strategies
            .iter()
            .position(|(c, _)| *c == ImprovementCategory::ErrorHandling)
            .unwrap();
        let cq_pos = strategies
            .iter()
            .position(|(c, _)| *c == ImprovementCategory::CodeQuality)
            .unwrap();
        assert!(
            eh_pos < cq_pos,
            "ErrorHandling should rank above cooldown CodeQuality"
        );

        // CodeQuality weight should be 0.0
        let cq_weight = strategies
            .iter()
            .find(|(c, _)| *c == ImprovementCategory::CodeQuality)
            .unwrap()
            .1;
        assert_eq!(cq_weight, 0.0);
    }

    #[test]
    fn test_strategy_score_serialization_roundtrip() {
        let mut score = StrategyScore::new(ImprovementCategory::ToolPipeline);
        score.attempts = 5;
        score.successes = 3;
        score.avg_effectiveness = 0.65;
        score.last_attempted = 12345;

        let json = serde_json::to_string(&score).unwrap();
        let deserialized: StrategyScore = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.category, ImprovementCategory::ToolPipeline);
        assert_eq!(deserialized.attempts, 5);
        assert!((deserialized.avg_effectiveness - 0.65).abs() < 0.001);
    }
}
