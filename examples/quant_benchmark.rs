//! Quant comparison benchmark — drives the **real selfware agent** against
//! SAB-style coding scenarios and records what it actually managed to do.
//!
//! Why this exists: synthetic prompts ("write fizzbuzz", "describe a red
//! square") pass for every quant we tested down to 2-bit, including ones
//! that fabricate "task complete" without editing the file. The old
//! version of this example hid that failure mode behind a 5/5 green
//! score. This one runs the agent end-to-end on a repository, lets the
//! model use tools, and validates by re-running the project's own test
//! suite — the same gate the SAB framework uses.
//!
//! # What gets measured per scenario
//!
//! 1. **pre_validator_failed** — bug injection actually broke the project
//!    (sanity check; if false the scenario is broken, not the model).
//! 2. **agent_exit** — `success` / `nonzero(N)` / `timeout`.
//! 3. **post_validator_passed** — did `cargo test` (or the scenario's
//!    validator) come back green after the agent finished?
//! 4. **wall_time_secs** — how long the whole agent run took.
//! 5. **agent_steps** — best-effort step count parsed from the agent's
//!    progress lines (`📝 Step N`); `None` if the format changes.
//!
//! # Speed test
//!
//! Kept as a single warm-up probe for the perf number — 3 cold runs of
//! a 200-token completion against the endpoint, median tok/s.
//!
//! # Usage
//!
//! ```bash
//! cargo run --release --example quant_benchmark -- \
//!     --endpoint http://127.0.0.1:8000/v1 \
//!     --quant Qwen3.6-27B-HauhauCS-Q4_K_P \
//!     --model qwen3.6-27b-q4kp \
//!     --output reports/quant_bench/q4kp.json
//! ```
//!
//! The harness shells out to the `selfware` binary (default: looks for
//! `target/release/selfware`, override with `SELFWARE_BIN` env var). It
//! also requires `cargo` on PATH for the validator.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use selfware::api::types::Message;
use selfware::api::{ApiClient, ThinkingMode};
use selfware::config::Config;
use serde::Serialize;

#[derive(Parser, Debug)]
#[command(about = "Drive a quantization through real SAB scenarios + a speed probe")]
struct Args {
    /// OpenAI-compatible endpoint, e.g. `http://127.0.0.1:8000/v1`
    #[arg(long)]
    endpoint: String,

    /// Logical name of the quantization (used in filenames + the report)
    #[arg(long)]
    quant: String,

    /// Output file path for `<quant>.json` (parent created if missing)
    #[arg(long)]
    output: PathBuf,

    /// Optional model name override; defaults to whatever the server reports
    #[arg(long, default_value = "qwen3.6-27b")]
    model: String,

    /// Per-scenario timeout in seconds (default: 480 = 8 min)
    #[arg(long, default_value_t = 480)]
    scenario_timeout_secs: u64,

    /// Keep work directories on FAIL (for debugging). Default: deleted on PASS,
    /// preserved on FAIL.
    #[arg(long, default_value = "/tmp/quant_bench_work")]
    keep_dir: PathBuf,

    /// Path to the selfware binary (default: target/release/selfware or $SELFWARE_BIN)
    #[arg(long)]
    selfware_bin: Option<PathBuf>,

    /// Skip the speed probe (e.g. when the same endpoint is already benched)
    #[arg(long)]
    skip_speed: bool,

    /// Skip scenarios entirely and only run the speed probe
    #[arg(long)]
    speed_only: bool,
}

/// One scenario the agent gets to drive.
///
/// `bug` modifies a single file in the work-dir copy of `template_rel` to
/// guarantee the validator fails before the agent runs. The agent gets the
/// task description in `prompt`, then we re-run `validator` and record
/// whether it now passes.
struct Scenario {
    name: &'static str,
    template_rel: &'static str,
    bug: BugSpec,
    prompt: &'static str,
    validator_program: &'static str,
    validator_args: &'static [&'static str],
}

/// How to corrupt a template so the validator fails before the agent runs.
#[allow(dead_code)] // OverwriteFile is part of the API even if no scenario uses it
enum BugSpec {
    /// Template already ships pre-broken — no injection needed.
    None,
    /// Replace the entire file at `path` with `content`.
    OverwriteFile {
        path: &'static str,
        content: &'static str,
    },
    /// Find `find` (must be unique in `path`) and replace it with `replace`.
    Patch {
        path: &'static str,
        find: &'static str,
        replace: &'static str,
    },
}

const FIX_PROMPT: &str = "You are fixing a small Rust library in the current directory.\n\
     Task:\n\
     1. Run tests and identify failing behavior.\n\
     2. Fix the implementation so all tests pass.\n\
     3. Keep all existing public function signatures unchanged.\n\
     4. Do not add dependencies.\n\
     5. Run cargo test before finishing.\n\
     \n\
     When done, summarize exactly what you changed.";

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "easy_calculator",
        template_rel: "system_tests/projecte2e/templates/easy_calculator",
        bug: BugSpec::Patch {
            path: "src/lib.rs",
            find: "pub fn multiply(a: i64, b: i64) -> i64 {\n    a * b\n}",
            replace: "pub fn multiply(a: i64, b: i64) -> i64 {\n    // BUG: should multiply\n    a + b\n}",
        },
        prompt: FIX_PROMPT,
        validator_program: "cargo",
        validator_args: &["test"],
    },
    Scenario {
        name: "easy_string_ops",
        template_rel: "system_tests/projecte2e/templates/easy_string_ops",
        bug: BugSpec::Patch {
            path: "src/string_ops.rs",
            find: "pub fn reverse(s: &str) -> String {\n    s.chars().rev().collect()\n}",
            replace: "pub fn reverse(s: &str) -> String {\n    // BUG: forgot rev()\n    s.chars().collect()\n}",
        },
        prompt: FIX_PROMPT,
        validator_program: "cargo",
        validator_args: &["test"],
    },
    Scenario {
        name: "medium_bitset",
        template_rel: "system_tests/projecte2e/templates/medium_bitset",
        bug: BugSpec::Patch {
            path: "src/lib.rs",
            find: "    pub fn count_ones(&self) -> usize {\n        self.words.iter().map(|w| w.count_ones() as usize).sum()\n    }",
            replace: "    pub fn count_ones(&self) -> usize {\n        // BUG: counts zero bits instead of one bits\n        self.words.iter().map(|w| w.count_zeros() as usize).sum()\n    }",
        },
        prompt: FIX_PROMPT,
        validator_program: "cargo",
        validator_args: &["test"],
    },
    Scenario {
        name: "medium_json_merge",
        template_rel: "system_tests/projecte2e/templates/medium_json_merge",
        bug: BugSpec::Patch {
            path: "src/lib.rs",
            // Replace the recursive call with a non-recursive overwrite,
            // making the merge shallow.
            find: "                if merged.contains_key(key) {\n                    let base_value = merged.get(key).expect(\"key exists after contains_key check\");\n                    merged.insert(key.clone(), merge_json(base_value, patch_value));\n                } else {\n                    merged.insert(key.clone(), patch_value.clone());\n                }",
            replace: "                // BUG: shallow merge — patch always overwrites without recursing\n                merged.insert(key.clone(), patch_value.clone());",
        },
        prompt: FIX_PROMPT,
        validator_program: "cargo",
        validator_args: &["test"],
    },
    // === pre-broken templates: no injection needed ===
    Scenario {
        name: "actor_pdvr",
        template_rel: "system_tests/projecte2e/templates/actor_pdvr",
        bug: BugSpec::None,
        prompt: FIX_PROMPT,
        validator_program: "cargo",
        validator_args: &["test"],
    },
    Scenario {
        name: "hard_event_bus",
        template_rel: "system_tests/projecte2e/templates/hard_event_bus",
        bug: BugSpec::None,
        prompt: FIX_PROMPT,
        validator_program: "cargo",
        validator_args: &["test"],
    },
    Scenario {
        name: "hard_scheduler",
        template_rel: "system_tests/projecte2e/templates/hard_scheduler",
        bug: BugSpec::None,
        prompt: FIX_PROMPT,
        validator_program: "cargo",
        validator_args: &["test"],
    },
    Scenario {
        name: "unsafe_scanner",
        template_rel: "system_tests/projecte2e/templates/unsafe_scanner",
        bug: BugSpec::None,
        prompt: FIX_PROMPT,
        validator_program: "cargo",
        validator_args: &["test"],
    },
    Scenario {
        name: "viz_ascii_table",
        template_rel: "system_tests/projecte2e/templates/viz_ascii_table",
        bug: BugSpec::None,
        prompt: FIX_PROMPT,
        validator_program: "cargo",
        validator_args: &["test"],
    },
    Scenario {
        name: "viz_maze_gen",
        template_rel: "system_tests/projecte2e/templates/viz_maze_gen",
        bug: BugSpec::None,
        prompt: FIX_PROMPT,
        validator_program: "cargo",
        validator_args: &["test"],
    },
    Scenario {
        name: "viz_svg_chart",
        template_rel: "system_tests/projecte2e/templates/viz_svg_chart",
        bug: BugSpec::None,
        prompt: FIX_PROMPT,
        validator_program: "cargo",
        validator_args: &["test"],
    },
];

#[derive(Debug, Serialize)]
struct ScenarioResult {
    name: String,
    pre_validator_failed: bool,
    agent_exit: AgentExit,
    post_validator_passed: bool,
    wall_time_secs: f64,
    agent_steps: Option<u32>,
    validator_summary: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AgentExit {
    Success,
    Nonzero { code: i32 },
    Timeout,
    Killed,
}

#[derive(Debug, Serialize, Clone)]
struct SpeedResult {
    samples_tok_per_sec: Vec<f64>,
    median_tok_per_sec: f64,
}

#[derive(Debug, Serialize)]
struct Report {
    quant: String,
    endpoint: String,
    model: String,
    started_at: String,
    total_duration_secs: f64,
    speed: Option<SpeedResult>,
    scenarios: Vec<ScenarioResult>,
    summary: Summary,
}

#[derive(Debug, Serialize)]
struct Summary {
    scenarios_passed: usize,
    scenarios_total: usize,
    speed_tok_per_sec_median: Option<f64>,
}

fn resolve_selfware_bin(args: &Args) -> Result<PathBuf> {
    if let Some(p) = &args.selfware_bin {
        return Ok(p.clone());
    }
    if let Ok(env) = std::env::var("SELFWARE_BIN") {
        return Ok(PathBuf::from(env));
    }
    // Look for target/release/selfware relative to the repo root (cwd).
    let candidate = PathBuf::from("target/release/selfware");
    if candidate.exists() {
        return Ok(candidate);
    }
    Err(anyhow!(
        "selfware binary not found — pass --selfware-bin or set SELFWARE_BIN, \
         or build with `cargo build --release --features extras`"
    ))
}

fn apply_bug(work_dir: &Path, bug: &BugSpec) -> Result<()> {
    match bug {
        BugSpec::None => Ok(()),
        BugSpec::OverwriteFile { path, content } => {
            let target = work_dir.join(path);
            fs::write(&target, content)
                .with_context(|| format!("write {} failed", target.display()))?;
            Ok(())
        }
        BugSpec::Patch {
            path,
            find,
            replace,
        } => {
            let target = work_dir.join(path);
            let original = fs::read_to_string(&target)
                .with_context(|| format!("read {} failed", target.display()))?;
            let occurrences = original.matches(find).count();
            if occurrences == 0 {
                return Err(anyhow!(
                    "bug-injection patch did not match any text in {} — \
                     the template may have changed; update the scenario.",
                    target.display()
                ));
            }
            if occurrences > 1 {
                return Err(anyhow!(
                    "bug-injection patch matched {occurrences} places in {} — \
                     make `find` more specific so it's unambiguous.",
                    target.display()
                ));
            }
            let patched = original.replace(find, replace);
            fs::write(&target, patched)?;
            Ok(())
        }
    }
}

fn copy_template(template: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    for entry in walkdir::WalkDir::new(template) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(template).unwrap();
        let target = dest.join(rel);
        if entry.file_type().is_dir() {
            // Skip target/ — it just bloats the copy and gets rebuilt anyway.
            if rel.components().any(|c| c.as_os_str() == "target") {
                continue;
            }
            fs::create_dir_all(&target)?;
        } else if entry.file_type().is_file() {
            if rel.components().any(|c| c.as_os_str() == "target") {
                continue;
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Run the validator. Returns (passed, summary string for the report).
fn run_validator(work_dir: &Path, scenario: &Scenario) -> (bool, String) {
    // Force a fresh build; otherwise stale artifacts can mask the bug.
    let _ = Command::new(scenario.validator_program)
        .arg("clean")
        .current_dir(work_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let output = match Command::new(scenario.validator_program)
        .args(scenario.validator_args)
        .current_dir(work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
    {
        Ok(o) => o,
        Err(e) => return (false, format!("validator failed to spawn: {e}")),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Find every `test result:` line and pick the most informative one — i.e.
    // the one with the highest passed+failed count. (`cargo test` emits one
    // line per harness — lib tests, each integration target, and doctests —
    // and the doctest line is often `0 passed; 0 failed` which hides the
    // real result if we just take the last.)
    //
    // We extract counts with a simple "<digits> passed" / "<digits> failed"
    // scan since the line starts with "test result: ok. <N> passed" or
    // "test result: FAILED. <N> passed; <M> failed".
    fn extract_count(line: &str, suffix: &str) -> u64 {
        let mut total = 0u64;
        let bytes = line.as_bytes();
        let pat = format!(" {}", suffix);
        let mut idx = 0;
        while let Some(found) = line[idx..].find(&pat) {
            let end = idx + found;
            // Walk left to find the digits.
            let mut start = end;
            while start > 0 && bytes[start - 1].is_ascii_digit() {
                start -= 1;
            }
            if start < end {
                if let Ok(n) = line[start..end].parse::<u64>() {
                    total += n;
                }
            }
            idx = end + pat.len();
        }
        total
    }

    let mut best: Option<(u64, &str)> = None;
    for line in combined.lines() {
        let line = line.trim();
        if !line.contains("test result:") {
            continue;
        }
        let total = extract_count(line, "passed") + extract_count(line, "failed");
        match best {
            None => best = Some((total, line)),
            Some((t, _)) if total > t => best = Some((total, line)),
            _ => {}
        }
    }
    let summary_line = best.map(|(_, l)| l.to_string()).unwrap_or_else(|| {
        // No test-result line at all — most likely a compile error.
        // Surface the first `error[E...]` so the user knows why.
        combined
            .lines()
            .find(|l| l.trim_start().starts_with("error"))
            .map(|l| l.trim().to_string())
            .unwrap_or_else(|| "(no test result line)".to_string())
    });

    (output.status.success(), summary_line)
}

/// Drive the selfware binary against the work dir; capture exit + step count.
///
/// If `log_path` is supplied, agent stdout+stderr is teed into it so the
/// model's full reasoning trace is preserved for debugging.
fn run_agent(
    selfware_bin: &Path,
    endpoint: &str,
    model: &str,
    work_dir: &Path,
    prompt: &str,
    timeout: Duration,
    log_path: Option<&Path>,
) -> (AgentExit, Option<u32>) {
    let start = Instant::now();
    let mut cmd = Command::new(selfware_bin);
    cmd.arg("-p")
        .arg(prompt)
        .arg("-C")
        .arg(work_dir)
        .arg("--yolo")
        .arg("--no-tui")
        .arg("--quiet")
        .env("SELFWARE_ENDPOINT", endpoint)
        .env("SELFWARE_MODEL", model)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("  selfware spawn error: {e}");
            return (AgentExit::Killed, None);
        }
    };

    // Poll for exit, killing on timeout.
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let exit = if status.success() {
                    AgentExit::Success
                } else if let Some(code) = status.code() {
                    AgentExit::Nonzero { code }
                } else {
                    AgentExit::Killed
                };
                let stdout = child
                    .stdout
                    .take()
                    .and_then(|mut s| {
                        use std::io::Read;
                        let mut buf = String::new();
                        s.read_to_string(&mut buf).ok().map(|_| buf)
                    })
                    .unwrap_or_default();
                let stderr = child
                    .stderr
                    .take()
                    .and_then(|mut s| {
                        use std::io::Read;
                        let mut buf = String::new();
                        s.read_to_string(&mut buf).ok().map(|_| buf)
                    })
                    .unwrap_or_default();
                if let Some(p) = log_path {
                    let _ = fs::write(
                        p,
                        format!("=== STDOUT ===\n{stdout}\n=== STDERR ===\n{stderr}"),
                    );
                }
                let steps = parse_step_count(&stdout);
                return (exit, steps);
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    if let Some(p) = log_path {
                        let _ = fs::write(p, "=== KILLED on timeout ===\n");
                    }
                    return (AgentExit::Timeout, None);
                }
                std::thread::sleep(Duration::from_millis(500));
            }
            Err(e) => {
                eprintln!("  try_wait error: {e}");
                return (AgentExit::Killed, None);
            }
        }
    }
}

fn parse_step_count(stdout: &str) -> Option<u32> {
    // selfware emits lines like `📝 Step 17 Executing...`. Find the largest N.
    let mut max = None;
    for line in stdout.lines() {
        if let Some(rest) = line.split("Step ").nth(1) {
            if let Some(num_str) = rest.split_whitespace().next() {
                if let Ok(n) = num_str.parse::<u32>() {
                    max = Some(max.map_or(n, |m: u32| m.max(n)));
                }
            }
        }
    }
    max
}

async fn run_speed_probe(args: &Args) -> Result<SpeedResult> {
    let cfg = Config {
        endpoint: args.endpoint.clone(),
        model: args.model.clone(),
        max_tokens: 200,
        temperature: 0.0,
        context_length: 32768,
        ..Config::default()
    };
    let client = ApiClient::new(&cfg)?;

    let prompt = "Count slowly from one to two hundred. Use words, one number per line. \
                  Do not add commentary. Begin with: one";
    let mut samples = Vec::new();
    for run in 1..=3 {
        let messages = vec![Message::user(prompt.to_string())];
        let started = Instant::now();
        let resp = client
            .chat(messages, None, ThinkingMode::Disabled)
            .await
            .with_context(|| format!("speed probe run {run} failed"))?;
        let elapsed = started.elapsed().as_secs_f64();
        let completion = resp.usage.completion_tokens.max(1) as f64;
        samples.push(completion / elapsed);
    }
    let mut sorted = samples.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = sorted[sorted.len() / 2];
    Ok(SpeedResult {
        samples_tok_per_sec: samples,
        median_tok_per_sec: median,
    })
}

fn render_markdown(report: &Report) -> String {
    let mut out = String::new();
    out.push_str(&format!("## Quant: `{}`\n\n", report.quant));
    out.push_str(&format!("- Endpoint: `{}`\n", report.endpoint));
    out.push_str(&format!("- Model: `{}`\n", report.model));
    out.push_str(&format!("- Started: {}\n", report.started_at));
    out.push_str(&format!(
        "- Total duration: {:.2}s\n",
        report.total_duration_secs
    ));
    if let Some(s) = &report.speed {
        out.push_str(&format!(
            "- Speed (median): **{:.1} tok/s**\n",
            s.median_tok_per_sec
        ));
    }
    out.push_str(&format!(
        "- Scenarios passed: **{}/{}**\n\n",
        report.summary.scenarios_passed, report.summary.scenarios_total
    ));

    if report.scenarios.is_empty() {
        out.push_str("(speed-only run, no scenarios)\n");
        return out;
    }

    out.push_str(
        "| Scenario | Pre-fail | Agent exit | Post-pass | Wall (s) | Steps | Validator summary |\n",
    );
    out.push_str(
        "|----------|----------|------------|-----------|---------:|------:|-------------------|\n",
    );
    for s in &report.scenarios {
        let agent = match &s.agent_exit {
            AgentExit::Success => "success".to_string(),
            AgentExit::Nonzero { code } => format!("nonzero({code})"),
            AgentExit::Timeout => "timeout".to_string(),
            AgentExit::Killed => "killed".to_string(),
        };
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {:.1} | {} | {} |\n",
            s.name,
            if s.pre_validator_failed { "✓" } else { "✗" },
            agent,
            if s.post_validator_passed {
                "✓"
            } else {
                "✗"
            },
            s.wall_time_secs,
            s.agent_steps.map_or("?".to_string(), |n| n.to_string()),
            md_escape(&s.validator_summary),
        ));
    }
    out
}

fn md_escape(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let started = Instant::now();
    let started_at = chrono::Utc::now().to_rfc3339();

    let speed = if args.skip_speed {
        None
    } else {
        eprintln!("=== speed probe ===");
        Some(run_speed_probe(&args).await?)
    };

    let mut scenarios_results = Vec::new();
    if !args.speed_only {
        let bin = resolve_selfware_bin(&args)?;
        eprintln!("=== using selfware binary: {} ===", bin.display());
        let timeout = Duration::from_secs(args.scenario_timeout_secs);

        // Use a stable directory under args.keep_dir so we can preserve
        // failing runs for inspection. Each scenario gets its own subdir
        // named by quant + scenario.
        fs::create_dir_all(&args.keep_dir)?;

        for scenario in SCENARIOS {
            eprintln!("\n=== scenario: {} ===", scenario.name);

            let work_path = args
                .keep_dir
                .join(format!("{}__{}", args.quant, scenario.name));
            // Wipe any previous run.
            let _ = fs::remove_dir_all(&work_path);

            let template = PathBuf::from(scenario.template_rel);
            copy_template(&template, &work_path)
                .with_context(|| format!("copy template {} failed", scenario.template_rel))?;

            // Inject the bug.
            apply_bug(&work_path, &scenario.bug)
                .with_context(|| format!("bug injection failed for {}", scenario.name))?;

            // Sanity: the validator must FAIL on the bugged code.
            let (pre_passed, _pre_summary) = run_validator(&work_path, scenario);
            let pre_failed = !pre_passed;
            if !pre_failed {
                eprintln!(
                    "  WARNING: bug injection didn't break the project. \
                     Scenario will not discriminate quants."
                );
            }

            // Run the agent. Capture stdout to a log file in the work dir
            // so we can inspect what the model actually did.
            let agent_log = work_path.join("_agent.log");
            let agent_started = Instant::now();
            let (agent_exit, agent_steps) = run_agent(
                &bin,
                &args.endpoint,
                &args.model,
                &work_path,
                scenario.prompt,
                timeout,
                Some(&agent_log),
            );
            let wall = agent_started.elapsed().as_secs_f64();
            eprintln!("  agent exit: {agent_exit:?} after {wall:.1}s, steps={agent_steps:?}");
            eprintln!("  log: {}", agent_log.display());

            // Final validation.
            let (post_passed, post_summary) = run_validator(&work_path, scenario);
            eprintln!(
                "  post-validator: {} ({})",
                if post_passed { "PASS" } else { "FAIL" },
                post_summary
            );

            // Clean up on PASS, keep on FAIL (or always keep if it's a
            // discriminating scenario the user wants to inspect).
            if post_passed && pre_failed {
                let _ = fs::remove_dir_all(&work_path);
            } else {
                eprintln!("  KEPT for inspection: {}", work_path.display());
            }

            scenarios_results.push(ScenarioResult {
                name: scenario.name.to_string(),
                pre_validator_failed: pre_failed,
                agent_exit,
                post_validator_passed: post_passed,
                wall_time_secs: wall,
                agent_steps,
                validator_summary: post_summary,
            });
        }
    }

    let scenarios_passed = scenarios_results
        .iter()
        .filter(|s| s.post_validator_passed && s.pre_validator_failed)
        .count();

    let report = Report {
        quant: args.quant.clone(),
        endpoint: args.endpoint.clone(),
        model: args.model.clone(),
        started_at,
        total_duration_secs: started.elapsed().as_secs_f64(),
        speed: speed.clone(),
        scenarios: scenarios_results,
        summary: Summary {
            scenarios_passed,
            scenarios_total: SCENARIOS.len(),
            speed_tok_per_sec_median: speed.as_ref().map(|s| s.median_tok_per_sec),
        },
    };

    // Write JSON.
    if let Some(parent) = args.output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(&args.output, serde_json::to_string_pretty(&report)?)?;

    // Markdown to stderr (so JSON on stdout stays clean if redirected).
    eprintln!("\n{}", render_markdown(&report));
    eprintln!("wrote {}", args.output.display());

    Ok(())
}
