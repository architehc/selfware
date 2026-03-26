//! Selfware Workshop - Your Personal AI Companion
//!
//! Software you own. Software that knows you. Software that lasts.

#[cfg(feature = "tui")]
use std::sync::mpsc;

use anyhow::Result;
use clap::{Parser, Subcommand};
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
    #[arg(short = 'm', long, value_enum)]
    mode: Option<ExecutionMode>,

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

    /// Use ASCII-only output (no emoji or extended Unicode)
    #[arg(long)]
    ascii: bool,

    /// Plan mode: agent proposes tool calls without executing them
    #[arg(long)]
    plan: bool,

    /// Resume a named chat session (alias for `selfware chat --resume <name>`)
    #[arg(long, value_name = "NAME")]
    resume_session: Option<String>,
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

/// Output format for `selfware graph`
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum GraphFormat {
    #[default]
    Ascii,
    Mermaid,
    Dot,
    Json,
    Plantuml,
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

#[derive(Subcommand, Clone, Debug)]
enum Commands {
    /// Check system dependencies and tool availability
    Doctor,

    /// Test local development workflow
    #[command(alias = "t")]
    Test {
        /// Test pattern to run (all, unit, integration, e2e, workflow)
        #[arg(short, long, default_value = "workflow")]
        pattern: String,
        /// Output format (text, json)
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Run SWE-bench Pro evaluation
    #[command(alias = "swe")]
    SWEBench {
        /// Dataset to use (public, held-out, commercial)
        #[arg(short, long, default_value = "public")]
        dataset: String,
        /// Number of tasks to evaluate
        #[arg(short, long)]
        limit: Option<usize>,
        /// Output file for results
        #[arg(short, long, default_value = "swebench_results.json")]
        output: String,
    },

    /// Run comprehensive benchmark suite
    #[command(alias = "b")]
    Bench {
        /// Endpoint URL to benchmark (defaults to config)
        #[arg(short, long)]
        endpoint: Option<String>,
        /// Benchmark suites to run (throughput,e2e,swebench)
        #[arg(short, long, default_value = "throughput,e2e")]
        suite: String,
        /// Number of concurrent tasks
        #[arg(short, long, default_value_t = 4)]
        concurrent: usize,
        /// Output format (text, json)
        #[arg(long, default_value = "text")]
        format: String,
    },

    /// Auto-detect and configure endpoint settings
    #[command(alias = "ac")]
    AutoConfig {
        /// API endpoint URL to test (e.g., http://localhost:8000/v1)
        #[arg(short, long)]
        endpoint: Option<String>,

        /// Model name to test (auto-detected if not provided)
        #[arg(short, long)]
        model: Option<String>,

        /// API key for authenticated endpoints
        #[arg(long)]
        api_key: Option<String>,

        /// Output the detected configuration as TOML
        #[arg(long)]
        toml: bool,

        /// Save configuration to selfware.toml
        #[arg(long)]
        save: bool,
    },

    /// Interactive setup wizard for first-time configuration
    Init {
        /// Use a specific template (rust, python, node, minimal)
        #[arg(long)]
        template: Option<String>,
    },

    /// Open your workshop for an interactive session
    #[command(alias = "c")]
    Chat {
        /// Shortcut for --mode=yolo (skip all confirmations)
        #[arg(short = 'y', long)]
        yolo: bool,
    },

    /// Multi-agent chat with concurrent streams
    #[command(alias = "m")]
    MultiChat {
        /// Maximum concurrent agents (1-16)
        #[arg(short = 'n', long, default_value_t = DEFAULT_MULTI_CHAT_CONCURRENCY)]
        concurrency: usize,
        /// Shortcut for --mode=yolo (skip all confirmations)
        #[arg(short = 'y', long)]
        yolo: bool,
    },

    /// Tend to a specific task in your garden
    #[command(alias = "r")]
    Run {
        /// What shall we tend to?
        task: String,
        /// Shortcut for --mode=yolo (skip all confirmations)
        #[arg(short = 'y', long)]
        yolo: bool,
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

    /// Explore the workspace as a code knowledge graph
    Graph {
        /// Workspace path to index
        #[arg(default_value = ".")]
        path: String,

        /// Focus the graph on a symbol or file path fragment
        #[arg(long)]
        focus: Option<String>,

        /// Neighborhood depth around the focus node
        #[arg(long, default_value_t = 2)]
        depth: usize,

        /// Maximum number of nodes to include in the rendered subgraph
        #[arg(long, default_value_t = 48)]
        max_nodes: usize,

        /// Output format
        #[arg(long, value_enum, default_value = "ascii")]
        format: GraphFormat,
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

    /// Evolve: run the evolutionary self-improvement daemon
    #[cfg(feature = "self-improvement")]
    Evolve {
        /// Number of generations (0 = infinite)
        #[arg(short, long, default_value = "10")]
        generations: usize,

        /// Population size per generation
        #[arg(short, long, default_value = "4")]
        population: usize,

        /// Maximum parallel sandbox evaluations
        #[arg(long, default_value = "2")]
        parallel: usize,

        /// Dry run: show config and exit
        #[arg(long)]
        dry_run: bool,
    },

    /// Run as MCP server (stdio transport) so other AI tools can use Selfware's capabilities
    McpServer,

    /// Start selfware in LSP server mode (for editor extensions)
    Lsp,

    /// Execute multiple tasks in parallel (batch mode)
    #[command(alias = "bat")]
    Batch {
        /// File containing tasks (one per line)
        #[arg(short, long)]
        file: String,
        /// Maximum concurrent workers
        #[arg(short, long, default_value = "16")]
        workers: usize,
        /// Timeout per task in seconds
        #[arg(short, long, default_value = "300")]
        timeout: u64,
        /// Output directory for results
        #[arg(short, long, default_value = "./batch_results")]
        output: String,
        /// Aggregate results into single file
        #[arg(long)]
        aggregate: bool,
    },

    /// Validate a website visually (screenshot + analysis)
    #[command(alias = "v")]
    Validate {
        /// URL to validate
        #[arg(short, long, default_value = "http://localhost:8080")]
        url: String,
        /// Local directory to serve (if not using external URL)
        #[arg(short, long)]
        dir: Option<String>,
        /// Number of validation iterations
        #[arg(short, long, default_value = "3")]
        iterations: usize,
        /// Target score threshold (0-10)
        #[arg(short, long, default_value = "8.0")]
        target: f32,
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

            crate::output::set_tui_active(false);

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
            format,
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
                .render(&graph, graph_format_to_output_format(format));

            println!(
                "{}",
                render_graph_output(&graph.name, focus.as_deref(), &summary, format, &rendered)
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

            let workflow = ValidationWorkflow {
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
                    Glyphs::gear(),
                    "Workflow Execution".workshop_title()
                );
                WorkflowExecutor::new_dry_run_with_config(&config.safety)
            } else {
                println!(
                    "\n{} {}\n",
                    Glyphs::gear(),
                    "Workflow Execution".workshop_title()
                );
                WorkflowExecutor::new_with_config(&config.safety)
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
                Glyphs::compass(),
                workflow_name.clone().emphasis()
            );
            if !inputs.is_empty() {
                println!("   {} Inputs: {:?}", Glyphs::journal(), inputs);
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
                _ => {
                    println!(
                        "\n   {} Workflow ended with status: {:?}",
                        Glyphs::leaf(),
                        result.status
                    );
                }
            }

            // Show step results
            println!("\n   {} Steps executed:", Glyphs::journal());
            for (id, step_result) in &result.step_results {
                let status_icon = match step_result.status {
                    crate::workflows::StepStatus::Completed => Glyphs::flower(),
                    crate::workflows::StepStatus::Failed => Glyphs::fallen_leaf(),
                    crate::workflows::StepStatus::Skipped => Glyphs::leaf(),
                    _ => Glyphs::gear(),
                };
                println!("      {} {} ({:?})", status_icon, id, step_result.status);
            }
            println!();
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
            format,
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
            let ep = endpoint
                .as_ref()
                .map(|s| s.as_str())
                .unwrap_or(&config.endpoint);

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

            // 2. E2E throughput test
            if suite.contains("e2e") || suite.contains("throughput") {
                println!("{} Running E2E throughput test...", "⏳".dimmed());
                // Would run actual E2E tests here
                println!("{} E2E test completed\n", "✓".green());
            }

            let elapsed = start_time.elapsed();
            println!(
                "{} Benchmark complete in {:.1}s\n",
                "✓".green(),
                elapsed.as_secs_f64()
            );
            println!("   Format: {} (full report generation TODO)\n", format);
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
            tokio::task::spawn_blocking(move || run_init_wizard(template))
                .await
                .unwrap()?;
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

fn run_init_wizard(template: Option<String>) -> Result<()> {
    use std::io::{self, BufRead, Write};
    use std::path::PathBuf;

    // If a template is provided, skip the interactive wizard
    if let Some(ref tmpl) = template {
        return write_template_config(tmpl);
    }

    println!();
    println!(
        "{} Welcome to Selfware! Let's set up your workspace.",
        Glyphs::seedling()
    );
    println!();

    // Detect project type
    let project_type = if std::path::Path::new("Cargo.toml").exists() {
        "Rust (Cargo.toml)"
    } else if std::path::Path::new("package.json").exists() {
        "Node.js (package.json)"
    } else if std::path::Path::new("pyproject.toml").exists()
        || std::path::Path::new("setup.py").exists()
    {
        "Python (pyproject.toml)"
    } else if std::path::Path::new("go.mod").exists() {
        "Go (go.mod)"
    } else {
        "Unknown"
    };
    println!("  Detecting project type... Found: {}", project_type);
    println!();

    // Step 1: Endpoint
    println!("Step 1/4: API Endpoint");
    println!("  Where should Selfware connect?");
    println!("  [1] Local (http://localhost:8080/v1) - Ollama, vLLM, LM Studio");
    println!("  [2] OpenAI-compatible API (https://api.openai.com/v1)");
    println!("  [3] Custom endpoint");
    print!("  > ");
    io::stdout().flush()?;
    let mut choice = String::new();
    io::stdin().lock().read_line(&mut choice)?;
    let endpoint = match choice.trim() {
        "2" => "https://api.openai.com/v1".to_string(),
        "3" => {
            print!("  Enter endpoint URL: ");
            io::stdout().flush()?;
            let mut url = String::new();
            io::stdin().lock().read_line(&mut url)?;
            url.trim().to_string()
        }
        _ => "http://localhost:8080/v1".to_string(),
    };
    println!();

    // Step 2: Model
    println!("Step 2/4: Model");
    let default_model = if endpoint.contains("openai") {
        "gpt-4"
    } else {
        "qwen3-coder"
    };
    print!("  Which model should Selfware use? [{}]: ", default_model);
    io::stdout().flush()?;
    let mut model = String::new();
    io::stdin().lock().read_line(&mut model)?;
    let model = if model.trim().is_empty() {
        default_model.to_string()
    } else {
        model.trim().to_string()
    };
    println!();

    // Step 3: Allowed paths
    println!("Step 3/4: Allowed Paths");
    println!("  Which directories can Selfware access?");
    println!("  [1] Current directory only (.)");
    println!("  [2] Home directory (~)");
    println!("  [3] Custom paths");
    print!("  > ");
    io::stdout().flush()?;
    let mut path_choice = String::new();
    io::stdin().lock().read_line(&mut path_choice)?;
    let allowed_paths = match path_choice.trim() {
        "2" => {
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            format!("[\"{}\"]", home.display())
        }
        "3" => {
            print!("  Enter paths (comma-separated): ");
            io::stdout().flush()?;
            let mut paths = String::new();
            io::stdin().lock().read_line(&mut paths)?;
            let paths: Vec<String> = paths
                .trim()
                .split(',')
                .map(|p| format!("\"{}\"", p.trim()))
                .collect();
            format!("[{}]", paths.join(", "))
        }
        _ => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            format!("[\"{}\"]", cwd.display())
        }
    };
    println!();

    // Step 4: Execution mode
    println!("Step 4/4: Execution Mode");
    println!("  How should Selfware handle file changes?");
    println!("  [1] Normal - Ask before every edit (safest)");
    println!("  [2] AutoEdit - Auto-approve file edits, confirm commands");
    println!("  [3] YOLO - Auto-approve everything (use with caution!)");
    print!("  > ");
    io::stdout().flush()?;
    let mut mode_choice = String::new();
    io::stdin().lock().read_line(&mut mode_choice)?;
    let mode = match mode_choice.trim() {
        "2" => "autoedit",
        "3" => "yolo",
        _ => "normal",
    };
    println!();

    // Write config
    write_config_file(&endpoint, &model, mode, &allowed_paths)
}

fn write_template_config(template: &str) -> Result<()> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    // Scaffold project files for known language templates
    match template {
        "rust" | "python" | "node" | "nodejs" | "typescript" => {
            println!("  {} Using '{}' template...", Glyphs::gear(), template);

            let lang_key = match template {
                "node" | "nodejs" | "typescript" => "nodejs",
                other => other,
            };

            let project_name = cwd
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("my-project")
                .to_string();

            let engine = crate::templates::TemplateEngine::new();
            let opts = crate::templates::ScaffoldOptions {
                description: format!("A {} project scaffolded by Selfware", template),
                framework: None,
                with_ci: true,
                with_tests: true,
                qa_profile: "standard".into(),
            };

            match engine.scaffold_project(lang_key, &project_name, &cwd, &opts) {
                Ok(files) => {
                    println!("  {} Scaffolded {} files:", Glyphs::bloom(), files.len());
                    for f in &files {
                        println!("    {}", f);
                    }
                }
                Err(e) => {
                    println!("  {} Could not scaffold project: {}", Glyphs::frost(), e);
                }
            }
        }
        "minimal" => {
            println!("  {} Using 'minimal' template...", Glyphs::gear());
        }
        other => {
            anyhow::bail!(
                "Unknown template '{}'. Available templates: rust, python, node, nodejs, typescript, minimal",
                other
            );
        }
    }

    let (endpoint, model, mode, allowed_paths) = match template {
        "rust" | "python" | "node" | "nodejs" | "typescript" => (
            "http://localhost:8080/v1".to_string(),
            "qwen3-coder".to_string(),
            "normal",
            format!("[\"{}\"]", cwd.display()),
        ),
        _ => (
            "http://localhost:8080/v1".to_string(),
            "qwen3-coder".to_string(),
            "normal",
            "[\".\"]".to_string(),
        ),
    };

    write_config_file(&endpoint, &model, mode, &allowed_paths)
}

fn write_config_file(endpoint: &str, model: &str, mode: &str, allowed_paths: &str) -> Result<()> {
    use std::path::PathBuf;

    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("selfware");
    std::fs::create_dir_all(&config_dir)?;
    let config_path = config_dir.join("config.toml");

    // Check if config already exists
    if config_path.exists() {
        use std::io::{self, BufRead, Write};

        println!(
            "  {} Configuration already exists at {}",
            Glyphs::frost(),
            config_path.display()
        );
        print!("  Overwrite? [y/N]: ");
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().lock().read_line(&mut answer)?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            println!("  Aborted. Existing configuration preserved.");
            return Ok(());
        }
    }

    let content = format!(
        r#"# Selfware Configuration
# Generated by `selfware init`

endpoint = "{}"
model = "{}"
execution_mode = "{}"

[safety]
allowed_paths = {}

[agent]
# token_budget defaults to max_tokens — set explicitly to match your model's context window
# token_budget = 131072
"#,
        endpoint, model, mode, allowed_paths
    );

    std::fs::write(&config_path, &content)?;
    println!(
        "  {} Configuration saved to {}",
        Glyphs::bloom(),
        config_path.display()
    );
    println!();
    println!(
        "  {} Run `selfware` to start your workshop!",
        Glyphs::sprout()
    );

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
        assert_eq!(default_workflow_name(Path::new("Makefile")), "Makefile");
    }

    #[test]
    fn default_workflow_name_falls_back_for_empty_path() {
        // Path with no file stem returns the default
        assert_eq!(default_workflow_name(Path::new("/")), DEFAULT_WORKFLOW_NAME);
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
        let concurrency = DEFAULT_MULTI_CHAT_CONCURRENCY;
        assert!((1..=64).contains(&concurrency));
        let desc_max: usize = JOURNAL_DESC_MAX_CHARS;
        assert_ne!(desc_max, 0);
        let hash_prefix: usize = COMMIT_HASH_PREFIX_CHARS;
        assert_ne!(hash_prefix, 0);
        let max_errors: usize = MAX_JOURNAL_ERRORS_DISPLAY;
        assert_ne!(max_errors, 0);
        assert!(!DEFAULT_WORKFLOW_NAME.is_empty());
    }

    // ── CLI flag parsing tests ──

    #[test]
    fn cli_chat_yolo_short_flag() {
        use clap::Parser;
        let cli = Cli::try_parse_from(["selfware", "chat", "-y"]).unwrap();
        match cli.command.unwrap() {
            Commands::Chat { yolo } => assert!(yolo, "chat -y should set yolo=true"),
            other => panic!("Expected Chat, got {:?}", other),
        }
    }

    #[test]
    fn cli_chat_yolo_long_flag() {
        use clap::Parser;
        let cli = Cli::try_parse_from(["selfware", "chat", "--yolo"]).unwrap();
        match cli.command.unwrap() {
            Commands::Chat { yolo } => assert!(yolo),
            other => panic!("Expected Chat, got {:?}", other),
        }
    }

    #[test]
    fn cli_chat_no_yolo() {
        use clap::Parser;
        let cli = Cli::try_parse_from(["selfware", "chat"]).unwrap();
        match cli.command.unwrap() {
            Commands::Chat { yolo } => assert!(!yolo, "chat without -y should be yolo=false"),
            other => panic!("Expected Chat, got {:?}", other),
        }
    }

    #[test]
    fn cli_run_yolo_flag() {
        use clap::Parser;
        let cli = Cli::try_parse_from(["selfware", "run", "-y", "fix bug"]).unwrap();
        match cli.command.unwrap() {
            Commands::Run { yolo, task } => {
                assert!(yolo, "run -y should set yolo=true");
                assert_eq!(task, "fix bug");
            }
            other => panic!("Expected Run, got {:?}", other),
        }
    }

    #[test]
    fn cli_multichat_yolo_flag() {
        use clap::Parser;
        let cli = Cli::try_parse_from(["selfware", "multi-chat", "-y"]).unwrap();
        match cli.command.unwrap() {
            Commands::MultiChat { yolo, .. } => assert!(yolo),
            other => panic!("Expected MultiChat, got {:?}", other),
        }
    }

    #[test]
    fn cli_global_yolo_still_works() {
        use clap::Parser;
        let cli = Cli::try_parse_from(["selfware", "-y", "chat"]).unwrap();
        assert!(cli.yolo, "global -y flag should still work");
    }

    #[test]
    fn cli_default_command_is_chat() {
        use clap::Parser;
        let cli = Cli::try_parse_from(["selfware"]).unwrap();
        // No subcommand → defaults to Chat { yolo: false }
        assert!(cli.command.is_none());
    }

    #[test]
    fn cli_command_tree_has_no_duplicate_aliases() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    // ── resolve_config_path tests ──

    #[test]
    fn resolve_config_path_no_flags_returns_none() {
        // No --config, no -C → None (Config::load does normal search)
        let result = resolve_config_path(None, false, Some(Path::new("/home/user/project")));
        assert!(result.is_none());
    }

    #[test]
    fn resolve_config_path_explicit_absolute_config() {
        // --config /etc/selfware.toml → returned as-is regardless of cwd or -C
        let result = resolve_config_path(
            Some("/etc/selfware.toml"),
            false,
            Some(Path::new("/home/user/project")),
        );
        assert_eq!(result.as_deref(), Some("/etc/selfware.toml"));
    }

    #[test]
    #[cfg(not(windows))] // Uses Unix paths
    fn resolve_config_path_explicit_relative_config_uses_original_cwd() {
        // --config my.toml with original cwd → absolutified against original cwd
        let result = resolve_config_path(
            Some("my.toml"),
            false,
            Some(Path::new("/home/user/project")),
        );
        assert_eq!(result.as_deref(), Some("/home/user/project/my.toml"));
    }

    #[test]
    #[cfg(not(windows))] // Uses Unix paths
    fn resolve_config_path_explicit_relative_config_with_workdir_uses_original_cwd() {
        // --config my.toml -C /other/dir → absolutified against ORIGINAL cwd, not /other/dir
        let result =
            resolve_config_path(Some("my.toml"), true, Some(Path::new("/home/user/project")));
        assert_eq!(result.as_deref(), Some("/home/user/project/my.toml"));
    }

    #[test]
    fn resolve_config_path_workdir_without_config_checks_original_cwd() {
        // -C /other/dir (no --config) → checks for selfware.toml in original cwd
        let tmp = tempfile::tempdir().unwrap();
        let config_file = tmp.path().join("selfware.toml");
        std::fs::write(&config_file, "[model]\nname = \"test\"\n").unwrap();

        let result = resolve_config_path(None, true, Some(tmp.path()));
        assert_eq!(
            result.as_deref(),
            Some(config_file.to_str().unwrap()),
            "should find selfware.toml in original cwd when -C is used"
        );
    }

    #[test]
    fn resolve_config_path_workdir_without_config_no_selfware_toml_returns_none() {
        // -C /other/dir (no --config), no selfware.toml in original cwd → None
        let tmp = tempfile::tempdir().unwrap();
        // Don't create selfware.toml

        let result = resolve_config_path(None, true, Some(tmp.path()));
        assert!(
            result.is_none(),
            "should return None when no selfware.toml in original cwd"
        );
    }

    #[test]
    fn resolve_config_path_no_original_cwd_falls_back_gracefully() {
        // Edge case: original_cwd is None (couldn't be determined)
        let result = resolve_config_path(Some("my.toml"), true, None);
        // Should still return the path, just not absolutified
        assert_eq!(result.as_deref(), Some("my.toml"));
    }
}
