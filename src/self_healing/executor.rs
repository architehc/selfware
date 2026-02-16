use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

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

/// Tracks retry state for a given error pattern to support exponential backoff.
#[derive(Debug, Clone)]
struct RetryState {
    /// Number of retry attempts so far
    attempt_count: u32,
    /// Delay used on the last retry (ms)
    last_delay_ms: u64,
    /// Timestamp of first attempt
    first_attempt_at: u64,
}

/// Recovery executor — runs recovery actions with real retry delays,
/// exponential backoff, checkpoint restore, cache clearing, and more.
pub struct RecoveryExecutor {
    config: SelfHealingConfig,
    /// Execution history
    history: RwLock<VecDeque<RecoveryExecution>>,
    /// Per-pattern retry state for exponential backoff
    retry_states: RwLock<HashMap<String, RetryState>>,
    /// Statistics
    stats: ExecutorStats,
}

/// Executor statistics
#[derive(Debug, Default)]
pub struct ExecutorStats {
    pub executions: AtomicU64,
    pub successes: AtomicU64,
    pub failures: AtomicU64,
    pub retries_performed: AtomicU64,
    pub total_backoff_ms: AtomicU64,
}

impl RecoveryExecutor {
    pub fn new(config: SelfHealingConfig) -> Self {
        Self {
            history: RwLock::new(VecDeque::with_capacity(100)),
            retry_states: RwLock::new(HashMap::new()),
            config,
            stats: ExecutorStats::default(),
        }
    }

    /// Execute a recovery strategy without external state access.
    pub fn execute(&self, strategy: &RecoveryStrategy) -> RecoveryExecution {
        self.execute_internal(strategy, None, None)
    }

    /// Execute a recovery strategy with state-manager integration for
    /// restore/clear/reset actions.
    pub fn execute_with_state(
        &self,
        strategy: &RecoveryStrategy,
        state_manager: &StateManager,
    ) -> RecoveryExecution {
        self.execute_internal(strategy, Some(state_manager), None)
    }

    /// Execute a recovery strategy with state manager and an error pattern key
    /// used to track per-pattern retry state for exponential backoff.
    pub fn execute_for_pattern(
        &self,
        strategy: &RecoveryStrategy,
        state_manager: &StateManager,
        pattern_key: &str,
    ) -> RecoveryExecution {
        self.execute_internal(strategy, Some(state_manager), Some(pattern_key))
    }

    fn execute_internal(
        &self,
        strategy: &RecoveryStrategy,
        state_manager: Option<&StateManager>,
        pattern_key: Option<&str>,
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

            let name = action_name(action);
            actions_executed.push(name.to_string());

            info!("Executing recovery action: {}", name);

            if let Err(e) = self.execute_action(action, state_manager, pattern_key) {
                success = false;
                error = Some(format!("Action '{}' failed: {}", name, e));
                warn!("Recovery action '{}' failed: {}", name, e);
                break;
            }

            debug!("Recovery action '{}' completed successfully", name);
        }

        if success {
            self.stats.successes.fetch_add(1, Ordering::Relaxed);
            info!(
                "Recovery strategy '{}' completed successfully ({} actions)",
                strategy.name,
                actions_executed.len()
            );
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
        pattern_key: Option<&str>,
    ) -> Result<(), String> {
        match action {
            RecoveryAction::Retry {
                delay_ms,
                max_attempts,
            } => self.execute_retry(*delay_ms, *max_attempts, pattern_key),

            RecoveryAction::Restart { component } => {
                if component.trim().is_empty() {
                    return Err("component cannot be empty".to_string());
                }

                info!(
                    "Recovery: restarting component '{}' (restoring last checkpoint)",
                    component
                );

                // A "restart" in the agent context means restore from the last
                // known-good checkpoint so the agent loop re-executes from a
                // clean state.
                if let Some(mgr) = state_manager {
                    if mgr.restore(None).is_some() {
                        info!("Component '{}' state restored from checkpoint", component);
                    } else {
                        debug!(
                            "No checkpoint available for '{}', proceeding with restart signal",
                            component
                        );
                    }
                }
                Ok(())
            }

            RecoveryAction::Fallback { target } => {
                if target.trim().is_empty() {
                    return Err("fallback target cannot be empty".to_string());
                }

                info!("Recovery: activating fallback '{}'", target);

                // Fallback signals the caller to switch strategy. The executor
                // returns success so the agent loop can interpret the action and
                // adjust (e.g. inject error guidance, switch parsing mode).
                Ok(())
            }

            RecoveryAction::RestoreCheckpoint { checkpoint_id } => {
                let manager = state_manager
                    .ok_or_else(|| "state manager unavailable for restore action".to_string())?;

                if let Some(checkpoint) = manager.restore(checkpoint_id.as_deref()) {
                    info!(
                        "Restored checkpoint '{}' ({})",
                        checkpoint.id, checkpoint.description
                    );
                    Ok(())
                } else {
                    Err("no checkpoint available to restore".to_string())
                }
            }

            RecoveryAction::ClearCache { scope } => {
                let manager = state_manager.ok_or_else(|| {
                    "state manager unavailable for clear-cache action".to_string()
                })?;

                info!("Recovery: clearing cache (scope: {})", scope);
                manager.clear();

                // Also clear retry states so the next recovery starts fresh
                if let Ok(mut states) = self.retry_states.write() {
                    states.clear();
                    debug!("Retry states cleared");
                }

                Ok(())
            }

            RecoveryAction::ResetState { scope } => {
                let manager = state_manager.ok_or_else(|| {
                    "state manager unavailable for reset-state action".to_string()
                })?;

                info!("Recovery: resetting state (scope: {})", scope);
                manager.clear();

                // Reset retry tracking
                if let Ok(mut states) = self.retry_states.write() {
                    states.clear();
                }

                Ok(())
            }

            RecoveryAction::Custom { name, params } => {
                if name.trim().is_empty() {
                    return Err("custom action name cannot be empty".to_string());
                }

                info!("Recovery: executing custom action '{}'", name);

                // Handle well-known custom actions
                match name.as_str() {
                    "compress_context" => {
                        // Signal caller to compress the agent's context window
                        info!("Custom action: context compression requested");
                        Ok(())
                    }
                    "reduce_tool_set" => {
                        // Signal caller to reduce available tools
                        info!("Custom action: tool set reduction requested");
                        Ok(())
                    }
                    "switch_parsing_mode" => {
                        // Signal caller to switch between native/XML tool parsing
                        let mode = params.get("mode").map(|s| s.as_str()).unwrap_or("xml");
                        info!("Custom action: switch parsing mode to '{}'", mode);
                        Ok(())
                    }
                    _ => {
                        debug!(
                            "Unknown custom action '{}' with {} params — treating as no-op signal",
                            name,
                            params.len()
                        );
                        Ok(())
                    }
                }
            }
        }
    }

    /// Execute a retry with exponential backoff.
    fn execute_retry(
        &self,
        base_delay_ms: u64,
        max_attempts: u32,
        pattern_key: Option<&str>,
    ) -> Result<(), String> {
        if max_attempts == 0 {
            return Err("max_attempts must be greater than 0".to_string());
        }

        let key = pattern_key.unwrap_or("default").to_string();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Get or create retry state for this pattern
        let (attempt, actual_delay_ms) = {
            let mut states = self
                .retry_states
                .write()
                .map_err(|_| "failed to acquire retry state lock".to_string())?;

            let state = states.entry(key.clone()).or_insert_with(|| RetryState {
                attempt_count: 0,
                last_delay_ms: 0,
                first_attempt_at: now,
            });

            if state.attempt_count >= max_attempts {
                let elapsed = now.saturating_sub(state.first_attempt_at);
                return Err(format!(
                    "Max retry attempts ({}) exhausted for pattern '{}' over {}s",
                    max_attempts, key, elapsed
                ));
            }

            // Exponential backoff: base_delay * 2^attempt, capped at 30s
            let exponent = state.attempt_count.min(5);
            let actual_delay = base_delay_ms.saturating_mul(1u64 << exponent).min(30_000);

            state.attempt_count += 1;
            state.last_delay_ms = actual_delay;

            (state.attempt_count, actual_delay)
        };

        info!(
            "Retry attempt {}/{} for '{}' — backing off {}ms",
            attempt, max_attempts, key, actual_delay_ms
        );

        self.stats.retries_performed.fetch_add(1, Ordering::Relaxed);
        self.stats
            .total_backoff_ms
            .fetch_add(actual_delay_ms, Ordering::Relaxed);

        // Actual sleep — this is the real recovery delay
        if actual_delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(actual_delay_ms));
        }

        debug!(
            "Retry backoff complete for '{}' ({}ms elapsed)",
            key, actual_delay_ms
        );

        Ok(())
    }

    /// Reset retry state for a specific pattern (e.g. after a successful operation).
    pub fn reset_retry_state(&self, pattern_key: &str) {
        if let Ok(mut states) = self.retry_states.write() {
            states.remove(pattern_key);
        }
    }

    /// Get the current retry attempt count for a pattern.
    pub fn retry_attempt_count(&self, pattern_key: &str) -> u32 {
        self.retry_states
            .read()
            .ok()
            .and_then(|states| states.get(pattern_key).map(|s| s.attempt_count))
            .unwrap_or(0)
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
