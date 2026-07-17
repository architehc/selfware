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

    /// Run in coordinator mode (multi-agent orchestration)
    /// When set, the agent runs as a coordinator with restricted tool access,
    /// orchestrating parallel work across multiple worker agents.
    #[arg(long)]
    pub(crate) coordinator: bool,

    /// Validate the configuration file and exit without running the agent
    #[arg(long)]
    pub(crate) validate_config: bool,

    /// Enable debug-channel output. With no value, enables every channel
    /// (requests, responses, gates, turns). Use `--debug=requests,gates`
    /// to enable specific ones. Channels: requests, responses, gates, turns, all.
    #[arg(
        long,
        value_name = "CHANNELS",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "",
    )]
    pub(crate) debug: Option<String>,

    /// Output format for headless mode: text, json, or stream-json.
    /// `global` so it may follow a subcommand too (e.g. `runs list --output-format json`).
    #[arg(long, value_enum, default_value = "text", global = true)]
    pub(crate) output_format: HeadlessOutputFormat,

    /// Maximum number of agent loop iterations (hard limit)
    #[arg(long)]
    pub(crate) max_turns: Option<usize>,

    /// Maximum total prompt+completion tokens before stopping
    #[arg(long)]
    pub(crate) max_budget_tokens: Option<usize>,

    /// Maximum wall-clock seconds before stopping
    #[arg(long)]
    pub(crate) max_wall_secs: Option<u64>,

    /// Maximum provider-reported USD cost before stopping (e.g. OpenRouter usage.cost)
    #[arg(long)]
    pub(crate) max_cost_usd: Option<f64>,

    /// Configuration profile to apply (e.g. `architect`, `swarm-8`, `batch-16`,
    /// `batch-32`, `visual`, `quick`).  Overrides `max_tokens` and
    /// `temperature` from the profile's built-in defaults.
    #[arg(long, value_name = "NAME")]
    pub(crate) profile: Option<String>,
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

/// Output format for headless (`-p` / `--run`) mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum HeadlessOutputFormat {
    /// Human-readable text (default)
    #[default]
    Text,
    /// Single JSON object emitted at task completion
    Json,
    /// Line-delimited JSON stream emitted during the run
    #[value(name = "stream-json")]
    StreamJson,
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
#[allow(clippy::large_enum_variant)]
pub(crate) enum Commands {
    /// Check system dependencies and tool availability
    Doctor,

    /// Diagnose the configured LLM backend and model setup
    LlmDoctor,

    /// Trust this repository's checkout-local selfware.toml. Trusting ACTIVATES
    /// all of its privileged settings, which are otherwise stripped for an
    /// untrusted repo: shell hooks, MCP subprocess servers, wildcard
    /// tool-permission grants, the post-edit command, yolo / destructive-shell,
    /// and safety-path/confirmation overrides — and it allows a globally-exported
    /// SELFWARE_API_KEY to be sent to the endpoint the config selects. Only trust
    /// repositories whose selfware.toml you have reviewed. Records the config's
    /// canonical path in ~/.selfware/trusted_repos.
    Trust {
        /// Path to the config to trust (default: ./selfware.toml).
        #[arg(default_value = "selfware.toml")]
        path: String,
    },

    /// Test local development workflow
    #[command(alias = "t")]
    Test {
        /// Test pattern to run (all, unit, integration, e2e, workflow)
        #[arg(short, long, default_value = "workflow")]
        pattern: String,
        /// Output format (text, pretty, json)
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// SWE-bench commands
    #[command(alias = "swe")]
    SWEBench {
        #[command(subcommand)]
        command: SWEBenchCommands,
    },

    /// Run comprehensive benchmark suite
    ///
    /// With no subcommand, runs the legacy throughput/e2e suite (back-compat
    /// with previous releases).  Use a subcommand for newer benchmarks.
    #[command(alias = "bm")]
    Bench {
        #[command(subcommand)]
        command: Option<BenchCommand>,
        /// Endpoint URL to benchmark (legacy mode, defaults to config)
        #[arg(short, long)]
        endpoint: Option<String>,
        /// Benchmark suites to run (legacy mode: throughput,e2e,multilang,all)
        #[arg(short, long, default_value = "throughput,e2e")]
        suite: String,
        /// Number of concurrent tasks (legacy mode)
        #[arg(short, long, default_value_t = 4)]
        concurrent: usize,
    },

    /// Run long-running system test (8+ hours)
    #[command(alias = "lt")]
    LongTest {
        /// Duration in hours (default: 8)
        #[arg(short = 'H', long, default_value_t = 8)]
        hours: u64,
        /// Timeout per project in seconds (default: 900)
        #[arg(short, long, default_value_t = 900)]
        timeout: u64,
        /// Maximum iterations per project (default: 80)
        #[arg(short, long, default_value_t = 80)]
        max_iters: usize,
        /// Maximum concurrent projects (default: 1)
        #[arg(short, long, default_value_t = 1)]
        concurrent: usize,
        /// Endpoint URL (defaults to config)
        #[arg(short, long)]
        endpoint: Option<String>,
        /// Model name (defaults to config)
        #[arg(long)]
        model: Option<String>,
        /// Templates directory
        #[arg(long)]
        templates: Option<String>,
        /// Output directory for results
        #[arg(short, long, default_value = "long_run_results")]
        output: String,
    },

    /// Auto-detect and configure endpoint settings
    #[command(alias = "ac")]
    AutoConfig {
        /// API endpoint URL to test (e.g., http://127.0.0.1:1234/v1)
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

    /// Zero-config auto-setup: scan local LLM servers, detect models, generate config
    #[command(alias = "up")]
    Unpack {
        /// Just scan without writing config
        #[arg(long)]
        scan: bool,

        /// Save generated config to selfware.toml
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

        /// Workflow to use: "default" for evolution daemon, "rsi" for RSI orchestrator
        #[arg(long, default_value = "default")]
        workflow: String,
    },

    /// Run as MCP server (stdio transport) so other AI tools can use Selfware's capabilities
    McpServer,

    /// Start selfware in LSP server mode (for editor extensions)
    Lsp,

    /// Experimental batch entrypoint — runs tasks from a file sequentially
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

    /// Workflow commands (SWL and YAML)
    #[command(alias = "w")]
    Workflow {
        #[command(subcommand)]
        command: WorkflowCommands,
    },

    /// Inspect or manipulate selfware configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Inspect workflow state
    #[command(alias = "st")]
    State {
        #[command(subcommand)]
        command: StateCommands,
    },

    /// Supervised run management (start, list, abort agent runs)
    Runs {
        #[command(subcommand)]
        command: RunsCommand,
    },
}

/// Subcommands of `selfware swebench`.
#[derive(Subcommand, Clone, Debug)]
pub(crate) enum SWEBenchCommands {
    /// Diagnose SWE-bench Pro traces
    Diagnose {
        /// Output directory containing trace.jsonl files
        output_dir: String,
    },
}

/// Subcommands of `selfware bench` for the modern benchmark surface.
#[derive(Subcommand, Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum BenchCommand {
    /// Run SWE-bench Pro instances against one or more local quants
    #[command(name = "swebench-pro")]
    SwebenchPro(SwebenchProArgs),
}

/// Flags for `selfware bench swebench-pro`.
#[derive(clap::Args, Clone, Debug)]
pub(crate) struct SwebenchProArgs {
    /// Comma-separated quant labels (or 'all' for every catalog entry)
    #[arg(
        long,
        default_value = "Qwen3.6-35B-A3B-Q3_K_XL,Qwen3.6-27B-HauhauCS-Q4_K_P,Qwen3.6-27B-HauhauCS-IQ4_XS,Qwen3.6-27B-HauhauCS-Q2_K_P"
    )]
    pub quants: String,

    /// How many SWE-bench Pro instances to subset (sorted by problem_statement length)
    #[arg(long, default_value_t = 3)]
    pub instances: usize,

    /// Comma-separated instance_ids (overrides --instances)
    #[arg(long)]
    pub instance_ids: Option<String>,

    /// Per-instance agent timeout (seconds)
    #[arg(long, default_value_t = 900)]
    pub scenario_timeout: u64,

    /// llama-server context size
    #[arg(long, default_value_t = 262_144)]
    pub ctx: u32,

    /// llama-server `--parallel` slots
    #[arg(long, default_value_t = 2)]
    pub parallel: u32,

    /// Port the spawned llama-server listens on (default 8000).  Selfware
    /// sub-runs are pointed at `http://127.0.0.1:<port>/v1`.
    #[arg(long, default_value_t = 8000)]
    pub port: u16,

    /// Directory containing the GGUF quant files (default: built-in models dir).
    #[arg(long)]
    pub models_dir: Option<String>,

    /// Path to the llama-server binary (default: built-in / PATH lookup).
    #[arg(long)]
    pub llama_server_bin: Option<String>,

    /// llama-server --tensor-split, e.g. "24,24". Use "auto" to omit the flag.
    #[arg(long)]
    pub tensor_split: Option<String>,

    /// Host/interface llama-server binds to (default 127.0.0.1).
    #[arg(long)]
    pub host: Option<String>,

    /// Use an already-running OpenAI-compatible endpoint (e.g.
    /// http://127.0.0.1:8000/v1) instead of booting a llama-server per quant.
    #[arg(long)]
    pub endpoint: Option<String>,

    /// Load the dataset from a local JSONL file instead of HuggingFace.
    #[arg(long)]
    pub instances_jsonl: Option<String>,

    /// Future-use: bench-side concurrency (currently surfaced in plan.json)
    #[arg(long, default_value_t = 1)]
    pub concurrency: u32,

    /// Number of trials per (quant, instance) pair
    #[arg(long, default_value_t = 1)]
    pub trials: u32,

    /// Number of candidate patches to generate per (quant, instance, trial).
    /// Each candidate gets its own subdirectory.  The best candidate is
    /// selected honestly and promoted to the trial-level patch.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=4))]
    pub candidates: u32,

    /// Output directory (default: reports/swebench_pro/<timestamp>)
    #[arg(long)]
    pub output: Option<String>,

    /// Path to the selfware binary used for sub-runs (defaults to current binary)
    #[arg(long)]
    pub selfware_bin: Option<String>,

    /// Skip (quant, instance, trial) triples whose .pred already exists
    #[arg(long)]
    pub skip_existing: bool,

    /// Resume an existing swebench-pro output directory when manifest options match.
    #[arg(long)]
    pub resume: bool,

    /// Re-run trials that are already complete in the manifest.
    #[arg(long)]
    pub force_rerun: bool,

    /// Prompt mode: `official` excludes oracle test fields for valid scoring;
    /// `diagnostic` includes fail-to-pass tests for local debugging.
    #[arg(long, default_value = "official")]
    pub prompt_mode: String,

    /// Prompt profile: `default` (legacy) or `swebench_pro` (Qwen3.6-optimized).
    #[arg(long, default_value = "swebench_pro")]
    pub prompt_profile: String,

    /// Run official SWE-bench Pro Docker eval after generating patches.
    #[arg(long)]
    pub official_eval: bool,

    /// Path to the SWE-bench Pro evaluator script.
    #[arg(
        long,
        default_value = "SWE-bench_Pro-os/swe_bench_pro_eval.py"
    )]
    pub official_eval_script: String,

    /// Path to the SWE-bench Pro raw sample CSV/JSONL.
    #[arg(
        long,
        default_value = "SWE-bench_Pro-os/helper_code/sweap_eval_full_v2.jsonl"
    )]
    pub official_eval_raw_sample_path: String,

    /// Directory containing per-instance run_script.sh/parser.py files.
    #[arg(long, default_value = "SWE-bench_Pro-os/run_scripts")]
    pub official_eval_scripts_dir: String,

    /// Docker Hub user/org that hosts sweap-images.
    #[arg(long, default_value = "jefzda")]
    pub official_eval_dockerhub_username: String,

    /// Parallel workers for official eval.
    #[arg(long, default_value_t = 1)]
    pub official_eval_num_workers: u32,

    /// Use Modal instead of local Docker for official eval.
    #[arg(long)]
    pub official_eval_modal: bool,

    /// Re-run official eval even when per-instance outputs already exist.
    #[arg(long)]
    pub official_eval_redo: bool,

    /// Block network access inside official-eval containers.
    #[arg(long)]
    pub official_eval_block_network: bool,
}

#[derive(Subcommand, Clone, Debug)]
pub(crate) enum ConfigCommands {
    /// Print effective configuration with provenance annotations
    Show {
        /// Render as JSON instead of the human-readable table
        #[arg(long)]
        json: bool,
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

/// Subcommands of `selfware runs`.
#[derive(Subcommand, Clone, Debug)]
pub(crate) enum RunsCommand {
    /// Start a task as a supervised run and stream its events.
    Start {
        /// The task description to run.
        task: String,
    },

    /// List runs tracked by the current process.
    ///
    /// NOTE: Without a persistent supervisor/daemon, this is empty across
    /// separate CLI invocations — only runs started *in this process* are
    /// visible.
    List,

    /// Abort a run by its registry id (e.g. `12345-1`), across processes.
    Abort {
        /// The run id to abort, as shown by `selfware runs list`.
        id: String,
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
