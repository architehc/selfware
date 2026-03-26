//! Interaction recorder — captures timestamped browser interaction traces
//! for downstream memory consolidation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::tasks::WebAction;

/// A reference to a captured screenshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotRef {
    /// Path to the screenshot file.
    pub path: PathBuf,
    /// When the screenshot was taken.
    pub timestamp: DateTime<Utc>,
    /// Human-readable label.
    pub label: String,
    /// Image dimensions (width, height).
    pub dimensions: (u32, u32),
}

/// The outcome of a single recorded action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionOutcome {
    /// Action completed successfully.
    Success { output: String },
    /// Action failed with an error.
    Failed { error: String },
    /// Action timed out.
    Timeout,
}

/// A single recorded browser action with timing and outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedAction {
    /// When the action started.
    pub timestamp: DateTime<Utc>,
    /// The action that was performed.
    pub action: WebAction,
    /// Outcome of the action.
    pub outcome: ActionOutcome,
    /// Screenshot taken before the action (if any).
    pub screenshot_before: Option<PathBuf>,
    /// Screenshot taken after the action (if any).
    pub screenshot_after: Option<PathBuf>,
    /// How long the action took in milliseconds.
    pub duration_ms: u64,
}

/// The final outcome of a web task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskOutcome {
    /// All success criteria met.
    Passed,
    /// Some criteria failed.
    Failed { reasons: Vec<String> },
    /// Task timed out.
    Timeout,
    /// Task encountered an error.
    Error { message: String },
}

/// A complete interaction trace for one web task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionTrace {
    /// Task identifier.
    pub task_id: String,
    /// Task name.
    pub task_name: String,
    /// When the task started.
    pub started_at: DateTime<Utc>,
    /// When the task ended.
    pub ended_at: DateTime<Utc>,
    /// Ordered sequence of recorded actions.
    pub actions: Vec<RecordedAction>,
    /// All screenshots taken during the task.
    pub screenshots: Vec<ScreenshotRef>,
    /// Final task outcome.
    pub final_outcome: TaskOutcome,
    /// Total duration in milliseconds.
    pub total_duration_ms: u64,
}

/// Records browser interactions for a single task execution.
pub struct InteractionRecorder {
    task_id: String,
    task_name: String,
    started_at: DateTime<Utc>,
    actions: Vec<RecordedAction>,
    screenshots: Vec<ScreenshotRef>,
    screenshot_dir: PathBuf,
}

impl InteractionRecorder {
    /// Create a new recorder for a task.
    pub fn new(
        task_id: impl Into<String>,
        task_name: impl Into<String>,
        screenshot_dir: PathBuf,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            task_name: task_name.into(),
            started_at: Utc::now(),
            actions: Vec::new(),
            screenshots: Vec::new(),
            screenshot_dir,
        }
    }

    /// Record a completed action.
    pub fn record_action(
        &mut self,
        action: WebAction,
        outcome: ActionOutcome,
        duration_ms: u64,
        screenshot_before: Option<PathBuf>,
        screenshot_after: Option<PathBuf>,
    ) {
        self.actions.push(RecordedAction {
            timestamp: Utc::now(),
            action,
            outcome,
            screenshot_before,
            screenshot_after,
            duration_ms,
        });
    }

    /// Record a screenshot reference.
    pub fn record_screenshot(
        &mut self,
        label: impl Into<String>,
        path: PathBuf,
        dimensions: (u32, u32),
    ) {
        self.screenshots.push(ScreenshotRef {
            path,
            timestamp: Utc::now(),
            label: label.into(),
            dimensions,
        });
    }

    /// Get the screenshot directory for this recorder.
    pub fn screenshot_dir(&self) -> &PathBuf {
        &self.screenshot_dir
    }

    /// Finalize the recording and produce an interaction trace.
    pub fn finish(self, outcome: TaskOutcome) -> InteractionTrace {
        let ended_at = Utc::now();
        let total_duration_ms = (ended_at - self.started_at).num_milliseconds().max(0) as u64;

        InteractionTrace {
            task_id: self.task_id,
            task_name: self.task_name,
            started_at: self.started_at,
            ended_at,
            actions: self.actions,
            screenshots: self.screenshots,
            final_outcome: outcome,
            total_duration_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_recorder_basic_flow() {
        let mut recorder =
            InteractionRecorder::new("test-1", "Test Task", PathBuf::from("/tmp/screenshots"));

        recorder.record_action(
            WebAction::Navigate {
                url: "https://example.com".into(),
            },
            ActionOutcome::Success {
                output: "200 OK".into(),
            },
            150,
            None,
            None,
        );

        recorder.record_screenshot(
            "after_nav",
            PathBuf::from("/tmp/screenshots/s1.png"),
            (1920, 1080),
        );

        let trace = recorder.finish(TaskOutcome::Passed);

        assert_eq!(trace.task_id, "test-1");
        assert_eq!(trace.actions.len(), 1);
        assert_eq!(trace.screenshots.len(), 1);
        assert!(matches!(trace.final_outcome, TaskOutcome::Passed));
    }

    #[test]
    fn test_trace_serde_roundtrip() {
        let mut recorder =
            InteractionRecorder::new("test-2", "Serde Test", PathBuf::from("/tmp/ss"));

        recorder.record_action(
            WebAction::Click {
                selector: "#btn".into(),
            },
            ActionOutcome::Failed {
                error: "not found".into(),
            },
            50,
            None,
            None,
        );

        let trace = recorder.finish(TaskOutcome::Failed {
            reasons: vec!["element not found".into()],
        });

        let json = serde_json::to_string(&trace).unwrap();
        let parsed: InteractionTrace = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.task_id, "test-2");
        assert_eq!(parsed.actions.len(), 1);
    }
}
