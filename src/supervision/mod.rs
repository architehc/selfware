//! Process supervision and recovery mechanisms

use crate::errors::SelfwareError;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// Component factory trait for creating child components
pub trait ComponentFactory: Send + Sync {
    /// Create a new component instance
    fn create(&self) -> Arc<dyn Send + Sync>;
    /// Stop the component
    fn stop(&self, component: &Arc<dyn Send + Sync>);
}

/// Child state tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildState {
    Starting,
    Running,
    Restarting,
    Stopped,
    Failed,
}

/// Child runtime information
struct ChildRuntime {
    state: ChildState,
    instance: Option<Arc<dyn Send + Sync>>,
    last_started: Option<Instant>,
    restart_count: u32,
}

impl std::fmt::Debug for ChildRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChildRuntime")
            .field("state", &self.state)
            .field("has_instance", &self.instance.is_some())
            .field("last_started", &self.last_started)
            .field("restart_count", &self.restart_count)
            .finish()
    }
}

pub mod circuit_breaker;
pub mod health;
pub mod run_registry;
pub mod run_supervisor;

pub use circuit_breaker::CircuitBreaker;
pub use health::{HealthCheck, HealthMonitor, HealthStatus};
pub use run_registry::{AbortOutcome, RunRecord, RunRegistry};
pub use run_supervisor::{RunId, RunStatus, RunSupervisor};

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
    /// Parent supervisor notification channel
    parent_tx: Option<mpsc::Sender<ParentNotification>>,
}

/// Notification sent to parent supervisor
#[derive(Debug, Clone)]
pub enum ParentNotification {
    /// Child exceeded max restarts
    Escalation {
        supervisor_id: String,
        child_id: String,
        error: String,
        restart_count: u32,
    },
    /// Child state changed
    StateChange {
        child_id: String,
        old_state: ChildState,
        new_state: ChildState,
    },
}

/// Child specification
#[derive(Clone)]
pub struct ChildSpec {
    pub id: String,
    pub restart_type: RestartType,
    pub max_restarts: Option<u32>,
    /// Factory for creating component instances
    pub factory: Option<Arc<dyn ComponentFactory>>,
}

impl std::fmt::Debug for ChildSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChildSpec")
            .field("id", &self.id)
            .field("restart_type", &self.restart_type)
            .field("max_restarts", &self.max_restarts)
            .field("has_factory", &self.factory.is_some())
            .finish()
    }
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
    parent_tx: Option<mpsc::Sender<ParentNotification>>,
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
            parent_tx: None,
        }
    }

    /// Start the supervisor
    pub async fn start(self) -> Result<SupervisorHandle, SelfwareError> {
        let (tx, mut rx) = mpsc::channel(100);

        let restart_counts: Arc<RwLock<HashMap<String, Vec<Instant>>>> =
            Arc::new(RwLock::new(HashMap::new()));

        // Initialize child runtime state
        let child_states: Arc<RwLock<HashMap<String, ChildRuntime>>> =
            Arc::new(RwLock::new(HashMap::new()));

        // Initialize all children as stopped
        {
            let mut states = child_states.write().await;
            for child in &self.children {
                states.insert(
                    child.id.clone(),
                    ChildRuntime {
                        state: ChildState::Stopped,
                        instance: None,
                        last_started: None,
                        restart_count: 0,
                    },
                );
            }
        }

        let supervisor_id = self
            .parent_tx
            .as_ref()
            .map(|_| format!("supervisor-{:p}", &self))
            .unwrap_or_default();

        tokio::spawn(async move {
            info!("Supervision tree started");

            // Start all permanent children initially
            for child in &self.children {
                if child.restart_type == RestartType::Permanent {
                    if let Err(e) = self.start_child(&child.id, &child_states).await {
                        error!(child_id = %child.id, error = %e, "Failed to start child");
                    }
                }
            }

            while let Some(event) = rx.recv().await {
                match event {
                    ChildEvent::Crashed { child_id, error } => {
                        error!(child_id = %child_id, error = %error, "Child crashed");

                        // Update child state to failed
                        {
                            let mut states = child_states.write().await;
                            if let Some(runtime) = states.get_mut(&child_id) {
                                runtime.state = ChildState::Failed;
                            }
                        }

                        // Check if we should restart based on restart type and policy
                        let should_restart = self
                            .should_restart_child(&child_id, &restart_counts, &error)
                            .await;

                        if should_restart {
                            if self.should_restart(&child_id, &restart_counts).await {
                                // Record this restart attempt *before*
                                // calculating backoff so the exponential
                                // strategy sees an escalating attempt count.
                                {
                                    let mut counts = restart_counts.write().await;
                                    counts
                                        .entry(child_id.clone())
                                        .or_default()
                                        .push(Instant::now());
                                }
                                let backoff =
                                    self.calculate_backoff(&child_id, &restart_counts).await;
                                warn!(child_id = %child_id, backoff_ms = backoff.as_millis(), "Restarting child");

                                tokio::time::sleep(backoff).await;
                                if let Err(e) = self.restart_child(&child_id, &child_states).await {
                                    error!(child_id = %child_id, error = %e, "Failed to restart child, escalating");
                                    self.escalate(
                                        &supervisor_id,
                                        &child_id,
                                        &error,
                                        &restart_counts,
                                    )
                                    .await;
                                }
                            } else {
                                error!(child_id = %child_id, "Max restarts exceeded, escalating");
                                self.escalate(&supervisor_id, &child_id, &error, &restart_counts)
                                    .await;
                            }
                        } else {
                            info!(child_id = %child_id, "Child will not be restarted based on restart type");
                        }
                    }
                    ChildEvent::Exited { child_id, reason } => {
                        if reason != ExitReason::Normal {
                            warn!(child_id = %child_id, reason = ?reason, "Child exited abnormally");
                            self.handle_abnormal_exit(
                                &child_id,
                                reason,
                                &child_states,
                                &restart_counts,
                            )
                            .await;
                        } else {
                            debug!(child_id = %child_id, "Child exited normally");
                            // Mark as stopped on normal exit
                            let mut states = child_states.write().await;
                            if let Some(runtime) = states.get_mut(&child_id) {
                                runtime.state = ChildState::Stopped;
                                runtime.instance = None;
                            }
                        }
                    }
                    ChildEvent::Heartbeat { child_id } => {
                        debug!(child_id = %child_id, "Heartbeat received");
                    }
                }
            }

            // Stop all children when supervisor shuts down
            for child in &self.children {
                if let Err(e) = self.stop_child(&child.id, &child_states).await {
                    error!(child_id = %child.id, error = %e, "Error stopping child during shutdown");
                }
            }

            info!("Supervision tree stopped");
        });

        Ok(SupervisorHandle { tx })
    }

    /// Check if a child should be restarted based on its restart type
    async fn should_restart_child(
        &self,
        child_id: &str,
        _restart_counts: &Arc<RwLock<HashMap<String, Vec<Instant>>>>,
        _error: &str,
    ) -> bool {
        // Find the child spec
        let child_spec = match self.children.iter().find(|c| c.id == child_id) {
            Some(spec) => spec,
            None => {
                warn!(child_id = %child_id, "Unknown child crashed");
                return false;
            }
        };

        match child_spec.restart_type {
            RestartType::Permanent => true,
            RestartType::Transient => {
                // Only restart on abnormal exit (crashes)
                // We're in the Crashed handler, so this is always abnormal
                true
            }
            RestartType::Temporary => {
                // Never restart temporary children
                info!(child_id = %child_id, "Temporary child crashed, not restarting");
                false
            }
        }
    }

    /// Start a child component
    async fn start_child(
        &self,
        child_id: &str,
        child_states: &Arc<RwLock<HashMap<String, ChildRuntime>>>,
    ) -> Result<(), SelfwareError> {
        let child_spec = self
            .children
            .iter()
            .find(|c| c.id == child_id)
            .ok_or_else(|| SelfwareError::Internal(format!("Child {} not found", child_id)))?;

        // Get the factory
        let factory = child_spec
            .factory
            .as_ref()
            .ok_or_else(|| SelfwareError::Internal(format!("Child {} has no factory", child_id)))?;

        // Update state to starting
        {
            let mut states = child_states.write().await;
            if let Some(runtime) = states.get_mut(child_id) {
                runtime.state = ChildState::Starting;
            }
        }

        info!(child_id = %child_id, "Starting child component");

        // Create new instance
        let instance = factory.create();

        // Update state to running
        {
            let mut states = child_states.write().await;
            if let Some(runtime) = states.get_mut(child_id) {
                runtime.state = ChildState::Running;
                runtime.instance = Some(instance);
                runtime.last_started = Some(Instant::now());
            }
        }

        info!(child_id = %child_id, "Child component started successfully");
        Ok(())
    }

    /// Stop a child component
    async fn stop_child(
        &self,
        child_id: &str,
        child_states: &Arc<RwLock<HashMap<String, ChildRuntime>>>,
    ) -> Result<(), SelfwareError> {
        let child_spec = self
            .children
            .iter()
            .find(|c| c.id == child_id)
            .ok_or_else(|| SelfwareError::Internal(format!("Child {} not found", child_id)))?;

        let mut states = child_states.write().await;
        if let Some(runtime) = states.get_mut(child_id) {
            if let Some(ref instance) = runtime.instance {
                if let Some(ref factory) = child_spec.factory {
                    factory.stop(instance);
                    info!(child_id = %child_id, "Child component stopped");
                }
            }
            runtime.state = ChildState::Stopped;
            runtime.instance = None;
        }

        Ok(())
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
        _restart_counts: &Arc<RwLock<HashMap<String, Vec<Instant>>>>,
    ) -> Duration {
        let counts = _restart_counts.read().await;

        let attempt = counts.get(child_id).map(|v| v.len() as u32).unwrap_or(0);

        self.restart_policy.backoff_strategy.duration(attempt)
    }

    /// Restart a child
    async fn restart_child(
        &self,
        child_id: &str,
        child_states: &Arc<RwLock<HashMap<String, ChildRuntime>>>,
    ) -> Result<(), SelfwareError> {
        info!(child_id = %child_id, "Restarting child component");

        // Step 1: Update state to restarting
        {
            let mut states = child_states.write().await;
            if let Some(runtime) = states.get_mut(child_id) {
                let old_state = runtime.state;
                runtime.state = ChildState::Restarting;
                runtime.restart_count += 1;
                info!(
                    child_id = %child_id,
                    restart_count = runtime.restart_count,
                    old_state = ?old_state,
                    "Child state changed to restarting"
                );
            }
        }

        // Step 2: Stop the failed component
        self.stop_child(child_id, child_states).await?;
        info!(child_id = %child_id, "Failed component stopped");

        // Step 3: Clear the old state (instance already cleared in stop_child)
        {
            let mut states = child_states.write().await;
            if let Some(runtime) = states.get_mut(child_id) {
                // Keep restart_count but clear instance reference
                runtime.instance = None;
            }
        }
        info!(child_id = %child_id, "Child state cleared");

        // Step 4: Start a fresh instance
        self.start_child(child_id, child_states).await?;
        info!(child_id = %child_id, "Fresh child instance started");

        // Step 5: Update health tracking - record successful restart
        {
            let mut states = child_states.write().await;
            if let Some(runtime) = states.get_mut(child_id) {
                runtime.state = ChildState::Running;
                info!(
                    child_id = %child_id,
                    state = ?runtime.state,
                    restart_count = runtime.restart_count,
                    "Health tracking updated after successful restart"
                );
            }
        }

        Ok(())
    }

    /// Handle abnormal exit
    async fn handle_abnormal_exit(
        &self,
        child_id: &str,
        reason: ExitReason,
        child_states: &Arc<RwLock<HashMap<String, ChildRuntime>>>,
        _restart_counts: &Arc<RwLock<HashMap<String, Vec<Instant>>>>,
    ) {
        warn!(child_id = %child_id, reason = ?reason, "Handling abnormal exit");

        // Find the child spec to check restart type
        let child_spec = match self.children.iter().find(|c| c.id == child_id) {
            Some(spec) => spec,
            None => {
                warn!(child_id = %child_id, "Unknown child exited abnormally");
                return;
            }
        };

        // Check if we should restart based on restart type
        let should_restart = match child_spec.restart_type {
            RestartType::Permanent => true,
            RestartType::Transient => reason != ExitReason::Normal,
            RestartType::Temporary => false,
        };

        if !should_restart {
            info!(child_id = %child_id, "Child will not be restarted based on restart type");
            return;
        }

        match self.strategy {
            SupervisionStrategy::OneForAll => {
                // Restart all children
                info!("OneForAll strategy: restarting all children");
                for child in &self.children {
                    if let Err(e) = self.restart_child(&child.id, child_states).await {
                        error!(child_id = %child.id, error = %e, "Failed to restart child in OneForAll");
                    }
                }
            }
            SupervisionStrategy::RestForOne => {
                // Restart failed child and all children started after it
                info!("RestForOne strategy: restarting failed child and subsequent children");
                let mut restart = false;
                for child in &self.children {
                    if child.id == child_id {
                        restart = true;
                    }
                    if restart {
                        if let Err(e) = self.restart_child(&child.id, child_states).await {
                            error!(child_id = %child.id, error = %e, "Failed to restart child in RestForOne");
                        }
                    }
                }
            }
            SupervisionStrategy::OneForOne => {
                // Only restart the failed child
                info!("OneForOne strategy: restarting failed child only");
                if let Err(e) = self.restart_child(child_id, child_states).await {
                    error!(child_id = %child_id, error = %e, "Failed to restart child in OneForOne");
                }
            }
        }
    }

    /// Escalate failure to parent supervisor
    async fn escalate(
        &self,
        supervisor_id: &str,
        child_id: &str,
        error: &str,
        _restart_counts: &Arc<RwLock<HashMap<String, Vec<Instant>>>>,
    ) {
        // Get current restart count for context
        let restart_count = {
            let counts = _restart_counts.read().await;
            counts.get(child_id).map(|v| v.len() as u32).unwrap_or(0)
        };

        error!(
            child_id = %child_id,
            error = %error,
            restart_count = restart_count,
            "Escalating failure to parent supervisor"
        );

        // Notify parent supervisor if one exists
        if let Some(ref parent_tx) = self.parent_tx {
            let notification = ParentNotification::Escalation {
                supervisor_id: supervisor_id.to_string(),
                child_id: child_id.to_string(),
                error: error.to_string(),
                restart_count,
            };

            match parent_tx.try_send(notification) {
                Ok(()) => {
                    info!(
                        child_id = %child_id,
                        "Successfully notified parent supervisor of escalation"
                    );
                }
                Err(mpsc::error::TrySendError::Full(_)) => {
                    error!(
                        child_id = %child_id,
                        "Parent supervisor channel full, dropping escalation notification"
                    );
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    error!(
                        child_id = %child_id,
                        "Parent supervisor channel closed, cannot escalate"
                    );
                }
            }
        } else {
            // No parent supervisor - this is the top-level supervisor
            error!(
                child_id = %child_id,
                "No parent supervisor available. This is a critical failure at the top level."
            );
            // Could trigger system-wide recovery here (e.g., shutdown, alert)
        }
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

    /// Set parent supervisor for escalation notifications
    pub fn with_parent(mut self, parent_tx: mpsc::Sender<ParentNotification>) -> Self {
        self.parent_tx = Some(parent_tx);
        self
    }

    /// Add a child to supervise
    pub fn add_child(mut self, id: impl Into<String>, factory: Arc<dyn ComponentFactory>) -> Self {
        self.children.push(ChildSpec {
            id: id.into(),
            restart_type: RestartType::Permanent,
            max_restarts: None,
            factory: Some(factory),
        });
        self
    }

    /// Build the supervisor
    pub fn build(self) -> Supervisor {
        Supervisor {
            strategy: self.strategy,
            restart_policy: self.restart_policy,
            children: self.children,
            parent_tx: self.parent_tx,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/supervision/mod_test.rs"]
mod tests;
