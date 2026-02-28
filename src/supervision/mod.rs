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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn test_backoff_fixed_duration() {
        let strategy = BackoffStrategy::Fixed { seconds: 5 };
        assert_eq!(strategy.duration(0), Duration::from_secs(5));
        assert_eq!(strategy.duration(1), Duration::from_secs(5));
        assert_eq!(strategy.duration(10), Duration::from_secs(5));
    }

    #[test]
    fn test_backoff_exponential_duration() {
        let strategy = BackoffStrategy::Exponential {
            base_seconds: 1,
            max_seconds: 60,
        };
        assert_eq!(strategy.duration(0), Duration::from_secs(1)); // 1 << 0 = 1
        assert_eq!(strategy.duration(1), Duration::from_secs(2)); // 1 << 1 = 2
        assert_eq!(strategy.duration(2), Duration::from_secs(4)); // 1 << 2 = 4
        assert_eq!(strategy.duration(3), Duration::from_secs(8)); // 1 << 3 = 8
    }

    #[test]
    fn test_backoff_exponential_respects_max() {
        let strategy = BackoffStrategy::Exponential {
            base_seconds: 1,
            max_seconds: 10,
        };
        assert_eq!(strategy.duration(5), Duration::from_secs(10)); // 1 << 5 = 32, capped at 10
        assert_eq!(strategy.duration(20), Duration::from_secs(10));
    }

    #[test]
    fn test_backoff_exponential_overflow_uses_max() {
        let strategy = BackoffStrategy::Exponential {
            base_seconds: 1,
            max_seconds: 100,
        };
        // Shifting by 64+ bits should overflow, falling back to max_seconds
        assert_eq!(strategy.duration(64), Duration::from_secs(100));
    }

    #[test]
    fn test_supervisor_builder_defaults() {
        let supervisor = Supervisor::builder().build();
        assert!(matches!(
            supervisor.strategy,
            SupervisionStrategy::OneForOne
        ));
        assert_eq!(supervisor.restart_policy.max_restarts, 5);
        assert_eq!(supervisor.restart_policy.max_seconds, 60);
        assert!(supervisor.children.is_empty());
    }

    #[test]
    fn test_supervisor_builder_with_strategy() {
        let supervisor = Supervisor::builder()
            .with_strategy(SupervisionStrategy::OneForAll)
            .build();
        assert!(matches!(
            supervisor.strategy,
            SupervisionStrategy::OneForAll
        ));
    }

    #[test]
    fn test_supervisor_builder_with_restart_policy() {
        let policy = RestartPolicy {
            max_restarts: 10,
            max_seconds: 120,
            backoff_strategy: BackoffStrategy::Fixed { seconds: 3 },
        };
        let supervisor = Supervisor::builder()
            .with_restart_policy(policy)
            .build();
        assert_eq!(supervisor.restart_policy.max_restarts, 10);
        assert_eq!(supervisor.restart_policy.max_seconds, 120);
    }

    #[test]
    fn test_supervisor_builder_add_child() {
        struct DummyComponent;
        let component: Arc<dyn Send + Sync> = Arc::new(DummyComponent);

        let supervisor = Supervisor::builder()
            .add_child("worker-1", component.clone())
            .add_child("worker-2", component)
            .build();

        assert_eq!(supervisor.children.len(), 2);
        assert_eq!(supervisor.children[0].id, "worker-1");
        assert_eq!(supervisor.children[1].id, "worker-2");
        assert_eq!(supervisor.children[0].restart_type, RestartType::Permanent);
    }

    #[test]
    fn test_child_spec_defaults() {
        let spec = ChildSpec {
            id: "test".into(),
            restart_type: RestartType::Permanent,
            max_restarts: None,
        };
        assert_eq!(spec.id, "test");
        assert_eq!(spec.restart_type, RestartType::Permanent);
        assert!(spec.max_restarts.is_none());
    }

    #[test]
    fn test_restart_type_variants() {
        assert_ne!(RestartType::Permanent, RestartType::Transient);
        assert_ne!(RestartType::Transient, RestartType::Temporary);
        assert_eq!(RestartType::Permanent, RestartType::Permanent);
    }

    #[test]
    fn test_exit_reason_variants() {
        assert_eq!(ExitReason::Normal, ExitReason::Normal);
        assert_ne!(ExitReason::Normal, ExitReason::Error);
        assert_ne!(ExitReason::Error, ExitReason::Killed);
        assert_ne!(ExitReason::Killed, ExitReason::Timeout);
    }

    #[tokio::test]
    async fn test_supervisor_start_returns_handle() {
        let supervisor = Supervisor::builder().build();
        let handle = supervisor.start().await;

        assert!(handle.is_ok());
        let handle = handle.unwrap();

        // Verify we can send events through the handle
        let result = handle
            .tx
            .send(ChildEvent::Heartbeat {
                child_id: "test".into(),
            })
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_supervisor_handle_send_crash_event() {
        let supervisor = Supervisor::builder().build();
        let handle = supervisor.start().await.unwrap();

        let result = handle
            .tx
            .send(ChildEvent::Crashed {
                child_id: "worker-1".into(),
                error: "out of memory".into(),
            })
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_supervisor_handle_send_exit_event() {
        let supervisor = Supervisor::builder().build();
        let handle = supervisor.start().await.unwrap();

        let result = handle
            .tx
            .send(ChildEvent::Exited {
                child_id: "worker-1".into(),
                reason: ExitReason::Normal,
            })
            .await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_restart_policy_serialization() {
        let policy = RestartPolicy {
            max_restarts: 3,
            max_seconds: 30,
            backoff_strategy: BackoffStrategy::Fixed { seconds: 2 },
        };
        let json = serde_json::to_string(&policy).unwrap();
        let deserialized: RestartPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.max_restarts, 3);
        assert_eq!(deserialized.max_seconds, 30);
    }

    #[test]
    fn test_supervision_strategy_serialization() {
        let strategies = vec![
            SupervisionStrategy::OneForOne,
            SupervisionStrategy::OneForAll,
            SupervisionStrategy::RestForOne,
        ];
        for strategy in strategies {
            let json = serde_json::to_string(&strategy).unwrap();
            let _: SupervisionStrategy = serde_json::from_str(&json).unwrap();
        }
    }
}
