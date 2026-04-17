//! Long-running system test harness.
//!
//! This module provides infrastructure for running extended duration tests
//! (8+ hours) that validate Selfware's ability to complete multiple
//! programming tasks across different languages and difficulty levels.
//!
//! # Architecture
//!
//! - `LongRunningConfig`: Configuration for test duration, timeouts, endpoints
//! - `LongRunningRunner`: Executes test tasks and collects results
//! - `LongRunningReport`: Aggregates and formats test results
//! - `ProjectType`/`TaskSetup`: Define project scaffolds (Rust, Python, Go, Templates)
//!
//! # Example
//!
//! ```ignore
//! use selfware::bench_harness::long_running::*;
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = LongRunningConfig::new(
//!         "http://localhost:8000/v1",
//!         "qwen3.5-122b"
//!     )
//!     .with_duration(Duration::from_secs(8 * 3600))
//!     .with_project_timeout(900)
//!     .with_max_iterations(80);
//!
//!     let runner = LongRunningRunner::new(config)?;
//!     let mut report = LongRunningReport::new();
//!
//!     // Run tasks while time remains
//!     while runner.should_continue() {
//!         let task = TestTask {
//!             name: "calculator".into(),
//!             project_type: ProjectType::Rust,
//!             setup: TaskSetup::RustGreenfield {
//!                 name: "calculator".into()
//!             },
//!             prompt: "Create a Calculator...".into(),
//!         };
//!
//!         let result = runner.run_task(&task, &work_dir).await;
//!         report.add_result(result);
//!     }
//!
//!     report.write_to_dir(Path::new("results"))?;
//!     Ok(())
//! }
//! ```

pub mod config;
pub mod project;
pub mod report;
pub mod runner;

pub use config::LongRunningConfig;
pub use project::{ProjectResult, ProjectStatus, ProjectType};
pub use report::{LongRunningReport, RoundSummary};
pub use runner::TaskSetup;
pub use runner::{LongRunningRunner, TestTask};
