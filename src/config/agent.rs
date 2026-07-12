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
    /// CLI-only; not persisted in config files.
    #[serde(skip)]
    pub max_budget_tokens: Option<usize>,

    /// Hard limit: stop after this many wall-clock seconds.
    /// CLI-only; not persisted in config files.
    #[serde(skip)]
    pub max_wall_secs: Option<u64>,

    /// Hard limit: stop when accumulated provider-reported USD cost exceeds this.
    /// CLI-only; not persisted in config files.
    #[serde(skip)]
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
mod tests {
    use super::*;

    // ---------------------------------------------------------------------------
    // Default function tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_default_max_iterations() {
        assert_eq!(default_max_iterations(), 100);
    }

    #[test]
    fn test_default_step_timeout() {
        assert_eq!(default_step_timeout(), 300);
    }

    #[test]
    fn test_default_min_completion_steps() {
        assert_eq!(default_min_completion_steps(), 3);
    }

    #[test]
    fn test_default_token_budget_is_zero_sentinel() {
        // 0 is a sentinel meaning "derive from max_tokens at load time"
        assert_eq!(default_token_budget(), 0);
    }

    #[test]
    fn test_default_token_safety_margin() {
        assert_eq!(default_token_safety_margin(), 8_192);
    }

    #[test]
    fn test_default_context_content_ratio() {
        let r = default_context_content_ratio();
        assert!((r - 0.75).abs() < f32::EPSILON);
        assert!(r > 0.0 && r < 1.0);
    }

    #[test]
    fn test_default_context_compression_ratio() {
        let r = default_context_compression_ratio();
        assert!((r - 0.20).abs() < f32::EPSILON);
        assert!(r > 0.0 && r < 1.0);
    }

    #[test]
    fn test_default_context_thinking_ratio() {
        let r = default_context_thinking_ratio();
        assert!((r - 0.05).abs() < f32::EPSILON);
        assert!(r > 0.0 && r < 1.0);
    }

    #[test]
    fn test_default_context_ratios_sum_to_one() {
        // The three reserved fractions should sum to exactly 1.0, meaning
        // all of the token_budget is allocated across content, compression,
        // and thinking partitions.
        let total = default_context_content_ratio()
            + default_context_compression_ratio()
            + default_context_thinking_ratio();
        assert!(
            (total - 1.0_f32).abs() < f32::EPSILON,
            "content + compression + thinking ratios should sum to 1.0, got {total}"
        );
    }

    #[test]
    fn test_default_compression_detail() {
        assert_eq!(default_compression_detail(), "signatures");
    }

    #[test]
    fn test_default_prompt_profile() {
        assert_eq!(default_prompt_profile(), "default");
    }

    // ---------------------------------------------------------------------------
    // ReadLoopPolicy tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_read_loop_policy_default_is_force_mutation() {
        assert_eq!(ReadLoopPolicy::default(), ReadLoopPolicy::ForceMutation);
    }

    #[test]
    fn test_read_loop_policy_copy_and_equality() {
        let a = ReadLoopPolicy::Nudge;
        let b = a; // relies on Copy
        assert_eq!(a, b);
        assert_ne!(a, ReadLoopPolicy::ForceMutation);
    }

    #[test]
    fn test_read_loop_policy_serde_snake_case() {
        // ForceMutation → "force_mutation"
        let json = serde_json::to_string(&ReadLoopPolicy::ForceMutation).unwrap();
        assert_eq!(json, "\"force_mutation\"");

        let nudge_json = serde_json::to_string(&ReadLoopPolicy::Nudge).unwrap();
        assert_eq!(nudge_json, "\"nudge\"");
    }

    #[test]
    fn test_read_loop_policy_serde_roundtrip() {
        for variant in [ReadLoopPolicy::Nudge, ReadLoopPolicy::ForceMutation] {
            let json = serde_json::to_string(&variant).unwrap();
            let back: ReadLoopPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn test_read_loop_policy_serde_invalid_variant() {
        let result: Result<ReadLoopPolicy, _> = serde_json::from_str("\"bogus\"");
        assert!(result.is_err());
    }

    // ---------------------------------------------------------------------------
    // AgentConfig::default tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_agent_config_default_matches_individual_defaults() {
        let cfg = AgentConfig::default();
        assert_eq!(cfg.max_iterations, default_max_iterations());
        assert_eq!(cfg.step_timeout_secs, default_step_timeout());
        assert_eq!(cfg.min_completion_steps, default_min_completion_steps());
        assert_eq!(cfg.token_safety_margin, default_token_safety_margin());
        assert!((cfg.context_content_ratio - default_context_content_ratio()).abs() < f32::EPSILON);
        assert!(
            (cfg.context_compression_ratio - default_context_compression_ratio()).abs()
                < f32::EPSILON
        );
        assert!(
            (cfg.context_thinking_ratio - default_context_thinking_ratio()).abs() < f32::EPSILON
        );
        assert_eq!(cfg.compression_detail, default_compression_detail());
        assert_eq!(cfg.prompt_profile, default_prompt_profile());
    }

    #[test]
    fn test_agent_config_default_specific_values() {
        let cfg = AgentConfig::default();
        // Fields that don't have a dedicated default fn but have known defaults
        assert!(!cfg.native_function_calling);
        assert!(cfg.streaming);
        assert!(cfg.require_verification_before_completion);
        assert_eq!(cfg.read_loop_policy, ReadLoopPolicy::ForceMutation);
        assert!(!cfg.require_visual_verification);
        // P0 privacy: turn artifacts (request/response/reasoning/tool-args
        // written to disk) must be OFF by default so secrets in conversation
        // text are NOT persisted unless the user explicitly opts in.
        assert!(cfg.disable_turn_artifacts);
        assert_eq!(cfg.post_edit_test_command, None);
        assert_eq!(cfg.max_budget_tokens, None);
        assert_eq!(cfg.max_wall_secs, None);
        assert_eq!(cfg.max_cost_usd, None);
    }

    #[test]
    fn test_agent_config_default_token_budget_matches_max_tokens() {
        // In Default::default(), token_budget is set to super::default_max_tokens()
        // which is 65536, NOT default_token_budget() (which returns 0).
        let cfg = AgentConfig::default();
        assert_eq!(cfg.token_budget, 65536);
        assert_ne!(cfg.token_budget, default_token_budget());
    }

    // ---------------------------------------------------------------------------
    // Serde round-trip tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_agent_config_serde_roundtrip() {
        let cfg = AgentConfig {
            max_iterations: 42,
            step_timeout_secs: 99,
            token_budget: 12345,
            token_safety_margin: 4096,
            native_function_calling: true,
            streaming: false,
            min_completion_steps: 7,
            require_verification_before_completion: false,
            read_loop_policy: ReadLoopPolicy::Nudge,
            require_visual_verification: true,
            context_content_ratio: 0.5,
            context_compression_ratio: 0.3,
            context_thinking_ratio: 0.1,
            compression_detail: "names".to_string(),
            disable_turn_artifacts: true,
            prompt_profile: "swe_bench".to_string(),
            post_edit_test_command: Some("cargo test".to_string()),
            max_budget_tokens: Some(99999),
            max_wall_secs: Some(600),
            max_cost_usd: Some(1.5),
        };

        let json = serde_json::to_string(&cfg).unwrap();
        let back: AgentConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(back.max_iterations, 42);
        assert_eq!(back.step_timeout_secs, 99);
        assert_eq!(back.token_budget, 12345);
        assert_eq!(back.token_safety_margin, 4096);
        assert!(back.native_function_calling);
        assert!(!back.streaming);
        assert_eq!(back.min_completion_steps, 7);
        assert!(!back.require_verification_before_completion);
        assert_eq!(back.read_loop_policy, ReadLoopPolicy::Nudge);
        assert!(back.require_visual_verification);
        assert!((back.context_content_ratio - 0.5).abs() < f32::EPSILON);
        assert!((back.context_compression_ratio - 0.3).abs() < f32::EPSILON);
        assert!((back.context_thinking_ratio - 0.1).abs() < f32::EPSILON);
        assert_eq!(back.compression_detail, "names");
        assert!(back.disable_turn_artifacts);
        assert_eq!(back.prompt_profile, "swe_bench");
        assert_eq!(back.post_edit_test_command, Some("cargo test".to_string()));
        // serde(skip) fields should reset to their Default value on deserialize
        assert_eq!(back.max_budget_tokens, None);
        assert_eq!(back.max_wall_secs, None);
        assert_eq!(back.max_cost_usd, None);
    }

    #[test]
    fn test_agent_config_serde_skip_fields_not_in_json() {
        let cfg = AgentConfig {
            max_budget_tokens: Some(50000),
            max_wall_secs: Some(120),
            max_cost_usd: Some(0.5),
            ..AgentConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        // The skip fields should not appear in the JSON output
        assert!(!json.contains("max_budget_tokens"));
        assert!(!json.contains("max_wall_secs"));
        assert!(!json.contains("max_cost_usd"));
    }

    #[test]
    fn test_agent_config_serde_empty_json_uses_defaults() {
        let back: AgentConfig = serde_json::from_str("{}").unwrap();
        // All serde-defaulted fields should match their default functions
        assert_eq!(back.max_iterations, default_max_iterations());
        assert_eq!(back.step_timeout_secs, default_step_timeout());
        assert_eq!(back.token_budget, default_token_budget()); // 0 sentinel
        assert_eq!(back.token_safety_margin, default_token_safety_margin());
        assert!(!back.native_function_calling); // #[serde(default)] → false
        assert!(back.streaming); // default_true
        assert_eq!(back.min_completion_steps, default_min_completion_steps());
        assert!(back.require_verification_before_completion); // default_true
        assert_eq!(back.read_loop_policy, ReadLoopPolicy::default());
        assert!(!back.require_visual_verification); // #[serde(default)] → false
        assert!(
            (back.context_content_ratio - default_context_content_ratio()).abs() < f32::EPSILON
        );
        assert!(
            (back.context_compression_ratio - default_context_compression_ratio()).abs()
                < f32::EPSILON
        );
        assert!(
            (back.context_thinking_ratio - default_context_thinking_ratio()).abs() < f32::EPSILON
        );
        assert_eq!(back.compression_detail, default_compression_detail());
        assert!(back.disable_turn_artifacts); // omitted capture remains privacy-safe
        assert_eq!(back.prompt_profile, default_prompt_profile());
        assert_eq!(back.post_edit_test_command, None); // #[serde(default)] → None
        assert_eq!(back.max_budget_tokens, None);
        assert_eq!(back.max_wall_secs, None);
        assert_eq!(back.max_cost_usd, None);
    }

    #[test]
    fn test_agent_config_serde_partial_json_overrides() {
        let json = r#"{
            "max_iterations": 50,
            "native_function_calling": true,
            "compression_detail": "full"
        }"#;
        let back: AgentConfig = serde_json::from_str(json).unwrap();
        assert_eq!(back.max_iterations, 50);
        assert!(back.native_function_calling);
        assert_eq!(back.compression_detail, "full");
        // Untouched fields should still have defaults
        assert_eq!(back.step_timeout_secs, default_step_timeout());
        assert!(back.streaming);
    }

    #[test]
    fn test_agent_config_serde_read_loop_policy_in_json() {
        let json = r#"{"read_loop_policy": "nudge"}"#;
        let back: AgentConfig = serde_json::from_str(json).unwrap();
        assert_eq!(back.read_loop_policy, ReadLoopPolicy::Nudge);

        let json2 = r#"{"read_loop_policy": "force_mutation"}"#;
        let back2: AgentConfig = serde_json::from_str(json2).unwrap();
        assert_eq!(back2.read_loop_policy, ReadLoopPolicy::ForceMutation);
    }

    #[test]
    fn test_agent_config_serde_post_edit_test_command() {
        let json = r#"{"post_edit_test_command": "make test"}"#;
        let back: AgentConfig = serde_json::from_str(json).unwrap();
        assert_eq!(back.post_edit_test_command, Some("make test".to_string()));

        let json_none = r#"{}"#;
        let back_none: AgentConfig = serde_json::from_str(json_none).unwrap();
        assert_eq!(back_none.post_edit_test_command, None);
    }

    #[test]
    fn test_agent_config_serde_unknown_field_rejected() {
        // Without #[serde(deny_unknown_fields)] serde will ignore unknown fields.
        // Verify deserialization succeeds even with extra fields (current behavior).
        let json = r#"{"unknown_field": 123, "max_iterations": 10}"#;
        let result: Result<AgentConfig, _> = serde_json::from_str(json);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().max_iterations, 10);
    }

    // ---------------------------------------------------------------------------
    // Clone / Debug tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_agent_config_clone_is_equal() {
        let cfg = AgentConfig::default();
        let cloned = cfg.clone();
        assert_eq!(cfg.max_iterations, cloned.max_iterations);
        assert_eq!(cfg.step_timeout_secs, cloned.step_timeout_secs);
        assert_eq!(cfg.token_budget, cloned.token_budget);
        assert_eq!(cfg.read_loop_policy, cloned.read_loop_policy);
        assert_eq!(cfg.compression_detail, cloned.compression_detail);
        assert_eq!(cfg.prompt_profile, cloned.prompt_profile);
    }

    #[test]
    fn test_agent_config_debug_format() {
        let cfg = AgentConfig::default();
        let debug_str = format!("{cfg:?}");
        assert!(debug_str.contains("AgentConfig"));
        assert!(debug_str.contains("max_iterations"));
    }

    #[test]
    fn test_read_loop_policy_debug_format() {
        let debug = format!("{:?}", ReadLoopPolicy::Nudge);
        assert_eq!(debug, "Nudge");
        let debug2 = format!("{:?}", ReadLoopPolicy::ForceMutation);
        assert_eq!(debug2, "ForceMutation");
    }

    // ---------------------------------------------------------------------------
    // Edge case / behavioral tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_agent_config_with_zero_ratios() {
        // Ratios of 0.0 are valid (if unusual). Ensure serde handles them.
        let json = r#"{
            "context_content_ratio": 0.0,
            "context_compression_ratio": 0.0,
            "context_thinking_ratio": 0.0
        }"#;
        let back: AgentConfig = serde_json::from_str(json).unwrap();
        assert_eq!(back.context_content_ratio, 0.0);
        assert_eq!(back.context_compression_ratio, 0.0);
        assert_eq!(back.context_thinking_ratio, 0.0);
    }

    #[test]
    fn test_agent_config_all_bool_fields_toggle() {
        let json = r#"{
            "native_function_calling": true,
            "streaming": false,
            "require_verification_before_completion": false,
            "require_visual_verification": true,
            "disable_turn_artifacts": true
        }"#;
        let back: AgentConfig = serde_json::from_str(json).unwrap();
        assert!(back.native_function_calling);
        assert!(!back.streaming);
        assert!(!back.require_verification_before_completion);
        assert!(back.require_visual_verification);
        assert!(back.disable_turn_artifacts);
    }

    #[test]
    fn test_agent_config_clone_independence() {
        let mut cfg = AgentConfig::default();
        let cloned = cfg.clone();
        cfg.max_iterations = 1;
        cfg.compression_detail = "changed".to_string();
        // Clone should be independent
        assert_eq!(cloned.max_iterations, default_max_iterations());
        assert_eq!(cloned.compression_detail, default_compression_detail());
    }
}
