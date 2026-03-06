//! Run the VLM benchmark suite against a live endpoint.
//!
//! Usage:
//!   cargo run --features vlm-bench --bin vlm_bench_run -- \
//!     --endpoint https://97921268284d.ngrok.app/v1 \
//!     --model "unsloth/Qwen3.5-9B-GGUF:Q8_0" \
//!     --concurrency 1

use clap::Parser;
use selfware::vlm_bench::config::VlmBenchConfig;
use selfware::vlm_bench::levels::all_levels;
use selfware::vlm_bench::runner::VlmBenchRunner;
use selfware::vlm_bench::Difficulty;

#[derive(Parser, Debug)]
#[command(name = "vlm_bench_run", about = "Run VLM benchmark suite")]
struct Args {
    /// VLM API endpoint (OpenAI-compatible /v1)
    #[arg(long, default_value = "http://192.168.1.99:1234/v1")]
    endpoint: String,

    /// Model name to request
    #[arg(long, default_value = "qwen/qwen3.5-9b")]
    model: String,

    /// Maximum concurrent requests
    #[arg(long, default_value_t = 1)]
    concurrency: usize,

    /// Maximum difficulty level to run (easy, medium, hard, veryhard, extreme, mega)
    #[arg(long)]
    max_difficulty: Option<String>,

    /// Maximum tokens per response
    #[arg(long, default_value_t = 4096)]
    max_tokens: usize,

    /// Sampling temperature
    #[arg(long, default_value_t = 0.2)]
    temperature: f32,

    /// Timeout per request in seconds
    #[arg(long, default_value_t = 120)]
    timeout: u64,

    /// Directory containing fixture images
    #[arg(long, default_value = "vlm_fixtures")]
    fixtures_dir: String,

    /// Directory for output reports
    #[arg(long, default_value = "vlm_results")]
    output_dir: String,
}

fn parse_difficulty(s: &str) -> Option<Difficulty> {
    match s.to_lowercase().as_str() {
        "easy" => Some(Difficulty::Easy),
        "medium" => Some(Difficulty::Medium),
        "hard" => Some(Difficulty::Hard),
        "veryhard" | "very_hard" | "very-hard" => Some(Difficulty::VeryHard),
        "extreme" => Some(Difficulty::Extreme),
        "mega" => Some(Difficulty::Mega),
        _ => None,
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    println!("VLM Benchmark Runner");
    println!("====================");
    println!("Endpoint:    {}", args.endpoint);
    println!("Model:       {}", args.model);
    println!("Concurrency: {}", args.concurrency);
    println!("Max tokens:  {}", args.max_tokens);
    println!("Temperature: {}", args.temperature);
    println!("Timeout:     {}s", args.timeout);
    println!();

    let mut config = VlmBenchConfig::new(&args.endpoint, &args.model)
        .with_concurrency(args.concurrency);
    config.max_tokens = args.max_tokens;
    config.temperature = args.temperature;
    config.timeout_secs = args.timeout;
    config.fixtures_dir = args.fixtures_dir.into();
    config.output_dir = args.output_dir.clone().into();

    if let Some(ref max_diff_str) = args.max_difficulty {
        if let Some(max_diff) = parse_difficulty(max_diff_str) {
            config = config.with_max_difficulty(max_diff);
            println!("Max difficulty: {}", max_diff);
        } else {
            anyhow::bail!(
                "Invalid difficulty: '{}'. Use: easy, medium, hard, veryhard, extreme, mega",
                max_diff_str
            );
        }
    }

    println!("Running {} difficulty levels...\n", config.levels.len());

    let levels = all_levels();
    let runner = VlmBenchRunner::new(config, levels)?;
    let report = runner.run().await?;

    // Print markdown summary to stdout
    println!("{}", report.to_markdown());

    // Write reports to disk
    let output_dir = std::path::Path::new(&args.output_dir);
    report.write_to_dir(output_dir)?;

    println!(
        "Reports written to {}/",
        output_dir.display()
    );
    println!(
        "  - vlm_benchmark_report.json",
    );
    println!(
        "  - vlm_benchmark_report.md",
    );

    Ok(())
}
