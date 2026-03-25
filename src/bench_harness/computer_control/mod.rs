//! Computer Control Benchmarks
//!
//! Web navigation and interaction benchmarks that test agent capability
//! using browser automation. Captures interaction traces with timestamps,
//! screenshots, and action outcomes for downstream memory consolidation.

pub mod evaluator;
pub mod recorder;
pub mod tasks;

pub use evaluator::WebTaskEvaluator;
pub use recorder::{InteractionRecorder, InteractionTrace, RecordedAction, ScreenshotRef};
pub use tasks::{SuccessCriterion, WebAction, WebTask};
