//! Visual Polish Loop — end-to-end multimodal system test.
//!
//! Demonstrates selfware building a website, screenshotting it with headless
//! Chromium, analyzing the screenshot with a VLM, and iterating until the
//! visual quality meets a threshold.
//!
//! Requires:
//!   - A live VLM endpoint (default: https://crazyshit.ngrok.io/v1)
//!   - Chromium / chromium-browser installed
//!   - Feature flag: --features system-tests
//!
//! Run:  cargo run --bin visual_polish_test --features system-tests

use base64::Engine as _;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

fn endpoint() -> String {
    std::env::var("SELFWARE_ENDPOINT")
        .unwrap_or_else(|_| "https://crazyshit.ngrok.io/v1".to_string())
}

fn model() -> String {
    std::env::var("SELFWARE_MODEL")
        .unwrap_or_else(|_| "txn545/Qwen3.5-122B-A10B-NVFP4".to_string())
}

const MAX_ITERATIONS: usize = 5;
const SERVER_PORT: u16 = 8899;
const SCREENSHOT_PATH: &str = "/tmp/selfware_visual_test.png";
const SCORE_THRESHOLD: u64 = 8;

// ---------------------------------------------------------------------------
// Dimension names (kept in order for consistent reporting)
// ---------------------------------------------------------------------------

const DIMENSIONS: &[&str] = &[
    "layout_spacing",
    "typography_readability",
    "color_harmony",
    "visual_hierarchy",
    "responsiveness_indicators",
    "content_completeness",
    "professional_polish",
];

// ---------------------------------------------------------------------------
// HTTP helpers (blocking reqwest)
// ---------------------------------------------------------------------------

fn chat_completion(messages: &[Value]) -> Result<String, Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;

    let body = serde_json::json!({
        "model": model(),
        "messages": messages,
        "max_tokens": 8192,
        "temperature": 0.7,
    });

    let resp = client
        .post(format!("{}/chat/completions", endpoint()))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()?;

    let status = resp.status();
    let text = resp.text()?;
    if !status.is_success() {
        return Err(format!("LLM request failed ({}): {}", status, text).into());
    }

    let v: Value = serde_json::from_str(&text)?;
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();
    Ok(content)
}

fn vlm_analyze(image_base64: &str) -> Result<String, Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;

    let body = serde_json::json!({
        "model": model(),
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": format!("data:image/png;base64,{}", image_base64)
                        }
                    },
                    {
                        "type": "text",
                        "text": r#"Analyze this website screenshot. Score each dimension 1-10:
1. Layout & spacing
2. Typography & readability
3. Color harmony
4. Visual hierarchy
5. Responsiveness indicators
6. Content completeness
7. Professional polish

For each score below 8, provide a specific correction.
Return ONLY valid JSON (no markdown fences, no commentary) with this schema:
{
  "scores": {
    "layout_spacing": N,
    "typography_readability": N,
    "color_harmony": N,
    "visual_hierarchy": N,
    "responsiveness_indicators": N,
    "content_completeness": N,
    "professional_polish": N
  },
  "corrections": ["...", "..."],
  "overall": N,
  "done": bool
}"#
                    }
                ]
            }
        ],
        "max_tokens": 4096,
        "temperature": 0.3,
    });

    let resp = client
        .post(format!("{}/chat/completions", endpoint()))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()?;

    let status = resp.status();
    let text = resp.text()?;
    if !status.is_success() {
        return Err(format!("VLM request failed ({}): {}", status, text).into());
    }

    let v: Value = serde_json::from_str(&text)?;
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();
    Ok(content)
}

// ---------------------------------------------------------------------------
// Local HTTP server (serves a single HTML file)
// ---------------------------------------------------------------------------

fn start_server(
    html_path: PathBuf,
    shutdown: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", SERVER_PORT))
            .expect("Failed to bind server port");
        listener
            .set_nonblocking(true)
            .expect("Cannot set non-blocking");

        while !shutdown.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let html = fs::read_to_string(&html_path).unwrap_or_default();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        html.len(),
                        html
                    );
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.flush();
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    eprintln!("[server] accept error: {}", e);
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Screenshot via headless Chromium
// ---------------------------------------------------------------------------

fn take_screenshot() -> Result<(), Box<dyn std::error::Error>> {
    // Try common chromium binary names
    let candidates = ["chromium", "chromium-browser", "google-chrome", "chrome"];
    let browser = candidates.iter().find(|name| {
        Command::new("which")
            .arg(name)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    });

    let browser = match browser {
        Some(b) => *b,
        None => return Err("No Chromium/Chrome binary found in PATH".into()),
    };

    let output = Command::new(browser)
        .args([
            "--headless",
            "--disable-gpu",
            "--no-sandbox",
            &format!("--screenshot={}", SCREENSHOT_PATH),
            "--window-size=1920,1080",
            &format!("http://127.0.0.1:{}", SERVER_PORT),
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Chromium screenshot failed: {}", stderr).into());
    }

    // Give the file a moment to flush
    thread::sleep(Duration::from_millis(500));

    if !Path::new(SCREENSHOT_PATH).exists() {
        return Err("Screenshot file was not created".into());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Parse VLM JSON response (tolerant of markdown fences)
// ---------------------------------------------------------------------------

fn parse_vlm_response(raw: &str) -> Result<Value, Box<dyn std::error::Error>> {
    // Strip markdown code fences if present
    let trimmed = raw.trim();
    let json_str = if trimmed.starts_with("```") {
        let without_start = trimmed
            .strip_prefix("```json")
            .or_else(|| trimmed.strip_prefix("```"))
            .unwrap_or(trimmed);
        without_start
            .strip_suffix("```")
            .unwrap_or(without_start)
            .trim()
    } else {
        trimmed
    };

    // Try to find a JSON object in the text
    let start = json_str.find('{');
    let end = json_str.rfind('}');
    match (start, end) {
        (Some(s), Some(e)) if s < e => {
            let candidate = &json_str[s..=e];
            let v: Value = serde_json::from_str(candidate)?;
            Ok(v)
        }
        _ => Err(format!("No JSON object found in VLM response: {}", &raw[..raw.len().min(200)]).into()),
    }
}

// ---------------------------------------------------------------------------
// Website generation prompt
// ---------------------------------------------------------------------------

fn initial_prompt() -> String {
    r#"Create a modern, professional landing page as a single HTML file with all CSS inlined (in a <style> tag).

Requirements:
- Hero section with a bold headline and subtitle
- Feature cards section (3 cards with icons using Unicode/emoji)
- Call-to-action button with hover effect
- Footer with copyright
- Use a modern color scheme (dark backgrounds, accent colors)
- Responsive layout using CSS Grid or Flexbox
- Smooth typography (system font stack or Google Fonts via <link>)
- Subtle shadows and rounded corners
- At least 3 sections of content

Return ONLY the complete HTML code, nothing else. No markdown fences."#
        .to_string()
}

fn correction_prompt(current_html: &str, corrections: &[String]) -> String {
    format!(
        r#"Here is the current HTML for a landing page:

```html
{}
```

Apply the following visual corrections to improve the design:
{}

Return ONLY the updated complete HTML code, nothing else. No markdown fences."#,
        current_html,
        corrections
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{}. {}", i + 1, c))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Report {
    iterations: usize,
    score_history: Vec<HashMap<String, u64>>,
    corrections_applied: Vec<Vec<String>>,
    final_overall: u64,
    final_screenshot: String,
}

impl Report {
    fn print(&self) {
        println!("\n{}", "=".repeat(60));
        println!("  VISUAL POLISH LOOP — REPORT");
        println!("{}\n", "=".repeat(60));
        println!("Iterations:       {}", self.iterations);
        println!("Final overall:    {}/10", self.final_overall);
        println!("Final screenshot: {}\n", self.final_screenshot);

        println!("--- Score Progression ---");
        for dim in DIMENSIONS {
            let mut line = format!("  {:30}", dim);
            for scores in &self.score_history {
                let s = scores.get(*dim).copied().unwrap_or(0);
                line.push_str(&format!("  {}", s));
            }
            println!("{}", line);
        }

        if !self.corrections_applied.is_empty() {
            println!("\n--- Corrections Applied ---");
            for (i, corrs) in self.corrections_applied.iter().enumerate() {
                println!("  Iteration {}:", i + 1);
                for c in corrs {
                    println!("    - {}", c);
                }
            }
        }

        println!("\n{}", "=".repeat(60));
    }
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

fn main() {
    println!("[visual_polish_test] Starting visual polish loop");
    println!("  Endpoint: {}", endpoint());
    println!("  Model:    {}", model());
    println!("  Max iter: {}", MAX_ITERATIONS);
    println!();

    // Create temp directory for the website
    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let html_path = tmp_dir.path().join("index.html");

    // -- Iteration 1: Generate initial website --
    println!("[iter 1] Generating initial website...");
    let messages = vec![serde_json::json!({
        "role": "user",
        "content": initial_prompt()
    })];

    let html = match chat_completion(&messages) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("FATAL: Failed to generate initial website: {}", e);
            std::process::exit(1);
        }
    };

    // Strip any markdown fences the LLM may have added
    let html = strip_code_fences(&html);
    fs::write(&html_path, &html).expect("Failed to write index.html");
    println!("[iter 1] Website written to {}", html_path.display());

    // Start local server
    let shutdown = Arc::new(AtomicBool::new(false));
    let server_handle = start_server(html_path.clone(), shutdown.clone());

    // Give the server a moment to bind
    thread::sleep(Duration::from_millis(200));

    let mut report = Report::default();
    let mut current_html = html;

    for iteration in 1..=MAX_ITERATIONS {
        println!("\n[iter {}] Taking screenshot...", iteration);

        if let Err(e) = take_screenshot() {
            eprintln!("[iter {}] Screenshot failed: {}", iteration, e);
            eprintln!("  (This test requires headless Chromium installed)");
            break;
        }

        // Read screenshot as base64
        let mut img_bytes = Vec::new();
        {
            let mut f = fs::File::open(SCREENSHOT_PATH).expect("Cannot open screenshot");
            f.read_to_end(&mut img_bytes).expect("Cannot read screenshot");
        }
        let img_b64 = base64::engine::general_purpose::STANDARD.encode(&img_bytes);
        println!(
            "[iter {}] Screenshot size: {} bytes, base64 len: {}",
            iteration,
            img_bytes.len(),
            img_b64.len()
        );

        // Send to VLM for analysis
        println!("[iter {}] Sending to VLM for analysis...", iteration);
        let vlm_raw = match vlm_analyze(&img_b64) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[iter {}] VLM analysis failed: {}", iteration, e);
                break;
            }
        };

        let vlm_json = match parse_vlm_response(&vlm_raw) {
            Ok(v) => v,
            Err(e) => {
                eprintln!(
                    "[iter {}] Failed to parse VLM response: {}\n  Raw: {}",
                    iteration,
                    e,
                    &vlm_raw[..vlm_raw.len().min(300)]
                );
                break;
            }
        };

        // Extract scores
        let mut scores: HashMap<String, u64> = HashMap::new();
        if let Some(s) = vlm_json.get("scores").and_then(|v| v.as_object()) {
            for dim in DIMENSIONS {
                let val = s
                    .get(*dim)
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                scores.insert(dim.to_string(), val);
            }
        }

        let overall = vlm_json
            .get("overall")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let done = vlm_json
            .get("done")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let corrections: Vec<String> = vlm_json
            .get("corrections")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        println!("[iter {}] Overall: {}/10, done: {}", iteration, overall, done);
        for dim in DIMENSIONS {
            let s = scores.get(*dim).copied().unwrap_or(0);
            println!("  {:30} {}/10", dim, s);
        }
        if !corrections.is_empty() {
            println!("[iter {}] Corrections:", iteration);
            for c in &corrections {
                println!("  - {}", c);
            }
        }

        report.score_history.push(scores);
        report.iterations = iteration;
        report.final_overall = overall;
        report.final_screenshot = SCREENSHOT_PATH.to_string();

        // Check if we're done
        if done || overall >= SCORE_THRESHOLD {
            println!("\n[iter {}] Quality threshold met! Stopping.", iteration);
            break;
        }

        if iteration == MAX_ITERATIONS {
            println!("\n[iter {}] Max iterations reached. Stopping.", iteration);
            break;
        }

        // Apply corrections
        println!(
            "\n[iter {}] Applying {} corrections...",
            iteration,
            corrections.len()
        );
        report.corrections_applied.push(corrections.clone());

        let fix_messages = vec![serde_json::json!({
            "role": "user",
            "content": correction_prompt(&current_html, &corrections)
        })];

        match chat_completion(&fix_messages) {
            Ok(new_html) => {
                let new_html = strip_code_fences(&new_html);
                fs::write(&html_path, &new_html).expect("Failed to write updated HTML");
                current_html = new_html;
                println!("[iter {}] Updated HTML written.", iteration);
                // Brief pause so server picks up the new file
                thread::sleep(Duration::from_millis(300));
            }
            Err(e) => {
                eprintln!("[iter {}] Failed to get corrections from LLM: {}", iteration, e);
                break;
            }
        }
    }

    // Shutdown server
    shutdown.store(true, Ordering::Relaxed);
    let _ = server_handle.join();

    // Copy final screenshot to a more permanent location
    let final_path = format!(
        "/tmp/selfware_visual_test_final_{}.png",
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    );
    if Path::new(SCREENSHOT_PATH).exists() {
        let _ = fs::copy(SCREENSHOT_PATH, &final_path);
        report.final_screenshot = final_path;
    }

    report.print();
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn strip_code_fences(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.starts_with("```") {
        let without_start = trimmed
            .strip_prefix("```html")
            .or_else(|| trimmed.strip_prefix("```"))
            .unwrap_or(trimmed);
        without_start
            .strip_suffix("```")
            .unwrap_or(without_start)
            .trim()
            .to_string()
    } else {
        trimmed.to_string()
    }
}
