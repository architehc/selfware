//! Concurrent Benchmark Harness
//!
//! High-throughput benchmark runner for evaluating LLM performance with
//! bounded concurrency.  Designed for local vLLM/sglang endpoints that can
//! handle 32+ concurrent streams.
//!
//! # Architecture
//!
//! ```text
//! ┌────────────┐     ┌──────────────┐     ┌───────────────┐
//! │ BenchTask  │────►│ HarnessRunner│────►│ HarnessReport │
//! │ + Evaluator│     │ (semaphore-  │     │ (latency p50/ │
//! │            │     │  bounded I/O)│     │  p95/p99, pass│
//! └────────────┘     └──────────────┘     │  rate, tok/s) │
//!                          │              └───────────────┘
//!                    HarnessConfig
//!                    (endpoint, model,
//!                     concurrency, timeout)
//! ```
//!
//! - [`HarnessConfig`]: Endpoint URL, model name, concurrency limit, and
//!   per-request timeout.
//! - [`BenchTask`] + [`TaskEvaluator`]: Each task carries a prompt and an
//!   evaluator that scores the response.  Evaluators are pluggable -- implement
//!   the trait or use a built-in.
//! - [`HarnessRunner`]: Sends all tasks concurrently (bounded by a tokio
//!   semaphore), collects streaming responses, and runs evaluators.
//! - [`HarnessReport`]: Aggregated results -- throughput (tokens/sec), latency
//!   percentiles (p50/p95/p99), per-task pass/fail with [`EvalDetail`]s.
//!   Can be serialized to JSON and written to disk.
//!
//! # Built-in evaluators
//!
//! - [`KeywordEvaluator`]: Passes if all specified keywords appear in the response.
//! - [`JsonEvaluator`]: Passes if the response is valid JSON containing required fields.
//! - [`NoopEvaluator`]: Always passes -- useful for throughput-only benchmarks.
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
//! println!("Pass rate: {:.1}%", report.pass_rate() * 100.0);
//! report.write_to_dir(Path::new("bench_results"))?;
//! ```

pub mod config;
pub mod report;
pub mod runner;
pub mod task;

pub mod computer_control;

pub use config::HarnessConfig;
pub use report::HarnessReport;
pub use runner::HarnessRunner;
pub use task::{
    BenchTask, EvalDetail, EvalResult, JsonEvaluator, KeywordEvaluator, NoopEvaluator,
    StreamResult, TaskEvaluator,
};
