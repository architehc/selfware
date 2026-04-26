//! Agent regression test — runs selfware against a live LLM endpoint and
//! verifies the core agent loop works correctly end-to-end.
//!
//! This is a standalone binary gated behind `--features system-tests`.
//!
//! Usage:
//! ```sh
//! cargo run --bin agent_regression --features system-tests -- \
//!   --endpoint http://127.0.0.1:8000/v1 \
//!   --model txn545/Qwen3.5-122B-A10B-NVFP4
//! ```
//!
//! Environment overrides:
//!   SELFWARE_ENDPOINT — LLM endpoint (default: http://127.0.0.1:8000/v1)
//!   SELFWARE_MODEL    — LLM model   (default: txn545/Qwen3.5-122B-A10B-NVFP4)

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:8000/v1";
const DEFAULT_MODEL: &str = "txn545/Qwen3.5-122B-A10B-NVFP4";
const DEFAULT_TIMEOUT_SECS: u64 = 120;

// Rough estimate: ~4 chars per token for English text
const CHARS_PER_TOKEN_ESTIMATE: usize = 4;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegressionResult {
    test_name: String,
    passed: bool,
    duration_secs: f64,
    tokens_used: usize,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegressionReport {
    endpoint: String,
    model: String,
    timestamp: String,
    results: Vec<RegressionResult>,
    total_passed: usize,
    total_failed: usize,
    total_duration_secs: f64,
}

// ---------------------------------------------------------------------------
// CLI (manual parsing — no clap import)
// ---------------------------------------------------------------------------

struct Args {
    endpoint: String,
    model: String,
    selfware_bin: PathBuf,
    test_filter: Option<String>,
    timeout_secs: u64,
}

fn print_usage() {
    eprintln!(
        "Usage: agent_regression [OPTIONS]\n\
         \n\
         Options:\n\
           --endpoint URL       LLM endpoint (default: {DEFAULT_ENDPOINT})\n\
           --model MODEL        Model name (default: {DEFAULT_MODEL})\n\
           --selfware-bin PATH  Path to selfware binary\n\
           --test NAME          Run only this test\n\
           --timeout SECS       Per-test timeout (default: {DEFAULT_TIMEOUT_SECS})\n\
           --help               Show this message"
    );
}

fn parse_args() -> Args {
    let mut endpoint: Option<String> = None;
    let mut model: Option<String> = None;
    let mut selfware_bin: Option<PathBuf> = None;
    let mut test_filter: Option<String> = None;
    let mut timeout_secs: Option<u64> = None;

    let argv: Vec<String> = env::args().collect();
    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            "--endpoint" => {
                i += 1;
                endpoint = Some(argv.get(i).expect("--endpoint requires a value").clone());
            }
            "--model" => {
                i += 1;
                model = Some(argv.get(i).expect("--model requires a value").clone());
            }
            "--selfware-bin" => {
                i += 1;
                selfware_bin = Some(PathBuf::from(
                    argv.get(i).expect("--selfware-bin requires a value"),
                ));
            }
            "--test" => {
                i += 1;
                test_filter = Some(argv.get(i).expect("--test requires a value").clone());
            }
            "--timeout" => {
                i += 1;
                timeout_secs = Some(
                    argv.get(i)
                        .expect("--timeout requires a value")
                        .parse()
                        .expect("--timeout must be a number"),
                );
            }
            other => {
                eprintln!("Unknown argument: {other}");
                print_usage();
                std::process::exit(1);
            }
        }
        i += 1;
    }

    // Resolve endpoint: CLI > env > default
    let endpoint = endpoint
        .or_else(|| env::var("SELFWARE_ENDPOINT").ok())
        .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());

    let model = model
        .or_else(|| env::var("SELFWARE_MODEL").ok())
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    let selfware_bin = selfware_bin.unwrap_or_else(find_selfware_bin);
    let timeout_secs = timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);

    Args {
        endpoint,
        model,
        selfware_bin,
        test_filter,
        timeout_secs,
    }
}

/// Locate the selfware binary: PATH first, then ./target/release/selfware.
fn find_selfware_bin() -> PathBuf {
    // Check PATH via `which`
    if let Ok(output) = Command::new("which").arg("selfware").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return PathBuf::from(path);
            }
        }
    }

    // Fallback: target/release/selfware relative to manifest dir
    let release = PathBuf::from("./target/release/selfware");
    if release.exists() {
        return release;
    }

    // Fallback: target/debug/selfware
    let debug = PathBuf::from("./target/debug/selfware");
    if debug.exists() {
        return debug;
    }

    eprintln!("ERROR: Cannot find selfware binary. Provide --selfware-bin or ensure it is on PATH.");
    std::process::exit(1);
}

// ---------------------------------------------------------------------------
// Config generation
// ---------------------------------------------------------------------------

fn write_selfware_toml(dir: &Path, endpoint: &str, model: &str) {
    let toml_content = format!(
        r#"endpoint = "{endpoint}"
model = "{model}"
max_tokens = 8192
temperature = 0.2

[safety]
allowed_paths = ["./**"]
denied_paths = []

[agent]
max_iterations = 15
step_timeout_secs = 120
native_function_calling = false
streaming = false

[retry]
max_retries = 2
base_delay_ms = 1000
max_delay_ms = 10000
"#
    );
    let path = dir.join("selfware.toml");
    fs::write(&path, toml_content).expect("Failed to write selfware.toml");
}

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

struct RunOutput {
    stdout: String,
    stderr: String,
    #[allow(dead_code)]
    exit_code: i32,
    duration_secs: f64,
}

fn run_selfware(
    bin: &Path,
    workspace: &Path,
    prompt: &str,
    timeout_secs: u64,
) -> Result<RunOutput, String> {
    let start = Instant::now();

    let child = Command::new(bin)
        .args(["-p", prompt, "--mode", "yolo", "-C"])
        .arg(workspace)
        .arg("--no-tui")
        .arg("--ascii")
        .arg("--compact")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("NO_COLOR", "1")
        .env("SELFWARE_DISABLE_TUI", "1")
        .spawn()
        .map_err(|e| format!("Failed to spawn selfware: {e}"))?;

    // Wait with timeout
    let output = wait_with_timeout(child, timeout_secs)?;
    let duration_secs = start.elapsed().as_secs_f64();

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    Ok(RunOutput {
        stdout,
        stderr,
        exit_code,
        duration_secs,
    })
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout_secs: u64,
) -> Result<std::process::Output, String> {
    let start = Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = child
                    .stdout
                    .take()
                    .map(|mut s| {
                        let mut buf = Vec::new();
                        std::io::Read::read_to_end(&mut s, &mut buf).ok();
                        buf
                    })
                    .unwrap_or_default();
                let stderr = child
                    .stderr
                    .take()
                    .map(|mut s| {
                        let mut buf = Vec::new();
                        std::io::Read::read_to_end(&mut s, &mut buf).ok();
                        buf
                    })
                    .unwrap_or_default();

                return Ok(std::process::Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    return Err(format!("Timed out after {timeout_secs}s"));
                }
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            Err(e) => return Err(format!("Error waiting for process: {e}")),
        }
    }
}

fn estimate_tokens(text: &str) -> usize {
    text.len() / CHARS_PER_TOKEN_ESTIMATE
}

// ---------------------------------------------------------------------------
// Test cases
// ---------------------------------------------------------------------------

type TestFn = fn(&Path, &Path, &str, &str, u64) -> RegressionResult;

struct TestCase {
    name: &'static str,
    description: &'static str,
    run: TestFn,
}

fn all_tests() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "test_file_create",
            description: "Create a file and verify contents",
            run: test_file_create,
        },
        TestCase {
            name: "test_file_read_and_edit",
            description: "Read then edit a file",
            run: test_file_read_and_edit,
        },
        TestCase {
            name: "test_shell_exec",
            description: "Execute shell command and report output",
            run: test_shell_exec,
        },
        TestCase {
            name: "test_multi_step",
            description: "Multi-step file creation and analysis",
            run: test_multi_step,
        },
        TestCase {
            name: "test_context_retention",
            description: "Read multiple files and identify the largest",
            run: test_context_retention,
        },
        TestCase {
            name: "test_error_recovery",
            description: "Handle nonexistent file gracefully",
            run: test_error_recovery,
        },
    ]
}

fn make_result(name: &str, passed: bool, duration: f64, tokens: usize, error: Option<String>) -> RegressionResult {
    RegressionResult {
        test_name: name.to_string(),
        passed,
        duration_secs: duration,
        tokens_used: tokens,
        error,
    }
}

// -- test_file_create -------------------------------------------------------

fn test_file_create(
    bin: &Path,
    workspace: &Path,
    endpoint: &str,
    model: &str,
    timeout: u64,
) -> RegressionResult {
    let name = "test_file_create";
    write_selfware_toml(workspace, endpoint, model);

    let prompt = "Create a file called hello.txt with the text 'Hello from selfware'";
    match run_selfware(bin, workspace, prompt, timeout) {
        Ok(out) => {
            let tokens = estimate_tokens(&out.stdout) + estimate_tokens(&out.stderr);
            let hello_path = workspace.join("hello.txt");

            if !hello_path.exists() {
                return make_result(name, false, out.duration_secs, tokens,
                    Some("hello.txt was not created".into()));
            }
            let contents = fs::read_to_string(&hello_path).unwrap_or_default();
            if !contents.contains("Hello from selfware") {
                return make_result(name, false, out.duration_secs, tokens,
                    Some(format!("hello.txt contents wrong: {contents:?}")));
            }
            make_result(name, true, out.duration_secs, tokens, None)
        }
        Err(e) => make_result(name, false, 0.0, 0, Some(e)),
    }
}

// -- test_file_read_and_edit ------------------------------------------------

fn test_file_read_and_edit(
    bin: &Path,
    workspace: &Path,
    endpoint: &str,
    model: &str,
    timeout: u64,
) -> RegressionResult {
    let name = "test_file_read_and_edit";
    write_selfware_toml(workspace, endpoint, model);

    // Pre-create the file from previous test
    let hello_path = workspace.join("hello.txt");
    fs::write(&hello_path, "Hello from selfware").ok();

    let prompt = "Read hello.txt, then edit it to say 'Hello from selfware v2'";
    match run_selfware(bin, workspace, prompt, timeout) {
        Ok(out) => {
            let tokens = estimate_tokens(&out.stdout) + estimate_tokens(&out.stderr);
            let contents = fs::read_to_string(&hello_path).unwrap_or_default();
            if !contents.contains("v2") {
                return make_result(name, false, out.duration_secs, tokens,
                    Some(format!("hello.txt missing 'v2': {contents:?}")));
            }
            make_result(name, true, out.duration_secs, tokens, None)
        }
        Err(e) => make_result(name, false, 0.0, 0, Some(e)),
    }
}

// -- test_shell_exec --------------------------------------------------------

fn test_shell_exec(
    bin: &Path,
    workspace: &Path,
    endpoint: &str,
    model: &str,
    timeout: u64,
) -> RegressionResult {
    let name = "test_shell_exec";
    write_selfware_toml(workspace, endpoint, model);

    let prompt = "Run 'echo hello_world' using shell_exec and tell me the output";
    match run_selfware(bin, workspace, prompt, timeout) {
        Ok(out) => {
            let tokens = estimate_tokens(&out.stdout) + estimate_tokens(&out.stderr);
            let combined = format!("{}{}", out.stdout, out.stderr);
            if !combined.contains("hello_world") {
                return make_result(name, false, out.duration_secs, tokens,
                    Some("Output does not contain 'hello_world'".into()));
            }
            make_result(name, true, out.duration_secs, tokens, None)
        }
        Err(e) => make_result(name, false, 0.0, 0, Some(e)),
    }
}

// -- test_multi_step --------------------------------------------------------

fn test_multi_step(
    bin: &Path,
    workspace: &Path,
    endpoint: &str,
    model: &str,
    timeout: u64,
) -> RegressionResult {
    let name = "test_multi_step";
    write_selfware_toml(workspace, endpoint, model);

    let prompt = "Create a file called count.txt with numbers 1 through 5, one per line. \
                  Then read it back and tell me how many lines it has.";
    match run_selfware(bin, workspace, prompt, timeout) {
        Ok(out) => {
            let tokens = estimate_tokens(&out.stdout) + estimate_tokens(&out.stderr);
            let count_path = workspace.join("count.txt");

            if !count_path.exists() {
                return make_result(name, false, out.duration_secs, tokens,
                    Some("count.txt was not created".into()));
            }

            let contents = fs::read_to_string(&count_path).unwrap_or_default();
            let line_count = contents.lines().filter(|l| !l.trim().is_empty()).count();
            if line_count != 5 {
                return make_result(name, false, out.duration_secs, tokens,
                    Some(format!("count.txt has {line_count} non-empty lines, expected 5")));
            }

            let combined = format!("{}{}", out.stdout, out.stderr);
            if !combined.contains('5') {
                return make_result(name, false, out.duration_secs, tokens,
                    Some("Agent output does not mention '5'".into()));
            }

            make_result(name, true, out.duration_secs, tokens, None)
        }
        Err(e) => make_result(name, false, 0.0, 0, Some(e)),
    }
}

// -- test_context_retention -------------------------------------------------

fn test_context_retention(
    bin: &Path,
    workspace: &Path,
    endpoint: &str,
    model: &str,
    timeout: u64,
) -> RegressionResult {
    let name = "test_context_retention";
    write_selfware_toml(workspace, endpoint, model);

    // Pre-create 3 .rs files of different sizes
    fs::write(workspace.join("small.rs"), "fn main() {}\n")
        .expect("write small.rs");
    fs::write(
        workspace.join("medium.rs"),
        "fn main() {\n    println!(\"hello\");\n    println!(\"world\");\n}\n",
    )
    .expect("write medium.rs");
    fs::write(
        workspace.join("largest.rs"),
        "/// This is the largest Rust source file in the directory.\n\
         /// It contains significantly more content than the others.\n\
         fn main() {\n\
         \x20   let items = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];\n\
         \x20   for item in &items {\n\
         \x20       println!(\"Processing item: {}\", item);\n\
         \x20   }\n\
         \x20   let total: i32 = items.iter().sum();\n\
         \x20   println!(\"Total: {}\", total);\n\
         }\n",
    )
    .expect("write largest.rs");

    let prompt = "Read all .rs files in this directory and tell me which one is largest";
    match run_selfware(bin, workspace, prompt, timeout) {
        Ok(out) => {
            let tokens = estimate_tokens(&out.stdout) + estimate_tokens(&out.stderr);
            let combined = format!("{}{}", out.stdout, out.stderr).to_lowercase();

            if !combined.contains("largest") {
                return make_result(name, false, out.duration_secs, tokens,
                    Some("Agent did not identify 'largest.rs' as the largest file".into()));
            }
            make_result(name, true, out.duration_secs, tokens, None)
        }
        Err(e) => make_result(name, false, 0.0, 0, Some(e)),
    }
}

// -- test_error_recovery ----------------------------------------------------

fn test_error_recovery(
    bin: &Path,
    workspace: &Path,
    endpoint: &str,
    model: &str,
    timeout: u64,
) -> RegressionResult {
    let name = "test_error_recovery";
    write_selfware_toml(workspace, endpoint, model);

    let prompt = "Read the file nonexistent_file_12345.rs";
    match run_selfware(bin, workspace, prompt, timeout) {
        Ok(out) => {
            let tokens = estimate_tokens(&out.stdout) + estimate_tokens(&out.stderr);
            // The agent should not crash — exit code 0 or a graceful non-zero is fine.
            // The key check: it produced output and did not panic/segfault.
            let combined = format!("{}{}", out.stdout, out.stderr).to_lowercase();

            // Agent should mention the file doesn't exist / not found / error
            let mentions_error = combined.contains("not found")
                || combined.contains("does not exist")
                || combined.contains("no such file")
                || combined.contains("doesn't exist")
                || combined.contains("error")
                || combined.contains("cannot")
                || combined.contains("failed");

            if !mentions_error {
                return make_result(name, false, out.duration_secs, tokens,
                    Some("Agent did not report file-not-found gracefully".into()));
            }
            make_result(name, true, out.duration_secs, tokens, None)
        }
        Err(e) => make_result(name, false, 0.0, 0, Some(e)),
    }
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

fn print_table(results: &[RegressionResult]) {
    println!();
    println!(
        "{:<25} {:<8} {:>10} {:>10}",
        "TEST", "STATUS", "DURATION", "TOKENS"
    );
    println!("{}", "-".repeat(60));

    for r in results {
        let status = if r.passed { "PASS" } else { "FAIL" };
        println!(
            "{:<25} {:<8} {:>9.1}s {:>10}",
            r.test_name, status, r.duration_secs, r.tokens_used
        );
        if let Some(err) = &r.error {
            println!("  -> {err}");
        }
    }

    let passed = results.iter().filter(|r| r.passed).count();
    let total = results.len();
    println!("{}", "-".repeat(60));
    println!(
        "Result: {passed}/{total} passed, {} failed",
        total - passed
    );
    println!();
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let args = parse_args();

    println!("=== Selfware Agent Regression Tests ===");
    println!("Endpoint:  {}", args.endpoint);
    println!("Model:     {}", args.model);
    println!("Binary:    {}", args.selfware_bin.display());
    println!("Timeout:   {}s per test", args.timeout_secs);
    println!();

    // Verify the binary exists
    if !args.selfware_bin.exists() {
        eprintln!(
            "ERROR: selfware binary not found at {}",
            args.selfware_bin.display()
        );
        std::process::exit(1);
    }

    let tests = all_tests();
    let tests: Vec<_> = match &args.test_filter {
        Some(filter) => tests
            .into_iter()
            .filter(|t| t.name == filter.as_str())
            .collect(),
        None => tests,
    };

    if tests.is_empty() {
        eprintln!("No tests matched the filter.");
        std::process::exit(1);
    }

    let mut results = Vec::new();
    let overall_start = Instant::now();

    for tc in &tests {
        println!("--- Running: {} ---", tc.name);
        println!("    {}", tc.description);

        // Each test gets its own tempdir
        let tmpdir = tempfile::tempdir().expect("Failed to create tempdir");
        let workspace = tmpdir.path();

        let result = (tc.run)(
            &args.selfware_bin,
            workspace,
            &args.endpoint,
            &args.model,
            args.timeout_secs,
        );

        let icon = if result.passed { "[PASS]" } else { "[FAIL]" };
        println!("    {icon} {:.1}s\n", result.duration_secs);

        results.push(result);
        // tmpdir is dropped here, cleaning up workspace
    }

    let total_duration = overall_start.elapsed().as_secs_f64();

    print_table(&results);

    // Build report
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.len() - passed;

    let report = RegressionReport {
        endpoint: args.endpoint.clone(),
        model: args.model.clone(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        results: results.clone(),
        total_passed: passed,
        total_failed: failed,
        total_duration_secs: total_duration,
    };

    // Write JSON report
    let json = serde_json::to_string_pretty(&report).expect("Failed to serialize report");
    let report_path = "agent_regression_results.json";
    fs::write(report_path, &json).expect("Failed to write report JSON");
    println!("Report written to {report_path}");

    if failed > 0 {
        std::process::exit(1);
    }
}
