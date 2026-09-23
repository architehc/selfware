//! Miscellaneous configuration types: execution mode, UI, continuous work,
//! retry settings, YOLO mode, and evolution daemon config.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Execution mode for tool approval
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    /// Ask for confirmation before executing tools (default)
    #[default]
    Normal,
    /// Auto-approve file edits, ask for other operations
    AutoEdit,
    /// Auto-approve all operations for this session
    Yolo,
    /// Run forever in autonomous loop
    Daemon,
}

impl std::fmt::Display for ExecutionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionMode::Normal => write!(f, "normal"),
            ExecutionMode::AutoEdit => write!(f, "auto-edit"),
            ExecutionMode::Yolo => write!(f, "yolo"),
            ExecutionMode::Daemon => write!(f, "daemon"),
        }
    }
}

/// UI configuration for themes, animations, and output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// Color theme: "amber", "ocean", "minimal", "high-contrast"
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Enable animations (spinners, progress bars)
    #[serde(default = "default_true")]
    pub animations: bool,
    /// Default to compact mode
    #[serde(default)]
    pub compact_mode: bool,
    /// Default to verbose mode
    #[serde(default)]
    pub verbose_mode: bool,
    /// Allow the agent to use the `ask_user` clarification tool.
    /// When `false`, the tool returns a null answer immediately so the
    /// agent proceeds with its best assumption. Defaults to `true`.
    #[serde(default = "default_true")]
    pub allow_clarification: bool,
    /// Always show token usage
    #[serde(default)]
    pub show_tokens: bool,
    /// Animation speed multiplier (1.0 = normal, 2.0 = faster)
    #[serde(default = "default_animation_speed")]
    pub animation_speed: f64,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            animations: true,
            compact_mode: false,
            verbose_mode: false,
            allow_clarification: true,
            show_tokens: false,
            animation_speed: 1.0,
        }
    }
}

/// Continuous work configuration for long-running sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuousWorkConfig {
    /// Enable periodic checkpointing policy.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Save checkpoint after this many tool calls.
    #[serde(default = "default_checkpoint_interval_tools")]
    pub checkpoint_interval_tools: usize,
    /// Save checkpoint after this many seconds.
    #[serde(default = "default_checkpoint_interval_secs")]
    pub checkpoint_interval_secs: u64,
    /// Enable automatic recovery attempts when available.
    #[serde(default = "default_true")]
    pub auto_recovery: bool,
    /// Maximum recovery attempts per failure.
    #[serde(default = "default_max_recovery_attempts")]
    pub max_recovery_attempts: u32,
}

impl Default for ContinuousWorkConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            checkpoint_interval_tools: default_checkpoint_interval_tools(),
            checkpoint_interval_secs: default_checkpoint_interval_secs(),
            auto_recovery: true,
            max_recovery_attempts: default_max_recovery_attempts(),
        }
    }
}

/// Retry configuration for API/network operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrySettings {
    /// Maximum retries before failing.
    #[serde(default = "default_retry_max_retries")]
    pub max_retries: u32,
    /// Initial delay before first retry.
    #[serde(default = "default_retry_base_delay_ms")]
    pub base_delay_ms: u64,
    /// Upper bound for retry delay.
    #[serde(default = "default_retry_max_delay_ms")]
    pub max_delay_ms: u64,
}

impl Default for RetrySettings {
    fn default() -> Self {
        Self {
            max_retries: default_retry_max_retries(),
            base_delay_ms: default_retry_base_delay_ms(),
            max_delay_ms: default_retry_max_delay_ms(),
        }
    }
}

/// YOLO mode configuration (loaded from config file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoloFileConfig {
    /// Whether YOLO mode is enabled
    #[serde(default)]
    pub enabled: bool,
    /// Maximum operations before requiring check-in (0 = unlimited)
    #[serde(default)]
    pub max_operations: usize,
    /// Maximum time in hours before requiring check-in (0 = unlimited)
    #[serde(default)]
    pub max_hours: f64,
    /// Whether to allow git push operations
    #[serde(default = "default_true")]
    pub allow_git_push: bool,
    /// Whether to allow destructive shell commands (rm -rf, etc.)
    #[serde(default)]
    pub allow_destructive_shell: bool,
    /// Audit log file path
    #[serde(default)]
    pub audit_log_path: Option<PathBuf>,
    /// Send periodic status updates (every N operations)
    #[serde(default = "default_status_interval")]
    pub status_interval: usize,
}

impl Default for YoloFileConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_operations: 0,
            max_hours: 0.0,
            allow_git_push: true,
            allow_destructive_shell: false,
            audit_log_path: None,
            status_interval: 100,
        }
    }
}

/// Concurrency governor configuration (loaded from `[concurrency]` in selfware.toml)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConcurrencyConfig {
    /// Maximum concurrent LLM streaming responses.
    #[serde(default = "default_max_streams")]
    pub max_streams: usize,
    /// Maximum concurrent tool executions per agent.
    #[serde(default = "default_max_tools")]
    pub max_tools: usize,
    /// Global limit on total inflight operations.
    #[serde(default = "default_max_global")]
    pub max_global: usize,
}

impl ConcurrencyConfig {
    /// Validate concurrency limits: each must be >= 1 and <= 256.
    pub fn validate(&self) -> anyhow::Result<()> {
        for (name, value) in [
            ("max_streams", self.max_streams),
            ("max_tools", self.max_tools),
            ("max_global", self.max_global),
        ] {
            if value < 1 {
                anyhow::bail!(
                    "Config error: concurrency.{} must be >= 1, got {}",
                    name,
                    value
                );
            }
            if value > 256 {
                anyhow::bail!(
                    "Config error: concurrency.{} must be <= 256, got {}",
                    name,
                    value
                );
            }
        }
        Ok(())
    }
}

impl Default for ConcurrencyConfig {
    fn default() -> Self {
        Self {
            max_streams: default_max_streams(),
            max_tools: default_max_tools(),
            max_global: default_max_global(),
        }
    }
}

/// Evolution daemon configuration (loaded from `[evolution]` in selfware.toml)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvolutionTomlConfig {
    /// Model profile name to use for hypothesis generation (e.g. "architect").
    /// If set, looks up `[models.<name>]` for endpoint/model. Falls back to default.
    #[serde(default)]
    pub hypothesis_model: Option<String>,
    /// Source files containing prompt construction logic
    #[serde(default)]
    pub prompt_logic: Vec<String>,
    /// Source files containing tool implementations
    #[serde(default)]
    pub tool_code: Vec<String>,
    /// Source files containing cognitive architecture
    #[serde(default)]
    pub cognitive: Vec<String>,
    /// Config keys the agent can modify
    #[serde(default)]
    pub config_keys: Vec<String>,
}

// --- Default value functions ---

pub(crate) fn default_true() -> bool {
    true
}
pub(crate) fn default_status_interval() -> usize {
    100
}
pub(crate) fn default_theme() -> String {
    "amber".to_string()
}
pub(crate) fn default_animation_speed() -> f64 {
    1.0
}
pub(crate) fn default_checkpoint_interval_tools() -> usize {
    10
}
pub(crate) fn default_checkpoint_interval_secs() -> u64 {
    300
}
pub(crate) fn default_max_recovery_attempts() -> u32 {
    3
}
pub(crate) fn default_retry_max_retries() -> u32 {
    5
}
pub(crate) fn default_retry_base_delay_ms() -> u64 {
    1000
}
pub(crate) fn default_retry_max_delay_ms() -> u64 {
    60000
}
pub(crate) fn default_max_streams() -> usize {
    4
}
pub(crate) fn default_max_tools() -> usize {
    8
}
pub(crate) fn default_max_global() -> usize {
    12
}

#[cfg(test)]
#[path = "../../tests/unit/config/types/types_test.rs"]
mod tests;

/// Desktop-automation configuration (loaded from `[computer]` in selfware.toml).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComputerConfig {
    /// Optional window-placement policy (`[computer.window_policy]`). When
    /// absent, window operations are unrestricted: no ownership gate, no
    /// bounds check, and coordinates are sent to the window manager as-is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_policy: Option<WindowPolicy>,
}

/// A visible region in device coordinates. A window is inside when
/// `x >= x_min`, `x + width <= x_max`, `y >= y_min` and
/// `y + height <= y_max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisibleRegion {
    pub x_min: i32,
    pub x_max: i32,
    pub y_min: i32,
    pub y_max: i32,
}

fn default_request_scale() -> u32 {
    1
}

/// Window-placement policy for `computer::WindowManager`
/// (`[computer.window_policy]`). Every rule is optional; only the rules
/// that are configured are enforced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowPolicy {
    /// When set, only windows whose (trimmed) title starts with this prefix
    /// may be moved, resized, minimized or closed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owned_title_prefix: Option<String>,
    /// When set, moves and resizes must leave the window inside this region.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_region: Option<VisibleRegion>,
    /// Divisor applied to `wmctrl -e` POSITION requests (sizes pass through).
    /// Some compositors (mutter at 200% scaling) double position requests;
    /// set this to 2 there. Default 1 (no scaling); 0 is treated as 1.
    #[serde(default = "default_request_scale")]
    pub request_scale: u32,
    /// Re-read the window geometry with `wmctrl -lG` after a move or resize,
    /// correct once if it landed outside `visible_region`, and fail loudly if
    /// it still does or cannot be verified.
    #[serde(default)]
    pub verify_after_move: bool,
}

impl Default for WindowPolicy {
    fn default() -> Self {
        Self {
            owned_title_prefix: None,
            visible_region: None,
            request_scale: default_request_scale(),
            verify_after_move: false,
        }
    }
}
