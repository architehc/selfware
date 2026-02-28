//! Selfware Workshop - Your Personal AI Companion
//!
//! Software you own. Software that knows you. Software that lasts.

#[cfg(feature = "tui")]
use std::sync::mpsc;

use anyhow::Result;
use clap::{Parser, Subcommand};
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

const DEFAULT_MULTI_CHAT_CONCURRENCY: usize = 4;
const JOURNAL_DESC_MAX_CHARS: usize = 50;
const COMMIT_HASH_PREFIX_CHARS: usize = 8;
const MAX_JOURNAL_ERRORS_DISPLAY: usize = 3;
const DEFAULT_WORKFLOW_NAME: &str = "default";

#[derive(Parser)]
#[command(name = "selfware")]
#[command(about = "Your personal AI workshop — software you own, software that lasts")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Headless mode: run prompt directly and exit (like qwen -p)
    #[arg(short = 'p', long, value_name = "PROMPT")]
    prompt: Option<String>,

    /// Config file path
    #[arg(short, long, value_name = "FILE")]
    config: Option<String>,

    /// Working directory (your garden)
    #[arg(short = 'C', long, value_name = "DIR")]
    workdir: Option<String>,

    /// Quiet mode (minimal output)
    #[arg(short, long)]
    quiet: bool,

    /// Execution mode: normal (ask), auto-edit, yolo, daemon
    #[arg(short = 'm', long, value_enum, default_value = "normal")]
    mode: ExecutionMode,

    /// Shortcut for --mode=yolo
    #[arg(short = 'y', long)]
    yolo: bool,

    /// Shortcut for --mode=daemon (run forever)
    #[arg(long)]
    daemon: bool,

    /// Disable colored output
    #[arg(long)]
    no_color: bool,

    /// Launch full TUI dashboard mode (requires --features tui)
    /// This is the default when no subcommand is specified
    #[arg(long)]
    tui: bool,

    /// Use classic CLI mode instead of TUI (overrides default TUI)
    #[arg(long)]
    no_tui: bool,

    /// Color theme: amber (default), ocean, minimal, high-contrast
    #[arg(long, value_enum, default_value = "amber")]
    theme: Theme,

    /// Compact output mode (less visual chrome, more dense)
    #[arg(long)]
    compact: bool,

    /// Verbose mode (detailed tool output and debug info)
    #[arg(short = 'v', long)]
    verbose: bool,

    /// Always display token usage after each response
    #[arg(long)]
    show_tokens: bool,
}

/// Color theme for terminal output
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum Theme {
    /// Warm amber tones (default)
    #[default]
    Amber,
    /// Cool ocean blues and teals
    Ocean,
    /// Clean grayscale minimal
    Minimal,
    /// High contrast for accessibility
    HighContrast,
}

/// Output format for CLI (currently only affects `status` command)
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum OutputFormat {
    /// Human-readable text (default)
    #[default]
    Text,
    /// JSON output for scripting
    Json,
}

/// Demo scenario selection for `selfware demo`
#[cfg(feature = "tui")]
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum DemoScenarioKind {
    Archaeology,
    FeatureFactory,
    BugHunt,
    TokenChallenge,
}

#[derive(Subcommand, Clone)]
enum Commands {
    /// Open your workshop for an interactive session
    #[command(alias = "c")]
    Chat,

    /// Multi-agent chat with concurrent streams
    #[command(alias = "m")]
    MultiChat {
        /// Maximum concurrent agents (1-16)
        #[arg(short = 'n', long, default_value_t = DEFAULT_MULTI_CHAT_CONCURRENCY)]
        concurrency: usize,
    },

    /// Tend to a specific task in your garden
    #[command(alias = "r")]
    Run {
        /// What shall we tend to?
        task: String,
    },

    /// Survey your garden (analyze codebase)
    #[command(alias = "a")]
    Analyze {
        /// Path to survey
        #[arg(default_value = ".")]
        path: String,
    },

    /// View your garden as a living ecosystem
    Garden {
        /// Path to visualize
        #[arg(default_value = ".")]
        path: String,
    },

    /// Run an animated multi-agent demo scenario
    #[cfg(feature = "tui")]
    Demo {
        /// Demo scenario to run
        #[arg(value_enum, default_value_t = DemoScenarioKind::FeatureFactory)]
        scenario: DemoScenarioKind,
        /// Use faster timings for CI/smoke runs
        #[arg(long)]
        fast: bool,
    },

    /// Launch dashboard mode explicitly
    #[cfg(feature = "tui")]
    Dashboard {
        /// Enable swarm-oriented dashboard hints
        #[arg(long)]
        swarm_mode: bool,
    },

    /// Resume tending from a journal entry
    Resume {
        /// Journal entry ID
        task_id: String,
    },

    /// Browse your journal entries
    #[command(alias = "j")]
    Journal,

    /// View a specific journal entry
    JournalEntry {
        /// Entry ID
        task_id: String,
    },

    /// Remove a journal entry
    JournalDelete {
        /// Entry ID
        task_id: String,
    },

    /// Show workshop status and statistics
    Status {
        /// Output format for machine consumption
        #[arg(long, value_enum, default_value = "text")]
        output_format: OutputFormat,
    },

    /// Self-improve: analyze and edit the selfware codebase
    #[cfg(feature = "self-improvement")]
    Improve {
        /// Analyze and propose improvements without making changes
        #[arg(long)]
        dry_run: bool,

        /// Keep improving until no targets or max cycles reached
        #[arg(long)]
        continuous: bool,

        /// Maximum improvement cycles (default 5)
        #[arg(long, default_value_t = 5)]
        max_cycles: usize,
    },

    /// Execute a workflow from a YAML file
    #[command(alias = "w")]
    Workflow {
        /// Path to workflow YAML file
        file: String,

        /// Workflow name to execute (if file contains multiple)
        #[arg(short, long)]
        name: Option<String>,

        /// Input variables (KEY=VALUE format)
        #[arg(short, long)]
        input: Vec<String>,

        /// Dry-run mode (log but don't execute)
        #[arg(long)]
        dry_run: bool,
    },
}

pub async fn run() -> Result<()> {
    // Initialize telemetry
    init_tracing();

    let cli = Cli::parse();

    // Apply --no-color early to disable all color output
    if cli.no_color || std::env::var("NO_COLOR").is_ok() {
        colored::control::set_override(false);
    }

    // Change to working directory FIRST (before resolving relative paths)
    if let Some(ref workdir) = cli.workdir {
        std::env::set_current_dir(workdir)
            .map_err(|e| anyhow::anyhow!("Cannot enter garden '{}': {}", workdir, e))?;

        if !cli.quiet {
            println!(
                "{} Entering garden: {}",
                Glyphs::SPROUT,
                workdir.as_str().path_local()
            );
        }
    }

    // Resolve config path (after -C so relative paths work correctly)
    let config_path: Option<String> = cli.config.map(|p| {
        // Expand ~ to home directory
        let expanded = if let Some(rest) = p.strip_prefix("~/") {
            match dirs::home_dir() {
                Some(home) => home.join(rest).to_string_lossy().to_string(),
                None => {
                    warn!(
                        "Could not resolve home directory for config path '{}'; using raw value",
                        p
                    );
                    p.clone()
                }
            }
        } else {
            p.clone()
        };

        // Resolve relative paths
        if std::path::Path::new(&expanded).is_absolute() {
            expanded
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(&expanded).to_string_lossy().to_string())
                .unwrap_or_else(|err| {
                    warn!(
                        "Could not resolve current directory for config path '{}': {}",
                        expanded, err
                    );
                    expanded
                })
        }
    });

    let mut config = Config::load(config_path.as_deref())?;

    // Resolve execution mode (flags override --mode)
    let exec_mode = if cli.daemon {
        ExecutionMode::Daemon
    } else if cli.yolo {
        ExecutionMode::Yolo
    } else {
        cli.mode
    };

    // Apply execution mode to config
    config.execution_mode = exec_mode;

    if config.execution_mode == ExecutionMode::Daemon {
        let addr = "127.0.0.1:9090".parse().unwrap();
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
            println!("\n{} {}\n", Glyphs::GEAR, "Headless Mode".workshop_title());
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

            // Run TUI in a separate thread
            let tui_handle = std::thread::spawn(move || {
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

            // Cleanup: join the TUI thread without blocking the async runtime
            tokio::task::block_in_place(|| {
                let _ = tui_handle.join();
            });
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
    let command = cli.command.unwrap_or(Commands::Chat);
    handle_command(command, cli.quiet, config, &ctx, exec_mode).await
}

async fn handle_command(
    command: Commands,
    quiet: bool,
    config: Config,
    ctx: &WorkshopContext,
    exec_mode: ExecutionMode,
) -> Result<()> {
    match command {
        Commands::Chat => {
            if !quiet {
                println!("{}", ui::components::render_welcome(ctx));
            }
            let mut agent = Agent::new(config).await?;
            agent.interactive().await?;
        }

        Commands::MultiChat { concurrency } => {
            if !quiet {
                println!("{}", render_header(ctx));
                println!(
                    "\n{} {} with {} concurrent streams\n",
                    Glyphs::GEAR,
                    "Multi-Agent Workshop".workshop_title(),
                    concurrency.to_string().emphasis()
                );
            }

            let agent_config =
                multiagent::MultiAgentConfig::default().with_concurrency(concurrency);
            let mut multi_agent = multiagent::MultiAgentChat::new(&config, agent_config)?;
            multi_agent.interactive().await?;
        }

        Commands::Run { task } => {
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
                    Glyphs::MAGNIFIER,
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
                    Glyphs::TREE,
                    "Visualizing your digital garden".craftsman_voice(),
                    path.as_str().path_local()
                );
            }

            // Build garden visualization
            let garden = ui::garden::build_garden_from_path(&path)?;
            println!("{}", garden.render());
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
                    Glyphs::GEAR,
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
                    Glyphs::BOOKMARK,
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
                        Glyphs::SPROUT,
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
                    Glyphs::JOURNAL,
                    "Note:".muted()
                );
            } else {
                println!(
                    "\n{} {}\n",
                    Glyphs::JOURNAL,
                    "Your Journal Entries:".workshop_title()
                );

                for task in tasks {
                    let status_glyph = match task.status {
                        checkpoint::TaskStatus::InProgress => Glyphs::GEAR,
                        checkpoint::TaskStatus::Completed => Glyphs::BLOOM,
                        checkpoint::TaskStatus::Failed => Glyphs::FROST,
                        checkpoint::TaskStatus::Paused => Glyphs::BOOKMARK,
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
                        Glyphs::BRANCH.muted(),
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
                Glyphs::JOURNAL,
                "Journal Entry".workshop_title()
            );

            let weather = match checkpoint.status {
                checkpoint::TaskStatus::InProgress => format!("{} Working", Glyphs::GEAR),
                checkpoint::TaskStatus::Completed => format!("{} Complete", Glyphs::BLOOM),
                checkpoint::TaskStatus::Failed => format!("{} Frost damage", Glyphs::FROST),
                checkpoint::TaskStatus::Paused => format!("{} Resting", Glyphs::LEAF),
            };

            println!(
                "   {} Entry ID:    {}",
                Glyphs::KEY,
                checkpoint.task_id.muted()
            );
            println!("   {} Weather:     {}", Glyphs::SPROUT, weather);
            println!(
                "   {} Step:        {}",
                Glyphs::BRANCH.muted(),
                checkpoint.current_step
            );
            println!(
                "   {} Started:     {}",
                Glyphs::SEEDLING,
                checkpoint.created_at.timestamp()
            );
            println!(
                "   {} Last tended: {}",
                Glyphs::LEAF,
                checkpoint.updated_at.timestamp()
            );
            println!();
            println!("   {} {}", Glyphs::JOURNAL, "Reflection:".craftsman_voice());
            println!("   {}", checkpoint.task_description.as_str().emphasis());
            println!();

            if let Some(ref git) = checkpoint.git_checkpoint {
                println!("   {} {}", Glyphs::TREE, "Garden State:".craftsman_voice());
                println!("      Branch: {}", git.branch.as_str().path_local());
                println!(
                    "      Commit: {}",
                    take_prefix_chars(&git.commit_hash, COMMIT_HASH_PREFIX_CHARS)
                        .as_str()
                        .muted()
                );
                if git.dirty {
                    println!("      {} Uncommitted changes", Glyphs::WILT);
                }
                println!();
            }

            println!(
                "   {} Growth rings: {} messages, {} tool calls",
                Glyphs::HARVEST,
                checkpoint.messages.len().to_string().garden_healthy(),
                checkpoint.tool_calls.len().to_string().muted()
            );

            if !checkpoint.errors.is_empty() {
                println!(
                    "\n   {} {}",
                    Glyphs::FROST,
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
                    Glyphs::FALLEN_LEAF,
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
                        Glyphs::HOME,
                        "Workshop Status".workshop_title()
                    );

                    let hosting = if ctx.is_local_model {
                        format!("{} Running on your hardware (local)", Glyphs::HOME)
                            .garden_healthy()
                    } else {
                        format!("{} Connected to remote model", Glyphs::COMPASS).garden_wilting()
                    };

                    println!(
                        "   {} Model: {}",
                        Glyphs::GEAR,
                        ctx.model_name.as_str().emphasis()
                    );
                    println!("   {}", hosting);
                    println!(
                        "   {} Garden: {}",
                        Glyphs::SPROUT,
                        ctx.project_path.as_str().path_local()
                    );

                    println!(
                        "\n   {} Journal: {} entries ({} complete, {} in progress)",
                        Glyphs::JOURNAL,
                        tasks.len().to_string().emphasis(),
                        completed.to_string().garden_healthy(),
                        in_progress.to_string().muted()
                    );

                    println!(
                        "\n   {} This is your software. It runs on your terms.\n",
                        Glyphs::KEY
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
                    Glyphs::GEAR,
                    "Self-Improvement Analysis".workshop_title()
                );
            }

            let project_root = std::env::current_dir()?;
            let orchestrator = SelfEditOrchestrator::new(project_root);
            let targets = orchestrator.analyze_self();

            if targets.is_empty() {
                println!(
                    "   {} No improvement targets found. The codebase looks good!",
                    Glyphs::BLOOM
                );
                return Ok(());
            }

            println!(
                "   {} Found {} improvement targets:\n",
                Glyphs::MAGNIFIER,
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
                println!("\n   {} Dry-run mode: no changes applied.", Glyphs::LEAF);
                return Ok(());
            }

            let cycles = if continuous { max_cycles } else { 1 };
            let mut agent = Agent::new(config).await?;

            for cycle in 0..cycles {
                let targets = orchestrator.analyze_self();
                let Some(target) = orchestrator.select_target(&targets) else {
                    println!("\n   {} No more improvement targets. Done!", Glyphs::BLOOM);
                    break;
                };

                println!(
                    "\n   {} Cycle {}/{}: applying '{}'",
                    Glyphs::GEAR,
                    cycle + 1,
                    cycles,
                    target.description
                );

                let prompt = orchestrator.build_improvement_prompt(target);
                match agent.run_task(&prompt).await {
                    Ok(()) => {
                        println!("   {} Improvement applied successfully.", Glyphs::BLOOM);
                    }
                    Err(e) => {
                        println!("   {} Improvement failed: {}", Glyphs::FROST, e);
                    }
                }
            }
        }

        Commands::Workflow {
            file,
            name,
            input,
            dry_run,
        } => {
            if !quiet {
                println!("{}", render_header(ctx));
            }

            // Load workflow file
            let path = std::path::Path::new(&file);
            if !path.exists() {
                anyhow::bail!("Workflow file not found: {}", file);
            }

            let mut executor = if dry_run {
                println!(
                    "\n{} {} (dry-run mode)\n",
                    Glyphs::GEAR,
                    "Workflow Execution".workshop_title()
                );
                WorkflowExecutor::new_dry_run()
            } else {
                println!(
                    "\n{} {}\n",
                    Glyphs::GEAR,
                    "Workflow Execution".workshop_title()
                );
                WorkflowExecutor::new()
            };

            executor.load_file(path)?;

            // Determine which workflow to run
            let workflow_name = name.unwrap_or_else(|| default_workflow_name(path));

            // Parse input variables
            let mut inputs = std::collections::HashMap::new();
            for kv in input {
                if let Some((k, v)) = kv.split_once('=') {
                    inputs.insert(k.to_string(), VarValue::String(v.to_string()));
                } else {
                    anyhow::bail!("Invalid input format '{}', expected KEY=VALUE", kv);
                }
            }

            println!(
                "   {} Running workflow: {}",
                Glyphs::COMPASS,
                workflow_name.clone().emphasis()
            );
            if !inputs.is_empty() {
                println!("   {} Inputs: {:?}", Glyphs::JOURNAL, inputs);
            }
            println!();

            // Execute workflow
            let working_dir = std::env::current_dir()?;
            let result = executor
                .execute(&workflow_name, inputs, working_dir)
                .await?;

            // Report result
            match result.status {
                crate::workflows::WorkflowStatus::Completed => {
                    println!(
                        "\n   {} Workflow completed successfully in {}ms",
                        Glyphs::FLOWER,
                        result.duration_ms
                    );
                }
                crate::workflows::WorkflowStatus::Failed => {
                    println!(
                        "\n   {} Workflow failed after {}ms",
                        Glyphs::FALLEN_LEAF,
                        result.duration_ms
                    );
                }
                _ => {
                    println!(
                        "\n   {} Workflow ended with status: {:?}",
                        Glyphs::LEAF,
                        result.status
                    );
                }
            }

            // Show step results
            println!("\n   {} Steps executed:", Glyphs::JOURNAL);
            for (id, step_result) in &result.step_results {
                let status_icon = match step_result.status {
                    crate::workflows::StepStatus::Completed => Glyphs::FLOWER,
                    crate::workflows::StepStatus::Failed => Glyphs::FALLEN_LEAF,
                    crate::workflows::StepStatus::Skipped => Glyphs::LEAF,
                    _ => Glyphs::GEAR,
                };
                println!("      {} {} ({:?})", status_icon, id, step_result.status);
            }
            println!();
        }
    }

    Ok(())
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
            Glyphs::GEAR,
            scenario_impl.name().emphasis()
        );
    }

    runner.start(scenario_impl.as_mut());
    while runner.next_stage(scenario_impl.as_mut()) {
        runner.update(0.16);
        if !quiet {
            println!(
                "   {} Stage {}/{}",
                Glyphs::BRANCH,
                runner.current_stage(),
                runner.total_stages()
            );
        }
        std::thread::sleep(step_delay);
    }

    if !quiet {
        println!(
            "\n{} Demo complete in {:.2}s\n",
            Glyphs::BLOOM,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // ── truncate_with_ellipsis tests ──

    #[test]
    fn truncate_with_ellipsis_short_string_unchanged() {
        assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
        assert_eq!(truncate_with_ellipsis("hello", 5), "hello");
    }

    #[test]
    fn truncate_with_ellipsis_adds_dots_when_over_limit() {
        // max_chars=8 means keep 5 chars + "..."
        assert_eq!(truncate_with_ellipsis("hello world", 8), "hello...");
    }

    #[test]
    fn truncate_with_ellipsis_empty_string() {
        assert_eq!(truncate_with_ellipsis("", 10), "");
        assert_eq!(truncate_with_ellipsis("", 0), "");
    }

    #[test]
    fn truncate_with_ellipsis_unicode_chars() {
        // Each emoji is 1 char but multiple bytes. "ab" = 2 chars, max=3 means no truncation needed for "ab"
        assert_eq!(truncate_with_ellipsis("ab", 3), "ab");
        // 5 chars total, max=4 => keep 1 + "..."
        let result = truncate_with_ellipsis("abcde", 4);
        assert_eq!(result, "a...");
    }

    #[test]
    fn truncate_with_ellipsis_max_less_than_three() {
        // max_chars=2, keep_chars = 2.saturating_sub(3) = 0, so just "..."
        assert_eq!(truncate_with_ellipsis("hello", 2), "...");
        assert_eq!(truncate_with_ellipsis("hello", 0), "...");
    }

    // ── take_prefix_chars tests ──

    #[test]
    fn take_prefix_chars_basic() {
        assert_eq!(take_prefix_chars("abcdef", 3), "abc");
        assert_eq!(take_prefix_chars("abcdef", 0), "");
        assert_eq!(take_prefix_chars("abcdef", 100), "abcdef");
    }

    #[test]
    fn take_prefix_chars_empty_string() {
        assert_eq!(take_prefix_chars("", 5), "");
    }

    // ── default_workflow_name tests ──

    #[test]
    fn default_workflow_name_extracts_stem() {
        assert_eq!(
            default_workflow_name(Path::new("my_workflow.yaml")),
            "my_workflow"
        );
        assert_eq!(
            default_workflow_name(Path::new("/path/to/deploy.yml")),
            "deploy"
        );
    }

    #[test]
    fn default_workflow_name_no_extension() {
        assert_eq!(
            default_workflow_name(Path::new("Makefile")),
            "Makefile"
        );
    }

    #[test]
    fn default_workflow_name_falls_back_for_empty_path() {
        // Path with no file stem returns the default
        assert_eq!(
            default_workflow_name(Path::new("/")),
            DEFAULT_WORKFLOW_NAME
        );
    }

    // ── Theme / OutputFormat enum tests ──

    #[test]
    fn theme_default_is_amber() {
        let theme: Theme = Default::default();
        assert!(matches!(theme, Theme::Amber));
    }

    #[test]
    fn output_format_default_is_text() {
        let fmt: OutputFormat = Default::default();
        assert!(matches!(fmt, OutputFormat::Text));
    }

    // ── Constants sanity checks ──

    #[test]
    fn constants_have_reasonable_values() {
        assert!(DEFAULT_MULTI_CHAT_CONCURRENCY >= 1);
        assert!(DEFAULT_MULTI_CHAT_CONCURRENCY <= 64);
        assert!(JOURNAL_DESC_MAX_CHARS > 0);
        assert!(COMMIT_HASH_PREFIX_CHARS > 0);
        assert!(MAX_JOURNAL_ERRORS_DISPLAY > 0);
        assert!(!DEFAULT_WORKFLOW_NAME.is_empty());
    }
}
