//! Selfware Workshop - Your Personal AI Companion
//!
//! Software you own. Software that knows you. Software that lasts.

use anyhow::Result;
use clap::{Parser, Subcommand};

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

    /// Launch full TUI dashboard mode (requires --features extras)
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

#[derive(Subcommand)]
enum Commands {
    /// Open your workshop for an interactive session
    #[command(alias = "c")]
    Chat,

    /// Multi-agent chat with concurrent streams
    #[command(alias = "m")]
    MultiChat {
        /// Maximum concurrent agents (1-16)
        #[arg(short = 'n', long, default_value = "4")]
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
        let expanded = if p.starts_with("~/") {
            dirs::home_dir()
                .map(|h| h.join(&p[2..]).to_string_lossy().to_string())
                .unwrap_or(p.clone())
        } else {
            p.clone()
        };

        // Resolve relative paths
        if std::path::Path::new(&expanded).is_absolute() {
            expanded
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(&expanded).to_string_lossy().to_string())
                .unwrap_or(expanded)
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

    // Apply UI settings from config file first
    config.apply_ui_settings();

    // CLI flags override config file settings
    // For theme, check if --theme was explicitly provided (not default)
    let theme_explicitly_set = std::env::args().any(|arg| arg.starts_with("--theme"));
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
            eprintln!("Error: Empty prompt provided");
            std::process::exit(1);
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
    // Default to TUI when extras feature is enabled and no subcommand specified
    // Use --no-tui to force classic CLI mode
    #[cfg(feature = "extras")]
    {
        let should_use_tui = cli.tui || (cli.command.is_none() && !cli.no_tui);
        if should_use_tui {
            use crate::tui;

            let _user_inputs = tui::run_tui_dashboard(&config.model)?;
            return Ok(());
        }
    }

    #[cfg(not(feature = "extras"))]
    if cli.tui {
        eprintln!("Error: TUI dashboard requires the 'extras' feature.");
        eprintln!("Rebuild with: cargo build --features extras");
        std::process::exit(1);
    }

    // Default to Chat if no subcommand specified (non-extras builds)
    let command = cli.command.unwrap_or(Commands::Chat);

    match command {
        Commands::Chat => {
            if !cli.quiet {
                println!("{}", ui::components::render_welcome(&ctx));
            }
            let mut agent = Agent::new(config).await?;
            agent.interactive().await?;
        }

        Commands::MultiChat { concurrency } => {
            if !cli.quiet {
                println!("{}", render_header(&ctx));
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
            if !cli.quiet {
                println!("{}", render_header(&ctx));
                println!("{}", render_task_start(&task));
            }

            let start = std::time::Instant::now();
            let mut agent = Agent::new(config).await?;
            agent.run_task(&task).await?;

            if !cli.quiet {
                println!("{}", render_task_complete(start.elapsed()));
            }
        }

        Commands::Analyze { path } => {
            if !cli.quiet {
                println!("{}", render_header(&ctx));
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
            if !cli.quiet {
                println!("{}", render_header(&ctx));
                println!(
                    "\n{} {} at {}...\n",
                    Glyphs::TREE,
                    "Visualizing your digital garden".craftsman_voice(),
                    path.as_str().path_local()
                );
            }

            // Build garden visualization
            let garden = build_garden_from_path(&path)?;
            println!("{}", garden.render());
        }

        Commands::Resume { task_id } => {
            if !cli.quiet {
                println!("{}", render_header(&ctx));
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
                if !cli.quiet {
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
            if !cli.quiet {
                println!("{}", render_header(&ctx));
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

                    let desc = if task.task_description.len() > 50 {
                        format!("{}...", &task.task_description[..47])
                    } else {
                        task.task_description.clone()
                    };

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
            if !cli.quiet {
                println!("{}", render_header(&ctx));
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
                    git.commit_hash[..8.min(git.commit_hash.len())].muted()
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
                for error in checkpoint.errors.iter().rev().take(3) {
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
            if !cli.quiet {
                println!(
                    "{} Journal entry {} has been composted.",
                    Glyphs::FALLEN_LEAF,
                    task_id.muted()
                );
            }
        }

        Commands::Status { output_format } => {
            // Count journal entries
            let tasks = Agent::list_tasks().unwrap_or_default();
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
                    if !cli.quiet {
                        println!("{}", render_header(&ctx));
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

                    println!("   {} Model: {}", Glyphs::GEAR, ctx.model_name.emphasis());
                    println!("   {}", hosting);
                    println!(
                        "   {} Garden: {}",
                        Glyphs::SPROUT,
                        ctx.project_path.path_local()
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

        Commands::Workflow {
            file,
            name,
            input,
            dry_run,
        } => {
            if !cli.quiet {
                println!("{}", render_header(&ctx));
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
            let workflow_name = name.unwrap_or_else(|| {
                // Use filename without extension as default
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("default")
                    .to_string()
            });

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

/// Build a digital garden visualization from a path
fn build_garden_from_path(path: &str) -> Result<ui::garden::DigitalGarden> {
    use std::fs;
    use std::time::SystemTime;
    use walkdir::WalkDir;

    let project_name = std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "your garden".to_string());

    let mut garden = ui::garden::DigitalGarden::new(&project_name);

    for entry in WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path_str = entry.path().display().to_string();

        // Skip hidden, target, node_modules, etc.
        if path_str.contains("/.")
            || path_str.contains("/target/")
            || path_str.contains("/node_modules/")
            || path_str.contains("/__pycache__/")
        {
            continue;
        }

        // Only include source files
        let ext = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        if !matches!(
            ext,
            "rs" | "py"
                | "js"
                | "ts"
                | "tsx"
                | "jsx"
                | "go"
                | "rb"
                | "java"
                | "c"
                | "cpp"
                | "h"
                | "hpp"
                | "md"
                | "toml"
                | "yaml"
                | "yml"
                | "json"
        ) {
            continue;
        }

        let metadata = fs::metadata(entry.path()).ok();
        let lines = fs::read_to_string(entry.path())
            .map(|c| c.lines().count())
            .unwrap_or(0);

        let modified = metadata
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Use saturating_sub to handle clock skew/future mtimes
        let age_days = now.saturating_sub(modified) / 86400;

        let plant = ui::garden::GardenPlant {
            path: path_str.clone(),
            name: entry.file_name().to_string_lossy().to_string(),
            extension: ext.to_string(),
            lines,
            age_days,
            last_tended_days: age_days, // Simplified
            growth_stage: ui::garden::GrowthStage::from_metrics(lines, age_days, age_days),
            plant_type: ui::garden::PlantType::from_path(&path_str),
        };

        garden.add_plant(plant);
    }

    Ok(garden)
}
