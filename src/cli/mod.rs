//! Selfware Workshop - Your Personal AI Companion
//!
//! Software you own. Software that knows you. Software that lasts.

pub(crate) mod args;
pub(crate) mod init_wizard;

#[cfg(feature = "tui")]
use std::sync::mpsc;

use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use tracing::warn;

// Use library exports instead of redeclaring modules
// This avoids duplicate compilation and maintains consistency
use crate::agent::Agent;
use crate::checkpoint;
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
use crate::workflows::{VarValue, WorkflowExecutor};

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

/// Resolve the config file path relative to the original working directory.
///
/// When `-C <dir>` is used, the process changes its cwd *after* this function
/// runs.  That way an explicit `--config my.toml` or the implicit
/// `selfware.toml` default are always resolved against the directory the user
/// was in when they invoked the command.
///
/// Rules:
/// 1. Explicit `--config` path → expand `~`, then absolutify against `original_cwd`.
/// 2. No `--config` but `-C` is active → check `original_cwd/selfware.toml`.
///    If it exists, return its absolute path.  Otherwise return `None` so that
///    `Config::load` does its normal search (in the new cwd + home dir).
/// 3. Neither flag → return `None` (normal search).
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
        Some(if std::path::Path::new(&expanded).is_absolute() {
            expanded
        } else if let Some(cwd) = original_cwd {
            cwd.join(&expanded).to_string_lossy().to_string()
        } else {
            warn!(
                "Could not resolve current directory for config path '{}'; using raw value",
                expanded
            );
            expanded
        })
    } else if has_workdir {
        // No explicit --config but -C is being used: check for selfware.toml
        // in the ORIGINAL directory first.  If found, pass its absolute path so
        // Config::load doesn't accidentally pick up a different file in the -C
        // target directory.
        if let Some(cwd) = original_cwd {
            let candidate = cwd.join("selfware.toml");
            if candidate.is_file() {
                Some(candidate.to_string_lossy().to_string())
            } else {
                // Not in original dir — let Config::load do its normal search
                // (which will look in the -C directory and ~/.config/selfware/).
                None
            }
        } else {
            None
        }
    } else {
        None
    }
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

    // Apply execution mode to config
    config.execution_mode = exec_mode;

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

    // Apply plan mode from CLI
    if cli.plan {
        config.plan_mode = true;
    }

    // Initialize output control with merged settings
    output::init(compact, verbose, show_tokens);

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

        if !cli.quiet {
            println!("{}", render_header(&ctx));
            println!(
                "\n{} {}\n",
                Glyphs::gear(),
                "Headless Mode".workshop_title()
            );
        }

        let start = std::time::Instant::now();
        let mut agent = Agent::new(config).await?;
        agent.run_task(&actual_prompt).await?;

        if !cli.quiet {
            println!("{}", render_task_complete(start.elapsed()));
        }
        return Ok(());
    }

    // Handle TUI dashboard mode
    #[cfg(feature = "tui")]
    {
        let should_use_tui = cli.tui || (cli.command.is_none() && !cli.no_tui);
        if should_use_tui {
            let (event_tx, event_rx) = mpsc::channel();
            let (user_input_tx, user_input_rx) = mpsc::channel();

            let mut agent = Agent::new(config.clone())
                .await?
                .with_event_sender(event_tx);

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
                )
            });

            // Process user inputs from TUI.
            // The recv() is blocking (std::sync::mpsc), so we use block_in_place
            // to let tokio move other tasks off this thread while we wait.
            loop {
                let input = tokio::task::block_in_place(|| user_input_rx.recv());

                match input {
                    Ok(input) if input != "exit" && input != "quit" => {
                        // Run the task — this will emit events to the TUI through event_tx
                        if let Err(e) = agent.run_task(&input).await {
                            warn!("Agent failed to run task: {}", e);
                        }
                    }
                    _ => break,
                }
            }

            crate::output::set_tui_active(false);

            // Cleanup: await the TUI task without blocking the async runtime
            let _ = tui_handle.await;
            return Ok(());
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
        config,
        &ctx,
        exec_mode,
        cli.resume_session,
    )
    .await
}

async fn handle_command(
    command: Commands,
    quiet: bool,
    mut config: Config,
    ctx: &WorkshopContext,
    exec_mode: ExecutionMode,
    resume_session: Option<String>,
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
            if !quiet {
                println!("{}", render_header(ctx));
                println!(
                    "\n{} {} with {} concurrent streams\n",
                    Glyphs::gear(),
                    "Multi-Agent Workshop".workshop_title(),
                    concurrency.to_string().emphasis()
                );
            }

            let agent_config =
                multiagent::MultiAgentConfig::default().with_concurrency(concurrency);
            let mut multi_agent = multiagent::MultiAgentChat::new(&config, agent_config)?;
            multi_agent.interactive().await?;
        }

        Commands::Run { task, yolo } => {
            if yolo {
                config.execution_mode = ExecutionMode::Yolo;
            }
            if !quiet {
                println!("{}", render_header(ctx));
                println!("{}", render_task_start(&task));
            }

            let start = std::time::Instant::now();
            let mut agent = Agent::new(config).await?;
            agent.run_task(&task).await?;

            if !quiet {
                println!("{}", render_task_complete(start.elapsed()));
            }
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
        Commands::Dashboard { swarm_mode } => {
            if swarm_mode && !quiet {
                println!(
                    "{} {}",
                    Glyphs::gear(),
                    "Swarm mode enabled for dashboard session".craftsman_voice()
                );
            }
            let _user_inputs = crate::ui::tui::run_tui_dashboard(&config.model)?;
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
        } => {
            use crate::evolution::daemon;
            use crate::evolution::{
                EvolutionConfig, FitnessWeights, LlmConfig, MutationTargets, SafetyConfig,
            };

            if !quiet {
                println!("{}", render_header(ctx));
                println!(
                    "\n{} {}\n",
                    Glyphs::gear(),
                    "Evolution Daemon".workshop_title()
                );
            }

            let repo_root = std::env::current_dir()?;
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

            let result = daemon::evolve(evo_config, &repo_root);

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
            let file_for_print = file.clone();
            println!("\n{} Batch Execution Mode\n", "⚡".emphasis());
            println!("   Tasks file: {}", file_for_print.emphasis());
            println!("   Workers: {}", workers.to_string().emphasis());
            println!("   Timeout: {}s per task", timeout);
            println!("   Output: {}", output);
            println!();

            use crate::batch::{parse_tasks_file, BatchConfig, BatchExecutor};

            let tasks = parse_tasks_file(&file.into())?;
            println!("   Loaded {} tasks\n", tasks.len());

            let batch_config = BatchConfig {
                max_workers: workers,
                timeout_secs: timeout,
                aggregate,
                output_dir: output.into(),
                continue_on_error: true,
            };

            let executor = BatchExecutor::new(batch_config);
            let results = executor.execute_tasks(tasks).await?;

            println!("\n✓ Batch Complete\n");
            println!(
                "   Successful: {}/{}",
                results.iter().filter(|r| r.success).count(),
                results.len()
            );
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
            println!("\n{} Visual Validation\n", "🎨".emphasis());
            println!("   URL: {}", url.clone().emphasis());
            println!("   Iterations: {}", iterations);
            println!("   Target score: {:.1}/10\n", target);

            use crate::validation::{Device, ScreenshotConfig, ValidationWorkflow};

            let _workflow = ValidationWorkflow {
                url: url.clone(),
                local_dir: dir.map(|d| d.into()),
                max_iterations: iterations,
                target_score: target,
                screenshot_config: ScreenshotConfig {
                    url: url.clone(),
                    output_dir: "./validation_screenshots".into(),
                    devices: vec![Device::desktop(), Device::mobile()],
                    wait_ms: 2000,
                    full_page: true,
                },
            };

            println!("   Running validation workflow...\n");
            // Note: Actual async execution would require tokio runtime setup
            println!("   ✓ Validation workflow configured");
            println!("   (Full execution requires Playwright installation)");
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
                            let source = std::fs::read_to_string(path)?;
                            let _doc = match parse_document(&source) {
                                Ok(d) => d,
                                Err(e) => {
                                    anyhow::bail!("Failed to parse SWL file: {}", e);
                                }
                            };

                            if dry_run {
                                println!(
                                    "\n{} {} (dry-run mode)\n",
                                    Glyphs::gear(),
                                    "SWL Workflow".workshop_title()
                                );
                            } else {
                                println!(
                                    "\n{} {}\n",
                                    Glyphs::gear(),
                                    "SWL Workflow".workshop_title()
                                );
                            }
                            println!(
                                "   {} File: {}",
                                Glyphs::journal(),
                                file.as_str().path_local()
                            );

                            if !inputs.is_empty() {
                                println!("   {} Inputs: {:?}", Glyphs::journal(), inputs);
                            }

                            println!();
                            println!("   {} SWL file parsed successfully.", Glyphs::bloom());
                            println!(
                                "   (Full SWL execution not yet implemented - use YAML workflows for live execution)"
                            );
                            println!();
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
                                executor =
                                    executor.with_llm_handler(Box::new(move |prompt, ctx| {
                                        let client = std::sync::Arc::clone(&client);
                                        let request = if ctx.is_empty() {
                                            prompt.to_string()
                                        } else {
                                            format!("{prompt}\n\nContext:\n{}", ctx.join("\n"))
                                        };

                                        tokio::task::block_in_place(|| {
                                            tokio::runtime::Handle::current().block_on(async move {
                                                let response = client
                                                    .chat(
                                                        vec![crate::api::Message::user(request)],
                                                        None,
                                                        crate::api::ThinkingMode::Disabled,
                                                    )
                                                    .await?;

                                                let choice = response
                                                    .choices
                                                    .into_iter()
                                                    .next()
                                                    .ok_or_else(|| {
                                                        anyhow::anyhow!("model returned no choices")
                                                    })?;
                                                let text = choice.message.content.text_all();

                                                if !text.trim().is_empty() {
                                                    Ok(text)
                                                } else if let Some(reasoning) = choice
                                                    .reasoning_content
                                                    .or(choice.message.reasoning_content)
                                                {
                                                    Ok(reasoning)
                                                } else {
                                                    Err(anyhow::anyhow!(
                                                        "model returned empty content"
                                                    ))
                                                }
                                            })
                                        })
                                    }));
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
                                    name.emphasis(),
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

        Commands::McpServer => {
            crate::mcp::server::run_mcp_server().await?;
        }

        Commands::Lsp => {
            crate::lsp::run_lsp_server().await?;
        }

        Commands::Test { pattern, format } => {
            if !quiet {
                println!("{}", render_header(ctx));
            }
            let pattern_clone = pattern.clone();
            println!("\n{} Running Tests\n", "🧪".emphasis());
            println!("   Pattern: {}", pattern.emphasis());
            println!("   Format: {}\n", format);

            use crate::swebench::LocalDevWorkflow;

            let workflow = LocalDevWorkflow {
                test_patterns: vec![pattern_clone],
                endpoints: vec![config.endpoint.clone()],
            };

            let report = workflow.test_workflow().await?;

            println!(
                "\n{} Test Results\n",
                if report.all_passed { "✓" } else { "✗" }
            );
            for (name, passed) in &report.results {
                let icon = if *passed { "✓" } else { "✗" };
                println!("   {} {}", icon, name);
            }
        }

        Commands::SWEBench {
            dataset,
            limit,
            output,
        } => {
            if !quiet {
                println!("{}", render_header(ctx));
            }
            let dataset_clone = dataset.clone();
            println!("\n{} SWE-bench Pro Evaluation\n", "📊".emphasis());
            println!("   Dataset: {}", dataset.emphasis());
            if let Some(l) = limit {
                println!("   Limit: {} tasks", l);
            }
            println!("   Output: {}\n", output);

            use crate::swebench::{SWEBenchEvaluator, SWEBenchTask};

            let evaluator = SWEBenchEvaluator::new(std::path::PathBuf::from("./swebench_work"));
            let tasks = evaluator.load_tasks(&dataset_clone)?;

            let tasks_to_run: Vec<SWEBenchTask> = match limit {
                Some(n) => tasks.into_iter().take(n).collect(),
                None => tasks,
            };

            println!("   Loaded {} tasks\n", tasks_to_run.len());
            println!("   (Full evaluation would run selfware on each task)");
            println!("   Results would be saved to: {}\n", output);
        }

        Commands::Bench {
            endpoint,
            suite,
            concurrent,
            format: _format,
        } => {
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

        Commands::Doctor => {
            if !quiet {
                println!("{}", render_header(ctx));
            }
            let report = crate::doctor::run_doctor().await;
            report.print();
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

        Commands::Init { template } => {
            tokio::task::spawn_blocking(move || init_wizard::run_init_wizard(template))
                .await
                .map_err(|e| anyhow::anyhow!("Task panicked: {}", e))??;
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
