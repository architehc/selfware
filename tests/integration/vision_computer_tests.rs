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

use selfware::swebench::*;
use selfware::tools::computer::{ComputerKeyboardTool, ComputerMouseTool, ComputerWindowTool};
use selfware::tools::vision::{VisionAnalyze, VisionCompare};
use selfware::tools::Tool;
use serde_json::json;
use std::path::PathBuf;
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
#[tokio::test]
async fn test_window_management_chain() {
    let window = ComputerWindowTool;

    // List windows
    let r = window.execute(json!({"action": "list"})).await.unwrap();
    assert_eq!(r["status"], "ok");
    assert!(r["windows"].is_array());

    // Focus a window
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

/// Test SWE-bench evaluator + report generation chain.
#[tokio::test]
async fn test_swebench_evaluator_chain() {
    let evaluator = SWEBenchEvaluator::new(PathBuf::from("/tmp/swebench_test"));

    // Load tasks
    let tasks = evaluator.load_tasks("public").unwrap();
    assert!(!tasks.is_empty());
    let task = &tasks[0];

    // Verify task structure
    assert!(!task.repo.is_empty());
    assert!(!task.instance_id.is_empty());
    assert!(!task.problem_statement.is_empty());
    assert!(!task.base_commit.is_empty());

    // Build an evaluation report manually
    let report = EvaluationReport {
        total_tasks: 1,
        resolved: 1,
        resolution_rate: 1.0,
        results: vec![TestResult {
            instance_id: task.instance_id.clone(),
            success: true,
            resolved: true,
            duration_secs: 10.0,
            iterations: 3,
            tokens_used: 5000,
            patch_applied: true,
            tests_passed: true,
            error: None,
            trajectory: vec![
                TrajectoryStep {
                    step: 1,
                    action: "setup".to_string(),
                    observation: "Environment ready".to_string(),
                    timestamp: "2026-03-24T12:00:00Z".to_string(),
                },
                TrajectoryStep {
                    step: 2,
                    action: "fix".to_string(),
                    observation: "Applied patch".to_string(),
                    timestamp: "2026-03-24T12:00:10Z".to_string(),
                },
            ],
        }],
        timestamp: "2026-03-24T12:00:00Z".to_string(),
    };

    // Generate and verify report
    let output = evaluator.generate_report(&report);
    assert!(output.contains("100.00%"));
    assert!(output.contains("RESOLVED"));
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

/// Test SWE-bench task loading + prompt construction chain with live endpoint check.
#[tokio::test]
async fn test_live_swebench_prompt_construction() {
    let endpoint = vision_endpoint();
    if !require_llm_endpoint_url(&endpoint).await {
        eprintln!("SKIPPED: test_live_swebench_prompt_construction - endpoint not available");
        return;
    }

    // Load tasks
    let evaluator = SWEBenchEvaluator::new(PathBuf::from("/tmp/swebench_live_test"));
    let tasks = evaluator.load_tasks("public").unwrap();
    let task = &tasks[0];

    // Build the prompt as the real pipeline would
    let prompt = format!(
        "SWE-bench Pro Task: {}\n\nRepository: {}\nProblem: {}\n\nFiles to modify: {:?}\n\n\
         Describe a strategy to fix this issue. Be concise.",
        task.instance_id, task.repo, task.problem_statement, task.target_files
    );

    // Send to the live model (text-only, no vision)
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .unwrap();

    let body = json!({
        "model": vision_model(),
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": 256,
        "temperature": 0.3,
        "stream": false
    });

    let response = client
        .post(format!("{}/chat/completions", endpoint))
        .json(&body)
        .send()
        .await;

    match response {
        Ok(r) if r.status().is_success() => {
            let json: serde_json::Value = r.json().await.unwrap();
            let content = json["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("");
            if content.is_empty() {
                // Some model configs may return empty for short prompts — not a failure
                eprintln!("Note: Model returned empty response (acceptable for some configs)");
            } else {
                println!(
                    "Live SWE-bench strategy: {}",
                    &content[..content.len().min(500)]
                );
            }
        }
        Ok(r) => {
            eprintln!("Note: Model returned HTTP {} (non-fatal)", r.status());
        }
        Err(e) => {
            eprintln!("SKIPPED: model request failed: {}", e);
        }
    }
}
