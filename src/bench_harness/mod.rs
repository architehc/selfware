//! Concurrent Benchmark Harness
//!
//! High-throughput benchmark runner for evaluating LLM performance with
//! bounded concurrency. Designed for local vLLM endpoints with 32+ concurrent
//! streams.
//!
//! # Architecture
//!
//! - `HarnessConfig`: Endpoint, model, concurrency, and timeout settings
//! - `BenchTask` + `TaskEvaluator`: Pluggable task definitions with custom scoring
//! - `HarnessRunner`: Semaphore-bounded concurrent execution engine
//! - `HarnessReport`: Aggregated throughput, latency percentiles, and per-task scores
//!
//! # Built-in Evaluators
//!
//! - `KeywordEvaluator`: Check for keyword presence in responses
//! - `JsonEvaluator`: Validate JSON structure and required fields
//! - `NoopEvaluator`: Throughput-only benchmarks (always passes)
//!
//! # Example
//!
//! ```ignore
//! use selfware::bench_harness::*;
//!
//! let config = HarnessConfig::new("http://localhost:8000/v1", "qwen3.5-27b");
//! let runner = HarnessRunner::new(config)?;
//!
//! let tasks = vec![
//!     BenchTask {
//!         id: "simple".into(),
//!         description: "Basic math".into(),
//!         messages: vec![Message::user("What is 2+2?")],
//!         evaluator: Box::new(KeywordEvaluator::new(vec!["4".into()])),
//!     },
//! ];
//!
//! let report = runner.run(tasks).await?;
//! report.write_to_dir(Path::new("bench_results"))?;
//! ```

pub mod config;
pub mod report;
pub mod runner;
pub mod task;

pub mod computer_control;
pub mod long_running;

pub use config::HarnessConfig;
pub use report::HarnessReport;
pub use runner::HarnessRunner;
pub use task::{
    BenchTask, EvalDetail, EvalResult, JsonEvaluator, KeywordEvaluator, NoopEvaluator,
    StreamResult, TaskEvaluator,
};
