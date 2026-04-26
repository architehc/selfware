//! Frame-by-frame video analysis system.
//!
//! Captures screen frames, sends them to a multimodal LLM, and generates
//! real-time debugging feedback with JSONL logging.
//!
//! Usage:
//! ```sh
//! cargo run --bin frame_analyzer --features system-tests -- \
//!   --endpoint http://127.0.0.1:8000/v1 \
//!   --model txn545/Qwen3.5-122B-A10B-NVFP4 \
//!   --fps 2 --duration 30 --game-mode
//! ```

use base64::Engine;
use chrono::Utc;
use image::ImageFormat;
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use xcap::Monitor;

// ---------------------------------------------------------------------------
// CLI config
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Config {
    endpoint: String,
    model: String,
    fps: u32,
    duration: u64,
    region: Option<(i32, i32, u32, u32)>,
    output: PathBuf,
    record: bool,
    mode: AnalysisMode,
}

#[derive(Debug, Clone, Copy)]
enum AnalysisMode {
    Generic,
    Game,
    Web,
}

impl Config {
    fn from_args() -> Self {
        let args: Vec<String> = std::env::args().collect();

        let mut cfg = Config {
            endpoint: "http://127.0.0.1:8000/v1".to_string(),
            model: "txn545/Qwen3.5-122B-A10B-NVFP4".to_string(),
            fps: 2,
            duration: 30,
            region: None,
            output: PathBuf::from("./frame_analysis"),
            record: false,
            mode: AnalysisMode::Generic,
        };

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--endpoint" => {
                    i += 1;
                    cfg.endpoint = args[i].clone();
                }
                "--model" => {
                    i += 1;
                    cfg.model = args[i].clone();
                }
                "--fps" => {
                    i += 1;
                    let v: u32 = args[i].parse().expect("--fps must be a number");
                    cfg.fps = v.min(10).max(1);
                }
                "--duration" => {
                    i += 1;
                    cfg.duration = args[i].parse().expect("--duration must be a number");
                }
                "--region" => {
                    i += 1;
                    let parts: Vec<i32> = args[i]
                        .split(',')
                        .map(|s| s.trim().parse().expect("region must be X,Y,W,H integers"))
                        .collect();
                    if parts.len() != 4 {
                        panic!("--region must have exactly 4 values: X,Y,W,H");
                    }
                    cfg.region = Some((parts[0], parts[1], parts[2] as u32, parts[3] as u32));
                }
                "--output" => {
                    i += 1;
                    cfg.output = PathBuf::from(&args[i]);
                }
                "--record" => {
                    cfg.record = true;
                }
                "--game-mode" => {
                    cfg.mode = AnalysisMode::Game;
                }
                "--web-mode" => {
                    cfg.mode = AnalysisMode::Web;
                }
                "--help" | "-h" => {
                    eprintln!(
                        "Usage: frame_analyzer [OPTIONS]\n\
                         \n\
                         Options:\n\
                         --endpoint URL     VLM endpoint (default: http://127.0.0.1:8000/v1)\n\
                         --model MODEL      Model name (default: txn545/Qwen3.5-122B-A10B-NVFP4)\n\
                         --fps N            Capture rate 1-10 (default: 2)\n\
                         --duration SECS    Total duration (default: 30)\n\
                         --region X,Y,W,H   Capture sub-region\n\
                         --output DIR       Output directory (default: ./frame_analysis)\n\
                         --record           Save individual frames as PNGs\n\
                         --game-mode        Optimised prompts for game UI\n\
                         --web-mode         Optimised prompts for web apps\n\
                         -h, --help         Show this help"
                    );
                    std::process::exit(0);
                }
                other => {
                    eprintln!("Unknown argument: {other}");
                    std::process::exit(1);
                }
            }
            i += 1;
        }

        cfg
    }
}

// ---------------------------------------------------------------------------
// Screen capture
// ---------------------------------------------------------------------------

fn capture_screen(region: Option<(i32, i32, u32, u32)>) -> Result<Vec<u8>, String> {
    let monitors = Monitor::all().map_err(|e| format!("Failed to list monitors: {e}"))?;
    let monitor = monitors
        .into_iter()
        .next()
        .ok_or_else(|| "No monitors found".to_string())?;

    let img = monitor
        .capture_image()
        .map_err(|e| format!("Capture failed: {e}"))?;

    let img = if let Some((x, y, w, h)) = region {
        let x = x.max(0) as u32;
        let y = y.max(0) as u32;
        let w = w.min(img.width().saturating_sub(x));
        let h = h.min(img.height().saturating_sub(y));
        image::DynamicImage::ImageRgba8(img).crop_imm(x, y, w, h)
    } else {
        image::DynamicImage::ImageRgba8(img)
    };

    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, ImageFormat::Png)
        .map_err(|e| format!("PNG encode failed: {e}"))?;

    Ok(buf.into_inner())
}

fn encode_base64(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

// ---------------------------------------------------------------------------
// VLM interaction
// ---------------------------------------------------------------------------

fn build_prompt(mode: AnalysisMode, frame_num: u64, timestamp: &str, prev_summary: &str) -> String {
    match mode {
        AnalysisMode::Game => format!(
            "You are analyzing a video game frame. This is frame {frame_num} captured at {timestamp}.\n\
             \n\
             Previous frame analysis: {prev_summary}\n\
             \n\
             Analyze this frame:\n\
             1. What is visible on screen? (UI elements, game state)\n\
             2. Any visual glitches, rendering errors, or artifacts?\n\
             3. UI/UX issues? (overlapping elements, unreadable text, broken layouts)\n\
             4. Performance indicators? (frame rate counter, loading indicators)\n\
             5. Compared to previous frame: what changed? Any regression?\n\
             \n\
             Return JSON:\n\
             {{\n\
               \"frame\": {frame_num},\n\
               \"state\": \"description of current state\",\n\
               \"issues\": [{{\"type\": \"glitch|layout|perf|ux\", \"description\": \"...\", \"severity\": \"low|medium|high\"}}],\n\
               \"changes_from_previous\": \"description\",\n\
               \"suggested_fixes\": [\"...\"],\n\
               \"overall_quality\": 0\n\
             }}"
        ),
        AnalysisMode::Web => format!(
            "Analyze this web application screenshot (frame {frame_num}, captured at {timestamp}).\n\
             Previous: {prev_summary}\n\
             Look for: broken layouts, CSS issues, missing content, error messages, loading failures, accessibility problems.\n\
             \n\
             Return JSON:\n\
             {{\n\
               \"frame\": {frame_num},\n\
               \"state\": \"description of current state\",\n\
               \"issues\": [{{\"type\": \"glitch|layout|perf|ux\", \"description\": \"...\", \"severity\": \"low|medium|high\"}}],\n\
               \"changes_from_previous\": \"description\",\n\
               \"suggested_fixes\": [\"...\"],\n\
               \"overall_quality\": 0\n\
             }}"
        ),
        AnalysisMode::Generic => format!(
            "Analyze this screen capture (frame {frame_num}, captured at {timestamp}).\n\
             Previous: {prev_summary}\n\
             Describe what is visible, identify any visual issues, errors, or anomalies.\n\
             \n\
             Return JSON:\n\
             {{\n\
               \"frame\": {frame_num},\n\
               \"state\": \"description of current state\",\n\
               \"issues\": [{{\"type\": \"glitch|layout|perf|ux\", \"description\": \"...\", \"severity\": \"low|medium|high\"}}],\n\
               \"changes_from_previous\": \"description\",\n\
               \"suggested_fixes\": [\"...\"],\n\
               \"overall_quality\": 0\n\
             }}"
        ),
    }
}

fn call_vlm(
    client: &reqwest::blocking::Client,
    endpoint: &str,
    model: &str,
    prompt: &str,
    image_b64: &str,
) -> Result<Value, String> {
    let body = json!({
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": format!("data:image/png;base64,{image_b64}")
                        }
                    },
                    {
                        "type": "text",
                        "text": prompt
                    }
                ]
            }
        ],
        "max_tokens": 2048,
        "temperature": 0.2
    });

    let resp = client
        .post(format!("{endpoint}/chat/completions"))
        .json(&body)
        .send()
        .map_err(|e| format!("VLM request failed: {e}"))?;

    let status = resp.status();
    let text = resp
        .text()
        .map_err(|e| format!("Read response body failed: {e}"))?;

    if !status.is_success() {
        return Err(format!("VLM returned {status}: {text}"));
    }

    let resp_json: Value =
        serde_json::from_str(&text).map_err(|e| format!("Parse VLM response failed: {e}"))?;

    // Extract the assistant message content
    let content = resp_json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();

    // Try to parse the content as JSON; if it fails, wrap it
    parse_analysis_json(&content)
}

/// Attempt to extract a JSON object from the VLM response text.
/// The model may wrap it in markdown code fences or include preamble text.
fn parse_analysis_json(raw: &str) -> Result<Value, String> {
    // Try direct parse first
    if let Ok(v) = serde_json::from_str::<Value>(raw) {
        if v.is_object() {
            return Ok(v);
        }
    }

    // Try to find JSON inside markdown code fences
    let json_str = if let Some(start) = raw.find("```json") {
        let after = &raw[start + 7..];
        if let Some(end) = after.find("```") {
            after[..end].trim()
        } else {
            after.trim()
        }
    } else if let Some(start) = raw.find("```") {
        let after = &raw[start + 3..];
        if let Some(end) = after.find("```") {
            after[..end].trim()
        } else {
            after.trim()
        }
    } else if let Some(start) = raw.find('{') {
        // Find the last closing brace
        if let Some(end) = raw.rfind('}') {
            &raw[start..=end]
        } else {
            raw.trim()
        }
    } else {
        raw.trim()
    };

    serde_json::from_str::<Value>(json_str)
        .map_err(|e| format!("Could not parse analysis JSON: {e}\nRaw: {raw}"))
}

// ---------------------------------------------------------------------------
// Summary report
// ---------------------------------------------------------------------------

struct RunStats {
    total_frames: u64,
    analyzed_frames: u64,
    skipped_frames: u64,
    failed_frames: u64,
    quality_scores: Vec<f64>,
    issue_counts: std::collections::HashMap<String, u64>,
    start_time: Instant,
}

impl RunStats {
    fn new() -> Self {
        Self {
            total_frames: 0,
            analyzed_frames: 0,
            skipped_frames: 0,
            failed_frames: 0,
            quality_scores: Vec::new(),
            issue_counts: std::collections::HashMap::new(),
            start_time: Instant::now(),
        }
    }

    fn record_analysis(&mut self, analysis: &Value) {
        self.analyzed_frames += 1;

        if let Some(q) = analysis["overall_quality"].as_f64() {
            self.quality_scores.push(q);
        }

        if let Some(issues) = analysis["issues"].as_array() {
            for issue in issues {
                let t = issue["type"].as_str().unwrap_or("unknown").to_string();
                *self.issue_counts.entry(t).or_insert(0) += 1;
            }
        }
    }

    fn print_report(&self) {
        let elapsed = self.start_time.elapsed();
        let avg_quality = if self.quality_scores.is_empty() {
            0.0
        } else {
            self.quality_scores.iter().sum::<f64>() / self.quality_scores.len() as f64
        };

        // Quality trend: compare first half vs second half
        let trend = if self.quality_scores.len() >= 4 {
            let mid = self.quality_scores.len() / 2;
            let first_avg: f64 = self.quality_scores[..mid].iter().sum::<f64>() / mid as f64;
            let second_avg: f64 = self.quality_scores[mid..].iter().sum::<f64>()
                / (self.quality_scores.len() - mid) as f64;
            if second_avg > first_avg + 0.5 {
                "improving"
            } else if second_avg < first_avg - 0.5 {
                "degrading"
            } else {
                "stable"
            }
        } else {
            "insufficient data"
        };

        eprintln!("\n========== ANALYSIS REPORT ==========");
        eprintln!("Duration:          {:.1}s", elapsed.as_secs_f64());
        eprintln!("Total frames:      {}", self.total_frames);
        eprintln!("Analyzed:          {}", self.analyzed_frames);
        eprintln!("Skipped:           {}", self.skipped_frames);
        eprintln!("Failed:            {}", self.failed_frames);
        eprintln!("Avg quality:       {avg_quality:.1}/10");
        eprintln!("Quality trend:     {trend}");
        eprintln!("Issues by type:");
        let mut sorted_issues: Vec<_> = self.issue_counts.iter().collect();
        sorted_issues.sort_by(|a, b| b.1.cmp(a.1));
        for (typ, count) in &sorted_issues {
            eprintln!("  {typ}: {count}");
        }
        eprintln!("=====================================\n");
    }
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

fn main() {
    let cfg = Config::from_args();

    eprintln!("[frame_analyzer] endpoint={}", cfg.endpoint);
    eprintln!("[frame_analyzer] model={}", cfg.model);
    eprintln!(
        "[frame_analyzer] fps={}, duration={}s",
        cfg.fps, cfg.duration
    );
    eprintln!("[frame_analyzer] mode={:?}", cfg.mode);
    eprintln!("[frame_analyzer] output={}", cfg.output.display());
    if cfg.record {
        eprintln!("[frame_analyzer] recording frames to PNG");
    }

    // Create output directory
    fs::create_dir_all(&cfg.output).expect("Failed to create output directory");

    // JSONL log file
    let jsonl_path = cfg.output.join("analysis.jsonl");
    let mut jsonl_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&jsonl_path)
        .expect("Failed to open JSONL log");

    // Ctrl+C handler
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        eprintln!("\n[frame_analyzer] Shutting down...");
        r.store(false, Ordering::SeqCst);
    })
    .expect("Failed to set Ctrl+C handler");

    // HTTP client with generous timeout for VLM inference
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .expect("Failed to build HTTP client");

    let frame_interval = Duration::from_secs_f64(1.0 / cfg.fps as f64);
    let total_duration = Duration::from_secs(cfg.duration);
    let run_start = Instant::now();
    let mut stats = RunStats::new();
    let mut prev_summary = "No previous frame.".to_string();
    let mut frame_num: u64 = 0;

    while running.load(Ordering::SeqCst) && run_start.elapsed() < total_duration {
        let frame_start = Instant::now();
        frame_num += 1;
        stats.total_frames += 1;

        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();

        // Capture screen
        let png_bytes = match capture_screen(cfg.region) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[frame {frame_num}] Capture error: {e}");
                stats.failed_frames += 1;
                sleep_remaining(frame_start, frame_interval);
                continue;
            }
        };

        // Save frame if recording
        if cfg.record {
            let path = cfg.output.join(format!("frame_{frame_num:04}.png"));
            if let Err(e) = fs::write(&path, &png_bytes) {
                eprintln!("[frame {frame_num}] Failed to save frame: {e}");
            }
        }

        let b64 = encode_base64(&png_bytes);
        let prompt = build_prompt(cfg.mode, frame_num, &timestamp, &prev_summary);

        eprintln!(
            "[frame {frame_num}] Captured {}KB, sending to VLM...",
            png_bytes.len() / 1024
        );

        // Call VLM
        let vlm_start = Instant::now();
        match call_vlm(&client, &cfg.endpoint, &cfg.model, &prompt, &b64) {
            Ok(analysis) => {
                let vlm_elapsed = vlm_start.elapsed();
                eprintln!(
                    "[frame {frame_num}] Analysis received in {:.1}s",
                    vlm_elapsed.as_secs_f64()
                );

                stats.record_analysis(&analysis);

                // Update previous summary for next frame's context
                prev_summary = analysis["state"]
                    .as_str()
                    .unwrap_or("Analysis completed.")
                    .to_string();

                // Write to JSONL
                let log_entry = json!({
                    "frame": frame_num,
                    "timestamp": timestamp,
                    "vlm_latency_ms": vlm_elapsed.as_millis(),
                    "analysis": analysis
                });
                if let Err(e) = writeln!(jsonl_file, "{}", log_entry) {
                    eprintln!("[frame {frame_num}] JSONL write error: {e}");
                }

                // Print issues inline
                if let Some(issues) = analysis["issues"].as_array() {
                    for issue in issues {
                        let sev = issue["severity"].as_str().unwrap_or("?");
                        let desc = issue["description"].as_str().unwrap_or("?");
                        let typ = issue["type"].as_str().unwrap_or("?");
                        eprintln!("  [{sev}] ({typ}) {desc}");
                    }
                }

                // Rate limiting: if VLM took longer than interval, skip frames
                if vlm_elapsed > frame_interval {
                    let frames_behind =
                        (vlm_elapsed.as_secs_f64() / frame_interval.as_secs_f64()).floor() as u64;
                    if frames_behind > 0 {
                        eprintln!(
                            "[frame {frame_num}] VLM latency ({:.1}s) exceeds interval ({:.1}s), \
                             skipping ~{frames_behind} frame(s)",
                            vlm_elapsed.as_secs_f64(),
                            frame_interval.as_secs_f64()
                        );
                        stats.skipped_frames += frames_behind;
                    }
                }
            }
            Err(e) => {
                eprintln!("[frame {frame_num}] VLM error: {e}");
                stats.failed_frames += 1;
            }
        }

        sleep_remaining(frame_start, frame_interval);
    }

    // Final report
    stats.print_report();

    // Write summary JSON
    let summary_path = cfg.output.join("summary.json");
    let avg_quality = if stats.quality_scores.is_empty() {
        0.0
    } else {
        stats.quality_scores.iter().sum::<f64>() / stats.quality_scores.len() as f64
    };
    let summary = json!({
        "total_frames": stats.total_frames,
        "analyzed_frames": stats.analyzed_frames,
        "skipped_frames": stats.skipped_frames,
        "failed_frames": stats.failed_frames,
        "avg_quality": avg_quality,
        "issue_counts": stats.issue_counts,
        "duration_secs": run_start.elapsed().as_secs_f64(),
        "config": {
            "endpoint": cfg.endpoint,
            "model": cfg.model,
            "fps": cfg.fps,
            "mode": format!("{:?}", cfg.mode),
        }
    });
    if let Err(e) = fs::write(
        &summary_path,
        serde_json::to_string_pretty(&summary).unwrap(),
    ) {
        eprintln!("Failed to write summary: {e}");
    } else {
        eprintln!(
            "[frame_analyzer] Summary written to {}",
            summary_path.display()
        );
    }

    eprintln!("[frame_analyzer] JSONL log at {}", jsonl_path.display());
}

fn sleep_remaining(start: Instant, interval: Duration) {
    let elapsed = start.elapsed();
    if elapsed < interval {
        std::thread::sleep(interval - elapsed);
    }
}
