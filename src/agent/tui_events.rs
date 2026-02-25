#[cfg(feature = "tui")]
use crate::ui::tui::TuiEvent;

/// Trait for emitting real-time events during agent execution.
///
/// This decouples the core agent logic from TUI-specific implementations.
pub trait EventEmitter: Send + Sync {
    #[cfg(feature = "tui")]
    fn emit(&self, event: TuiEvent);
}

/// A no-op event emitter that does nothing.
pub struct NoopEmitter;

impl EventEmitter for NoopEmitter {
    #[cfg(feature = "tui")]
    fn emit(&self, _event: TuiEvent) {}
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
    fn emit(&self, event: TuiEvent) {
        let _ = self.tx.send(event);
    }
}
