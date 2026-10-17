use anyhow::Result;
use colored::Colorize;
#[cfg(feature = "bench-harness")]
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::ui::components::{render_header, WorkshopContext};
use crate::ui::style::SelfwareStyle;

use super::args::{BenchCommand, SWEBenchCommands, SwebenchProArgs};

pub(crate) struct LongTestCommandArgs {
    pub hours: u64,
    pub timeout: u64,
    pub max_iters: usize,
    pub concurrent: usize,
    pub endpoint: Option<String>,
    pub model: Option<String>,
    pub templates: Option<String>,
    pub output: String,
}

pub(crate) fn handle_swebench_command(command: SWEBenchCommands) -> Result<()> {
    match command {
        SWEBenchCommands::Diagnose { output_dir } => {
            run_swebench_diagnose(&output_dir)?;
        }
    }
    Ok(())
}

pub(crate) async fn handle_bench_command(
    config: &Config,
    ctx: &WorkshopContext,
    quiet: bool,
    command: Option<BenchCommand>,
    endpoint: Option<String>,
    suite: String,
    concurrent: usize,
) -> Result<()> {
    if let Some(sub) = command {
        return dispatch_bench_subcommand(sub, config).await;
    }
    if !quiet {
        println!("{}", render_header(ctx));
    }

    println!("\n{} Selfware Benchmark Suite\n", "📊".emphasis());
    println!("   Suites: {}", suite.clone().emphasis());
    println!("   Concurrent: {}", concurrent);
    if let Some(ref ep) = endpoint {
        println!("   Endpoint: {}", ep);
    }
    println!();

    // Run benchmarks
    let start_time = std::time::Instant::now();

    // 1. Endpoint health check
    println!("{} Checking endpoint health...", "⏳".dimmed());
    let ep = endpoint.as_deref().unwrap_or(&config.endpoint);

    let auto_cfg = crate::config::auto_config::AutoConfigurator::new(ep, None);
    match auto_cfg.fetch_models().await {
        Ok(models) => {
            println!(
                "{} Endpoint online: {} models available\n",
                "✓".green(),
                models.len()
            );
            for m in models.iter().take(3) {
                println!(
                    "   - {} ({} tokens)",
                    m.id.clone().emphasis(),
                    m.max_model_len
                );
            }
            if models.len() > 3 {
                println!("   ... and {} more", models.len() - 3);
            }
            println!();
        }
        Err(e) => {
            println!("{} Failed to connect: {}\n", "✗".red(), e);
        }
    }

    #[cfg(not(feature = "bench-harness"))]
    let mut unavailable_suites: Vec<&str> = Vec::new();

    // 2. Throughput benchmark
    if suite.contains("throughput") || suite.contains("e2e") || suite.contains("all") {
        println!(
            "{} Running throughput benchmark ({concurrent} concurrent)...",
            "⏳".dimmed()
        );

        let _ep = endpoint.as_deref().unwrap_or(&config.endpoint);
        let _model_name = config.model.clone();

        #[cfg(feature = "bench-harness")]
        {
            use crate::api::types::Message;
            use crate::bench_harness::*;

            let bench_config = HarnessConfig {
                endpoint: _ep.to_string(),
                model: _model_name.clone(),
                max_concurrent: concurrent,
                max_tokens: 256,
                temperature: 0.7,
                timeout_secs: config.agent.step_timeout_secs.max(300),
                output_dir: "bench_results/cli_bench".into(),
                extra_body: serde_json::json!({"chat_template_kwargs": {"enable_thinking": false}}),
                api_key: config.api_key.as_ref().map(|k| k.expose().to_string()),
                ..Default::default()
            };

            let runner = HarnessRunner::new(bench_config)?;
            let tasks: Vec<BenchTask> = (0..concurrent)
                .map(|i| {
                    let prompts = [
                        "What is 2+2? Answer with just the number.",
                        "Name the capital of France in one word.",
                        "Is Rust a compiled language? Yes or no.",
                        "What color is the sky? One word.",
                    ];
                    BenchTask {
                        id: format!("bench-{i}"),
                        description: format!("Quick test {i}"),
                        messages: vec![
                            Message::system("Answer concisely."),
                            Message::user(prompts[i % prompts.len()]),
                        ],
                        evaluator: Box::new(NoopEvaluator),
                    }
                })
                .collect();

            match runner.run(tasks).await {
                Ok(report) => {
                    if report.total_queue_wait_ms > 0 {
                        println!(
                            "{} Throughput: {:.0} tok/s ({:.0} tok/s active infer) | p50: {:.1}s ({:.1}s infer) | {}/{} passed\n",
                            "✓".green(),
                            report.tokens_per_sec,
                            report.inference_tokens_per_sec,
                            report.latency_p50_ms as f64 / 1000.0,
                            report.inference_p50_ms as f64 / 1000.0,
                            report.tasks_passed,
                            report.tasks_total,
                        );
                    } else {
                        println!(
                            "{} Throughput: {:.0} tok/s | p50: {:.1}s | {}/{} passed\n",
                            "✓".green(),
                            report.tokens_per_sec,
                            report.latency_p50_ms as f64 / 1000.0,
                            report.tasks_passed,
                            report.tasks_total,
                        );
                    }
                }
                Err(e) => println!("{} Throughput test failed: {}\n", "✗".red(), e),
            }
        }

        #[cfg(not(feature = "bench-harness"))]
        {
            println!(
                "{} Throughput benchmark unavailable: this build lacks the `bench-harness` feature\n",
                "✗".red()
            );
            unavailable_suites.push("throughput");
        }
    }

    // 3. Multi-language coding benchmark
    if suite.contains("multilang") || suite.contains("all") {
        println!("{} Running multi-language benchmark...", "⏳".dimmed());

        #[cfg(feature = "bench-harness")]
        {
            use crate::api::types::Message;
            use crate::bench_harness::*;

            let ep = endpoint.as_deref().unwrap_or(&config.endpoint);

            let bench_config = HarnessConfig {
                endpoint: ep.to_string(),
                model: config.model.clone(),
                max_concurrent: concurrent,
                max_tokens: 2048,
                temperature: 0.3,
                timeout_secs: config.agent.step_timeout_secs.max(300),
                output_dir: "bench_results/cli_bench".into(),
                extra_body: serde_json::json!({"chat_template_kwargs": {"enable_thinking": false}}),
                api_key: config.api_key.as_ref().map(|k| k.expose().to_string()),
                ..Default::default()
            };

            let runner = HarnessRunner::new(bench_config)?;
            let tasks = vec![
                BenchTask {
                    id: "rust".into(),
                    description: "Rust fibonacci".into(),
                    messages: vec![
                        Message::system("Output ONLY code."),
                        Message::user(
                            "Write a Rust function `fn fibonacci(n: u64) -> u64` iteratively.",
                        ),
                    ],
                    evaluator: Box::new(KeywordEvaluator::new(vec![
                        "fn fibonacci".into(),
                        "u64".into(),
                    ])),
                },
                BenchTask {
                    id: "python".into(),
                    description: "Python merge sort".into(),
                    messages: vec![
                        Message::system("Output ONLY code."),
                        Message::user("Write a Python function `def merge_sort(arr): ...`"),
                    ],
                    evaluator: Box::new(KeywordEvaluator::new(vec![
                        "def merge_sort".into(),
                        "merge".into(),
                    ])),
                },
                BenchTask {
                    id: "javascript".into(),
                    description: "JS debounce".into(),
                    messages: vec![
                        Message::system("Output ONLY code."),
                        Message::user(
                            "Write a JavaScript `function debounce(fn, delay)` that returns a debounced function.",
                        ),
                    ],
                    evaluator: Box::new(KeywordEvaluator::new(vec![
                        "function debounce".into(),
                        "setTimeout".into(),
                    ])),
                },
                BenchTask {
                    id: "go".into(),
                    description: "Go worker pool".into(),
                    messages: vec![
                        Message::system("Output ONLY code."),
                        Message::user(
                            "Write a Go worker pool with `func NewPool(workers int)`, `Submit(task func())`, `Wait()`.",
                        ),
                    ],
                    evaluator: Box::new(KeywordEvaluator::new(vec![
                        "func NewPool".into(),
                        "chan".into(),
                    ])),
                },
            ];

            match runner.run(tasks).await {
                Ok(report) => {
                    if report.total_queue_wait_ms > 0 {
                        println!(
                            "{} Multi-lang: {}/{} passed | {:.0} tok/s ({:.0} tok/s active infer)\n",
                            "✓".green(),
                            report.tasks_passed,
                            report.tasks_total,
                            report.tokens_per_sec,
                            report.inference_tokens_per_sec,
                        );
                    } else {
                        println!(
                            "{} Multi-lang: {}/{} passed | {:.0} tok/s\n",
                            "✓".green(),
                            report.tasks_passed,
                            report.tasks_total,
                            report.tokens_per_sec,
                        );
                    }
                    for r in &report.results {
                        let score = r
                            .eval
                            .as_ref()
                            .map(|e| format!("{:.0}%", e.score * 100.0))
                            .unwrap_or("ERR".into());
                        let icon = if r.success {
                            "✓".green().to_string()
                        } else {
                            "✗".red().to_string()
                        };
                        println!("     {} {} {}", icon, r.task_id, score);
                    }
                    println!();
                }
                Err(e) => println!("{} Multi-lang test failed: {}\n", "✗".red(), e),
            }
        }

        #[cfg(not(feature = "bench-harness"))]
        {
            println!(
                "{} Multi-language benchmark unavailable: this build lacks the `bench-harness` feature\n",
                "✗".red()
            );
            unavailable_suites.push("multilang");
        }
    }

    // Honest status: a suite that could not run is a failure, not a
    // silent skip under a green "Benchmark complete".
    #[cfg(not(feature = "bench-harness"))]
    if !unavailable_suites.is_empty() {
        return Err(bench_harness_unavailable(&format!(
            "bench --suite {}",
            unavailable_suites.join(",")
        )));
    }

    let elapsed = start_time.elapsed();
    println!(
        "{} Benchmark complete in {:.1}s\n",
        "✓".green(),
        elapsed.as_secs_f64()
    );
    Ok(())
}

pub(crate) async fn handle_long_test_command(
    #[allow(unused_variables)] config: &Config,
    ctx: &WorkshopContext,
    quiet: bool,
    args: LongTestCommandArgs,
) -> Result<()> {
    let LongTestCommandArgs {
        hours,
        timeout,
        max_iters,
        concurrent,
        endpoint,
        model,
        #[allow(unused_variables)]
        templates,
        output,
    } = args;

    if !quiet {
        println!("{}", render_header(ctx));
    }

    println!("\n{} Selfware Long-Running Test\n", "⏱️ ".emphasis());
    println!("   Duration: {} hours", hours.to_string().emphasis());
    println!("   Timeout: {}s per project", timeout);
    println!("   Max iterations: {}", max_iters);
    println!("   Concurrent: {}", concurrent.to_string().emphasis());
    println!("   Output: {}", output.clone().emphasis());
    if let Some(ref ep) = endpoint {
        println!("   Endpoint: {}", ep);
    }
    if let Some(ref m) = model {
        println!("   Model: {}", m);
    }
    println!();

    #[cfg(feature = "bench-harness")]
    {
        use crate::bench_harness::long_running::*;
        use std::time::Duration;

        let ep = endpoint.as_deref().unwrap_or(&config.endpoint);
        let mdl = model.as_deref().unwrap_or(&config.model);
        let _templates = templates; // Used to suppress unused warning when bench-harness disabled
        let tmpl = _templates
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("system_tests/projecte2e/templates"));

        let test_config = LongRunningConfig::new(ep, mdl)
            .with_duration(Duration::from_secs(hours * 3600))
            .with_project_timeout(timeout)
            .with_max_iterations(max_iters)
            .with_max_concurrent(concurrent)
            .with_templates_dir(tmpl)
            .with_output_dir(&output);

        println!("{} Initializing test runner...", "⏳".dimmed());
        let runner = match LongRunningRunner::new(test_config) {
            Ok(r) => {
                println!("{} Runner ready\n", "✓".green());
                r
            }
            Err(e) => {
                println!("{} Failed to initialize: {}\n", "✗".red(), e);
                anyhow::bail!("Failed to initialize long-running test runner: {}", e);
            }
        };

        println!(
            "{} Starting long-running test (press Ctrl+C to stop)\n",
            "🚀".emphasis()
        );

        let mut report = LongRunningReport::new();
        let start = std::time::Instant::now();

        // Example round: Core greenfield Rust projects
        let mut round_num = 1;
        while runner.should_continue() {
            println!(
                "  {} Round {} ({} remaining)",
                "▶".dimmed(),
                round_num,
                format!("{:.0}h", runner.time_remaining().as_secs_f64() / 3600.0).dimmed()
            );

            // Run a few example tasks
            let tasks = vec![TestTask {
                name: format!("calc_r{}", round_num),
                project_type: ProjectType::Rust,
                setup: TaskSetup::RustGreenfield {
                    name: "calculator".into(),
                },
                prompt: "Create a Calculator in src/lib.rs with add, subtract, multiply, divide (returns Result for div by zero). Write 5 unit tests. Run cargo test.".into(),
            }];

            let round_dir = PathBuf::from(&output).join(format!("round_{}", round_num));
            std::fs::create_dir_all(&round_dir).ok();

            let mut round_results = Vec::new();
            for task in tasks {
                let work_dir = round_dir.join(&task.name);
                let result = runner.run_task(&task, &work_dir).await;
                println!(
                    "    {} {}: {} ({}s, {}p/{}f)",
                    if matches!(result.status, ProjectStatus::Green) {
                        "✓".green()
                    } else if matches!(
                        result.status,
                        ProjectStatus::Partial | ProjectStatus::Compiles
                    ) {
                        "◐".yellow()
                    } else {
                        "✗".red()
                    },
                    result.name,
                    result.status.to_string().dimmed(),
                    result.duration_secs,
                    result.tests_passed,
                    result.tests_failed
                );
                report.add_result(result.clone());
                round_results.push(result);
            }

            report.add_round(RoundSummary {
                round_num,
                name: format!("Round {}", round_num),
                results: round_results,
            });

            round_num += 1;

            // Save progress after each round
            report.set_duration(start.elapsed().as_secs());
            if let Err(e) = report.write_to_dir(Path::new(&output)) {
                println!("    {} Failed to save report: {}", "⚠".yellow(), e);
            }
        }

        println!();
        let counts = report.count_by_status();
        println!("{} Test complete!\n", "✓".green());
        println!("  Total projects: {}", report.results.len());
        println!(
            "  🟢 GREEN: {}",
            counts.get(&ProjectStatus::Green).copied().unwrap_or(0)
        );
        println!(
            "  🟡 PARTIAL: {}",
            counts.get(&ProjectStatus::Partial).copied().unwrap_or(0)
        );
        println!(
            "  🔵 COMPILES: {}",
            counts.get(&ProjectStatus::Compiles).copied().unwrap_or(0)
        );
        println!(
            "  ⚪ WROTE: {}",
            counts.get(&ProjectStatus::Wrote).copied().unwrap_or(0)
        );
        println!(
            "  🔴 FAIL: {}",
            counts.get(&ProjectStatus::Fail).copied().unwrap_or(0)
        );
        println!();
        println!("  Report saved to: {}/report.md", output);
        println!();
        Ok(())
    }

    #[cfg(not(feature = "bench-harness"))]
    {
        Err(bench_harness_unavailable("long-test"))
    }
}

/// Dispatcher for the modern `selfware bench <subcommand>` surface.
pub(crate) async fn dispatch_bench_subcommand(sub: BenchCommand, _config: &Config) -> Result<()> {
    match sub {
        BenchCommand::SwebenchPro(args) => run_swebench_pro_cli(args).await,
    }
}

/// Resolve the three SWE-bench Pro official-eval paths. They are required only
/// when `--official-eval` is set; there are no machine-specific defaults.
#[cfg(any(feature = "bench-harness", test))]
pub(crate) fn resolve_official_eval_paths(
    args: &SwebenchProArgs,
) -> anyhow::Result<(std::path::PathBuf, std::path::PathBuf, std::path::PathBuf)> {
    let missing: Vec<&str> = [
        (
            args.official_eval_script.is_none(),
            "--official-eval-script",
        ),
        (
            args.official_eval_raw_sample_path.is_none(),
            "--official-eval-raw-sample-path",
        ),
        (
            args.official_eval_scripts_dir.is_none(),
            "--official-eval-scripts-dir",
        ),
    ]
    .into_iter()
    .filter_map(|(is_missing, flag)| is_missing.then_some(flag))
    .collect();
    if args.official_eval && !missing.is_empty() {
        anyhow::bail!(
            "--official-eval requires {}; pass the paths of your SWE-bench Pro checkout",
            missing.join(", ")
        );
    }
    Ok((
        std::path::PathBuf::from(args.official_eval_script.clone().unwrap_or_default()),
        std::path::PathBuf::from(
            args.official_eval_raw_sample_path
                .clone()
                .unwrap_or_default(),
        ),
        std::path::PathBuf::from(args.official_eval_scripts_dir.clone().unwrap_or_default()),
    ))
}

#[cfg(feature = "bench-harness")]
pub(crate) async fn run_swebench_pro_cli(args: SwebenchProArgs) -> Result<()> {
    use crate::bench_harness::swebench_pro::{
        catalog::{quant_catalog, DEFAULT_QUANTS},
        run_swebench_pro, SwebenchProOpts,
    };
    use std::time::Duration;

    // Fail fast: official-eval paths are required up front when --official-eval
    // is set, before any quant/dataset work starts.
    let (official_eval_script, official_eval_raw_sample_path, official_eval_scripts_dir) =
        resolve_official_eval_paths(&args)?;

    let quants: Vec<String> = if args.quants.eq_ignore_ascii_case("all") {
        quant_catalog().keys().map(|s| s.to_string()).collect()
    } else if args.quants.trim().is_empty() {
        DEFAULT_QUANTS.iter().map(|s| s.to_string()).collect()
    } else {
        args.quants
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    };

    let instance_ids: Vec<String> = args
        .instance_ids
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();

    let output = match args.output {
        Some(p) => PathBuf::from(p),
        None => {
            let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
            PathBuf::from("reports/swebench_pro").join(stamp)
        }
    };

    let selfware_bin = match args.selfware_bin {
        Some(p) => PathBuf::from(p),
        None => {
            std::env::current_exe().unwrap_or_else(|_| PathBuf::from("target/release/selfware"))
        }
    };

    let mut llama_opts = crate::bench_harness::swebench_pro::harness::LlamaServerOpts {
        port: args.port,
        ctx: args.ctx,
        parallel: args.parallel,
        ..Default::default()
    };
    if let Some(ref d) = args.models_dir {
        llama_opts.models_dir = std::path::PathBuf::from(d);
    }
    if let Some(ref b) = args.llama_server_bin {
        llama_opts.binary = std::path::PathBuf::from(b);
    }
    if let Some(ref ts) = args.tensor_split {
        // "auto"/"none"/empty disables the flag (e.g. single-GPU); else pass it through.
        llama_opts.tensor_split = match ts.as_str() {
            "auto" | "none" | "" => None,
            other => Some(other.to_string()),
        };
    }
    if let Some(ref h) = args.host {
        llama_opts.host = h.clone();
    }
    // Validate the prompt mode up front: an unknown value must fail loudly, not
    // silently fall back to diagnostic mode — which injects the oracle test
    // files (fail_to_pass) into the prompt and leaks the benchmark answer. A
    // `--prompt-mode offical` typo previously enabled that silently.
    if !matches!(args.prompt_mode.as_str(), "official" | "diagnostic") {
        anyhow::bail!(
            "invalid --prompt-mode '{}': expected 'official' or 'diagnostic'",
            args.prompt_mode
        );
    }
    // Same for the prompt profile: an unknown value silently fell back to the
    // Default profile (see runner.rs), so a typo ran the wrong prompt without
    // any signal. Only the two known profiles are valid.
    if !matches!(args.prompt_profile.as_str(), "swebench_pro" | "default") {
        anyhow::bail!(
            "invalid --prompt-profile '{}': expected 'swebench_pro' or 'default'",
            args.prompt_profile
        );
    }
    let opts = SwebenchProOpts {
        quants,
        instance_ids,
        instances: args.instances,
        scenario_timeout: Duration::from_secs(args.scenario_timeout),
        ctx: args.ctx,
        parallel: args.parallel,
        concurrency: args.concurrency,
        trials: args.trials,
        candidates: args.candidates,
        output,
        selfware_bin,
        skip_existing: args.skip_existing,
        resume: args.resume,
        force_rerun: args.force_rerun,
        prompt_mode: args.prompt_mode,
        prompt_profile: args.prompt_profile,
        official_eval: args.official_eval,
        official_eval_script,
        official_eval_raw_sample_path,
        official_eval_scripts_dir,
        official_eval_dockerhub_username: args.official_eval_dockerhub_username,
        official_eval_num_workers: args.official_eval_num_workers,
        official_eval_use_local_docker: !args.official_eval_modal,
        official_eval_redo: args.official_eval_redo,
        official_eval_block_network: args.official_eval_block_network,
        llama_opts,
        endpoint: args.endpoint,
        instances_jsonl: args.instances_jsonl.map(PathBuf::from),
    };

    // The harness is blocking (subprocess + file I/O) and intentionally serial;
    // run it inside `spawn_blocking` so we don't hold the runtime hostage.
    tokio::task::spawn_blocking(move || run_swebench_pro(opts))
        .await
        .map_err(|e| anyhow::anyhow!("swebench-pro task join error: {}", e))?
}

/// The error every `bench-harness`-gated subcommand returns on a build
/// compiled without that feature. It names the exact rebuild commands, and
/// flows through `errors::get_exit_code` as a plain error (`EXIT_ERROR`, 1),
/// the same code the other unavailable-feature paths (`--tui` without `tui`)
/// produce. The feature is deliberately NOT in the default set.
#[cfg(any(not(feature = "bench-harness"), test))]
pub(crate) fn bench_harness_unavailable(what: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "`selfware {what}` is unavailable: this binary was built without the \
         `bench-harness` feature. Rebuild with it:\n  \
         cargo install selfware --features bench-harness   (installed binary)\n  \
         cargo build --release --features bench-harness    (source checkout)"
    )
}

#[cfg(not(feature = "bench-harness"))]
pub(crate) async fn run_swebench_pro_cli(_args: SwebenchProArgs) -> Result<()> {
    Err(bench_harness_unavailable("bench swebench-pro"))
}

#[cfg(not(feature = "bench-harness"))]
pub(crate) fn run_swebench_diagnose(_output_dir: &str) -> Result<()> {
    Err(bench_harness_unavailable("swebench diagnose"))
}

#[cfg(feature = "bench-harness")]
pub(crate) fn run_swebench_diagnose(output_dir: &str) -> Result<()> {
    use crate::bench_harness::swebench_pro::trace::{DiagnosisSummary, PerRunDiagnosis, RunTrace};

    let output_path = std::path::Path::new(output_dir);
    if !output_path.exists() {
        anyhow::bail!("Output directory does not exist: {}", output_dir);
    }

    // Find all trace.jsonl files recursively.
    let mut trace_files = Vec::new();
    for entry in walkdir::WalkDir::new(output_path) {
        let entry = entry?;
        if entry.file_type().is_file() && entry.file_name() == "trace.jsonl" {
            trace_files.push(entry.path().to_path_buf());
        }
    }

    if trace_files.is_empty() {
        anyhow::bail!("No trace.jsonl files found in {}", output_dir);
    }

    eprintln!("Found {} trace file(s)", trace_files.len());

    let mut diagnoses: Vec<(RunTrace, PerRunDiagnosis)> = Vec::new();

    for path in &trace_files {
        let mut trace = match RunTrace::read_jsonl(path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("  ⚠ failed to read {}: {}", path.display(), e);
                continue;
            }
        };
        // Derive metadata from the file path if the trace header is empty.
        if trace.run_id.is_empty() {
            if let Some(parent) = path.parent() {
                trace.instance_id = parent
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
            }
        }
        let diag = PerRunDiagnosis::from_trace(&trace);

        // Write per-run diagnosis next to the trace file.
        let diagnosis_path = path.with_file_name("diagnosis.json");
        if let Err(e) = std::fs::write(&diagnosis_path, serde_json::to_vec_pretty(&diag)?) {
            eprintln!("  ⚠ failed to write {}: {}", diagnosis_path.display(), e);
        }

        diagnoses.push((trace, diag));
    }

    // Compute sweep-level summary.
    let summary = DiagnosisSummary::from_diagnoses(&diagnoses);
    let summary_path = output_path.join("diagnosis_summary.json");
    std::fs::write(&summary_path, serde_json::to_vec_pretty(&summary)?)?;

    eprintln!("Wrote diagnosis_summary.json to {}", summary_path.display());
    eprintln!(
        "  total_runs={} read_loop={:.1}% fake_complete={:.1}% timeout={:.1}%",
        summary.total_runs,
        summary.read_loop_rate * 100.0,
        summary.fake_complete_rate * 100.0,
        summary.timeout_rate * 100.0,
    );

    Ok(())
}
