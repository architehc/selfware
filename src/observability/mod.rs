//! Observability and analytics module
//!
//! This module contains telemetry and analytics functionality including:
//! - Telemetry collection
//! - Usage analytics
//! - Log analysis
//! - Carbon tracking
//! - Test dashboards

pub mod telemetry;
pub mod analytics;
pub mod carbon_tracker;
pub mod dashboard;
pub mod test_dashboard;

#[cfg(feature = "log-analysis")]
pub mod log_analysis;
