//! Agent behavior configuration.

use serde::{Deserialize, Serialize};

use super::types::default_true;

/// Policy for mutation-required tasks that keep issuing read-only tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ReadLoopPolicy {
    /// Preserve the historical behavior: block read-only tools and nudge.
    Nudge,
    /// Require the next action to mutate state, then abort if the model keeps
    /// trying read-only actions. This is the default for SWE-style tasks.
    #[default]
    ForceMutation,
}

/// Agent behavior settings: iteration limits, timeouts, token budgets, and calling mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
    #[serde(default = "default_step_timeout")]
    pub step_timeout_secs: u64,
    #[serde(default = "default_token_budget")]
    pub token_budget: usize,
    /// Safety margin subtracted from token_budget to prevent exceeding model context limit.
    /// Accounts for tool definitions, system prompt overhead, and output tokens.
    #[serde(default = "default_token_safety_margin")]
    pub token_safety_margin: usize,
    /// Enable native function calling (requires backend support like sglang --tool-call-parser)
    /// When true, tools are passed via API and tool_calls are returned in response
    /// When false (default), tools are embedded in system prompt and parsed from content
    #[serde(default)]
    pub native_function_calling: bool,
    /// Enable streaming responses for real-time output
    /// When true, LLM responses are displayed as they arrive
    #[serde(default = "default_true")]
    pub streaming: bool,
    /// Minimum number of execution steps before accepting task completion.
    /// Prevents early self-termination by requiring the agent to do meaningful work.
    #[serde(default = "default_min_completion_steps")]
    pub min_completion_steps: usize,
    /// Require at least one successful verification (cargo_check/cargo_test/cargo_clippy)
    /// before accepting task completion.
    ///
    /// This gate is project-type-aware: it is automatically skipped when the working
    /// directory has no `Cargo.toml` or when only non-Rust tools (browser, vision,
    /// computer control, web fetch, etc.) were used during the task.
    #[serde(default = "default_true")]
    pub require_verification_before_completion: bool,
    /// Behavior when a mutation-required task loops on read-only tools.
    #[serde(default)]
    pub read_loop_policy: ReadLoopPolicy,

    /// When true, visual verification failures with confidence > 0.6 act as hard
    /// gates — the tool result is marked as needing retry and the assertion is
    /// logged to the checkpoint.  When false (default), failures are advisory only.
    #[serde(default)]
    pub require_visual_verification: bool,
    /// Fraction of token_budget reserved for content (files, conversation, tool results).
    /// Compression triggers when content exceeds this fraction.
    #[serde(default = "default_context_content_ratio")]
    pub context_content_ratio: f32,
    /// Fraction of token_budget reserved as compression headroom.
    /// Ensures compression always has room to work.
    #[serde(default = "default_context_compression_ratio")]
    pub context_compression_ratio: f32,
    /// Fraction of token_budget reserved for model thinking/reasoning blocks.
    #[serde(default = "default_context_thinking_ratio")]
    pub context_thinking_ratio: f32,
    /// Compression detail level: "names", "signatures", or "full".
    /// Controls how much information is preserved when downgrading context levels.
    /// - "names": only module/function/struct names (~90% reduction)
    /// - "signatures": full function signatures and struct field types (~70% reduction)
    /// - "full": current behavior, summarize everything via LLM (~50% reduction)
    #[serde(default = "default_compression_detail")]
    pub compression_detail: String,
    /// Disable per-turn debug artifact capture under `<workdir>/.selfware/turns/`.
    ///
    /// Capture is off by default because these files contain model responses,
    /// parsed tool calls, and agent decisions. Set this to `false` explicitly
    /// when per-turn diagnostic artifacts are needed.
    #[serde(default = "default_true")]
    pub disable_turn_artifacts: bool,

    /// Prompt profile used for benchmark / evaluation runs.
    #[serde(default = "default_prompt_profile")]
    pub prompt_profile: String,

    /// Optional command to run automatically after every file_edit/file_write.
    /// Used by SWE-bench Pro to run the official fail_to_pass tests.
    #[serde(default)]
    pub post_edit_test_command: Option<String>,

    /// Hard limit: stop when total prompt+completion tokens exceed this.
    #[serde(default)]
    pub max_budget_tokens: Option<usize>,

    /// Hard limit: stop after this many wall-clock seconds.
    #[serde(default)]
    pub max_wall_secs: Option<u64>,

    /// Hard limit: stop when accumulated provider-reported USD cost exceeds this.
    #[serde(default)]
    pub max_cost_usd: Option<f64>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: default_max_iterations(),
            step_timeout_secs: default_step_timeout(),
            token_budget: super::default_max_tokens(), // matches max_tokens; overridden by Config::load() when user sets max_tokens
            token_safety_margin: default_token_safety_margin(),
            native_function_calling: false,
            streaming: true,
            min_completion_steps: default_min_completion_steps(),
            require_verification_before_completion: true,
            read_loop_policy: ReadLoopPolicy::default(),
            require_visual_verification: false,
            context_content_ratio: default_context_content_ratio(),
            context_compression_ratio: default_context_compression_ratio(),
            context_thinking_ratio: default_context_thinking_ratio(),
            compression_detail: default_compression_detail(),
            disable_turn_artifacts: true,
            prompt_profile: default_prompt_profile(),
            post_edit_test_command: None,
            max_budget_tokens: None,
            max_wall_secs: None,
            max_cost_usd: None,
        }
    }
}

pub fn default_max_iterations() -> usize {
    100
}
pub fn default_step_timeout() -> u64 {
    300
}
pub fn default_min_completion_steps() -> usize {
    3
}
pub fn default_token_budget() -> usize {
    0 // sentinel: 0 means "derive from max_tokens at load time"
}
pub fn default_token_safety_margin() -> usize {
    8_192 // 8K token safety margin for tool definitions + output + overhead
}
pub fn default_context_content_ratio() -> f32 {
    0.75
}
pub fn default_context_compression_ratio() -> f32 {
    0.20
}
pub fn default_context_thinking_ratio() -> f32 {
    0.05
}
pub fn default_compression_detail() -> String {
    "signatures".to_string()
}
pub fn default_prompt_profile() -> String {
    "default".to_string()
}

#[cfg(test)]
#[path = "../../tests/unit/config/agent/agent_test.rs"]
mod tests;
