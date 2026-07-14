//! Computer Control Benchmarks
//!
//! Web navigation and interaction benchmarks that test agent capability
//! using browser automation. Captures interaction traces with timestamps,
//! screenshots, and action outcomes for downstream memory consolidation.

pub mod bench_runner;
pub mod browser_executor;
pub mod evaluator;
pub mod executor;
pub mod recorder;
pub mod tasks;

pub use bench_runner::{
    BrowserBenchConfig, BrowserBenchReport, BrowserBenchRunner, ExecutionBackend,
};
pub use evaluator::WebTaskEvaluator;
pub use executor::WebTaskExecutor;
pub use recorder::{InteractionRecorder, InteractionTrace, RecordedAction, ScreenshotRef};
pub use tasks::{SuccessCriterion, WebAction, WebTask};
