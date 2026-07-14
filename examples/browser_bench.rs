//! Browser benchmark: execute web tasks, analyze with 16 concurrent LLM streams,
//! and store interaction traces for memory consolidation.
//!
//! Run with: cargo run --features bench-harness --example browser_bench

use selfware::bench_harness::computer_control::{
    BrowserBenchConfig, BrowserBenchRunner, ExecutionBackend,
};
use selfware::bench_harness::HarnessConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info,selfware::bench_harness=debug")
        .init();

    let config = BrowserBenchConfig {
        harness: HarnessConfig {
            endpoint: "http://localhost:8000/v1".into(),
            model: "qwen3.5-27b".into(),
            max_concurrent: 16,
            max_tokens: 1024,
            temperature: 0.3,
            timeout_secs: 300,
            max_retries: 3,
            retry_delay_ms: 500,
            output_dir: "bench_results/browser".into(),
            extra_body: serde_json::json!({
                "chat_template_kwargs": {"enable_thinking": false}
            }),
        },
        max_browser_concurrent: 4,
        output_dir: "bench_results/browser".into(),
        llm_analysis: true,
        store_traces: true,
        backend: ExecutionBackend::Browser,
    };

    let runner = BrowserBenchRunner::new(config.clone())?;

    eprintln!("\n=== Browser Benchmark: Web Tasks + 16-Stream LLM Analysis ===");
    eprintln!("LLM Endpoint: {}", config.harness.endpoint);
    eprintln!("Model: {}", config.harness.model);
    eprintln!("LLM concurrency: {}", config.harness.max_concurrent);
    eprintln!("Browser concurrency: {}", config.max_browser_concurrent);
    eprintln!();

    // Run all predefined scenarios
    let report = runner.run_all().await?;

    // Print summary
    eprintln!("\n{}", "=".repeat(60));
    eprintln!("BROWSER BENCHMARK RESULTS");
    eprintln!("{}", "=".repeat(60));
    eprintln!(
        "Web tasks:   {}/{} passed",
        report.tasks_passed, report.tasks_total
    );
    if let Some(llm) = &report.llm_report {
        eprintln!(
            "LLM analysis: {}/{} passed",
            llm.tasks_passed, llm.tasks_total
        );
        eprintln!("Throughput:  {:.0} tok/s", llm.tokens_per_sec);
        eprintln!("Avg score:   {:.0}%", llm.avg_score * 100.0);
    }
    eprintln!("Duration:    {:.1}s", report.total_duration_secs);
    eprintln!("{}", "=".repeat(60));

    // Write reports
    report.write_to_dir(std::path::Path::new("bench_results/browser"))?;
    eprintln!("\nReports written to bench_results/browser/");

    // Print markdown
    eprintln!("\n{}", report.to_markdown());

    // Exit non-zero when any web task failed, so CI and scripts can detect a
    // broken run instead of a false success.
    if report.tasks_passed < report.tasks_total {
        eprintln!(
            "\n{} of {} task(s) failed.",
            report.tasks_total - report.tasks_passed,
            report.tasks_total
        );
        std::process::exit(1);
    }

    Ok(())
}
