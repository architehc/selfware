//! Metrics collection and export

use crate::agent::CompletedTask;
use crate::config::MetricsConfig;
use crate::error::SelfwareError;
use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::PrometheusBuilder;
use std::net::SocketAddr;
use tracing::info;

/// Metrics collector for system metrics
pub struct MetricsCollector;

impl MetricsCollector {
    /// Initialize metrics collection
    pub fn init(config: &MetricsConfig) -> Result<(), SelfwareError> {
        if !config.enabled {
            return Ok(());
        }
        
        let addr: SocketAddr = ([0, 0, 0, 0], config.prometheus_port).into();
        
        PrometheusBuilder::new()
            .with_http_listener(addr)
            .install_recorder()
            .map_err(|e| SelfwareError::Config(format!("Failed to install Prometheus recorder: {}", e)))?;
        
        // Describe metrics
        metrics::describe_counter!(
            "selfware_tasks_completed_total",
            "Total number of completed tasks"
        );
        metrics::describe_counter!(
            "selfware_tasks_failed_total",
            "Total number of failed tasks"
        );
        metrics::describe_gauge!(
            "selfware_active_agents",
            "Number of currently active agents"
        );
        metrics::describe_gauge!(
            "selfware_context_tokens",
            "Current context window token count"
        );
        metrics::describe_gauge!(
            "selfware_gpu_memory_used_bytes",
            "GPU memory used in bytes"
        );
        metrics::describe_histogram!(
            "selfware_inference_duration_seconds",
            "Inference request duration"
        );
        metrics::describe_histogram!(
            "selfware_checkpoint_size_bytes",
            "Size of checkpoints in bytes"
        );
        metrics::describe_gauge!(
            "selfware_system_uptime_seconds",
            "System uptime in seconds"
        );
        
        info!(address = %addr, "Metrics collection initialized");
        
        Ok(())
    }
    
    /// Record task completion
    pub fn record_task_completion(task: &CompletedTask) {
        counter!(
            "selfware_tasks_completed_total",
            "task_type" => task.task.task_type.clone()
        )
        .increment(1);
        
        histogram!(
            "selfware_task_duration_seconds",
            "task_type" => task.task.task_type.clone()
        )
        .record(task.result.duration_ms as f64 / 1000.0);
        
        histogram!(
            "selfware_task_tokens_used",
            "task_type" => task.task.task_type.clone()
        )
        .record(task.result.tokens_used as f64);
    }
    
    /// Record task failure
    pub fn record_task_failure(task_type: &str) {
        counter!(
            "selfware_tasks_failed_total",
            "task_type" => task_type.to_string()
        )
        .increment(1);
    }
    
    /// Update system metrics
    pub fn update_system_metrics(state: &SystemMetrics) {
        gauge!("selfware_active_agents").set(state.active_agents as f64);
        gauge!("selfware_queued_tasks").set(state.queued_tasks as f64);
        gauge!("selfware_context_tokens").set(state.context_tokens as f64);
        gauge!("selfware_system_uptime_seconds").set(state.uptime_seconds as f64);
    }
    
    /// Update resource metrics
    pub fn update_resource_metrics(metrics: &ResourceMetrics) {
        gauge!("selfware_memory_used_bytes").set(metrics.memory_used as f64);
        gauge!("selfware_gpu_memory_used_bytes").set(metrics.gpu_memory_used as f64);
        gauge!("selfware_gpu_temperature").set(metrics.gpu_temperature as f64);
        gauge!("selfware_disk_used_bytes").set(metrics.disk_used as f64);
    }
    
    /// Record checkpoint creation
    pub fn record_checkpoint(size_bytes: u64, level: &str) {
        counter!(
            "selfware_checkpoints_created_total",
            "level" => level.to_string()
        )
        .increment(1);
        
        histogram!("selfware_checkpoint_size_bytes")
            .record(size_bytes as f64);
    }
    
    /// Record inference metrics
    pub fn record_inference(duration_ms: u64, tokens_generated: usize) {
        histogram!("selfware_inference_duration_seconds")
            .record(duration_ms as f64 / 1000.0);
        
        histogram!("selfware_inference_tokens_generated")
            .record(tokens_generated as f64);
    }
}

/// System metrics snapshot
#[derive(Debug, Clone, Default)]
pub struct SystemMetrics {
    pub active_agents: usize,
    pub queued_tasks: usize,
    pub context_tokens: usize,
    pub uptime_seconds: u64,
}

/// Resource metrics snapshot
#[derive(Debug, Clone, Default)]
pub struct ResourceMetrics {
    pub memory_used: u64,
    pub gpu_memory_used: u64,
    pub gpu_temperature: u32,
    pub disk_used: u64,
}

/// Task metrics
#[derive(Debug, Clone, Default)]
pub struct TaskMetrics {
    pub total_tasks: u64,
    pub successful_tasks: u64,
    pub failed_tasks: u64,
    pub average_duration_ms: u64,
    pub total_tokens_used: u64,
}
