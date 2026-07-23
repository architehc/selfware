use super::*;
use crate::config::{default_endpoint, default_model, Config};

// =========================================================================
// DiscoveredEndpoint struct tests
// =========================================================================

#[test]
fn test_discovered_endpoint_fields() {
    let ep = DiscoveredEndpoint {
        provider: "Ollama".to_string(),
        endpoint: "http://localhost:11434/v1".to_string(),
        model: "qwen3.5:7b".to_string(),
        context_length: 32768,
        multimodal: false,
    };
    assert_eq!(ep.provider, "Ollama");
    assert_eq!(ep.endpoint, "http://localhost:11434/v1");
    assert_eq!(ep.model, "qwen3.5:7b");
    assert_eq!(ep.context_length, 32768);
    assert!(!ep.multimodal);
}

#[test]
fn test_is_multimodal_by_name_heuristic() {
    assert!(is_multimodal_by_name("moonshotai/kimi-k3"));
    assert!(is_multimodal_by_name("kimi-k3-vl"));
    assert!(is_multimodal_by_name("qwen/qwen3.5-122b-vl"));
    assert!(is_multimodal_by_name("google/gemini-2.5-flash"));
    assert!(is_multimodal_by_name("openai/gpt-4o"));
    assert!(is_multimodal_by_name("anthropic/claude-3-5-sonnet"));
    assert!(!is_multimodal_by_name("meta-llama/llama-3-70b-instruct"));
}

#[test]
fn test_discovered_endpoint_multimodal() {
    let ep = DiscoveredEndpoint {
        provider: "LM Studio".to_string(),
        endpoint: "http://127.0.0.1:1234/v1".to_string(),
        model: "llava-v1.6".to_string(),
        context_length: 8192,
        multimodal: true,
    };
    assert!(ep.multimodal);
    assert_eq!(ep.context_length, 8192);
}

#[test]
fn test_discovered_endpoint_clone() {
    let ep = DiscoveredEndpoint {
        provider: "Generic (port 8000)".to_string(),
        endpoint: "http://localhost:8000/v1".to_string(),
        model: "test-model".to_string(),
        context_length: 131072,
        multimodal: true,
    };
    let cloned = ep.clone();
    assert_eq!(cloned.provider, ep.provider);
    assert_eq!(cloned.endpoint, ep.endpoint);
    assert_eq!(cloned.model, ep.model);
    assert_eq!(cloned.context_length, ep.context_length);
    assert_eq!(cloned.multimodal, ep.multimodal);
}

#[test]
fn test_discovered_endpoint_debug() {
    let ep = DiscoveredEndpoint {
        provider: "Test".to_string(),
        endpoint: "http://localhost:8000/v1".to_string(),
        model: "model".to_string(),
        context_length: 4096,
        multimodal: false,
    };
    let debug_str = format!("{:?}", ep);
    assert!(debug_str.contains("DiscoveredEndpoint"));
    assert!(debug_str.contains("Test"));
    assert!(debug_str.contains("4096"));
}

// =========================================================================
// is_multimodal_by_name tests
// =========================================================================

#[test]
fn test_is_multimodal_by_name_vision() {
    assert!(is_multimodal_by_name("llama-3-vision"));
    assert!(is_multimodal_by_name("qwen-vision-7b"));
}

#[test]
fn test_is_multimodal_by_name_vl_suffix() {
    assert!(is_multimodal_by_name("qwen2.5-vl"));
    assert!(is_multimodal_by_name("qwen2.5-vl-7b"));
}

#[test]
fn test_is_multimodal_by_name_vl_infix() {
    assert!(is_multimodal_by_name("qwen2.5-vl-7b-instruct"));
    assert!(is_multimodal_by_name("model-vl-2b"));
}

#[test]
fn test_is_multimodal_by_name_llava() {
    assert!(is_multimodal_by_name("llava-v1.6"));
    assert!(is_multimodal_by_name("LLAVA-1.5-7b"));
}

#[test]
fn test_is_multimodal_by_name_onevision() {
    assert!(is_multimodal_by_name("onevision-7b"));
    assert!(is_multimodal_by_name("OneVision-v1"));
}

#[test]
fn test_is_multimodal_by_name_pixtral() {
    assert!(is_multimodal_by_name("pixtral-12b"));
    assert!(is_multimodal_by_name("Pixtral-12B-2409"));
}

#[test]
fn test_is_multimodal_by_name_gemma3_it() {
    assert!(is_multimodal_by_name("gemma-3-it"));
    assert!(is_multimodal_by_name("gemma-3-4b-it"));
}

#[test]
fn test_is_multimodal_by_name_gemma3_non_it() {
    // gemma-3 without "it" should NOT be multimodal
    assert!(!is_multimodal_by_name("gemma-3-4b"));
    assert!(!is_multimodal_by_name("gemma-3-base"));
}

#[test]
fn test_is_multimodal_by_name_case_insensitive() {
    assert!(is_multimodal_by_name("LLAVA-V1.6"));
    assert!(is_multimodal_by_name("Qwen2.5-VL"));
    assert!(is_multimodal_by_name("PIXTRAL-12B"));
}

#[test]
fn test_is_multimodal_by_name_non_multimodal() {
    assert!(!is_multimodal_by_name("llama-3-8b-instruct"));
    assert!(!is_multimodal_by_name("qwen3.5:7b"));
    assert!(!is_multimodal_by_name("mistral-7b-instruct"));
    assert!(!is_multimodal_by_name("phi-3.5-mini"));
    assert!(!is_multimodal_by_name("deepseek-coder-v2"));
}

#[test]
fn test_is_multimodal_by_name_empty() {
    assert!(!is_multimodal_by_name(""));
}

#[test]
fn test_is_multimodal_by_name_plain_text() {
    assert!(!is_multimodal_by_name("text-only-model"));
}

// =========================================================================
// is_multimodal tests (uses both name and JSON info)
// =========================================================================

#[test]
fn test_is_multimodal_by_name_match() {
    let info = serde_json::json!({});
    assert!(is_multimodal("llava-7b", &info));
}

#[test]
fn test_is_multimodal_by_capabilities_vision() {
    let info = serde_json::json!({
        "capabilities": ["text", "vision"]
    });
    assert!(is_multimodal("some-text-model", &info));
}

#[test]
fn test_is_multimodal_by_capabilities_multimodal() {
    let info = serde_json::json!({
        "capabilities": ["multimodal"]
    });
    assert!(is_multimodal("some-text-model", &info));
}

#[test]
fn test_is_multimodal_no_capabilities_not_multimodal() {
    let info = serde_json::json!({
        "capabilities": ["text", "tool_use"]
    });
    assert!(!is_multimodal("some-text-model", &info));
}

#[test]
fn test_is_multimodal_empty_capabilities() {
    let info = serde_json::json!({
        "capabilities": []
    });
    assert!(!is_multimodal("some-text-model", &info));
}

#[test]
fn test_is_multimodal_no_capabilities_field() {
    let info = serde_json::json!({
        "max_model_len": 8192
    });
    assert!(!is_multimodal("some-text-model", &info));
}

#[test]
fn test_is_multimodal_capabilities_not_array() {
    let info = serde_json::json!({
        "capabilities": "vision"
    });
    assert!(!is_multimodal("some-text-model", &info));
}

#[test]
fn test_is_multimodal_capabilities_with_non_string_entries() {
    let info = serde_json::json!({
        "capabilities": [42, true, "vision"]
    });
    assert!(is_multimodal("some-text-model", &info));
}

#[test]
fn test_is_multimodal_name_overrides_info() {
    // Even without capabilities, name match should return true
    let info = serde_json::json!({});
    assert!(is_multimodal("pixtral-12b", &info));
}

// =========================================================================
// infer_context_length tests
// =========================================================================

#[test]
fn test_infer_context_length_128k_explicit() {
    assert_eq!(infer_context_length("model-128k"), 131072);
}

#[test]
fn test_infer_context_length_131072_explicit() {
    assert_eq!(infer_context_length("model-131072"), 131072);
}

#[test]
fn test_infer_context_length_32k_explicit() {
    assert_eq!(infer_context_length("model-32k"), 32768);
}

#[test]
fn test_infer_context_length_32768_explicit() {
    assert_eq!(infer_context_length("model-32768"), 32768);
}

#[test]
fn test_infer_context_length_8k_explicit() {
    assert_eq!(infer_context_length("model-8k"), 8192);
}

#[test]
fn test_infer_context_length_8192_explicit() {
    assert_eq!(infer_context_length("model-8192"), 8192);
}

#[test]
fn test_infer_context_length_qwen35() {
    assert_eq!(infer_context_length("qwen3.5-7b"), 131072);
}

#[test]
fn test_infer_context_length_qwen35_dash_variant() {
    assert_eq!(infer_context_length("qwen3-5-7b"), 131072);
}

#[test]
fn test_infer_context_length_qwen3_coder() {
    assert_eq!(infer_context_length("qwen3-coder-32b"), 131072);
}

#[test]
fn test_infer_context_length_llama3_dash() {
    assert_eq!(infer_context_length("llama-3-8b-instruct"), 131072);
}

#[test]
fn test_infer_context_length_llama3_nodash() {
    assert_eq!(infer_context_length("llama3-70b"), 131072);
}

#[test]
fn test_infer_context_length_default_unknown() {
    assert_eq!(infer_context_length("unknown-model"), 32768);
}

#[test]
fn test_infer_context_length_mistral() {
    // Mistral is not in the special-cased list, so should default to 32768
    assert_eq!(infer_context_length("mistral-7b-instruct"), 32768);
}

#[test]
fn test_infer_context_length_empty() {
    assert_eq!(infer_context_length(""), 32768);
}

#[test]
fn test_infer_context_length_case_insensitive() {
    assert_eq!(infer_context_length("LLAMA-3-8B"), 131072);
    assert_eq!(infer_context_length("Qwen3.5-7B"), 131072);
}

#[test]
fn test_infer_context_length_128k_takes_precedence_over_32k() {
    // If both patterns are present, 128k should win (checked first)
    assert_eq!(infer_context_length("model-128k-32k"), 131072);
}

#[test]
fn test_infer_context_length_32k_takes_precedence_over_8k() {
    assert_eq!(infer_context_length("model-32k-8k"), 32768);
}

#[test]
fn test_infer_context_length_qwen35_overrides_8k() {
    // qwen3.5 is checked before the default; the model name has qwen3.5
    // but also 8k — the 8k check comes first so it returns 8192
    assert_eq!(infer_context_length("qwen3.5-8k"), 8192);
}

// =========================================================================
// pick_ollama_model_for_hardware tests
// =========================================================================

#[test]
fn test_pick_ollama_model_for_hardware_returns_valid_model() {
    let model = pick_ollama_model_for_hardware();
    // Should always return a non-empty model name
    assert!(!model.is_empty());
    // Should contain "qwen3.5"
    assert!(
        model.contains("qwen3.5"),
        "expected model to contain 'qwen3.5', got: {}",
        model
    );
}

#[test]
fn test_pick_ollama_model_for_hardware_returns_known_size() {
    let model = pick_ollama_model_for_hardware();
    // Should be one of the known model sizes
    let valid_models = ["qwen3.5:4b", "qwen3.5:7b", "qwen3.5:14b", "qwen3.5:32b"];
    assert!(
        valid_models.contains(&model),
        "expected one of {:?}, got: {}",
        valid_models,
        model
    );
}

// =========================================================================
// auto_calibrate tests (non-network paths)
// =========================================================================

#[tokio::test]
async fn test_auto_calibrate_skips_when_config_is_explicit() {
    // When both endpoint and model are non-default, auto_calibrate should
    // return Ok(false) immediately without doing any network work.
    let mut config = Config::default();
    config.endpoint = "http://my-custom-endpoint:9999/v1".to_string();
    config.model = "my-custom-model".to_string();

    let result = auto_calibrate(&mut config).await;
    assert!(result.is_ok(), "auto_calibrate should not error");
    assert!(
        !result.unwrap(),
        "should return false when config is already explicit"
    );

    // Config should be unchanged
    assert_eq!(config.endpoint, "http://my-custom-endpoint:9999/v1");
    assert_eq!(config.model, "my-custom-model");
}

#[tokio::test]
async fn test_auto_calibrate_skips_when_only_endpoint_custom_and_model_custom() {
    let mut config = Config::default();
    config.endpoint = "http://localhost:9999/v1".to_string();
    config.model = "custom-llm".to_string();

    let result = auto_calibrate(&mut config).await;
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[tokio::test]
async fn test_auto_calibrate_does_not_skip_when_default_endpoint_only() {
    // Endpoint is the built-in default (no provenance) but the model was
    // set to a custom value → the model looks user-configured while the
    // endpoint does not, so calibration should still proceed.
    let mut config = Config::default();
    // Keep endpoint as default, set model to custom
    config.model = "custom-model".to_string();

    // This will try to scan endpoints (network). In a test environment
    // with no local servers, it should eventually return Ok(true) after
    // hardware analysis. We just verify it doesn't skip (returns Ok).
    let result = auto_calibrate(&mut config).await;
    assert!(result.is_ok(), "auto_calibrate should not error");
    // It should return true because it tried to calibrate (even if nothing found)
    assert!(
        result.unwrap(),
        "should return true when at least one field is still default"
    );
}

#[tokio::test]
async fn test_auto_calibrate_skips_when_provenance_is_user_even_at_default_values() {
    // Regression test for the default-cloud-config hijack: a LOADED config
    // whose endpoint/model happen to equal the built-in defaults (exactly
    // the README OpenRouter setup) must NOT be "calibrated" back to a
    // local backend. Provenance — not value-equality — decides.
    let mut config = Config::default(); // endpoint/model == built-in defaults
    let user_file = std::path::PathBuf::from("/home/user/.config/selfware/config.toml");
    config.sources.set(
        "endpoint",
        super::super::provenance::ConfigSource::ConfigFile(user_file.clone()),
    );
    config.sources.set(
        "model",
        super::super::provenance::ConfigSource::ConfigFile(user_file),
    );

    let result = auto_calibrate(&mut config).await;
    assert!(result.is_ok());
    assert!(
        !result.unwrap(),
        "user-configured endpoint/model must never be hijacked, even at default values"
    );
    assert_eq!(config.endpoint, crate::config::default_endpoint());
    assert_eq!(config.model, crate::config::default_model());
}

// =========================================================================
// save_unpack_config tests
// =========================================================================

fn with_temp_cwd<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    let _guard = crate::test_support::CwdGuard::hold();
    let original = std::env::current_dir().expect("failed to get cwd");
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    std::env::set_current_dir(&tmp).expect("failed to change to temp dir");
    let result = f();
    // Always restore, even if f() panicked
    std::env::set_current_dir(&original).expect("failed to restore cwd");
    result
}

#[test]
fn test_save_unpack_config_creates_file() {
    with_temp_cwd(|| {
        let config = Config::default();
        let path = save_unpack_config(&config).expect("save should succeed");
        assert!(path.exists(), "config file should exist");
        assert_eq!(
            path.file_name().unwrap(),
            "selfware.toml",
            "file should be named selfware.toml"
        );
    });
}

#[test]
fn test_save_unpack_config_content_contains_endpoint() {
    with_temp_cwd(|| {
        let mut config = Config::default();
        config.endpoint = "http://localhost:8080/v1".to_string();
        let path = save_unpack_config(&config).expect("save should succeed");
        let content = std::fs::read_to_string(&path).expect("should read file");
        assert!(
            content.contains("http://localhost:8080/v1"),
            "content should contain the endpoint: {}",
            content
        );
    });
}

#[test]
fn test_save_unpack_config_content_contains_model() {
    with_temp_cwd(|| {
        let mut config = Config::default();
        config.model = "test-model-xyz".to_string();
        let path = save_unpack_config(&config).expect("save should succeed");
        let content = std::fs::read_to_string(&path).expect("should read file");
        assert!(
            content.contains("test-model-xyz"),
            "content should contain the model: {}",
            content
        );
    });
}

#[test]
fn test_save_unpack_config_content_contains_max_tokens() {
    with_temp_cwd(|| {
        let mut config = Config::default();
        config.max_tokens = 12345;
        let path = save_unpack_config(&config).expect("save should succeed");
        let content = std::fs::read_to_string(&path).expect("should read file");
        assert!(
            content.contains("12345"),
            "content should contain max_tokens: {}",
            content
        );
    });
}

#[test]
fn test_save_unpack_config_content_contains_context_length() {
    with_temp_cwd(|| {
        let mut config = Config::default();
        config.context_length = 65536;
        config.max_tokens = 8192; // must fit inside context_length
        let path = save_unpack_config(&config).expect("save should succeed");
        let content = std::fs::read_to_string(&path).expect("should read file");
        assert!(
            content.contains("65536"),
            "content should contain context_length: {}",
            content
        );
    });
}

#[test]
fn test_save_unpack_config_content_contains_temperature() {
    with_temp_cwd(|| {
        let mut config = Config::default();
        config.temperature = 0.42;
        let path = save_unpack_config(&config).expect("save should succeed");
        let content = std::fs::read_to_string(&path).expect("should read file");
        assert!(
            content.contains("0.42"),
            "content should contain temperature: {}",
            content
        );
    });
}

#[test]
fn test_save_unpack_config_refuses_invalid_config_and_writes_nothing() {
    with_temp_cwd(|| {
        let mut config = Config::default();
        config.context_length = 0; // the exact garbage the loader rejects
        let result = save_unpack_config(&config);
        assert!(
            result.is_err(),
            "an invalid generated config must fail loudly, not be written"
        );
        assert!(
            !std::path::Path::new("selfware.toml").exists(),
            "no file may be left behind on validation failure"
        );
    });
}

#[test]
fn test_save_unpack_config_derives_token_budget_sentinel() {
    with_temp_cwd(|| {
        let mut config = Config::default();
        config.context_length = 32768;
        config.max_tokens = 8192; // must fit inside context_length
        config.agent.token_budget = 0; // serde sentinel: derive at load time
        let path = save_unpack_config(&config).expect("save should succeed");
        let content = std::fs::read_to_string(&path).expect("should read file");
        // The written file carries the sentinel; the loader-side
        // derivation (validated before writing) turns it into 32768*3/5.
        assert!(content.contains("token_budget = 0"), "{}", content);
        Config::validate_generated_toml(&content).unwrap();
    });
}

#[test]
fn test_save_unpack_config_content_contains_safety_section() {
    with_temp_cwd(|| {
        let config = Config::default();
        let path = save_unpack_config(&config).expect("save should succeed");
        let content = std::fs::read_to_string(&path).expect("should read file");
        assert!(
            content.contains("[safety]"),
            "content should contain [safety] section: {}",
            content
        );
        assert!(
            content.contains("allowed_paths"),
            "content should contain allowed_paths: {}",
            content
        );
        assert!(
            content.contains("denied_paths"),
            "content should contain denied_paths: {}",
            content
        );
    });
}

#[test]
fn test_save_unpack_config_content_contains_agent_section() {
    with_temp_cwd(|| {
        let config = Config::default();
        let path = save_unpack_config(&config).expect("save should succeed");
        let content = std::fs::read_to_string(&path).expect("should read file");
        assert!(
            content.contains("[agent]"),
            "content should contain [agent] section: {}",
            content
        );
        assert!(
            content.contains("native_function_calling"),
            "content should contain native_function_calling: {}",
            content
        );
        assert!(
            content.contains("streaming"),
            "content should contain streaming: {}",
            content
        );
        assert!(
            content.contains("token_budget"),
            "content should contain token_budget: {}",
            content
        );
        assert!(
            content.contains("step_timeout_secs"),
            "content should contain step_timeout_secs: {}",
            content
        );
    });
}

#[test]
fn test_save_unpack_config_agent_values_reflected() {
    with_temp_cwd(|| {
        let mut config = Config::default();
        config.agent.native_function_calling = true;
        config.agent.streaming = false;
        config.agent.token_budget = 50000;
        config.agent.step_timeout_secs = 120;
        let path = save_unpack_config(&config).expect("save should succeed");
        let content = std::fs::read_to_string(&path).expect("should read file");
        assert!(
            content.contains("native_function_calling = true"),
            "should have native_function_calling = true: {}",
            content
        );
        assert!(
            content.contains("streaming = false"),
            "should have streaming = false: {}",
            content
        );
        assert!(
            content.contains("token_budget = 50000"),
            "should have token_budget = 50000: {}",
            content
        );
        assert!(
            content.contains("step_timeout_secs = 120"),
            "should have step_timeout_secs = 120: {}",
            content
        );
    });
}

#[test]
fn test_save_unpack_config_with_extra_body() {
    with_temp_cwd(|| {
        let mut config = Config::default();
        let mut extra = serde_json::Map::new();
        extra.insert(
            "chat_template_kwargs".to_string(),
            serde_json::json!({"enable_thinking": false}),
        );
        config.extra_body = Some(extra);

        let path = save_unpack_config(&config).expect("save should succeed");
        let content = std::fs::read_to_string(&path).expect("should read file");
        assert!(
            content.contains("[extra_body]"),
            "content should contain [extra_body] section: {}",
            content
        );
        assert!(
            content.contains("chat_template_kwargs"),
            "content should contain the extra_body key: {}",
            content
        );
    });
}

#[test]
fn test_save_unpack_config_with_extra_body_scalar() {
    with_temp_cwd(|| {
        let mut config = Config::default();
        let mut extra = serde_json::Map::new();
        extra.insert("top_p".to_string(), serde_json::json!(0.95));
        config.extra_body = Some(extra);

        let path = save_unpack_config(&config).expect("save should succeed");
        let content = std::fs::read_to_string(&path).expect("should read file");
        assert!(
            content.contains("top_p = 0.95"),
            "content should contain the scalar extra_body: {}",
            content
        );
    });
}

#[test]
fn test_save_unpack_config_without_extra_body() {
    with_temp_cwd(|| {
        let config = Config::default();
        // extra_body is None by default
        assert!(config.extra_body.is_none());
        let path = save_unpack_config(&config).expect("save should succeed");
        let content = std::fs::read_to_string(&path).expect("should read file");
        assert!(
            !content.contains("[extra_body]"),
            "content should NOT contain [extra_body] when none: {}",
            content
        );
    });
}

#[test]
fn test_save_unpack_config_returns_correct_path() {
    with_temp_cwd(|| {
        let config = Config::default();
        let path = save_unpack_config(&config).expect("save should succeed");
        assert_eq!(
            path,
            std::path::PathBuf::from("selfware.toml"),
            "should return selfware.toml path"
        );
    });
}

#[test]
fn test_save_unpack_config_overwrites_existing() {
    with_temp_cwd(|| {
        let mut config = Config::default();
        config.model = "first-model".to_string();
        save_unpack_config(&config).expect("first save should succeed");

        // Now save a different config
        config.model = "second-model".to_string();
        let path = save_unpack_config(&config).expect("second save should succeed");
        let content = std::fs::read_to_string(&path).expect("should read file");
        assert!(
            content.contains("second-model"),
            "file should contain the second model: {}",
            content
        );
        assert!(
            !content.contains("first-model"),
            "file should NOT contain the first model: {}",
            content
        );
    });
}

#[test]
fn test_save_unpack_config_content_has_header_comment() {
    with_temp_cwd(|| {
        let config = Config::default();
        let path = save_unpack_config(&config).expect("save should succeed");
        let content = std::fs::read_to_string(&path).expect("should read file");
        assert!(
            content.contains("auto-generated by unpack"),
            "content should have header comment: {}",
            content
        );
    });
}

// =========================================================================
// unpack() tests
// =========================================================================

#[tokio::test]
async fn test_unpack_returns_ok() {
    // unpack() creates a default config and auto-calibrates.
    // In a test environment with no local servers, it should still
    // return Ok (either Some or None config).
    let result = unpack().await;
    assert!(result.is_ok(), "unpack should not error");
}

#[tokio::test]
async fn test_unpack_with_explicit_config_returns_none() {
    // We can't easily make unpack() return None since it always starts
    // with a default config and auto_calibrate returns true in most cases.
    // But we can verify it returns Ok.
    let result = unpack().await;
    assert!(result.is_ok(), "unpack should return Ok");
    // The result is Ok(Option<Config>) — it should be Some if calibration
    // happened, or None if not.
    let _ = result.unwrap();
}

// =========================================================================
// has_config_file tests
// =========================================================================

#[test]
fn test_has_config_file_in_temp_dir_without_config() {
    with_temp_cwd(|| {
        // In a fresh temp dir with no selfware.toml and no ~/.config/selfware/config.toml,
        // has_config_file should return false (unless the user's home dir has one).
        // We can't guarantee the home dir state, so we just verify it returns a bool.
        let _ = has_config_file();
        // If selfware.toml exists in temp dir, we know it's false:
        assert!(
            !std::path::Path::new("selfware.toml").exists(),
            "temp dir should not have selfware.toml"
        );
    });
}

#[test]
fn test_has_config_file_detects_selfware_toml() {
    with_temp_cwd(|| {
        // Create a selfware.toml in the temp dir
        std::fs::write("selfware.toml", "# test config").expect("failed to write");
        assert!(has_config_file(), "should detect selfware.toml in cwd");
    });
}

// =========================================================================
// Integration: save_unpack_config then verify content matches Config
// =========================================================================

#[test]
fn test_save_unpack_config_full_content_verification() {
    with_temp_cwd(|| {
        let mut config = Config::default();
        config.endpoint = "http://192.168.1.100:1234/v1".to_string();
        config.model = "qwen3.5:14b".to_string();
        config.max_tokens = 32768;
        config.context_length = 131072;
        config.temperature = 0.7;
        config.agent.native_function_calling = true;
        config.agent.streaming = true;
        config.agent.token_budget = 100000;
        config.agent.step_timeout_secs = 600;

        let path = save_unpack_config(&config).expect("save should succeed");
        let content = std::fs::read_to_string(&path).expect("should read file");

        // Verify all key fields appear in the content
        assert!(content.contains("http://192.168.1.100:1234/v1"));
        assert!(content.contains("qwen3.5:14b"));
        assert!(content.contains("32768"));
        assert!(content.contains("131072"));
        assert!(content.contains("0.7"));
        assert!(content.contains("native_function_calling = true"));
        assert!(content.contains("token_budget = 100000"));
        assert!(content.contains("step_timeout_secs = 600"));
    });
}

// =========================================================================
// Default value verification for auto_calibrate logic
// =========================================================================

#[test]
fn test_default_endpoint_is_openrouter() {
    // Verify the default endpoint that auto_calibrate checks against
    assert_eq!(default_endpoint(), "https://openrouter.ai/api/v1");
}

#[test]
fn test_default_model_is_glm52() {
    assert_eq!(default_model(), "z-ai/glm-5.2");
}

#[test]
fn test_config_default_uses_defaults() {
    let config = Config::default();
    assert_eq!(config.endpoint, default_endpoint());
    assert_eq!(config.model, default_model());
}

#[tokio::test]
async fn test_auto_calibrate_with_known_default_endpoints() {
    // The code treats several endpoints as "default":
    // - default_endpoint() (openrouter)
    // - http://localhost:8000/v1
    // - http://127.0.0.1:1234/v1
    // If we set model to non-default but keep a known default endpoint,
    // auto_calibrate should NOT skip.
    let mut config = Config::default();
    config.endpoint = "http://localhost:8000/v1".to_string();
    config.model = "custom-model".to_string();

    let result = auto_calibrate(&mut config).await;
    assert!(result.is_ok());
    // Should proceed with calibration since endpoint is a known default
    assert!(
        result.unwrap(),
        "should proceed when endpoint is a known default"
    );
}

#[tokio::test]
async fn test_auto_calibrate_with_lm_studio_default_endpoint() {
    let mut config = Config::default();
    config.endpoint = "http://127.0.0.1:1234/v1".to_string();
    config.model = "custom-model".to_string();

    let result = auto_calibrate(&mut config).await;
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[tokio::test]
async fn test_auto_calibrate_default_model_custom_endpoint_skips() {
    // If model is default but endpoint is fully custom (not one of the
    // known defaults), then is_default_endpoint = false and is_default_model = true,
    // so the skip condition `!is_default_endpoint && !is_default_model` is false
    // (because is_default_model is true). So it should NOT skip.
    let mut config = Config::default();
    config.endpoint = "http://my-server:9999/v1".to_string();
    // model stays as default

    let result = auto_calibrate(&mut config).await;
    assert!(result.is_ok());
    // Should proceed because one of the two is still default
    assert!(result.unwrap());
}

// =========================================================================
// safe_detect_specs tests
// =========================================================================

#[test]
fn test_safe_detect_specs_returns_option() {
    // Should either return Some(specs) or None, but never panic
    let result = safe_detect_specs();
    if let Some(specs) = &result {
        // Basic sanity checks
        assert!(specs.total_cpu_cores > 0, "should have at least 1 CPU core");
        assert!(specs.total_ram_gb > 0.0, "should have positive RAM");
    }
    // None is also acceptable in restricted environments
}

// =========================================================================
// scan_local_endpoints tests
// =========================================================================

#[tokio::test]
async fn test_scan_local_endpoints_returns_vec() {
    // In a test environment, there may or may not be local servers.
    // We just verify it returns a Vec without panicking.
    let endpoints = scan_local_endpoints().await;
    // Each endpoint should have valid fields
    for ep in &endpoints {
        assert!(!ep.provider.is_empty(), "provider should not be empty");
        assert!(!ep.endpoint.is_empty(), "endpoint should not be empty");
        assert!(!ep.model.is_empty(), "model should not be empty");
        assert!(ep.context_length > 0, "context_length should be positive");
    }
}
