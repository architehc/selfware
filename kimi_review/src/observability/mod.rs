//! Observability stack for logging, metrics, and tracing

use crate::config::{LoggingConfig, MetricsConfig, TracingConfig};
use crate::error::SelfwareError;
use axum::{routing::get, Json, Router};
use serde::Serialize;
use std::time::Duration;
use tracing::{info, warn};

pub mod alerts;
pub mod logging;
pub mod metrics_collector;
pub mod tracing_setup;

pub use logging::init_logging;
pub use metrics_collector::MetricsCollector;
pub use tracing_setup::init_tracing;

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
    pub session_id: String,
}

/// Create health check router
pub fn health_router() -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/health/live", get(liveness_handler))
        .route("/health/ready", get(readiness_handler))
        .route("/metrics", get(metrics_handler))
}

/// Health handler
async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: 0, // Would track actual uptime
        session_id: crate::get_session_id().to_string(),
    })
}

/// Liveness handler
async fn liveness_handler() -> &'static str {
    "alive"
}

/// Readiness handler
async fn readiness_handler() -> &'static str {
    "ready"
}

/// Metrics handler
async fn metrics_handler() -> Result<String, String> {
    // In a real implementation, this would return Prometheus metrics
    Ok("# Selfware metrics\n".to_string())
}

/// Log entry structure
#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: LogLevel,
    pub target: String,
    pub message: String,
    pub fields: serde_json::Map<String, serde_json::Value>,
    pub session_id: String,
    pub span_context: Vec<SpanContext>,
}

/// Log level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/// Span context for tracing
#[derive(Debug, Clone, Serialize)]
pub struct SpanContext {
    pub name: String,
    pub id: u64,
    pub parent_id: Option<u64>,
}

/// Task event for structured logging
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum TaskEvent {
    Started { task_type: String },
    Checkpointed { checkpoint_id: String },
    Completed { duration_ms: u64, tokens_used: u64 },
    Failed { error: String, recoverable: bool },
}

/// Log a task event
pub fn log_task_event(task_id: &str, event: TaskEvent) {
    match &event {
        TaskEvent::Started { task_type } => {
            tracing::info!(
                task_id = task_id,
                task_type = task_type,
                "Task started"
            );
        }
        TaskEvent::Checkpointed { checkpoint_id } => {
            tracing::info!(
                task_id = task_id,
                checkpoint_id = checkpoint_id,
                "Task checkpointed"
            );
        }
        TaskEvent::Completed { duration_ms, tokens_used } => {
            tracing::info!(
                task_id = task_id,
                duration_ms = duration_ms,
                tokens_used = tokens_used,
                "Task completed"
            );
        }
        TaskEvent::Failed { error, recoverable } => {
            if *recoverable {
                tracing::warn!(
                    task_id = task_id,
                    error = error,
                    "Task failed (recoverable)"
                );
            } else {
                tracing::error!(
                    task_id = task_id,
                    error = error,
                    "Task failed (unrecoverable)"
                );
            }
        }
    }
}

/// Create a tracing span for an agent
pub fn agent_span(agent_id: &str, task_id: Option<&str>) -> tracing::Span {
    tracing::info_span!(
        "agent",
        agent_id = agent_id,
        task_id = task_id.unwrap_or("none"),
        session_id = crate::get_session_id(),
    )
}

/// Create a tracing span for a task
pub fn task_span(task_id: &str, task_type: &str) -> tracing::Span {
    tracing::info_span!(
        "task",
        task_id = task_id,
        task_type = task_type,
    )
}

/// Track operation with timeout
pub async fn track_with_timeout<F, T>(
    operation: F,
    timeout: Duration,
) -> Result<T, SelfwareError>
where
    F: std::future::Future<Output = T>,
{
    match tokio::time::timeout(timeout, operation).await {
        Ok(result) => Ok(result),
        Err(_) => {
            warn!("Operation timed out after {:?}", timeout);
            Err(SelfwareError::Timeout)
        }
    }
}
