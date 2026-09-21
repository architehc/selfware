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
    assert_eq!(c.multimodal, None);
}

// =========================================================================
// Behavioral vision verification tests
// =========================================================================

#[test]
fn test_verify_vision_responses_both_conditioned() {
    assert!(verify_vision_responses(Some("Red"), Some("Blue")));
    assert!(verify_vision_responses(
        Some("The color is red."),
        Some("This is a blue square.")
    ));
}

#[test]
fn test_verify_vision_responses_unconditioned_invariant_white() {
    // Both return 'White' (the exact bug observed on the server)
    assert!(!verify_vision_responses(Some("White"), Some("White")));
    assert!(!verify_vision_responses(
        Some("The image is solid white."),
        Some("The image is solid white.")
    ));
}

#[test]
fn test_verify_vision_responses_unconditioned_invariant_same_color() {
    // Model blindly guesses 'Red' for both probes
    assert!(!verify_vision_responses(Some("red"), Some("red")));
}

#[test]
fn test_verify_vision_responses_missing_or_error() {
    assert!(!verify_vision_responses(None, Some("blue")));
    assert!(!verify_vision_responses(Some("red"), None));
    assert!(!verify_vision_responses(None, None));
}

#[test]
fn test_verify_vision_responses_hallucinated_or_contradictory() {
    // Mentions both red and blue in the answer
    assert!(!verify_vision_responses(
        Some("It looks red or blue"),
        Some("blue")
    ));
    // Completely irrelevant text (e.g. OCR hallucinating 13)
    assert!(!verify_vision_responses(Some("13"), Some("blue")));
}

#[test]
fn test_evaluate_vision_responses_distinguishes_inconclusive_empty_from_unconditioned_inverted() {
    // Conditioned: red has red, blue has blue
    assert_eq!(
        evaluate_vision_responses(Some("Red"), Some("Blue")),
        VisionProbeOutcome::Conditioned
    );
    assert_eq!(
        evaluate_vision_responses(Some("It is red"), Some("It is blue")),
        VisionProbeOutcome::Conditioned
    );

    // Inverted / color swap: red has blue, blue has red -> Unconditioned
    assert_eq!(
        evaluate_vision_responses(Some("Blue"), Some("Red")),
        VisionProbeOutcome::Unconditioned
    );

    // Invariant responses: both say same text -> Unconditioned
    assert_eq!(
        evaluate_vision_responses(Some("White"), Some("White")),
        VisionProbeOutcome::Unconditioned
    );
    assert_eq!(
        evaluate_vision_responses(Some("red"), Some("red")),
        VisionProbeOutcome::Unconditioned
    );

    // Inconclusive: empty string or token exhaustion
    assert_eq!(
        evaluate_vision_responses(Some(""), Some("blue")),
        VisionProbeOutcome::Inconclusive
    );
    assert_eq!(
        evaluate_vision_responses(Some("   "), Some("")),
        VisionProbeOutcome::Inconclusive
    );

    // Inconclusive: missing response (None)
    assert_eq!(
        evaluate_vision_responses(None, Some("blue")),
        VisionProbeOutcome::Inconclusive
    );
    assert_eq!(
        evaluate_vision_responses(Some("red"), None),
        VisionProbeOutcome::Inconclusive
    );
    assert_eq!(
        evaluate_vision_responses(None, None),
        VisionProbeOutcome::Inconclusive
    );

    // Inconclusive: non-color tokens (e.g. model output didn't contain red or blue)
    assert_eq!(
        evaluate_vision_responses(Some("13"), Some("42")),
        VisionProbeOutcome::Inconclusive
    );
}

#[test]
fn test_is_vision_configured_and_target_resolution() {
    let mut config = Config::default();
    assert!(!is_vision_configured("qwen3-coder", &config));

    // When a vision model profile is configured with modalities = ["text", "vision"]
    let profile = crate::config::ModelProfile {
        endpoint: "http://127.0.0.1:1234/v1".to_string(),
        model: "qwen-vl".to_string(),
        api_key: None,
        max_tokens: 4096,
        temperature: 0.0,
        modalities: vec!["text".to_string(), "vision".to_string()],
        context_length: 32768,
        extra_body: None,
        native_function_calling: None,
        max_retries: None,
        response_timeout_floor_secs: None,
    };
    config.models.insert("vision".to_string(), profile);

    // Only the model or profile with vision configured reports true
    assert!(is_vision_configured("qwen-vl", &config));
    assert!(is_vision_configured("vision", &config));
    assert!(
        !is_vision_configured("other-model", &config),
        "unrelated text models must NOT be treated as vision configured"
    );

    // Target resolution tests
    let target_vl = resolve_vision_target("qwen-vl", &config).expect("should resolve target");
    assert_eq!(target_vl.model, "qwen-vl");

    let target_text = resolve_vision_target("other-model", &config);
    assert!(
        target_text.is_none(),
        "text model with no vision config must resolve to None"
    );
}

#[test]
fn test_map_vision_status_and_detail_all_variants() {
    // 1. Pure text model, probe not run -> Ok "no vision modality configured"
    let (status, detail, fix) = map_vision_status_and_detail(None, None, false);
    assert_eq!(status, DoctorCheckStatus::Ok);
    assert!(detail.contains("no vision modality configured"));
    assert!(fix.is_none());

    // 2. Vision expected, probe not completed -> Warning
    let (status, detail, fix) =
        map_vision_status_and_detail(None, Some("primary model (qwen-vl)"), true);
    assert_eq!(status, DoctorCheckStatus::Warning);
    assert!(detail.contains("vision suggested"));
    assert!(fix.is_some());

    // 3. Unauthorized HTTP 401/403 -> Warning with auth-specific fix hint (not color tokens!)
    let (status, detail, fix) = map_vision_status_and_detail(
        Some(VisionProbeOutcome::Unauthorized),
        Some("model profile 'vision' (qwen-vl)"),
        true,
    );
    assert_eq!(status, DoctorCheckStatus::Warning);
    assert!(detail.contains("authentication error"));
    assert!(fix.unwrap().contains("API key"));

    // 4. Conditioned -> Ok
    let (status, detail, fix) = map_vision_status_and_detail(
        Some(VisionProbeOutcome::Conditioned),
        Some("primary model (qwen-vl)"),
        true,
    );
    assert_eq!(status, DoctorCheckStatus::Ok);
    assert!(detail.contains("conditioned"));
    assert!(fix.is_none());

    // 5. Unconditioned -> Warning
    let (status, detail, fix) = map_vision_status_and_detail(
        Some(VisionProbeOutcome::Unconditioned),
        Some("primary model (qwen-vl)"),
        true,
    );
    assert_eq!(status, DoctorCheckStatus::Warning);
    assert!(detail.contains("failed image conditioning"));
    assert!(fix.is_some());

    // 6. Inconclusive -> Warning
    let (status, detail, fix) = map_vision_status_and_detail(
        Some(VisionProbeOutcome::Inconclusive),
        Some("primary model (qwen-vl)"),
        true,
    );
    assert_eq!(status, DoctorCheckStatus::Warning);
    assert!(detail.contains("unknown"));
    assert!(fix.is_some());
}

#[tokio::test]
async fn test_probe_vision_conditioning_401_returns_unauthorized_without_second_probe() {
    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_error(401, "{\"error\": \"Unauthorized\"}")
        .build()
        .await;

    let client = reqwest::Client::new();
    let url = format!("{}/v1/chat/completions", server.url());
    let outcome = probe_vision_conditioning(&client, &url, "qwen-vl", Some("invalid-key")).await;

    assert_eq!(outcome, VisionProbeOutcome::Unauthorized);
    assert_eq!(
        server.captured_request_bodies().await.len(),
        1,
        "must not send second (blue) probe after red probe 401"
    );
    server.stop().await;
}

#[test]
fn test_resolve_vision_target_precedence_and_credential_scoping() {
    let mut config = Config::default();
    config.endpoint = "https://primary-llm.example.com/v1".to_string();
    config.model = "qwen-vl".to_string();
    config.api_key = Some(crate::config::RedactedString::new("primary-secret-key"));

    // Case 1: Explicit profile takes precedence over looks_multimodal(model)
    let mut profile1 = crate::config::ModelProfile {
        endpoint: "https://custom-vision.example.com/v1".to_string(),
        model: "qwen-vl".to_string(),
        api_key: None,
        max_tokens: 4096,
        temperature: 0.0,
        modalities: vec!["text".to_string(), "vision".to_string()],
        context_length: 32768,
        extra_body: None,
        native_function_calling: None,
        max_retries: None,
        response_timeout_floor_secs: None,
    };
    config.models.insert("vision".to_string(), profile1.clone());

    let target = resolve_vision_target("qwen-vl", &config).expect("must resolve target");
    assert_eq!(target.endpoint, "https://custom-vision.example.com/v1");
    // Case 2: Endpoint on different host must NOT inherit primary API key!
    assert!(
        target.api_key.is_none(),
        "profile on different endpoint must not inherit primary API key"
    );

    // Case 3: Explicit profile with its own API key uses its own key
    profile1.api_key = Some(crate::config::RedactedString::new("profile-secret-key"));
    config.models.insert("vision".to_string(), profile1);
    let target_with_key = resolve_vision_target("qwen-vl", &config).expect("must resolve target");
    assert_eq!(target_with_key.api_key, Some("profile-secret-key"));

    // Case 4: Profile on SAME host inherits primary key when profile key is None
    let profile_same_host = crate::config::ModelProfile {
        endpoint: "https://primary-llm.example.com/v2".to_string(),
        model: "qwen-vl-fast".to_string(),
        api_key: None,
        max_tokens: 4096,
        temperature: 0.0,
        modalities: vec!["vision".to_string()],
        context_length: 32768,
        extra_body: None,
        native_function_calling: None,
        max_retries: None,
        response_timeout_floor_secs: None,
    };
    config
        .models
        .insert("same-host".to_string(), profile_same_host);
    let target_same = resolve_vision_target("qwen-vl-fast", &config).expect("must resolve target");
    assert_eq!(target_same.api_key, Some("primary-secret-key"));
}

// =========================================================================
// Streamed tool-call probe: verdict classification (fixtures)
// =========================================================================

/// Raw SSE stream that delivers a native `tool_calls` delta (OpenAI-style).
const SSE_NATIVE_TOOL_CALL: &str = r#"data: {"choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}

data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"calculator","arguments":""}}]},"finish_reason":null}]}

data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"expression\":\"2+2\"}"}}]},"finish_reason":null}]}

data: {"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}

data: [DONE]

"#;

/// Raw SSE stream that delivers a text/XML tool call inside streamed content
/// (GLM/Qwen text-format models).
const SSE_TEXT_XML_TOOL_CALL: &str = r#"data: {"choices":[{"index":0,"delta":{"content":"<tool>\n<name>calculator</name>\n<arguments>{\"expression\":\"2+2\"}</arguments>\n</tool>"},"finish_reason":null}]}

data: {"choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}

data: [DONE]

"#;

#[test]
fn test_classify_streaming_tool_call_native_sse_delivered() {
    assert_eq!(
        classify_streaming_tool_call_body(SSE_NATIVE_TOOL_CALL),
        StreamToolCallVerdict::Delivered
    );
}

#[test]
fn test_classify_streaming_tool_call_text_xml_delivered() {
    assert_eq!(
        classify_streaming_tool_call_body(SSE_TEXT_XML_TOOL_CALL),
        StreamToolCallVerdict::Delivered
    );
}

#[test]
fn test_classify_streaming_tool_call_healthy_content_no_tool() {
    let body = r#"data: {"choices":[{"index":0,"delta":{"content":"4"},"finish_reason":null}]}

data: {"choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":1,"total_tokens":11}}

data: [DONE]

"#;
    assert_eq!(
        classify_streaming_tool_call_body(body),
        StreamToolCallVerdict::NoToolCall
    );
}

#[test]
fn test_classify_streaming_tool_call_role_only_no_tool() {
    // Deltas arrived (role + finish) but no content and no tool call.
    let body = r#"data: {"choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}

data: {"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]

"#;
    assert_eq!(
        classify_streaming_tool_call_body(body),
        StreamToolCallVerdict::NoToolCall
    );
}

#[test]
fn test_classify_streaming_tool_call_empty_body_broken() {
    assert_eq!(
        classify_streaming_tool_call_body(""),
        StreamToolCallVerdict::Broken
    );
}

#[test]
fn test_classify_streaming_tool_call_done_only_broken() {
    let body = "data: [DONE]\n\n";
    assert_eq!(
        classify_streaming_tool_call_body(body),
        StreamToolCallVerdict::Broken
    );
}

#[test]
fn test_classify_streaming_tool_call_plain_json_broken() {
    // A server that ANSWERED a stream=true request with a plain JSON body
    // (even one containing tool_calls) ignored `stream` — broken path.
    let body = r#"{"id":"r","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":"","tool_calls":[{"id":"call_1","type":"function","function":{"name":"calculator","arguments":"{\"expression\":\"2+2\"}"}}]},"finish_reason":"tool_calls"}]}"#;
    assert_eq!(
        classify_streaming_tool_call_body(body),
        StreamToolCallVerdict::Broken
    );
}

#[test]
fn test_classify_streaming_tool_call_malformed_json_broken() {
    let body = "data: {not json}\n\ndata: [DONE]\n\n";
    assert_eq!(
        classify_streaming_tool_call_body(body),
        StreamToolCallVerdict::Broken
    );
}

// =========================================================================
// Streamed tool-call probe: end-to-end against real HTTP servers
// =========================================================================

/// Drain one HTTP request (headers + Content-Length body) from the socket so
/// the probe's POST is fully consumed before we answer.
async fn drain_request_body(socket: &mut tokio::net::TcpStream) {
    use tokio::io::AsyncReadExt;
    let mut buf = [0u8; 8192];
    let mut received: Vec<u8> = Vec::new();
    let mut expected: Option<usize> = None;
    loop {
        let n = socket.read(&mut buf).await.unwrap();
        if n == 0 {
            break;
        }
        received.extend_from_slice(&buf[..n]);
        if expected.is_none() {
            if let Some(headers_end) = received.windows(4).position(|window| window == b"\r\n\r\n")
            {
                let headers = String::from_utf8_lossy(&received[..headers_end]).to_lowercase();
                let length: usize = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .and_then(|value| value.trim().parse().ok())
                    .unwrap_or(0);
                expected = Some(headers_end + 4 + length);
            }
        }
        if expected.is_some_and(|total| received.len() >= total) {
            break;
        }
    }
}

#[tokio::test]
async fn test_probe_streaming_tool_call_delivered_over_sse() {
    use tokio::io::AsyncWriteExt;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        drain_request_body(&mut socket).await;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{}",
            SSE_NATIVE_TOOL_CALL
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let probe =
        probe_streaming_tool_call(&format!("http://{addr}"), "mock-model", &Config::default())
            .await;
    server.await.unwrap();

    assert_eq!(
        probe.verdict,
        StreamToolCallVerdict::Delivered,
        "streamed probe must classify a healthy SSE tool-call stream as Delivered; detail: {}",
        probe.detail
    );
}

#[tokio::test]
async fn test_probe_streaming_tool_call_broken_when_server_ignores_stream() {
    // MockLlmServer answers a stream=true request with a PLAIN JSON body
    // containing tool_calls — the streaming path did not stream → Broken.
    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_tool_calls(vec![crate::testing::mock_api::MockToolCall {
            id: "call_1".to_string(),
            name: "calculator".to_string(),
            arguments: "{\"expression\":\"2+2\"}".to_string(),
        }])
        .build()
        .await;

    let endpoint = format!("{}/v1", server.url());
    let probe = probe_streaming_tool_call(&endpoint, "mock-model", &Config::default()).await;
    server.stop().await;

    assert_eq!(
        probe.verdict,
        StreamToolCallVerdict::Broken,
        "a server that ignores `stream` must be classified Broken; detail: {}",
        probe.detail
    );
}

#[tokio::test]
async fn test_probe_streaming_tool_call_broken_on_http_error() {
    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_error(500, "{\"error\": \"internal\"}")
        .build()
        .await;

    let endpoint = format!("{}/v1", server.url());
    let probe = probe_streaming_tool_call(&endpoint, "mock-model", &Config::default()).await;
    server.stop().await;

    assert_eq!(
        probe.verdict,
        StreamToolCallVerdict::Broken,
        "an HTTP error on the streamed request must be classified Broken; detail: {}",
        probe.detail
    );
}

// =========================================================================
// /get_server_info parsing
// =========================================================================

#[test]
fn test_parse_server_info_sglang_full() {
    let body = r#"{"sglang_version":"0.4.3.post2","max_total_num_tokens":32768,"max_running_requests":8,"tool_call_parser":"qwen","reasoning_parser":"qwen3"}"#;
    let info = parse_server_info(body);
    assert_eq!(info.context_length, Some(32768));
    assert_eq!(info.max_streams, Some(8));
    assert_eq!(info.tool_call_parser.as_deref(), Some("qwen"));
}

#[test]
fn test_parse_server_info_alternative_field_names() {
    let body = r#"{"context_length":8192,"max_streams":4,"max_concurrent_requests":2}"#;
    let info = parse_server_info(body);
    assert_eq!(info.context_length, Some(8192));
    assert_eq!(info.max_streams, Some(4));
    assert_eq!(info.tool_call_parser, None);
}

#[test]
fn test_parse_server_info_tool_parser_empty() {
    let body = r#"{"sglang_version":"0.4.0","tool_call_parser":"","reasoning_parser":"qwen3"}"#;
    let info = parse_server_info(body);
    assert_eq!(info.context_length, None);
    assert_eq!(info.max_streams, None);
    assert_eq!(info.tool_call_parser.as_deref(), Some(""));
}

#[test]
fn test_parse_server_info_garbage_defaults() {
    assert_eq!(parse_server_info("not json"), ServerInfo::default());
    assert_eq!(parse_server_info("[]"), ServerInfo::default());
    assert_eq!(parse_server_info(""), ServerInfo::default());
}

// =========================================================================
// /get_server_info comparison vs configured model needs
// =========================================================================

fn capacity_names(rows: &[(String, DoctorCheckStatus, String, Option<String>)]) -> Vec<&str> {
    rows.iter().map(|(name, _, _, _)| name.as_str()).collect()
}

fn capacity_warns(rows: &[(String, DoctorCheckStatus, String, Option<String>)]) -> Vec<&str> {
    rows.iter()
        .filter(|(_, status, _, _)| *status == DoctorCheckStatus::Warning)
        .map(|(name, _, _, _)| name.as_str())
        .collect()
}

#[test]
fn test_server_capacity_warns_on_small_context_window() {
    let info = ServerInfo {
        context_length: Some(8192),
        ..ServerInfo::default()
    };
    let config = Config {
        context_length: 32768,
        ..Config::default()
    };
    let rows = server_capacity_checks(&info, &config);
    assert_eq!(
        capacity_warns(&rows),
        vec!["server context window (get_server_info)"]
    );
}

#[test]
fn test_server_capacity_silent_when_context_satisfies() {
    let info = ServerInfo {
        context_length: Some(65536),
        ..ServerInfo::default()
    };
    let config = Config {
        context_length: 32768,
        ..Config::default()
    };
    let rows = server_capacity_checks(&info, &config);
    assert!(
        !rows
            .iter()
            .any(|(_, status, _, _)| *status == DoctorCheckStatus::Warning),
        "satisfied context window must not warn: {:?}",
        rows
    );
    assert!(capacity_names(&rows).contains(&"server context window (get_server_info)"));
}

#[test]
fn test_server_capacity_warns_on_zero_streams_with_streaming_on() {
    let info = ServerInfo {
        max_streams: Some(0),
        ..ServerInfo::default()
    };
    let config = Config {
        agent: crate::config::AgentConfig {
            streaming: true,
            ..crate::config::AgentConfig::default()
        },
        ..Config::default()
    };
    let rows = server_capacity_checks(&info, &config);
    assert_eq!(
        capacity_warns(&rows),
        vec!["server stream capacity (get_server_info)"]
    );
}

#[test]
fn test_server_capacity_warns_on_low_streams_with_streaming_on() {
    // selfware default [concurrency] max_streams is 4; a server limit of 2
    // cannot serve selfware's own concurrency demand.
    let info = ServerInfo {
        max_streams: Some(2),
        ..ServerInfo::default()
    };
    let config = Config {
        agent: crate::config::AgentConfig {
            streaming: true,
            ..crate::config::AgentConfig::default()
        },
        ..Config::default()
    };
    assert_eq!(config.concurrency.max_streams, 4);
    let rows = server_capacity_checks(&info, &config);
    assert_eq!(
        capacity_warns(&rows),
        vec!["server stream capacity (get_server_info)"]
    );
}

#[test]
fn test_server_capacity_no_stream_row_when_streaming_disabled() {
    let info = ServerInfo {
        max_streams: Some(0),
        ..ServerInfo::default()
    };
    let config = Config {
        agent: crate::config::AgentConfig {
            streaming: false,
            ..crate::config::AgentConfig::default()
        },
        ..Config::default()
    };
    let rows = server_capacity_checks(&info, &config);
    assert!(
        !capacity_names(&rows).contains(&"server stream capacity (get_server_info)"),
        "streaming disabled → no stream-capacity row"
    );
    assert!(rows.is_empty());
}

#[test]
fn test_server_capacity_silent_when_stream_capacity_satisfies() {
    let info = ServerInfo {
        max_streams: Some(16),
        ..ServerInfo::default()
    };
    let config = Config {
        agent: crate::config::AgentConfig {
            streaming: true,
            ..crate::config::AgentConfig::default()
        },
        ..Config::default()
    };
    let rows = server_capacity_checks(&info, &config);
    assert!(
        !rows
            .iter()
            .any(|(_, status, _, _)| *status == DoctorCheckStatus::Warning),
        "adequate stream capacity with streaming on must not warn: {:?}",
        rows
    );
}

#[test]
fn test_server_capacity_warns_on_missing_tool_parser_with_native_fc() {
    let info = ServerInfo {
        tool_call_parser: None,
        ..ServerInfo::default()
    };
    let config = Config {
        agent: crate::config::AgentConfig {
            native_function_calling: true,
            ..crate::config::AgentConfig::default()
        },
        ..Config::default()
    };
    let rows = server_capacity_checks(&info, &config);
    assert_eq!(
        capacity_warns(&rows),
        vec!["server tool-call parser (get_server_info)"]
    );
}

#[test]
fn test_server_capacity_silent_on_tool_parser_with_native_fc() {
    let info = ServerInfo {
        tool_call_parser: Some("qwen".to_string()),
        ..ServerInfo::default()
    };
    let config = Config {
        agent: crate::config::AgentConfig {
            native_function_calling: true,
            ..crate::config::AgentConfig::default()
        },
        ..Config::default()
    };
    let rows = server_capacity_checks(&info, &config);
    assert!(
        !rows
            .iter()
            .any(|(_, status, _, _)| *status == DoctorCheckStatus::Warning),
        "server tool-call parser present → must not warn: {:?}",
        rows
    );
}

#[test]
fn test_server_capacity_silent_without_native_fc() {
    // No native FC in config → no tool-parser row at all.
    let info = ServerInfo {
        tool_call_parser: None,
        ..ServerInfo::default()
    };
    let config = Config {
        agent: crate::config::AgentConfig {
            native_function_calling: false,
            ..crate::config::AgentConfig::default()
        },
        ..Config::default()
    };
    let rows = server_capacity_checks(&info, &config);
    assert!(!capacity_names(&rows).contains(&"server tool-call parser (get_server_info)"));
    assert!(
        !rows
            .iter()
            .any(|(_, status, _, _)| *status == DoctorCheckStatus::Warning),
        "native FC off → no tool-parser row and no warnings: {:?}",
        rows
    );
}

#[test]
fn test_server_capacity_satisfied_model_stays_silent() {
    // All three capabilities satisfy the model → zero warnings.
    let info = ServerInfo {
        context_length: Some(131072),
        max_streams: Some(64),
        tool_call_parser: Some("qwen".to_string()),
    };
    let config = Config {
        context_length: 32768,
        agent: crate::config::AgentConfig {
            streaming: true,
            native_function_calling: true,
            ..crate::config::AgentConfig::default()
        },
        ..Config::default()
    };
    let rows = server_capacity_checks(&info, &config);
    assert!(
        rows.iter()
            .all(|(_, status, _, _)| *status == DoctorCheckStatus::Ok),
        "all satisfied → every row passes (no warnings), got: {:?}",
        rows
    );
}

// =========================================================================
// "disable thinking" advice gating
// =========================================================================

#[test]
fn test_thinking_disable_advice_only_on_endpoint_rejection() {
    // Endpoint accepts thinking control → NO advice (the stale-advice fix).
    assert_eq!(thinking_disable_advice(Some(true), Some(true)), None);
    assert_eq!(thinking_disable_advice(None, Some(true)), None);
    assert_eq!(thinking_disable_advice(Some(true), Some(false)), Some(
        "Endpoint rejects thinking control — disable chat_template_kwargs.enable_thinking in selfware config"
    ));
    assert_eq!(thinking_disable_advice(None, Some(false)), Some(
        "Endpoint rejects thinking control — add chat_template_kwargs.enable_thinking = false for tool-heavy Qwen requests"
    ));
    // No typed signal (probe skipped) → NO advice.
    assert_eq!(thinking_disable_advice(Some(true), None), None);
    assert_eq!(thinking_disable_advice(None, None), None);
    // Config already disabled → no advice regardless of the endpoint signal.
    assert_eq!(thinking_disable_advice(Some(false), Some(false)), None);
    assert_eq!(thinking_disable_advice(Some(false), Some(true)), None);
    assert_eq!(thinking_disable_advice(Some(false), None), None);
}
