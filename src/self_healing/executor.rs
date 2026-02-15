use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{RecoveryAction, RecoveryStrategy, SelfHealingConfig, StateManager};

/// Recovery execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryExecution {
    /// Strategy used
    pub strategy: String,
    /// Started at
    pub started_at: u64,
    /// Completed at
    pub completed_at: Option<u64>,
    /// Success
    pub success: bool,
    /// Actions executed
    pub actions_executed: Vec<String>,
    /// Error if failed
    pub error: Option<String>,
}

/// Recovery executor
pub struct RecoveryExecutor {
    config: SelfHealingConfig,
    /// Execution history
    history: RwLock<VecDeque<RecoveryExecution>>,
    /// Statistics
    stats: ExecutorStats,
}

/// Executor statistics
#[derive(Debug, Default)]
pub struct ExecutorStats {
    pub executions: AtomicU64,
    pub successes: AtomicU64,
    pub failures: AtomicU64,
}

impl RecoveryExecutor {
    pub fn new(config: SelfHealingConfig) -> Self {
        Self {
            history: RwLock::new(VecDeque::with_capacity(100)),
            config,
            stats: ExecutorStats::default(),
        }
    }

    /// Execute a recovery strategy without external state access.
    pub fn execute(&self, strategy: &RecoveryStrategy) -> RecoveryExecution {
        self.execute_internal(strategy, None)
    }

    /// Execute a recovery strategy with state-manager integration for
    /// restore/clear/reset actions.
    pub fn execute_with_state(
        &self,
        strategy: &RecoveryStrategy,
        state_manager: &StateManager,
    ) -> RecoveryExecution {
        self.execute_internal(strategy, Some(state_manager))
    }

    fn execute_internal(
        &self,
        strategy: &RecoveryStrategy,
        state_manager: Option<&StateManager>,
    ) -> RecoveryExecution {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if !self.config.enabled {
            return RecoveryExecution {
                strategy: strategy.name.clone(),
                started_at: now,
                completed_at: Some(now),
                success: false,
                actions_executed: vec![],
                error: Some("Self-healing is disabled".to_string()),
            };
        }

        self.stats.executions.fetch_add(1, Ordering::Relaxed);

        let mut actions_executed = Vec::new();
        let mut success = true;
        let mut error = None;

        let max_actions = self.config.max_healing_attempts.max(1) as usize;

        for (index, action) in strategy.actions.iter().enumerate() {
            if index >= max_actions {
                success = false;
                error = Some(format!(
                    "Recovery aborted: exceeded max healing attempts ({})",
                    self.config.max_healing_attempts
                ));
                break;
            }

            let action_name = action_name(action);
            actions_executed.push(action_name.to_string());

            if let Err(e) = self.execute_action(action, state_manager) {
                success = false;
                error = Some(format!("Action '{}' failed: {}", action_name, e));
                break;
            }
        }

        if success {
            self.stats.successes.fetch_add(1, Ordering::Relaxed);
        } else {
            self.stats.failures.fetch_add(1, Ordering::Relaxed);
        }

        let completed_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let execution = RecoveryExecution {
            strategy: strategy.name.clone(),
            started_at: now,
            completed_at: Some(completed_at),
            success,
            actions_executed,
            error,
        };

        if let Ok(mut history) = self.history.write() {
            history.push_back(execution.clone());
            while history.len() > 100 {
                history.pop_front();
            }
        }

        execution
    }

    fn execute_action(
        &self,
        action: &RecoveryAction,
        state_manager: Option<&StateManager>,
    ) -> Result<(), String> {
        match action {
            RecoveryAction::Retry {
                delay_ms: _,
                max_attempts,
            } => {
                if *max_attempts == 0 {
                    Err("max_attempts must be greater than 0".to_string())
                } else {
                    Ok(())
                }
            }
            RecoveryAction::Restart { component } => {
                if component.trim().is_empty() {
                    Err("component cannot be empty".to_string())
                } else {
                    Ok(())
                }
            }
            RecoveryAction::Fallback { target } => {
                if target.trim().is_empty() {
                    Err("fallback target cannot be empty".to_string())
                } else {
                    Ok(())
                }
            }
            RecoveryAction::RestoreCheckpoint { checkpoint_id } => {
                let manager = state_manager
                    .ok_or_else(|| "state manager unavailable for restore action".to_string())?;

                if manager.restore(checkpoint_id.as_deref()).is_some() {
                    Ok(())
                } else {
                    Err("no checkpoint available to restore".to_string())
                }
            }
            RecoveryAction::ClearCache { .. } => {
                let manager = state_manager.ok_or_else(|| {
                    "state manager unavailable for clear-cache action".to_string()
                })?;
                manager.clear();
                Ok(())
            }
            RecoveryAction::ResetState { .. } => {
                let manager = state_manager.ok_or_else(|| {
                    "state manager unavailable for reset-state action".to_string()
                })?;
                manager.clear();
                Ok(())
            }
            RecoveryAction::Custom { name, .. } => {
                if name.trim().is_empty() {
                    Err("custom action name cannot be empty".to_string())
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Get success rate
    pub fn success_rate(&self) -> f32 {
        let total = self.stats.executions.load(Ordering::Relaxed) as f32;
        let successes = self.stats.successes.load(Ordering::Relaxed) as f32;
        if total > 0.0 {
            successes / total
        } else {
            0.0
        }
    }

    /// Get history
    pub fn history(&self) -> Vec<RecoveryExecution> {
        self.history
            .read()
            .map(|h| h.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get summary
    pub fn summary(&self) -> ExecutorSummary {
        ExecutorSummary {
            executions: self.stats.executions.load(Ordering::Relaxed),
            successes: self.stats.successes.load(Ordering::Relaxed),
            failures: self.stats.failures.load(Ordering::Relaxed),
            success_rate: self.success_rate(),
        }
    }
}

impl Default for RecoveryExecutor {
    fn default() -> Self {
        Self::new(SelfHealingConfig::default())
    }
}

/// Executor summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorSummary {
    pub executions: u64,
    pub successes: u64,
    pub failures: u64,
    pub success_rate: f32,
}

fn action_name(action: &RecoveryAction) -> &str {
    match action {
        RecoveryAction::Retry { .. } => "retry",
        RecoveryAction::Restart { .. } => "restart",
        RecoveryAction::Fallback { .. } => "fallback",
        RecoveryAction::RestoreCheckpoint { .. } => "restore",
        RecoveryAction::ClearCache { .. } => "clear_cache",
        RecoveryAction::ResetState { .. } => "reset_state",
        RecoveryAction::Custom { name, .. } => name.as_str(),
    }
}
