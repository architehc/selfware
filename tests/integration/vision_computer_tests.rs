//! Integration tests for the vision + computer control + SWE-bench pipeline.
//!
//! These tests exercise the full chain: screen capture → vision analysis → computer
//! control → visual feedback loop, using the local Qwen3.5-27B endpoint.
//!
//! Run with:
//!   SELFWARE_ENDPOINT=http://localhost:8000/v1 SELFWARE_MODEL=qwen3.5-27b \
//!     cargo test --features integration vision_computer
//!
//! The tests gracefully skip when the model endpoint is unreachable.

use selfware::agent::Agent;
use selfware::config::{AgentConfig, Config, ExecutionMode, ModelProfile, SafetyConfig};
use selfware::tools::computer::{ComputerKeyboardTool, ComputerMouseTool, ComputerWindowTool};
use selfware::tools::vision::{VisionAnalyze, VisionCompare};
use selfware::tools::Tool;
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::helpers::require_llm_endpoint_url;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn vision_endpoint() -> String {
    std::env::var("SELFWARE_ENDPOINT").unwrap_or_else(|_| "http://localhost:8000/v1".to_string())
}

fn vision_model() -> String {
    std::env::var("SELFWARE_MODEL").unwrap_or_else(|_| "qwen3.5-27b".to_string())
}

fn vision_dispatch_opt_in() -> bool {
    std::env::var("SELFWARE_RUN_LIVE_VISION_DISPATCH")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn vision_dispatch_endpoint() -> String {
    std::env::var("SELFWARE_VISION_ENDPOINT").unwrap_or_else(|_| vision_endpoint())
}

fn vision_dispatch_model() -> String {
    std::env::var("SELFWARE_VISION_MODEL").unwrap_or_else(|_| vision_model())
}

/// Spawn a mock OpenAI-compatible /chat/completions endpoint that returns
/// a canned response.
async fn spawn_mock_vision_server(response_content: &str) -> (tokio::task::JoinHandle<()>, String) {
    let body = serde_json::json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": response_content
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 100,
            "completion_tokens": 50,
            "total_tokens": 150
        }
    });
    let body_str = body.to_string();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let body = body_str.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 8192];
                let _ = stream.read(&mut buf).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });

    (handle, format!("http://127.0.0.1:{}", addr.port()))
}

/// Spawn a mock OpenAI-compatible chat server that returns queued JSON bodies
/// for successive /v1/chat/completions requests.
async fn spawn_mock_chat_sequence_server(
    responses: Vec<String>,
) -> (tokio::task::JoinHandle<()>, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let responses = Arc::new(responses);
    let response_idx = Arc::new(AtomicUsize::new(0));

    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let responses = Arc::clone(&responses);
            let response_idx = Arc::clone(&response_idx);

            tokio::spawn(async move {
                let mut buf = [0u8; 8192];
                let _ = stream.read(&mut buf).await;

                let idx = response_idx.fetch_add(1, Ordering::SeqCst);
                let body = responses
                    .get(idx)
                    .or_else(|| responses.last())
                    .cloned()
                    .unwrap_or_else(|| {
                        json!({
                            "id": "chatcmpl-fallback",
                            "object": "chat.completion",
                            "choices": [{
                                "index": 0,
                                "message": {
                                    "role": "assistant",
                                    "content": "done"
                                },
                                "finish_reason": "stop"
                            }],
                            "usage": {
                                "prompt_tokens": 1,
                                "completion_tokens": 1,
                                "total_tokens": 2
                            }
                        })
                        .to_string()
                    });

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });

    (handle, format!("http://127.0.0.1:{}", addr.port()))
}

/// Create a minimal valid PNG in a temp file and return the path.
/// Uses .png suffix so image::open can detect the format.
fn create_test_png() -> tempfile::NamedTempFile {
    let tmp = tempfile::Builder::new().suffix(".png").tempfile().unwrap();
    // Minimal 1x1 red PNG
    let png_bytes: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8,
        0xCF, 0xC0, 0xF0, 0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00,
        0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    std::fs::write(tmp.path(), png_bytes).unwrap();
    tmp
}

// ===========================================================================
// Unit-level integration: chaining components without a live LLM
// ===========================================================================

/// Test that mouse + keyboard tools can be chained sequentially.
#[tokio::test]
async fn test_computer_control_chain() {
    let mouse = ComputerMouseTool;
    let keyboard = ComputerKeyboardTool;

    // Move mouse
    let r = mouse
        .execute(json!({"action": "move_to", "x": 500, "y": 300}))
        .await
        .unwrap();
    assert_eq!(r["status"], "ok");

    // Click
    let r = mouse
        .execute(json!({"action": "click", "x": 500, "y": 300}))
        .await
        .unwrap();
    assert_eq!(r["status"], "ok");

    // Type text
    let r = keyboard
        .execute(json!({"action": "type", "text": "ls -la"}))
        .await
        .unwrap();
    assert_eq!(r["status"], "ok");

    // Press Enter
    let r = keyboard
        .execute(json!({"action": "press", "key": "Enter"}))
        .await
        .unwrap();
    assert_eq!(r["status"], "ok");
}

/// Test that window management tools work in sequence.
///
/// Requires `wmctrl` or `xdotool` and a real display server, so it skips
/// gracefully on headless WSL/CI.
#[tokio::test]
async fn test_window_management_chain() {
    fn binary_available(name: &str) -> bool {
        std::process::Command::new("sh")
            .args(["-c", &format!("command -v {name}")])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    if !binary_available("wmctrl") && !binary_available("xdotool") {
        eprintln!("SKIPPED: test_window_management_chain — no wmctrl/xdotool on PATH");
        return;
    }

    let window = ComputerWindowTool;

    let r = window.execute(json!({"action": "list"})).await.unwrap();
    assert_eq!(r["status"], "ok");
    let windows = r["windows"].as_array().expect("windows array");
    if windows.is_empty() {
        eprintln!("SKIPPED: test_window_management_chain — no windows visible to the WM");
        return;
    }

    let r = window
        .execute(json!({"action": "focus", "window_id": 0}))
        .await
        .unwrap();
    assert_eq!(r["status"], "ok");
}

/// Test the vision_analyze tool with a mock server (no real LLM needed).
#[tokio::test]
async fn test_vision_analyze_with_mock() {
    let (handle, endpoint) =
        spawn_mock_vision_server("This is a screenshot of a terminal with code.").await;

    let tool = VisionAnalyze;
    let result = tool
        .execute(json!({
            "prompt": "Describe what you see",
            "endpoint": endpoint,
            "model": "mock-model",
            "image_base64": "iVBORw0KGgo=",
            "detail": "low",
            "max_tokens": 100
        }))
        .await
        .unwrap();

    assert_eq!(result["success"], true);
    assert!(result["analysis"].as_str().unwrap().contains("terminal"));
    assert!(result["usage"]["total_tokens"].as_u64().unwrap() > 0);

    handle.abort();
}

/// Test vision_compare with actual image files (pixel comparison, no LLM).
#[tokio::test]
async fn test_vision_compare_identical_images() {
    let img = create_test_png();
    let path = img.path().to_str().unwrap();

    let tool = VisionCompare;
    let result = tool
        .execute(json!({
            "image_a": path,
            "image_b": path,
            "threshold": 90.0
        }))
        .await
        .unwrap();

    assert_eq!(result["success"], true);
    assert_eq!(result["passed"], true);
    assert!((result["pixel_similarity"].as_f64().unwrap() - 100.0).abs() < 0.01);
    assert_eq!(result["dimensions_matched"], true);
}

/// Test vision_compare with semantic comparison via mock server.
#[tokio::test]
async fn test_vision_compare_with_semantic_mock() {
    let (handle, endpoint) = spawn_mock_vision_server("The images are identical.").await;

    let img = create_test_png();
    let path = img.path().to_str().unwrap();

    let tool = VisionCompare;
    let result = tool
        .execute(json!({
            "image_a": path,
            "image_b": path,
            "threshold": 90.0,
            "endpoint": endpoint,
            "model": "mock-model"
        }))
        .await
        .unwrap();

    assert_eq!(result["success"], true);
    assert!(result["semantic_comparison"]
        .as_str()
        .unwrap()
        .contains("identical"));

    handle.abort();
}

/// Chain: mouse control → keyboard → then verify via mock vision analysis.
#[tokio::test]
async fn test_control_then_analyze_chain() {
    let (handle, endpoint) = spawn_mock_vision_server(
        r#"{"composition":85,"hierarchy":80,"readability":90,"consistency":88,"accessibility":82,"overall":85,"suggestions":["Increase font size"]}"#
    ).await;

    // Step 1: Simulate computer interaction
    let mouse = ComputerMouseTool;
    let keyboard = ComputerKeyboardTool;

    mouse
        .execute(json!({"action": "click", "x": 100, "y": 100}))
        .await
        .unwrap();
    keyboard
        .execute(json!({"action": "type", "text": "vim main.py"}))
        .await
        .unwrap();
    keyboard
        .execute(json!({"action": "press", "key": "Enter"}))
        .await
        .unwrap();

    // Step 2: Analyze the "screen" via vision
    let vision = VisionAnalyze;
    let result = vision
        .execute(json!({
            "prompt": "Evaluate this code editor view",
            "endpoint": endpoint,
            "model": "mock-model",
            "image_base64": "iVBORw0KGgo="
        }))
        .await
        .unwrap();

    // Step 3: Parse the response as a VisualScore
    let analysis = result["analysis"].as_str().unwrap();
    let score: selfware::visual_loop::VisualScore = serde_json::from_str(analysis).unwrap();
    assert!(score.overall > 80.0);
    assert_eq!(score.suggestions.len(), 1);

    handle.abort();
}

/// Test the full visual feedback loop build_critic_prompt → mock VLM → parse chain.
#[tokio::test]
async fn test_visual_feedback_loop_chain() {
    use selfware::visual_loop::*;

    let (handle, endpoint) = spawn_mock_vision_server(
        r#"{"composition":92,"hierarchy":88,"readability":95,"consistency":90,"accessibility":85,"overall":90,"suggestions":["Minor: increase button padding"]}"#
    ).await;

    let config = VisualFeedbackLoop {
        max_iterations: 3,
        quality_threshold: 0.85,
        vision_model_id: "qwen3.5-27b".to_string(),
        capture_method: CaptureMethod::Screen,
    };

    // Simulate one iteration of the loop
    let mut score_history: Vec<VisualScore> = Vec::new();

    for i in 0..2 {
        let previous = score_history.last();
        let prompt = build_critic_prompt("Build a dashboard UI", previous, i);
        assert!(prompt.contains(&format!("iteration {}", i + 1)));

        // Call mock VLM
        let vision = VisionAnalyze;
        let result = vision
            .execute(json!({
                "prompt": prompt,
                "endpoint": &endpoint,
                "model": "mock-model",
                "image_base64": "iVBORw0KGgo="
            }))
            .await
            .unwrap();

        let response_text = result["analysis"].as_str().unwrap();
        let mut score = parse_critic_response(response_text).unwrap();
        score.compute_overall();

        score_history.push(score.clone());

        // Check threshold
        if score.overall / 100.0 >= config.quality_threshold {
            break;
        }
    }

    assert!(!score_history.is_empty());
    let final_score = score_history.last().unwrap();
    assert!(final_score.overall > 80.0);

    let loop_result = VisualLoopResult {
        iterations: score_history.len(),
        threshold_met: final_score.overall / 100.0 >= config.quality_threshold,
        score_history: score_history.clone(),
        final_score: final_score.clone(),
    };
    assert!(loop_result.threshold_met);

    handle.abort();
}

// ===========================================================================
// Live LLM integration tests (require local Qwen3.5-27B at localhost:8000)
// ===========================================================================

/// Test vision_analyze with the real local Qwen3.5-27B endpoint.
#[tokio::test]
async fn test_live_vision_analyze() {
    let endpoint = vision_endpoint();
    if !require_llm_endpoint_url(&endpoint).await {
        eprintln!(
            "SKIPPED: test_live_vision_analyze - endpoint not available at {}",
            endpoint
        );
        return;
    }

    let tool = VisionAnalyze;
    let png = create_test_png();
    let b64 = selfware::tools::vision::encode_image_file(png.path().to_str().unwrap()).unwrap();

    let result = tool
        .execute(json!({
            "prompt": "What do you see in this image? Describe the color and size.",
            "endpoint": endpoint,
            "model": vision_model(),
            "image_base64": b64,
            "detail": "low",
            "max_tokens": 256
        }))
        .await;

    match result {
        Ok(val) => {
            assert_eq!(val["success"], true);
            let analysis = val["analysis"].as_str().unwrap_or("");
            // Model may return empty for very small images — log but don't fail
            if analysis.is_empty() {
                eprintln!(
                    "Note: Vision model returned empty analysis for 1x1 test image (acceptable)"
                );
            } else {
                println!("Live vision analysis: {}", analysis);
            }
        }
        Err(e) => {
            // Some vLLM configs may not support vision — skip gracefully
            eprintln!(
                "SKIPPED: vision endpoint error (may not support vision): {}",
                e
            );
        }
    }
}

/// End-to-end: computer control → mock screen capture → live vision analysis.
#[tokio::test]
async fn test_live_control_capture_analyze_chain() {
    let endpoint = vision_endpoint();
    if !require_llm_endpoint_url(&endpoint).await {
        eprintln!("SKIPPED: test_live_control_capture_analyze_chain - endpoint not available");
        return;
    }

    // Step 1: Computer control actions
    let mouse = ComputerMouseTool;
    let keyboard = ComputerKeyboardTool;
    mouse
        .execute(json!({"action": "move_to", "x": 500, "y": 500}))
        .await
        .unwrap();
    keyboard
        .execute(json!({"action": "type", "text": "echo hello"}))
        .await
        .unwrap();

    // Step 2: Use a test PNG as our "screenshot"
    let png = create_test_png();
    let b64 = selfware::tools::vision::encode_image_file(png.path().to_str().unwrap()).unwrap();

    // Step 3: Send to live vision model
    let vision = VisionAnalyze;
    let result = vision
        .execute(json!({
            "prompt": "Describe the content of this screenshot. Is there any text visible?",
            "endpoint": endpoint,
            "model": vision_model(),
            "image_base64": b64,
            "max_tokens": 256
        }))
        .await;

    match result {
        Ok(val) => {
            assert_eq!(val["success"], true);
            println!(
                "Live chain analysis: {}",
                val["analysis"].as_str().unwrap_or("(empty)")
            );
        }
        Err(e) => {
            eprintln!("SKIPPED: vision chain error: {}", e);
        }
    }
}

/// End-to-end regression for `models.vision` injection through tool dispatch.
///
/// Uses a mock text model to force a deterministic `vision_analyze` tool call
/// without endpoint/model arguments, while the actual vision request goes to
/// the real vision endpoint configured via `models.vision`.
#[tokio::test]
async fn test_live_tool_dispatch_uses_models_vision_profile() {
    if !vision_dispatch_opt_in() {
        eprintln!(
            "SKIPPED: set SELFWARE_RUN_LIVE_VISION_DISPATCH=1 to run live vision dispatch regression"
        );
        return;
    }

    let endpoint = vision_dispatch_endpoint();
    if !require_llm_endpoint_url(&endpoint).await {
        eprintln!(
            "SKIPPED: live vision dispatch endpoint not available at {}",
            endpoint
        );
        return;
    }

    let png = create_test_png();
    let image_base64 = selfware::tools::vision::encode_image_file(png.path().to_str().unwrap())
        .expect("test png should encode");
    let tool_args = json!({
        "prompt": "Describe the dominant color in this tiny test image in one short sentence.",
        "image_base64": image_base64,
        "detail": "low",
        "max_tokens": 192
    })
    .to_string();

    let tool_call_response = json!({
        "id": "chatcmpl-tool",
        "object": "chat.completion",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_vision_dispatch",
                    "type": "function",
                    "function": {
                        "name": "vision_analyze",
                        "arguments": tool_args
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 5,
            "total_tokens": 15
        }
    })
    .to_string();
    let final_response = json!({
        "id": "chatcmpl-final",
        "object": "chat.completion",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "done"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 5,
            "completion_tokens": 1,
            "total_tokens": 6
        }
    })
    .to_string();

    let (mock_handle, mock_url) =
        spawn_mock_chat_sequence_server(vec![tool_call_response, final_response]).await;

    let mut config = Config {
        endpoint: format!("{}/v1", mock_url),
        model: "mock-text-model".to_string(),
        max_tokens: 1024,
        temperature: 0.0,
        safety: SafetyConfig {
            allowed_paths: vec!["./**".to_string(), "/tmp/**".to_string()],
            ..Default::default()
        },
        agent: AgentConfig {
            max_iterations: 4,
            step_timeout_secs: 90,
            stream_stall_timeout_secs: None,
            token_budget: 4096,
            native_function_calling: true,
            streaming: false,
            min_completion_steps: 0,
            require_verification_before_completion: false,
            ..Default::default()
        },
        execution_mode: ExecutionMode::Yolo,
        ..Default::default()
    };
    config.models.insert(
        "vision".to_string(),
        ModelProfile {
            endpoint: endpoint.clone(),
            model: vision_dispatch_model(),
            api_key: None,
            max_tokens: 192,
            temperature: 0.0,
            modalities: vec!["text".to_string(), "vision".to_string()],
            context_length: 262_144,
            extra_body: Some({
                let mut extra = serde_json::Map::new();
                extra.insert(
                    "chat_template_kwargs".to_string(),
                    json!({ "enable_thinking": false }),
                );
                extra
            }),
            native_function_calling: None,
            max_retries: None,
            response_timeout_floor_secs: None,
        },
    );

    let mut agent = Agent::new(config).await.expect("agent should initialize");
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        agent.run_task("Use the vision tool on the provided image."),
    )
    .await;

    match result {
        Ok(Ok(())) => {}
        Ok(Err(err)) => panic!("live vision dispatch task failed: {err}"),
        Err(_) => panic!("live vision dispatch task timed out"),
    }

    let tool_result = agent
        .retrieve_last_tool_output()
        .map(|output| output.full_output)
        .unwrap_or_default();

    assert!(
        tool_result.contains("\"success\":true"),
        "expected successful tool result, got: {}",
        tool_result
    );
    assert!(
        tool_result.contains(&vision_dispatch_model()),
        "expected injected vision model in tool result, got: {}",
        tool_result
    );

    mock_handle.abort();
}

/// Live visual feedback loop with the real endpoint: build prompt → analyze → score.
#[tokio::test]
async fn test_live_visual_feedback_loop() {
    use selfware::visual_loop::*;

    let endpoint = vision_endpoint();
    if !require_llm_endpoint_url(&endpoint).await {
        eprintln!("SKIPPED: test_live_visual_feedback_loop - endpoint not available");
        return;
    }

    let png = create_test_png();
    let b64 = selfware::tools::vision::encode_image_file(png.path().to_str().unwrap()).unwrap();

    let prompt = build_critic_prompt(
        "Evaluate a minimal red pixel image for visual quality",
        None,
        0,
    );

    let vision = VisionAnalyze;
    let result = vision.execute(json!({
        "prompt": format!("{}\n\nRespond with ONLY a JSON object with fields: composition, hierarchy, readability, consistency, accessibility, overall (all 0-100), and suggestions (array of strings).", prompt),
        "endpoint": endpoint,
        "model": vision_model(),
        "image_base64": b64,
        "max_tokens": 512
    })).await;

    match result {
        Ok(val) => {
            let analysis = val["analysis"].as_str().unwrap_or("");
            println!("Live VLM critic response: {}", analysis);

            // Try to parse as VisualScore
            match parse_critic_response(analysis) {
                Ok(mut score) => {
                    score.compute_overall();
                    println!("Parsed score — overall: {:.1}", score.overall);
                    assert!(score.overall >= 0.0 && score.overall <= 100.0);
                }
                Err(e) => {
                    // Model may not return perfect JSON — that's OK for a live test
                    eprintln!("Note: Could not parse VLM response as VisualScore: {}", e);
                    eprintln!("Raw response: {}", analysis);
                }
            }
        }
        Err(e) => {
            eprintln!("SKIPPED: vision feedback loop error: {}", e);
        }
    }
}
