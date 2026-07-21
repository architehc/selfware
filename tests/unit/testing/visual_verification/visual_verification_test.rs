use super::*;

// ---- Result type construction & serialization ----

#[test]
fn test_visual_verification_result_serialization() {
    let result = VisualVerificationResult {
        passed: true,
        confidence: 0.95,
        description: "A login page with two input fields".to_string(),
        issues: vec![],
    };
    let json = serde_json::to_string(&result).unwrap();
    let deserialized: VisualVerificationResult = serde_json::from_str(&json).unwrap();
    assert!(deserialized.passed);
    assert!((deserialized.confidence - 0.95).abs() < f64::EPSILON);
    assert!(deserialized.issues.is_empty());
}

#[test]
fn test_visual_verification_result_with_issues() {
    let result = VisualVerificationResult {
        passed: false,
        confidence: 0.4,
        description: "A blank white page".to_string(),
        issues: vec![
            "Expected login form not found".to_string(),
            "No input fields visible".to_string(),
        ],
    };
    let json = serde_json::to_string(&result).unwrap();
    let deserialized: VisualVerificationResult = serde_json::from_str(&json).unwrap();
    assert!(!deserialized.passed);
    assert_eq!(deserialized.issues.len(), 2);
}

#[test]
fn test_visual_diff_result_serialization() {
    let result = VisualDiffResult {
        changes_detected: true,
        expected_change_found: true,
        description: "Button color changed from gray to blue".to_string(),
        unexpected_changes: vec![],
    };
    let json = serde_json::to_string(&result).unwrap();
    let deserialized: VisualDiffResult = serde_json::from_str(&json).unwrap();
    assert!(deserialized.changes_detected);
    assert!(deserialized.expected_change_found);
    assert!(deserialized.unexpected_changes.is_empty());
}

#[test]
fn test_ui_element_serialization() {
    let element = UiElement {
        name: "Submit Button".to_string(),
        element_type: "button".to_string(),
        expected_text: Some("Submit".to_string()),
        expected_location: Some("bottom-right".to_string()),
    };
    let json = serde_json::to_string(&element).unwrap();
    let deserialized: UiElement = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, "Submit Button");
    assert_eq!(deserialized.element_type, "button");
    assert_eq!(deserialized.expected_text.as_deref(), Some("Submit"));
    assert_eq!(
        deserialized.expected_location.as_deref(),
        Some("bottom-right")
    );
}

#[test]
fn test_element_verification_serialization() {
    let ev = ElementVerification {
        element: UiElement {
            name: "Logo".to_string(),
            element_type: "image".to_string(),
            expected_text: None,
            expected_location: Some("top-left".to_string()),
        },
        found: true,
        location: Some("top-left".to_string()),
        actual_text: None,
    };
    let json = serde_json::to_string(&ev).unwrap();
    let deserialized: ElementVerification = serde_json::from_str(&json).unwrap();
    assert!(deserialized.found);
    assert_eq!(deserialized.location.as_deref(), Some("top-left"));
}

#[test]
fn test_layout_analysis_serialization() {
    let analysis = LayoutAnalysis {
        overall_quality: "good".to_string(),
        alignment_issues: vec![],
        spacing_issues: vec!["Footer too close to content".to_string()],
        responsive_notes: vec!["Sidebar collapses on narrow viewports".to_string()],
    };
    let json = serde_json::to_string(&analysis).unwrap();
    let deserialized: LayoutAnalysis = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.overall_quality, "good");
    assert!(deserialized.alignment_issues.is_empty());
    assert_eq!(deserialized.spacing_issues.len(), 1);
    assert_eq!(deserialized.responsive_notes.len(), 1);
}

// ---- Config defaults ----

#[test]
fn test_config_defaults() {
    let config = VisualVerificationConfig::default();
    assert!(!config.enabled);
    assert_eq!(config.endpoint, "http://localhost:1234/v1");
    assert_eq!(config.model, "qwen2-vl-7b");
    assert_eq!(config.timeout_secs, 120);
    assert!((config.confidence_threshold - 0.7).abs() < f64::EPSILON);
}

#[test]
fn test_config_serialization_roundtrip() {
    let config = VisualVerificationConfig {
        enabled: true,
        endpoint: "http://example.com/v1".to_string(),
        model: "gpt-4-vision".to_string(),
        timeout_secs: 60,
        confidence_threshold: 0.85,
    };
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: VisualVerificationConfig = serde_json::from_str(&json).unwrap();
    assert!(deserialized.enabled);
    assert_eq!(deserialized.model, "gpt-4-vision");
}

// ---- Prompt construction ----

#[test]
fn test_build_verify_prompt() {
    let prompt = build_verify_prompt("A page with a red button");
    assert!(prompt.contains("A page with a red button"));
    assert!(prompt.contains("passed"));
    assert!(prompt.contains("confidence"));
    assert!(prompt.contains("description"));
    assert!(prompt.contains("issues"));
    assert!(prompt.contains("16 words"));
}

#[test]
fn test_build_compare_prompt() {
    let prompt = build_compare_prompt("The header changed from blue to green");
    assert!(prompt.contains("The header changed from blue to green"));
    assert!(prompt.contains("changes_detected"));
    assert!(prompt.contains("expected_change_found"));
    assert!(prompt.contains("unexpected_changes"));
    assert!(prompt.contains("16 words"));
}

#[test]
fn test_build_elements_prompt() {
    let elements = vec![
        UiElement {
            name: "Login Button".to_string(),
            element_type: "button".to_string(),
            expected_text: Some("Log In".to_string()),
            expected_location: Some("center".to_string()),
        },
        UiElement {
            name: "Logo".to_string(),
            element_type: "image".to_string(),
            expected_text: None,
            expected_location: Some("top-left".to_string()),
        },
    ];
    let prompt = build_elements_prompt(&elements);
    assert!(prompt.contains("Login Button"));
    assert!(prompt.contains("button"));
    assert!(prompt.contains("Log In"));
    assert!(prompt.contains("center"));
    assert!(prompt.contains("Logo"));
    assert!(prompt.contains("image"));
    assert!(prompt.contains("top-left"));
}

#[test]
fn test_build_elements_prompt_empty() {
    let prompt = build_elements_prompt(&[]);
    // Should still produce a valid prompt, just no element list
    assert!(prompt.contains("JSON array"));
}

// ---- Response parsing ----

#[test]
fn test_parse_verification_response_pass() {
    let raw = r#"{"passed": true, "confidence": 0.92, "description": "Login page with form", "issues": []}"#;
    let result = parse_verification_response(raw).unwrap();
    assert!(result.passed);
    assert!((result.confidence - 0.92).abs() < f64::EPSILON);
    assert_eq!(result.description, "Login page with form");
    assert!(result.issues.is_empty());
}

#[test]
fn test_parse_verification_response_fail() {
    let raw = r#"{"passed": false, "confidence": 0.3, "description": "Empty page", "issues": ["No form found", "Missing header"]}"#;
    let result = parse_verification_response(raw).unwrap();
    assert!(!result.passed);
    assert_eq!(result.issues.len(), 2);
    assert_eq!(result.issues[0], "No form found");
}

#[test]
fn test_parse_verification_response_with_markdown_fences() {
    let raw = "Here is the result:\n```json\n{\"passed\": true, \"confidence\": 0.88, \"description\": \"OK\", \"issues\": []}\n```\nDone.";
    let result = parse_verification_response(raw).unwrap();
    assert!(result.passed);
    assert!((result.confidence - 0.88).abs() < f64::EPSILON);
}

#[test]
fn test_parse_verification_response_with_preamble() {
    let raw = "I analyzed the screenshot and here is my assessment:\n{\"passed\": false, \"confidence\": 0.5, \"description\": \"A dashboard\", \"issues\": [\"Missing sidebar\"]}";
    let result = parse_verification_response(raw).unwrap();
    assert!(!result.passed);
    assert_eq!(result.issues.len(), 1);
}

#[test]
fn test_parse_verification_response_clamps_confidence() {
    let raw = r#"{"passed": true, "confidence": 1.5, "description": "Good", "issues": []}"#;
    let result = parse_verification_response(raw).unwrap();
    assert!((result.confidence - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_parse_verification_response_missing_fields() {
    // VLM might omit some fields -- we should handle gracefully
    let raw = r#"{"passed": true}"#;
    let result = parse_verification_response(raw).unwrap();
    assert!(result.passed);
    assert!((result.confidence - 1.0).abs() < f64::EPSILON);
    assert_eq!(result.description, "");
    assert!(result.issues.is_empty());
}

#[test]
fn test_parse_verification_response_compact_schema() {
    let raw =
        r#"{"passed": true, "summary": "Help panel visible", "visible_text": ["HELP", "EXIT"]}"#;
    let result = parse_verification_response(raw).unwrap();
    assert!(result.passed);
    assert_eq!(result.description, "Help panel visible");
    assert!(result.issues.is_empty());
}

#[test]
fn test_parse_diff_response() {
    let raw = r#"{"changes_detected": true, "expected_change_found": true, "description": "Button color changed", "unexpected_changes": ["Font size also changed"]}"#;
    let result = parse_diff_response(raw).unwrap();
    assert!(result.changes_detected);
    assert!(result.expected_change_found);
    assert_eq!(result.unexpected_changes.len(), 1);
}

#[test]
fn test_parse_diff_response_no_changes() {
    let raw = r#"{"changes_detected": false, "expected_change_found": false, "description": "Images appear identical", "unexpected_changes": []}"#;
    let result = parse_diff_response(raw).unwrap();
    assert!(!result.changes_detected);
    assert!(!result.expected_change_found);
}

#[test]
fn test_parse_diff_response_compact_schema() {
    let raw = r#"{"changed": true, "change_kind": "layout", "differences": ["Panel moved", "Footer changed"]}"#;
    let result = parse_diff_response(raw).unwrap();
    assert!(result.changes_detected);
    assert!(result.expected_change_found);
    assert!(result.description.contains("layout"));
}

#[test]
fn test_parse_elements_response() {
    let elements = vec![
        UiElement {
            name: "Login".to_string(),
            element_type: "button".to_string(),
            expected_text: Some("Log In".to_string()),
            expected_location: None,
        },
        UiElement {
            name: "Logo".to_string(),
            element_type: "image".to_string(),
            expected_text: None,
            expected_location: Some("top-left".to_string()),
        },
    ];
    let raw = r#"[
            {"name": "Login", "found": true, "location": "center", "actual_text": "Log In"},
            {"name": "Logo", "found": true, "location": "top-left", "actual_text": null}
        ]"#;
    let results = parse_elements_response(raw, &elements).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results[0].found);
    assert_eq!(results[0].actual_text.as_deref(), Some("Log In"));
    assert!(results[1].found);
    assert_eq!(results[1].location.as_deref(), Some("top-left"));
}

#[test]
fn test_parse_elements_response_missing_element() {
    let elements = vec![
        UiElement {
            name: "Button".to_string(),
            element_type: "button".to_string(),
            expected_text: None,
            expected_location: None,
        },
        UiElement {
            name: "Missing".to_string(),
            element_type: "text".to_string(),
            expected_text: None,
            expected_location: None,
        },
    ];
    // VLM only returned info about "Button", not "Missing"
    let raw = r#"[{"name": "Button", "found": true, "location": "center", "actual_text": null}]"#;
    let results = parse_elements_response(raw, &elements).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results[0].found);
    assert!(!results[1].found); // Missing element marked as not found
}

#[test]
fn test_parse_layout_response() {
    let raw = r#"{"overall_quality": "fair", "alignment_issues": ["Logo off-center"], "spacing_issues": [], "responsive_notes": ["Works on mobile"]}"#;
    let result = parse_layout_response(raw).unwrap();
    assert_eq!(result.overall_quality, "fair");
    assert_eq!(result.alignment_issues.len(), 1);
    assert!(result.spacing_issues.is_empty());
    assert_eq!(result.responsive_notes.len(), 1);
}

#[test]
fn test_parse_layout_response_minimal() {
    let raw = r#"{"overall_quality": "good"}"#;
    let result = parse_layout_response(raw).unwrap();
    assert_eq!(result.overall_quality, "good");
    assert!(result.alignment_issues.is_empty());
}

// ---- JSON extraction ----

#[test]
fn test_extract_json_from_clean() {
    let raw = r#"{"key": "value"}"#;
    assert_eq!(extract_json_from_response(raw), raw);
}

#[test]
fn test_extract_json_from_markdown_fences() {
    let raw = "Some text\n```json\n{\"key\": \"value\"}\n```\nMore text";
    assert_eq!(extract_json_from_response(raw), r#"{"key": "value"}"#);
}

#[test]
fn test_extract_json_from_plain_fences() {
    let raw = "```\n{\"key\": \"value\"}\n```";
    assert_eq!(extract_json_from_response(raw), r#"{"key": "value"}"#);
}

#[test]
fn test_extract_json_with_preamble() {
    let raw = "Here is the result: {\"key\": \"value\"} and more text";
    assert_eq!(extract_json_from_response(raw), r#"{"key": "value"}"#);
}

#[test]
fn test_extract_json_array() {
    let raw = "Result: [{\"a\": 1}, {\"b\": 2}]";
    assert_eq!(extract_json_from_response(raw), r#"[{"a": 1}, {"b": 2}]"#);
}

// ---- Request body construction ----

#[test]
fn test_build_single_image_request() {
    let verifier = VisualVerifier::new("http://localhost:1234/v1", "test-model");
    let body = verifier
        .build_single_image_request("Describe this", "AAAA")
        .unwrap();
    assert_eq!(body["model"], "test-model");
    assert_eq!(body["temperature"], 0.0);
    assert_eq!(body["stream"], false);
    assert_eq!(body["messages"][0]["role"], "system");
    let content = body["messages"][1]["content"].as_array().unwrap();
    assert_eq!(content.len(), 2);
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[0]["text"], "Describe this");
    assert_eq!(content[1]["type"], "image_url");
    assert!(content[1]["image_url"]["url"]
        .as_str()
        .unwrap()
        .starts_with("data:image/png;base64,"));
    assert_eq!(content[1]["image_url"]["detail"], "low");
}

#[test]
fn test_build_two_image_request() {
    let verifier = VisualVerifier::new("http://localhost:1234/v1", "test-model");
    let body = verifier
        .build_two_image_request("Compare", "BEFORE", "AFTER")
        .unwrap();
    assert_eq!(body["messages"][0]["role"], "system");
    let content = body["messages"][1]["content"].as_array().unwrap();
    assert_eq!(content.len(), 3);
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[1]["type"], "image_url");
    assert_eq!(content[2]["type"], "image_url");
    let url1 = content[1]["image_url"]["url"].as_str().unwrap();
    let url2 = content[2]["image_url"]["url"].as_str().unwrap();
    assert!(url1.contains("BEFORE"));
    assert!(url2.contains("AFTER"));
    assert_eq!(content[1]["image_url"]["detail"], "low");
    assert_eq!(content[2]["image_url"]["detail"], "low");
}

#[test]
fn test_build_single_image_request_verification_budget() {
    let verifier = VisualVerifier::new("http://localhost:1234/v1", "test-model");
    let body = verifier
        .build_single_image_request_with_options("Verify", "AAAA", VERIFICATION_MAX_TOKENS)
        .unwrap();
    assert_eq!(body["max_tokens"], VERIFICATION_MAX_TOKENS);
}

#[test]
fn test_build_two_image_request_diff_budget() {
    let verifier = VisualVerifier::new("http://localhost:1234/v1", "test-model");
    let body = verifier
        .build_two_image_request_with_options("Compare", "BEFORE", "AFTER", DIFF_MAX_TOKENS)
        .unwrap();
    assert_eq!(body["max_tokens"], DIFF_MAX_TOKENS);
}

#[test]
fn test_build_single_image_request_merges_runtime_overrides() {
    let mut extra_body = serde_json::Map::new();
    extra_body.insert(
        "chat_template_kwargs".to_string(),
        json!({ "enable_thinking": false }),
    );
    let verifier = VisualVerifier::new("http://localhost:1234/v1", "test-model")
        .with_generation(256, 0.25)
        .with_image_detail("high")
        .with_extra_body(Some(extra_body));
    let body = verifier
        .build_single_image_request_with_options("Verify", "AAAA", 4096)
        .unwrap();
    assert_eq!(body["max_tokens"], 256);
    assert_eq!(body["temperature"], 0.25);
    assert_eq!(
        body["messages"][1]["content"][1]["image_url"]["detail"],
        "high"
    );
    assert_eq!(
        body["chat_template_kwargs"]["enable_thinking"],
        json!(false)
    );
}

// ---- Verifier construction ----

#[test]
fn test_verifier_new() {
    let v = VisualVerifier::new("http://example.com/v1", "model-x");
    assert_eq!(v.endpoint, "http://example.com/v1");
    assert_eq!(v.model, "model-x");
    assert!(v.api_key.is_none());
    assert_eq!(v.timeout_secs, 120);
    assert_eq!(v.default_max_tokens, 4096);
    assert_eq!(v.temperature, 0.0);
    assert_eq!(v.image_detail, "low");
    assert!(v.extra_body.is_none());
}

#[test]
fn test_verifier_from_config() {
    let config = VisualVerificationConfig {
        enabled: true,
        endpoint: "http://myhost:5000/v1".to_string(),
        model: "llava".to_string(),
        timeout_secs: 30,
        confidence_threshold: 0.9,
    };
    let v = VisualVerifier::from_config(&config);
    assert_eq!(v.endpoint, "http://myhost:5000/v1");
    assert_eq!(v.model, "llava");
    assert_eq!(v.timeout_secs, 30);
    assert_eq!(v.default_max_tokens, 4096);
    assert_eq!(v.temperature, 0.0);
}

#[test]
fn test_verifier_with_timeout() {
    let v = VisualVerifier::new("http://localhost/v1", "m").with_timeout(45);
    assert_eq!(v.timeout_secs, 45);
}

#[test]
fn test_verifier_from_model_profile() {
    let mut extra_body = serde_json::Map::new();
    extra_body.insert(
        "chat_template_kwargs".to_string(),
        json!({ "enable_thinking": false }),
    );
    let profile = crate::config::ModelProfile {
        endpoint: "https://vision.example/v1".to_string(),
        model: "vision-model".to_string(),
        api_key: Some(crate::config::RedactedString::new("vision-secret")),
        max_tokens: 192,
        temperature: 0.0,
        modalities: vec!["text".to_string(), "vision".to_string()],
        context_length: 262_144,
        extra_body: Some(extra_body.clone()),
        native_function_calling: None,
    };
    let v = VisualVerifier::from_model_profile(&profile);
    assert_eq!(v.endpoint, "https://vision.example/v1");
    assert_eq!(v.model, "vision-model");
    assert_eq!(v.api_key.as_ref().unwrap().expose(), "vision-secret");
    assert_eq!(v.default_max_tokens, 192);
    assert_eq!(v.temperature, 0.0);
    assert_eq!(
        v.extra_body
            .as_ref()
            .and_then(|m| m.get("chat_template_kwargs")),
        extra_body.get("chat_template_kwargs")
    );
}

#[test]
fn test_verifier_from_app_config_prefers_vision_profile() {
    let mut config = crate::config::Config {
        endpoint: "http://localhost:8000/v1".to_string(),
        model: "default-text".to_string(),
        max_tokens: 8192,
        temperature: 0.2,
        ..Default::default()
    };
    config.agent.step_timeout_secs = 45;

    config.models.insert(
        "default".to_string(),
        crate::config::ModelProfile {
            endpoint: "http://localhost:8000/v1".to_string(),
            model: "default-text".to_string(),
            api_key: None,
            max_tokens: 8192,
            temperature: 0.2,
            modalities: vec!["text".to_string()],
            context_length: 1_000_000,
            extra_body: None,
            native_function_calling: None,
        },
    );
    config.models.insert(
        "vision".to_string(),
        crate::config::ModelProfile {
            endpoint: "https://vision.example/v1".to_string(),
            model: "remote-vision".to_string(),
            api_key: None,
            max_tokens: 192,
            temperature: 0.0,
            modalities: vec!["text".to_string(), "vision".to_string()],
            context_length: 262_144,
            extra_body: Some({
                let mut map = serde_json::Map::new();
                map.insert(
                    "chat_template_kwargs".to_string(),
                    json!({ "enable_thinking": false }),
                );
                map
            }),
            native_function_calling: None,
        },
    );

    let v = VisualVerifier::from_app_config(&config);
    assert_eq!(v.endpoint, "https://vision.example/v1");
    assert_eq!(v.model, "remote-vision");
    assert_eq!(v.default_max_tokens, 192);
    assert_eq!(v.timeout_secs, 45);
    assert_eq!(v.temperature, 0.0);
    assert_eq!(
        v.extra_body
            .as_ref()
            .and_then(|m| m.get("chat_template_kwargs"))
            .and_then(|v| v.get("enable_thinking")),
        Some(&json!(false))
    );
}

// ---- Invalid JSON handling ----

#[test]
fn test_parse_verification_response_invalid_json() {
    let raw = "This is not JSON at all";
    assert!(parse_verification_response(raw).is_err());
}

#[test]
fn test_parse_diff_response_invalid_json() {
    let raw = "Not valid";
    assert!(parse_diff_response(raw).is_err());
}

#[test]
fn test_parse_elements_response_not_array() {
    let elements = vec![];
    let raw = r#"{"not": "an array"}"#;
    assert!(parse_elements_response(raw, &elements).is_err());
}

#[test]
fn test_parse_layout_response_invalid() {
    let raw = "garbage";
    assert!(parse_layout_response(raw).is_err());
}

// ---- Visual verifier edge cases ----

#[test]
fn test_visual_verifier_handles_invalid_image() {
    // Garbage base64 should still produce a valid request body without panicking.
    // The actual VLM call would fail, but construction must be graceful.
    let verifier = VisualVerifier::new("http://localhost:1234/v1", "test-model");
    let garbage = "not-valid-base64-!@#$%^&*()";
    let body = verifier
        .build_single_image_request("Check this", garbage)
        .unwrap();
    // The body is constructed; the data URI contains the garbage verbatim
    let url = body["messages"][1]["content"][1]["image_url"]["url"]
        .as_str()
        .unwrap();
    assert!(url.starts_with("data:image/png;base64,"));
    assert!(url.contains(garbage));

    // Parsing a verification response that would come from invalid image
    // input should also be handled — the VLM would return an error string,
    // which fails JSON parsing gracefully.
    let bad_response = "Error: invalid image data";
    assert!(parse_verification_response(bad_response).is_err());
}

#[test]
fn test_visual_verifier_handles_empty_expectation() {
    // An empty expected description should not panic during prompt construction.
    let prompt = build_verify_prompt("");
    assert!(prompt.contains("EXPECTED:"));
    // The prompt still contains the structural JSON instructions
    assert!(prompt.contains("passed"));
    assert!(prompt.contains("confidence"));

    // Parsing a response for an empty-expectation query should work normally
    let raw = r#"{"passed": true, "confidence": 0.5, "description": "Blank screen", "issues": []}"#;
    let result = parse_verification_response(raw).unwrap();
    assert!(result.passed);
}

#[test]
fn test_visual_verification_result_serde_roundtrip() {
    let original = VisualVerificationResult {
        passed: false,
        confidence: 0.73,
        description: "Dashboard with charts".to_string(),
        issues: vec![
            "Missing legend".to_string(),
            "Colors too similar".to_string(),
        ],
    };
    let json = serde_json::to_string(&original).unwrap();
    let roundtripped: VisualVerificationResult = serde_json::from_str(&json).unwrap();
    assert_eq!(roundtripped.passed, original.passed);
    assert!((roundtripped.confidence - original.confidence).abs() < f64::EPSILON);
    assert_eq!(roundtripped.description, original.description);
    assert_eq!(roundtripped.issues, original.issues);

    // Also test via serde_json::Value to confirm field names
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(value.get("passed").is_some());
    assert!(value.get("confidence").is_some());
    assert!(value.get("description").is_some());
    assert!(value.get("issues").is_some());
}

// ---- Visual Stuck-Loop Detection Tests ----

#[test]
fn test_visual_state_tracker_new() {
    let tracker = VisualStateTracker::new(20, 2);
    assert_eq!(tracker.history_size(), 0);
}

#[test]
fn test_visual_state_tracker_default_config() {
    let tracker = VisualStateTracker::default_config();
    assert_eq!(tracker.history_size(), 0);
}

#[test]
fn test_visual_state_tracker_record_state_proceed() {
    let mut tracker = VisualStateTracker::new(10, 2);

    // First state - should proceed
    let result = tracker.record_state_with_hash(
        "abc123".to_string(),
        "Login page".to_string(),
        "click".to_string(),
        false,
    );

    match result {
        LoopDetectionResult::Proceed => {}
        _ => panic!("Expected Proceed for first state"),
    }
    assert_eq!(tracker.history_size(), 1);
}

#[test]
fn test_visual_state_tracker_detects_stuck_loop() {
    let mut tracker = VisualStateTracker::new(10, 2);

    // Record same hash + action + failed three times
    for _ in 0..2 {
        tracker.record_state_with_hash(
            "abc123".to_string(),
            "Login page".to_string(),
            "click".to_string(),
            false,
        );
    }

    // Third time should trigger stuck detection
    let result = tracker.record_state_with_hash(
        "abc123".to_string(),
        "Login page".to_string(),
        "click".to_string(),
        false,
    );

    match result {
        LoopDetectionResult::Stuck {
            loop_pattern,
            suggested_recovery,
        } => {
            assert_eq!(loop_pattern.len(), 2);
            // Should suggest recovery strategy
            if let RecoveryStrategy::TryDifferentAction { alternatives } = suggested_recovery {
                assert!(!alternatives.is_empty());
            }
        }
        _ => panic!("Expected Stuck result, got {:?}", result),
    }
}

#[test]
fn test_visual_state_tracker_different_hashes_proceed() {
    let mut tracker = VisualStateTracker::new(10, 2);

    // Record clearly different hashes - should never get stuck
    // Use hex strings that are very different from each other
    let hashes = vec![
        "0000000000000000",
        "ffffffffffffffff",
        "aaaaaaaaaaaaaaaa",
        "5555555555555555",
        "123456789abcdef0",
    ];

    for hash in hashes {
        let result = tracker.record_state_with_hash(
            hash.to_string(),
            "Different page".to_string(),
            "click".to_string(),
            false,
        );

        match result {
            LoopDetectionResult::Proceed => {}
            _ => panic!("Expected Proceed for different hashes, got {:?}", result),
        }
    }
}

#[test]
fn test_visual_state_tracker_success_does_not_stick() {
    let mut tracker = VisualStateTracker::new(10, 2);

    // Same hash but action succeeded - should not count as stuck
    for _ in 0..5 {
        let result = tracker.record_state_with_hash(
            "abc123".to_string(),
            "Login page".to_string(),
            "click".to_string(),
            true, // succeeded
        );

        match result {
            LoopDetectionResult::Proceed => {}
            _ => panic!("Successful actions should not trigger stuck detection"),
        }
    }
}

#[test]
fn test_hash_similarity_identical() {
    let hash1 = "abcdef123456";
    let hash2 = "abcdef123456";
    let sim = compute_hash_similarity(hash1, hash2);
    assert!((sim - 1.0).abs() < f32::EPSILON);
}

#[test]
fn test_hash_similarity_completely_different() {
    let hash1 = "00000000";
    let hash2 = "ffffffff";
    let sim = compute_hash_similarity(hash1, hash2);
    assert!(sim < 0.5); // Should be low similarity
}

#[test]
fn test_hash_similarity_one_bit_different() {
    // Two hashes that differ by one nibble
    let hash1 = "00000000";
    let hash2 = "10000000";
    let sim = compute_hash_similarity(hash1, hash2);
    assert!(sim > 0.9); // Should still be high similarity
    assert!(sim < 1.0); // But not identical
}

#[test]
fn test_recovery_strategy_display() {
    let strategy = RecoveryStrategy::WaitAndRetry { delay_ms: 1000 };
    let display = format!("{}", strategy);
    assert!(display.contains("Wait"));
    assert!(display.contains("1000ms"));

    let strategy = RecoveryStrategy::ResetToCheckpoint;
    let display = format!("{}", strategy);
    assert!(display.contains("Reset"));

    let strategy = RecoveryStrategy::TryDifferentAction {
        alternatives: vec!["alt1".to_string(), "alt2".to_string()],
    };
    let display = format!("{}", strategy);
    assert!(display.contains("alt1"));
    assert!(display.contains("alt2"));
}

#[test]
fn test_visual_loop_config_default() {
    let config = VisualLoopConfig::default();
    assert_eq!(config.max_history, 20);
    assert_eq!(config.stuck_threshold, 2);
    assert!(config.auto_recovery);
}

#[test]
fn test_visual_loop_config_create_tracker() {
    let config = VisualLoopConfig::default();
    let tracker = config.create_tracker();
    assert_eq!(tracker.history_size(), 0);
}

#[test]
fn test_screenshot_state_serialization() {
    use chrono::Utc;
    let state = ScreenshotState {
        hash: "abc123".to_string(),
        semantic_description: "Login page".to_string(),
        timestamp: Utc::now(),
        action_taken: "click".to_string(),
        action_succeeded: false,
        screenshot_path: Some(std::path::PathBuf::from("/tmp/test.png")),
    };

    let json = serde_json::to_string(&state).unwrap();
    assert!(json.contains("abc123"));
    assert!(json.contains("Login page"));
    assert!(json.contains("click"));
}

#[test]
fn test_visual_state_tracker_clear_history() {
    let mut tracker = VisualStateTracker::new(10, 2);

    tracker.record_state_with_hash(
        "abc123".to_string(),
        "Login page".to_string(),
        "click".to_string(),
        false,
    );

    assert_eq!(tracker.history_size(), 1);
    tracker.clear_history();
    assert_eq!(tracker.history_size(), 0);
}

#[test]
fn test_visual_state_tracker_has_similar_state() {
    let mut tracker = VisualStateTracker::new(10, 2);

    tracker.record_state_with_hash(
        "abc123".to_string(),
        "Login page".to_string(),
        "click".to_string(),
        false,
    );

    // Should find similar state
    assert!(tracker.has_similar_state("abc123", "click"));

    // Different hash - should not match
    assert!(!tracker.has_similar_state("xyz789", "click"));

    // Different action - should not match
    assert!(!tracker.has_similar_state("abc123", "type"));
}

#[test]
fn test_visual_state_tracker_history_limit() {
    let mut tracker = VisualStateTracker::new(3, 2);

    // Record more states than max_history
    for i in 0..5 {
        tracker.record_state_with_hash(
            format!("hash{}", i),
            format!("Page {}", i),
            "click".to_string(),
            false,
        );
    }

    // History should be capped at max_history
    assert_eq!(tracker.history_size(), 3);
}
