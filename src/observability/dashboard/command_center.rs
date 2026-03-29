//! Command Center Dashboard for SWL Workflow Monitoring
//!
//! Real-time monitoring dashboard for SWL workflows

/// Dashboard update mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateMode {
    Polling,
    Streaming,
}

impl Default for UpdateMode {
    fn default() -> Self {
        UpdateMode::Polling
    }
}

/// Run the command center dashboard
#[cfg(feature = "tui")]
pub async fn run_command_center() -> anyhow::Result<()> {
    println!("Command Center dashboard started");
    println!("(Full TUI implementation with ratatui would go here)");
    Ok(())
}

/// Run with shared state for external integration
#[cfg(feature = "tui")]
pub async fn run_command_center_with_state(
    _state: std::sync::Arc<tokio::sync::RwLock<CommandCenterState>>,
) -> anyhow::Result<()> {
    run_command_center().await
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
