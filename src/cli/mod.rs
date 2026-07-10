//! Selfware Workshop - Your Personal AI Companion
//!
//! Software you own. Software that knows you. Software that lasts.

pub(crate) mod args;
pub(crate) mod headless;
pub(crate) mod init_wizard;

#[cfg(feature = "tui")]
use std::sync::mpsc;

use anyhow::Result;
use clap::Parser;
use colored::Colorize;
#[cfg(feature = "bench-harness")]
use std::path::{Path, PathBuf};
use tracing::warn;

// Use library exports instead of redeclaring modules
// This avoids duplicate compilation and maintains consistency
use crate::agent::Agent;
use crate::checkpoint;
use crate::cli::args::HeadlessOutputFormat;
use crate::config::{Config, ExecutionMode};
use crate::multiagent;
use crate::output;
use crate::telemetry::init_tracing;
use crate::ui;
use crate::ui::components::{
    render_header, render_task_complete, render_task_start, WorkshopContext,
};
use crate::ui::style::{Glyphs, SelfwareStyle};
use crate::ui::theme::{self, ThemeId};
use crate::workflows::{LlmCallOutput, LlmTokenUsage, VarValue, WorkflowExecutor};

use args::*;

const JOURNAL_DESC_MAX_CHARS: usize = 50;
const COMMIT_HASH_PREFIX_CHARS: usize = 8;
const MAX_JOURNAL_ERRORS_DISPLAY: usize = 3;
const DEFAULT_WORKFLOW_NAME: &str = "default";

/// Workflow file kind for determining how to handle workflow files
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkflowFileKind {
    Swl,
    Yaml,
}

/// Determine the workflow file kind from a path
fn workflow_file_kind(path: &std::path::Path) -> Option<WorkflowFileKind> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("swl") => Some(WorkflowFileKind::Swl),
        Some("yaml") | Some("yml") => Some(WorkflowFileKind::Yaml),
        _ => None,
    }
}

/// Parse workflow input variables in KEY=VALUE format
fn parse_workflow_inputs(values: &[String]) -> Result<std::collections::HashMap<String, VarValue>> {
    let mut inputs = std::collections::HashMap::new();
    for kv in values {
        if let Some((k, v)) = kv.split_once('=') {
            inputs.insert(k.to_string(), VarValue::String(v.to_string()));
        } else {
            anyhow::bail!("Invalid input format '{}', expected KEY=VALUE", kv);
        }
    }
    Ok(inputs)
}

fn workflow_llm_output_from_response(response: crate::api::ChatResponse) -> Result<LlmCallOutput> {
    let usage = LlmTokenUsage {
        prompt_tokens: response.usage.prompt_tokens as u64,
        completion_tokens: response.usage.completion_tokens as u64,
        total_tokens: response.usage.total_tokens as u64,
    };
    let estimated_cost_usd = estimate_workflow_llm_cost_usd(
        response.usage.prompt_tokens,
        response.usage.completion_tokens,
    );
    let model = response.model.clone();

    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("model returned no choices"))?;
    let text = choice.message.content.text_all();
    let content = if !text.trim().is_empty() {
        text
    } else if let Some(reasoning) = choice
        .reasoning_content
        .or(choice.message.reasoning_content)
    {
        reasoning
    } else {
        anyhow::bail!("model returned empty content");
    };

    Ok(LlmCallOutput::text(content)
        .with_model(model)
        .with_usage(usage)
        .with_estimated_cost(estimated_cost_usd))
}

fn print_workflow_telemetry(result: &crate::workflows::WorkflowResult) {
    if result.telemetry.llm_calls == 0 {
        return;
    }

    println!("\n   {} Telemetry:", Glyphs::journal());
    println!(
        "      LLM calls: {}",
        result.telemetry.llm_calls.to_string().garden_healthy()
    );
    println!(
        "      Tokens: {} in / {} out / {} total",
        result.telemetry.prompt_tokens,
        result.telemetry.completion_tokens,
        result.telemetry.total_tokens
    );
    println!(
        "      Estimated cost: ~${:.4}",
        result.telemetry.estimated_cost_usd
    );
    println!("      LLM latency: {}ms", result.telemetry.llm_latency_ms);
}

fn estimate_workflow_llm_cost_usd(prompt_tokens: usize, completion_tokens: usize) -> f64 {
    let prompt_cost_per_1m = 3.0;
    let completion_cost_per_1m = 15.0;

    (prompt_tokens as f64 / 1_000_000.0 * prompt_cost_per_1m)
        + (completion_tokens as f64 / 1_000_000.0 * completion_cost_per_1m)
}

fn workflow_agent_label(prompt: &str) -> String {
    prompt
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("SWL agent: "))
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .unwrap_or("workflow_llm")
        .to_string()
}

fn build_workflow_llm_handler(
    client: std::sync::Arc<crate::api::ApiClient>,
    model_label: String,
) -> crate::workflows::LlmHandler {
    Box::new(move |prompt, ctx| {
        let client = std::sync::Arc::clone(&client);
        let model_label = model_label.clone();
        let agent_label = workflow_agent_label(prompt);
        let request = if ctx.is_empty() {
            prompt.to_string()
        } else {
            format!("{prompt}\n\nContext:\n{}", ctx.join("\n"))
        };

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                crate::observability::telemetry::increment_api_requests();

                let started = std::time::Instant::now();
                let response = client
                    .chat(
                        vec![crate::api::Message::user(request)],
                        None,
                        crate::api::ThinkingMode::Disabled,
                    )
                    .await;
                let latency_ms = started.elapsed().as_secs_f64() * 1000.0;

                match response {
                    Ok(response) => {
                        let prompt_tokens = response.usage.prompt_tokens as u64;
                        let completion_tokens = response.usage.completion_tokens as u64;
                        let total_tokens = response.usage.total_tokens as u64;
                        let estimated_cost_usd = estimate_workflow_llm_cost_usd(
                            response.usage.prompt_tokens,
                            response.usage.completion_tokens,
                        );

                        tracing::info!(
                            model = %model_label,
                            agent = %agent_label,
                            latency_ms,
                            prompt_tokens,
                            completion_tokens,
                            total_tokens,
                            estimated_cost_usd,
                            "workflow llm request completed"
                        );
                        workflow_llm_output_from_response(response)
                    }
                    Err(err) => {
                        crate::observability::telemetry::increment_api_errors();
                        tracing::warn!(
                            model = %model_label,
                            agent = %agent_label,
                            latency_ms,
                            error = %err,
                            "workflow llm request failed"
                        );
                        Err(err)
                    }
                }
            })
        })
    })
}

/// Build a [`ToolHandler`](crate::workflows::ToolHandler) that dispatches Tool
/// steps to the real [`ToolRegistry`] behind the safety gate.
///
/// The handler converts the `HashMap<String, String>` args from the workflow
/// engine into a `serde_json::Value::Object`, then bridges the sync→async
/// boundary using `tokio::task::block_in_place` +
/// `Handle::current().block_on(...)` so we can `.await` the registry's async
/// `execute_any` call.
///
/// `execute_any` is used (instead of `execute`) so that deferred tools — which
/// are common in workflow scripts (e.g. `git_status`, `cargo_test`) — are also
/// reachable without requiring an explicit activation step.
fn build_workflow_tool_handler(
    safety_config: &crate::config::SafetyConfig,
) -> crate::workflows::ToolHandler {
    use crate::tools::ToolRegistry;
    use std::collections::HashMap;
    use std::sync::Arc;

    // Build the registry with the same safety config used elsewhere in the CLI.
    let registry = Arc::new(ToolRegistry::with_safety_config(Some(safety_config)));

    Box::new(move |tool_name: &str, args: &HashMap<String, String>| {
        let registry = Arc::clone(&registry);

        // Convert HashMap<String, String> → serde_json::Value::Object
        let mut json_map = serde_json::Map::new();
        for (k, v) in args {
            // Try to parse each value as JSON first (so numbers/bools/arrays
            // pass through correctly); fall back to a plain string.
            let parsed = serde_json::from_str::<serde_json::Value>(v)
                .unwrap_or(serde_json::Value::String(v.clone()));
            json_map.insert(k.clone(), parsed);
        }
        let input = serde_json::Value::Object(json_map);

        tracing::info!(
            tool = %tool_name,
            args = ?input,
            "workflow tool_handler dispatching to ToolRegistry"
        );

        // Bridge sync → async. block_in_place is safe on the multi-thread
        // runtime (the default for Selfware) and avoids the
        // "cannot block_on within async" panic.
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async move { registry.execute_any(tool_name, input).await })
        });

        match result {
            Ok(value) => Ok(value.to_string()),
            Err(err) => {
                tracing::warn!(tool = %tool_name, error = %err, "workflow tool execution failed");
                Err(err)
            }
        }
    })
}

/// Resolve the config file path relative to the original working directory.
///
/// When `-C <dir>` is used, the process changes its cwd *after* this function
/// runs.  That way an explicit `--config my.toml` or the implicit
/// `selfware.toml` default are always resolved against the directory the user
/// was in when they invoked the command.
///
/// Precedence (highest to lowest):
/// 1. Explicit `--config` path → expand `~`, then absolutify against `original_cwd`.
/// 2. `SELFWARE_CONFIG` env var → use as-is (must be absolute or resolvable).
///    This MUST be honoured before falling back to scanning original cwd —
///    otherwise `-C /workdir` combined with `SELFWARE_CONFIG=/abs/path.toml`
///    silently loads `./selfware.toml` from the original cwd instead of the
///    explicitly requested env-var path.
/// 3. No `--config` but `-C` is active → check `original_cwd/selfware.toml`.
///    If it exists, return its absolute path.  Otherwise return `None` so that
///    `Config::load` does its normal search (in the new cwd + home dir).
/// 4. Neither flag → return `None` (normal search).
fn resolve_config_path(
    config_flag: Option<&str>,
    has_workdir: bool,
    original_cwd: Option<&std::path::Path>,
) -> Option<String> {
    if let Some(p) = config_flag {
        // Expand ~ to home directory
        let expanded = if let Some(rest) = p.strip_prefix("~/") {
            match dirs::home_dir() {
                Some(home) => home.join(rest).to_string_lossy().to_string(),
                None => {
                    warn!(
                        "Could not resolve home directory for config path '{}'; using raw value",
                        p
                    );
                    p.to_string()
                }
            }
        } else {
            p.to_string()
        };

        // Make relative paths absolute against the original cwd
        return Some(if std::path::Path::new(&expanded).is_absolute() {
            expanded
        } else if let Some(cwd) = original_cwd {
            cwd.join(&expanded).to_string_lossy().to_string()
        } else {
            warn!(
                "Could not resolve current directory for config path '{}'; using raw value",
                expanded
            );
            expanded
        });
    }

    // SELFWARE_CONFIG must take precedence over scanning original_cwd.
    // Returning `None` here lets `Config::load` consult the env var on its
    // own; if we returned an absolute path from `original_cwd/selfware.toml`
    // here, it would override SELFWARE_CONFIG inside the loader.
    if std::env::var_os("SELFWARE_CONFIG").is_some() {
        return None;
    }

    if has_workdir {
        // No explicit --config and no SELFWARE_CONFIG, but -C is being used:
        // check for selfware.toml in the ORIGINAL directory first.  If found,
        // pass its absolute path so Config::load doesn't accidentally pick up
        // a different file in the -C target directory.
        if let Some(cwd) = original_cwd {
            let candidate = cwd.join("selfware.toml");
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }

    None
}

/// Decide whether auto-calibration should be skipped for the current CLI
/// invocation.
///
/// Returns `true` (skip calibration) when:
/// - The command is purely diagnostic (`config show`, `bench`, `doctor`, …).
/// - The user explicitly pointed at a config file (`--config`) — they know
///   what they want.
/// - The loaded config already looks configured (non-default endpoint or model).
///
/// Returns `false` (run calibration) only for `autoconfig` / `unpack`, or when
/// the install appears to be completely fresh (bare defaults, no CLI overrides).
fn should_skip_calibration(cli: &Cli, config: &Config) -> bool {
    // Commands that explicitly opt-in to calibration side-effects.
    let is_opt_in = matches!(
        cli.command,
        Some(Commands::AutoConfig { .. } | Commands::Unpack { .. })
    );
    if is_opt_in {
        return false;
    }

    // Diagnostic / read-only commands should never trigger port scans or
    // backend auto-starts.
    let is_diagnostic = matches!(
        cli.command,
        Some(
            Commands::Config { .. }
                | Commands::Bench { .. }
                | Commands::Doctor
                | Commands::LlmDoctor
                | Commands::Test { .. }
                | Commands::Status { .. }
                | Commands::Validate { .. }
        )
    );
    if is_diagnostic {
        return true;
    }

    // User pointed at a specific config file → respect their choice.
    if cli.config.is_some() {
        return true;
    }

    // Config already looks configured (non-default endpoint or model).
    let is_default_endpoint = config.endpoint == crate::config::default_endpoint();
    let is_default_model = config.model == crate::config::default_model();
    if !is_default_endpoint || !is_default_model {
        return true;
    }

    // Fresh install with bare defaults → allow calibration to run.
    false
}

/// Parse task-file contents into a list of tasks.
///
/// Each non-empty, non-comment line is a task.  Lines starting with `#`
/// are treated as comments and skipped, as are blank/whitespace-only lines.
/// Leading and trailing whitespace is trimmed from each task.
pub fn parse_task_file(contents: &str) -> Vec<String> {
    contents
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect()
}

pub async fn run() -> Result<()> {
    // Initialize telemetry
    init_tracing();

    let cli = Cli::parse();

    // Apply --no-color early to disable all color output
    if cli.no_color || std::env::var("NO_COLOR").is_ok() {
        colored::control::set_override(false);
    }

    // Apply --ascii mode (or SELFWARE_ASCII env var) for terminals without emoji support
    if cli.ascii || std::env::var("SELFWARE_ASCII").is_ok() {
        crate::ui::style::set_ascii_mode(true);
    }

    // Capture the original working directory BEFORE -C changes it.
    // This is needed so that config file paths (both explicit and default
    // "selfware.toml") are resolved relative to where the user invoked the
    // command, not relative to the -C target directory.
    let original_cwd = std::env::current_dir().ok();

    // Resolve config path BEFORE changing directories so that relative paths
    // (including the implicit "selfware.toml") resolve against the original cwd.
    let config_path = resolve_config_path(
        cli.config.as_deref(),
        cli.workdir.is_some(),
        original_cwd.as_deref(),
    );

    // Change to working directory
    if let Some(ref workdir) = cli.workdir {
        std::env::set_current_dir(workdir)
            .map_err(|e| anyhow::anyhow!("Cannot enter garden '{}': {}", workdir, e))?;

        if !cli.quiet {
            println!(
                "{} Entering garden: {}",
                Glyphs::sprout(),
                workdir.as_str().path_local()
            );
        }
    }

    let mut config = Config::load(config_path.as_deref())?;

    // ── Apply named configuration profile (if requested) ──
    // `--profile architect|swarm-8|batch-16|batch-32|visual|quick` applies
    // built-in overrides for max_tokens / temperature from `ProfileManager`.
    if let Some(ref profile_name) = cli.profile {
        let pm = crate::profiles::ProfileManager::new();
        match pm.apply_profile(&mut config, profile_name) {
            Ok(()) => {
                tracing::info!(
                    "Applied configuration profile '{}' (max_tokens={}, \
                     temperature={})",
                    profile_name,
                    config.max_tokens,
                    config.temperature
                );
            }
            Err(e) => {
                tracing::warn!("Could not apply profile '{}': {}", profile_name, e);
            }
        }
    }

    // ── Merge CLI debug flag onto config + re-apply env overrides ──
    // Precedence: defaults < TOML < CLI flag < env vars.  `Config::load`
    // already applied env overrides once after TOML parse; merging CLI on
    // top and re-applying env keeps env strictly the highest priority.
    if let Some(spec) = cli.debug.as_deref() {
        let cli_dbg = crate::config::DebugConfig::from_channel_list(spec);
        config.debug.merge_cli(&cli_dbg);
        config.debug.apply_env_overrides();
    }

    // ── Wire headless limit flags into config ──
    if let Some(max_turns) = cli.max_turns {
        config.agent.max_iterations = max_turns;
    }
    config.agent.max_budget_tokens = cli.max_budget_tokens;
    config.agent.max_wall_secs = cli.max_wall_secs;

    // ── Validate config and exit if requested ──
    if cli.validate_config {
        match config.validate() {
            Ok(()) => {
                println!("{} Configuration is valid.", Glyphs::bloom());
                return Ok(());
            }
            Err(e) => {
                eprintln!("{} Configuration validation failed: {}", Glyphs::frost(), e);
                std::process::exit(1);
            }
        }
    }

    // ── Auto-calibration: opt-in only ──
    // Calibration scans local ports, may auto-start backends, pull models, and
    // (re)write `selfware.toml`.  Those side effects are surprising for any
    // command that already has a working configuration, an explicit endpoint,
    // an explicit config path, or that is purely diagnostic (e.g. `bench`,
    // `config show`).  We therefore skip calibration unless the user has
    // *explicitly* opted in via `selfware autoconfig` / `selfware unpack`,
    // OR the loaded config is still on the bare defaults and the user did not
    // point at a specific config / endpoint via CLI or env.
    if !should_skip_calibration(&cli, &config) {
        if let Err(e) = crate::config::unpack::auto_calibrate(&mut config).await {
            tracing::warn!("Auto-calibration failed: {}", e);
        }
    }

    // Resolve execution mode: explicit CLI flags > --mode > env var (from Config::load)
    let exec_mode = if cli.daemon {
        ExecutionMode::Daemon
    } else if cli.yolo {
        ExecutionMode::Yolo
    } else if let Some(mode) = cli.mode {
        mode
    } else {
        config.execution_mode // Preserve SELFWARE_MODE env var / default
    };

    // Apply execution mode to config — record provenance for `config show`.
    let prior_exec_mode = config.execution_mode;
    config.execution_mode = exec_mode;
    if exec_mode != prior_exec_mode {
        let flag = if cli.daemon {
            "--daemon"
        } else if cli.yolo {
            "--yolo"
        } else {
            "--mode"
        };
        config.sources.set(
            "execution_mode",
            crate::config::ConfigSource::CliArg(flag.into()),
        );
    }

    if config.execution_mode == ExecutionMode::Daemon {
        let addr = "127.0.0.1:9090"
            .parse()
            .map_err(|e| anyhow::anyhow!("Failed to parse Prometheus address: {}", e))?;
        if let Err(e) = crate::telemetry::start_prometheus_exporter(addr) {
            tracing::warn!("Failed to start prometheus exporter: {}", e);
        } else {
            tracing::info!("Prometheus metrics exporter started on {}", addr);
        }
    }

    // Start the resource monitor for all run modes: without this, `[resources]`
    // config (GPU temperature/memory thresholds, memory
    // warning/critical/emergency thresholds, disk max_usage_percent) is
    // parsed but nothing ever constructs a ResourceManager or ticks its
    // monitor loop, so the OOM circuit breaker, GPU overheat throttle, and
    // disk-full guard never actually run -- an operator configuring
    // conservative thresholds for a long autonomous run gets no protection
    // at all. `_resource_monitor_shutdown_tx` must stay bound (not
    // `_`-dropped) for the duration of `run()`: monitor_loop's select! loop
    // busy-spins once every sender is dropped (tokio::watch::Receiver::changed()
    // resolves immediately with an error, but the loop only checks the
    // *value*, not sender liveness).
    let _resource_monitor_shutdown_tx = {
        match crate::resource::ResourceManager::new(&config.resources).await {
            Ok(manager) => {
                let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
                tokio::spawn(async move {
                    manager.monitor_loop(shutdown_rx).await;
                });
                tracing::info!(
                    "Resource monitor started (GPU/memory/disk thresholds now enforced)"
                );
                Some(shutdown_tx)
            }
            Err(e) => {
                tracing::warn!("Failed to start resource monitor: {}", e);
                None
            }
        }
    };

    // Apply UI settings from config file first
    config.apply_ui_settings();

    // CLI flags override config file settings
    // For theme, check if --theme was explicitly provided (not default)
    let theme_explicitly_set = std::env::args_os().any(|arg| {
        arg.to_str()
            .map(|s| s == "--theme" || s.starts_with("--theme="))
            .unwrap_or(false)
    });
    if theme_explicitly_set {
        let theme_id = match cli.theme {
            Theme::Amber => ThemeId::Amber,
            Theme::Ocean => ThemeId::Ocean,
            Theme::Minimal => ThemeId::Minimal,
            Theme::HighContrast => ThemeId::HighContrast,
        };
        theme::set_theme(theme_id);
    }

    // CLI flags override config for compact/verbose/show_tokens
    let compact = cli.compact || config.ui.compact_mode;
    let verbose = cli.verbose || config.ui.verbose_mode;
    let show_tokens = cli.show_tokens || config.ui.show_tokens;

    config.compact_mode = compact;
    config.verbose_mode = verbose;
    config.show_tokens = show_tokens;

    // Record CLI-arg provenance for the override flags.
    if cli.compact {
        config.sources.set(
            "ui.compact_mode",
            crate::config::ConfigSource::CliArg("--compact".into()),
        );
    }
    if cli.verbose {
        config.sources.set(
            "ui.verbose_mode",
            crate::config::ConfigSource::CliArg("--verbose".into()),
        );
    }
    if cli.show_tokens {
        config.sources.set(
            "ui.show_tokens",
            crate::config::ConfigSource::CliArg("--show-tokens".into()),
        );
    }

    // Apply plan mode from CLI
    if cli.plan {
        config.plan_mode = true;
        config.sources.set(
            "plan_mode",
            crate::config::ConfigSource::CliArg("--plan".into()),
        );
    }
    // Record `--debug` provenance when the flag is present.
    if cli.debug.is_some() {
        config.sources.set(
            "debug",
            crate::config::ConfigSource::CliArg("--debug".into()),
        );
    }

    // Initialize output control with merged settings
    output::init(compact, verbose, show_tokens);
    output::set_quiet(cli.quiet);

    let ctx = WorkshopContext::from_config(&config.endpoint, &config.model).with_mode(exec_mode);

    // Headless mode: run prompt directly and exit (like qwen -p)
    if let Some(prompt) = cli.prompt {
        // Support reading from stdin with "-p -"
        let actual_prompt = if prompt == "-" {
            use std::io::{self, Read};
            let mut buffer = String::new();
            io::stdin().read_to_string(&mut buffer)?;
            buffer.trim().to_string()
        } else {
            prompt
        };

        if actual_prompt.is_empty() {
            anyhow::bail!("Empty prompt provided");
        }

        let is_json = cli.output_format == HeadlessOutputFormat::Json;
        let is_stream_json = cli.output_format == HeadlessOutputFormat::StreamJson;
        let is_structured = is_json || is_stream_json;

        // In JSON/stream-JSON modes, suppress all human-oriented stdout so
        // only valid machine-readable JSON is emitted.
        if is_structured {
            output::set_json_mode(true);
        }

        if !cli.quiet && !is_structured {
            println!("{}", render_header(&ctx));
            println!(
                "\n{} {}\n",
                Glyphs::gear(),
                "Headless Mode".workshop_title()
            );
        }

        let start = std::time::Instant::now();
        let mut agent = Agent::new(config).await?;
        // Resume named session if --resume-session was provided (headless path)
        if let Some(ref session_name) = cli.resume_session {
            match agent.resume_named_session(session_name) {
                Ok(msg_count) => {
                    if !cli.quiet && !is_structured {
                        println!(
                            "▶ Resumed session '{}' ({} messages)",
                            session_name, msg_count
                        );
                    }
                }
                Err(e) => {
                    if !cli.quiet && !is_structured {
                        eprintln!("Failed to resume session '{}': {}", session_name, e);
                    }
                }
            }
        }
        let mut emitters: Vec<std::sync::Arc<dyn crate::agent::progress::ProgressEmitter>> =
            Vec::new();
        if is_stream_json {
            emitters.push(std::sync::Arc::new(headless::JsonlProgressEmitter::new()));
        } else if !cli.quiet {
            emitters.push(std::sync::Arc::new(
                crate::agent::progress::StderrProgressEmitter::new(),
            ));
        }
        #[cfg(feature = "bench-harness")]
        {
            if let Ok(result_dir) = std::env::var("SELFWARE_RESULT_DIR") {
                let trace_path = std::path::PathBuf::from(result_dir).join("trace.jsonl");
                if let Ok(emitter) =
                    crate::bench_harness::swebench_pro::trace::TraceProgressEmitter::new(
                        &trace_path,
                    )
                {
                    emitters.push(std::sync::Arc::new(emitter));
                }
            }
        }
        if !emitters.is_empty() {
            agent = agent.with_progress_emitter(std::sync::Arc::new(
                crate::agent::progress::MultiProgressEmitter::new(emitters),
            ));
        }
        let run_result = agent.run_task(&actual_prompt).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        if is_structured {
            let result = build_session_result(&agent, &run_result, duration_ms);
            headless::emit_result(&result);
        } else if !cli.quiet && run_result.is_ok() {
            println!("{}", render_task_complete(start.elapsed()));
        }
        if let Err(e) = &run_result {
            if !cli.quiet && !is_structured {
                eprintln!("✗ Task failed: {}", e);
            }
        }
        return run_result;
    }

    // Handle TUI dashboard mode
    #[cfg(feature = "tui")]
    {
        let should_use_tui = !cli.quiet && (cli.tui || (cli.command.is_none() && !cli.no_tui));
        if should_use_tui {
            return run_live_agent_tui(config).await;
        }
    }

    #[cfg(not(feature = "tui"))]
    if cli.tui {
        anyhow::bail!(
            "TUI dashboard requires the 'tui' feature. Rebuild with: cargo build --features tui"
        );
    }

    // Default to Chat if no subcommand specified (non-extras builds)
    let command = cli.command.unwrap_or(Commands::Chat { yolo: false });
    handle_command(
        command,
        cli.quiet,
        cli.coordinator,
        config,
        &ctx,
        exec_mode,
        cli.resume_session,
        cli.output_format,
        config_path,
    )
    .await
}

fn build_session_result(
    agent: &Agent,
    run_result: &Result<()>,
    duration_ms: u64,
) -> headless::SessionResult {
    let exit_status = if run_result.is_ok() { 0 } else { 1 };
    let stop_reason = match run_result {
        Ok(()) => agent
            .last_run_failure_mode()
            .map(|fm| fm.kind.tag().to_string())
            .unwrap_or_else(|| "completed".to_string()),
        Err(e) => format!("error: {}", e),
    };
    let num_turns = agent.current_iteration();
    let patch = headless::capture_patch().unwrap_or_default();
    let patch_bytes = patch.len();
    let patch_lines = patch.lines().count();
    let usage = agent.cumulative_token_usage().clone();
    let model = agent.model().to_string();
    let failure_mode = agent
        .last_run_failure_mode()
        .map(|fm| format!("{}: {}", fm.kind.tag(), fm.evidence));
    let artifact_dir = std::env::var("SELFWARE_RESULT_DIR")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            agent.current_checkpoint.as_ref().map(|c| {
                dirs::home_dir()
                    .unwrap_or_default()
                    .join(".selfware")
                    .join("checkpoints")
                    .join(&c.task_id)
            })
        });

    headless::SessionResult {
        session_id: agent
            .current_checkpoint
            .as_ref()
            .map(|c| c.task_id.clone())
            .unwrap_or_default(),
        exit_status,
        stop_reason,
        num_turns,
        patch_bytes,
        patch_lines,
        usage,
        model,
        duration_ms,
        failure_mode,
        artifact_dir,
    }
}

/// Launch the live agent-driven TUI dashboard.
///
/// This is the single entry point used by the default no-subcommand path,
/// `selfware dashboard`, and `selfware command-center`. It builds an
/// `Agent` with an event sender, spawns the TUI in a blocking thread,
/// processes user input, and cleans up on exit.
#[cfg(feature = "tui")]
async fn run_live_agent_tui(config: Config) -> Result<()> {
    let (event_tx, event_rx) = mpsc::channel();
    let (user_input_tx, user_input_rx) = mpsc::channel();
    let (permission_tx, permission_rx) = mpsc::channel();

    let mut agent = Agent::new(config.clone())
        .await?
        .with_event_sender(event_tx)
        .with_permission_channel(permission_rx);

    let shared_state = crate::ui::tui::SharedDashboardState::default();
    let model = config.model.clone();

    // Suppress ALL direct stdout/stderr from the agent — the TUI
    // owns the terminal and renders from events only.
    crate::output::set_tui_active(true);

    // Run TUI using spawn_blocking to properly integrate with tokio runtime
    let tui_handle = tokio::task::spawn_blocking(move || {
        crate::ui::tui::run_tui_dashboard_with_events(
            &model,
            shared_state,
            event_rx,
            user_input_tx,
            permission_tx,
        )
    });

    // Process user inputs from TUI.
    // The recv() is blocking (std::sync::mpsc), so we use block_in_place
    // to let tokio move other tasks off this thread while we wait.
    loop {
        let input = tokio::task::block_in_place(|| user_input_rx.recv());

        match input {
            Ok(ref input) if input == "exit" || input == "quit" => break,
            Ok(input) => {
                // Slash commands must NOT be sent to the LLM as prompts.
                // In TUI dashboard mode the full interactive command
                // dispatcher is not available, so we skip LLM routing
                // for any input starting with '/'.
                if input.starts_with('/') {
                    warn!(
                        "Slash command '{}' ignored in TUI mode — use interactive mode for command dispatch",
                        input
                    );
                    continue;
                }
                // Run the task — this will emit events to the TUI through event_tx
                if let Err(e) = agent.run_task(&input).await {
                    warn!("Agent failed to run task: {}", e);
                }
            }
            _ => break,
        }
    }

    crate::output::set_tui_active(false);

    // Auto-save conversation/session on exit so history isn't lost.
    if let Err(e) = agent.save_checkpoint("TUI session exit") {
        warn!("Failed to auto-save session on TUI exit: {}", e);
    }

    // Cleanup: await the TUI task with a bounded timeout so a stuck
    // TUI thread can never block shutdown indefinitely.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), tui_handle).await;
    Ok(())
}

async fn handle_command(
    command: Commands,
    quiet: bool,
    coordinator: bool,
    mut config: Config,
    ctx: &WorkshopContext,
    exec_mode: ExecutionMode,
    resume_session: Option<String>,
    output_format: HeadlessOutputFormat,
    config_path: Option<String>,
) -> Result<()> {
    match command {
        Commands::Chat { yolo } => {
            if yolo {
                config.execution_mode = ExecutionMode::Yolo;
            }
            if !quiet {
                println!("{}", ui::components::render_welcome(ctx));
            }
            let mut agent = Agent::new(config).await?;
            // If the subcommand --yolo flag was set, ensure the YoloManager
            // is enabled after construction.
            if yolo {
                agent.set_execution_mode(ExecutionMode::Yolo);
            }
            // Resume named session if --resume-session was provided
            if let Some(ref session_name) = resume_session {
                match agent.resume_named_session(session_name) {
                    Ok(msg_count) => {
                        if !quiet {
                            println!(
                                "▶ Resumed session '{}' ({} messages)",
                                session_name, msg_count
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to resume session '{}': {}", session_name, e);
                    }
                }
            }
            agent.interactive().await?;
        }

        Commands::MultiChat { concurrency, yolo } => {
            if yolo {
                config.execution_mode = ExecutionMode::Yolo;
            }
            // The --coordinator flag selects the swarm coordinator path.
            // When set, the multi-agent chat is orchestrated through the
            // Swarm coordinator (queue/assign/consensus) instead of the
            // plain fan-out.
            let use_coordinator = coordinator;
            if !quiet {
                println!("{}", render_header(ctx));
                if use_coordinator {
                    println!(
                        "\n{} {} with {} concurrent streams — {}",
                        Glyphs::gear(),
                        "Multi-Agent Workshop".workshop_title(),
                        concurrency.to_string().emphasis(),
                        "Coordinator (Swarm) Mode".bright_cyan()
                    );
                } else {
                    println!(
                        "\n{} {} with {} concurrent streams\n",
                        Glyphs::gear(),
                        "Multi-Agent Workshop".workshop_title(),
                        concurrency.to_string().emphasis()
                    );
                }
            }

            let agent_config =
                multiagent::MultiAgentConfig::default().with_concurrency(concurrency);
            let mut multi_agent = multiagent::MultiAgentChat::new(&config, agent_config)?;
            if use_coordinator {
                multi_agent.interactive_swarm().await?;
            } else {
                multi_agent.interactive().await?;
            }
        }

        Commands::Run { task, yolo } => {
            if yolo {
                config.execution_mode = ExecutionMode::Yolo;
            }
            let is_json = output_format == HeadlessOutputFormat::Json;
            let is_stream_json = output_format == HeadlessOutputFormat::StreamJson;
            let is_structured = is_json || is_stream_json;

            // In JSON/stream-JSON modes, suppress all human-oriented stdout so
            // only valid machine-readable JSON is emitted.
            if is_structured {
                output::set_json_mode(true);
            }

            if !quiet && !is_structured {
                println!("{}", render_header(ctx));
                println!("{}", render_task_start(&task));
            }

            let start = std::time::Instant::now();
            let mut agent = Agent::new(config).await?;
            // If the subcommand --yolo flag was set, ensure the YoloManager
            // is enabled after construction (covers the case where the
            // top-level --yolo was not passed but the subcommand flag was).
            if yolo {
                agent.set_execution_mode(ExecutionMode::Yolo);
            }
            // Resume named session if --resume-session was provided
            if let Some(ref session_name) = resume_session {
                match agent.resume_named_session(session_name) {
                    Ok(msg_count) => {
                        if !quiet && !is_structured {
                            println!(
                                "▶ Resumed session '{}' ({} messages)",
                                session_name, msg_count
                            );
                        }
                    }
                    Err(e) => {
                        if !quiet && !is_structured {
                            eprintln!("Failed to resume session '{}': {}", session_name, e);
                        }
                    }
                }
            }
            let mut emitters: Vec<std::sync::Arc<dyn crate::agent::progress::ProgressEmitter>> =
                Vec::new();
            if is_stream_json {
                emitters.push(std::sync::Arc::new(headless::JsonlProgressEmitter::new()));
            }
            #[cfg(feature = "bench-harness")]
            {
                if let Ok(result_dir) = std::env::var("SELFWARE_RESULT_DIR") {
                    let trace_path = std::path::PathBuf::from(result_dir).join("trace.jsonl");
                    if let Ok(emitter) =
                        crate::bench_harness::swebench_pro::trace::TraceProgressEmitter::new(
                            &trace_path,
                        )
                    {
                        emitters.push(std::sync::Arc::new(emitter));
                    }
                }
            }
            if !emitters.is_empty() {
                agent = agent.with_progress_emitter(std::sync::Arc::new(
                    crate::agent::progress::MultiProgressEmitter::new(emitters),
                ));
            }
            let run_result = agent.run_task(&task).await;
            let duration_ms = start.elapsed().as_millis() as u64;

            if is_structured {
                let result = build_session_result(&agent, &run_result, duration_ms);
                headless::emit_result(&result);
            } else if !quiet && run_result.is_ok() {
                println!("{}", render_task_complete(start.elapsed()));
            }
            if let Err(e) = &run_result {
                if !quiet && !is_structured {
                    eprintln!("✗ Task failed: {}", e);
                }
            }
            run_result?;
        }

        Commands::Analyze { path } => {
            if !quiet {
                println!("{}", render_header(ctx));
                println!(
                    "{} {} your garden at {}...\n",
                    Glyphs::magnifier(),
                    "Surveying".craftsman_voice(),
                    path.as_str().path_local()
                );
            }

            let mut agent = Agent::new(config).await?;
            agent.analyze(&path).await?;
        }

        Commands::Garden { path } => {
            if !quiet {
                println!("{}", render_header(ctx));
                println!(
                    "\n{} {} at {}...\n",
                    Glyphs::tree(),
                    "Visualizing your digital garden".craftsman_voice(),
                    path.as_str().path_local()
                );
            }

            // Build garden visualization
            let garden = ui::garden::build_garden_from_path(&path)?;
            println!("{}", garden.render());
        }

        Commands::Graph {
            path,
            focus,
            depth,
            max_nodes,
            format: output_format,
        } => {
            if !quiet {
                println!("{}", render_header(ctx));
                println!(
                    "{} {} at {}...\n",
                    Glyphs::tree(),
                    "Tracing the code graph".craftsman_voice(),
                    path.as_str().path_local()
                );
            }

            let mut options = crate::analysis::workspace_graph::WorkspaceGraphOptions::new(&path);
            options.focus = focus.clone();
            options.neighborhood_depth = depth;
            options.max_nodes = max_nodes;

            let graph = crate::analysis::workspace_graph::build_workspace_graph(&options)?;
            let summary = crate::analysis::workspace_graph::summarize_graph(&graph);
            let rendered = crate::analysis::code_graph::GraphRenderer::new()
                .with_direction("LR")
                .cluster()
                .render(&graph, graph_format_to_output_format(output_format));

            println!(
                "{}",
                render_graph_output(
                    &graph.name,
                    focus.as_deref(),
                    &summary,
                    output_format,
                    &rendered
                )
            );
        }

        #[cfg(feature = "tui")]
        Commands::Demo { scenario, fast } => {
            if !quiet {
                println!("{}", render_header(ctx));
            }
            run_demo_scenario(scenario, fast, quiet)?;
        }

        #[cfg(feature = "tui")]
        Commands::Dashboard { swarm_mode: _ } => {
            // Dashboard launches the same live agent-driven TUI as the
            // default no-subcommand path — no canned agent-less loop.
            return run_live_agent_tui(config).await;
        }

        #[cfg(feature = "tui")]
        Commands::CommandCenter {
            mode: _,
            refresh: _,
        } => {
            // Command Center now launches the same live agent-driven TUI
            // as the default no-subcommand path — no dead stub.
            return run_live_agent_tui(config).await;
        }

        Commands::Resume { task_id } => {
            if !quiet {
                println!("{}", render_header(ctx));
                println!(
                    "{} {} journal entry {}...",
                    Glyphs::bookmark(),
                    "Opening".craftsman_voice(),
                    task_id.as_str().emphasis()
                );
            }

            let mut agent = Agent::resume(config, &task_id).await?;
            if let Some(checkpoint) = &agent.current_checkpoint {
                let task = checkpoint.task_description.clone();
                if !quiet {
                    println!(
                        "{} Continuing: {}\n",
                        Glyphs::sprout(),
                        task.craftsman_voice()
                    );
                }
                agent.continue_execution().await?;
            }
        }

        Commands::Journal => {
            if !quiet {
                println!("{}", render_header(ctx));
            }
            let tasks = Agent::list_tasks()?;

            if tasks.is_empty() {
                println!(
                    "\n{} {} Your journal is empty. Start a task to create entries.\n",
                    Glyphs::journal(),
                    "Note:".muted()
                );
            } else {
                println!(
                    "\n{} {}\n",
                    Glyphs::journal(),
                    "Your Journal Entries:".workshop_title()
                );

                for task in tasks {
                    let status_glyph = match task.status {
                        checkpoint::TaskStatus::InProgress => Glyphs::gear(),
                        checkpoint::TaskStatus::Completed => Glyphs::bloom(),
                        checkpoint::TaskStatus::Failed => Glyphs::frost(),
                        checkpoint::TaskStatus::Paused => Glyphs::bookmark(),
                    };

                    let desc =
                        truncate_with_ellipsis(&task.task_description, JOURNAL_DESC_MAX_CHARS);

                    println!(
                        "   {} {} {}",
                        status_glyph,
                        task.task_id.muted(),
                        desc.craftsman_voice()
                    );
                    println!(
                        "      {} Step {} · {:?}",
                        Glyphs::branch().muted(),
                        task.current_step.to_string().muted(),
                        task.status
                    );
                }
                println!();
            }
        }

        Commands::JournalEntry { task_id } => {
            if !quiet {
                println!("{}", render_header(ctx));
            }
            let checkpoint = Agent::task_status(&task_id)?;

            println!(
                "\n{} {}\n",
                Glyphs::journal(),
                "Journal Entry".workshop_title()
            );

            let weather = match checkpoint.status {
                checkpoint::TaskStatus::InProgress => format!("{} Working", Glyphs::gear()),
                checkpoint::TaskStatus::Completed => format!("{} Complete", Glyphs::bloom()),
                checkpoint::TaskStatus::Failed => format!("{} Frost damage", Glyphs::frost()),
                checkpoint::TaskStatus::Paused => format!("{} Resting", Glyphs::leaf()),
            };

            println!(
                "   {} Entry ID:    {}",
                Glyphs::key(),
                checkpoint.task_id.muted()
            );
            println!("   {} Weather:     {}", Glyphs::sprout(), weather);
            println!(
                "   {} Step:        {}",
                Glyphs::branch().muted(),
                checkpoint.current_step
            );
            println!(
                "   {} Started:     {}",
                Glyphs::seedling(),
                checkpoint.created_at.timestamp()
            );
            println!(
                "   {} Last tended: {}",
                Glyphs::leaf(),
                checkpoint.updated_at.timestamp()
            );
            println!();
            println!(
                "   {} {}",
                Glyphs::journal(),
                "Reflection:".craftsman_voice()
            );
            println!("   {}", checkpoint.task_description.as_str().emphasis());
            println!();

            if let Some(ref git) = checkpoint.git_checkpoint {
                println!(
                    "   {} {}",
                    Glyphs::tree(),
                    "Garden State:".craftsman_voice()
                );
                println!("      Branch: {}", git.branch.as_str().path_local());
                println!(
                    "      Commit: {}",
                    take_prefix_chars(&git.commit_hash, COMMIT_HASH_PREFIX_CHARS)
                        .as_str()
                        .muted()
                );
                if git.dirty {
                    println!("      {} Uncommitted changes", Glyphs::wilt());
                }
                println!();
            }

            println!(
                "   {} Growth rings: {} messages, {} tool calls",
                Glyphs::harvest(),
                checkpoint.messages.len().to_string().garden_healthy(),
                checkpoint.tool_calls.len().to_string().muted()
            );

            if !checkpoint.errors.is_empty() {
                println!(
                    "\n   {} {}",
                    Glyphs::frost(),
                    "Frost damage:".garden_wilting()
                );
                for error in checkpoint
                    .errors
                    .iter()
                    .rev()
                    .take(MAX_JOURNAL_ERRORS_DISPLAY)
                {
                    println!(
                        "      Step {}: {}",
                        error.step,
                        error.error.as_str().muted()
                    );
                }
            }
            println!();
        }

        Commands::JournalDelete { task_id } => {
            Agent::delete_task(&task_id)?;
            if !quiet {
                println!(
                    "{} Journal entry {} has been composted.",
                    Glyphs::fallen_leaf(),
                    task_id.muted()
                );
            }
        }

        Commands::Status { output_format } => {
            // Count journal entries
            let tasks = match Agent::list_tasks() {
                Ok(tasks) => tasks,
                Err(err) => {
                    warn!("Failed to list journal entries for status: {}", err);
                    Vec::new()
                }
            };
            let completed = tasks
                .iter()
                .filter(|t| matches!(t.status, checkpoint::TaskStatus::Completed))
                .count();
            let in_progress = tasks
                .iter()
                .filter(|t| {
                    matches!(
                        t.status,
                        checkpoint::TaskStatus::InProgress | checkpoint::TaskStatus::Paused
                    )
                })
                .count();

            match output_format {
                OutputFormat::Json => {
                    let status = serde_json::json!({
                        "model": ctx.model_name,
                        "endpoint": config.endpoint,
                        "is_local": ctx.is_local_model,
                        "project_path": ctx.project_path,
                        "execution_mode": format!("{:?}", exec_mode),
                        "journal": {
                            "total": tasks.len(),
                            "completed": completed,
                            "in_progress": in_progress
                        }
                    });
                    println!("{}", serde_json::to_string_pretty(&status)?);
                }
                OutputFormat::Text => {
                    if !quiet {
                        println!("{}", render_header(ctx));
                    }
                    println!(
                        "\n{} {}\n",
                        Glyphs::home(),
                        "Workshop Status".workshop_title()
                    );

                    let hosting = if ctx.is_local_model {
                        format!("{} Running on your hardware (local)", Glyphs::home())
                            .garden_healthy()
                    } else {
                        format!("{} Connected to remote model", Glyphs::compass()).garden_wilting()
                    };

                    println!(
                        "   {} Model: {}",
                        Glyphs::gear(),
                        ctx.model_name.as_str().emphasis()
                    );
                    println!("   {}", hosting);
                    println!(
                        "   {} Garden: {}",
                        Glyphs::sprout(),
                        ctx.project_path.as_str().path_local()
                    );

                    println!(
                        "\n   {} Journal: {} entries ({} complete, {} in progress)",
                        Glyphs::journal(),
                        tasks.len().to_string().emphasis(),
                        completed.to_string().garden_healthy(),
                        in_progress.to_string().muted()
                    );

                    println!(
                        "\n   {} This is your software. It runs on your terms.\n",
                        Glyphs::key()
                    );
                }
            }
        }

        #[cfg(feature = "self-improvement")]
        Commands::Improve {
            dry_run,
            continuous,
            max_cycles,
        } => {
            use crate::cognitive::self_edit::SelfEditOrchestrator;

            if !quiet {
                println!("{}", render_header(ctx));
                println!(
                    "\n{} {}\n",
                    Glyphs::gear(),
                    "Self-Improvement Analysis".workshop_title()
                );
            }

            let project_root = std::env::current_dir()?;
            let orchestrator = SelfEditOrchestrator::new(project_root);
            let targets = orchestrator.analyze_self();

            if targets.is_empty() {
                println!(
                    "   {} No improvement targets found. The codebase looks good!",
                    Glyphs::bloom()
                );
                return Ok(());
            }

            println!(
                "   {} Found {} improvement targets:\n",
                Glyphs::magnifier(),
                targets.len().to_string().emphasis()
            );

            for (i, target) in targets.iter().take(10).enumerate() {
                let file_info = target.file.as_deref().unwrap_or("(no specific file)");
                println!(
                    "   {}. [{}] {} (priority: {:.2})",
                    i + 1,
                    target.category,
                    target.description,
                    target.priority
                );
                println!(
                    "      File: {} | Source: {:?}",
                    file_info.path_local(),
                    target.source
                );
            }

            if dry_run {
                println!("\n   {} Dry-run mode: no changes applied.", Glyphs::leaf());
                return Ok(());
            }

            let cycles = if continuous { max_cycles } else { 1 };
            let mut agent = Agent::new(config).await?;

            for cycle in 0..cycles {
                let targets = orchestrator.analyze_self();
                let Some(target) = orchestrator.select_target(&targets) else {
                    println!(
                        "\n   {} No more improvement targets. Done!",
                        Glyphs::bloom()
                    );
                    break;
                };

                println!(
                    "\n   {} Cycle {}/{}: applying '{}'",
                    Glyphs::gear(),
                    cycle + 1,
                    cycles,
                    target.description
                );

                let prompt = orchestrator.build_improvement_prompt(target);
                match agent.run_task(&prompt).await {
                    Ok(()) => {
                        println!("   {} Improvement applied successfully.", Glyphs::bloom());
                    }
                    Err(e) => {
                        println!("   {} Improvement failed: {}", Glyphs::frost(), e);
                    }
                }
            }
        }

        #[cfg(feature = "self-improvement")]
        Commands::Evolve {
            generations,
            population,
            parallel,
            dry_run,
            workflow,
        } => {
            if !quiet {
                println!("{}", render_header(ctx));
            }

            let repo_root = std::env::current_dir()?;

            if workflow == "rsi" {
                // RSI Orchestrator workflow: recursive self-improvement with
                // circuit breaker, fitness measurement, meta-learning, and
                // state persistence across restarts.
                use crate::cognitive::rsi_orchestrator::RSIOrchestrator;

                if !quiet {
                    println!(
                        "\n{} {}\n",
                        Glyphs::gear(),
                        "RSI Orchestrator".workshop_title()
                    );
                }

                if dry_run {
                    println!("   RSI project root: {}", repo_root.display());
                    println!("\n   {} Dry-run mode: no RSI loop started.", Glyphs::leaf());
                    return Ok(());
                }

                let mut orchestrator = RSIOrchestrator::new(repo_root);
                match orchestrator.run_loop().await {
                    Ok(()) => {
                        println!("\n   {} RSI loop completed successfully.", Glyphs::bloom());
                    }
                    Err(e) => {
                        println!("\n   {} RSI loop stopped: {}", Glyphs::frost(), e);
                    }
                }
            } else {
                // Default evolution daemon workflow
                use crate::evolution::daemon;
                use crate::evolution::{
                    EvolutionConfig, FitnessWeights, LlmConfig, MutationTargets, SafetyConfig,
                };

                if !quiet {
                    println!(
                        "\n{} {}\n",
                        Glyphs::gear(),
                        "Evolution Daemon".workshop_title()
                    );
                }

                let evo_config = EvolutionConfig {
                    generations,
                    population_size: population,
                    parallel_eval: parallel,
                    checkpoint_interval: 5,
                    fitness_weights: FitnessWeights::default(),
                    mutation_targets: MutationTargets {
                        config_keys: config.evolution.config_keys.clone(),
                        prompt_logic: config
                            .evolution
                            .prompt_logic
                            .iter()
                            .map(std::path::PathBuf::from)
                            .collect(),
                        tool_code: config
                            .evolution
                            .tool_code
                            .iter()
                            .map(std::path::PathBuf::from)
                            .collect(),
                        cognitive: config
                            .evolution
                            .cognitive
                            .iter()
                            .map(std::path::PathBuf::from)
                            .collect(),
                    },
                    safety: SafetyConfig::default(),
                    llm: {
                        // Use the hypothesis_model profile if configured, else fall back to default
                        let hypothesis_profile = config
                            .evolution
                            .hypothesis_model
                            .as_deref()
                            .and_then(|name| config.resolve_model(Some(name)));
                        if let Some(profile) = hypothesis_profile {
                            tracing::info!(
                                "Evolution using '{}' model profile for hypothesis generation: {}",
                                config
                                    .evolution
                                    .hypothesis_model
                                    .as_deref()
                                    .unwrap_or("default"),
                                profile.model
                            );
                            LlmConfig {
                                endpoint: profile.endpoint.clone(),
                                model: profile.model.clone(),
                                api_key: profile.api_key.as_ref().map(|k| k.expose().to_string()),
                                max_tokens: profile.max_tokens,
                                temperature: profile.temperature,
                            }
                        } else {
                            LlmConfig {
                                endpoint: config.endpoint.clone(),
                                model: config.model.clone(),
                                api_key: config.api_key.as_ref().map(|k| k.expose().to_string()),
                                max_tokens: config.max_tokens,
                                temperature: config.temperature,
                            }
                        }
                    },
                };

                if dry_run {
                    println!("   Evolution config: {:?}", evo_config);
                    println!(
                        "\n   {} Dry-run mode: no evolution started.",
                        Glyphs::leaf()
                    );
                    return Ok(());
                }

                let result = daemon::evolve(evo_config, &repo_root).await;

                println!(
                    "\n   {} Evolution complete: {} generations, {} improvements",
                    Glyphs::bloom(),
                    result.generations_run,
                    result.improvements.len()
                );
                println!(
                    "   SAB: {:.0} → {:.0} ({:+.1})",
                    result.initial_sab_score,
                    result.final_sab_score,
                    result.final_sab_score - result.initial_sab_score
                );
                println!("   Duration: {:.0}s", result.total_duration.as_secs_f64());
            }
        }

        Commands::Batch {
            file,
            workers,
            timeout,
            output,
            aggregate,
        } => {
            if !quiet {
                println!("{}", render_header(ctx));
            }

            // ── Read and parse the task file ──
            let contents = match std::fs::read_to_string(&file) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("✗ Cannot read task file '{}': {}", file, e);
                    anyhow::bail!("Failed to read batch task file '{}': {}", file, e);
                }
            };
            let tasks = parse_task_file(&contents);
            if tasks.is_empty() {
                if !quiet {
                    println!(
                        "No tasks found in '{}' (empty file or all lines blank/commented).",
                        file
                    );
                }
                return Ok(());
            }

            if !quiet {
                println!(
                    "\n{} Batch mode: {} task(s) from '{}'",
                    Glyphs::gear(),
                    tasks.len(),
                    file
                );
                if workers > 1 {
                    println!(
                        "⚠ Note: concurrent batch tasks share the same workspace and may conflict."
                    );
                    println!(
                        "  Running sequentially for safety (workers={} requested).",
                        workers
                    );
                }
            }

            // ── Run each task sequentially for workspace safety ──
            struct TaskResult {
                index: usize,
                task: String,
                ok: bool,
                error: Option<String>,
            }

            let mut results: Vec<TaskResult> = Vec::with_capacity(tasks.len());

            for (i, task) in tasks.iter().enumerate() {
                if !quiet {
                    println!("\n── Task {}/{}: {} ──", i + 1, tasks.len(), task);
                }
                let start = std::time::Instant::now();
                let mut agent = crate::agent::Agent::new(config.clone()).await?;
                let run_result = agent.run_task(task).await;
                let duration = start.elapsed();

                let (ok, error) = match &run_result {
                    Ok(()) => (true, None),
                    Err(e) => (false, Some(format!("{:#}", e))),
                };

                if !quiet {
                    let snippet = truncate_with_ellipsis(task, 60);
                    let status = if ok { "PASS" } else { "FAIL" };
                    if ok {
                        println!("  ✓ {} · {} ({}s)", status, snippet, duration.as_secs());
                    } else {
                        println!(
                            "  ✗ {} · {} ({}s) — {}",
                            status,
                            snippet,
                            duration.as_secs(),
                            error.as_deref().unwrap_or("unknown error")
                        );
                    }
                }

                results.push(TaskResult {
                    index: i,
                    task: task.clone(),
                    ok,
                    error,
                });
            }

            // ── Summary ──
            let total = results.len();
            let passed = results.iter().filter(|r| r.ok).count();
            let failed = total - passed;

            println!("\n{}", "─".repeat(60));
            println!(
                "Batch Summary: {} total, {} passed, {} failed",
                total, passed, failed
            );
            for r in &results {
                let snippet = truncate_with_ellipsis(&r.task, 50);
                let status = if r.ok { "PASS" } else { "FAIL" };
                if r.ok {
                    println!("  {} · {} · {}", r.index + 1, status, snippet);
                } else {
                    println!(
                        "  {} · {} · {} — {}",
                        r.index + 1,
                        status,
                        snippet,
                        r.error.as_deref().unwrap_or("unknown error")
                    );
                }
            }
            println!("{}", "─".repeat(60));

            // ── Write results to output directory ──
            let _ = std::fs::create_dir_all(&output);
            let summary_path = std::path::Path::new(&output).join("batch_summary.txt");
            let mut summary = format!(
                "Batch Summary: {} total, {} passed, {} failed\n\n",
                total, passed, failed
            );
            for r in &results {
                let status = if r.ok { "PASS" } else { "FAIL" };
                summary.push_str(&format!("{} · {} · {}", r.index + 1, status, r.task));
                if let Some(ref err) = r.error {
                    summary.push_str(&format!(" — {}", err));
                }
                summary.push('\n');
            }
            let _ = std::fs::write(&summary_path, &summary);
            if !quiet {
                println!("Results written to: {}", summary_path.display());
            }

            let _ = timeout;
            let _ = aggregate;

            if failed > 0 {
                anyhow::bail!(
                    "Batch completed with {} failed task(s) out of {}",
                    failed,
                    total
                );
            }
        }

        Commands::Validate {
            url,
            dir,
            iterations,
            target,
        } => {
            let url = url.clone();
            if !quiet {
                println!("{}", render_header(ctx));
            }
            anyhow::bail!(
                "Visual validation is not fully wired yet. Screenshot capture exists, but scoring/reporting is still placeholder-only.\n\
                 Requested target={} iterations={} url={}{}",
                target,
                iterations,
                url,
                dir.as_ref()
                    .map(|value| format!(" dir={}", value))
                    .unwrap_or_default()
            );
        }

        Commands::Workflow { command } => {
            use crate::swl::{parse_document, validate_document};
            use args::WorkflowCommands;

            if !quiet {
                println!("{}", render_header(ctx));
            }

            match command {
                WorkflowCommands::Validate { file } => {
                    let path = std::path::Path::new(&file);
                    if !path.exists() {
                        anyhow::bail!("Workflow file not found: {}", file);
                    }

                    match workflow_file_kind(path) {
                        Some(WorkflowFileKind::Swl) => {
                            println!(
                                "\n{} {}\n",
                                Glyphs::gear(),
                                "SWL Validation".workshop_title()
                            );
                            println!(
                                "   {} File: {}",
                                Glyphs::journal(),
                                file.as_str().path_local()
                            );

                            let source = std::fs::read_to_string(path)?;
                            let doc = match parse_document(&source) {
                                Ok(d) => d,
                                Err(e) => {
                                    println!("\n   {} Validation failed:\n", Glyphs::frost());
                                    println!("   {}\n", e);
                                    anyhow::bail!("SWL validation failed");
                                }
                            };

                            let issues = validate_document(&doc);
                            if issues.is_empty() {
                                println!("\n   {} SWL file is valid!", Glyphs::bloom());
                                println!("   Name: {}", doc.name.emphasis());
                                println!("   Version: {}", doc.version);
                                println!(
                                    "   Agents: {}",
                                    doc.agents.len().to_string().garden_healthy()
                                );
                                println!(
                                    "   Workflows: {}",
                                    doc.workflows.len().to_string().garden_healthy()
                                );
                                println!();
                            } else {
                                println!("\n   {} Validation issues found:\n", Glyphs::frost());
                                for issue in &issues {
                                    println!(
                                        "   - {}: {}",
                                        issue.path.clone().path_local(),
                                        issue.message
                                    );
                                }
                                println!();
                                anyhow::bail!("SWL validation failed with {} issues", issues.len());
                            }
                        }
                        Some(WorkflowFileKind::Yaml) => {
                            println!(
                                "\n{} {}\n",
                                Glyphs::gear(),
                                "YAML Workflow Validation".workshop_title()
                            );
                            println!(
                                "   {} File: {}",
                                Glyphs::journal(),
                                file.as_str().path_local()
                            );

                            let source = std::fs::read_to_string(path)?;
                            let workflow: crate::workflows::Workflow =
                                serde_yaml::from_str(&source).map_err(|e| {
                                    anyhow::anyhow!("Failed to parse workflow YAML: {}", e)
                                })?;

                            let mut executor = WorkflowExecutor::new_with_config(&config.safety);
                            executor.load_file(path)?;

                            println!("\n   {} YAML workflow is valid!", Glyphs::bloom());
                            println!("   Name: {}", workflow.name.emphasis());
                            println!("   Version: {}", workflow.version);
                            println!(
                                "   Steps: {}",
                                workflow.steps.len().to_string().garden_healthy()
                            );
                            println!(
                                "   Inputs: {}",
                                workflow.inputs.len().to_string().garden_healthy()
                            );
                            println!();
                        }
                        None => anyhow::bail!(
                            "Unsupported workflow file '{}'. Use .swl, .yaml, or .yml",
                            file
                        ),
                    }
                }

                WorkflowCommands::Run {
                    file,
                    workflow,
                    input,
                    dry_run,
                } => {
                    let path = std::path::Path::new(&file);
                    if !path.exists() {
                        anyhow::bail!("Workflow file not found: {}", file);
                    }

                    let inputs = parse_workflow_inputs(&input)?;

                    match workflow_file_kind(path) {
                        Some(WorkflowFileKind::Swl) => {
                            use crate::swl::{lower_document, parse_document};

                            let source = std::fs::read_to_string(path)?;
                            let doc = match parse_document(&source) {
                                Ok(d) => d,
                                Err(e) => {
                                    anyhow::bail!("Failed to parse SWL file: {}", e);
                                }
                            };

                            let lowered = lower_document(&doc)?;
                            let workflow_name = workflow
                                .clone()
                                .or_else(|| lowered.workflows.first().map(|w| w.name.clone()))
                                .or_else(|| doc.workflows.keys().next().cloned())
                                .unwrap_or_else(|| "main".to_string());

                            if dry_run {
                                println!(
                                    "\n{} {} (dry-run mode)\n",
                                    Glyphs::gear(),
                                    "SWL Workflow".workshop_title()
                                );
                                println!(
                                    "   {} File: {}",
                                    Glyphs::journal(),
                                    file.as_str().path_local()
                                );
                                if !inputs.is_empty() {
                                    println!("   {} Inputs: {:?}", Glyphs::journal(), inputs);
                                }
                                if !lowered.warnings.is_empty() {
                                    println!("   {} Lowering warnings:", Glyphs::fallen_leaf());
                                    for warning in &lowered.warnings {
                                        println!("      - {}", warning);
                                    }
                                }
                                println!();

                                let mut executor =
                                    WorkflowExecutor::new_dry_run_with_config(&config.safety);
                                for workflow in lowered.workflows {
                                    executor.register(workflow);
                                }

                                let working_dir = std::env::current_dir()?;
                                let result = executor
                                    .execute(&workflow_name, inputs, working_dir)
                                    .await?;
                                println!(
                                    "   {} Workflow completed (dry-run) in {}ms",
                                    Glyphs::bloom(),
                                    result.duration_ms
                                );
                                if !result.outputs.is_empty() {
                                    println!("   {} Outputs:", Glyphs::journal());
                                    for (name, value) in &result.outputs {
                                        println!(
                                            "      {} = {:?}",
                                            name.as_str().emphasis(),
                                            value
                                        );
                                    }
                                }
                                println!();
                            } else {
                                println!(
                                    "\n{} {}\n",
                                    Glyphs::gear(),
                                    "SWL Workflow".workshop_title()
                                );
                                println!(
                                    "   {} File: {}",
                                    Glyphs::journal(),
                                    file.as_str().path_local()
                                );
                                if !inputs.is_empty() {
                                    println!("   {} Inputs: {:?}", Glyphs::journal(), inputs);
                                }
                                if !lowered.warnings.is_empty() {
                                    println!("   {} Lowering warnings:", Glyphs::fallen_leaf());
                                    for warning in &lowered.warnings {
                                        println!("      - {}", warning);
                                    }
                                }
                                println!();

                                let mut executor =
                                    WorkflowExecutor::new_with_config(&config.safety);
                                for workflow in lowered.workflows {
                                    executor.register(workflow);
                                }

                                let client =
                                    std::sync::Arc::new(crate::api::ApiClient::new(&config)?);
                                executor = executor.with_llm_handler(build_workflow_llm_handler(
                                    client,
                                    config.model.clone(),
                                ));
                                executor = executor
                                    .with_tool_handler(build_workflow_tool_handler(&config.safety));

                                println!(
                                    "   {} Executing workflow: {}\n",
                                    Glyphs::compass(),
                                    workflow_name.as_str().emphasis()
                                );

                                let working_dir = std::env::current_dir()?;
                                let result = executor
                                    .execute(&workflow_name, inputs, working_dir)
                                    .await?;

                                match result.status {
                                    crate::workflows::WorkflowStatus::Completed => {
                                        println!(
                                            "\n   {} Workflow completed successfully in {}ms",
                                            Glyphs::flower(),
                                            result.duration_ms
                                        );
                                    }
                                    crate::workflows::WorkflowStatus::Failed => {
                                        println!(
                                            "\n   {} Workflow failed after {}ms",
                                            Glyphs::fallen_leaf(),
                                            result.duration_ms
                                        );
                                    }
                                    other => {
                                        println!(
                                            "\n   {} Workflow ended with status: {:?}",
                                            Glyphs::leaf(),
                                            other
                                        );
                                    }
                                }

                                print_workflow_telemetry(&result);
                                if !result.outputs.is_empty() {
                                    println!("\n   {} Outputs:", Glyphs::journal());
                                    for (name, value) in &result.outputs {
                                        println!(
                                            "      {} = {:?}",
                                            name.as_str().emphasis(),
                                            value
                                        );
                                    }
                                }
                                println!();
                            }
                        }
                        Some(WorkflowFileKind::Yaml) => {
                            let source = std::fs::read_to_string(path)?;
                            let workflow_name =
                                serde_yaml::from_str::<crate::workflows::Workflow>(&source)
                                    .map(|workflow| workflow.name)
                                    .unwrap_or_else(|_| default_workflow_name(path));

                            let mut executor = if dry_run {
                                println!(
                                    "\n{} {} (dry-run mode)\n",
                                    Glyphs::gear(),
                                    "YAML Workflow".workshop_title()
                                );
                                WorkflowExecutor::new_dry_run_with_config(&config.safety)
                            } else {
                                println!(
                                    "\n{} {}\n",
                                    Glyphs::gear(),
                                    "YAML Workflow".workshop_title()
                                );
                                WorkflowExecutor::new_with_config(&config.safety)
                            };

                            load_related_yaml_workflows(&mut executor, path)?;

                            if !dry_run {
                                let client =
                                    std::sync::Arc::new(crate::api::ApiClient::new(&config)?);
                                executor = executor.with_llm_handler(build_workflow_llm_handler(
                                    client,
                                    config.model.clone(),
                                ));
                                executor = executor
                                    .with_tool_handler(build_workflow_tool_handler(&config.safety));
                            }

                            println!(
                                "   {} File: {}",
                                Glyphs::journal(),
                                file.as_str().path_local()
                            );
                            println!(
                                "   {} Running workflow: {}",
                                Glyphs::compass(),
                                workflow_name.clone().emphasis()
                            );
                            if !inputs.is_empty() {
                                println!("   {} Inputs: {:?}", Glyphs::journal(), inputs);
                            }
                            println!();

                            let working_dir = std::env::current_dir()?;
                            let result = executor
                                .execute(&workflow_name, inputs, working_dir)
                                .await?;

                            match result.status {
                                crate::workflows::WorkflowStatus::Completed => {
                                    println!(
                                        "\n   {} Workflow completed successfully in {}ms",
                                        Glyphs::flower(),
                                        result.duration_ms
                                    );
                                }
                                crate::workflows::WorkflowStatus::Failed => {
                                    println!(
                                        "\n   {} Workflow failed after {}ms",
                                        Glyphs::fallen_leaf(),
                                        result.duration_ms
                                    );
                                }
                                other => {
                                    println!(
                                        "\n   {} Workflow ended with status: {:?}",
                                        Glyphs::leaf(),
                                        other
                                    );
                                }
                            }

                            print_workflow_telemetry(&result);
                            if !result.outputs.is_empty() {
                                println!("\n   {} Outputs:", Glyphs::journal());
                                for (name, value) in &result.outputs {
                                    println!("      {} = {:?}", name.as_str().emphasis(), value);
                                }
                            }
                            println!();
                        }
                        None => anyhow::bail!(
                            "Unsupported workflow file '{}'. Use .swl, .yaml, or .yml",
                            file
                        ),
                    }
                }

                WorkflowCommands::List { dir, all } => {
                    println!(
                        "\n{} {}\n",
                        Glyphs::gear(),
                        "Available Workflows".workshop_title()
                    );

                    let dir_path = std::path::Path::new(&dir);
                    if !dir_path.exists() {
                        anyhow::bail!("Directory not found: {}", dir);
                    }

                    // Find SWL files
                    let mut swl_files = Vec::new();
                    let mut yaml_files = Vec::new();

                    for entry in walkdir::WalkDir::new(dir_path).max_depth(2) {
                        let entry = entry?;
                        if entry.file_type().is_file() {
                            let path = entry.path();
                            if let Some(ext) = path.extension() {
                                if ext == "swl" {
                                    swl_files.push(path.to_path_buf());
                                } else if all && (ext == "yaml" || ext == "yml") {
                                    // Try to parse as workflow file
                                    if let Ok(content) = std::fs::read_to_string(path) {
                                        if content.contains("workflows:")
                                            || content.contains("steps:")
                                        {
                                            yaml_files.push(path.to_path_buf());
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if swl_files.is_empty() && yaml_files.is_empty() {
                        println!("   No workflow files found.\n");
                        println!(
                            "   {} Tip: Create .swl files to define workflows\n",
                            Glyphs::sprout()
                        );
                    } else {
                        if !swl_files.is_empty() {
                            println!("   {} SWL Workflows:", Glyphs::journal());
                            for file in &swl_files {
                                let name = file
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("unknown");
                                let rel_path =
                                    file.strip_prefix(dir_path).unwrap_or(file.as_path());
                                println!(
                                    "      {} {} ({})",
                                    Glyphs::flower(),
                                    name.to_string().emphasis(),
                                    rel_path.display()
                                );
                            }
                            println!();
                        }

                        if all && !yaml_files.is_empty() {
                            println!("   {} YAML Workflows:", Glyphs::journal());
                            for file in &yaml_files {
                                let name = file
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("unknown");
                                let rel_path =
                                    file.strip_prefix(dir_path).unwrap_or(file.as_path());
                                println!(
                                    "      {} {} ({})",
                                    Glyphs::leaf(),
                                    name,
                                    rel_path.display()
                                );
                            }
                            println!();
                        }

                        println!(
                            "   Found {} workflow file(s)\n",
                            (swl_files.len() + yaml_files.len())
                                .to_string()
                                .garden_healthy()
                        );
                    }
                }
            }
        }

        Commands::State { command } => {
            use crate::swl::state::backend::{FileBackend, StateBackend};
            use crate::swl::state::StateManager;
            use args::{StateCommands, StateOutputFormat};

            match command {
                StateCommands::Show {
                    workflow,
                    format,
                    dir,
                } => {
                    let base_dir = resolve_state_dir(dir);
                    let mut manager = StateManager::new_file_based(&workflow, base_dir)?;
                    manager.load().await?;
                    let state = manager.get_all();

                    match format {
                        StateOutputFormat::Json => {
                            println!("{}", serde_json::to_string_pretty(state)?);
                        }
                        StateOutputFormat::Text | StateOutputFormat::Table => {
                            if state.is_empty() {
                                println!("No saved state for workflow '{}'.", workflow);
                            } else {
                                println!("Workflow state: {}", workflow.emphasis());
                                for key in sorted_json_keys(state) {
                                    if let Some(value) = state.get(&key) {
                                        println!("  {} = {}", key, value);
                                    }
                                }
                            }
                        }
                    }
                }
                StateCommands::List { dir } => {
                    let backend = FileBackend::new(resolve_state_dir(dir))?;
                    let mut workflows = backend.list().await?;
                    workflows.sort();
                    if workflows.is_empty() {
                        println!("No saved workflow state found.");
                    } else {
                        for workflow in workflows {
                            println!("{workflow}");
                        }
                    }
                }
                StateCommands::Delete {
                    workflow,
                    dir,
                    force,
                } => {
                    if !force {
                        anyhow::bail!(
                            "Refusing to delete workflow state without --force: {}",
                            workflow
                        );
                    }
                    let backend = FileBackend::new(resolve_state_dir(dir))?;
                    backend.delete(&workflow).await?;
                    println!("Deleted state for workflow '{}'.", workflow);
                }
                StateCommands::Export {
                    workflow,
                    output,
                    dir,
                } => {
                    let base_dir = resolve_state_dir(dir);
                    let mut manager = StateManager::new_file_based(&workflow, base_dir)?;
                    manager.load().await?;
                    let payload = serde_json::to_string_pretty(manager.get_all())?;
                    std::fs::write(&output, payload)?;
                    println!("Exported state for '{}' to {}.", workflow, output);
                }
                StateCommands::Import {
                    workflow,
                    input,
                    dir,
                    force,
                } => {
                    let base_dir = resolve_state_dir(dir);
                    let backend = FileBackend::new(base_dir.clone())?;
                    if backend.exists(&workflow).await && !force {
                        anyhow::bail!(
                            "State for '{}' already exists. Re-run with --force to overwrite.",
                            workflow
                        );
                    }

                    let content = std::fs::read_to_string(&input)?;
                    let state: std::collections::HashMap<String, serde_json::Value> =
                        serde_json::from_str(&content)?;

                    let mut manager = StateManager::new_file_based(&workflow, base_dir)?;
                    manager.clear();
                    manager.set_all(state)?;
                    manager.save().await?;
                    println!("Imported state for '{}' from {}.", workflow, input);
                }
            }
        }

        Commands::Config { command } => match command {
            ConfigCommands::Show { json } => {
                config_show(&config, json)?;
            }
        },

        Commands::McpServer => {
            crate::mcp::server::run_mcp_server(&config, config_path.as_deref())
                .await?;
        }

        Commands::Lsp => {
            crate::lsp::run_lsp_server().await?;
        }

        Commands::LlmDoctor => {
            if !quiet {
                println!("{}", render_header(ctx));
            }
            crate::llm_doctor::run_llm_doctor(&config).await?;
        }

        Commands::Test { pattern, format } => {
            if !quiet {
                println!("{}", render_header(ctx));
            }
            println!("\n{} Running Tests\n", "🧪".emphasis());
            println!("   Pattern: {}", pattern.clone().emphasis());
            println!("   Format: {}\n", format);
            run_local_tests(&pattern, &format).await?;
        }

        Commands::SWEBench { command } => {
            use args::SWEBenchCommands;
            match command {
                SWEBenchCommands::Run {
                    dataset,
                    limit,
                    output,
                } => {
                    if !quiet {
                        println!("{}", render_header(ctx));
                    }
                    anyhow::bail!(
                        "SWE-bench evaluation is not implemented yet. The current path only uses embedded demo data and placeholder success.\n\
                         Requested dataset={}{} output={}. Use the repo's experimental examples/scripts instead.",
                        dataset,
                        limit
                            .map(|value| format!(", limit {}", value))
                            .unwrap_or_default(),
                        output
                    );
                }
                SWEBenchCommands::Diagnose { output_dir } => {
                    run_swebench_diagnose(&output_dir)?;
                }
            }
        }

        Commands::Bench {
            command,
            endpoint,
            suite,
            concurrent,
            format: _format,
        } => {
            if let Some(sub) = command {
                return dispatch_bench_subcommand(sub, &config).await;
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
                        timeout_secs: 120,
                        output_dir: "bench_results/cli_bench".into(),
                        extra_body: serde_json::json!({"chat_template_kwargs": {"enable_thinking": false}}),
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
                            println!(
                                "{} Throughput: {:.0} tok/s | p50: {:.1}s | {}/{} passed\n",
                                "✓".green(),
                                report.tokens_per_sec,
                                report.latency_p50_ms as f64 / 1000.0,
                                report.tasks_passed,
                                report.tasks_total,
                            );
                        }
                        Err(e) => println!("{} Throughput test failed: {}\n", "✗".red(), e),
                    }
                }

                #[cfg(not(feature = "bench-harness"))]
                {
                    println!(
                        "{} Benchmark requires --features bench-harness\n",
                        "✗".red()
                    );
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
                        timeout_secs: 120,
                        output_dir: "bench_results/cli_bench".into(),
                        extra_body: serde_json::json!({"chat_template_kwargs": {"enable_thinking": false}}),
                        ..Default::default()
                    };

                    let runner = HarnessRunner::new(bench_config)?;
                    let tasks = vec![
                        BenchTask {
                            id: "rust".into(), description: "Rust fibonacci".into(),
                            messages: vec![Message::system("Output ONLY code."), Message::user("Write a Rust function `fn fibonacci(n: u64) -> u64` iteratively.")],
                            evaluator: Box::new(KeywordEvaluator::new(vec!["fn fibonacci".into(), "u64".into()])),
                        },
                        BenchTask {
                            id: "python".into(), description: "Python merge sort".into(),
                            messages: vec![Message::system("Output ONLY code."), Message::user("Write a Python function `def merge_sort(arr): ...`")],
                            evaluator: Box::new(KeywordEvaluator::new(vec!["def merge_sort".into(), "merge".into()])),
                        },
                        BenchTask {
                            id: "javascript".into(), description: "JS debounce".into(),
                            messages: vec![Message::system("Output ONLY code."), Message::user("Write a JavaScript `function debounce(fn, delay)` that returns a debounced function.")],
                            evaluator: Box::new(KeywordEvaluator::new(vec!["function debounce".into(), "setTimeout".into()])),
                        },
                        BenchTask {
                            id: "go".into(), description: "Go worker pool".into(),
                            messages: vec![Message::system("Output ONLY code."), Message::user("Write a Go worker pool with `func NewPool(workers int)`, `Submit(task func())`, `Wait()`.")],
                            evaluator: Box::new(KeywordEvaluator::new(vec!["func NewPool".into(), "chan".into()])),
                        },
                    ];

                    match runner.run(tasks).await {
                        Ok(report) => {
                            println!(
                                "{} Multi-lang: {}/{} passed | {:.0} tok/s\n",
                                "✓".green(),
                                report.tasks_passed,
                                report.tasks_total,
                                report.tokens_per_sec,
                            );
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
                println!(
                    "{} Benchmark requires --features bench-harness\n",
                    "✗".red()
                );
            }

            let elapsed = start_time.elapsed();
            println!(
                "{} Benchmark complete in {:.1}s\n",
                "✓".green(),
                elapsed.as_secs_f64()
            );
        }

        Commands::LongTest {
            hours,
            timeout,
            max_iters,
            concurrent,
            endpoint,
            model,
            #[allow(unused_variables)]
            templates,
            output,
        } => {
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
                        return Ok(());
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
                    let tasks = vec![
                        TestTask {
                            name: format!("calc_r{}", round_num),
                            project_type: ProjectType::Rust,
                            setup: TaskSetup::RustGreenfield {
                                name: "calculator".into(),
                            },
                            prompt: "Create a Calculator in src/lib.rs with add, subtract, multiply, divide (returns Result for div by zero). Write 5 unit tests. Run cargo test.".into(),
                        },
                    ];

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
            }

            #[cfg(not(feature = "bench-harness"))]
            {
                println!(
                    "{} Long-running test requires --features bench-harness\n",
                    "✗".red()
                );
            }
        }

        Commands::Doctor => {
            if !quiet {
                println!("{}", render_header(ctx));
            }
            let report = crate::doctor::run_doctor(config_path.as_deref()).await;
            report.print();
            let code = report.exit_code();
            if code != 0 {
                // Return an error so main.rs can exit gracefully with a
                // non-zero exit code, instead of calling process::exit
                // which would skip cleanup (tracing flush, etc.).
                return Err(anyhow::anyhow!(
                    "doctor: one or more checks failed (exit code {})",
                    code
                ));
            }
            // Also run the LLM backend probe so `doctor` actually tests
            // the model.  An unreachable/failing model is a real failure.
            crate::llm_doctor::run_llm_doctor(&config).await?;
        }

        Commands::AutoConfig {
            endpoint,
            model,
            api_key,
            toml,
            save,
        } => {
            use crate::config::auto_config::AutoConfigurator;
            use colored::Colorize;

            let endpoint = endpoint.unwrap_or_else(|| config.endpoint.clone());
            let api_key = api_key
                .as_deref()
                .or(config.api_key.as_ref().map(|k| k.expose()));

            println!("{}", render_header(ctx));
            println!("\n{} Auto-Configuration Detection\n", "⚙️".emphasis());
            println!("   Endpoint: {}", endpoint.clone().emphasis());

            // Show the model-defaults profile that was matched (if any) at
            // config-load time. This is purely static (driven by config.model)
            // and never makes network calls, so it is safe to print before
            // we even talk to the backend.
            match (&config.matched_profile, &config.matched_profile_applied) {
                (Some(name), applied) if !applied.is_empty() => {
                    println!(
                        "   Profile: {} (applied: {})",
                        name.clone().emphasis(),
                        applied.join(", ")
                    );
                }
                (Some(name), _) => {
                    println!(
                        "   Profile: {} (no fields applied — user config covers all)",
                        name.clone().emphasis()
                    );
                }
                (None, _) => {
                    println!(
                        "   Profile: {} (no built-in defaults for model '{}')",
                        "none".dimmed(),
                        config.model
                    );
                }
            }

            let configurator = AutoConfigurator::new(&endpoint, api_key);

            let models = match configurator.fetch_models().await {
                Ok(m) => m,
                Err(e) => {
                    println!("\n{} Failed to fetch models: {}", "✗".red().bold(), e);
                    std::process::exit(1);
                }
            };

            if models.is_empty() {
                println!("\n{} No models found at endpoint", "✗".red().bold());
                std::process::exit(1);
            }

            println!("   Found {} model(s)", models.len());

            let model_to_test = model.unwrap_or_else(|| models[0].id.clone());
            println!("   Testing model: {}\n", model_to_test.clone().emphasis());

            let results = match configurator.run_tests(&model_to_test).await {
                Ok(r) => r,
                Err(e) => {
                    println!("\n{} Tests failed: {}", "✗".red().bold(), e);
                    std::process::exit(1);
                }
            };

            let detected_config = match configurator.generate_config(&model_to_test).await {
                Ok(c) => c,
                Err(e) => {
                    println!("\n{} Config generation failed: {}", "✗".red().bold(), e);
                    std::process::exit(1);
                }
            };

            println!("\n{} Detection Results:\n", "📊".emphasis());
            println!(
                "   Backend: {}",
                results
                    .backend_type
                    .map(|b| b.name())
                    .unwrap_or("Unknown")
                    .emphasis()
            );
            println!(
                "   Function Calling: {}",
                if results.function_calling {
                    "✓ Supported".green().to_string()
                } else {
                    "✗ Not detected".yellow().to_string()
                }
            );
            println!(
                "   Streaming: {}",
                if results.streaming {
                    "✓ Supported".green().to_string()
                } else {
                    "✗ Not detected".yellow().to_string()
                }
            );
            println!(
                "   Chat API: {}",
                if results.chat_works {
                    "✓ Working".green().to_string()
                } else {
                    "✗ Failed".red().to_string()
                }
            );

            if toml || save {
                configurator.print_config_toml(&detected_config);
            }

            if save {
                let mut toml_str = format!(
                    r#"# Auto-configured by selfware auto-config
endpoint = "{}"
model = "{}"
max_tokens = {}
context_length = {}
temperature = {}

[safety]
allowed_paths = ["./**", "/tmp/**"]
denied_paths = ["**/.env", "**/secrets/**", "**/.ssh/**"]
protected_branches = ["main"]

[agent]
native_function_calling = {}
streaming = {}
token_budget = {}
step_timeout_secs = {}

[continuous_work]
enabled = true
checkpoint_interval_tools = 10
checkpoint_interval_secs = 300
auto_recovery = true
max_recovery_attempts = 3
"#,
                    detected_config.endpoint,
                    detected_config.model,
                    detected_config.max_tokens,
                    detected_config.context_length,
                    detected_config.temperature,
                    detected_config.agent.native_function_calling,
                    detected_config.agent.streaming,
                    detected_config.agent.token_budget,
                    detected_config.agent.step_timeout_secs
                );

                // Include extra_body if present (e.g., thinking mode config)
                if let Some(ref extra) = detected_config.extra_body {
                    toml_str.push_str("\n[extra_body]\n");
                    for (k, v) in extra {
                        if let Some(obj) = v.as_object() {
                            let inner: Vec<String> =
                                obj.iter().map(|(ik, iv)| format!("{ik} = {iv}")).collect();
                            toml_str.push_str(&format!("{k} = {{ {} }}\n", inner.join(", ")));
                        } else {
                            toml_str.push_str(&format!("{k} = {v}\n"));
                        }
                    }
                }

                toml_str.push_str(
                    "\n[retry]\nmax_retries = 5\nbase_delay_ms = 1000\nmax_delay_ms = 60000\n",
                );

                std::fs::write("selfware.toml", &toml_str)?;
                println!("{} Configuration saved to selfware.toml", "✓".green());
            }

            println!();
        }

        Commands::Unpack { scan, save } => {
            use crate::config::unpack;
            use colored::Colorize;

            println!("{}", render_header(ctx));

            match unpack::unpack().await {
                Ok(Some(detected_config)) => {
                    if !scan {
                        if save {
                            match unpack::save_unpack_config(&detected_config) {
                                Ok(path) => {
                                    println!(
                                        "  {} Saved configuration to {}",
                                        "✓".bright_green(),
                                        path.display().to_string().bright_white()
                                    );
                                }
                                Err(e) => {
                                    eprintln!("  {} Failed to save config: {}", "✗".red(), e);
                                }
                            }
                        } else {
                            println!(
                                "  {} Run {} to persist this configuration.",
                                "→".dimmed(),
                                "selfware unpack --save".bright_white()
                            );
                        }
                    }
                }
                Ok(None) => {
                    println!(
                        "\n  {} Could not auto-configure. Please set up a local LLM backend first.",
                        "⚠️".yellow()
                    );
                }
                Err(e) => {
                    eprintln!("\n  {} Unpack failed: {}", "✗".red(), e);
                }
            }
            println!();
        }

        Commands::Init { template } => {
            tokio::task::spawn_blocking(move || init_wizard::run_init_wizard(template))
                .await
                .map_err(|e| anyhow::anyhow!("Task panicked: {}", e))??;
        }

        Commands::Runs { command } => {
            use crate::supervision::run_registry::{AbortOutcome, RunRecord, RunRegistry};
            use crate::supervision::run_supervisor::{RunStatus, RunSupervisor};
            use args::RunsCommand;

            let registry = RunRegistry::new()?;

            match command {
                RunsCommand::Start { task } => {
                    let supervisor = RunSupervisor::new();
                    if !quiet {
                        println!("{} Starting supervised run…", Glyphs::gear());
                    }
                    let id = supervisor.start(task.clone(), config.clone()).await;
                    let composite = format!("{}-{}", std::process::id(), id.0);
                    let now = chrono::Utc::now().timestamp();
                    let record = RunRecord {
                        id: composite.clone(),
                        pid: std::process::id(),
                        task: task.clone(),
                        status: "Running".to_string(),
                        started_unix: now,
                        updated_unix: now,
                    };
                    if let Err(e) = registry.write(&record) {
                        warn!("failed to persist run record: {}", e);
                    }
                    println!("started {}", composite);

                    if let Some(mut rx) = supervisor.attach(&id).await {
                        loop {
                            if let Some(st) = supervisor.status(&id).await {
                                if matches!(
                                    st,
                                    RunStatus::Completed
                                        | RunStatus::Failed
                                        | RunStatus::Aborted
                                ) {
                                    break;
                                }
                            }
                            match rx.recv().await {
                                Ok(ev) => {
                                    if !quiet {
                                        println!("  [{composite}] {ev:?}");
                                    }
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                    continue;
                                }
                            }
                        }
                    }

                    let status_str = match supervisor.status(&id).await {
                        Some(RunStatus::Completed) => "Completed",
                        Some(RunStatus::Failed) => "Failed",
                        Some(RunStatus::Aborted) => "Aborted",
                        Some(RunStatus::Running) | Some(RunStatus::Paused) => "Running",
                        None => "Failed",
                    };
                    let _ = registry.update_status(
                        &composite,
                        status_str,
                        chrono::Utc::now().timestamp(),
                    );
                    println!("{} final status: {}", composite, status_str);
                }

                RunsCommand::List => {
                    let runs = registry.list()?;
                    if runs.is_empty() {
                        println!(
                            "No runs recorded. (Runs are persisted under \
                             ~/.selfware/runs/ as they start.)"
                        );
                    } else {
                        println!("{:<22} {:<10} {}", "ID", "STATUS", "TASK");
                        for r in runs {
                            let task_short: String = r.task.chars().take(50).collect();
                            println!(
                                "{:<22} {:<10} {}",
                                r.id,
                                r.effective_status(),
                                task_short
                            );
                        }
                    }
                }

                RunsCommand::Abort { id } => {
                    match registry.abort(&id, chrono::Utc::now().timestamp())? {
                        AbortOutcome::NotFound => println!(
                            "No run '{}' in the registry (~/.selfware/runs/).",
                            id
                        ),
                        AbortOutcome::AlreadyDone => {
                            println!("Run '{}' is already finished.", id)
                        }
                        AbortOutcome::WasStale => println!(
                            "Run '{}' owner process was already gone; marked Aborted.",
                            id
                        ),
                        AbortOutcome::Signalled(pid) => println!(
                            "Sent SIGTERM to process {} for run '{}'. \
                             Note: this terminates that whole process (which may \
                             host other runs).",
                            pid, id
                        ),
                    }
                }
            }
        }
    }

    Ok(())
}

fn graph_format_to_output_format(format: GraphFormat) -> crate::analysis::code_graph::OutputFormat {
    match format {
        GraphFormat::Ascii => crate::analysis::code_graph::OutputFormat::Ascii,
        GraphFormat::Mermaid => crate::analysis::code_graph::OutputFormat::Mermaid,
        GraphFormat::Dot => crate::analysis::code_graph::OutputFormat::Dot,
        GraphFormat::Json => crate::analysis::code_graph::OutputFormat::Json,
        GraphFormat::Plantuml => crate::analysis::code_graph::OutputFormat::PlantUml,
    }
}

fn render_graph_output(
    graph_name: &str,
    focus: Option<&str>,
    summary: &crate::analysis::workspace_graph::WorkspaceGraphSummary,
    format: GraphFormat,
    rendered: &str,
) -> String {
    let focus_line = focus
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("Focus: {}\n", value))
        .unwrap_or_default();

    let summary_text = format!(
        "Workspace Graph: {}\n{}Nodes: {} | Edges: {} | Files: {} | Functions: {} | Types: {} | Imports: {} | Type deps: {} | Cycles: {}\n",
        graph_name,
        focus_line,
        summary.node_count,
        summary.edge_count,
        summary.file_count,
        summary.function_count,
        summary.type_count,
        summary.import_edges,
        summary.dependency_edges,
        summary.cycle_count,
    );

    match format {
        GraphFormat::Ascii => format!("{}\n{}", summary_text, rendered),
        GraphFormat::Mermaid => format!("{summary_text}\n```mermaid\n{rendered}\n```"),
        GraphFormat::Dot => format!("{summary_text}\n```dot\n{rendered}\n```"),
        GraphFormat::Json => format!("{summary_text}\n```json\n{rendered}\n```"),
        GraphFormat::Plantuml => format!("{summary_text}\n```plantuml\n{rendered}\n```"),
    }
}

#[cfg(feature = "tui")]
fn run_demo_scenario(scenario: DemoScenarioKind, fast: bool, quiet: bool) -> Result<()> {
    use crate::ui::demo::{
        BugHuntSafariScenario, CodebaseArchaeologyScenario, DemoConfig, DemoRunner, DemoScenario,
        FeatureFactoryScenario, TokenChallengeScenario,
    };

    let config = if fast {
        DemoConfig::fast()
    } else {
        DemoConfig::default()
    };
    let step_delay = config.step_delay;
    let mut runner = DemoRunner::new(config);

    let mut scenario_impl: Box<dyn DemoScenario> = match scenario {
        DemoScenarioKind::Archaeology => Box::new(CodebaseArchaeologyScenario::new()),
        DemoScenarioKind::FeatureFactory => Box::new(FeatureFactoryScenario::new()),
        DemoScenarioKind::BugHunt => Box::new(BugHuntSafariScenario::new()),
        DemoScenarioKind::TokenChallenge => Box::new(TokenChallengeScenario::new()),
    };

    if !quiet {
        println!(
            "\n{} Running demo: {}\n",
            Glyphs::gear(),
            scenario_impl.name().emphasis()
        );
    }

    runner.start(scenario_impl.as_mut());
    while runner.next_stage(scenario_impl.as_mut()) {
        runner.update(0.16);
        if !quiet {
            println!(
                "   {} Stage {}/{}",
                Glyphs::branch(),
                runner.current_stage(),
                runner.total_stages()
            );
        }
        std::thread::sleep(step_delay);
    }

    if !quiet {
        println!(
            "\n{} Demo complete in {:.2}s\n",
            Glyphs::bloom(),
            runner.elapsed().as_secs_f64()
        );
    }

    Ok(())
}

fn truncate_with_ellipsis(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }

    let keep_chars = max_chars.saturating_sub(3);
    let mut out: String = input.chars().take(keep_chars).collect();
    out.push_str("...");
    out
}

fn take_prefix_chars(input: &str, max_chars: usize) -> String {
    input.chars().take(max_chars).collect()
}

fn default_workflow_name(path: &std::path::Path) -> String {
    match path.file_stem().and_then(|s| s.to_str()) {
        Some(name) => name.to_string(),
        None => {
            warn!(
                "Could not infer workflow name from file '{}'; using '{}'",
                path.display(),
                DEFAULT_WORKFLOW_NAME
            );
            DEFAULT_WORKFLOW_NAME.to_string()
        }
    }
}

fn resolve_state_dir(dir: Option<String>) -> std::path::PathBuf {
    dir.map(std::path::PathBuf::from)
        .unwrap_or_else(crate::swl::state::StateBackendType::default_state_dir)
}

fn sorted_json_keys(state: &std::collections::HashMap<String, serde_json::Value>) -> Vec<String> {
    let mut keys: Vec<String> = state.keys().cloned().collect();
    keys.sort();
    keys
}

fn local_test_command(pattern: &str) -> Result<Vec<String>> {
    let args = match pattern {
        "all" => vec!["test".to_string(), "--all-targets".to_string()],
        "unit" => vec!["test".to_string(), "--test".to_string(), "unit".to_string()],
        "integration" => vec![
            "test".to_string(),
            "--features".to_string(),
            "integration".to_string(),
            "--test".to_string(),
            "integration".to_string(),
        ],
        "e2e" => vec![
            "test".to_string(),
            "--all-targets".to_string(),
            "e2e".to_string(),
        ],
        "workflow" => vec![
            "test".to_string(),
            "--all-targets".to_string(),
            "workflow".to_string(),
        ],
        other if !other.trim().is_empty() => vec![
            "test".to_string(),
            "--all-targets".to_string(),
            other.to_string(),
        ],
        _ => anyhow::bail!("Test pattern must not be empty"),
    };

    Ok(args)
}

async fn run_local_tests(pattern: &str, format: &str) -> Result<()> {
    let args = local_test_command(pattern)?;
    let output = tokio::process::Command::new("cargo")
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(1);

    match format {
        "json" => {
            let payload = serde_json::json!({
                "command": format!("cargo {}", args.join(" ")),
                "success": output.status.success(),
                "exit_code": exit_code,
                "stdout": stdout,
                "stderr": stderr,
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        "text" | "pretty" => {
            if !stdout.trim().is_empty() {
                print!("{stdout}");
            }
            if !stderr.trim().is_empty() {
                eprint!("{stderr}");
            }
        }
        other => anyhow::bail!("Unsupported test output format: {}", other),
    }

    if !output.status.success() {
        anyhow::bail!("Test command failed with exit code {}", exit_code);
    }

    Ok(())
}

fn load_related_yaml_workflows(
    executor: &mut WorkflowExecutor,
    path: &std::path::Path,
) -> Result<()> {
    let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));

    for entry in walkdir::WalkDir::new(dir).max_depth(1) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let workflow_path = entry.path();
        if workflow_file_kind(workflow_path) == Some(WorkflowFileKind::Yaml) {
            executor.load_file(workflow_path)?;
        }
    }

    Ok(())
}

/// Render the effective configuration with provenance annotations.
///
/// Mirrors `selfware config show`. Each row is `key = value [source]`,
/// formatted into aligned columns for readability.
pub(crate) fn config_show(config: &Config, json: bool) -> Result<()> {
    use crate::config::ConfigSource;

    let mut rows: Vec<(String, String, ConfigSource)> = Vec::new();

    rows.push((
        "endpoint".to_string(),
        config.endpoint.clone(),
        config.source_of("endpoint"),
    ));
    rows.push((
        "model".to_string(),
        config.model.clone(),
        config.source_of("model"),
    ));
    rows.push((
        "temperature".to_string(),
        format!("{}", config.temperature),
        config.source_of("temperature"),
    ));
    rows.push((
        "max_tokens".to_string(),
        format!("{}", config.max_tokens),
        config.source_of("max_tokens"),
    ));
    rows.push((
        "context_length".to_string(),
        format!("{}", config.context_length),
        config.source_of("context_length"),
    ));

    rows.push((
        "agent.native_function_calling".to_string(),
        format!("{}", config.agent.native_function_calling),
        config.source_of("agent.native_function_calling"),
    ));
    rows.push((
        "agent.streaming".to_string(),
        format!("{}", config.agent.streaming),
        config.source_of("agent.streaming"),
    ));
    rows.push((
        "agent.max_iterations".to_string(),
        format!("{}", config.agent.max_iterations),
        config.source_of("agent.max_iterations"),
    ));
    rows.push((
        "agent.step_timeout_secs".to_string(),
        format!("{}", config.agent.step_timeout_secs),
        config.source_of("agent.step_timeout_secs"),
    ));

    if let Some(extra) = &config.extra_body {
        let mut keys: Vec<&String> = extra.keys().collect();
        keys.sort();
        for k in keys {
            let key = format!("extra_body.{}", k);
            let val = match extra.get(k) {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(v) => v.to_string(),
                None => String::new(),
            };
            let source = config.source_of(&key);
            rows.push((key, val, source));
        }
    }

    if json {
        let entries: Vec<serde_json::Value> = rows
            .iter()
            .map(|(k, v, src)| {
                serde_json::json!({
                    "key": k,
                    "value": v,
                    "source": src.label(),
                })
            })
            .collect();
        let payload = serde_json::json!({ "config": entries });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload)
                .map_err(|e| anyhow::anyhow!("failed to serialize config: {}", e))?
        );
        return Ok(());
    }

    let key_w = rows.iter().map(|(k, _, _)| k.len()).max().unwrap_or(0);
    let val_w = rows.iter().map(|(_, v, _)| v.len()).max().unwrap_or(0);

    println!("Effective configuration (selfware config show)");
    println!();
    for (k, v, src) in &rows {
        println!(
            "  {:<kw$} = {:<vw$}  [{}]",
            k,
            v,
            src.label(),
            kw = key_w,
            vw = val_w
        );
    }

    Ok(())
}

/// Dispatcher for the modern `selfware bench <subcommand>` surface.
///
/// `swebench-pro` is implemented; the other planned subcommands stub with a
/// clear "not yet implemented" error so users see them in `--help` without
/// hitting an `unimplemented!()` panic.
async fn dispatch_bench_subcommand(sub: args::BenchCommand, _config: &Config) -> Result<()> {
    use args::BenchCommand;

    match sub {
        BenchCommand::SwebenchPro(args) => run_swebench_pro_cli(args).await,
        BenchCommand::Throughput => {
            anyhow::bail!(
                "`bench throughput` subcommand is not yet implemented; use `selfware bench --suite throughput` for the legacy path."
            )
        }
        BenchCommand::Multilang => {
            anyhow::bail!(
                "`bench multilang` subcommand is not yet implemented; use `selfware bench --suite multilang` for the legacy path."
            )
        }
        BenchCommand::Browser => {
            anyhow::bail!("`bench browser` subcommand is not yet implemented.")
        }
        BenchCommand::LongRun => {
            anyhow::bail!(
                "`bench long-run` subcommand is not yet implemented; use `selfware long-test` for the legacy path."
            )
        }
    }
}

#[cfg(feature = "bench-harness")]
async fn run_swebench_pro_cli(args: args::SwebenchProArgs) -> Result<()> {
    use crate::bench_harness::swebench_pro::{
        catalog::{quant_catalog, DEFAULT_QUANTS},
        run_swebench_pro, SwebenchProOpts,
    };
    use std::time::Duration;

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

    let llama_opts = crate::bench_harness::swebench_pro::harness::LlamaServerOpts {
        port: args.port,
        ctx: args.ctx,
        parallel: args.parallel,
        ..Default::default()
    };
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
        official_eval_script: PathBuf::from(args.official_eval_script),
        official_eval_raw_sample_path: PathBuf::from(args.official_eval_raw_sample_path),
        official_eval_scripts_dir: PathBuf::from(args.official_eval_scripts_dir),
        official_eval_dockerhub_username: args.official_eval_dockerhub_username,
        official_eval_num_workers: args.official_eval_num_workers,
        official_eval_use_local_docker: !args.official_eval_modal,
        official_eval_redo: args.official_eval_redo,
        official_eval_block_network: args.official_eval_block_network,
        llama_opts,
    };

    // The harness is blocking (subprocess + file I/O) and intentionally serial;
    // run it inside `spawn_blocking` so we don't hold the runtime hostage.
    tokio::task::spawn_blocking(move || run_swebench_pro(opts))
        .await
        .map_err(|e| anyhow::anyhow!("swebench-pro task join error: {}", e))?
}

#[cfg(not(feature = "bench-harness"))]
async fn run_swebench_pro_cli(_args: args::SwebenchProArgs) -> Result<()> {
    anyhow::bail!(
        "`bench swebench-pro` requires the `bench-harness` feature: rebuild with `--features bench-harness`."
    )
}

#[cfg(not(feature = "bench-harness"))]
fn run_swebench_diagnose(_output_dir: &str) -> Result<()> {
    anyhow::bail!(
        "`swebench diagnose` requires the `bench-harness` feature: rebuild with `--features bench-harness`."
    )
}

#[cfg(feature = "bench-harness")]
fn run_swebench_diagnose(output_dir: &str) -> Result<()> {
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
