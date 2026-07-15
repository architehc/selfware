//! Live benchmark: 32 concurrent streams against local vLLM endpoint.
//!
//! Run with: cargo run --features bench-harness --example bench_32_streams

use selfware::api::types::Message;
use selfware::bench_harness::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("info,selfware::bench_harness=debug")
        .init();

    let config = HarnessConfig {
        endpoint: "http://localhost:8000/v1".into(),
        model: "qwen3.5-27b".into(),
        max_concurrent: 32,
        max_tokens: 512,
        temperature: 0.7,
        timeout_secs: 300,
        max_retries: 3,
        retry_delay_ms: 500,
        output_dir: "bench_results".into(),
        api_key: None,
        extra_body: serde_json::json!({
            "chat_template_kwargs": {"enable_thinking": false}
        }),
    };

    let runner = HarnessRunner::new(config.clone())?;

    // Generate 32 diverse tasks to max out GPU
    let tasks: Vec<BenchTask> = (0..32)
        .map(|i| {
            let (prompt, keywords) = match i % 8 {
                0 => (
                    "Explain the borrow checker in Rust in 3 sentences.".to_string(),
                    vec!["ownership".into(), "lifetime".into(), "borrow".into()],
                ),
                1 => (
                    "Write a Python function to compute Fibonacci numbers using memoization.".to_string(),
                    vec!["def".into(), "fibonacci".into(), "memo".into()],
                ),
                2 => (
                    "What are the SOLID principles in software engineering? List all 5.".to_string(),
                    vec!["single responsibility".into(), "open".into(), "liskov".into()],
                ),
                3 => (
                    "Explain how a B-tree differs from a binary search tree.".to_string(),
                    vec!["node".into(), "key".into(), "balanced".into()],
                ),
                4 => (
                    "Write a Rust function that implements binary search on a sorted slice.".to_string(),
                    vec!["fn".into(), "binary".into(), "search".into()],
                ),
                5 => (
                    "Describe the CAP theorem and its implications for distributed databases.".to_string(),
                    vec!["consistency".into(), "availability".into(), "partition".into()],
                ),
                6 => (
                    "What is the difference between async/await and threads? When should you use each?".to_string(),
                    vec!["async".into(), "thread".into(), "concur".into()],
                ),
                7 => (
                    "Explain how garbage collection works in Go vs reference counting in Rust.".to_string(),
                    vec!["garbage".into(), "reference".into(), "memory".into()],
                ),
                _ => unreachable!(),
            };

            BenchTask {
                id: format!("task-{i:02}"),
                description: prompt.clone(),
                messages: vec![
                    Message::system("You are a concise technical expert. Answer directly.".to_string()),
                    Message::user(prompt),
                ],
                evaluator: Box::new(KeywordEvaluator::new(keywords)),
            }
        })
        .collect();

    eprintln!("\n=== Benchmark: 32 Concurrent Streams ===");
    eprintln!("Endpoint: {}", config.endpoint);
    eprintln!("Model: {}", config.model);
    eprintln!("Tasks: {}", tasks.len());
    eprintln!("Max concurrent: {}", config.max_concurrent);
    eprintln!();

    let report = runner.run(tasks).await?;

    // Print summary
    eprintln!("\n{}", "=".repeat(60));
    eprintln!("RESULTS");
    eprintln!("{}", "=".repeat(60));
    eprintln!(
        "Tasks:       {}/{} passed",
        report.tasks_passed, report.tasks_total
    );
    eprintln!("Throughput:  {:.0} tok/s", report.tokens_per_sec);
    eprintln!("Avg score:   {:.0}%", report.avg_score * 100.0);
    eprintln!("Latency p50: {}ms", report.latency_p50_ms);
    eprintln!("Latency p95: {}ms", report.latency_p95_ms);
    eprintln!("Latency p99: {}ms", report.latency_p99_ms);
    eprintln!(
        "Total tokens: {} prompt + {} completion",
        report.total_prompt_tokens, report.total_completion_tokens
    );
    eprintln!("Duration:    {:.1}s", report.total_duration_secs);
    eprintln!("{}", "=".repeat(60));

    // Write reports
    report.write_to_dir(std::path::Path::new("bench_results"))?;
    eprintln!("\nReports written to bench_results/");

    // Also print markdown
    eprintln!("\n{}", report.to_markdown());

    Ok(())
}
