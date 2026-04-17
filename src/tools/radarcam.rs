//! RadarCam integration tools — operate, monitor, test, and introspect the RadarCam system.
//!
//! These tools allow the selfware agent to:
//! - Read system state (cameras, awareness, live events)
//! - Fetch camera frames for visual analysis
//! - Trigger calibration, 3D generation, and validation
//! - Read logs and run Python tests
//! - Perform comprehensive introspection

use super::Tool;
use anyhow::Result;
use async_trait::async_trait;
use base64::Engine;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

const RADARCAM_BASE: &str = "http://localhost:8000";
#[allow(dead_code)]
const RADARCAM_3DGEN: &str = "http://localhost:8001";
const RADARCAM_LOG_DIR: &str = "/home/ivo/radarcam";

/// Shared HTTP client helper
async fn radarcam_get(path: &str) -> Result<Value> {
    let url = format!("{}{}", RADARCAM_BASE, path);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;
    let resp = client.get(&url).send().await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        anyhow::bail!("RadarCam returned HTTP {} for {}: {}", status, url, text);
    }
    // Try parse as JSON, fall back to wrapping raw text
    match serde_json::from_str(&text) {
        Ok(v) => Ok(v),
        Err(_) => Ok(serde_json::json!({ "raw_text": text })),
    }
}

async fn radarcam_post(path: &str, body: Option<Value>) -> Result<Value> {
    let url = format!("{}{}", RADARCAM_BASE, path);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let mut req = client.post(&url);
    if let Some(b) = body {
        req = req.json(&b);
    }
    let resp = req.send().await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        anyhow::bail!("RadarCam returned HTTP {} for {}: {}", status, url, text);
    }
    match serde_json::from_str(&text) {
        Ok(v) => Ok(v),
        Err(_) => Ok(serde_json::json!({ "raw_text": text })),
    }
}

// ---------------------------------------------------------------------------
// Tool 1: radarcam_status — comprehensive system state
// ---------------------------------------------------------------------------

pub struct RadarCamStatus;

#[async_trait]
impl Tool for RadarCamStatus {
    fn name(&self) -> &str {
        "radarcam_status"
    }

    fn description(&self) -> &str {
        "Fetch comprehensive RadarCam system state. Returns camera status, awareness data, live events, calibration history, and validation summary. Use this as the first step before any RadarCam operation to understand the current system state."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "include_awareness": {
                    "type": "boolean",
                    "default": true,
                    "description": "Include full awareness data (can be large)"
                },
                "include_calibration": {
                    "type": "boolean",
                    "default": true,
                    "description": "Include latest calibration result"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_true")]
            include_awareness: bool,
            #[serde(default = "default_true")]
            include_calibration: bool,
        }
        fn default_true() -> bool { true }
        let args: Args = serde_json::from_value(args)?;

        // Fetch endpoints in parallel where possible
        let status = radarcam_get("/api/status").await;
        let intel = radarcam_get("/api/intel").await;
        let live_state = radarcam_get("/api/live-state").await;

        let calibration = if args.include_calibration {
            radarcam_get("/api/calibration").await.ok()
        } else {
            None
        };
        let cal_history = if args.include_calibration {
            radarcam_get("/api/calibration/history").await.ok()
        } else {
            None
        };

        let awareness = if args.include_awareness {
            radarcam_get("/api/awareness").await.ok()
        } else {
            None
        };

        // Read latest validation report if it exists
        let validation = async {
            let path = std::path::Path::new("/home/ivo/radarcam/validation_outputs/validation_report.json");
            if path.exists() {
                std::fs::read_to_string(path)
                    .ok()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            } else {
                None
            }
        }
        .await;

        // Check if key services are running
        let dashboard_up = status.is_ok();
        let gen3d_up = check_service("http://localhost:8001/").await;
        let vlm_up = check_service("http://localhost:9000/v1/models").await;

        let mut result = serde_json::json!({
            "dashboard_up": dashboard_up,
            "gen3d_up": gen3d_up,
            "vlm_up": vlm_up,
            "status": status.unwrap_or_else(|e| serde_json::json!({"error": e.to_string()})),
            "intel": intel.unwrap_or_else(|e| serde_json::json!({"error": e.to_string()})),
            "live_state": live_state.unwrap_or_else(|e| serde_json::json!({"error": e.to_string()})),
            "validation_latest": validation,
        });

        if let Some(obj) = result.as_object_mut() {
            if let Some(c) = calibration {
                obj.insert("calibration".to_string(), c);
            }
            if let Some(ch) = cal_history {
                obj.insert("calibration_history".to_string(), ch);
            }
            if let Some(a) = awareness {
                obj.insert("awareness".to_string(), a);
            }
        }

        Ok(result)
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::network()
    }
}

// ---------------------------------------------------------------------------
// Tool 2: radarcam_frame — fetch camera frame for visual analysis
// ---------------------------------------------------------------------------

pub struct RadarCamFrame;

#[async_trait]
impl Tool for RadarCamFrame {
    fn name(&self) -> &str {
        "radarcam_frame"
    }

    fn description(&self) -> &str {
        "Fetch a camera frame from RadarCam and return it as a base64-encoded image for visual analysis. The agent can then use its vision capabilities to analyze what the camera sees. Use camera_index 0 or 1. You can also request overlay, acquire, or spectral views."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "camera_index": {
                    "type": "integer",
                    "default": 0,
                    "description": "Camera index (0 or 1)"
                },
                "view": {
                    "type": "string",
                    "enum": ["natural", "overlay", "acquire", "spectral", "bank"],
                    "default": "natural",
                    "description": "View mode: natural, overlay (tactical HUD), acquire (cross-camera), spectral, or bank (filter bank)"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            camera_index: u32,
            #[serde(default = "default_view")]
            view: String,
        }
        fn default_view() -> String { "natural".to_string() }
        let args: Args = serde_json::from_value(args)?;

        let url = match args.view.as_str() {
            "bank" => format!("{}/spectral_bank/{}.jpg?view=bank", RADARCAM_BASE, args.camera_index),
            _ => format!("{}/frame/{}.jpg?view={}", RADARCAM_BASE, args.camera_index, args.view),
        };

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()?;
        let resp = client.get(&url).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("Failed to fetch frame: HTTP {}", resp.status());
        }
        let bytes = resp.bytes().await?;
        let base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

        Ok(serde_json::json!({
            "camera_index": args.camera_index,
            "view": args.view,
            "format": "jpeg",
            "bytes": bytes.len(),
            "base64_png": base64,
            "image_attached": true,
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::network()
    }
}

// ---------------------------------------------------------------------------
// Tool 3: radarcam_control — trigger operations
// ---------------------------------------------------------------------------

pub struct RadarCamControl;

#[async_trait]
impl Tool for RadarCamControl {
    fn name(&self) -> &str {
        "radarcam_control"
    }

    fn description(&self) -> &str {
        "Control RadarCam operations: trigger calibration, run validation checks, start 3D generation, or send camera control commands. Returns the operation result or job ID for async tasks."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["calibrate", "validate", "validate_check", "3dgen", "camera_control"],
                    "description": "Operation to perform"
                },
                "camera_index": {
                    "type": "integer",
                    "description": "Camera index for camera_control or 3dgen (0 or 1)"
                },
                "check_name": {
                    "type": "string",
                    "description": "Specific validation check name for validate_check (e.g. 'calibration_pipeline', 'vlm_endpoint', 'hardware')"
                },
                "camera_url": {
                    "type": "string",
                    "description": "Custom camera URL for 3dgen (optional, defaults to frame 0)"
                },
                "control_body": {
                    "type": "object",
                    "description": "JSON body for camera_control POST"
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            operation: String,
            camera_index: Option<u32>,
            check_name: Option<String>,
            camera_url: Option<String>,
            #[serde(default)]
            control_body: Option<Value>,
        }
        let args: Args = serde_json::from_value(args)?;

        match args.operation.as_str() {
            "calibrate" => {
                radarcam_post("/api/calibration/run", None).await
            }
            "validate" => {
                radarcam_post("/validation/run", None).await
            }
            "validate_check" => {
                let check = args.check_name.as_deref().unwrap_or("dashboard");
                radarcam_post(&format!("/validation/run/{}", check), None).await
            }
            "3dgen" => {
                let cam_url = args.camera_url.unwrap_or_else(|| {
                    format!("http://192.168.1.243:8080/frame/{}.jpg", args.camera_index.unwrap_or(0))
                });
                radarcam_post("/api/3d/generate", Some(serde_json::json!({"camera_url": cam_url}))).await
            }
            "camera_control" => {
                let cam = args.camera_index.unwrap_or(0);
                let body = args.control_body.unwrap_or(serde_json::json!({}));
                radarcam_post(&format!("/api/camera_controls/{}", cam), Some(body)).await
            }
            other => anyhow::bail!("Unknown operation: {}", other),
        }
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::custom(false, false, crate::safety::RiskLevel::Medium, true, false)
    }
}

// ---------------------------------------------------------------------------
// Tool 4: radarcam_logs — read log files
// ---------------------------------------------------------------------------

pub struct RadarCamLogs;

#[async_trait]
impl Tool for RadarCamLogs {
    fn name(&self) -> &str {
        "radarcam_logs"
    }

    fn description(&self) -> &str {
        "Read recent log files from the RadarCam system. Supports server.log (dashboard), gen3d_server.log (3D generation), vlm_server.log (VLM endpoint), and calibration results directory. Use tail_lines to limit output size."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "log_type": {
                    "type": "string",
                    "enum": ["server", "gen3d", "vlm", "calibration_results", "validation"],
                    "default": "server",
                    "description": "Which log to read"
                },
                "tail_lines": {
                    "type": "integer",
                    "default": 100,
                    "description": "Number of recent lines to return"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_log_type")]
            log_type: String,
            #[serde(default = "default_tail")]
            tail_lines: usize,
        }
        fn default_log_type() -> String { "server".to_string() }
        fn default_tail() -> usize { 100 }
        let args: Args = serde_json::from_value(args)?;

        let max_tail = 5000;
        let tail = args.tail_lines.min(max_tail);

        let result = match args.log_type.as_str() {
            "server" => read_log_file(&format!("{}/server.log", RADARCAM_LOG_DIR), tail),
            "gen3d" => read_log_file(&format!("{}/gen3d_server.log", RADARCAM_LOG_DIR), tail),
            "vlm" => read_log_file(&format!("{}/calibration/vlm_server.log", RADARCAM_LOG_DIR), tail),
            "calibration_results" => list_calibration_results(),
            "validation" => read_validation_report(),
            other => anyhow::bail!("Unknown log type: {}", other),
        };

        Ok(serde_json::json!({
            "log_type": args.log_type,
            "tail_lines": tail,
            "content": result.unwrap_or_else(|e| format!("Error reading log: {}", e)),
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::read_only()
    }
}

// ---------------------------------------------------------------------------
// Tool 5: radarcam_test — run Python tests
// ---------------------------------------------------------------------------

pub struct RadarCamTest;

#[async_trait]
impl Tool for RadarCamTest {
    fn name(&self) -> &str {
        "radarcam_test"
    }

    fn description(&self) -> &str {
        "Run Python tests in the RadarCam project. Can run the full validation suite, specific benchmark harnesses, or a specific Python test file. Uses the 'myenv' conda environment."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "test_type": {
                    "type": "string",
                    "enum": ["validation", "calibration_pipeline", "synthetic_sky", "fineair", "custom"],
                    "default": "validation",
                    "description": "Type of test to run"
                },
                "custom_command": {
                    "type": "string",
                    "description": "Custom test command for 'custom' type (runs in /home/ivo/radarcam with myenv conda env)"
                },
                "max_samples": {
                    "type": "integer",
                    "description": "Max samples for benchmark tests"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_test_type")]
            test_type: String,
            custom_command: Option<String>,
            max_samples: Option<usize>,
        }
        fn default_test_type() -> String { "validation".to_string() }
        let args: Args = serde_json::from_value(args)?;

        let cmd = match args.test_type.as_str() {
            "validation" => {
                "cd /home/ivo/radarcam && source /home/ivo/miniconda3/etc/profile.d/conda.sh && conda activate myenv && python validate.py".to_string()
            }
            "calibration_pipeline" => {
                "cd /home/ivo/radarcam && source /home/ivo/miniconda3/etc/profile.d/conda.sh && conda activate myenv && python -c 'from validation.checks import _check_calibration_pipeline; print(_check_calibration_pipeline())'".to_string()
            }
            "synthetic_sky" => {
                let samples = args.max_samples.unwrap_or(5);
                format!("cd /home/ivo/radarcam && source /home/ivo/miniconda3/etc/profile.d/conda.sh && conda activate myenv && python benchmarks/synthetic_sky/run_benchmark.py --num-samples {} --seed 42 --use-cv --output-dir /tmp/synthetic_sky_test", samples)
            }
            "fineair" => {
                let samples = args.max_samples.unwrap_or(2);
                format!("cd /home/ivo/radarcam/benchmarks/fineair && source /home/ivo/miniconda3/etc/profile.d/conda.sh && conda activate myenv && python run_benchmark.py --max-samples {} --output-dir /tmp/fineair_test", samples)
            }
            "custom" => {
                args.custom_command.unwrap_or_else(|| "echo 'No custom command provided'".to_string())
            }
            other => anyhow::bail!("Unknown test type: {}", other),
        };

        // Run via shell
        let output = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(&cmd)
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(serde_json::json!({
            "test_type": args.test_type,
            "exit_code": output.status.code(),
            "stdout": stdout,
            "stderr": stderr,
            "success": output.status.success(),
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::custom(false, false, crate::safety::RiskLevel::Medium, false, true)
    }
}

// ---------------------------------------------------------------------------
// Tool 6: radarcam_introspect — comprehensive health + visual analysis
// ---------------------------------------------------------------------------

pub struct RadarCamIntrospect;

#[async_trait]
impl Tool for RadarCamIntrospect {
    fn name(&self) -> &str {
        "radarcam_introspect"
    }

    fn description(&self) -> &str {
        "Perform a comprehensive introspection of RadarCam: check all service health, fetch a camera frame for visual analysis, read recent logs, and run validation. Returns a consolidated report. This is the 'full system check' tool — use it when you want to understand everything about RadarCam's current state in one call."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "camera_index": {
                    "type": "integer",
                    "default": 0,
                    "description": "Camera to capture for visual analysis"
                },
                "include_validation": {
                    "type": "boolean",
                    "default": false,
                    "description": "Run full validation (can take several minutes)"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            camera_index: u32,
            #[serde(default)]
            include_validation: bool,
        }
        let args: Args = serde_json::from_value(args)?;

        // 1. Service health
        let dashboard_up = check_service(&format!("{}/", RADARCAM_BASE)).await;
        let gen3d_up = check_service("http://localhost:8001/").await;
        let vlm_up = check_service("http://localhost:9000/v1/models").await;

        // 2. System state
        let status = radarcam_get("/api/status").await.ok();
        let intel = radarcam_get("/api/intel").await.ok();
        let live = radarcam_get("/api/live-state").await.ok();

        // 3. Fetch frame as base64 for multimodal analysis
        let frame_url = format!("{}/frame/{}.jpg", RADARCAM_BASE, args.camera_index);
        let frame_result = async {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()?;
            let resp = client.get(&frame_url).send().await?;
            if !resp.status().is_success() {
                anyhow::bail!("HTTP {}", resp.status());
            }
            let bytes = resp.bytes().await?;
            Ok::<_, anyhow::Error>(base64::engine::general_purpose::STANDARD.encode(&bytes))
        }
        .await;

        // 4. Recent logs (last 50 lines)
        let server_log = read_log_file(&format!("{}/server.log", RADARCAM_LOG_DIR), 50).unwrap_or_default();
        let gen3d_log = read_log_file(&format!("{}/gen3d_server.log", RADARCAM_LOG_DIR), 50).unwrap_or_default();

        // 5. Validation (optional, async)
        let validation = if args.include_validation {
            radarcam_post("/validation/run", None).await.ok()
        } else {
            None
        };

        let mut result = serde_json::json!({
            "health": {
                "dashboard": dashboard_up,
                "gen3d_server": gen3d_up,
                "vlm_endpoint": vlm_up,
            },
            "status": status,
            "intel": intel,
            "live_state": live,
            "logs": {
                "server_tail": server_log,
                "gen3d_tail": gen3d_log,
            },
            "validation": validation,
        });

        // Attach frame as base64_png for multimodal promotion
        if let Ok(b64) = frame_result {
            if let Some(obj) = result.as_object_mut() {
                obj.insert("base64_png".to_string(), serde_json::Value::String(b64));
                obj.insert("camera_index".to_string(), serde_json::json!(args.camera_index));
                obj.insert("frame_fetched".to_string(), serde_json::json!(true));
            }
        } else {
            if let Some(obj) = result.as_object_mut() {
                obj.insert("frame_fetched".to_string(), serde_json::json!(false));
                obj.insert("frame_error".to_string(), serde_json::json!(format!("{}", frame_result.unwrap_err())));
            }
        }

        Ok(result)
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::network()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn check_service(url: &str) -> bool {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build();
    match client {
        Ok(c) => match c.get(url).send().await {
            Ok(r) => r.status().is_success(),
            Err(_) => false,
        },
        Err(_) => false,
    }
}

fn read_log_file(path: &str, tail_lines: usize) -> Result<String> {
    let path = std::path::Path::new(path);
    if !path.exists() {
        return Ok(format!("Log file not found: {}", path.display()));
    }
    let content = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(tail_lines);
    let tail = lines[start..].join("\n");
    Ok(tail)
}

fn list_calibration_results() -> Result<String> {
    let dir = std::path::Path::new("/home/ivo/radarcam/calibration/results");
    if !dir.exists() {
        return Ok("No calibration results directory".to_string());
    }
    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "json").unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.metadata().ok().and_then(|m| m.modified().ok()).unwrap_or(std::time::SystemTime::UNIX_EPOCH)));
    let names: Vec<String> = entries.iter().take(10).map(|e| e.file_name().to_string_lossy().to_string()).collect();
    Ok(format!("Latest calibration results ({} total):\n{}", entries.len(), names.join("\n")))
}

fn read_validation_report() -> Result<String> {
    let path = std::path::Path::new("/home/ivo/radarcam/validation_outputs/validation_report.json");
    if !path.exists() {
        return Ok("No validation report found".to_string());
    }
    let content = std::fs::read_to_string(path)?;
    let json: Value = serde_json::from_str(&content)?;
    let passed = json["overall_passed"].as_bool();
    let summary = &json["summary"];
    Ok(format!(
        "Validation Report:\n  Overall: {}\n  Total: {}\n  Passed: {}\n  Failed: {}",
        passed.map(|p| if p { "PASS" } else { "FAIL" }).unwrap_or("UNKNOWN"),
        summary["total"].as_u64().unwrap_or(0),
        summary["passed"].as_u64().unwrap_or(0),
        summary["failed"].as_u64().unwrap_or(0),
    ))
}
