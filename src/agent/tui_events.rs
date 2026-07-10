#[cfg(feature = "tui")]
use crate::ui::tui::TuiEvent;

/// Lightweight event type that is always available (not feature-gated).
///
/// This allows agent code to emit events unconditionally without
/// `#[cfg(feature = "tui")]` at every call site. When the TUI feature
/// is enabled, these are translated to `TuiEvent` and sent to the UI.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    Started,
    Completed {
        message: String,
    },
    Error {
        message: String,
    },
    Status {
        message: String,
    },
    TokenUsage {
        prompt_tokens: u64,
        completion_tokens: u64,
    },
    ToolStarted {
        name: String,
    },
    ToolCompleted {
        name: String,
        success: bool,
        duration_ms: u64,
    },
    /// Streaming content chunk from the assistant
    AssistantDelta {
        text: String,
    },
    /// Streaming reasoning/thinking chunk
    ThinkingDelta {
        text: String,
    },
    /// Reasoning phase finished
    ThinkingEnd,
    /// Tool execution progress update
    ToolProgress {
        name: String,
        status: String,
    },
    /// Loading spinner started
    SpinnerStart {
        message: String,
    },
    /// Loading spinner message changed
    SpinnerUpdate {
        message: String,
    },
    /// Loading spinner finished
    SpinnerStop,
    /// User queued a message during generation
    InputQueued {
        message: String,
        position: usize,
    },
    /// Permission requested for tool execution
    PermissionRequested {
        tool_name: String,
        reason: String,
    },
    /// Mode change requested (e.g., user selected "Yolo" from permission prompt)
    ModeChangeRequested {
        mode: crate::config::ExecutionMode,
    },
}

/// Trait for emitting real-time events during agent execution.
///
/// This decouples the core agent logic from TUI-specific implementations.
pub trait EventEmitter: Send + Sync {
    fn emit(&self, event: AgentEvent);
}

/// A no-op event emitter that does nothing.
pub struct NoopEmitter;

impl EventEmitter for NoopEmitter {
    fn emit(&self, _event: AgentEvent) {}
}

/// A broadcast event emitter that fans out `AgentEvent`s to multiple
/// subscribers via a `tokio::sync::broadcast` channel.
///
/// Unlike [`TuiEmitter`] (single-consumer `mpsc`), this supports N concurrent
/// subscribers — each call to [`tokio::sync::broadcast::Sender::subscribe`]
/// on the underlying sender produces an independent receiver.
pub struct BroadcastEmitter {
    tx: tokio::sync::broadcast::Sender<AgentEvent>,
}

impl BroadcastEmitter {
    /// Wrap an existing broadcast sender.
    pub fn new(tx: tokio::sync::broadcast::Sender<AgentEvent>) -> Self {
        Self { tx }
    }
}

impl EventEmitter for BroadcastEmitter {
    fn emit(&self, event: AgentEvent) {
        // `send` errors only when there are zero active receivers; ignore.
        let _ = self.tx.send(event);
    }
}

/// An event emitter that sends events via an mpsc channel to the TUI.
#[cfg(feature = "tui")]
pub struct TuiEmitter {
    tx: std::sync::mpsc::Sender<TuiEvent>,
}

#[cfg(feature = "tui")]
impl TuiEmitter {
    pub fn new(tx: std::sync::mpsc::Sender<TuiEvent>) -> Self {
        Self { tx }
    }
}

#[cfg(feature = "tui")]
impl EventEmitter for TuiEmitter {
    fn emit(&self, event: AgentEvent) {
        let tui_event = match event {
            AgentEvent::Started => TuiEvent::AgentStarted,
            AgentEvent::Completed { message } => TuiEvent::AgentCompleted { message },
            AgentEvent::Error { message } => TuiEvent::AgentError { message },
            AgentEvent::Status { message } => TuiEvent::StatusUpdate { message },
            AgentEvent::TokenUsage {
                prompt_tokens,
                completion_tokens,
            } => TuiEvent::TokenUsage {
                prompt_tokens,
                completion_tokens,
            },
            AgentEvent::ToolStarted { name } => TuiEvent::ToolStarted { name },
            AgentEvent::ToolCompleted {
                name,
                success,
                duration_ms,
            } => TuiEvent::ToolCompleted {
                name,
                success,
                duration_ms,
            },
            AgentEvent::AssistantDelta { text } => TuiEvent::AssistantDelta { text },
            AgentEvent::ThinkingDelta { text } => TuiEvent::ThinkingDelta { text },
            AgentEvent::ThinkingEnd => TuiEvent::ThinkingEnd,
            AgentEvent::ToolProgress { name, status } => TuiEvent::ToolProgress { name, status },
            AgentEvent::SpinnerStart { message } => TuiEvent::SpinnerStart { message },
            AgentEvent::SpinnerUpdate { message } => TuiEvent::SpinnerUpdate { message },
            AgentEvent::SpinnerStop => TuiEvent::SpinnerStop,
            AgentEvent::InputQueued { message, position } => {
                TuiEvent::InputQueued { message, position }
            }
            AgentEvent::PermissionRequested { tool_name, reason } => {
                TuiEvent::PermissionRequested { tool_name, reason }
            }
            AgentEvent::ModeChangeRequested { mode } => TuiEvent::ModeChangeRequested { mode },
        };
        let _ = self.tx.send(tui_event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn broadcast_emitter_fans_out() {
        let (tx, _) = tokio::sync::broadcast::channel::<AgentEvent>(16);
        let emitter = BroadcastEmitter::new(tx.clone());

        let mut rx1 = tx.subscribe();
        let mut rx2 = tx.subscribe();

        emitter.emit(AgentEvent::Started);

        let r1 = tokio::time::timeout(std::time::Duration::from_secs(2), rx1.recv())
            .await
            .unwrap()
            .unwrap();
        let r2 = tokio::time::timeout(std::time::Duration::from_secs(2), rx2.recv())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(r1, AgentEvent::Started);
        assert_eq!(r2, AgentEvent::Started);
    }

    #[tokio::test]
    async fn broadcast_emitter_no_receivers_is_ok() {
        // Emitting with zero receivers should not panic — the Result is
        // silently dropped.
        let (tx, _) = tokio::sync::broadcast::channel::<AgentEvent>(16);
        let emitter = BroadcastEmitter::new(tx);
        emitter.emit(AgentEvent::Started);
    }
}
