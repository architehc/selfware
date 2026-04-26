//! Computer control integration test — drives desktop applications via xdotool,
//! captures screenshots with xcap, and verifies each step through a VLM endpoint.
//!
//! This is a standalone binary gated behind `--features system-tests`.
//!
//! Usage:
//! ```sh
//! cargo run --bin computer_control_test --features system-tests -- \
//!   --scenario browser \
//!   --output /tmp/cc_test_output
//! ```
//!
//! Environment overrides:
//!   CC_TEST_ENDPOINT  — VLM endpoint (default: http://127.0.0.1:8000/v1)
//!   CC_TEST_MODEL     — VLM model   (default: txn545/Qwen3.5-122B-A10B-NVFP4)

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use base64::Engine as _;
use clap::Parser;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(
    name = "computer_control_test",
    about = "Desktop control + VLM verification"
)]
struct Args {
    /// Which scenario to run: browser, editor, window, all
    #[arg(long, default_value = "all")]
    scenario: String,

    /// Output directory for screenshots and report
    #[arg(long, default_value = "/tmp/cc_test_output")]
    output: PathBuf,

    /// VLM endpoint (OpenAI-compatible /v1). Override with CC_TEST_ENDPOINT env var.
    #[arg(long, default_value = "http://127.0.0.1:8000/v1")]
    endpoint: String,

    /// VLM model name. Override with CC_TEST_MODEL env var.
    #[arg(long, default_value = "txn545/Qwen3.5-122B-A10B-NVFP4")]
    model: String,

    /// Timeout per VLM request in seconds
    #[arg(long, default_value_t = 120)]
    vlm_timeout: u64,
}

// ---------------------------------------------------------------------------
// Action / Step types
// ---------------------------------------------------------------------------

/// A desktop action the test can perform.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum Action {
    /// Launch a process with the given command line.
    Launch { cmd: String, args: Vec<String> },
    /// Move mouse to (x,y) and left-click.
    Click { x: i32, y: i32 },
    /// Type text via xdotool with inter-key delay.
    Type { text: String },
    /// Send a key combo, e.g. "ctrl+l", "Return".
    KeyCombo { keys: String },
    /// Sleep for the given number of milliseconds.
    Wait { ms: u64 },
    /// Capture a screenshot (always performed after every non-Wait action too).
    Screenshot,
}

/// One step in a test scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ControlStep {
    /// Human description of what this step does.
    description: String,
    /// The action to execute.
    action: Action,
    /// What the VLM should see after this step (empty = no verification).
    expected: String,
    /// Path where the screenshot was saved.
    screenshot_path: PathBuf,
    /// Raw VLM response text.
    vlm_response: Option<String>,
    /// Whether the VLM judged the step as passing.
    passed: bool,
    /// Wall-clock duration of this step (action + verification).
    duration_ms: u64,
}

/// Parsed VLM verification response.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct VlmVerdict {
    passed: bool,
    actual: String,
    issues: Vec<String>,
}

// ---------------------------------------------------------------------------
// Action execution
// ---------------------------------------------------------------------------

fn run_xdotool(args: &[&str]) -> Result<String> {
    let output = Command::new("xdotool")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("Failed to run xdotool — is it installed?")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("xdotool {:?} failed: {}", args, stderr);
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn execute_action(action: &Action) -> Result<()> {
    match action {
        Action::Launch { cmd, args } => {
            // Spawn detached — we don't wait for exit.
            Command::new(cmd)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .with_context(|| format!("Failed to launch {cmd}"))?;
            Ok(())
        }
        Action::Click { x, y } => {
            run_xdotool(&["mousemove", &x.to_string(), &y.to_string()])?;
            run_xdotool(&["click", "1"])?;
            Ok(())
        }
        Action::Type { text } => {
            run_xdotool(&["type", "--delay", "50", text])?;
            Ok(())
        }
        Action::KeyCombo { keys } => {
            run_xdotool(&["key", keys])?;
            Ok(())
        }
        Action::Wait { ms } => {
            std::thread::sleep(Duration::from_millis(*ms));
            Ok(())
        }
        Action::Screenshot => {
            // Screenshot is taken separately; this is a no-op action marker.
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Screenshot capture (xcap)
// ---------------------------------------------------------------------------

fn capture_screenshot(path: &Path) -> Result<()> {
    let monitors =
        xcap::Monitor::all().map_err(|e| anyhow::anyhow!("Failed to enumerate monitors: {e}"))?;
    let monitor = monitors
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No monitors found"))?;
    let image = monitor
        .capture_image()
        .map_err(|e| anyhow::anyhow!("Screen capture failed: {e}"))?;
    image
        .save(path)
        .map_err(|e| anyhow::anyhow!("Failed to save screenshot: {e}"))?;
    Ok(())
}

fn load_screenshot_base64(path: &Path) -> Result<String> {
    let data = std::fs::read(path).context("Reading screenshot file")?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&data))
}

// ---------------------------------------------------------------------------
// VLM verification
// ---------------------------------------------------------------------------

fn vlm_verify(
    client: &reqwest::blocking::Client,
    endpoint: &str,
    model: &str,
    screenshot_b64: &str,
    description: &str,
    expected: &str,
) -> Result<VlmVerdict> {
    let prompt = format!(
        "You are verifying a computer control test step.\n\
         Step: {description}\n\
         Expected outcome: {expected}\n\n\
         Look at this screenshot and determine:\n\
         1. Did the expected outcome occur? (yes/no)\n\
         2. What do you actually see?\n\
         3. Any unexpected elements or errors?\n\n\
         Return ONLY valid JSON (no markdown fences): \
         {{\"passed\": bool, \"actual\": \"description\", \"issues\": [...]}}"
    );

    let body = serde_json::json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": [
                {
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:image/png;base64,{screenshot_b64}"),
                        "detail": "low"
                    }
                },
                {
                    "type": "text",
                    "text": prompt
                }
            ]
        }],
        "max_tokens": 512,
        "temperature": 0.0,
    });

    let url = format!("{}/chat/completions", endpoint.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .context("VLM request failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        bail!("VLM returned {status}: {text}");
    }

    let resp_json: serde_json::Value = resp.json().context("VLM response is not JSON")?;

    let content = resp_json
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Try to parse JSON from the content (strip markdown fences if present).
    let json_str = content
        .trim()
        .strip_prefix("```json")
        .or_else(|| content.trim().strip_prefix("```"))
        .unwrap_or(content.trim())
        .strip_suffix("```")
        .unwrap_or(content.trim())
        .trim();

    match serde_json::from_str::<VlmVerdict>(json_str) {
        Ok(verdict) => Ok(verdict),
        Err(_) => {
            // Fallback: treat any response as an informational pass/fail heuristic.
            let lower = content.to_lowercase();
            Ok(VlmVerdict {
                passed: lower.contains("\"passed\": true")
                    || lower.contains("\"passed\":true")
                    || (lower.contains("yes") && !lower.contains("\"passed\": false")),
                actual: content.clone(),
                issues: vec![format!("VLM response was not valid JSON: {content}")],
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Scenario definitions
// ---------------------------------------------------------------------------

fn scenario_browser() -> Vec<ControlStep> {
    let steps = vec![
        (
            "Launch Chromium to about:blank",
            Action::Launch {
                cmd: "chromium".into(),
                args: vec![
                    "--no-first-run".into(),
                    "--no-default-browser-check".into(),
                    "about:blank".into(),
                ],
            },
            "",
        ),
        ("Wait for browser to open", Action::Wait { ms: 2000 }, ""),
        (
            "Verify empty browser page",
            Action::Screenshot,
            "An empty browser window is visible showing about:blank or a blank page",
        ),
        (
            "Focus address bar",
            Action::KeyCombo {
                keys: "ctrl+l".into(),
            },
            "",
        ),
        (
            "Type URL",
            Action::Type {
                text: "https://example.com".into(),
            },
            "",
        ),
        (
            "Press Enter to navigate",
            Action::KeyCombo {
                keys: "Return".into(),
            },
            "",
        ),
        ("Wait for page load", Action::Wait { ms: 3000 }, ""),
        (
            "Verify example.com loaded",
            Action::Screenshot,
            "The browser shows example.com with the heading 'Example Domain'",
        ),
    ];

    steps
        .into_iter()
        .enumerate()
        .map(|(i, (desc, action, expected))| ControlStep {
            description: desc.to_string(),
            action,
            expected: expected.to_string(),
            screenshot_path: PathBuf::from(format!("step_{i}_{}.png", slug(desc))),
            vlm_response: None,
            passed: false,
            duration_ms: 0,
        })
        .collect()
}

fn scenario_editor() -> Vec<ControlStep> {
    let steps = vec![
        (
            "Launch terminal with nano",
            Action::Launch {
                cmd: "xterm".into(),
                args: vec!["-e".into(), "nano".into(), "/tmp/cc_test_note.txt".into()],
            },
            "",
        ),
        ("Wait for editor to open", Action::Wait { ms: 2000 }, ""),
        (
            "Verify editor is open",
            Action::Screenshot,
            "A terminal window is visible with the nano text editor open",
        ),
        (
            "Type test text",
            Action::Type {
                text: "Hello from selfware computer control test".into(),
            },
            "",
        ),
        ("Wait for text entry", Action::Wait { ms: 500 }, ""),
        (
            "Verify text was entered",
            Action::Screenshot,
            "The nano editor shows the text 'Hello from selfware computer control test'",
        ),
        (
            "Save file with Ctrl+O",
            Action::KeyCombo {
                keys: "ctrl+o".into(),
            },
            "",
        ),
        (
            "Confirm filename with Enter",
            Action::KeyCombo {
                keys: "Return".into(),
            },
            "",
        ),
        (
            "Exit nano with Ctrl+X",
            Action::KeyCombo {
                keys: "ctrl+x".into(),
            },
            "",
        ),
    ];

    steps
        .into_iter()
        .enumerate()
        .map(|(i, (desc, action, expected))| ControlStep {
            description: desc.to_string(),
            action,
            expected: expected.to_string(),
            screenshot_path: PathBuf::from(format!("step_{i}_{}.png", slug(desc))),
            vlm_response: None,
            passed: false,
            duration_ms: 0,
        })
        .collect()
}

fn scenario_window() -> Vec<ControlStep> {
    let steps = vec![
        (
            "Launch xterm window",
            Action::Launch {
                cmd: "xterm".into(),
                args: vec!["-title".into(), "CC_Test_Window".into()],
            },
            "",
        ),
        ("Wait for window", Action::Wait { ms: 1500 }, ""),
        (
            "Verify window is open",
            Action::Screenshot,
            "A terminal window titled CC_Test_Window or an xterm window is visible",
        ),
        (
            "Move window with wmctrl",
            Action::Launch {
                cmd: "wmctrl".into(),
                args: vec![
                    "-r".into(),
                    "CC_Test_Window".into(),
                    "-e".into(),
                    "0,100,100,800,600".into(),
                ],
            },
            "",
        ),
        ("Wait for move", Action::Wait { ms: 500 }, ""),
        (
            "Verify window repositioned",
            Action::Screenshot,
            "The terminal window has been moved/resized and is visible at a different position",
        ),
    ];

    steps
        .into_iter()
        .enumerate()
        .map(|(i, (desc, action, expected))| ControlStep {
            description: desc.to_string(),
            action,
            expected: expected.to_string(),
            screenshot_path: PathBuf::from(format!("step_{i}_{}.png", slug(desc))),
            vlm_response: None,
            passed: false,
            duration_ms: 0,
        })
        .collect()
}

/// Produce a filename-safe slug from a description.
fn slug(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

fn run_scenario(
    name: &str,
    steps: &mut [ControlStep],
    output_dir: &Path,
    client: &reqwest::blocking::Client,
    endpoint: &str,
    model: &str,
) {
    let scenario_dir = output_dir.join(name);
    std::fs::create_dir_all(&scenario_dir).expect("create scenario output dir");

    let sep = "=".repeat(60);
    println!("\n{sep}");
    println!("  Scenario: {name}");
    println!("{sep}\n");

    for (i, step) in steps.iter_mut().enumerate() {
        let t0 = Instant::now();

        // Resolve screenshot path relative to scenario dir.
        let screenshot_file = scenario_dir.join(&step.screenshot_path);
        step.screenshot_path = screenshot_file.clone();

        println!("  [{i}] {}", step.description);

        // Execute the action.
        if let Err(e) = execute_action(&step.action) {
            eprintln!("      ACTION ERROR: {e}");
            step.passed = false;
            step.duration_ms = t0.elapsed().as_millis() as u64;
            continue;
        }

        // If this step has an expected outcome, take a screenshot and verify.
        if !step.expected.is_empty() {
            // Small grace period for UI to settle.
            std::thread::sleep(Duration::from_millis(300));

            match capture_screenshot(&screenshot_file) {
                Ok(()) => println!("      screenshot -> {}", screenshot_file.display()),
                Err(e) => {
                    eprintln!("      SCREENSHOT ERROR: {e}");
                    step.passed = false;
                    step.duration_ms = t0.elapsed().as_millis() as u64;
                    continue;
                }
            }

            match load_screenshot_base64(&screenshot_file) {
                Ok(b64) => {
                    match vlm_verify(
                        client,
                        endpoint,
                        model,
                        &b64,
                        &step.description,
                        &step.expected,
                    ) {
                        Ok(verdict) => {
                            step.passed = verdict.passed;
                            step.vlm_response =
                                Some(serde_json::to_string(&verdict).unwrap_or_default());
                            println!(
                                "      VLM: {} — {}",
                                if verdict.passed { "PASS" } else { "FAIL" },
                                verdict.actual
                            );
                            if !verdict.issues.is_empty() {
                                for issue in &verdict.issues {
                                    println!("        issue: {issue}");
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("      VLM ERROR: {e}");
                            step.passed = false;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("      BASE64 ERROR: {e}");
                    step.passed = false;
                }
            }
        } else {
            // No verification needed — mark as passed (action-only step).
            step.passed = true;
        }

        step.duration_ms = t0.elapsed().as_millis() as u64;
    }
}

// ---------------------------------------------------------------------------
// Report generation
// ---------------------------------------------------------------------------

fn generate_report(scenarios: &[(&str, &[ControlStep])], output_dir: &Path) -> Result<PathBuf> {
    let report_path = output_dir.join("report.md");
    let mut md = String::new();

    md.push_str("# Computer Control Test Report\n\n");
    md.push_str(&format!(
        "Generated: {}\n\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    ));

    let mut total_steps = 0usize;
    let mut total_verified = 0usize;
    let mut total_passed = 0usize;

    for (name, steps) in scenarios {
        md.push_str(&format!("## Scenario: {name}\n\n"));
        md.push_str("| # | Action | Expected | Actual | Pass | Time |\n");
        md.push_str("|---|--------|----------|--------|------|------|\n");

        for (i, step) in steps.iter().enumerate() {
            total_steps += 1;
            let is_verified = !step.expected.is_empty();
            if is_verified {
                total_verified += 1;
                if step.passed {
                    total_passed += 1;
                }
            }

            let actual = step
                .vlm_response
                .as_deref()
                .and_then(|r| serde_json::from_str::<VlmVerdict>(r).ok())
                .map(|v| v.actual)
                .unwrap_or_else(|| {
                    if is_verified {
                        "—".into()
                    } else {
                        "(no verification)".into()
                    }
                });

            let pass_str = if !is_verified {
                "n/a".to_string()
            } else if step.passed {
                "PASS".to_string()
            } else {
                "FAIL".to_string()
            };

            md.push_str(&format!(
                "| {i} | {} | {} | {} | {pass_str} | {}ms |\n",
                step.description,
                if step.expected.is_empty() {
                    "—"
                } else {
                    &step.expected
                },
                actual,
                step.duration_ms
            ));
        }

        md.push('\n');

        // List screenshot paths.
        md.push_str("### Screenshots\n\n");
        for step in *steps {
            if step.screenshot_path.exists() {
                md.push_str(&format!("- `{}`\n", step.screenshot_path.display()));
            }
        }
        md.push('\n');
    }

    // Summary
    md.push_str("## Summary\n\n");
    md.push_str(&format!("- Total steps: {total_steps}\n"));
    md.push_str(&format!("- Verified steps: {total_verified}\n"));
    md.push_str(&format!("- Passed: {total_passed}\n"));
    md.push_str(&format!("- Failed: {}\n", total_verified - total_passed));
    if total_verified > 0 {
        let pct = (total_passed as f64 / total_verified as f64) * 100.0;
        md.push_str(&format!("- Pass rate: {pct:.1}%\n"));
    }

    std::fs::write(&report_path, &md)?;
    Ok(report_path)
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    let mut args = Args::parse();

    // Allow env vars to override CLI defaults.
    if let Ok(ep) = std::env::var("CC_TEST_ENDPOINT") {
        args.endpoint = ep;
    }
    if let Ok(m) = std::env::var("CC_TEST_MODEL") {
        args.model = m;
    }

    // Initialize tracing.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    std::fs::create_dir_all(&args.output)?;

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(args.vlm_timeout))
        .build()?;

    // Collect the scenarios to run.
    let run_browser = args.scenario == "all" || args.scenario == "browser";
    let run_editor = args.scenario == "all" || args.scenario == "editor";
    let run_window = args.scenario == "all" || args.scenario == "window";

    let mut browser_steps = if run_browser {
        scenario_browser()
    } else {
        vec![]
    };
    let mut editor_steps = if run_editor {
        scenario_editor()
    } else {
        vec![]
    };
    let mut window_steps = if run_window {
        scenario_window()
    } else {
        vec![]
    };

    if run_browser {
        run_scenario(
            "browser",
            &mut browser_steps,
            &args.output,
            &client,
            &args.endpoint,
            &args.model,
        );
    }
    if run_editor {
        run_scenario(
            "editor",
            &mut editor_steps,
            &args.output,
            &client,
            &args.endpoint,
            &args.model,
        );
    }
    if run_window {
        run_scenario(
            "window",
            &mut window_steps,
            &args.output,
            &client,
            &args.endpoint,
            &args.model,
        );
    }

    // Build report input.
    let mut scenario_refs: Vec<(&str, &[ControlStep])> = Vec::new();
    if run_browser {
        scenario_refs.push(("browser", &browser_steps));
    }
    if run_editor {
        scenario_refs.push(("editor", &editor_steps));
    }
    if run_window {
        scenario_refs.push(("window", &window_steps));
    }

    let report_path = generate_report(&scenario_refs, &args.output)?;
    println!("\nReport written to: {}", report_path.display());

    // Exit with non-zero if any verified step failed.
    let any_failure = scenario_refs
        .iter()
        .any(|(_, steps)| steps.iter().any(|s| !s.expected.is_empty() && !s.passed));

    if any_failure {
        std::process::exit(1);
    }

    Ok(())
}
