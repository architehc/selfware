//! Observability and analytics module
//!
//! This module contains telemetry and analytics functionality including:
//! - Telemetry collection
//! - Usage analytics
//! - Log analysis
//! - Carbon tracking
//! - Test dashboards

#![allow(unused_imports)]

pub mod dashboard;
pub mod telemetry;

#[cfg(feature = "log-analysis")]
pub mod log_analysis;

pub use dashboard::TokenUsage;
#[cfg(feature = "log-analysis")]
pub use log_analysis::{
    Alert, AlertSeverity, Anomaly, AnomalyDetector, LogAnalyzer, LogEntry, LogFormat, LogLevel,
    LogPattern, RootCause, RootCauseAnalyzer, RootCauseCategory,
};
pub use telemetry::{
    increment_log_count, init_tracing, log_entry_count, rotate_if_needed, sampling_rate,
    sanitize_for_log, set_sampling_rate, should_sample, MAX_LOG_ENTRIES,
};
