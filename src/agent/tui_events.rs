#[cfg(feature = "tui")]
use crate::ui::tui::TuiEvent;

/// Lightweight event type that is always available (not feature-gated).
///
/// This allows agent code to emit events unconditionally without
/// `#[cfg(feature = "tui")]` at every call site. When the TUI feature
/// is enabled, these are translated to `TuiEvent` and sent to the UI.
#[derive(Debug, Clone)]
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
            AgentEvent::ToolProgress { name, status } => {
                TuiEvent::ToolProgress { name, status }
            }
            AgentEvent::SpinnerStart { message } => TuiEvent::SpinnerStart { message },
            AgentEvent::SpinnerUpdate { message } => TuiEvent::SpinnerUpdate { message },
            AgentEvent::SpinnerStop => TuiEvent::SpinnerStop,
            AgentEvent::InputQueued { message, position } => {
                TuiEvent::InputQueued { message, position }
            }
        };
        let _ = self.tx.send(tui_event);
    }
}
