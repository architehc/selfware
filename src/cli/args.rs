//! CLI argument definitions: structs, enums, and subcommands.

use clap::{Parser, Subcommand};

use crate::config::ExecutionMode;

pub(crate) const DEFAULT_MULTI_CHAT_CONCURRENCY: usize = 4;

/// Clap value parser rejecting zero — for counts that must be >= 1
/// (e.g. `evolve --generations`, where 0 used to mean "infinite").
fn parse_nonzero_usize(s: &str) -> Result<usize, String> {
    let value: usize = s.parse().map_err(|_| format!("invalid number: {}", s))?;
    if value == 0 {
        return Err("value must be >= 1".to_string());
    }
    Ok(value)
}

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
    #[arg(short, long, value_name = "FILE", global = true)]
    pub(crate) config: Option<String>,

    /// Working directory
    #[arg(short = 'C', long, value_name = "DIR", global = true)]
    pub(crate) workdir: Option<String>,

    /// Quiet mode (minimal output)
    #[arg(short, long, global = true)]
    pub(crate) quiet: bool,

    /// Execution mode: normal (ask), auto-edit, yolo, daemon
    #[arg(short = 'm', long, value_enum, global = true)]
    pub(crate) mode: Option<ExecutionMode>,

    /// Model override for this session (overrides the config `model` key;
    /// long-only on purpose — `-m` stays --mode for existing scripts)
    #[arg(long, value_name = "MODEL", global = true)]
    pub(crate) model: Option<String>,

    /// Shortcut for --mode=yolo
    #[arg(short = 'y', long, global = true)]
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
    #[arg(short = 'v', long, global = true)]
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

    /// Resume the MOST RECENT journal entry and continue it (claude -c
    /// parity; long-only so `-c` stays --config for existing scripts)
    #[arg(long = "continue")]
    pub(crate) continue_flag: bool,

    /// Multi-chat: assign each task to role-matched idle swarm agents by trust.
    ///
    /// Only meaningful with `multi-chat`. The coordinator's assignment gates
    /// execution — a task that cannot be assigned makes no LLM calls — but
    /// execution itself is the same single-completion-per-agent fan-out as
    /// plain multi-chat; there are no separate worker agents and no
    /// restricted tool set. Other subcommands ignore this flag (with a note).
    #[arg(long, global = true)]
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
    #[arg(long, global = true)]
    pub(crate) max_turns: Option<usize>,

    /// Maximum total prompt+completion tokens before stopping
    #[arg(long, global = true)]
    pub(crate) max_budget_tokens: Option<usize>,

    /// Maximum wall-clock seconds before stopping
    #[arg(long, global = true)]
    pub(crate) max_wall_secs: Option<u64>,

    /// Maximum provider-reported USD cost before stopping (e.g. OpenRouter usage.cost)
    #[arg(long, global = true)]
    pub(crate) max_cost_usd: Option<f64>,

    /// Configuration profile to apply (e.g. `architect`, `swarm-8`, `batch-16`,
    /// `batch-32`, `visual`, `quick`).  Overrides `max_tokens` and
    /// `temperature` from the profile's built-in defaults.
    #[arg(long, value_name = "NAME", global = true)]
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
    /// Interactive setup wizard for first-time configuration
    #[command(display_order = 1)]
    Init {
        /// Use a specific template (rust, python, node, minimal)
        #[arg(long)]
        template: Option<String>,
        /// Ask what to build, then scaffold it into the current directory
        #[arg(long)]
        scaffold: bool,
    },

    /// Start an interactive chat session
    #[command(alias = "c", display_order = 2)]
    Chat,

    /// Run a task headless and exit (aliases: `r`, `exec`)
    #[command(alias = "r", visible_alias = "exec", display_order = 3)]
    Run {
        /// The task to run (omit when --preset is given)
        #[arg(required_unless_present = "preset")]
        task: Option<String>,
        /// Inject a user skill's instructions (from ~/.selfware/skills or ./.selfware/skills)
        #[arg(long)]
        skill: Option<String>,
        /// Run an evolve preset by id (its task + invariants become the prompt)
        #[arg(long)]
        preset: Option<String>,
    },

    /// Resume a task from a journal entry
    #[command(display_order = 4)]
    Resume {
        /// Journal entry ID
        task_id: String,
    },

    /// Show model, endpoint, and journal status
    #[command(display_order = 5)]
    Status,

    /// Check system dependencies and tool availability
    #[command(display_order = 6)]
    Doctor,

    /// Diagnose the configured LLM backend and model setup
    #[command(display_order = 7)]
    LlmDoctor,

    /// Trust this repository's checkout-local selfware.toml. Trusting ACTIVATES
    /// all of its privileged settings, which are otherwise stripped for an
    /// untrusted repo: shell hooks, MCP subprocess servers, wildcard
    /// tool-permission grants, the post-edit command, yolo / destructive-shell,
    /// and safety-path/confirmation overrides — and it allows a globally-exported
    /// SELFWARE_API_KEY to be sent to the endpoint the config selects. Only trust
    /// repositories whose selfware.toml you have reviewed. Records the config's
    /// canonical path in ~/.selfware/trusted_repos.
    #[command(display_order = 10)]
    Trust {
        /// Path to the config to trust (default: ./selfware.toml).
        #[arg(default_value = "selfware.toml")]
        path: String,
    },

    /// Test local development workflow
    #[command(alias = "t", hide = true)]
    Test {
        /// Test pattern to run (all, unit, integration, e2e, workflow)
        #[arg(short, long, default_value = "workflow")]
        pattern: String,
        /// Output format (text, pretty, json)
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// SWE-bench commands
    #[command(alias = "swe", hide = true)]
    SWEBench {
        #[command(subcommand)]
        command: SWEBenchCommands,
    },

    /// Run comprehensive benchmark suite
    ///
    /// With no subcommand, runs the legacy throughput/e2e suite (back-compat
    /// with previous releases).  Use a subcommand for newer benchmarks.
    #[command(alias = "bm", hide = true)]
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
        #[arg(short = 'n', long, default_value_t = 4)]
        concurrent: usize,
    },

    /// Run long-running system test (8+ hours)
    #[command(alias = "lt", hide = true)]
    LongTest {
        /// Duration in hours (default: 8)
        #[arg(short = 'H', long, default_value_t = 8)]
        hours: u64,
        /// Timeout per project in seconds (default: 900)
        #[arg(short, long, default_value_t = 900)]
        timeout: u64,
        /// Maximum iterations per project (default: 80)
        #[arg(short = 'i', long, default_value_t = 80)]
        max_iters: usize,
        /// Maximum concurrent projects (default: 1)
        #[arg(short = 'n', long, default_value_t = 1)]
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
    #[command(alias = "ac", display_order = 10)]
    AutoConfig {
        /// API endpoint URL to test (e.g., http://127.0.0.1:1234/v1)
        #[arg(short, long)]
        endpoint: Option<String>,

        /// Model name to test (auto-detected if not provided)
        #[arg(long)]
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
    #[command(alias = "up", display_order = 10)]
    Unpack {
        /// Just scan without writing config
        #[arg(long)]
        scan: bool,

        /// Save generated config to selfware.toml
        #[arg(long)]
        save: bool,
    },

    /// Multi-agent chat with concurrent streams.
    ///
    /// Without a TASK, starts an interactive session. With a TASK, runs a
    /// single fan-out across the role agents, prints the aggregated results,
    /// and exits (headless one-shot); `-p <task> multi-chat` is equivalent.
    #[command(alias = "m", display_order = 10)]
    MultiChat {
        /// Run one fan-out for this task and exit instead of starting an
        /// interactive session.
        task: Option<String>,
        /// Maximum concurrent agents (1-16)
        #[arg(short = 'n', long, default_value_t = DEFAULT_MULTI_CHAT_CONCURRENCY)]
        concurrency: usize,
    },

    /// Analyze a codebase
    #[command(alias = "a", display_order = 10)]
    Analyze {
        /// Path to analyze
        #[arg(default_value = ".")]
        path: String,
    },

    /// Render the codebase as an ecosystem visualization
    #[command(display_order = 10)]
    Garden {
        /// Path to visualize
        #[arg(default_value = ".")]
        path: String,
    },

    /// Explore the workspace as a code knowledge graph
    #[command(display_order = 10)]
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
    #[command(hide = true)]
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
    #[command(display_order = 10)]
    Dashboard,

    /// List journal entries from past tasks
    #[command(alias = "j", display_order = 10)]
    Journal,

    /// View a specific journal entry
    #[command(display_order = 10)]
    JournalEntry {
        /// Entry ID
        task_id: String,
    },

    /// Remove a journal entry
    #[command(display_order = 10)]
    JournalDelete {
        /// Entry ID
        task_id: String,
    },

    /// Self-improve: analyze and edit the selfware codebase
    #[cfg(feature = "self-improvement")]
    #[command(hide = true)]
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
    #[command(hide = true)]
    Evolve {
        /// Number of generations (must be >= 1)
        #[arg(short, long, default_value = "10", value_parser = parse_nonzero_usize)]
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

    /// Manage MCP servers (`mcp list` shows the configured ones)
    #[command(display_order = 10)]
    Mcp {
        #[command(subcommand)]
        command: McpCommands,
    },

    /// Run as MCP server (stdio transport) so other AI tools can use Selfware's capabilities
    #[command(display_order = 10)]
    McpServer,

    /// Self-evolve: build the code graph and serve the evolution UI over HTTP
    #[command(display_order = 10)]
    SelfEvolve {
        /// Port for the evolve HTTP server
        #[arg(short, long, default_value_t = 7777)]
        port: u16,
    },

    /// Start selfware in LSP server mode (for editor extensions)
    #[command(hide = true)]
    Lsp,

    /// Experimental batch entrypoint — runs tasks from a file sequentially
    #[command(alias = "bat", hide = true)]
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
    #[command(alias = "w", display_order = 10)]
    Workflow {
        #[command(subcommand)]
        command: WorkflowCommands,
    },

    /// Inspect or manipulate selfware configuration
    #[command(display_order = 10)]
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Inspect workflow state
    #[command(alias = "st", display_order = 10)]
    State {
        #[command(subcommand)]
        command: StateCommands,
    },

    /// Supervised run management (start, list, abort agent runs)
    #[command(display_order = 10)]
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
#[derive(clap::Args, Clone, Debug, Default)]
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

    /// Path to the SWE-bench Pro evaluator script (required with --official-eval).
    #[arg(long)]
    pub official_eval_script: Option<String>,

    /// Path to the SWE-bench Pro raw sample CSV/JSONL (required with --official-eval).
    #[arg(long)]
    pub official_eval_raw_sample_path: Option<String>,

    /// Directory containing per-instance run_script.sh/parser.py files (required with --official-eval).
    #[arg(long)]
    pub official_eval_scripts_dir: Option<String>,

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

    /// Store an API key in the OS keyring for the configured endpoint
    SetKey {
        /// The API key to store (kept out of files; omit to be prompted)
        key: Option<String>,
    },
}

#[derive(Subcommand, Clone, Debug)]
pub(crate) enum McpCommands {
    /// List MCP servers from the resolved configuration (read-only —
    /// reports what is configured, not live connectivity)
    List,

    /// Add an MCP server to the config file
    Add {
        /// Server name (used as the tool prefix)
        name: String,
        /// Command that spawns the server
        #[arg(long)]
        command: String,
        /// Arguments for the command (repeatable)
        #[arg(long)]
        args: Vec<String>,
    },

    /// Remove an MCP server from the config file
    Remove {
        /// Server name to remove
        name: String,
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

    /// Generate a Rust stub from an SWL workflow file
    Codegen {
        /// Path to the .swl file
        file: std::path::PathBuf,
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
