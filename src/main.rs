//! Selfware Workshop - Your Personal AI Companion
//!
//! Software you own. Software that knows you. Software that lasts.

use anyhow::Result;
use clap::{Parser, Subcommand};

mod agent;
mod analyzer;
mod api;
mod cache;
mod checkpoint;
mod cognitive;
mod config;
mod confirm;
mod dry_run;
mod input;
mod memory;
mod multiagent;
mod parallel;
mod process_manager;
mod redact;
mod safety;
mod swarm;
mod telemetry;
mod tokens;
mod tool_parser;
mod tools;
mod tui;
mod ui;
mod verification;
mod yolo;

use crate::agent::Agent;
use crate::config::Config;
use crate::telemetry::init_tracing;
use crate::ui::components::{
    render_header, render_task_complete, render_task_start, WorkshopContext,
};
use crate::ui::style::{Glyphs, SelfwareStyle};

use crate::config::ExecutionMode;

#[derive(Parser)]
#[command(name = "selfware")]
#[command(about = "Your personal AI workshop — software you own, software that lasts")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

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
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize telemetry
    init_tracing();

    let cli = Cli::parse();

    // Resolve config path
    let config_path: Option<String> = cli.config.map(|p| {
        if std::path::Path::new(&p).is_absolute() {
            p
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(&p).to_string_lossy().to_string())
                .unwrap_or(p)
        }
    });

    // Change to working directory if specified
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

    let ctx = WorkshopContext::from_config(&config.endpoint, &config.model).with_mode(exec_mode);

    match cli.command {
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
            println!("{}", render_header(&ctx));
            println!(
                "\n{} {} at {}...\n",
                Glyphs::TREE,
                "Visualizing your digital garden".craftsman_voice(),
                path.as_str().path_local()
            );

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
                println!(
                    "{} Continuing: {}\n",
                    Glyphs::SPROUT,
                    task.craftsman_voice()
                );
                agent.continue_execution().await?;
            }
        }

        Commands::Journal => {
            println!("{}", render_header(&ctx));
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
            println!("{}", render_header(&ctx));
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
            println!(
                "{} Journal entry {} has been composted.",
                Glyphs::FALLEN_LEAF,
                task_id.muted()
            );
        }

        Commands::Status => {
            println!("{}", render_header(&ctx));
            println!(
                "\n{} {}\n",
                Glyphs::HOME,
                "Workshop Status".workshop_title()
            );

            let hosting = if ctx.is_local_model {
                format!("{} Running on your hardware (local)", Glyphs::HOME).garden_healthy()
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

        let age_days = (now - modified) / 86400;

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
