use super::*;

#[test]
fn test_is_qwen35_model() {
    assert!(is_qwen35_model("Qwen3.5-122B-A10B"));
    assert!(is_qwen35_model("qwen3.5-32b"));
    assert!(is_qwen35_model("Qwen3-5-122B"));
    assert!(!is_qwen35_model("Qwen3-Coder"));
    assert!(!is_qwen35_model("llama-3"));
}

#[test]
fn test_is_qwen_model() {
    assert!(is_qwen_model("Qwen3.5-122B-A10B"));
    assert!(is_qwen_model("Qwen/Qwen3-Coder-Next-FP8"));
    assert!(!is_qwen_model("llama-3.1-70b"));
}

#[test]
fn test_is_model_small() {
    assert!(is_model_small("qwen-7b"));
    assert!(is_model_small("llama-3b-instruct"));
    assert!(is_model_small("phi-2b"));
    assert!(!is_model_small("qwen-72b"));
    assert!(!is_model_small("qwen3.5-122b"));
    assert!(!is_model_small("qwen-14b"));
}

#[test]
fn test_parse_models_empty() {
    let body = serde_json::json!({"data": []});
    let models = parse_models(&body);
    assert!(models.is_empty());
}

#[test]
fn test_parse_models_with_data() {
    let body = serde_json::json!({
        "data": [
            {
                "id": "Qwen/Qwen3.5-122B-A10B",
                "max_model_len": 131072
            },
            {
                "id": "other-model",
                "context_length": 8192
            }
        ]
    });
    let models = parse_models(&body);
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "Qwen/Qwen3.5-122B-A10B");
    assert_eq!(models[0].max_model_len, Some(131072));
    assert_eq!(models[1].id, "other-model");
    assert_eq!(models[1].max_model_len, Some(8192));
}

#[test]
fn test_configured_enable_thinking_false() {
    let config = Config {
        extra_body: Some({
            let mut extra = serde_json::Map::new();
            extra.insert(
                "chat_template_kwargs".to_string(),
                serde_json::json!({ "enable_thinking": false }),
            );
            extra
        }),
        ..Config::default()
    };

    assert_eq!(configured_enable_thinking(&config), Some(false));
}

#[test]
fn test_configured_enable_thinking_missing() {
    let config = Config::default();
    assert_eq!(configured_enable_thinking(&config), None);
}

#[test]
fn test_connection_test_timeout_respects_minimum() {
    let config = Config {
        agent: crate::config::AgentConfig {
            step_timeout_secs: 5,
            ..crate::config::AgentConfig::default()
        },
        ..Config::default()
    };
    assert_eq!(
        connection_test_timeout(&config),
        Duration::from_secs(MIN_CONNECTION_TEST_TIMEOUT_SECS)
    );
}

#[test]
fn test_connection_test_timeout_respects_maximum() {
    let config = Config {
        agent: crate::config::AgentConfig {
            step_timeout_secs: 600,
            ..crate::config::AgentConfig::default()
        },
        ..Config::default()
    };
    assert_eq!(
        connection_test_timeout(&config),
        Duration::from_secs(MAX_CONNECTION_TEST_TIMEOUT_SECS)
    );
}

#[test]
fn test_truncate_str() {
    assert_eq!(truncate_str("hello", 10), "hello");
    assert_eq!(truncate_str("hello world foo bar", 10), "hello w...");
}

#[test]
fn test_backend_display() {
    assert_eq!(Backend::Sglang.to_string(), "sglang");
    assert_eq!(Backend::Vllm.to_string(), "vllm");
    assert_eq!(Backend::Ollama.to_string(), "ollama");
    assert_eq!(Backend::LlamaCpp.to_string(), "llama.cpp");
    assert_eq!(Backend::LmStudio.to_string(), "lmstudio");
    assert_eq!(
        Backend::Unknown("test".to_string()).to_string(),
        "unknown (test)"
    );
}

#[test]
fn test_extract_tokens_per_second() {
    let body = serde_json::json!({
        "usage": {
            "completion_tokens": 10
        }
    });
    let tps = extract_tokens_per_second(&body, Duration::from_secs(1));
    assert_eq!(tps, Some(10.0));

    let empty = serde_json::json!({});
    assert_eq!(
        extract_tokens_per_second(&empty, Duration::from_secs(1)),
        None
    );
}

// =========================================================================
// is_model_small extended tests
// =========================================================================

#[test]
fn test_is_model_small_1b() {
    assert!(is_model_small("model-1b"));
}

#[test]
fn test_is_model_small_0_5b() {
    assert!(is_model_small("model-0.5b"));
}

#[test]
fn test_is_model_small_1_5b() {
    assert!(is_model_small("model-1.5b"));
}

#[test]
fn test_is_model_small_3b() {
    assert!(is_model_small("llama-3b-instruct"));
}

#[test]
fn test_is_model_small_4b() {
    assert!(is_model_small("phi-4b"));
}

#[test]
fn test_is_model_small_5b() {
    assert!(is_model_small("model_5b"));
}

#[test]
fn test_is_model_small_6b() {
    assert!(is_model_small("chatglm-6b"));
}

#[test]
fn test_is_model_small_7b_not_72b() {
    assert!(is_model_small("qwen-7b"));
    assert!(!is_model_small("qwen-72b"));
}

#[test]
fn test_not_small_14b() {
    assert!(!is_model_small("qwen-14b"));
}

#[test]
fn test_not_small_32b() {
    assert!(!is_model_small("qwen-32b"));
}

#[test]
fn test_not_small_70b() {
    assert!(!is_model_small("llama-70b"));
}

#[test]
fn test_not_small_122b() {
    assert!(!is_model_small("qwen3.5-122b"));
}

#[test]
fn test_is_model_small_underscore_separator() {
    assert!(is_model_small("model_7b"));
    assert!(is_model_small("model_3b_instruct"));
}

// =========================================================================
// is_qwen35_model extended tests
// =========================================================================

#[test]
fn test_is_qwen35_lowercase() {
    assert!(is_qwen35_model("qwen3.5-27b"));
}

#[test]
fn test_is_qwen35_mixed_case() {
    assert!(is_qwen35_model("QWEN3.5-122B-A10B"));
}

#[test]
fn test_is_qwen35_dash_variant() {
    assert!(is_qwen35_model("qwen3-5-27b"));
}

#[test]
fn test_not_qwen35_qwen2() {
    assert!(!is_qwen35_model("qwen2.5-72b"));
}

// =========================================================================
// is_qwen_model extended tests
// =========================================================================

#[test]
fn test_is_qwen_any_version() {
    assert!(is_qwen_model("Qwen2-72B"));
    assert!(is_qwen_model("qwen-1.5-7b"));
    assert!(is_qwen_model("Qwen3.5-122B"));
}

#[test]
fn test_not_qwen_other_model() {
    assert!(!is_qwen_model("llama-3-70b"));
    assert!(!is_qwen_model("phi-3-mini"));
}

// =========================================================================
// parse_models extended tests
// =========================================================================

#[test]
fn test_parse_models_no_data_field() {
    let body = serde_json::json!({"other": "value"});
    let models = parse_models(&body);
    assert!(models.is_empty());
}

#[test]
fn test_parse_models_data_not_array() {
    let body = serde_json::json!({"data": "string"});
    let models = parse_models(&body);
    assert!(models.is_empty());
}

#[test]
fn test_parse_models_with_max_tokens_field() {
    let body = serde_json::json!({
        "data": [{"id": "model-1", "max_tokens": 4096}]
    });
    let models = parse_models(&body);
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].max_model_len, Some(4096));
}

#[test]
fn test_parse_models_no_context_info() {
    let body = serde_json::json!({
        "data": [{"id": "minimal-model"}]
    });
    let models = parse_models(&body);
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "minimal-model");
    assert_eq!(models[0].max_model_len, None);
}

#[test]
fn test_parse_models_missing_id() {
    let body = serde_json::json!({
        "data": [{"max_model_len": 8192}]
    });
    let models = parse_models(&body);
    assert_eq!(models[0].id, "unknown");
}

// =========================================================================
// Backend display tests (extended)
// =========================================================================

#[test]
fn test_backend_equality() {
    assert_eq!(Backend::Sglang, Backend::Sglang);
    assert_ne!(Backend::Sglang, Backend::Vllm);
    assert_ne!(Backend::Ollama, Backend::LlamaCpp);
    assert_eq!(
        Backend::Unknown("test".to_string()),
        Backend::Unknown("test".to_string())
    );
    assert_ne!(
        Backend::Unknown("a".to_string()),
        Backend::Unknown("b".to_string())
    );
}

#[test]
fn test_backend_clone() {
    let b = Backend::Vllm;
    let cloned = b.clone();
    assert_eq!(b, cloned);
}

// =========================================================================
// extract_tokens_per_second extended tests
// =========================================================================

#[test]
fn test_extract_tps_zero_seconds() {
    let body = serde_json::json!({"usage": {"completion_tokens": 10}});
    let tps = extract_tokens_per_second(&body, Duration::from_secs(0));
    // 0 seconds -> secs_f64 == 0.0, condition `secs > 0.0` is false => None
    assert_eq!(tps, None);
}

#[test]
fn test_extract_tps_zero_tokens() {
    let body = serde_json::json!({"usage": {"completion_tokens": 0}});
    let tps = extract_tokens_per_second(&body, Duration::from_secs(1));
    assert_eq!(tps, None);
}

#[test]
fn test_extract_tps_no_usage() {
    let body = serde_json::json!({"choices": []});
    assert_eq!(
        extract_tokens_per_second(&body, Duration::from_secs(1)),
        None
    );
}

#[test]
fn test_extract_tps_no_completion_tokens() {
    let body = serde_json::json!({"usage": {"prompt_tokens": 100}});
    assert_eq!(
        extract_tokens_per_second(&body, Duration::from_secs(1)),
        None
    );
}

// =========================================================================
// truncate_str tests
// =========================================================================

#[test]
fn test_truncate_str_short() {
    assert_eq!(truncate_str("hi", 10), "hi");
}

#[test]
fn test_truncate_str_exact() {
    assert_eq!(truncate_str("12345", 5), "12345");
}

#[test]
fn test_truncate_str_long() {
    assert_eq!(truncate_str("hello world", 8), "hello...");
}

#[test]
fn test_truncate_str_very_short_max() {
    assert_eq!(truncate_str("hello", 3), "...");
}

// =========================================================================
// ModelInfo tests
// =========================================================================

#[test]
fn test_model_info_clone() {
    let info = ModelInfo {
        id: "test-model".to_string(),
        max_model_len: Some(131072),
        raw: serde_json::json!({}),
    };
    let cloned = info.clone();
    assert_eq!(cloned.id, "test-model");
    assert_eq!(cloned.max_model_len, Some(131072));
}

#[test]
fn test_model_info_debug() {
    let info = ModelInfo {
        id: "model".to_string(),
        max_model_len: None,
        raw: serde_json::json!({}),
    };
    let s = format!("{:?}", info);
    assert!(s.contains("model"));
}

// =========================================================================
// Header-based backend detection
// =========================================================================

#[test]
fn test_detect_backend_from_headers_vllm() {
    let headers = "x-vllm-version: 0.6.0\ncontent-type: application/json\n";
    let body = serde_json::json!({"data": []});
    assert_eq!(
        detect_backend_from_headers(headers, &body),
        Some(Backend::Vllm)
    );
}

#[test]
fn test_detect_backend_from_headers_sglang() {
    let headers = "server: sglang\n";
    let body = serde_json::json!({"data": []});
    assert_eq!(
        detect_backend_from_headers(headers, &body),
        Some(Backend::Sglang)
    );
}

#[test]
fn test_detect_backend_from_headers_ollama() {
    let headers = "server: ollama\n";
    let body = serde_json::json!({"data": []});
    assert_eq!(
        detect_backend_from_headers(headers, &body),
        Some(Backend::Ollama)
    );
}

#[test]
fn test_detect_backend_from_headers_lmstudio_via_body() {
    let headers = "content-type: application/json\n";
    let body = serde_json::json!({
        "data": [{"id": "lm-studio-model", "owned_by": "lmstudio"}]
    });
    assert_eq!(
        detect_backend_from_headers(headers, &body),
        Some(Backend::LmStudio)
    );
}

#[test]
fn test_detect_backend_from_headers_no_match() {
    let headers = "content-type: application/json\nserver: nginx\n";
    let body = serde_json::json!({"data": []});
    assert_eq!(detect_backend_from_headers(headers, &body), None);
}

// =========================================================================
// looks_multimodal heuristic
// =========================================================================

#[test]
fn test_looks_multimodal_positive() {
    assert!(looks_multimodal("Qwen3.5-VL-7B"));
    assert!(looks_multimodal("llava-1.5"));
    assert!(looks_multimodal("vision-large"));
    assert!(looks_multimodal("multimodal-pro"));
}

#[test]
fn test_looks_multimodal_negative() {
    assert!(!looks_multimodal("qwen-7b"));
    assert!(!looks_multimodal("llama-3-70b"));
}

// =========================================================================
// print_unified_check return value
// =========================================================================

#[test]
fn test_print_unified_check_returns_true_on_fail() {
    // Output is captured by the test framework; we only assert the boolean.
    assert!(print_unified_check(
        "x",
        DoctorCheckStatus::Missing,
        "fail",
        None
    ));
    assert!(!print_unified_check(
        "x",
        DoctorCheckStatus::Warning,
        "warn",
        None
    ));
    assert!(!print_unified_check("x", DoctorCheckStatus::Ok, "ok", None));
}

// =========================================================================
// Capabilities struct sanity
// =========================================================================

#[test]
fn test_capabilities_default() {
    let c = Capabilities::default();
    assert_eq!(c.tools, None);
    assert_eq!(c.streaming, None);
    assert_eq!(c.thinking, None);
    assert!(!c.multimodal);
}
