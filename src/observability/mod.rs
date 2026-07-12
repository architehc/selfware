//! Observability and analytics module
//!
//! This module contains telemetry and analytics functionality including:
//! - Telemetry collection
//! - Usage analytics
//! - Log analysis
//! - Carbon tracking
//! - Test dashboards

pub mod dashboard;
pub mod telemetry;

#[cfg(feature = "log-analysis")]
pub mod log_analysis;
