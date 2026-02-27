//! Process supervision and recovery mechanisms

use crate::errors::SelfwareError;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

pub mod circuit_breaker;
pub mod health;

pub use circuit_breaker::CircuitBreaker;
pub use health::{HealthCheck, HealthMonitor, HealthStatus};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum BackoffStrategy {
    Exponential { base_seconds: u64, max_seconds: u64 },
    Fixed { seconds: u64 },
}

impl BackoffStrategy {
    pub fn duration(&self, attempt: u32) -> Duration {
        match self {
            Self::Exponential {
                base_seconds,
                max_seconds,
            } => {
                let secs = (*base_seconds).checked_shl(attempt).unwrap_or(*max_seconds);
                Duration::from_secs(secs.min(*max_seconds))
            }
            Self::Fixed { seconds } => Duration::from_secs(*seconds),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RestartPolicy {
    pub max_restarts: u32,
    pub max_seconds: u32,
    pub backoff_strategy: BackoffStrategy,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SupervisionStrategy {
    OneForOne,
    OneForAll,
    RestForOne,
}

/// Supervisor for managing child processes/components
pub struct Supervisor {
    strategy: SupervisionStrategy,
    restart_policy: RestartPolicy,
    children: Vec<ChildSpec>,
}

/// Child specification
#[derive(Debug, Clone)]
pub struct ChildSpec {
    pub id: String,
    pub restart_type: RestartType,
    pub max_restarts: Option<u32>,
}

/// Restart type for child
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartType {
    Permanent, // Always restart
    Transient, // Restart only on abnormal exit
    Temporary, // Never restart
}

/// Child event from supervised component
#[derive(Debug, Clone)]
pub enum ChildEvent {
    Crashed {
        child_id: String,
        error: String,
    },
    Exited {
        child_id: String,
        reason: ExitReason,
    },
    Heartbeat {
        child_id: String,
    },
}

/// Exit reason
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitReason {
    Normal,
    Error,
    Killed,
    Timeout,
}

/// Supervisor handle
#[derive(Debug, Clone)]
pub struct SupervisorHandle {
    pub tx: mpsc::Sender<ChildEvent>,
}

/// Supervisor builder
pub struct SupervisorBuilder {
    strategy: SupervisionStrategy,
    restart_policy: RestartPolicy,
    children: Vec<ChildSpec>,
}

impl Supervisor {
    /// Create a new supervisor builder
    pub fn builder() -> SupervisorBuilder {
        SupervisorBuilder {
            strategy: SupervisionStrategy::OneForOne,
            restart_policy: RestartPolicy {
                max_restarts: 5,
                max_seconds: 60,
                backoff_strategy: BackoffStrategy::Exponential {
                    base_seconds: 1,
                    max_seconds: 60,
                },
            },
            children: Vec::new(),
        }
    }

    /// Start the supervisor
    pub async fn start(self) -> Result<SupervisorHandle, SelfwareError> {
        let (tx, mut rx) = mpsc::channel(100);

        let restart_counts: Arc<RwLock<HashMap<String, Vec<Instant>>>> =
            Arc::new(RwLock::new(HashMap::new()));

        tokio::spawn(async move {
            info!("Supervision tree started");

            while let Some(event) = rx.recv().await {
                match event {
                    ChildEvent::Crashed { child_id, error } => {
                        error!(child_id = %child_id, error = %error, "Child crashed");

                        if self.should_restart(&child_id, &restart_counts).await {
                            let backoff = self.calculate_backoff(&child_id, &restart_counts).await;
                            warn!(child_id = %child_id, backoff_ms = backoff.as_millis(), "Restarting child");

                            tokio::time::sleep(backoff).await;
                            self.restart_child(&child_id).await;
                        } else {
                            error!(child_id = %child_id, "Max restarts exceeded, escalating");
                            self.escalate(&child_id, &error).await;
                        }
                    }
                    ChildEvent::Exited { child_id, reason } => {
                        if reason != ExitReason::Normal {
                            warn!(child_id = %child_id, reason = ?reason, "Child exited abnormally");
                            self.handle_abnormal_exit(&child_id, reason).await;
                        } else {
                            debug!(child_id = %child_id, "Child exited normally");
                        }
                    }
                    ChildEvent::Heartbeat { child_id } => {
                        debug!(child_id = %child_id, "Heartbeat received");
                    }
                }
            }

            info!("Supervision tree stopped");
        });

        Ok(SupervisorHandle { tx })
    }

    /// Check if a child should be restarted
    async fn should_restart(
        &self,
        child_id: &str,
        restart_counts: &Arc<RwLock<HashMap<String, Vec<Instant>>>>,
    ) -> bool {
        let counts = restart_counts.read().await;

        if let Some(times) = counts.get(child_id) {
            let window_start =
                Instant::now() - Duration::from_secs(self.restart_policy.max_seconds as u64);
            let recent_restarts = times.iter().filter(|&&t| t > window_start).count();

            recent_restarts < self.restart_policy.max_restarts as usize
        } else {
            true
        }
    }

    /// Calculate backoff duration
    async fn calculate_backoff(
        &self,
        child_id: &str,
        restart_counts: &Arc<RwLock<HashMap<String, Vec<Instant>>>>,
    ) -> Duration {
        let counts = restart_counts.read().await;

        let attempt = counts.get(child_id).map(|v| v.len() as u32).unwrap_or(0);

        self.restart_policy.backoff_strategy.duration(attempt)
    }

    /// Restart a child
    async fn restart_child(&self, child_id: &str) {
        info!(child_id = %child_id, "Restarting child");
        // In a real implementation, this would restart the actual component
    }

    /// Handle abnormal exit
    async fn handle_abnormal_exit(&self, child_id: &str, reason: ExitReason) {
        warn!(child_id = %child_id, reason = ?reason, "Handling abnormal exit");

        match self.strategy {
            SupervisionStrategy::OneForAll => {
                // Restart all children
                for child in &self.children {
                    if child.id != child_id {
                        self.restart_child(&child.id).await;
                    }
                }
            }
            SupervisionStrategy::RestForOne => {
                // Restart failed child and all children started after it
                let mut restart = false;
                for child in &self.children {
                    if child.id == child_id {
                        restart = true;
                    }
                    if restart {
                        self.restart_child(&child.id).await;
                    }
                }
            }
            SupervisionStrategy::OneForOne => {
                // Only restart the failed child
                self.restart_child(child_id).await;
            }
        }
    }

    /// Escalate failure to parent supervisor
    async fn escalate(&self, child_id: &str, error: &str) {
        error!(child_id = %child_id, error = %error, "Escalating failure");
        // In a real implementation, this would notify a parent supervisor
        // or trigger system-wide recovery
    }
}

impl SupervisorBuilder {
    /// Set supervision strategy
    pub fn with_strategy(mut self, strategy: SupervisionStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Set restart policy
    pub fn with_restart_policy(mut self, policy: RestartPolicy) -> Self {
        self.restart_policy = policy;
        self
    }

    /// Add a child to supervise
    pub fn add_child(mut self, id: impl Into<String>, _component: Arc<dyn Send + Sync>) -> Self {
        self.children.push(ChildSpec {
            id: id.into(),
            restart_type: RestartType::Permanent,
            max_restarts: None,
        });
        self
    }

    /// Build the supervisor
    pub fn build(self) -> Supervisor {
        Supervisor {
            strategy: self.strategy,
            restart_policy: self.restart_policy,
            children: self.children,
        }
    }
}
