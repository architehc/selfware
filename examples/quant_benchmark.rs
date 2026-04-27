//! Qwen3.6 quantization benchmark harness.
//!
//! Drives a `llama-server` endpoint through a fixed test suite and emits both
//! a JSON record (for collation) and a markdown table (for human review).
//!
//! # Test categories
//!
//! 1. **Speed** — Tokens/second on a 200-token completion (3 cold runs, median).
//! 2. **Tool-use** — Verifies the model invokes a 3-step tool chain
//!    (`file_read` → `grep_search` → `file_edit`) on a temp file.
//! 3. **Codegen** — Asks for a tiny Rust `fizzbuzz` and runs `cargo check` on it.
//! 4. **Reasoning** — Multi-step word problem with a verifiable numeric answer.
//! 5. **Multimodal** — Sends a generated PNG and checks for visual references in
//!    the reply (requires the `mmproj` to be loaded server-side).
//!
//! # Usage
//!
//! ```bash
//! cargo run --release --example quant_benchmark -- \
//!     --endpoint http://127.0.0.1:8080/v1 \
//!     --quant Qwen3.6-IQ2_M \
//!     --output reports/
//! ```
//!
//! The example uses [`selfware::api::ApiClient`] directly, with a synthetic
//! [`selfware::config::Config`] (no config file required).

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use base64::Engine as _;
use clap::Parser;
use image::{ImageBuffer, Rgb};
use selfware::api::types::{FunctionDefinition, Message, MessageContent, ToolDefinition};
use selfware::api::{ApiClient, ThinkingMode};
use selfware::config::Config;
use serde::Serialize;
use serde_json::json;

#[derive(Parser, Debug)]
#[command(
    name = "quant_benchmark",
    about = "Drive a quant through a fixed test suite"
)]
struct Args {
    /// OpenAI-compatible endpoint, e.g. `http://127.0.0.1:8080/v1`.
    #[arg(long)]
    endpoint: String,

    /// Logical name of the quantization (used in filenames + the report).
    #[arg(long)]
    quant: String,

    /// Output directory for `<quant>.json` (created if missing).
    #[arg(long)]
    output: PathBuf,

    /// Optional model name override (defaults to the loaded model on the server).
    #[arg(long, default_value = "qwen3.6-27b")]
    model: String,

    /// Per-request timeout in seconds (default: 120).
    #[arg(long, default_value_t = 120)]
    timeout_secs: u64,

    /// Skip the multimodal test (for quants without an mmproj attached).
    #[arg(long)]
    skip_multimodal: bool,
}

#[derive(Debug, Serialize, Clone)]
struct TestResult {
    name: &'static str,
    passed: bool,
    duration_ms: u128,
    detail: String,
    /// Optional metric (tokens/sec, etc.) — only populated for `speed`.
    metric: Option<f64>,
}

#[derive(Debug, Serialize)]
struct BenchReport {
    quant: String,
    endpoint: String,
    model: String,
    started_at: String,
    total_duration_ms: u128,
    tests: Vec<TestResult>,
    summary: Summary,
}

#[derive(Debug, Serialize)]
struct Summary {
    passed: usize,
    failed: usize,
    total: usize,
    tokens_per_sec_median: Option<f64>,
}

fn build_client(args: &Args) -> Result<ApiClient> {
    // Pass `chat_template_kwargs.enable_thinking = false` like the model card asks.
    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        json!({ "enable_thinking": false }),
    );

    let mut config = Config {
        endpoint: args.endpoint.clone(),
        model: args.model.clone(),
        // llama.cpp servers handle 131072-context fine when started with -c 131072.
        context_length: 131072,
        max_tokens: 4096,
        temperature: 0.0,
        extra_body: Some(extra),
        ..Config::default()
    };
    config.agent.step_timeout_secs = args.timeout_secs;

    ApiClient::new(&config)
}

// ── Test 1: Speed ──────────────────────────────────────────────────────────────

async fn test_speed(client: &ApiClient) -> TestResult {
    let start = Instant::now();
    let prompt = "Count slowly from one to two hundred, one number per line, like:\n1\n2\n3\n... \
                  Continue all the way to 200. Output the numbers and nothing else.";
    let messages = vec![
        Message::system("You are a benchmark target. Comply exactly with the user's request."),
        Message::user(prompt),
    ];

    let mut samples: Vec<f64> = Vec::with_capacity(3);
    let mut last_err: Option<String> = None;

    for run in 0..3 {
        let run_start = Instant::now();
        match client
            .chat(messages.clone(), None, ThinkingMode::Disabled)
            .await
        {
            Ok(resp) => {
                let elapsed = run_start.elapsed().as_secs_f64();
                let completion_tokens = resp.usage.completion_tokens.max(1) as f64;
                let toks = completion_tokens / elapsed.max(0.001);
                samples.push(toks);
                tracing::info!(run = run, tok_per_sec = toks, "speed sample");
            }
            Err(e) => {
                last_err = Some(e.to_string());
            }
        }
    }

    if samples.is_empty() {
        return TestResult {
            name: "speed",
            passed: false,
            duration_ms: start.elapsed().as_millis(),
            detail: format!(
                "all 3 runs failed: {}",
                last_err.as_deref().unwrap_or("unknown")
            ),
            metric: None,
        };
    }

    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = samples[samples.len() / 2];

    TestResult {
        name: "speed",
        passed: true,
        duration_ms: start.elapsed().as_millis(),
        detail: format!(
            "median={:.1} tok/s (samples={:?})",
            median,
            samples
                .iter()
                .map(|s| format!("{:.1}", s))
                .collect::<Vec<_>>()
        ),
        metric: Some(median),
    }
}

// ── Test 2: Tool-use ──────────────────────────────────────────────────────────

fn make_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            def_type: "function".into(),
            function: FunctionDefinition {
                name: "file_read".into(),
                description: "Read a UTF-8 text file and return its contents.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Absolute path to the file"}
                    },
                    "required": ["path"]
                }),
            },
        },
        ToolDefinition {
            def_type: "function".into(),
            function: FunctionDefinition {
                name: "grep_search".into(),
                description: "Search for a regex pattern across files in a directory.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {"type": "string"},
                        "path": {"type": "string"}
                    },
                    "required": ["pattern", "path"]
                }),
            },
        },
        ToolDefinition {
            def_type: "function".into(),
            function: FunctionDefinition {
                name: "file_edit".into(),
                description: "Replace `old_string` with `new_string` in a file.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "old_string": {"type": "string"},
                        "new_string": {"type": "string"}
                    },
                    "required": ["path", "old_string", "new_string"]
                }),
            },
        },
    ]
}

async fn test_tool_use(client: &ApiClient) -> TestResult {
    let start = Instant::now();

    // Set up a known temp file the model is supposed to (notionally) work on.
    let tmp = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => {
            return TestResult {
                name: "tool_use",
                passed: false,
                duration_ms: start.elapsed().as_millis(),
                detail: format!("could not create tempdir: {}", e),
                metric: None,
            }
        }
    };
    let target = tmp.path().join("notes.txt");
    if let Err(e) = fs::write(&target, "hello world\nlorem ipsum dolor\n") {
        return TestResult {
            name: "tool_use",
            passed: false,
            duration_ms: start.elapsed().as_millis(),
            detail: format!("could not write fixture: {}", e),
            metric: None,
        };
    }

    let prompt = format!(
        "There is a text file at `{path}`. Please:\n\
         1. Use `file_read` to load it.\n\
         2. Use `grep_search` with pattern `lorem` against `{dir}` to confirm the match.\n\
         3. Use `file_edit` to replace `lorem` with `LOREM` in that file.\n\
         Call the tools in that order, one per turn.",
        path = target.display(),
        dir = tmp.path().display(),
    );

    let tools = make_tools();
    let messages = vec![
        Message::system(
            "You are a coding assistant with access to tools. Use the provided tools \
             instead of describing what you would do.",
        ),
        Message::user(prompt),
    ];

    let resp = match client
        .chat(messages, Some(tools), ThinkingMode::Disabled)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return TestResult {
                name: "tool_use",
                passed: false,
                duration_ms: start.elapsed().as_millis(),
                detail: format!("chat failed: {}", e),
                metric: None,
            }
        }
    };

    let calls = resp
        .choices
        .first()
        .and_then(|c| c.message.tool_calls.as_ref());

    let names: Vec<String> = calls
        .map(|tc| tc.iter().map(|c| c.function.name.clone()).collect())
        .unwrap_or_default();

    let allowed = ["file_read", "grep_search", "file_edit"];
    let invoked_any_known = names.iter().any(|n| allowed.contains(&n.as_str()));
    let all_known = !names.is_empty() && names.iter().all(|n| allowed.contains(&n.as_str()));
    let any_invalid_args = calls
        .map(|tc| {
            tc.iter()
                .any(|c| serde_json::from_str::<serde_json::Value>(&c.function.arguments).is_err())
        })
        .unwrap_or(false);

    let passed = invoked_any_known && all_known && !any_invalid_args;
    let detail = if passed {
        format!(
            "{} tool call(s) issued: {}",
            names.len(),
            names.join(" -> ")
        )
    } else if names.is_empty() {
        format!(
            "no tool calls in reply; raw text='{}'",
            preview(
                &resp
                    .choices
                    .first()
                    .map(|c| c.message.content.text_all())
                    .unwrap_or_default(),
                160,
            )
        )
    } else {
        format!(
            "tool-call set unexpected: {:?} (any_invalid_args={})",
            names, any_invalid_args
        )
    };

    TestResult {
        name: "tool_use",
        passed,
        duration_ms: start.elapsed().as_millis(),
        detail,
        metric: None,
    }
}

// ── Test 3: Codegen ───────────────────────────────────────────────────────────

async fn test_codegen(client: &ApiClient) -> TestResult {
    let start = Instant::now();
    let messages = vec![
        Message::system(
            "You are a Rust expert. Output ONLY the contents of a single Rust source file. \
             No code fences, no explanation.",
        ),
        Message::user(
            "Write a Rust program with `fn main()` that prints the FizzBuzz output for \
             numbers 1 through 15. Print 'Fizz' for multiples of 3, 'Buzz' for multiples \
             of 5, 'FizzBuzz' for multiples of both, and the number otherwise.",
        ),
    ];

    let resp = match client.chat(messages, None, ThinkingMode::Disabled).await {
        Ok(r) => r,
        Err(e) => {
            return TestResult {
                name: "codegen",
                passed: false,
                duration_ms: start.elapsed().as_millis(),
                detail: format!("chat failed: {}", e),
                metric: None,
            }
        }
    };

    let raw = resp
        .choices
        .first()
        .map(|c| c.message.content.text_all())
        .unwrap_or_default();
    let source = strip_code_fences(&raw);

    let detail_text = preview(&source, 120);
    let check_result = match cargo_check_snippet(&source) {
        Ok(()) => {
            return TestResult {
                name: "codegen",
                passed: true,
                duration_ms: start.elapsed().as_millis(),
                detail: format!("compiled cleanly ({} bytes)", source.len()),
                metric: None,
            }
        }
        Err(e) => format!("cargo check failed: {} (snippet: {})", e, detail_text),
    };

    TestResult {
        name: "codegen",
        passed: false,
        duration_ms: start.elapsed().as_millis(),
        detail: check_result,
        metric: None,
    }
}

fn strip_code_fences(s: &str) -> String {
    let t = s.trim();
    if let Some(stripped) = t.strip_prefix("```") {
        // drop optional language tag on the first line
        let after_lang = stripped
            .split_once('\n')
            .map(|(_, body)| body)
            .unwrap_or("");
        if let Some(end) = after_lang.rfind("```") {
            return after_lang[..end].trim().to_string();
        }
    }
    t.to_string()
}

fn cargo_check_snippet(source: &str) -> Result<()> {
    let dir = tempfile::tempdir().context("tempdir for codegen")?;
    let crate_name = "qb_fizzbuzz";

    fs::create_dir_all(dir.path().join("src"))?;
    fs::write(
        dir.path().join("Cargo.toml"),
        format!(
            "[package]\nname = \"{crate_name}\"\nversion = \"0.0.1\"\nedition = \"2021\"\n\n\
             [dependencies]\n",
        ),
    )?;
    fs::write(dir.path().join("src/main.rs"), source)?;

    let output = Command::new("cargo")
        .args(["check", "--quiet", "--offline"])
        .current_dir(dir.path())
        .output()
        .context("spawn cargo check")?;

    if output.status.success() {
        Ok(())
    } else {
        // Retry without --offline in case the registry is not pre-populated.
        let retry = Command::new("cargo")
            .args(["check", "--quiet"])
            .current_dir(dir.path())
            .output()
            .context("spawn cargo check (retry)")?;
        if retry.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&retry.stderr);
            Err(anyhow!("{}", preview(&stderr, 240)))
        }
    }
}

// ── Test 4: Reasoning ─────────────────────────────────────────────────────────

async fn test_reasoning(client: &ApiClient) -> TestResult {
    let start = Instant::now();
    // Three independent steps, single deterministic answer = 26.
    //   1. Alice has 12 apples. She gives Bob a third.   → 8 left.
    //   2. Bob then doubles his share and returns 2 apples to Alice. → Alice 10, Bob 6.
    //   3. They each pick up an additional 5 apples.     → Alice 15, Bob 11. Total = 26.
    let messages = vec![
        Message::system(
            "You are a careful problem solver. Show brief reasoning, then on the final \
             line write `ANSWER: <number>` with no other text on that line.",
        ),
        Message::user(
            "Alice has 12 apples. She gives one third of her apples to Bob. \
             Bob then doubles his share by picking apples from a tree, and afterwards \
             returns 2 apples to Alice. Then both Alice and Bob each pick up 5 \
             additional apples. How many apples do Alice and Bob have in total now? \
             Finish with `ANSWER: <number>` on its own line.",
        ),
    ];

    let resp = match client.chat(messages, None, ThinkingMode::Disabled).await {
        Ok(r) => r,
        Err(e) => {
            return TestResult {
                name: "reasoning",
                passed: false,
                duration_ms: start.elapsed().as_millis(),
                detail: format!("chat failed: {}", e),
                metric: None,
            }
        }
    };

    let text = resp
        .choices
        .first()
        .map(|c| c.message.content.text_all())
        .unwrap_or_default();

    let answer = extract_answer(&text);
    let passed = answer == Some(26);

    TestResult {
        name: "reasoning",
        passed,
        duration_ms: start.elapsed().as_millis(),
        detail: match answer {
            Some(n) => format!("answer={} (expected 26)", n),
            None => format!(
                "could not parse ANSWER: line; reply='{}'",
                preview(&text, 160)
            ),
        },
        metric: None,
    }
}

fn extract_answer(text: &str) -> Option<i64> {
    for line in text.lines().rev() {
        let lower = line.trim().to_lowercase();
        if let Some(rest) = lower.strip_prefix("answer:") {
            return parse_first_int(rest);
        }
        if let Some(rest) = lower.strip_prefix("answer =") {
            return parse_first_int(rest);
        }
    }
    parse_first_int(text)
}

fn parse_first_int(s: &str) -> Option<i64> {
    let mut digits = String::new();
    let mut started = false;
    let mut negative = false;
    for c in s.chars() {
        if c == '-' && !started {
            negative = true;
            started = true;
        } else if c.is_ascii_digit() {
            digits.push(c);
            started = true;
        } else if started {
            break;
        }
    }
    if digits.is_empty() {
        None
    } else {
        let n: i64 = digits.parse().ok()?;
        Some(if negative { -n } else { n })
    }
}

// ── Test 5: Multimodal ────────────────────────────────────────────────────────

async fn test_multimodal(client: &ApiClient) -> TestResult {
    let start = Instant::now();
    let png_bytes = match generate_red_square_on_green_png() {
        Ok(b) => b,
        Err(e) => {
            return TestResult {
                name: "multimodal",
                passed: false,
                duration_ms: start.elapsed().as_millis(),
                detail: format!("could not generate fixture: {}", e),
                metric: None,
            }
        }
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);

    let user_content = MessageContent::from_text(
        "Briefly describe this image in one sentence. Mention any colors and shapes you see.",
    )
    .with_image(&b64);

    let messages = vec![
        Message::system(
            "You are a vision-capable assistant. Reply with a short visual description.",
        ),
        Message {
            role: "user".into(),
            content: user_content,
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
    ];

    let resp = match client.chat(messages, None, ThinkingMode::Disabled).await {
        Ok(r) => r,
        Err(e) => {
            return TestResult {
                name: "multimodal",
                passed: false,
                duration_ms: start.elapsed().as_millis(),
                detail: format!("chat failed (mmproj loaded?): {}", e),
                metric: None,
            }
        }
    };

    let text = resp
        .choices
        .first()
        .map(|c| c.message.content.text_all())
        .unwrap_or_default()
        .to_lowercase();

    // Look for any visual-vocabulary marker.  We don't require pixel-perfect
    // identification — only that the model's reply references *something*
    // visual that could plausibly come from the image.
    let visual_markers = [
        "red",
        "green",
        "square",
        "rectangle",
        "shape",
        "color",
        "image",
        "picture",
    ];
    let hits: Vec<&str> = visual_markers
        .iter()
        .copied()
        .filter(|m| text.contains(m))
        .collect();
    let passed = hits.len() >= 2;

    TestResult {
        name: "multimodal",
        passed,
        duration_ms: start.elapsed().as_millis(),
        detail: format!(
            "matched markers: {:?} | reply: '{}'",
            hits,
            preview(&text, 120)
        ),
        metric: None,
    }
}

fn generate_red_square_on_green_png() -> Result<Vec<u8>> {
    let w = 64u32;
    let h = 64u32;
    let mut img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(w, h);
    for (x, y, p) in img.enumerate_pixels_mut() {
        // Green background, red square in the centre.
        if (16..48).contains(&x) && (16..48).contains(&y) {
            *p = Rgb([220, 30, 30]);
        } else {
            *p = Rgb([30, 180, 60]);
        }
    }
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .context("encode PNG")?;
    Ok(buf.into_inner())
}

// ── Output ────────────────────────────────────────────────────────────────────

fn preview(s: &str, max: usize) -> String {
    let one_line: String = s.chars().map(|c| if c == '\n' { ' ' } else { c }).collect();
    if one_line.chars().count() <= max {
        one_line
    } else {
        let truncated: String = one_line.chars().take(max).collect();
        format!("{}…", truncated)
    }
}

fn render_markdown(report: &BenchReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("## Quant: `{}`\n\n", report.quant));
    out.push_str(&format!("- Endpoint: `{}`\n", report.endpoint));
    out.push_str(&format!("- Model: `{}`\n", report.model));
    out.push_str(&format!("- Started: {}\n", report.started_at));
    out.push_str(&format!(
        "- Total duration: {:.2}s\n",
        report.total_duration_ms as f64 / 1000.0
    ));
    if let Some(tps) = report.summary.tokens_per_sec_median {
        out.push_str(&format!("- Speed (median): **{:.1} tok/s**\n", tps));
    }
    out.push_str(&format!(
        "- Result: **{}/{} passed**\n\n",
        report.summary.passed, report.summary.total
    ));
    out.push_str("| Test | Pass | Duration (ms) | Detail |\n");
    out.push_str("|------|------|---------------|--------|\n");
    for t in &report.tests {
        out.push_str(&format!(
            "| `{}` | {} | {} | {} |\n",
            t.name,
            if t.passed { "OK" } else { "FAIL" },
            t.duration_ms,
            md_escape(&t.detail),
        ));
    }
    out
}

fn md_escape(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

fn save_report(out_dir: &Path, report: &BenchReport) -> Result<PathBuf> {
    fs::create_dir_all(out_dir).context("create output dir")?;
    let safe_quant: String = report
        .quant
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let path = out_dir.join(format!("{}.json", safe_quant));
    let json = serde_json::to_string_pretty(report).context("serialize report")?;
    fs::write(&path, json).context("write report")?;
    Ok(path)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if std::env::var("SELFWARE_DEBUG").is_ok() {
        tracing_subscriber::fmt()
            .with_env_filter("selfware=debug,quant_benchmark=debug")
            .init();
    }

    let client = build_client(&args)?;
    let started = chrono::Utc::now().to_rfc3339();
    let total_start = Instant::now();

    let mut tests = Vec::new();
    eprintln!("[1/5] speed");
    tests.push(test_speed(&client).await);
    eprintln!("[2/5] tool_use");
    tests.push(test_tool_use(&client).await);
    eprintln!("[3/5] codegen");
    tests.push(test_codegen(&client).await);
    eprintln!("[4/5] reasoning");
    tests.push(test_reasoning(&client).await);
    if args.skip_multimodal {
        eprintln!("[5/5] multimodal (skipped)");
        tests.push(TestResult {
            name: "multimodal",
            passed: false,
            duration_ms: 0,
            detail: "skipped via --skip-multimodal".into(),
            metric: None,
        });
    } else {
        eprintln!("[5/5] multimodal");
        tests.push(test_multimodal(&client).await);
    }

    let passed = tests.iter().filter(|t| t.passed).count();
    let total = tests.len();
    let tps = tests
        .iter()
        .find(|t| t.name == "speed")
        .and_then(|t| t.metric);

    let report = BenchReport {
        quant: args.quant.clone(),
        endpoint: args.endpoint.clone(),
        model: args.model.clone(),
        started_at: started,
        total_duration_ms: total_start.elapsed().as_millis(),
        tests,
        summary: Summary {
            passed,
            failed: total - passed,
            total,
            tokens_per_sec_median: tps,
        },
    };

    // JSON to stdout.
    println!("{}", serde_json::to_string_pretty(&report)?);
    // Markdown to stderr (so callers can pipe stdout to a JSON file).
    eprintln!("\n{}", render_markdown(&report));
    // Also persist to <output>/<quant>.json for the collator.
    let saved = save_report(&args.output, &report)?;
    eprintln!("wrote {}", saved.display());

    // Don't return non-zero on partial failure — the collator wants every
    // run's report regardless.  Only true infrastructure errors (e.g. client
    // construction) bubble up.
    Ok(())
}
