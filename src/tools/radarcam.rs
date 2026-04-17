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
        fn default_true() -> bool {
            true
        }
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
            let path = std::path::Path::new(
                "/home/ivo/radarcam/validation_outputs/validation_report.json",
            );
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
        fn default_view() -> String {
            "natural".to_string()
        }
        let args: Args = serde_json::from_value(args)?;

        let url = match args.view.as_str() {
            "bank" => format!("{}/spectral_bank/{}.jpg", RADARCAM_BASE, args.camera_index),
            _ => format!(
                "{}/frame/{}.jpg?view={}",
                RADARCAM_BASE, args.camera_index, args.view
            ),
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
            "calibrate" => radarcam_post("/api/calibration/run", None).await,
            "validate" => radarcam_post("/validation/run", None).await,
            "validate_check" => {
                let check = args.check_name.as_deref().unwrap_or("dashboard");
                radarcam_post(&format!("/validation/run/{}", check), None).await
            }
            "3dgen" => {
                let cam_url = args.camera_url.unwrap_or_else(|| {
                    format!(
                        "http://192.168.1.243:8080/frame/{}.jpg",
                        args.camera_index.unwrap_or(0)
                    )
                });
                radarcam_post(
                    "/api/3d/generate",
                    Some(serde_json::json!({"camera_url": cam_url})),
                )
                .await
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
        crate::safety::ToolMetadata::custom(
            false,
            false,
            crate::safety::RiskLevel::Medium,
            true,
            false,
        )
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
        fn default_log_type() -> String {
            "server".to_string()
        }
        fn default_tail() -> usize {
            100
        }
        let args: Args = serde_json::from_value(args)?;

        let max_tail = 5000;
        let tail = args.tail_lines.min(max_tail);

        let result = match args.log_type.as_str() {
            "server" => read_log_file(&format!("{}/server.log", RADARCAM_LOG_DIR), tail),
            "gen3d" => read_log_file(&format!("{}/gen3d_server.log", RADARCAM_LOG_DIR), tail),
            "vlm" => read_log_file(
                &format!("{}/calibration/vlm_server.log", RADARCAM_LOG_DIR),
                tail,
            ),
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
        fn default_test_type() -> String {
            "validation".to_string()
        }
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
        crate::safety::ToolMetadata::custom(
            false,
            false,
            crate::safety::RiskLevel::Medium,
            false,
            true,
        )
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
        let server_log =
            read_log_file(&format!("{}/server.log", RADARCAM_LOG_DIR), 50).unwrap_or_default();
        let gen3d_log = read_log_file(&format!("{}/gen3d_server.log", RADARCAM_LOG_DIR), 50)
            .unwrap_or_default();

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
                obj.insert(
                    "camera_index".to_string(),
                    serde_json::json!(args.camera_index),
                );
                obj.insert("frame_fetched".to_string(), serde_json::json!(true));
            }
        } else if let Some(obj) = result.as_object_mut() {
            obj.insert("frame_fetched".to_string(), serde_json::json!(false));
            obj.insert(
                "frame_error".to_string(),
                serde_json::json!(format!("{}", frame_result.unwrap_err())),
            );
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
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "json")
                .unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|e| {
        std::cmp::Reverse(
            e.metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
        )
    });
    let names: Vec<String> = entries
        .iter()
        .take(10)
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    Ok(format!(
        "Latest calibration results ({} total):\n{}",
        entries.len(),
        names.join("\n")
    ))
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
        passed
            .map(|p| if p { "PASS" } else { "FAIL" })
            .unwrap_or("UNKNOWN"),
        summary["total"].as_u64().unwrap_or(0),
        summary["passed"].as_u64().unwrap_or(0),
        summary["failed"].as_u64().unwrap_or(0),
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Test helpers
    // ========================================================================

    /// Retry wrapper for dashboard availability check with exponential backoff.
    /// Handles transient failures when the dashboard is under load.
    async fn dashboard_available() -> bool {
        for attempt in 0..3 {
            if check_service("http://localhost:8000/api/status").await {
                return true;
            }
            if attempt < 2 {
                tokio::time::sleep(Duration::from_millis(200 * (attempt + 1))).await;
            }
        }
        false
    }

    /// Retry wrapper for radarcam_get with transient-failure tolerance.
    async fn radarcam_get_with_retry(path: &str, max_retries: u32) -> Result<Value> {
        let mut last_err = None;
        for attempt in 0..=max_retries {
            match radarcam_get(path).await {
                Ok(v) => return Ok(v),
                Err(e) => {
                    let msg = e.to_string();
                    // Retry on transient errors (timeout, 502, 503)
                    let is_transient = msg.contains("502")
                        || msg.contains("503")
                        || msg.contains("timeout")
                        || msg.contains("Timeout");
                    if !is_transient || attempt == max_retries {
                        return Err(e);
                    }
                    last_err = Some(msg);
                    tokio::time::sleep(Duration::from_millis(300 * (attempt + 1) as u64)).await;
                }
            }
        }
        anyhow::bail!("Exhausted retries: {:?}", last_err)
    }

    /// Spawn the Python comparison harness with retry logic.
    async fn run_python_comparison(command: &str, args: &[&str]) -> Result<Value> {
        let script = format!(
            "source /home/ivo/miniconda3/etc/profile.d/conda.sh && conda activate myenv && python /home/ivo/radarcam/tests/python_comparison.py {} {}",
            command,
            args.join(" ")
        );

        let mut last_err = None;
        for attempt in 0..3 {
            let mut cmd = tokio::process::Command::new("bash");
            cmd.arg("-c").arg(&script);
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());

            match cmd.output().await {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);

                    if !output.status.success() {
                        let err_msg = format!(
                            "Python script failed (exit={:?}): stderr={}",
                            output.status.code(),
                            stderr
                        );
                        // Retry on transient Python failures
                        let is_transient = stderr.contains("502")
                            || stderr.contains("503")
                            || stderr.contains("timeout")
                            || stderr.contains("Connection");
                        if is_transient && attempt < 2 {
                            last_err = Some(err_msg);
                            tokio::time::sleep(Duration::from_millis(300 * (attempt + 1) as u64))
                                .await;
                            continue;
                        }
                        anyhow::bail!(err_msg);
                    }

                    return match serde_json::from_str(&stdout) {
                        Ok(v) => Ok(v),
                        Err(e) => {
                            anyhow::bail!("Failed to parse Python output: {}\nRaw: {}", e, stdout)
                        }
                    };
                }
                Err(e) => {
                    last_err = Some(e.to_string());
                    if attempt < 2 {
                        tokio::time::sleep(Duration::from_millis(300 * (attempt + 1) as u64)).await;
                    }
                }
            }
        }
        anyhow::bail!("Python comparison exhausted retries: {:?}", last_err)
    }

    // ========================================================================
    // 1. UNIT TESTS — no external dependencies, always run
    // ========================================================================

    #[test]
    fn test_tool_names() {
        assert_eq!(RadarCamStatus.name(), "radarcam_status");
        assert_eq!(RadarCamFrame.name(), "radarcam_frame");
        assert_eq!(RadarCamControl.name(), "radarcam_control");
        assert_eq!(RadarCamLogs.name(), "radarcam_logs");
        assert_eq!(RadarCamTest.name(), "radarcam_test");
        assert_eq!(RadarCamIntrospect.name(), "radarcam_introspect");
    }

    #[test]
    fn test_tool_descriptions_non_empty() {
        let tools: Vec<Box<dyn Tool>> = vec![
            Box::new(RadarCamStatus),
            Box::new(RadarCamFrame),
            Box::new(RadarCamControl),
            Box::new(RadarCamLogs),
            Box::new(RadarCamTest),
            Box::new(RadarCamIntrospect),
        ];
        for t in tools {
            assert!(
                !t.description().is_empty(),
                "{} description is empty",
                t.name()
            );
            assert!(
                t.description().len() > 20,
                "{} description too short",
                t.name()
            );
        }
    }

    #[test]
    fn test_tool_schemas_are_valid_json() {
        let tools: Vec<Box<dyn Tool>> = vec![
            Box::new(RadarCamStatus),
            Box::new(RadarCamFrame),
            Box::new(RadarCamControl),
            Box::new(RadarCamLogs),
            Box::new(RadarCamTest),
            Box::new(RadarCamIntrospect),
        ];
        for t in tools {
            let schema = t.schema();
            assert!(
                schema.get("type").is_some(),
                "{} schema missing 'type' field",
                t.name()
            );
            assert_eq!(
                schema["type"],
                "object",
                "{} schema type not object",
                t.name()
            );
            assert!(
                schema.get("properties").is_some(),
                "{} schema missing 'properties'",
                t.name()
            );
        }
    }

    #[test]
    fn test_tool_metadata() {
        assert!(RadarCamStatus.metadata().read_only);
        assert!(RadarCamFrame.metadata().read_only);
        assert!(RadarCamLogs.metadata().read_only);
        assert!(RadarCamIntrospect.metadata().read_only);
        assert!(!RadarCamControl.metadata().read_only);
        assert!(!RadarCamTest.metadata().read_only);
    }

    #[test]
    fn test_read_log_file_missing() {
        let result = read_log_file("/tmp/nonexistent_log_file_12345.log", 10);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Log file not found"));
    }

    #[test]
    fn test_read_log_file_with_content() {
        let tmpfile = std::env::temp_dir().join("radarcam_test_log.txt");
        let content = "line1\nline2\nline3\nline4\nline5";
        std::fs::write(&tmpfile, content).unwrap();
        let result = read_log_file(tmpfile.to_str().unwrap(), 3);
        assert!(result.is_ok());
        let text = result.unwrap();
        assert!(text.contains("line3"));
        assert!(text.contains("line4"));
        assert!(text.contains("line5"));
        assert!(!text.contains("line1"));
        std::fs::remove_file(&tmpfile).ok();
    }

    #[test]
    fn test_read_log_file_tail_larger_than_content() {
        let tmpfile = std::env::temp_dir().join("radarcam_test_log2.txt");
        std::fs::write(&tmpfile, "a\nb").unwrap();
        let result = read_log_file(tmpfile.to_str().unwrap(), 100);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "a\nb");
        std::fs::remove_file(&tmpfile).ok();
    }

    #[test]
    fn test_list_calibration_results_missing_dir() {
        let result = list_calibration_results();
        assert!(result.is_ok());
    }

    #[test]
    fn test_read_validation_report_missing_or_present() {
        let result = read_validation_report();
        assert!(result.is_ok());
        let text = result.unwrap();
        assert!(
            text.contains("Validation Report") || text.contains("No validation report"),
            "Unexpected output: {}",
            text
        );
    }

    #[tokio::test]
    async fn test_check_service_offline() {
        let up = check_service("http://localhost:59999/").await;
        assert!(!up, "Expected offline service to return false");
    }

    #[tokio::test]
    async fn test_check_service_online() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }
        let up = check_service("http://localhost:8000/api/status").await;
        assert!(up, "Dashboard should be online");
    }

    // ========================================================================
    // 2. INTEGRATION TESTS — require RadarCam dashboard on localhost:8000
    // ========================================================================

    #[tokio::test]
    async fn test_radarcam_get_status_endpoint() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }
        let result = radarcam_get_with_retry("/api/status", 2).await;
        assert!(result.is_ok(), "status failed: {:?}", result.err());
        let json = result.unwrap();
        assert!(json.get("cameras").is_some());
    }

    #[tokio::test]
    async fn test_radarcam_get_intel_endpoint() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }
        let result = radarcam_get_with_retry("/api/intel", 2).await;
        assert!(result.is_ok(), "intel failed: {:?}", result.err());
        let json = result.unwrap();
        assert!(json.get("generated_at").is_some());
    }

    #[tokio::test]
    async fn test_radarcam_get_live_state_endpoint() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }
        let result = radarcam_get_with_retry("/api/live-state", 2).await;
        assert!(result.is_ok(), "live-state failed: {:?}", result.err());
        let json = result.unwrap();
        assert!(json.get("cameras").is_some() || json.get("live").is_some());
    }

    #[tokio::test]
    async fn test_radarcam_get_calibration_endpoint() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }
        let result = radarcam_get_with_retry("/api/calibration", 2).await;
        assert!(result.is_ok(), "calibration failed: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_radarcam_get_awareness_endpoint() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }
        let result = radarcam_get_with_retry("/api/awareness", 2).await;
        assert!(result.is_ok(), "awareness failed: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_radarcam_get_nonjson_endpoint_returns_raw_text() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }
        let result = radarcam_get_with_retry("/", 2).await;
        assert!(result.is_ok(), "root failed: {:?}", result.err());
        let json = result.unwrap();
        assert!(json.get("raw_text").is_some());
    }

    // --- Frame tool variants ---

    #[tokio::test]
    async fn test_radarcam_frame_natural() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }
        let tool = RadarCamFrame;
        let result = tool
            .execute(serde_json::json!({"camera_index": 0, "view": "natural"}))
            .await;
        assert!(result.is_ok(), "frame natural failed: {:?}", result.err());
        let json = result.unwrap();
        assert!(json.get("base64_png").is_some());
        assert_eq!(json["camera_index"], 0);
        assert_eq!(json["view"], "natural");
        assert!(json["bytes"].as_u64().unwrap_or(0) > 1000);
    }

    #[tokio::test]
    async fn test_radarcam_frame_overlay() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }
        let tool = RadarCamFrame;
        let result = tool
            .execute(serde_json::json!({"camera_index": 0, "view": "overlay"}))
            .await;
        assert!(result.is_ok(), "frame overlay failed: {:?}", result.err());
        let json = result.unwrap();
        assert_eq!(json["view"], "overlay");
        assert!(json["base64_png"].as_str().unwrap_or("").len() > 100);
    }

    #[tokio::test]
    async fn test_radarcam_frame_bank() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }
        let tool = RadarCamFrame;
        let result = tool
            .execute(serde_json::json!({"camera_index": 0, "view": "bank"}))
            .await;
        assert!(result.is_ok(), "frame bank failed: {:?}", result.err());
        let json = result.unwrap();
        assert_eq!(json["view"], "bank");
    }

    #[tokio::test]
    async fn test_radarcam_frame_defaults() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }
        let tool = RadarCamFrame;
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_ok(), "frame defaults failed: {:?}", result.err());
        let json = result.unwrap();
        assert_eq!(json["camera_index"], 0);
        assert_eq!(json["view"], "natural");
    }

    // --- Status tool variants ---

    #[tokio::test]
    async fn test_radarcam_status_defaults() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }
        let tool = RadarCamStatus;
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_ok(), "status defaults failed: {:?}", result.err());
        let json = result.unwrap();
        assert!(json.get("dashboard_up").is_some());
        assert!(json.get("status").is_some());
        assert!(json.get("intel").is_some());
        assert!(json.get("live_state").is_some());
    }

    #[tokio::test]
    async fn test_radarcam_status_no_awareness() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }
        let tool = RadarCamStatus;
        let result = tool
            .execute(serde_json::json!({"include_awareness": false}))
            .await;
        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.get("awareness").is_none());
    }

    #[tokio::test]
    async fn test_radarcam_status_no_calibration() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }
        let tool = RadarCamStatus;
        let result = tool
            .execute(serde_json::json!({"include_calibration": false}))
            .await;
        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.get("calibration").is_none());
        assert!(json.get("calibration_history").is_none());
    }

    // --- Control tool variants ---

    #[tokio::test]
    async fn test_radarcam_control_validate() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }
        let tool = RadarCamControl;
        let result = tool
            .execute(serde_json::json!({"operation": "validate"}))
            .await;
        assert!(
            result.is_ok(),
            "control validate failed: {:?}",
            result.err()
        );
        let json = result.unwrap();
        assert!(json.get("overall_passed").is_some() || json.get("run_id").is_some());
    }

    #[tokio::test]
    async fn test_radarcam_control_validate_check() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }
        let tool = RadarCamControl;
        let result = tool
            .execute(serde_json::json!({"operation": "validate_check", "check_name": "dashboard"}))
            .await;
        assert!(
            result.is_ok(),
            "control validate_check failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_radarcam_control_unknown_operation() {
        let tool = RadarCamControl;
        let result = tool
            .execute(serde_json::json!({"operation": "not_a_real_op"}))
            .await;
        assert!(result.is_err(), "expected error for unknown operation");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Unknown operation"), "error msg: {}", err);
    }

    // --- Logs tool variants ---

    #[tokio::test]
    async fn test_radarcam_logs_server() {
        let tool = RadarCamLogs;
        let result = tool
            .execute(serde_json::json!({"log_type": "server", "tail_lines": 10}))
            .await;
        assert!(result.is_ok(), "logs server failed: {:?}", result.err());
        let json = result.unwrap();
        assert_eq!(json["log_type"], "server");
        assert_eq!(json["tail_lines"], 10);
        assert!(json.get("content").is_some());
    }

    #[tokio::test]
    async fn test_radarcam_logs_gen3d() {
        let tool = RadarCamLogs;
        let result = tool
            .execute(serde_json::json!({"log_type": "gen3d", "tail_lines": 5}))
            .await;
        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["log_type"], "gen3d");
    }

    #[tokio::test]
    async fn test_radarcam_logs_vlm() {
        let tool = RadarCamLogs;
        let result = tool.execute(serde_json::json!({"log_type": "vlm"})).await;
        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["log_type"], "vlm");
    }

    #[tokio::test]
    async fn test_radarcam_logs_calibration_results() {
        let tool = RadarCamLogs;
        let result = tool
            .execute(serde_json::json!({"log_type": "calibration_results"}))
            .await;
        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["log_type"], "calibration_results");
    }

    #[tokio::test]
    async fn test_radarcam_logs_validation() {
        let tool = RadarCamLogs;
        let result = tool
            .execute(serde_json::json!({"log_type": "validation"}))
            .await;
        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["log_type"], "validation");
    }

    #[tokio::test]
    async fn test_radarcam_logs_tail_clamped() {
        let tool = RadarCamLogs;
        let result = tool
            .execute(serde_json::json!({"log_type": "server", "tail_lines": 99999}))
            .await;
        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["tail_lines"], 5000);
    }

    #[tokio::test]
    async fn test_radarcam_logs_unknown_type() {
        let tool = RadarCamLogs;
        let result = tool
            .execute(serde_json::json!({"log_type": "not_a_log"}))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown log type"));
    }

    // --- Introspect tool variants ---

    #[tokio::test]
    async fn test_radarcam_introspect_without_validation() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }
        let tool = RadarCamIntrospect;
        let result = tool
            .execute(serde_json::json!({"camera_index": 0, "include_validation": false}))
            .await;
        assert!(result.is_ok(), "introspect failed: {:?}", result.err());
        let json = result.unwrap();
        assert!(json.get("health").is_some());
        assert!(json.get("status").is_some());
        assert!(json.get("intel").is_some());
        assert!(json.get("logs").is_some());
    }

    #[tokio::test]
    async fn test_radarcam_introspect_with_validation() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }
        let tool = RadarCamIntrospect;
        let result = tool
            .execute(serde_json::json!({"camera_index": 0, "include_validation": true}))
            .await;
        assert!(
            result.is_ok(),
            "introspect+validation failed: {:?}",
            result.err()
        );
        let json = result.unwrap();
        assert!(json.get("validation").is_some());
    }

    // ========================================================================
    // 3. RUST ↔ PYTHON COMPARISON TESTS — run in parallel with retry
    // ========================================================================

    #[tokio::test]
    async fn test_compare_status_rust_vs_python() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }

        let (rust_result, python_result) = tokio::join!(
            RadarCamStatus.execute(serde_json::json!({})),
            run_python_comparison("status", &[]),
        );

        let rust = rust_result.expect("Rust status failed");
        let python = python_result.expect("Python status failed");

        assert_eq!(
            rust["dashboard_up"].as_bool(),
            python["dashboard_up"].as_bool(),
            "dashboard_up mismatch"
        );
        assert_eq!(
            rust["gen3d_up"].as_bool(),
            python["gen3d_up"].as_bool(),
            "gen3d_up mismatch"
        );
        assert_eq!(
            rust["vlm_up"].as_bool(),
            python["vlm_up"].as_bool(),
            "vlm_up mismatch"
        );
        // Both should have cameras in status OR an error (if endpoint was temporarily down)
        let rust_status_ok = rust["status"].get("cameras").is_some();
        let py_status_ok = python["status"].get("cameras").is_some();
        assert_eq!(
            rust_status_ok, py_status_ok,
            "Status availability mismatch: Rust has cameras={} vs Py has cameras={}",
            rust_status_ok, py_status_ok
        );

        // Both should have generated_at in intel OR an error
        // NOTE: Allow temporal drift — the two requests run in parallel and the
        // dashboard may generate the intel report between them. We only verify
        // that neither side has a hard error for the intel endpoint.
        let rust_intel_err = rust["intel"].get("error").is_some();
        let py_intel_err = python["intel"].get("error").is_some();
        assert_eq!(
            rust_intel_err, py_intel_err,
            "Intel error mismatch: Rust has error={} vs Py has error={}",
            rust_intel_err, py_intel_err
        );

        eprintln!("✓ Rust ↔ Python status comparison passed");
    }

    #[tokio::test]
    async fn test_compare_frame_rust_vs_python() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }

        // Frame fetching is the heaviest operation — retry once on transient timeout.
        let mut last_err = None;
        for attempt in 0..2 {
            let (rust_result, python_result) = tokio::join!(
                RadarCamFrame.execute(serde_json::json!({"camera_index": 0, "view": "natural"})),
                run_python_comparison("frame", &["0", "natural"]),
            );

            match (&rust_result, &python_result) {
                (Ok(rust), Ok(python)) => {
                    assert_eq!(rust["camera_index"], python["camera_index"]);
                    assert_eq!(rust["view"], python["view"]);
                    assert_eq!(rust["format"], python["format"]);

                    let rust_bytes = rust["bytes"].as_u64().unwrap_or(0);
                    let py_bytes = python["bytes"].as_u64().unwrap_or(0);
                    assert!(
                        rust_bytes > 0 && py_bytes > 0,
                        "Frame byte counts invalid: Rust={} Py={}",
                        rust_bytes,
                        py_bytes
                    );
                    // JPEG encoding can vary slightly between concurrent requests
                    // (overlay timestamps, re-compression). Allow 2% tolerance.
                    let byte_diff = rust_bytes.abs_diff(py_bytes);
                    let tolerance = (rust_bytes.max(py_bytes) as f64 * 0.02) as u64;
                    assert!(
                        byte_diff <= tolerance.max(1024),
                        "frame byte count mismatch: Rust={} Py={} (diff={}, tolerance={})",
                        rust_bytes,
                        py_bytes,
                        byte_diff,
                        tolerance
                    );

                    let rust_b64_len = rust["base64_png"].as_str().map(|s| s.len()).unwrap_or(0);
                    let py_b64_len = python["base64_png"].as_str().map(|s| s.len()).unwrap_or(0);
                    let b64_diff = rust_b64_len.abs_diff(py_b64_len);
                    let b64_tolerance = (rust_b64_len.max(py_b64_len) as f64 * 0.02) as usize;
                    assert!(
                        b64_diff <= b64_tolerance.max(1024),
                        "frame base64 length mismatch: Rust={} Py={} (diff={}, tolerance={})",
                        rust_b64_len,
                        py_b64_len,
                        b64_diff,
                        b64_tolerance
                    );

                    eprintln!(
                        "✓ Rust ↔ Python frame comparison passed ({} bytes)",
                        rust_bytes
                    );
                    return;
                }
                _ => {
                    let err = format!(
                        "Attempt {}: Rust={:?}, Py={:?}",
                        attempt, rust_result, python_result
                    );
                    last_err = Some(err);
                    if attempt == 0 {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            }
        }
        panic!("Frame comparison failed after retries: {:?}", last_err);
    }

    #[tokio::test]
    async fn test_compare_validation_rust_vs_python() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }

        let (rust_result, python_result) = tokio::join!(
            RadarCamControl.execute(
                serde_json::json!({"operation": "validate_check", "check_name": "dashboard"})
            ),
            run_python_comparison("validate", &["dashboard"]),
        );

        let rust = rust_result.expect("Rust validation failed");
        let python = python_result.expect("Python validation failed");

        let rust_has_time = rust.get("started_at").is_some() || rust.get("run_id").is_some();
        let py_has_time = python.get("started_at").is_some() || python.get("run_id").is_some();
        assert_eq!(
            rust_has_time, py_has_time,
            "Rust and Python validation time/run_id mismatch"
        );

        eprintln!("✓ Rust ↔ Python validation comparison passed");
    }

    #[tokio::test]
    async fn test_compare_logs_rust_vs_python() {
        let (rust_result, python_result) = tokio::join!(
            RadarCamLogs.execute(serde_json::json!({"log_type": "server", "tail_lines": 20})),
            run_python_comparison("logs", &["server", "20"]),
        );

        let rust = rust_result.expect("Rust logs failed");
        let python = python_result.expect("Python logs failed");

        assert_eq!(rust["log_type"], python["log_type"]);
        assert_eq!(rust["tail_lines"], python["tail_lines"]);

        let rust_content = rust["content"].as_str().unwrap_or("");
        let py_content = python["content"].as_str().unwrap_or("");

        // The log file is being appended to concurrently by the running dashboard,
        // so exact line-by-line comparison is inherently racy. Instead, verify
        // structural similarity: both have content, similar line counts, and
        // contain recognizable log patterns.
        if !rust_content.is_empty() && !py_content.is_empty() {
            let rust_lines: Vec<&str> = rust_content.lines().collect();
            let py_lines: Vec<&str> = py_content.lines().collect();

            // Both should have approximately the same number of lines
            let line_diff = rust_lines.len().abs_diff(py_lines.len());
            assert!(
                line_diff <= 5,
                "Log line count mismatch: Rust={} vs Py={}",
                rust_lines.len(),
                py_lines.len()
            );

            // Both should contain recognizable server log patterns
            assert!(
                rust_content.contains("INFO:") || rust_content.contains("ERROR:"),
                "Rust server log missing expected patterns"
            );
            assert!(
                py_content.contains("INFO:") || py_content.contains("ERROR:"),
                "Python server log missing expected patterns"
            );
        }

        eprintln!("✓ Rust ↔ Python logs comparison passed");
    }

    #[tokio::test]
    async fn test_compare_frame_views_rust_vs_python() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }

        let mut last_err = None;
        for attempt in 0..2 {
            let (rust_result, python_result) = tokio::join!(
                RadarCamFrame.execute(serde_json::json!({"camera_index": 0, "view": "overlay"})),
                run_python_comparison("frame", &["0", "overlay"]),
            );

            match (&rust_result, &python_result) {
                (Ok(rust), Ok(python)) => {
                    assert_eq!(rust["view"], python["view"]);
                    assert_eq!(rust["format"], python["format"]);
                    let rust_bytes = rust["bytes"].as_u64().unwrap_or(0);
                    let py_bytes = python["bytes"].as_u64().unwrap_or(0);
                    assert!(
                        rust_bytes > 0 && py_bytes > 0,
                        "Overlay frame byte counts invalid: Rust={} Py={}",
                        rust_bytes,
                        py_bytes
                    );
                    // Overlay frames include dynamic timestamps → allow 2% tolerance
                    let byte_diff = rust_bytes.abs_diff(py_bytes);
                    let tolerance = (rust_bytes.max(py_bytes) as f64 * 0.02) as u64;
                    assert!(
                        byte_diff <= tolerance.max(1024),
                        "overlay frame byte count mismatch: Rust={} Py={} (diff={}, tolerance={})",
                        rust_bytes,
                        py_bytes,
                        byte_diff,
                        tolerance
                    );
                    eprintln!("✓ Rust ↔ Python frame overlay comparison passed");
                    return;
                }
                _ => {
                    let err = format!(
                        "Attempt {}: Rust={:?}, Py={:?}",
                        attempt, rust_result, python_result
                    );
                    last_err = Some(err);
                    if attempt == 0 {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            }
        }
        panic!(
            "Frame overlay comparison failed after retries: {:?}",
            last_err
        );
    }

    #[tokio::test]
    async fn test_compare_control_calibrate_rust_vs_python() {
        if !dashboard_available().await {
            eprintln!("SKIPPING: RadarCam dashboard not running");
            return;
        }

        let (rust_result, python_result) = tokio::join!(
            RadarCamControl.execute(serde_json::json!({"operation": "calibrate"})),
            run_python_comparison("control", &["calibrate"]),
        );

        match (&rust_result, &python_result) {
            (Ok(rust), Ok(python)) => {
                let rust_has_job = rust.get("job_id").is_some() || rust.get("id").is_some();
                let py_has_job = python.get("job_id").is_some() || python.get("id").is_some();
                assert_eq!(
                    rust_has_job, py_has_job,
                    "Rust and Python calibrate job_id mismatch"
                );
                eprintln!("✓ Rust ↔ Python calibrate comparison passed");
            }
            (Err(rust_err), Err(py_err)) => {
                eprintln!(
                    "✓ Both calibrate calls failed (expected if busy): Rust={}, Py={}",
                    rust_err, py_err
                );
            }
            (Ok(_), Err(e)) | (Err(e), Ok(_)) => {
                panic!("Mismatch: one succeeded and one failed: {:?}", e);
            }
        }
    }

    // ========================================================================
    // 4. TEST TOOL — smoke tests
    // ========================================================================

    #[tokio::test]
    async fn test_radarcam_test_custom_echo() {
        let tool = RadarCamTest;
        let result = tool
            .execute(serde_json::json!({"test_type": "custom", "custom_command": "echo 'hello from test'"}))
            .await;
        assert!(result.is_ok(), "test custom failed: {:?}", result.err());
        let json = result.unwrap();
        assert!(json["success"].as_bool().unwrap_or(false));
        assert!(json["stdout"]
            .as_str()
            .unwrap_or("")
            .contains("hello from test"));
    }

    #[tokio::test]
    async fn test_radarcam_test_unknown_type() {
        let tool = RadarCamTest;
        let result = tool
            .execute(serde_json::json!({"test_type": "nonexistent_type"}))
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown test type"));
    }
}
