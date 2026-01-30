use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::info;

mod agent;
mod api;
mod config;
mod memory;
mod safety;
mod tools;

use crate::agent::Agent;
use crate::config::Config;


#[derive(Parser)]
#[command(name = "kimi-agent")]
#[command(about = "Agentic harness for Kimi K2.5")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    #[arg(short, long, value_name = "FILE")]
    config: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start interactive chat session
    Chat,
    /// Execute a single task
    Run {
        /// The task description
        task: String,
    },
    /// Analyze codebase structure
    Analyze {
        /// Path to analyze
        path: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    let cli = Cli::parse();
    let config = Config::load(cli.config.as_deref())?;

    match cli.command {
        Commands::Chat => {
            let mut agent = Agent::new(config).await?;
            agent.interactive().await?;
        }
        Commands::Run { task } => {
            let mut agent = Agent::new(config).await?;
            agent.run_task(&task).await?;
        }
        Commands::Analyze { path } => {
            let mut agent = Agent::new(config).await?;
            agent.analyze(&path).await?;
        }
    }

    Ok(())
}
