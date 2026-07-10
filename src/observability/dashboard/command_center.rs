//! Command Center Dashboard for SWL Workflow Monitoring
//!
//! The live agent-driven TUI is now unified in [`crate::cli::run_live_agent_tui`].
//! This module retains the shared state types that may be referenced by other
//! components, but the dead placeholder runner has been removed.

/// Dashboard update mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UpdateMode {
    #[default]
    Polling,
    Streaming,
}

/// Dashboard state
#[derive(Debug)]
pub struct CommandCenterState {
    pub session_start: std::time::Instant,
}

impl Default for CommandCenterState {
    fn default() -> Self {
        Self {
            session_start: std::time::Instant::now(),
        }
    }
}
