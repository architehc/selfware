//! CLI argument definitions: structs, enums, and subcommands.

use clap::{Parser, Subcommand};

use crate::config::ExecutionMode;

pub(crate) const DEFAULT_MULTI_CHAT_CONCURRENCY: usize = 4;

#[derive(Parser)]
#[command(name = "selfware")]
#[command(about = "Your personal AI workshop — software you own, software that lasts")]
#[command(version)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Commands>,

    /// Headless mode: run prompt directly and exit (like qwen -p)
    #[arg(short = 'p', long, value_name = "PROMPT")]
    pub(crate) prompt: Option<String>,

    /// Config file path
    #[arg(short, long, value_name = "FILE")]
    pub(crate) config: Option<String>,

    /// Working directory (your garden)
    #[arg(short = 'C', long, value_name = "DIR")]
    pub(crate) workdir: Option<String>,

    /// Quiet mode (minimal output)
    #[arg(short, long)]
    pub(crate) quiet: bool,

    /// Execution mode: normal (ask), auto-edit, yolo, daemon
    #[arg(short = 'm', long, value_enum)]
    pub(crate) mode: Option<ExecutionMode>,

    /// Shortcut for --mode=yolo
    #[arg(short = 'y', long)]
    pub(crate) yolo: bool,

    /// Shortcut for --mode=daemon (run forever)
    #[arg(long)]
    pub(crate) daemon: bool,

    /// Disable colored output
    #[arg(long)]
    pub(crate) no_color: bool,

    /// Launch full TUI dashboard mode (requires --features tui)
    /// This is the default when no subcommand is specified
    #[arg(long)]
    pub(crate) tui: bool,

    /// Use classic CLI mode instead of TUI (overrides default TUI)
    #[arg(long)]
    pub(crate) no_tui: bool,

    /// Color theme: amber (default), ocean, minimal, high-contrast
    #[arg(long, value_enum, default_value = "amber")]
    pub(crate) theme: Theme,

    /// Compact output mode (less visual chrome, more dense)
    #[arg(long)]
    pub(crate) compact: bool,

    /// Verbose mode (detailed tool output and debug info)
    #[arg(short = 'v', long)]
    pub(crate) verbose: bool,

    /// Always display token usage after each response
    #[arg(long)]
    pub(crate) show_tokens: bool,

    /// Use ASCII-only output (no emoji or extended Unicode)
    #[arg(long)]
    pub(crate) ascii: bool,

    /// Plan mode: agent proposes tool calls without executing them
    #[arg(long)]
    pub(crate) plan: bool,

    /// Resume a named chat session (alias for `selfware chat --resume <name>`)
    #[arg(long, value_name = "NAME")]
    pub(crate) resume_session: Option<String>,
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
pub(crate) enum DemoScenarioKind {
    Archaeology,
    FeatureFactory,
    BugHunt,
    TokenChallenge,
}

#[derive(Subcommand, Clone, Debug)]
pub(crate) enum Commands {
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
    #[command(alias = "bm")]
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

    /// Launch Command Center for SWL workflow monitoring
    #[cfg(feature = "tui")]
    #[command(alias = "cc")]
    CommandCenter {
        /// Update mode: poll or stream
        #[arg(short, long, default_value = "poll")]
        mode: String,
        
        /// Auto-refresh interval in milliseconds
        #[arg(short, long, default_value = "100")]
        refresh: u64,
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

    /// Workflow commands (SWL and YAML)
    #[command(alias = "w")]
    Workflow {
        #[command(subcommand)]
        command: WorkflowCommands,
    },

    /// Inspect workflow state
    #[command(alias = "st")]
    State {
        #[command(subcommand)]
        command: StateCommands,
    },
}

#[derive(Subcommand, Clone, Debug)]
pub(crate) enum WorkflowCommands {
    /// Validate an SWL file
    Validate {
        /// Path to the SWL file
        file: String,
    },

    /// Run an SWL workflow
    Run {
        /// Path to the SWL file
        file: String,

        /// Workflow name to run (defaults to first workflow in file)
        #[arg(short, long)]
        workflow: Option<String>,

        /// Input variables (KEY=VALUE format)
        #[arg(short, long)]
        input: Vec<String>,

        /// Dry-run mode (log but don't execute)
        #[arg(long)]
        dry_run: bool,
    },

    /// List available workflows in the current directory
    List {
        /// Directory to search for workflows (default: current directory)
        #[arg(short, long, default_value = ".")]
        dir: String,

        /// Show all workflows including SWL and YAML formats
        #[arg(long)]
        all: bool,
    },
}

#[derive(Subcommand, Clone, Debug)]
pub(crate) enum StateCommands {
    /// Show state for a workflow
    Show {
        /// Workflow name
        workflow: String,

        /// Output format
        #[arg(short, long, value_enum, default_value = "text")]
        format: StateOutputFormat,

        /// State backend directory (default: ~/.selfware/state)
        #[arg(short, long)]
        dir: Option<String>,
    },

    /// List all saved workflow states
    List {
        /// State backend directory (default: ~/.selfware/state)
        #[arg(short, long)]
        dir: Option<String>,
    },

    /// Delete state for a workflow
    Delete {
        /// Workflow name
        workflow: String,

        /// State backend directory (default: ~/.selfware/state)
        #[arg(short, long)]
        dir: Option<String>,

        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Export state to JSON file
    Export {
        /// Workflow name
        workflow: String,

        /// Output file path
        #[arg(short, long)]
        output: String,

        /// State backend directory (default: ~/.selfware/state)
        #[arg(short, long)]
        dir: Option<String>,
    },

    /// Import state from JSON file
    Import {
        /// Workflow name
        workflow: String,

        /// Input file path
        #[arg(short, long)]
        input: String,

        /// State backend directory (default: ~/.selfware/state)
        #[arg(short, long)]
        dir: Option<String>,

        /// Skip confirmation if state exists
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub(crate) enum StateOutputFormat {
    /// Human-readable text (default)
    #[default]
    Text,
    /// JSON output
    Json,
    /// Table format
    Table,
}
