//! Selfware Workshop - Your Personal AI Companion
//!
//! Software you own. Software that knows you. Software that lasts.

pub(crate) mod args;
mod bench_cmd;
pub(crate) mod headless;
#[cfg(feature = "self-improvement")]
mod improve_cmd;
pub(crate) mod init_wizard;
mod subcommands;
mod task_cmd;
mod workflow_cmd;

#[allow(unused_imports)]
pub(crate) use bench_cmd::*;
#[cfg(feature = "self-improvement")]
#[allow(unused_imports)]
pub(crate) use improve_cmd::*;
pub use subcommands::parse_task_file;
#[allow(unused_imports)]
pub(crate) use subcommands::*;
#[allow(unused_imports)]
pub(crate) use workflow_cmd::*;

#[cfg(feature = "tui")]
use std::sync::mpsc;

use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use std::path::Path;
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

use args::*;

const JOURNAL_DESC_MAX_CHARS: usize = 50;
const COMMIT_HASH_PREFIX_CHARS: usize = 8;
const MAX_JOURNAL_ERRORS_DISPLAY: usize = 3;

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

/// Load the config for `run()`, recording operator-explicit provenance when
/// the path came from an explicit `--config`/`-c` flag.
///
/// The untrusted-repo endpoint gate (`config::loader`) exempts config FILEs
/// the operator chose explicitly — `SELFWARE_CONFIG` and an explicit
/// `--config` path — from the checkout-local refusal: the operator pointed at
/// that file by name, so it is not repository content smuggled past them. The
/// default-name `selfware.toml` (auto-discovery, or the `-C` original-cwd
/// probe) carries NO explicit provenance and stays untrusted. See
/// `Config::load_async_explicit_cli_path` / `Config::load_async`.
async fn load_async_config_with_provenance(
    cli: &Cli,
    config_path: &Option<String>,
) -> anyhow::Result<Config> {
    if cli.config.is_some() {
        let path = config_path
            .as_deref()
            .expect("an explicit --config path is always resolved to a concrete path");
        crate::config::Config::load_async_explicit_cli_path(path).await
    } else {
        crate::config::Config::load_async(config_path.as_deref()).await
    }
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
/// Returns `false` (run calibration) only when the install appears to be
/// completely fresh (bare defaults, no CLI overrides). `autoconfig` and
/// `unpack` run their own calibration in `handle_command` (honoring flags
/// such as `--scan` and `--save`), so startup calibration is skipped for them.
fn should_skip_calibration(cli: &Cli, config: &Config) -> bool {
    // Commands that run their own calibration pass in handle_command.
    let is_opt_in = matches!(
        cli.command,
        Some(Commands::AutoConfig { .. } | Commands::Unpack { .. })
    );
    if is_opt_in {
        return true;
    }

    // Diagnostic / read-only / setup commands should never trigger port scans or
    // backend auto-starts.
    let is_diagnostic = matches!(
        cli.command,
        Some(
            Commands::Init { .. }
                | Commands::Config { .. }
                | Commands::Bench { .. }
                | Commands::Doctor { .. }
                | Commands::LlmDoctor
                | Commands::Boot { .. }
                | Commands::Test { .. }
                | Commands::Status
                | Commands::ArtifactHarness { .. }
                | Commands::Review { .. }
                | Commands::Analyze { .. }
                | Commands::Garden { .. }
                | Commands::Graph { .. }
                | Commands::Journal
                | Commands::JournalEntry { .. }
                | Commands::JournalDelete { .. }
                | Commands::Skill { .. }
                | Commands::Mcp { .. }
                | Commands::State { .. }
                | Commands::Workflow { .. }
                | Commands::Runs { .. }
                | Commands::Resources { .. }
                | Commands::Killswitch { .. }
                | Commands::Tasks { .. }
                | Commands::Agents { .. }
                | Commands::Task { .. }
                | Commands::Trust { .. }
        )
    );
    if is_diagnostic {
        return true;
    }

    // User pointed at a specific config file (flag or env) → respect their choice.
    if cli.config.is_some() || std::env::var_os("SELFWARE_CONFIG").is_some() {
        return true;
    }

    // A config file exists on disk — an auto-discovered `./selfware.toml` or the
    // global `~/.config/selfware/config.toml`. Its presence means the user has
    // configured selfware, even if some values happen to equal the built-in
    // defaults. Calibration would port-scan for a local backend and overwrite
    // their endpoint/model, so skip it. (Without this, a project selfware.toml
    // that sets model = the default value is mistaken for a bare install and
    // silently clobbered by a locally-detected backend.)
    let cwd_config_exists = std::path::Path::new("selfware.toml").is_file();
    let global_config_exists = dirs::home_dir()
        .map(|h| h.join(".config/selfware/config.toml").is_file())
        .unwrap_or(false);
    if cwd_config_exists || global_config_exists {
        return true;
    }

    // A remote (non-localhost) endpoint cannot be calibrated by scanning local
    // ports — skip so we never overwrite a remote config (e.g. OpenRouter).
    if !endpoint_is_local(&config.endpoint) {
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

/// Whether an endpoint URL points at the local machine (calibration only makes
/// sense for a local backend it can port-scan / auto-start).
fn endpoint_is_local(endpoint: &str) -> bool {
    // URL-parsed (the loader's predicate), never a substring match.
    crate::config::is_local_endpoint(endpoint)
}

/// Reject global entry modes that would otherwise swallow subcommands and
/// silently run a different agent surface.
fn artifact_harness_flag_conflict(cli: &Cli) -> Option<&'static str> {
    if matches!(cli.command, Some(Commands::ArtifactHarness { .. })) {
        if cli.prompt.is_some() {
            return Some(
                "-p/--prompt cannot be combined with `artifact-harness`; pass the task as the subcommand's TASK argument",
            );
        }
        if cli.tui {
            return Some(
                "--tui cannot be combined with `artifact-harness`; the artifact harness is a bounded headless command",
            );
        }
        return None;
    }
    if cli.prompt.is_some()
        && cli.command.is_some()
        && !matches!(cli.command, Some(Commands::MultiChat { .. }))
    {
        return Some(
            "-p/--prompt cannot be combined with a subcommand; pass either `-p <PROMPT>` or a subcommand",
        );
    }
    None
}

/// Whether the `-C/--workdir` "Entering garden" banner may be printed.
///
/// Suppressed in quiet mode and whenever a machine-readable output format is
/// selected: with `--output-format json`/`stream-json`, a decorative banner on
/// stdout precedes the JSON object and breaks `json.load(stdout)`.
fn should_print_garden_banner(quiet: bool, output_format: HeadlessOutputFormat) -> bool {
    if quiet {
        return false;
    }
    !matches!(
        output_format,
        HeadlessOutputFormat::Json | HeadlessOutputFormat::StreamJson
    )
}

/// Merge the headless limit flags onto the loaded config.
///
/// A flag only wins when it was actually passed: clap leaves the `Option` at
/// `None` when the flag is absent, and writing that `None` over the config
/// would silently erase budgets set in `selfware.toml` (a TOML-only
/// `max_wall_secs = 60` never applied until this guard was added).
fn apply_cli_limit_overrides(cli: &Cli, config: &mut Config) {
    if let Some(max_turns) = cli.max_turns {
        config.agent.max_iterations = max_turns;
    }
    if let Some(max_budget_tokens) = cli.max_budget_tokens {
        config.agent.max_budget_tokens = Some(max_budget_tokens);
    }
    if let Some(max_wall_secs) = cli.max_wall_secs {
        config.agent.max_wall_secs = Some(max_wall_secs);
    }
    if let Some(max_cost_usd) = cli.max_cost_usd {
        config.agent.max_cost_usd = Some(max_cost_usd);
    }
}

/// Why the interactive TUI cannot launch, or `None` when it can.
///
/// The default no-subcommand launch opens the TUI dashboard, which needs a
/// real terminal on both stdin (keys) and stdout (raw-mode drawing). On a
/// pipe/CI run the previous behavior was to silently exit 0 having done
/// nothing — indistinguishable from success in automation.
// Only the TUI launch path calls this; the unit tests exercise it in every
// feature configuration.
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
fn tui_launch_block_reason(stdin_is_tty: bool, stdout_is_tty: bool) -> Option<&'static str> {
    if stdin_is_tty && stdout_is_tty {
        None
    } else {
        Some("interactive TUI requires a terminal; use -p/--prompt for headless")
    }
}

fn apply_session_model_overrides(cli: &Cli, config: &mut Config) -> Result<()> {
    // ── Apply named configuration profile (if requested) ──
    // `--profile architect|swarm-8|batch-16|batch-32|visual|quick` applies
    // built-in overrides for max_tokens / temperature from `ProfileManager`.
    if let Some(ref profile_name) = cli.profile {
        let pm = crate::profiles::ProfileManager::new();
        match pm.apply_profile(config, profile_name) {
            Ok(()) => {
                config.sources.set(
                    "max_tokens",
                    crate::config::ConfigSource::CliArg("--profile".into()),
                );
                config.sources.set(
                    "temperature",
                    crate::config::ConfigSource::CliArg("--profile".into()),
                );
                config.sync_default_model_profile();
                tracing::info!(
                    "Applied configuration profile '{}' (max_tokens={}, \
                     temperature={})",
                    profile_name,
                    config.max_tokens,
                    config.temperature
                );
            }
            Err(e) => {
                anyhow::bail!(
                    "Unknown --profile '{}': {}. Available profiles: architect, swarm-8, batch-16, batch-32, visual, quick.",
                    profile_name, e
                );
            }
        }
    }

    // ── Global --model override (Claude Code / Codex / Gemini parity) ──
    // Overrides the config `model` key for this session. Long-only on
    // purpose: `-m` stays --mode for existing scripts.
    if let Some(ref model) = cli.model {
        let model = model.trim();
        anyhow::ensure!(
            !model.is_empty(),
            "--model must not be empty — it overrides the `model` key in the configuration"
        );
        config.apply_cli_model_override(model.to_string());
    }

    if cli.profile.is_some() || cli.model.is_some() {
        config.validate()?;
    }

    Ok(())
}

/// Decide whether `--autocontinue` should run at startup, given which
/// explicit task/resume arguments are present.
///
/// Auto-resume is an implicit fallback (resume whatever long task was left
/// unfinished after a crash/restart) and must never override explicit intent:
/// a subcommand or `-p` prompt names a concrete task, while `--continue` and
/// `--resume-session` name a concrete session to resume. Returns `false`
/// (suppress auto-resume) when any of them is present, `true` otherwise.
pub(crate) fn autocontinue_should_run(
    has_subcommand: bool,
    has_prompt: bool,
    has_continue_flag: bool,
    has_resume_session: bool,
) -> bool {
    !(has_subcommand || has_prompt || has_continue_flag || has_resume_session)
}

/// Progress emitter for a RESUMED run (`--continue`, `--autocontinue`,
/// `selfware resume`), mirroring the fresh-headless wiring:
/// `stream-json` gets the JSONL emitter on stdout, plain text mode gets the
/// stderr emitter, and quiet / single-object `--output-format json` get none
/// so machine-readable stdout stays clean. A resumed run's execution loop
/// emits the same [`crate::agent::progress::ProgressEvent`]s as a fresh one —
/// without an attached emitter they fired into the no-op default and the
/// resume looked silent (2026-09-22 long-horizon finding).
fn resume_progress_emitter(
    quiet: bool,
    verbose: bool,
    output_format: HeadlessOutputFormat,
) -> Option<ResumeEmitters> {
    use crate::agent::progress::ProgressEmitter;
    let emitter: std::sync::Arc<dyn ProgressEmitter> = match output_format {
        HeadlessOutputFormat::StreamJson => {
            let jsonl = std::sync::Arc::new(headless::JsonlProgressEmitter::new());
            return Some((jsonl.clone(), Some(jsonl)));
        }
        // Structured `kind=…` lines are diagnostics: `--verbose` only. By
        // default the spinner (with the waiting status), the run summary and
        // the failure banner carry what a user needs; the raw lines split
        // the streamed answer mid-line (UX field test, 0.9.0).
        HeadlessOutputFormat::Text if !quiet && verbose => {
            std::sync::Arc::new(crate::agent::progress::StderrProgressEmitter::new())
        }
        _ => return None,
    };
    Some((emitter, None))
}

/// The progress emitter for a resumed run, plus the stream-json writer when
/// that is what it is (its assistant-text half is wired separately).
type ResumeEmitters = (
    std::sync::Arc<dyn crate::agent::progress::ProgressEmitter>,
    Option<std::sync::Arc<headless::JsonlProgressEmitter>>,
);

/// The event emitter for a headless run: the answer capture, fronted in
/// stream-json mode by the writer's assistant-text half so the answer
/// streams live as `text_delta` lines (0.9.1 field finding: IDE clients saw
/// only lifecycle events and no answer until the final result object).
fn answer_event_emitter(
    capture: &headless::AnswerCapture,
    jsonl: Option<&headless::JsonlProgressEmitter>,
) -> std::sync::Arc<dyn crate::agent::tui_events::EventEmitter> {
    let inner = std::sync::Arc::new(capture.emitter());
    match jsonl {
        Some(jsonl) => std::sync::Arc::new(jsonl.text_emitter(inner)),
        None => inner,
    }
}

/// Emit the final structured result object (`--output-format json` /
/// `stream-json`) for an agent run. The ONE emitter shared by fresh runs
/// (`-p`, `run`) and resumed runs (`resume <id>`, `--continue`,
/// `--autocontinue`), so every path reports the same keys, an `exit_status`
/// equal to the process exit code, and `grounding` when present. Returns
/// the emitted line.
fn emit_structured_result(
    agent: &Agent,
    run_result: &Result<()>,
    duration_ms: u64,
    answer: Option<String>,
    baseline: &headless::PatchBaseline,
) -> Option<String> {
    let result = build_session_result(agent, run_result, duration_ms, answer, baseline);
    headless::emit_result(&result, agent.grounding_status().as_ref())
}

/// Continue a resumed agent to completion and report it like a fresh run:
/// the progress emitter, the answer capture, the human summary (text
/// formats) and the final structured result (json / stream-json). Shared by
/// `resume <id>`, `--continue` and `--autocontinue`.
///
/// 0.8.2 validation D8: `--output-format stream-json resume <id>` ended with
/// `task_failed` and no result object (no exit_status, no grounding).
async fn run_resumed_agent(
    mut agent: Agent,
    quiet: bool,
    output_format: HeadlessOutputFormat,
) -> Result<()> {
    let start = std::time::Instant::now();
    let mut jsonl = None;
    if let Some((emitter, stream)) =
        resume_progress_emitter(quiet, crate::output::is_verbose(), output_format)
    {
        agent = agent.with_progress_emitter(emitter);
        jsonl = stream;
    }
    let answer_capture = headless::AnswerCapture::new();
    agent = agent.with_event_emitter(answer_event_emitter(&answer_capture, jsonl.as_deref()));
    let baseline = patch_baseline_for(output_format);
    let run_result = agent.continue_execution().await;
    if let Some(jsonl) = &jsonl {
        jsonl.flush_text();
    }
    finish_resumed_run(
        &agent,
        &run_result,
        start.elapsed().as_millis() as u64,
        answer_capture.take(),
        quiet,
        output_format,
        &baseline,
    );
    run_result
}

/// Report a finished resumed run: the human summary for text formats, the
/// shared structured result for json / stream-json. Returns the structured
/// line when one was emitted.
fn finish_resumed_run(
    agent: &Agent,
    run_result: &Result<()>,
    duration_ms: u64,
    answer: Option<String>,
    quiet: bool,
    output_format: HeadlessOutputFormat,
    baseline: &headless::PatchBaseline,
) -> Option<String> {
    print_resume_run_summary(agent, run_result, quiet, output_format);
    if is_structured_format(output_format) {
        emit_structured_result(agent, run_result, duration_ms, answer, baseline)
    } else {
        None
    }
}

/// The task-start working-tree snapshot the structured result's patch is
/// measured against. Only json / stream-json report a patch, so text runs
/// skip the git work.
fn patch_baseline_for(output_format: HeadlessOutputFormat) -> headless::PatchBaseline {
    if is_structured_format(output_format) {
        headless::PatchBaseline::capture()
    } else {
        headless::PatchBaseline::none()
    }
}

fn is_structured_format(output_format: HeadlessOutputFormat) -> bool {
    matches!(
        output_format,
        HeadlessOutputFormat::Json | HeadlessOutputFormat::StreamJson
    )
}

/// Print the end-of-run summary for a resumed run — the same block a fresh
/// headless run prints. The iteration figure is chain-wide
/// (`chain_run_summary`): token/cost totals, the budget caps, and the
/// files-changed list already accumulate across segments via the checkpoint
/// restore, and the per-segment loop counter alone would under-report a
/// resumed task's work. Suppressed in quiet mode and in machine-readable
/// output formats, exactly like the fresh-run summary.
fn print_resume_run_summary(
    agent: &Agent,
    run_result: &Result<()>,
    quiet: bool,
    output_format: HeadlessOutputFormat,
) {
    if quiet
        || matches!(
            output_format,
            HeadlessOutputFormat::Json | HeadlessOutputFormat::StreamJson
        )
    {
        return;
    }
    println!(
        "{}",
        crate::output::hyperlink::linkify_for_terminal(&render_text_run_summary(
            &agent.chain_run_summary(),
            run_result
        ))
    );
    if let Err(e) = run_result {
        report_text_run_error(agent, e);
    }
}

/// Canonical, absolute identity of the current workspace, used by
/// `--autocontinue` to match a checkpoint's recorded `project_root`.
///
/// Mirrors how `TaskCheckpoint::new` records the identity (both canonicalize),
/// so two processes in the same physical directory — even reached via a
/// symlink — produce the same string, and a different directory always
/// produces a different one. A checkpoint from another workspace is never
/// auto-resumed.
fn current_workspace_identity() -> Result<String> {
    let cwd = std::env::current_dir()?;
    let canonical = cwd.canonicalize().unwrap_or(cwd);
    Ok(canonical.to_string_lossy().into_owned())
}

/// Resume a named session for the CLI run paths, or BAIL instead of silently
/// continuing with a fresh EMPTY session.
///
/// `--resume-session` explicitly asks to continue a specific conversation.
/// Previously a typo'd (or purged) name printed "Failed to resume session" and
/// then ran the task/chat on an empty context — burning a full run on a
/// session that never existed. Named-chat resume now fails closed, mirroring
/// the task-resume path (missing checkpoint = hard error). When the session
/// exists and loads, behaviour is unchanged: the resume is announced (when
/// `announce`) and the run proceeds with the restored messages.
///
/// The session-not-found vs load-failure distinction is mirrored from the
/// chat store, which strings a missing file as "Chat '<name>' not found"
/// (`fs::read` context) and everything else (corrupt / undecryptable /
/// malformed JSON) as a load error. Both bail, with different diagnostics.
fn resume_named_session_or_bail(
    resume: impl FnOnce() -> anyhow::Result<usize>,
    session_name: &str,
    announce: bool,
) -> Result<()> {
    match resume() {
        Ok(msg_count) => {
            if announce {
                println!(
                    "▶ Resumed session '{}' ({} messages)",
                    session_name, msg_count
                );
            }
            Ok(())
        }
        Err(e) => {
            let detail = crate::observability::telemetry::redact_secrets(&e.to_string());
            if detail.contains("not found") {
                anyhow::bail!(
                    "cannot resume session '{}': it does not exist ({}) — \
                     refusing to start with an empty session",
                    session_name,
                    detail
                );
            }
            anyhow::bail!(
                "cannot resume session '{}': load failed ({})",
                session_name,
                detail
            );
        }
    }
}

pub async fn run() -> Result<()> {
    // Initialize telemetry
    init_tracing();

    let cli = Cli::parse();

    if let Some(reason) = artifact_harness_flag_conflict(&cli) {
        anyhow::bail!(reason);
    }

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

        // Machine-readable output (--output-format json/stream-json) must stay
        // valid: the garden banner would precede the JSON object on stdout and
        // break every json.load(stdout). Drop it like the other human headers.
        if should_print_garden_banner(cli.quiet, cli.output_format) {
            println!(
                "{} Entering garden: {}",
                Glyphs::sprout(),
                workdir.as_str().path_local()
            );
        }
    }

    // `selfware trust` records repo trust and must run BEFORE Config::load so it
    // is not blocked by the very credential-origin gate it manages.
    if let Some(Commands::Trust { path }) = &cli.command {
        let mut cfg_path = std::path::Path::new(path).to_path_buf();
        // Trust is per config FILE (config::trust matches exact canonical
        // paths only). A directory argument means that directory's
        // selfware.toml; a directory without one has nothing to trust, and
        // recording the bare directory would be a line that never matches.
        if cfg_path.is_dir() {
            let candidate = cfg_path.join("selfware.toml");
            if !candidate.is_file() {
                anyhow::bail!(
                    "No selfware.toml in '{}' to trust. Create it first, then run \
                     `selfware trust {}`.",
                    path,
                    path
                );
            }
            cfg_path = candidate;
        }
        if !cfg_path.exists() {
            anyhow::bail!(
                "No config to trust at '{}'. Pass a path (`selfware trust <path>`) \
                 or create ./selfware.toml first.",
                path
            );
        }
        let canon = std::fs::canonicalize(&cfg_path).unwrap_or_else(|_| cfg_path.clone());
        crate::config::trust::add_trusted_config(&cfg_path)?;
        if !cli.quiet {
            println!("Trusted {}", canon.display());
            println!(
                "  This repo's selfware.toml may now run its hooks, MCP servers, \
                 and post-edit command, apply its permission/yolo/safety settings, \
                 and use its endpoint with your API key. Trust covers the file as it \
                 is now: after it changes (a pull, a branch switch, an edit), review \
                 it and run `selfware trust` again. Untrust by removing that line \
                 from ~/.selfware/trusted_repos."
            );
        }
        return Ok(());
    }

    // The lifecycle views only read the event log: no configuration needed,
    // so they work even when the configuration is invalid.
    match &cli.command {
        Some(Commands::Tasks { limit, tree }) => {
            let log = crate::lifecycle::EventLog::default_location();
            print!(
                "{}",
                crate::lifecycle::projection::tasks_command_output(&log, *limit, *tree)
            );
            return Ok(());
        }
        Some(Commands::Agents { limit }) => {
            let log = crate::lifecycle::EventLog::default_location();
            let unreleased: Vec<(String, Option<String>)> =
                crate::resources::ResourceRegistry::global()
                    .snapshot()
                    .resources
                    .into_iter()
                    .filter(|r| !r.state.is_released())
                    .map(|r| (r.owner_task, r.owner_agent))
                    .collect();
            let alive = |pid: u32| {
                crate::resources::driver::probe_pid(pid, None) != crate::resources::Probe::Gone
            };
            print!(
                "{}",
                crate::lifecycle::projection::agents_command_output(
                    &log,
                    &unreleased,
                    &alive,
                    *limit
                )
            );
            return Ok(());
        }
        Some(Commands::Task { command }) => {
            let log = crate::lifecycle::EventLog::default_location();
            let alive = |pid: u32| crate::supervision::run_registry::pid_alive(pid);
            let verb = match command {
                args::TaskCommands::Show { id } => {
                    let description_of = |task_id: &str| {
                        Agent::task_status(task_id)
                            .ok()
                            .map(|c| (c.task_description, c.original_task_description))
                    };
                    return match crate::lifecycle::projection::task_show_output_with(
                        &log,
                        id,
                        &description_of,
                    ) {
                        Ok(out) => {
                            print!("{out}");
                            Ok(())
                        }
                        Err(msg) => Err(anyhow::anyhow!(msg)),
                    };
                }
                args::TaskCommands::Edit { id, description } => {
                    let (records, _) = log.read_all();
                    let description_of = |task_id: &str| {
                        Agent::task_status(task_id).ok().map(|c| c.task_description)
                    };
                    let (task, original) =
                        task_cmd::plan_edit(&records, &log, id, &description_of, &alive)
                            .map_err(|m| anyhow::anyhow!(m))?;
                    let new_description = match description {
                        Some(d) => d.trim().to_string(),
                        None => task_cmd::edit_in_editor(original.as_deref().unwrap_or(""))?,
                    };
                    if new_description.is_empty() {
                        anyhow::bail!("empty description: edit cancelled, nothing recorded");
                    }
                    print!("{}", task_cmd::fork_instructions(&task, &new_description));
                    return Ok(());
                }
                args::TaskCommands::Pause { id } => (id, task_cmd::ControlVerb::Pause),
                args::TaskCommands::Resume { id } => (id, task_cmd::ControlVerb::Resume),
                args::TaskCommands::Cancel { id } => (id, task_cmd::ControlVerb::Cancel),
            };
            let (records, _) = log.read_all();
            return match task_cmd::control_output(&records, &log, verb.0, verb.1, &alive) {
                Ok(out) => {
                    print!("{out}");
                    Ok(())
                }
                Err(msg) => Err(anyhow::anyhow!(msg)),
            };
        }
        _ => {}
    }

    // Emergency killswitch management must run BEFORE Config::load so an operator
    // can check, trip, or reset the killswitch even when configuration is invalid.
    if let Some(Commands::Killswitch { command }) = &cli.command {
        use args::KillswitchCommands;
        match command {
            KillswitchCommands::Status => {
                let status = crate::safety::killswitch::get_killswitch_status(None);
                if status.is_active {
                    if let Some(err) = status.error {
                        println!("🛑 Killswitch is ACTIVE: {err}");
                    } else {
                        println!("🛑 Killswitch is ACTIVE");
                    }
                } else {
                    println!("✅ Killswitch is INACTIVE");
                }
            }
            KillswitchCommands::Trip { reason, global } => {
                let target_root = if *global {
                    dirs::home_dir()
                        .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?
                } else {
                    std::env::current_dir()?
                };
                let outcome =
                    crate::safety::killswitch::trip_file_killswitch(&target_root, reason)?;
                match outcome {
                    crate::safety::killswitch::TripFileOutcome::Written { path } => {
                        println!("🛑 Killswitch TRIPPED at {}: {reason}", path.display());
                    }
                    crate::safety::killswitch::TripFileOutcome::PreservedExisting {
                        path,
                        description,
                    } => {
                        println!(
                            "🛑 Killswitch already ACTIVE via existing sentinel at {} ({}); operator reason not written to disk: {reason}",
                            path.display(),
                            description
                        );
                    }
                }
            }
            KillswitchCommands::Reset { global } => {
                let target_root = if *global {
                    dirs::home_dir()
                        .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?
                } else {
                    std::env::current_dir()?
                };
                let ks_path = target_root
                    .join(".selfware")
                    .join(crate::safety::killswitch::KILLSWITCH_FILE_NAME);
                let removed = crate::safety::killswitch::remove_file_killswitch(&target_root)?;
                if removed {
                    println!("Killswitch file removed: {}", ks_path.display());
                } else {
                    println!("Killswitch file not present at {}", ks_path.display());
                }

                // Re-verify actual system status to report honestly if another gate (e.g. global file or env var) remains active
                let check_root = if *global {
                    None
                } else {
                    Some(target_root.as_path())
                };
                match crate::safety::killswitch::check_killswitch(check_root) {
                    Ok(()) => {
                        println!("✅ Killswitch verified clear: system is operational.");
                    }
                    Err(e) => {
                        println!("⚠️  Note: System remains halted by killswitch: {e}");
                    }
                }
            }
        }
        return Ok(());
    }

    // `selfware resources` is an operator tool like the killswitch: it must
    // work without (or with a broken) configuration.
    if let Some(Commands::Resources {
        command,
        zombies,
        json,
        all,
    }) = &cli.command
    {
        return run_resources_command(command.as_ref(), *zombies, *json, *all).await;
    }

    // Recovery must remain reachable even when the selected configuration
    // cannot be parsed or trusted. Load it only for the diagnostic branch.
    if let Some(Commands::Boot { chat, check }) = &cli.command {
        if *check {
            let loaded = match load_async_config_with_provenance(&cli, &config_path).await {
                Ok(mut config) => apply_session_model_overrides(&cli, &mut config).map(|_| config),
                Err(e) => Err(e),
            };
            let config_error = loaded.as_ref().err().map(|error| format!("{error:#}"));
            let current = loaded
                .as_ref()
                .map_err(|_| config_error.as_deref().unwrap());
            let (checks, ok) = crate::boot::check::run_boot_check_with_config(current).await;
            println!();
            for check in checks {
                println!("  {}", check.render());
            }
            if !ok {
                anyhow::bail!("boot --check: one or more checks FAILED");
            }
        } else if *chat {
            crate::boot::chat::run_boot_chat().await?;
        } else {
            crate::boot::wizard::run_boot_wizard_for_path(
                config_path.map(std::path::PathBuf::from),
            )
            .await?;
        }
        return Ok(());
    }

    // Diagnostic subcommands report the config file themselves (doctor's
    // "config file" row, llm-doctor's config sanity step): the loader's
    // startup `config:` line printed it a second (doctor: third) time.
    if matches!(
        cli.command,
        Some(Commands::Doctor { .. } | Commands::LlmDoctor)
    ) {
        crate::config::set_config_line_suppressed(true);
    }
    let mut config = load_async_config_with_provenance(&cli, &config_path).await?;
    crate::resources::teardown::configure_session_deadline(std::time::Duration::from_secs(
        config.resources.teardown_deadline_secs,
    ));

    // Report (never reap) resources earlier sessions left behind: one line
    // on stderr, registry only — no container runtime calls on startup.
    if !cli.quiet {
        if let Some(line) = crate::resources::reaper::startup_report(
            crate::resources::ResourceRegistry::global(),
            &crate::resources::SystemDriver::default(),
        ) {
            eprintln!("{line}");
        }
    }

    apply_session_model_overrides(&cli, &mut config)?;
    config.discover_sglang_capabilities().await;

    // Tell the tokenizer which model is configured so it can pick a matching
    // HF tokenizer. It must NOT reload the config itself: a mid-session
    // `Config::load(None)` ignores `-c` and prints a spurious
    // `config: <path>` line into the session output (P2-11).
    crate::token_count::set_configured_model(&config.model);

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
    apply_cli_limit_overrides(&cli, &mut config);

    // ── Validate config and exit if requested ──
    if cli.validate_config {
        match config.validate_async().await {
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
    // at all. `_resource_monitor_shutdown_tx` is held until `run()` returns;
    // dropping it then IS the shutdown signal — `monitor_loop` treats a
    // dropped sender (not only an explicit `true`) as stop, so the monitor
    // task exits promptly instead of busy-spinning /proc scans on every
    // command exit.
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
    // Streaming responses are printed live, so final_answer must not re-print
    // the same content afterward (prevents the double-printed answer).
    output::set_streaming_mode(config.agent.streaming);
    // A structured output format applies to ALL commands (e.g.
    // `runs list --output-format json`), not only the headless prompt path, so
    // machine-readable output isn't polluted by human tables/prose.
    if matches!(
        cli.output_format,
        HeadlessOutputFormat::Json | HeadlessOutputFormat::StreamJson
    ) {
        output::set_json_mode(true);
    }

    let ctx = WorkshopContext::from_config(&config.endpoint, &config.model).with_mode(exec_mode);

    // `--coordinator` is global (so it can follow the subcommand) but only
    // means something to `multi-chat`; say so instead of silently ignoring it.
    if cli.coordinator && !matches!(cli.command, Some(Commands::MultiChat { .. })) {
        eprintln!("note: --coordinator only affects `multi-chat`; ignoring it for this command");
    }
    // `--max-turns` bounds the single-agent ReAct loop; multi-chat agents
    // each make exactly one completion, so the flag has no meaning there
    // (P0-3). Don't hard-fail — just say it.
    if cli.max_turns.is_some() && matches!(cli.command, Some(Commands::MultiChat { .. })) {
        eprintln!(
            "note: --max-turns has no effect in multi-chat mode \
             (each agent makes a single completion)"
        );
    }
    if cli.max_turns.is_some() && matches!(cli.command, Some(Commands::ArtifactHarness { .. })) {
        eprintln!(
            "note: --max-turns has no effect in artifact-harness mode; use --rounds to bound analysis and typed message reactivation"
        );
    }

    // --continue (claude -c parity, long-only so -c stays --config): resume
    // the MOST RECENT journal entry via the exact `resume <task-id>` path.
    if cli.continue_flag {
        if cli.command.is_some() || cli.prompt.is_some() {
            anyhow::bail!(
                "--continue cannot be combined with a subcommand or -p (it resumes the latest session)"
            );
        }
        // Only entries that carry a real user task are eligible: session-exit
        // placeholders ("interactive session exit" from earlier builds) and
        // step-0 saves with no user message have no instruction, and resuming
        // one ran an instruction-less agent turn that mutated files.
        let manager = anyhow::Context::context(
            crate::checkpoint::CheckpointManager::default_path(),
            "Failed to initialize checkpoint manager",
        )?;
        let selection = manager.latest_continuable_task()?;
        if !selection.skipped.is_empty() && !cli.quiet {
            eprintln!(
                "--continue: skipping {} newer journal entr{} with no user task to resume:",
                selection.skipped.len(),
                if selection.skipped.len() == 1 {
                    "y"
                } else {
                    "ies"
                }
            );
            for (summary, reason) in &selection.skipped {
                eprintln!("  {} ({})", summary.task_id, reason.describe());
            }
        }
        let Some(latest) = selection.selected.as_ref() else {
            if selection.skipped.is_empty() {
                anyhow::bail!("--continue: no sessions to continue (journal is empty)");
            }
            anyhow::bail!(
                "--continue: no session with a user task to continue ({} placeholder/empty \
                 entr{} skipped). Start a new task, or `selfware resume <task-id>` to \
                 resume a specific entry explicitly.",
                selection.skipped.len(),
                if selection.skipped.len() == 1 {
                    "y"
                } else {
                    "ies"
                }
            );
        };
        let is_structured = is_structured_format(cli.output_format);
        if is_structured {
            output::set_json_mode(true);
        }
        if !cli.quiet && !is_structured {
            println!(
                "{} Resuming latest session: {}{}",
                Glyphs::bookmark(),
                journal_title(&latest.task_description, 60),
                latest.edited_suffix()
            );
        }
        let agent = Agent::resume(config, &latest.task_id).await?;
        return run_resumed_agent(agent, cli.quiet, cli.output_format).await;
    }

    // --autocontinue: resume the most recent unfinished task of THIS workspace
    // at startup so a long task interrupted mid-run (crash/restart/machine
    // hiccup) survives via its checkpoint, using the exact same resume+continue
    // path as `selfware resume <id>`. Implicit and conservative on three axes:
    // an explicit subcommand, -p prompt, --continue, or --resume-session
    // argument always wins and suppresses the auto-resume; only checkpoints
    // recorded for the CURRENT working directory are eligible (a task from
    // another repository must never be pulled into this one); and eligibility
    // is InProgress checkpoints plus Failed checkpoints whose terminal stop is
    // the iteration-cap family (see terminal_stop_allows_autochain) — Paused
    // and every other failure class stay explicit-`resume` territory.
    // Nothing eligible → normal startup (announced).
    if cli.autocontinue {
        if !autocontinue_should_run(
            cli.command.is_some(),
            cli.prompt.is_some(),
            cli.continue_flag,
            cli.resume_session.is_some(),
        ) {
            if !cli.quiet {
                output::chrome_line(&format!(
                    "{} --autocontinue ignored — explicit task/resume argument given",
                    Glyphs::sprout()
                ));
            }
        } else {
            let workspace = current_workspace_identity()?;
            let checkpoint_manager = crate::checkpoint::CheckpointManager::default_path()?;
            match checkpoint_manager.latest_autoresumable_task(&workspace)? {
                Some(latest) => {
                    let is_structured = is_structured_format(cli.output_format);
                    if is_structured {
                        output::set_json_mode(true);
                    }
                    if !cli.quiet && !is_structured {
                        println!(
                            "{} Auto-resuming task {} from checkpoint — {}{}",
                            Glyphs::bookmark(),
                            latest.task_id.as_str().emphasis(),
                            journal_title(&latest.task_description, 60),
                            latest.edited_suffix()
                        );
                    }
                    tracing::info!(
                        task_id = %latest.task_id,
                        workspace = %workspace,
                        "auto-resuming in-progress task from this workspace"
                    );
                    let agent = Agent::resume(config, &latest.task_id).await?;
                    return run_resumed_agent(agent, cli.quiet, cli.output_format).await;
                }
                None => {
                    tracing::info!(
                        workspace = %workspace,
                        "no in-progress task from this workspace found; starting normally"
                    );
                    if !cli.quiet {
                        output::chrome_line(&format!(
                            "{} --autocontinue: no in-progress task in this workspace to resume — starting normally",
                            Glyphs::sprout()
                        ));
                    }
                }
            }
        }
    }

    // Headless mode: run prompt directly and exit (like qwen -p)
    if let Some(prompt) = cli.prompt {
        use std::io::IsTerminal;

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

        // `-p` combined with `multi-chat` is a multi-agent one-shot fan-out,
        // not the single-agent headless path below — previously this silently
        // degraded to single-agent, ignoring both multi-chat and
        // --coordinator (P1-4). Route it before the interactive-confirmation
        // guard: multi-chat agents make no tool calls, so there is nothing
        // to confirm even without a terminal on stdin.
        if let Some(Commands::MultiChat { concurrency, task }) = &cli.command {
            if let Some(positional) = task {
                anyhow::bail!(
                    "task supplied twice: via -p and as the `multi-chat` positional \
                     argument ('{}'); pass it once",
                    positional
                );
            }
            return run_multi_chat_one_shot(
                &config,
                &ctx,
                *concurrency,
                cli.coordinator,
                &actual_prompt,
                cli.output_format,
                cli.quiet,
            )
            .await;
        }

        // Headless Normal mode runs read-only tools; the first call that
        // needs a confirmation nobody can give stops the run with the typed
        // ConfirmationRequired error before it executes (see
        // `headless::headless_normal_mode_notice`).
        if let Some(notice) =
            headless::headless_normal_mode_notice(exec_mode, std::io::stdin().is_terminal())
        {
            if !cli.quiet {
                eprintln!("{notice}");
            }
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
        let mut agent = Agent::new(config)
            .await?
            .with_confirmation_timeout(ONE_SHOT_CONFIRM_TIMEOUT);
        // Resume named session if --resume-session was provided (headless path)
        if let Some(ref session_name) = cli.resume_session {
            resume_named_session_or_bail(
                || agent.resume_named_session(session_name),
                session_name,
                !cli.quiet && !is_structured,
            )?;
        }
        let mut emitters: Vec<std::sync::Arc<dyn crate::agent::progress::ProgressEmitter>> =
            Vec::new();
        let jsonl =
            is_stream_json.then(|| std::sync::Arc::new(headless::JsonlProgressEmitter::new()));
        if let Some(jsonl) = &jsonl {
            emitters.push(jsonl.clone());
        } else if !cli.quiet && !is_structured && crate::output::is_verbose() {
            // Structured event lines are `--verbose` diagnostics (see
            // `resume_progress_emitter`).
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
        // Capture the agent's final answer for the structured result. The
        // answer text is only reachable through the terminal `Completed`
        // event (the public conduit to `last_assistant_response`), so the
        // capture emitter is attached before the run starts.
        let answer_capture = headless::AnswerCapture::new();
        agent = agent.with_event_emitter(answer_event_emitter(&answer_capture, jsonl.as_deref()));
        let baseline = patch_baseline_for(cli.output_format);
        let run_result = agent.run_task(&actual_prompt).await;
        let duration_ms = start.elapsed().as_millis() as u64;
        if let Some(jsonl) = &jsonl {
            jsonl.flush_text();
        }

        if !cli.quiet && !is_structured {
            println!(
                "{}",
                crate::output::hyperlink::linkify_for_terminal(&render_text_run_summary(
                    &agent.run_summary(),
                    &run_result
                ))
            );
            // A wall-clock TIMEOUT without a final answer: show what was
            // gathered, labelled PARTIAL — the run still fails below.
            if let Some(partial) = agent.partial_progress(&run_result) {
                println!("{}", partial.render());
            }
        }

        if is_structured {
            emit_structured_result(
                &agent,
                &run_result,
                duration_ms,
                answer_capture.take(),
                &baseline,
            );
        } else if !cli.quiet
            && run_result.is_ok()
            && earns_task_complete_banner(&agent)
            && agent
                .grounding_status()
                .is_none_or(|g| g.warning_note().is_none())
        {
            println!("{}", render_task_complete(start.elapsed()));
        }
        if let Err(e) = &run_result {
            if !cli.quiet && !is_structured {
                report_text_run_error(&agent, e);
            }
        }
        return run_result;
    }

    // Handle TUI dashboard mode
    #[cfg(feature = "tui")]
    {
        // `--tui multi-chat` opens the per-agent multi-chat TUI, not the
        // single-agent dashboard (which used to swallow the subcommand).
        if let Some(Commands::MultiChat { concurrency, task }) = &cli.command {
            if cli.tui && !cli.quiet {
                if let Some(reason) =
                    multi_chat_tui_block_reason(cli.coordinator, cli.output_format)
                {
                    eprintln!("note: {reason}; using the plain per-agent output instead");
                } else {
                    use std::io::IsTerminal;
                    if let Some(reason) = tui_launch_block_reason(
                        std::io::stdin().is_terminal(),
                        std::io::stdout().is_terminal(),
                    ) {
                        anyhow::bail!("{}", reason);
                    }
                    return run_multi_chat_tui(config, *concurrency, task.clone()).await;
                }
            }
        }
        let is_multi_chat = matches!(cli.command, Some(Commands::MultiChat { .. }));
        let should_use_tui =
            !cli.quiet && !is_multi_chat && (cli.tui || (cli.command.is_none() && !cli.no_tui));
        if should_use_tui {
            // Fail loudly on a non-terminal launch: the interactive TUI needs
            // a TTY (crossterm raw mode), and previously a CI/pipe/script run
            // silently exited 0 having done nothing — the worst possible CI
            // signal. Headless automation belongs on -p/--prompt.
            use std::io::IsTerminal;
            if let Some(reason) = tui_launch_block_reason(
                std::io::stdin().is_terminal(),
                std::io::stdout().is_terminal(),
            ) {
                anyhow::bail!("{}", reason);
            }
            return run_live_agent_tui(config).await;
        }
    }

    #[cfg(not(feature = "tui"))]
    if cli.tui {
        anyhow::bail!(
            "TUI dashboard requires the 'tui' feature (on by default; this build disabled it). \
             Rebuild with: cargo install selfware --features tui  or  cargo build --release --features tui"
        );
    }

    // Default to Chat if no subcommand specified (non-extras builds)
    let command = cli.command.unwrap_or(Commands::Chat);
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
    answer: Option<String>,
    baseline: &headless::PatchBaseline,
) -> headless::SessionResult {
    // The process exits with this same code (src/main.rs), so the structured
    // record never disagrees with `$?` (130 on interrupt, 143 on SIGTERM, ...).
    let exit_status = i32::from(crate::errors::process_exit_code(
        run_result,
        crate::shutdown_reason(),
    ));
    let stop_reason = match run_result {
        Ok(()) => agent
            .last_run_failure_mode()
            .map(|fm| fm.kind.tag().to_string())
            .unwrap_or_else(|| "completed".to_string()),
        Err(e) => format!(
            "error: {}",
            crate::observability::telemetry::redact_secrets(&e.to_string())
        ),
    };
    // One per `step_started` event (see `SessionResult::num_turns`). The
    // iteration counter used before skipped the planning turn's tool batch
    // and reset on auto-continue, so it disagreed with the event stream
    // (1 vs 3 step_started in a 0.9.1 field run).
    let num_turns = agent.turns_run();
    let outcome = crate::errors::RunEnd::classify(run_result, crate::shutdown_reason())
        .as_str()
        .to_string();
    // A capture failure must NOT masquerade as a clean empty patch (0 bytes),
    // which would fool automated success metrics. Surface it instead.
    // Measured from the task-start tree so pre-existing workspace edits are
    // not reported as this run's patch.
    let (patch, patch_capture_error) = match headless::capture_patch_since(baseline.tree()) {
        Ok(p) => (p, None),
        Err(e) => {
            eprintln!("warning: patch capture failed: {e}");
            (String::new(), Some(format!("patch_capture_failed: {e}")))
        }
    };
    let patch_bytes = patch.len();
    let patch_lines = patch.lines().count();
    let files_changed = headless::patch_file_count(&patch);
    let patch_baseline = if baseline.tree().is_some() {
        headless::PATCH_BASELINE_TASK_START
    } else {
        headless::PATCH_BASELINE_HEAD
    }
    .to_string();
    let usage = agent.cumulative_token_usage().clone();
    let model = agent.model().to_string();
    // Prefer the agent's own failure mode; otherwise report a patch-capture
    // failure so a git error is visible in the structured result.
    let failure_mode = agent
        .last_run_failure_mode()
        .map(|fm| format!("{}: {}", fm.kind.tag(), fm.evidence))
        .or(patch_capture_error);
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
        outcome,
        files_changed,
        patch_baseline,
        answer,
        requirements_audit: agent.requirements_audit_status().map(|a| a.label()),
        partial: agent.partial_progress(run_result),
        resources: agent.resource_teardown.clone(),
        task_edited: agent.edited_task_description(),
        review_coverage: agent.review_coverage(),
        done_check: agent.done_check_report(),
    }
}

/// Launch the live agent-driven TUI dashboard.
///
/// This is the single entry point used by the default no-subcommand path and
/// `selfware dashboard`. It builds an
/// `Agent` with an event sender, spawns the TUI in a blocking thread,
/// processes user input, and cleans up on exit.
/// Why `--tui multi-chat` must fall back to plain output, or `None` when the
/// per-agent TUI can run.
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
fn multi_chat_tui_block_reason(
    coordinator: bool,
    output_format: HeadlessOutputFormat,
) -> Option<&'static str> {
    if coordinator {
        Some("the multi-chat TUI does not support --coordinator")
    } else if output_format != HeadlessOutputFormat::Text {
        Some("--output-format json/stream-json is headless; the multi-chat TUI is not used")
    } else {
        None
    }
}

/// `selfware --tui multi-chat [task]`: per-agent tabs plus an "All" overview
/// (see [`crate::ui::tui::multichat`]). The summary table of the last run is
/// printed after the TUI closes so it survives the alternate screen.
#[cfg(feature = "tui")]
async fn run_multi_chat_tui(
    config: Config,
    concurrency: usize,
    task: Option<String>,
) -> Result<()> {
    let agent_config = multiagent::MultiAgentConfig::default().with_concurrency(concurrency);
    let roles = agent_config.roles.clone();
    let chat = multiagent::MultiAgentChat::new(&config, agent_config)?;
    let results = crate::ui::tui::multichat::run_multichat_tui(chat, roles, task).await?;
    if !results.is_empty() {
        multiagent::print_agent_summary(&results);
    }
    Ok(())
}

#[cfg(feature = "tui")]
async fn run_live_agent_tui(config: Config) -> Result<()> {
    let (event_tx, event_rx) = mpsc::channel();
    let (user_input_tx, user_input_rx) = mpsc::channel();
    let (permission_tx, permission_rx) = mpsc::channel();

    // Keep a sender copy so the bridge can surface honest "unsupported
    // command" messages to the TUI log panel instead of dropping them.
    let bridge_event_tx = event_tx.clone();

    let mut agent = Agent::new(config.clone())
        .await?
        .with_event_sender(event_tx)
        .with_permission_channel(permission_rx);

    // Shared cancellation token: the TUI latches it on Esc to abort the
    // running task; the bridge resets it before each new task below.
    let cancel_token = agent.cancel_token();
    // Shared task control: the Tasks pane pauses, edits and cancels the
    // running task through it (acted on between steps).
    let task_control = agent.task_control();
    let tui_task_control = task_control.clone();

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
            cancel_token,
            tui_task_control,
        )
    });

    // Process user inputs from TUI.
    // The recv() is blocking (std::sync::mpsc), so we use block_in_place
    // to let tokio move other tasks off this thread while we wait.
    let skill_registry = crate::skills::SkillRegistry::discover();
    let _ = bridge_event_tx.send(crate::ui::tui::TuiEvent::Log {
        level: crate::ui::tui::LogLevel::Info,
        message: "type /help for commands · !cmd to run a shell command · @path to attach a file"
            .to_string(),
    });
    loop {
        let input = tokio::task::block_in_place(|| user_input_rx.recv());

        match input {
            Ok(ref input) if crate::input::command_registry::is_exit_command(input.trim()) => break,
            Ok(mut input) => {
                // Log helper for the slash handlers below (TUI log panel).
                let log_line = |message: String| {
                    let _ = bridge_event_tx.send(crate::ui::tui::TuiEvent::Log {
                        level: crate::ui::tui::LogLevel::Info,
                        message,
                    });
                };

                // Shell passthrough (gemini/aider `!cmd`): run the command,
                // show the output, and push it into the conversation as
                // context so the next task sees it.
                if let Some(cmd) = input.strip_prefix('!').map(str::trim) {
                    if cmd.is_empty() {
                        log_line("usage: !<command>".to_string());
                        continue;
                    }
                    let rendered = agent.shell_passthrough(cmd).await;
                    log_line(rendered);
                    continue;
                }

                // /clear resets the model conversation so the next task starts
                // fresh (the TUI already cleared the display).
                if input == "/clear" {
                    agent.clear_conversation();
                    continue;
                }
                // Plan-mode commands drive the real agent state via the same
                // public setters the interactive dispatcher uses; the
                // execution gate keys off the `plan_mode` bool.
                if input == "/plan" {
                    agent.set_plan_mode(true);
                    agent.enter_plan_mode();
                    continue;
                }
                if input == "/execute" {
                    agent.approve_plan();
                    agent.set_plan_mode(false);
                    continue;
                }
                if input == "/modify" {
                    agent.clear_plan();
                    agent.set_plan_mode(false);
                    continue;
                }
                // Session slash commands (harness-alignment round E). All of
                // them answer in the TUI log panel; the same handlers serve
                // the palette (its commands route through this loop).
                if input == "/help" {
                    for line in slash_help_text().lines() {
                        log_line(line.to_string());
                    }
                    continue;
                }
                if input == "/status" {
                    log_line(session_status_text(&agent));
                    continue;
                }
                if input == "/compact" {
                    let before = agent.message_count();
                    let target = agent.compaction_threshold();
                    agent.compress_to_structured_summary(target);
                    log_line(format!(
                        "/compact: context compression pass complete — {} → {} messages (target ~{} tokens)",
                        before,
                        agent.message_count(),
                        target
                    ));
                    continue;
                }
                if input == "/ctx" {
                    // The REPL's measured label: current conversation tokens
                    // of the model's window, plus the compaction threshold.
                    log_line(agent.context_usage_text());
                    continue;
                }
                if input == "/cost" {
                    // Session scope (every task), same fold as the REPL's
                    // /cost and /quit — the run summary is per task.
                    for line in agent
                        .session_usage()
                        .render_cost_lines(agent.session_main_loop_tokens())
                    {
                        log_line(line);
                    }
                    continue;
                }
                if input == "/model" {
                    log_line(model_status_text(&agent));
                    continue;
                }
                if let Some(name) = input.strip_prefix("/model ") {
                    let name = name.trim();
                    if name.is_empty() {
                        log_line(model_status_text(&agent));
                    } else {
                        match agent.switch_model(name) {
                            Ok(previous) => log_line(format!(
                                "/model: session model switched {previous} → {name} (config file unchanged)"
                            )),
                            Err(e) => log_line(format!("/model failed: {e}")),
                        }
                    }
                    continue;
                }
                if input == "/doctor" {
                    let report = crate::doctor::run_doctor(None).await;
                    let issues: Vec<&str> = report
                        .checks
                        .iter()
                        .filter(|check| !matches!(check.status, crate::doctor::CheckStatus::Ok))
                        .map(|check| check.name.as_str())
                        .collect();
                    if issues.is_empty() {
                        log_line(format!(
                            "/doctor: all {} checks passed",
                            report.checks.len()
                        ));
                    } else {
                        log_line(format!(
                            "/doctor: {} issue(s) — {}",
                            issues.len(),
                            issues.join(", ")
                        ));
                    }
                    continue;
                }
                if input == "/mcp" {
                    log_line(mcp_servers_text(agent.config()));
                    continue;
                }
                // ── Round E2 handlers (classic ↔ TUI parity + power features)
                if input == "/undo" {
                    log_line(agent.undo_last_edit().await);
                    continue;
                }
                if input == "/redo" {
                    log_line(agent.redo_last_edit().await);
                    continue;
                }
                if input == "/agents" {
                    log_line(agents_status_text());
                    continue;
                }
                if input == "/permissions" {
                    log_line(permissions_text(&agent));
                    continue;
                }
                if input == "/bug" {
                    log_line(bug_report_text(&agent));
                    continue;
                }
                if input == "/journal" {
                    log_line(journal_entries_text(10));
                    continue;
                }
                if input == "/memory" {
                    log_line(agent.memory_status_text());
                    continue;
                }
                if input == "/tools" {
                    log_line(tools_text(&agent));
                    continue;
                }
                if input == "/garden" {
                    match crate::ui::garden::build_garden_from_path(".") {
                        Ok(garden) => {
                            let rendered = garden.render();
                            for line in rendered.lines().take(24) {
                                log_line(line.to_string());
                            }
                        }
                        Err(e) => log_line(format!("/garden failed: {e}")),
                    }
                    continue;
                }
                if input == "/analyze" || input.starts_with("/analyze ") {
                    let path = input
                        .strip_prefix("/analyze")
                        .map(str::trim)
                        .filter(|p| !p.is_empty())
                        .unwrap_or(".");
                    if let Err(e) = agent.analyze(path).await {
                        log_line(format!("/analyze failed: {e}"));
                    }
                    continue;
                }
                if input == "/review" || input.starts_with("/review ") {
                    let path = input
                        .strip_prefix("/review")
                        .map(str::trim)
                        .filter(|p| !p.is_empty())
                        .unwrap_or(".");
                    if let Err(e) = agent.review(path).await {
                        log_line(format!("/review failed: {e}"));
                    }
                    continue;
                }
                if input == "/explain" || input.starts_with("/explain ") {
                    let args = input.strip_prefix("/explain").map(str::trim).unwrap_or("");
                    if args.is_empty() {
                        log_line("Usage: /explain <path> | /explain --level <level> | /explain --curriculum <dir>".to_string());
                    } else {
                        agent.handle_explain_command(args).await;
                    }
                    continue;
                }
                if input == "/scan" || input.starts_with("/scan ") {
                    let path_arg = input
                        .strip_prefix("/scan")
                        .map(str::trim)
                        .filter(|p| !p.is_empty())
                        .unwrap_or(".");
                    if path_arg == "help" {
                        log_line("Usage: /scan <path>".to_string());
                    } else {
                        agent.handle_scan_command(path_arg).await;
                    }
                    continue;
                }
                if input == "/resume" {
                    log_line(journal_entries_text(10));
                    log_line("resume one with: /resume <task-id-prefix>".to_string());
                    continue;
                }
                if let Some(prefix) = input.strip_prefix("/resume ").map(str::trim) {
                    let tasks = crate::agent::Agent::list_tasks().unwrap_or_default();
                    let entry = tasks.iter().find(|t| t.task_id.starts_with(prefix));
                    match entry {
                        Some(entry) => {
                            let task_id = entry.task_id.clone();
                            let title = format!(
                                "{}{}",
                                journal_title(&entry.task_description, 48),
                                entry.edited_suffix()
                            );
                            match Agent::resume(agent.config().clone(), &task_id).await {
                                Ok(resumed) => {
                                    let count = resumed.message_count();
                                    // Carry this session's wiring over (event
                                    // sender, permission channel, progress
                                    // emitter, Esc token, Tasks-pane control,
                                    // …): the same helper the REPL uses.
                                    agent.restore_resumed_state(resumed);
                                    log_line(format!(
                                        "resumed '{title}' ({count} messages) — continue chatting"
                                    ));
                                }
                                Err(e) => log_line(format!("/resume failed: {e}")),
                            }
                        }
                        None => log_line(format!("no journal entry matching '{prefix}'")),
                    }
                    continue;
                }
                if input == "/skills" {
                    if skill_registry.is_empty() {
                        log_line(
                            "no skills found in ~/.selfware/skills, ./.selfware/skills, or ./.selfware/commands"
                                .to_string(),
                        );
                    } else {
                        let mut out = format!("{} skill(s) available:", skill_registry.len());
                        for skill in skill_registry.list() {
                            let unverified_suffix =
                                if skill.verified { "" } else { " (UNVERIFIED)" };
                            out.push_str(&format!(
                                "\n  /{}{} — {}",
                                skill.name, unverified_suffix, skill.description
                            ));
                        }
                        log_line(out);
                    }
                    continue;
                }
                // Dynamic slash skill invocation or unsupported slash command.
                if let Some(cmd_rest) = input.strip_prefix('/') {
                    let mut parts = cmd_rest.splitn(2, ' ');
                    let skill_name = parts.next().unwrap_or("");
                    let arguments = parts.next().unwrap_or("").trim();
                    if let Some(skill) = skill_registry.get(skill_name) {
                        let unverified_suffix = if skill.verified { "" } else { " (UNVERIFIED)" };
                        log_line(format!("Activated skill: /{skill_name}{unverified_suffix}"));
                        input = skill.render_with_trust_gate(arguments);
                    } else {
                        warn!("Slash command '{}' not supported in TUI mode", input);
                        let _ = bridge_event_tx.send(crate::ui::tui::TuiEvent::Log {
                            level: crate::ui::tui::LogLevel::Warning,
                            message: format!(
                                "'{}' is not supported in TUI mode — use interactive CLI mode",
                                input
                            ),
                        });
                        continue;
                    }
                }
                // A stale latched cancel token (e.g. Esc pressed just as the
                // previous run finished) must not abort the new task.
                agent.reset_cancellation();
                // A fork queued from the Tasks pane runs as a new task whose
                // parent is the original.
                if let Some(parent) = task_control.take_fork_for(&input) {
                    agent.set_fork_parent(parent);
                }
                // @path attachments (claude/gemini parity): resolve `@file`
                // tokens into context blocks appended to the task.
                let attachments = expand_attachments(&input);
                let task = if attachments.is_empty() {
                    input
                } else {
                    log_line(format!(
                        "attached {} file(s): {}",
                        attachments.len(),
                        attachments
                            .iter()
                            .map(|a| a.path.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                    format!("{}{}", input, render_attachments(&attachments))
                };
                // Run the task — this will emit events to the TUI through event_tx
                let run_result = agent.run_task(&task).await;
                if let Err(e) = &run_result {
                    warn!("Agent failed to run task: {}", e);
                }
                // End-of-task outcome in the chat + logs, like the CLI's
                // ✅/⚠️ banner and run summary (the TUI showed none).
                let _ = bridge_event_tx.send(crate::ui::tui::TuiEvent::RunOutcome {
                    summary: tui_run_outcome_text(&agent, &run_result),
                });
            }
            _ => break,
        }
    }

    crate::output::set_tui_active(false);

    // Session end: drain what the session still owns, printed after the TUI
    // left the alternate screen so the line survives.
    agent.teardown_session_resources().await;

    // Session-exit summary (claude prints usage on quit): the Round A run
    // summary — outcome, iterations, files changed, verification, tokens.
    println!(
        "{}",
        crate::output::hyperlink::linkify_for_terminal(&render_run_summary(
            &agent.run_summary(),
            None
        ))
    );

    // Auto-save conversation/session on exit so history isn't lost — only
    // when the session carried a user task (a "TUI session exit" placeholder
    // entry would be picked up by `--continue` with nothing to do).
    if let Some(task_desc) = agent.session_exit_task_description() {
        if let Err(e) = agent.save_checkpoint(&task_desc) {
            warn!("Failed to auto-save session on TUI exit: {}", e);
        }
    }

    // Cleanup: await the TUI task with a bounded timeout so a stuck
    // TUI thread can never block shutdown indefinitely.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), tui_handle).await;
    Ok(())
}

/// The TUI's end-of-task note: the failure-mode banner line (✅ / ⚠️ /
/// failure tag — the same verdict the CLI prints) followed by the run
/// summary. The banner line is omitted when the run ended before a
/// verdict was classified (the summary's `outcome:` line still says so).
#[cfg(feature = "tui")]
fn tui_run_outcome_text(agent: &Agent, run_result: &Result<()>) -> String {
    let failure = run_result
        .as_ref()
        .err()
        .map(|e| crate::observability::telemetry::redact_secrets(&e.to_string()));
    let summary = render_run_summary(&agent.run_summary(), failure.as_deref());
    match agent
        .last_run_failure_mode()
        .and_then(|fm| agent.outcome_banner(fm))
        .and_then(|banner| banner.lines().next().map(str::to_string))
    {
        Some(banner) => format!("{banner}\n{summary}"),
        None => summary,
    }
}

/// URL the `status` command probes: `{endpoint}/models`, which every
/// OpenAI-compatible server (SGLang, vLLM, llama.cpp, OpenRouter) routes.
/// The bare `/v1` base has no route on SGLang, so probing it reported a
/// false "Endpoint not found (404)" for a healthy endpoint (UX field test,
/// 0.9.0). Same convention as llm-doctor's backend detection.
fn status_probe_url(endpoint: &str) -> String {
    format!("{}/models", endpoint.trim_end_matches('/'))
}

async fn probe_status_endpoint(config: &Config) -> (bool, String) {
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .connect_timeout(std::time::Duration::from_secs(2));
    if crate::config::api_key::is_local_endpoint(&config.endpoint) {
        builder = builder.no_proxy();
    }
    let client = match builder.build() {
        Ok(c) => c,
        Err(e) => return (false, format!("Endpoint unreachable: {}", e)),
    };
    let req = match crate::config::api_key::authorize_request(
        client.get(status_probe_url(&config.endpoint)),
        &config.endpoint,
        config.api_key.as_ref().map(|k| k.expose()),
    ) {
        Ok(req) => req,
        Err(e) => return (false, e.to_string()),
    };
    match req.send().await {
        Ok(resp) => {
            let code = resp.status();
            if code.is_success() || code == reqwest::StatusCode::METHOD_NOT_ALLOWED {
                (true, "Connected".to_string())
            } else if code == reqwest::StatusCode::UNAUTHORIZED
                || code == reqwest::StatusCode::FORBIDDEN
            {
                (false, format!("Authentication failed ({})", code))
            } else if code == reqwest::StatusCode::NOT_FOUND {
                (false, format!("Endpoint not found ({})", code))
            } else {
                (false, format!("Endpoint returned error ({})", code))
            }
        }
        Err(e) => (false, format!("Endpoint unreachable: {}", e)),
    }
}

/// How long a tool-confirmation prompt waits in a non-REPL run (`-p`, `run`,
/// `improve`, `batch`) before failing closed. Long enough for someone
/// watching to read the reason and answer; short enough that an unattended
/// run does not stall (UX field test: a `-y` run waited ~14 minutes on an
/// unexplained prompt until it was killed).
pub(crate) const ONE_SHOT_CONFIRM_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(120);

/// Run a single multi-agent fan-out over `task` and return — the headless
/// one-shot form of `multi-chat` (P1-4).
///
/// Reached via a positional task (`selfware multi-chat "task"`) or `-p`
/// combined with the subcommand (`selfware -p "task" multi-chat`). In
/// coordinator mode the swarm assignment gates execution exactly as in the
/// interactive swarm loop: a task the coordinator cannot assign to the full
/// role fleet makes no LLM calls and is reported as an error.
///
/// Output follows the global `--output-format`: `json`/`stream-json` emit a
/// machine-readable array of per-agent results (including provider-reported
/// usage); `text` prints the per-agent summary and the aggregated result.
///
/// Exit semantics: `Ok(())` when every agent succeeded; an error (process
/// exit 1) when any agent failed.
async fn run_multi_chat_one_shot(
    config: &Config,
    ctx: &WorkshopContext,
    concurrency: usize,
    coordinator: bool,
    task: &str,
    output_format: HeadlessOutputFormat,
    quiet: bool,
) -> Result<()> {
    let is_json = output_format == HeadlessOutputFormat::Json;
    let is_stream_json = output_format == HeadlessOutputFormat::StreamJson;
    let is_structured = is_json || is_stream_json;

    // In JSON/stream-JSON modes, suppress all human-oriented stdout so only
    // machine-readable JSON is emitted.
    if is_structured {
        output::set_json_mode(true);
    }

    if !quiet && !is_structured {
        println!("{}", render_header(ctx));
        println!(
            "\n{} {} with {} concurrent streams{}\n",
            Glyphs::gear(),
            "Multi-Agent One-Shot".workshop_title(),
            concurrency.to_string().emphasis(),
            if coordinator {
                " (coordinator mode)"
            } else {
                ""
            }
        );
        println!(
            "{} each agent runs a single completion with no tools — answers come from model \
             priors, not from this repository, and cannot cite files.",
            "note:".dimmed()
        );
    }

    let agent_config = multiagent::MultiAgentConfig::default().with_concurrency(concurrency);

    let start = std::time::Instant::now();
    let results = if coordinator {
        run_coordinated_fan_out(config, agent_config, task, quiet || is_structured).await?
    } else if quiet || is_structured {
        multiagent::run_multiagent_task(config, task, concurrency).await?
    } else {
        // Human text output: each agent's reply as `[tag] …` lines, never
        // interleaved mid-line with another agent's.
        let mut chat = multiagent::MultiAgentChat::new(config, agent_config)?;
        chat.run_task_tagged(task, "Summary:").await?
    };

    if is_structured {
        let json = serde_json::Value::Array(
            results
                .iter()
                .map(multi_agent_result_json)
                .collect::<Vec<_>>(),
        );
        // stream-json is line-delimited: the whole array goes on one line.
        if is_stream_json {
            println!("{}", serde_json::to_string(&json)?);
        } else {
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
    } else if !quiet {
        multiagent::print_agent_summary(&results);

        println!(
            "\n{} Total time: {:.2}s",
            "⏱".bright_yellow(),
            start.elapsed().as_secs_f64()
        );
    }

    let failed = results.iter().filter(|r| !r.success).count();
    if failed > 0 {
        anyhow::bail!(
            "multi-chat one-shot: {failed}/{} agents failed",
            results.len()
        );
    }
    Ok(())
}

/// Coordinator-mode fan-out for the one-shot path: the swarm assignment
/// gates execution, mirroring `MultiAgentChat::interactive_swarm`. The task
/// runs only when the coordinator assigns the full role fleet; otherwise no
/// LLM calls are made and this returns an error. Kept in the CLI layer
/// because the interactive helper is private to the orchestration module.
async fn run_coordinated_fan_out(
    config: &Config,
    agent_config: multiagent::MultiAgentConfig,
    task: &str,
    quiet: bool,
) -> Result<Vec<multiagent::AgentResult>> {
    use crate::swarm::{Agent as SwarmAgent, Swarm, SwarmTask, TaskStatus};

    let role_count = agent_config.roles.len();

    // Build the swarm 1:1 from the configured roles so the coordinator's
    // assignment is authoritative: a full assignment is exactly the set of
    // agents `run_task` will execute.
    let mut swarm = Swarm::new();
    let mut swarm_agent_ids = Vec::with_capacity(role_count);
    for (i, role) in agent_config.roles.iter().enumerate() {
        let id = swarm.add_agent(SwarmAgent::new(
            format!("Agent-{}-{}", i, role.name()),
            *role,
        ));
        swarm_agent_ids.push(id);
    }

    let mut swarm_task = SwarmTask::new(task);
    for role in &agent_config.roles {
        swarm_task = swarm_task.with_role(*role);
    }
    swarm.queue_task(swarm_task)?;
    let task_id = swarm
        .next_task()
        .ok_or_else(|| anyhow::anyhow!("coordinator: no task available from swarm queue"))?;

    // The coordinator's assignment GATES execution: running (and billing) a
    // different set of agents than the coordinator assigned is not allowed.
    let assigned = swarm.assign_task(&task_id);
    if assigned.len() < role_count {
        swarm.fail_task(&task_id);
        anyhow::bail!(
            "coordinator assigned only {}/{} role agents — task not run (no LLM calls made)",
            assigned.len(),
            role_count
        );
    }
    if !quiet {
        println!(
            "  {} Swarm coordinator assigned {} agents to task",
            "📋".bright_blue(),
            assigned.len()
        );
    }

    let mut multi_agent = multiagent::MultiAgentChat::new(config, agent_config)?;
    let results = if quiet {
        multi_agent.run_task(task).await?
    } else {
        multi_agent.run_task_tagged(task, "Summary:").await?
    };

    // Feed results back to the swarm with the ACTUAL per-agent success flag
    // so trust scores and failure counters reflect reality (`complete_task`
    // also returns each agent to Idle).
    for result in &results {
        if let Some(agent_id) = swarm_agent_ids.get(result.agent_id) {
            swarm.complete_task(&task_id, agent_id, result.content.clone(), result.success);
        }
    }
    // Settle the task: an assigned agent that never reported a result must
    // not leave the SwarmTask stuck InProgress forever. Tasks already settled
    // by `complete_task` (Completed, Partial, or Failed) keep their outcome.
    let task_unsettled = swarm
        .get_task(&task_id)
        .map(|t| matches!(t.status, TaskStatus::Pending | TaskStatus::InProgress))
        .unwrap_or(false);
    if task_unsettled {
        swarm.fail_task(&task_id);
    }

    Ok(results)
}

/// Serialize one agent result for the one-shot JSON output.
fn multi_agent_result_json(result: &multiagent::AgentResult) -> serde_json::Value {
    serde_json::json!({
        "agent_id": result.agent_id,
        "agent_name": result.agent_name,
        "role": result.role.name(),
        "success": result.success,
        "duration_secs": result.duration.as_secs_f64(),
        "content": result.content,
        "error": result.error,
        "usage": result.usage,
        // Stated explicitly so a consumer cannot mistake a persona completion
        // for an evidence-backed finding. The fan-out has no tools, so
        // `content` is never grounded in a repository, and a downstream
        // orchestrator must not treat it as findings.
        "tools_available": false,
        "grounded": false,
    })
}

async fn handle_command(
    command: Commands,
    quiet: bool,
    coordinator: bool,
    config: Config,
    ctx: &WorkshopContext,
    exec_mode: ExecutionMode,
    resume_session: Option<String>,
    output_format: HeadlessOutputFormat,
    config_path: Option<String>,
) -> Result<()> {
    match command {
        Commands::Chat => {
            crate::safety::killswitch::check_killswitch(None)?;
            // The REPL prints the one (compact) welcome itself; the workshop
            // banner + greeting here made two greetings back to back.
            let mut agent = Agent::new(config).await?;
            // Resume named session if --resume-session was provided
            if let Some(ref session_name) = resume_session {
                resume_named_session_or_bail(
                    || agent.resume_named_session(session_name),
                    session_name,
                    !quiet,
                )?;
            }
            agent.interactive().await?;
        }

        Commands::MultiChat { task, concurrency } => {
            // Headless one-shot: a positional task runs a single fan-out and
            // exits instead of entering the interactive REPL (P1-4).
            // (`-p <task> multi-chat` is routed to the same function from the
            // headless entry path.)
            if let Some(task) = task {
                return run_multi_chat_one_shot(
                    &config,
                    ctx,
                    concurrency,
                    coordinator,
                    &task,
                    output_format,
                    quiet,
                )
                .await;
            }

            // The --coordinator flag selects the swarm coordinator path.
            // When set, each task is queued in a Swarm mirroring the fleet
            // and the coordinator's role/trust assignment gates execution.
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

        Commands::ArtifactHarness {
            task,
            path,
            rounds,
            max_artifacts,
            allow_remote_source_egress,
        } => {
            let report = crate::orchestration::primitives::run_artifact_harness(
                &config,
                &task,
                Path::new(&path),
                rounds,
                max_artifacts,
                allow_remote_source_egress,
            )
            .await?;
            match output_format {
                HeadlessOutputFormat::Text => {
                    let rendered = report.render_text(quiet);
                    if !rendered.is_empty() {
                        println!("{rendered}");
                    }
                }
                HeadlessOutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                }
                HeadlessOutputFormat::StreamJson => {
                    println!("{}", serde_json::to_string(&report)?);
                }
            }
            if !report.analysis_succeeded() {
                anyhow::bail!(
                    "artifact harness did not complete every planned analysis call ({:?})",
                    report.status
                );
            }
        }

        Commands::Run {
            task,
            skill,
            preset,
            fork_of,
        } => {
            crate::safety::killswitch::check_killswitch(None)?;
            // `--fork-of`: resolve the finished task first, before any setup.
            let fork_parent = match fork_of.as_deref() {
                Some(query) => {
                    let log = crate::lifecycle::EventLog::default_location();
                    let (records, _) = log.read_all();
                    Some(
                        task_cmd::fork_parent(&records, &log, query)
                            .map_err(|m| anyhow::anyhow!("--fork-of: {m}"))?,
                    )
                }
                None => None,
            };
            // Resolve the task: --preset <id> renders the preset's task +
            // invariants; otherwise the positional task is required (clap
            // enforces this via required_unless_present).
            let task = resolve_preset_task(preset, task)?;
            // Headless Normal mode: read-only tools run; the first call that
            // needs confirmation stops the run (see the -p entry path).
            {
                use std::io::IsTerminal;
                if let Some(notice) =
                    headless::headless_normal_mode_notice(exec_mode, std::io::stdin().is_terminal())
                {
                    if !quiet {
                        eprintln!("{notice}");
                    }
                }
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

            // Wrap the task with the requested skill's instructions, if any.
            let task = match skill.as_deref() {
                Some(name) => {
                    let registry = crate::skills::SkillRegistry::discover();
                    if let Some(wrapped) = registry.wrap_task_with_skill(&task, name) {
                        wrapped
                    } else if let Some(refused) = registry.refused().iter().find(|r| r.name == name)
                    {
                        let rel = std::env::current_dir()
                            .ok()
                            .and_then(|cwd| {
                                refused
                                    .path
                                    .strip_prefix(&cwd)
                                    .ok()
                                    .map(|p| p.to_path_buf())
                            })
                            .unwrap_or_else(|| refused.path.clone());
                        anyhow::bail!(
                            "skill '{name}' was refused from {}: {}\n\
                             To admit into project: selfware skill admit {}\n\
                             To treat as user tool: mv {} ~/.selfware/skills/",
                            rel.display(),
                            refused.reason,
                            rel.display(),
                            rel.display()
                        );
                    } else {
                        anyhow::bail!(
                            "unknown skill '{name}' (looked in ~/.selfware/skills, ~/.selfware/commands, ./.selfware/skills, and ./.selfware/commands)"
                        );
                    }
                }
                None => task,
            };

            let start = std::time::Instant::now();
            let mut agent = Agent::new(config)
                .await?
                .with_confirmation_timeout(ONE_SHOT_CONFIRM_TIMEOUT);
            // Resume named session if --resume-session was provided
            if let Some(ref session_name) = resume_session {
                resume_named_session_or_bail(
                    || agent.resume_named_session(session_name),
                    session_name,
                    !quiet && !is_structured,
                )?;
            }
            let mut emitters: Vec<std::sync::Arc<dyn crate::agent::progress::ProgressEmitter>> =
                Vec::new();
            let jsonl =
                is_stream_json.then(|| std::sync::Arc::new(headless::JsonlProgressEmitter::new()));
            if let Some(jsonl) = &jsonl {
                emitters.push(jsonl.clone());
            } else if !quiet && !is_structured && crate::output::is_verbose() {
                // Structured event lines are `--verbose` diagnostics (see
                // `resume_progress_emitter`); the run summary and failure
                // banner explain a failed run by default.
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
            // Capture the agent's final answer for the structured result (see
            // the `-p` headless path for why this uses the event channel).
            let answer_capture = headless::AnswerCapture::new();
            agent =
                agent.with_event_emitter(answer_event_emitter(&answer_capture, jsonl.as_deref()));
            let baseline = patch_baseline_for(output_format);
            if let Some(parent) = fork_parent {
                agent.set_fork_parent(parent);
            }
            let run_result = agent.run_task(&task).await;
            let duration_ms = start.elapsed().as_millis() as u64;
            if let Some(jsonl) = &jsonl {
                jsonl.flush_text();
            }

            if !quiet && !is_structured {
                println!(
                    "{}",
                    crate::output::hyperlink::linkify_for_terminal(&render_text_run_summary(
                        &agent.run_summary(),
                        &run_result
                    ))
                );
                if let Some(partial) = agent.partial_progress(&run_result) {
                    println!("{}", partial.render());
                }
            }

            if is_structured {
                emit_structured_result(
                    &agent,
                    &run_result,
                    duration_ms,
                    answer_capture.take(),
                    &baseline,
                );
            } else if !quiet
                && run_result.is_ok()
                && earns_task_complete_banner(&agent)
                && agent
                    .grounding_status()
                    .is_none_or(|g| g.warning_note().is_none())
            {
                println!("{}", render_task_complete(start.elapsed()));
            }
            if let Err(e) = &run_result {
                if !quiet && !is_structured {
                    report_text_run_error(&agent, e);
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

        Commands::Review { path, scope, json } => {
            let root = std::path::PathBuf::from(&path);
            let inventory = tokio::task::spawn_blocking(move || {
                crate::analysis::repo_inventory::RepoInventory::scan(&root)
            })
            .await??;
            let scope = crate::analysis::repo_inventory::resolve_review_scope(
                scope.as_deref().unwrap_or_default(),
                &inventory,
            );
            let plan = inventory.review_plan(scope);
            if json {
                let value = serde_json::json!({
                    "inventory": inventory,
                    "review": plan,
                });
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else {
                println!("{}", inventory.render_text(Some(&plan)));
            }
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
        Commands::Dashboard => {
            // Dashboard launches the same live agent-driven TUI as the
            // default no-subcommand path — no canned agent-less loop.
            return run_live_agent_tui(config).await;
        }

        Commands::Resume { task_id } => {
            // Machine-readable formats: stdout carries ONLY JSON.
            let is_structured = is_structured_format(output_format);
            if is_structured {
                output::set_json_mode(true);
            }
            if !quiet && !is_structured {
                println!("{}", render_header(ctx));
                println!(
                    "{} {} journal entry {}...",
                    Glyphs::bookmark(),
                    "Opening".craftsman_voice(),
                    task_id.as_str().emphasis()
                );
            }

            let agent = Agent::resume(config, &task_id).await?;
            if let Some(checkpoint) = &agent.current_checkpoint {
                let task = match checkpoint.edited_description() {
                    Some(edited) => format!(
                        "{} (edited mid-run; started as: {})",
                        checkpoint.task_description, edited.original
                    ),
                    None => checkpoint.task_description.clone(),
                };
                if !quiet && !is_structured {
                    println!(
                        "{} Continuing: {}\n",
                        Glyphs::sprout(),
                        task.craftsman_voice()
                    );
                }
                run_resumed_agent(agent, quiet, output_format).await?;
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

                    let desc = format!(
                        "{}{}",
                        journal_title(&task.task_description, JOURNAL_DESC_MAX_CHARS),
                        task.edited_suffix()
                    );

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
                checkpoint.task_id.as_str().muted()
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
            if let Some(edited) = checkpoint.edited_description() {
                println!(
                    "   {}",
                    format!("(edited mid-run; started as: {})", edited.original).muted()
                );
            }
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

        Commands::Status => {
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

            // Fast reachability probe of configured endpoint (2s timeout)
            // to satisfy Rule 3 (never claim Connected without verification).
            let (endpoint_reachable, endpoint_status_detail) = probe_status_endpoint(&config).await;
            let redacted_endpoint = crate::config::redact_endpoint(&config.endpoint);

            // The global --output-format flag covers this command: both JSON
            // variants emit a single machine-readable object here.
            match output_format {
                HeadlessOutputFormat::Json | HeadlessOutputFormat::StreamJson => {
                    let status = serde_json::json!({
                        "model": ctx.model_name,
                        "endpoint": redacted_endpoint,
                        "is_local": ctx.is_local_model,
                        "endpoint_reachable": endpoint_reachable,
                        "endpoint_status": endpoint_status_detail,
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
                HeadlessOutputFormat::Text => {
                    if !quiet {
                        println!("{}", render_header(ctx));
                    }
                    println!(
                        "\n{} {}\n",
                        Glyphs::home(),
                        "Workshop Status".workshop_title()
                    );

                    let hosting = if ctx.is_local_model {
                        if endpoint_reachable {
                            format!("{} Running on your hardware (local)", Glyphs::home())
                                .garden_healthy()
                        } else {
                            format!(
                                "{} Local model endpoint issue: {} ({})",
                                Glyphs::wilt(),
                                endpoint_status_detail,
                                redacted_endpoint
                            )
                            .garden_wilting()
                        }
                    } else if endpoint_reachable {
                        format!("{} Connected to remote model", Glyphs::compass()).garden_healthy()
                    } else {
                        format!(
                            "{} Remote model endpoint issue: {} ({})",
                            Glyphs::wilt(),
                            endpoint_status_detail,
                            redacted_endpoint
                        )
                        .garden_wilting()
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
            improve_cmd::handle_improve_command(
                config, ctx, quiet, dry_run, continuous, max_cycles,
            )
            .await?;
        }

        #[cfg(feature = "self-improvement")]
        Commands::Evolve {
            generations,
            population,
            parallel,
            dry_run,
            workflow,
            arms_file,
            consensus,
            replicates,
            bench_cmd,
            arm_sandbox,
            keep_arms,
        } => {
            improve_cmd::handle_evolve_command(
                &config,
                ctx,
                quiet,
                improve_cmd::EvolveCommandArgs {
                    generations,
                    population,
                    parallel,
                    dry_run,
                    workflow,
                    arms_file,
                    consensus,
                    replicates,
                    bench_cmd,
                    arm_sandbox,
                    keep_arms,
                },
            )
            .await?;
        }

        Commands::Batch {
            file,
            workers,
            timeout,
            output,
            aggregate,
        } => {
            subcommands::handle_batch_command(
                &config, ctx, quiet, file, workers, timeout, output, aggregate,
            )
            .await?;
        }

        Commands::Workflow { command } => {
            workflow_cmd::handle_workflow_command(command, &config, ctx, quiet).await?;
        }

        Commands::State { command } => {
            subcommands::handle_state_command(command).await?;
        }

        Commands::Config { command } => match command {
            ConfigCommands::Show { json } => {
                config_show(&config, json)?;
            }
            ConfigCommands::SetKey { key } => {
                let key = match key {
                    Some(k) => k,
                    None => {
                        eprintln!("Enter API key (input echoed; prefer your OS keyring prompt in the future):");
                        let mut buf = String::new();
                        std::io::stdin().read_line(&mut buf)?;
                        buf.trim().to_string()
                    }
                };
                let endpoint = crate::config::set_api_key_for_endpoint(&config, &key)?;
                println!("API key stored in the OS keyring for {}", endpoint);
            }
        },

        Commands::Mcp { command } => match command {
            args::McpCommands::List => {
                if !quiet {
                    println!("{}", render_header(ctx));
                }
                if config.mcp.servers.is_empty() {
                    println!(
                        "No MCP servers configured (add [[mcp.servers]] to your config file, or use `selfware mcp add`)."
                    );
                } else {
                    println!("Configured MCP servers (from config, not live connectivity):");
                    for server in &config.mcp.servers {
                        let args = if server.args.is_empty() {
                            String::new()
                        } else {
                            format!(" {}", server.args.join(" "))
                        };
                        println!(
                            "  {} — {}{} (init timeout {}s)",
                            server.name, server.command, args, server.init_timeout_secs
                        );
                    }
                }
            }
            args::McpCommands::Add {
                name,
                command,
                args,
            } => {
                let path = mcp_target_config_path(&config);
                let message = edit_mcp_servers(&path, |servers| {
                    if servers
                        .iter()
                        .any(|s| s.get("name").and_then(|n| n.as_str()) == Some(name.as_str()))
                    {
                        anyhow::bail!("MCP server '{name}' already exists in {}", path.display());
                    }
                    let mut entry = toml::value::Table::new();
                    entry.insert("name".to_string(), toml::Value::String(name.clone()));
                    entry.insert("command".to_string(), toml::Value::String(command.clone()));
                    if !args.is_empty() {
                        entry.insert(
                            "args".to_string(),
                            toml::Value::Array(
                                args.iter()
                                    .map(|a| toml::Value::String(a.clone()))
                                    .collect(),
                            ),
                        );
                    }
                    servers.push(toml::Value::Table(entry));
                    Ok(format!("added MCP server '{name}' to {}", path.display()))
                })?;
                println!("{message}");
                println!("  takes effect on the next `selfware` start.");
            }
            args::McpCommands::Remove { name } => {
                let path = mcp_target_config_path(&config);
                let message = edit_mcp_servers(&path, |servers| {
                    let before = servers.len();
                    servers
                        .retain(|s| s.get("name").and_then(|n| n.as_str()) != Some(name.as_str()));
                    if servers.len() == before {
                        anyhow::bail!("no MCP server named '{name}' in {}", path.display());
                    }
                    Ok(format!(
                        "removed MCP server '{name}' from {}",
                        path.display()
                    ))
                })?;
                println!("{message}");
            }
        },

        Commands::McpServer => {
            crate::mcp::server::run_mcp_server(&config, config_path.as_deref()).await?;
        }

        Commands::SelfEvolve { port } => {
            crate::evolve::run_self_evolve_with_config(port, &config).await?;
        }

        Commands::Lsp => {
            crate::lsp::run_lsp_server().await?;
        }

        Commands::LlmDoctor => {
            // No workshop banner: a diagnostic report, not a session.
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
            bench_cmd::handle_swebench_command(command)?;
        }

        Commands::Bench {
            command,
            endpoint,
            suite,
            concurrent,
        } => {
            bench_cmd::handle_bench_command(
                &config, ctx, quiet, command, endpoint, suite, concurrent,
            )
            .await?;
        }

        Commands::LongTest {
            hours,
            timeout,
            max_iters,
            concurrent,
            endpoint,
            model,
            templates,
            output,
        } => {
            bench_cmd::handle_long_test_command(
                &config,
                ctx,
                quiet,
                bench_cmd::LongTestCommandArgs {
                    hours,
                    timeout,
                    max_iters,
                    concurrent,
                    endpoint,
                    model,
                    templates,
                    output,
                },
            )
            .await?;
        }

        Commands::Trust { .. } => unreachable!("Trust is handled before Config::load"),

        Commands::Doctor { all } => {
            // No workshop banner: a diagnostic report, not a session.
            let report = crate::doctor::run_doctor_scoped(config_path.as_deref(), all).await;
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
            println!(
                "   Endpoint: {}",
                crate::config::redact_endpoint(&endpoint).emphasis()
            );

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
                let redacted_endpoint = crate::config::redact_endpoint(&detected_config.endpoint);
                let mut toml_str = format!(
                    r#"# Auto-configured by selfware auto-config
endpoint = {}
model = {}
max_tokens = {}
context_length = {}
temperature = {}

[safety]
allowed_paths = ["./**", "/tmp/**"]
denied_paths = {}
protected_branches = ["main", "master"]

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
                    crate::cli::init_wizard::toml_quote(&redacted_endpoint),
                    crate::cli::init_wizard::toml_quote(&detected_config.model),
                    detected_config.max_tokens,
                    detected_config.context_length,
                    detected_config.temperature,
                    crate::config::default_denied_paths_toml(),
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

                crate::config::Config::validate_generated_toml(&toml_str)?;
                let save_path = std::path::Path::new("selfware.toml");
                if std::fs::symlink_metadata(save_path).is_ok_and(|m| m.file_type().is_symlink()) {
                    anyhow::bail!(
                        "Refusing to write configuration through symlink {}",
                        save_path.display()
                    );
                }
                std::fs::write(save_path, &toml_str)?;
                let _ = crate::config::trust::add_trusted_config(save_path);
                println!("{} Configuration saved to selfware.toml", "✓".green());
            }

            println!();
        }

        Commands::Unpack { scan, save } => {
            use crate::config::unpack;
            use colored::Colorize;

            println!("{}", render_header(ctx));

            match unpack::unpack_with_options(scan).await {
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
                                    anyhow::bail!("Failed to save config: {}", e);
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
                    anyhow::bail!("Unpack failed: {}", e);
                }
            }
            println!();
        }

        Commands::Init {
            template,
            scaffold,
            with_boot_assistant,
        } => {
            tokio::task::spawn_blocking(move || init_wizard::run_init_wizard(template, scaffold))
                .await
                .map_err(|e| anyhow::anyhow!("Task panicked: {}", e))??;
            if with_boot_assistant {
                // Fetch the tiny Q&A model now so `selfware boot --chat` works
                // offline later. A download failure must not fail `init` —
                // the config the wizard just wrote is the primary artifact.
                match crate::boot::model::ensure_model(&crate::boot::model::boot_dir()).await {
                    Ok(path) => println!("  Boot-assistant model ready at {}", path.display()),
                    Err(e) => eprintln!(
                        "  Boot-assistant download failed ({:#}). Retry later with `selfware boot --chat`.",
                        e
                    ),
                }
            }
        }

        Commands::Boot { .. } => unreachable!("boot is dispatched before configuration loading"),

        Commands::Runs { command } => {
            use crate::supervision::run_registry::{AbortOutcome, RunRecord, RunRegistry};
            use crate::supervision::run_supervisor::{RunStatus, RunSupervisor};
            use args::RunsCommand;

            let registry = RunRegistry::new()?;

            match command {
                RunsCommand::Start { task } => {
                    use std::io::IsTerminal;

                    // Same as the headless `-p` path: a supervised run in
                    // Normal mode runs read-only tools and stops at the first
                    // call that needs a confirmation nobody can give.
                    if let Some(notice) = headless::headless_normal_mode_notice(
                        config.execution_mode,
                        std::io::stdin().is_terminal(),
                    ) {
                        if !quiet {
                            eprintln!("{notice}");
                        }
                    }

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

                    if let Some(rx) = supervisor.attach(&id).await {
                        // P0-3: wait via status polling + event draining —
                        // the broadcast channel is never closed, so a bare
                        // `recv().await` loop would hang here forever once
                        // the agent stops emitting events.
                        supervisor
                            .wait_for_terminal(&id, rx, |ev| {
                                if !quiet {
                                    println!("  [{composite}] {ev:?}");
                                }
                            })
                            .await;
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
                    if crate::output::is_json_mode() {
                        // Machine-readable: a JSON array (empty array when none).
                        let arr: Vec<serde_json::Value> = runs
                            .iter()
                            .map(|r| {
                                serde_json::json!({
                                    "id": r.id,
                                    "pid": r.pid,
                                    "task": r.task,
                                    "status": r.effective_status(),
                                    "started_unix": r.started_unix,
                                    "updated_unix": r.updated_unix,
                                })
                            })
                            .collect();
                        println!(
                            "{}",
                            serde_json::to_string(&arr).unwrap_or_else(|_| "[]".to_string())
                        );
                    } else if runs.is_empty() {
                        println!(
                            "No runs recorded. (Runs are persisted under \
                             ~/.selfware/runs/ as they start.)"
                        );
                    } else {
                        println!("{:<22} {:<10} TASK", "ID", "STATUS");
                        for r in runs {
                            let task_short: String = r.task.chars().take(50).collect();
                            println!("{:<22} {:<10} {}", r.id, r.effective_status(), task_short);
                        }
                    }
                }

                RunsCommand::Abort { id } => {
                    match registry.abort(&id, chrono::Utc::now().timestamp())? {
                        AbortOutcome::NotFound => {
                            anyhow::bail!(
                                "No run '{}' in the registry (~/.selfware/runs/). \
                                 Run `selfware runs list` to see known runs.",
                                id
                            );
                        }
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
                        AbortOutcome::SignalUnsupported(pid) => println!(
                            "Cannot abort run '{}' on this platform: signalling the \
                             owning process (pid {}) requires Unix. The run is still \
                             running and was left unchanged.",
                            id, pid
                        ),
                        AbortOutcome::SignalFailed(pid, err) => println!(
                            "Could not abort run '{}': signalling the owning process \
                             (pid {}) failed ({}). The run is still running and was \
                             left unchanged.",
                            id, pid, err
                        ),
                    }
                }
            }
        }

        Commands::Killswitch { .. } => {
            unreachable!("Killswitch command handled before Config::load");
        }
        Commands::Resources { .. } => {
            unreachable!("Resources command handled before Config::load");
        }

        Commands::Tasks { .. } | Commands::Task { .. } | Commands::Agents { .. } => {
            unreachable!("lifecycle views are handled before Config::load");
        }

        Commands::Skill { command } => {
            use crate::skills::SkillRegistry;
            use args::SkillCommands;
            match command {
                SkillCommands::List => {
                    let registry = SkillRegistry::discover();
                    println!("Discovered skills ({}):", registry.len());
                    for skill in registry.list() {
                        let badge = skill.trust_badge();
                        let origin = skill.origin.as_deref().unwrap_or("user");
                        let tools = if skill.tools.is_empty() {
                            "-".to_string()
                        } else {
                            skill.tools.join(", ")
                        };
                        println!(
                            "  • {:<20} {:<35} (origin: {:<10}, tools: [{}])",
                            skill.name, badge, origin, tools
                        );
                    }

                    if let Ok(cwd) = std::env::current_dir() {
                        // Check candidates in candidate directory
                        let candidate_dir = cwd.join(".selfware").join("skill-candidates");
                        let candidates = SkillRegistry::discover_candidates(&candidate_dir);
                        if !candidates.is_empty() {
                            println!("\nUnadmitted skill candidates ({}):", candidates.len());
                            for c in candidates {
                                println!(
                                    "  ? {:<20} {} (path: .selfware/skill-candidates/{}.md)",
                                    c.name, c.description, c.name
                                );
                            }
                        }

                        // Check refused files in project discovery directories
                        let project_skills = cwd.join(".selfware").join("skills");
                        let unadmitted_skills =
                            SkillRegistry::detect_unadmitted_in_dir(&project_skills);
                        let project_commands = cwd.join(".selfware").join("commands");
                        let unadmitted_commands =
                            SkillRegistry::detect_unadmitted_in_dir(&project_commands);
                        let total_unadmitted: Vec<_> = unadmitted_skills
                            .into_iter()
                            .chain(unadmitted_commands)
                            .collect();

                        if !total_unadmitted.is_empty() {
                            println!(
                                "\n⚠️  Refused unadmitted files in project directory ({}):",
                                total_unadmitted.len()
                            );
                            for p in total_unadmitted {
                                let rel = p.strip_prefix(&cwd).unwrap_or(&p);
                                println!(
                                    "   • {} (missing from .admitted_ledger.json)",
                                    rel.display()
                                );
                                println!(
                                    "     To admit into project: selfware skill admit {}",
                                    rel.display()
                                );
                                println!(
                                    "     To treat as user tool: mv {} ~/.selfware/skills/",
                                    rel.display()
                                );
                            }
                        }
                    }
                }
                SkillCommands::Admit {
                    candidate_path,
                    target_dir,
                } => {
                    let c_path = std::path::PathBuf::from(candidate_path);
                    let t_dir = if let Some(td) = target_dir {
                        std::path::PathBuf::from(td)
                    } else if let Some(parent) = c_path.parent() {
                        if parent.ends_with("skills") || parent.ends_with("commands") {
                            parent.to_path_buf()
                        } else {
                            let cwd = std::env::current_dir()?;
                            cwd.join(".selfware").join("skills")
                        }
                    } else {
                        let cwd = std::env::current_dir()?;
                        cwd.join(".selfware").join("skills")
                    };

                    let res = SkillRegistry::admit_candidate_with_denied(
                        &c_path,
                        &t_dir,
                        Some(config.safety.denied_paths.as_slice()),
                    );
                    match res {
                        Ok(admitted) => {
                            println!(
                                "✅ Candidate skill '{}' admitted successfully into {}",
                                admitted.name,
                                t_dir.display()
                            );
                            if let Some(hash) = admitted.content_hash {
                                println!("   SHA-256 integrity digest: {}", hash);
                            }
                        }
                        Err(e) => {
                            anyhow::bail!("Failed to admit candidate: {e}");
                        }
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

/// Slash-command help for the chat surfaces (classic chat + TUI palette —
/// both route through the same input loop, so this is the single list).
pub(crate) fn slash_help_text() -> String {
    [
        "Available slash commands:",
        "  /help            — this list",
        "  /status          — session status (model, endpoint, messages, iterations)",
        "  /compact         — compress context now (structured summary pass)",
        "  /cost            — token totals and cost (when the endpoint bills)",
        "  /model [name]    — show current model/endpoint; with a name, hot-switch the session model",
        "  /doctor          — run environment diagnostics inline",
        "  /mcp             — list configured MCP servers",
        "  /clear           — reset the conversation",
        "  /plan            — enter plan mode (propose without executing)",
        "  /execute         — approve and run the current plan",
        "  /modify          — discard the plan, back to normal mode",
        "  /undo, /redo     — revert / reapply the last file edit (single-step, drift-guarded)",
        "  /agents          — configured swarm roster (no live agents between runs)",
        "  /resume [prefix] — list journal entries; with a task-id prefix, resume that session",
        "  /permissions     — current approval mode and how to change it",
        "  /journal         — recent journal entries",
        "  /memory          — memory entries and token usage",
        "  /tools           — registered tools (* = active)",
        "  /garden          — codebase garden visualization",
        "  /analyze <path>  — analyze a directory",
        "  /review <file>   — review a file",
        "  /bug             — copy-pasteable issue blurb (version, endpoint, model)",
        "  /skills          — discovered user skills (also /<name> to invoke)",
        "  /quit, /exit     — leave the session",
        "Also: !cmd runs a shell command (output goes into context), @path attaches a file.",
    ]
    .join("\n")
}

/// One-line session status for /status (model, endpoint, conversation size,
/// loop position) — the classic-chat form of the status panel.
#[cfg(feature = "tui")]
pub(crate) fn session_status_text(agent: &crate::agent::Agent) -> String {
    format!(
        "model: {} · endpoint: {} · messages: {} · iteration: {}/{}",
        agent.model(),
        agent.config().endpoint,
        agent.message_count(),
        agent.current_iteration(),
        agent.run_summary().max_iterations,
    )
}

/// /model without an argument — current model, endpoint, and how to change it.
pub(crate) fn model_status_text(agent: &crate::agent::Agent) -> String {
    format!(
        "model: {} · endpoint: {} · session switch: `/model <name>` · persistent: config key `model`",
        agent.model(),
        agent.config().endpoint
    )
}

/// /mcp — configured MCP servers from the resolved config (read-only;
/// reports configuration, not live connectivity).
pub(crate) fn mcp_servers_text(config: &crate::config::Config) -> String {
    if config.mcp.servers.is_empty() {
        return "no MCP servers configured (add [[mcp.servers]] to your config file)".to_string();
    }
    let mut out = format!("{} configured MCP server(s):", config.mcp.servers.len());
    for server in &config.mcp.servers {
        let args = if server.args.is_empty() {
            String::new()
        } else {
            format!(" {}", server.args.join(" "))
        };
        out.push_str(&format!(
            "\n  {} — {}{} (init timeout {}s)",
            server.name, server.command, args, server.init_timeout_secs
        ));
    }
    out
}

/// @path file attachment (claude/gemini parity): whitespace tokens of the
/// form `@some/file` that resolve to a readable file attach their content
/// as context. Unresolvable `@` tokens pass through untouched — never
/// pretend an attachment happened (rule 3).
pub(crate) struct Attachment {
    pub path: String,
    pub content: String,
    pub truncated: bool,
}

/// Cap on one attached file's content (chars) before the truncation flag.
const ATTACHMENT_MAX_CHARS: usize = 50_000;

/// Extract `@path` attachments from chat input. Returns the input unchanged
/// (the `@token` stays in the text) plus the resolved attachments.
pub(crate) fn expand_attachments(input: &str) -> Vec<Attachment> {
    let mut out = Vec::new();
    for token in input.split_whitespace() {
        let Some(raw) = token.strip_prefix('@') else {
            continue;
        };
        let raw = raw.trim_matches(|c| matches!(c, '"' | '\'' | ',' | ';'));
        if raw.is_empty() {
            continue;
        }
        let path = std::path::Path::new(raw);
        if !path.is_file() {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let truncated = content.chars().count() > ATTACHMENT_MAX_CHARS;
        let content: String = content.chars().take(ATTACHMENT_MAX_CHARS).collect();
        out.push(Attachment {
            path: raw.to_string(),
            content,
            truncated,
        });
    }
    out
}

/// Render attachments as a context block appended to the task text.
pub(crate) fn render_attachments(attachments: &[Attachment]) -> String {
    let mut out = String::new();
    for attachment in attachments {
        out.push_str(&format!(
            "\n\n<attached-file path=\"{}\"{}>\n{}\n</attached-file>",
            attachment.path,
            if attachment.truncated {
                " truncated=\"true\""
            } else {
                ""
            },
            attachment.content
        ));
    }
    out
}

/// /bug — a copy-pasteable issue blurb from tracked state only.
pub(crate) fn bug_report_text(agent: &crate::agent::Agent) -> String {
    let config = agent.config();
    format!(
        "selfware bug report\n\
         - version: {}\n\
         - endpoint: {}\n\
         - model: {}\n\
         - execution mode: {:?}\n\
         - config: {}",
        env!("CARGO_PKG_VERSION"),
        crate::config::redact_endpoint(&config.endpoint),
        config.model,
        config.execution_mode,
        config
            .loaded_config_path()
            .map(crate::config::display_config_path)
            .unwrap_or_else(|| "(defaults / env)".to_string()),
    )
}

/// /agents — the configured swarm roster with an honest liveness note
/// (no agents are tracked between runs).
pub(crate) fn agents_status_text() -> String {
    let swarm = crate::orchestration::swarm::create_dev_swarm();
    let mut agents: Vec<_> = swarm.list_agents();
    agents.sort_by_key(|a| std::cmp::Reverse(a.role.priority()));
    let mut out = format!(
        "{} configured swarm agent(s) — roster only; no live agents are tracked between runs:",
        agents.len()
    );
    for agent in agents {
        out.push_str(&format!("\n  {} ({})", agent.name, agent.role.name()));
    }
    out
}

/// /permissions — current approval mode and how to change it.
pub(crate) fn permissions_text(agent: &crate::agent::Agent) -> String {
    let mode = agent.config().execution_mode;
    let description = match mode {
        crate::config::ExecutionMode::Normal => "normal — asks for confirmation on mutating tools",
        crate::config::ExecutionMode::AutoEdit => "auto-edit — auto-approves file operations",
        crate::config::ExecutionMode::Yolo => "yolo — executes all tools without confirmation",
        crate::config::ExecutionMode::Daemon => "daemon — permanent yolo",
    };
    format!(
        "approval mode: {description}\nchange with: `-m normal|auto-edit|yolo`, `--yolo`, or SELFWARE_MODE env var"
    )
}

/// /journal + /resume listing — recent journal entries, titled.
pub(crate) fn journal_entries_text(limit: usize) -> String {
    match crate::agent::Agent::list_tasks() {
        Ok(tasks) if tasks.is_empty() => {
            "no journal entries yet — run a task to create one".to_string()
        }
        Ok(tasks) => {
            let mut out = format!("journal entries ({} total, showing {limit}):", tasks.len());
            for task in tasks.iter().take(limit) {
                out.push_str(&format!(
                    "\n  {} [{:?}] {}{}",
                    task.task_id,
                    task.status,
                    journal_title(&task.task_description, 48),
                    task.edited_suffix()
                ));
            }
            out
        }
        Err(e) => format!("journal unavailable: {e}"),
    }
}

/// /tools — registered tools, compact (activated marked).
#[cfg(feature = "tui")]
pub(crate) fn tools_text(agent: &crate::agent::Agent) -> String {
    let registry = agent.registry();
    let mut names: Vec<(String, bool)> = registry
        .list()
        .iter()
        .map(|t| (t.name().to_string(), registry.is_activated(t.name())))
        .collect();
    names.sort();
    let mut out = format!("{} registered tools (* = active):", names.len());
    for (name, active) in names {
        out.push_str(&format!("\n  {}{}", name, if active { " *" } else { "" }));
    }
    out
}

/// Which config file `mcp add`/`mcp remove` should edit. The resolved loaded
/// path wins when it's a user-level or TRUSTED repo config; an untrusted
/// repo-local config gets its mcp.servers reset on load, so editing it is
/// pointless — fall back to the user-level config with a note.
fn mcp_target_config_path(config: &crate::config::Config) -> std::path::PathBuf {
    if let Some(path) = config.loaded_config_path() {
        let checkout_local = path.file_name() == Some(std::ffi::OsStr::new("selfware.toml"));
        if !checkout_local || crate::config::trust::is_config_trusted(path) {
            return path.to_path_buf();
        }
        eprintln!(
            "note: '{}' is an untrusted repo config (its mcp.servers would be reset on load) — writing to the user config instead",
            path.display()
        );
    }
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".config/selfware/config.toml")
}

/// Apply one edit to the config file's `mcp.servers` array and persist
/// atomically (temp file + rename, like OntologyStore). The edit closure
/// returns the user-facing result message.
fn edit_mcp_servers(
    path: &std::path::Path,
    edit: impl FnOnce(&mut Vec<toml::Value>) -> Result<String>,
) -> Result<String> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let mut doc: toml::Value = if content.trim().is_empty() {
        toml::Value::Table(Default::default())
    } else {
        toml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("failed to parse {}: {}", path.display(), e))?
    };
    let root = doc
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("{} root is not a TOML table", path.display()))?;
    let mcp = root
        .entry("mcp".to_string())
        .or_insert_with(|| toml::Value::Table(Default::default()));
    let servers = mcp
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("[mcp] in {} is not a table", path.display()))?
        .entry("servers".to_string())
        .or_insert_with(|| toml::Value::Array(Vec::new()));
    let servers = servers
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("mcp.servers in {} is not an array", path.display()))?;
    let message = edit(servers)?;

    // Trust is bound to the file's content, so this operator-initiated
    // rewrite of a trusted repo config must re-record it afterwards or the
    // next run would treat the edited file as an untrusted checkout.
    let was_trusted = crate::config::trust::is_config_trusted(path);
    let rendered = toml::to_string_pretty(&doc)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    use std::io::Write;
    tmp.write_all(rendered.as_bytes())?;
    tmp.flush()?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)
        .map_err(|e| anyhow::anyhow!("failed to persist {}: {}", path.display(), e.error))?;
    if was_trusted {
        crate::config::trust::add_trusted_config(path)?;
    }
    Ok(message)
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

/// Title for a task prompt in journal/task listings: the first NON-EMPTY
/// trimmed line, ellipsized. A raw char-prefix of a multi-line prompt
/// renders blank when the prompt starts with newlines or indentation —
/// observed live: `-p` heredoc/stdin prompts produced empty journal titles.
pub(crate) fn journal_title(input: &str, max_chars: usize) -> String {
    let first_line = input
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    if first_line.is_empty() {
        return "(untitled task)".to_string();
    }
    truncate_with_ellipsis(first_line, max_chars)
}

/// `AgentError::TaskFailed` displays as "Task failed: …"; the CLI already
/// says "✗ Task failed:" / "outcome: failed —", so the inner prefix would
/// read twice ("✗ Task failed: Task failed: task incomplete …", UX field
/// test 0.9.2).
fn without_task_failed_prefix(error: &str) -> &str {
    error.strip_prefix("Task failed: ").unwrap_or(error)
}

/// Render the end-of-run summary block (headless text mode): the handful of
/// lines a user actually reads after a run — outcome, iterations, files
/// changed, verification, tokens/cost. Printed for completed AND failed
/// runs (a bare `✗ Task failed: MAX_ITERATIONS` explained nothing).
/// The line(s) a text-mode `run` / `-p` / resume prints on stderr after the
/// run summary when the run returned an error: `✗ Task failed: …` for a
/// failure, or — for a user interrupt / SIGTERM — a plain note plus the
/// resume hint the REPL prints on `/quit` (a saved checkpoint is what makes
/// the run resumable). The caller then marks the failure reported so the
/// process edge (`src/main.rs`) does not print it again as `Error: …`.
fn render_run_end_notice(
    end: crate::errors::RunEnd,
    error: &str,
    resume_id: Option<&str>,
) -> String {
    use crate::errors::RunEnd;
    let what = match end {
        RunEnd::Interrupted => "Task interrupted",
        RunEnd::Terminated => "Task terminated (SIGTERM)",
        RunEnd::Failed | RunEnd::Completed => {
            return format!("✗ Task failed: {}", without_task_failed_prefix(error))
        }
    };
    match resume_id {
        Some(id) => {
            format!("{what}. Resume later with:\n  selfware resume {id}\n  selfware --continue")
        }
        None => format!("{what}."),
    }
}

/// The last line `main` prints for an error that reached the process edge,
/// or `None` when the CLI already reported the run's end (see
/// [`crate::mark_failure_reported`]). A user interrupt reads
/// `Interrupted: …`, not `Error: …`. Scrubbed of credentials either way.
pub fn process_exit_message(e: &anyhow::Error, already_reported: bool) -> Option<String> {
    use crate::errors::RunEnd;
    if already_reported {
        return None;
    }
    let redact = crate::observability::telemetry::redact_secrets;
    Some(match RunEnd::classify_error(e) {
        RunEnd::Interrupted => format!("Interrupted: {}", redact(&e.to_string())),
        RunEnd::Terminated => format!("Terminated: {}", redact(&e.to_string())),
        RunEnd::Failed | RunEnd::Completed => format!("Error: {}", redact(&format!("{e:?}"))),
    })
}

/// The text run summary for a finished run, its outcome classified from the
/// result (a user interrupt reads `outcome: interrupted`, not `failed`).
/// `selfware resources [--zombies] [--json] [--all]` and
/// `selfware resources reap [--dry-run] [--include-kept]`.
async fn run_resources_command(
    command: Option<&args::ResourcesCommand>,
    zombies: bool,
    json: bool,
    all: bool,
) -> Result<()> {
    use crate::resources::reaper;
    let registry = crate::resources::ResourceRegistry::global();
    let driver = crate::resources::SystemDriver::default();
    match command {
        None => {
            let listing = reaper::list(registry, &driver, true, all).await;
            if json {
                let entries: Vec<&reaper::Entry> = listing
                    .entries
                    .iter()
                    .filter(|e| !zombies || matches!(e.status, reaper::EntryStatus::Zombie(_)))
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "entries": entries,
                        "container_check": listing.container_check,
                        "registry": registry.path(),
                    }))?
                );
            } else {
                println!("{}", reaper::render_listing(&listing, zombies));
            }
        }
        Some(args::ResourcesCommand::Reap {
            dry_run,
            include_kept,
            deadline_secs,
            json,
        }) => {
            let policy = crate::resources::TeardownPolicy::with_deadline(
                std::time::Duration::from_secs(*deadline_secs),
            );
            let report = reaper::reap(registry, &driver, *dry_run, *include_kept, policy).await;
            if *json {
                let drained = report
                    .drained
                    .as_ref()
                    .map(|d| serde_json::json!({ "released": d.released, "leaked": d.leaked }));
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "report": report,
                        "drained": drained,
                    }))?
                );
            } else {
                println!("{}", reaper::render_reap(&report));
            }
            if report
                .drained
                .as_ref()
                .is_some_and(|d| !d.leaked.is_empty())
            {
                anyhow::bail!("some resources could not be confirmed released");
            }
        }
    }
    Ok(())
}

fn render_text_run_summary(summary: &crate::agent::RunSummary, run_result: &Result<()>) -> String {
    let end = crate::errors::RunEnd::classify(run_result, crate::shutdown_reason());
    let reason = run_result
        .as_ref()
        .err()
        .map(|e| crate::observability::telemetry::redact_secrets(&e.to_string()));
    render_run_summary_for(summary, end, reason.as_deref())
}

/// Print the text-mode end of a run that returned an error (see
/// [`render_run_end_notice`]) and mark it reported.
fn report_text_run_error(agent: &Agent, e: &anyhow::Error) {
    let end = crate::errors::RunEnd::classify_error(e);
    let resume_id = agent
        .current_checkpoint
        .as_ref()
        .map(|c| c.task_id.as_str())
        .filter(|_| {
            matches!(
                end,
                crate::errors::RunEnd::Interrupted | crate::errors::RunEnd::Terminated
            )
        });
    // Defense-in-depth: upstream error bodies can echo the API key
    // (redacted at the source in http_status_error) — scrub again before
    // printing. `{e:#}` keeps the cause chain `main` used to print.
    let error = crate::observability::telemetry::redact_secrets(&format!("{e:#}"));
    eprintln!("{}", render_run_end_notice(end, &error, resume_id));
    crate::mark_failure_reported();
}

/// Whether a finished run gets the green "Task complete." line: exactly when
/// its classified outcome earns a clean ✅ (`FailureMode::is_clean_success`).
/// One decision, so the line can never be green under a ⚠️ or ❌ banner
/// (review of C2, 0.9.1: this used to re-derive it from the run summary).
fn earns_task_complete_banner(agent: &crate::agent::Agent) -> bool {
    agent
        .last_run_failure_mode()
        .is_some_and(|fm| fm.is_clean_success())
}

/// The run summary for the TUI's session exit and outcome line (the text
/// CLI renders a classified run end with [`render_run_summary_for`]).
#[cfg(any(feature = "tui", test))]
fn render_run_summary(summary: &crate::agent::RunSummary, failure: Option<&str>) -> String {
    let end = if failure.is_some() {
        crate::errors::RunEnd::Failed
    } else {
        crate::errors::RunEnd::Completed
    };
    render_run_summary_for(summary, end, failure)
}

/// [`render_run_summary`] for a classified run end: a user interrupt reads
/// `outcome: interrupted — …` (and SIGTERM `outcome: terminated — …`),
/// never `outcome: failed` — stopping a run is not a failure of the run.
fn render_run_summary_for(
    summary: &crate::agent::RunSummary,
    end: crate::errors::RunEnd,
    reason: Option<&str>,
) -> String {
    use crate::errors::RunEnd;
    let mut lines = vec!["── Run summary ──".to_string()];
    if let Some(edited) = &summary.edited_task {
        lines.push(format!(
            "task: {} (edited mid-run; started as: {})",
            journal_title(&edited.description, 80),
            journal_title(&edited.original, 80)
        ));
    }
    let stopped = matches!(end, RunEnd::Interrupted | RunEnd::Terminated);
    let failure = match end {
        RunEnd::Failed => Some(reason.unwrap_or("unknown error")),
        _ => None,
    };
    match failure {
        _ if stopped => lines.push(match reason {
            Some(reason) => format!("outcome: {} — {reason}", end.as_str()),
            None => format!("outcome: {}", end.as_str()),
        }),
        // A stop on a verified, unchanged tree says so: the work is not
        // what failed, the missing final answer is.
        Some(reason) => lines.push(match &summary.finish_stall_outcome {
            Some(clause) => format!(
                "outcome: failed — {}; {clause}",
                without_task_failed_prefix(reason)
            ),
            None => format!("outcome: failed — {}", without_task_failed_prefix(reason)),
        }),
        // A read-only task that changed nothing: the checks the model ran
        // are findings about the workspace, not a failed verification of a
        // change (0.9.4). Still never a bare "completed" over them.
        None if summary.verification_informational
            && matches!(summary.verification, Some((false, _))) =>
        {
            lines.push(
                "outcome: finished — no edits; checks run during the task did not pass (informational, not a verification failure)"
                    .to_string(),
            )
        }
        // Never print a bare "completed" over failed checks (AGENTS.md rule 3).
        None if matches!(summary.verification, Some((false, _))) => lines
            .push("outcome: finished — verification FAILED (not a verified result)".to_string()),
        // Allowed with a warning: checks fail on the final tree, but only
        // with errors the starting tree already had — not a verified result,
        // and not a failure of this change (c24).
        None if summary.verification_informational && !summary.preexisting_failures.is_empty() => {
            lines.push(
                "outcome: finished — no edits; checks run during the task fail on the unchanged \
                 tree (pre-existing, informational — no change was verified)"
                    .to_string(),
            )
        }
        None if !summary.preexisting_failures.is_empty() => lines.push(
            "outcome: completed — ⚠️ checks were already failing before the task \
             (pre-existing, not caused by this change; not a verified result)"
                .to_string(),
        ),
        // Allowed with a warning: the audit infrastructure failed, so the
        // result was not audited — never a bare "completed" over it.
        None if summary
            .requirements_audit
            .as_ref()
            .is_some_and(|a| a.is_not_performed()) =>
        {
            lines.push(
                "outcome: completed — requirements audit NOT PERFORMED (result not audited)"
                    .to_string(),
            )
        }
        // Allowed with a warning: the audit ledger stepped aside with
        // findings still open — never a bare "completed" over them.
        None if summary
            .requirements_audit
            .as_ref()
            .is_some_and(|a| a.open_findings() > 0) =>
        {
            let open = summary
                .requirements_audit
                .as_ref()
                .map(|a| a.open_findings())
                .unwrap_or_default();
            lines.push(format!(
                "outcome: completed — {open} requirements-audit finding(s) still OPEN (ledger stepped aside)"
            ))
        }
        // A review that ended with relevant files unread: finished, never a
        // completed review — the same verdict as the banner ("⚠️ Review
        // finished with PARTIAL coverage"; review 2026-09-27: this line said
        // "completed" under a banner that said it was not). The coverage line
        // below says how much was read.
        None if summary
            .review_coverage
            .as_ref()
            .is_some_and(|c| !c.complete) =>
        {
            lines.push(
                "outcome: finished — review coverage PARTIAL (not a completed review; see \
                 coverage below)"
                    .to_string(),
            )
        }
        // Allowed with a warning: the citation gate stepped aside with
        // citations that still do not match the files.
        None if summary
            .grounding
            .as_ref()
            .is_some_and(|g| g.warning_note().is_some()) =>
        {
            // The citation result itself is printed once, in the grounding
            // block below — the outcome only names the verdict.
            lines.push(
                "outcome: completed — answer not fully grounded (see citations below)".to_string(),
            )
        }
        None => lines.push("outcome: completed".to_string()),
    }
    let extension_note = if summary.budget_extended {
        " (budget extended)"
    } else {
        ""
    };
    // `turns` is the headless result's `num_turns` (one per step_started
    // event); `iterations` is the budget counter, which the planning turn
    // does not consume — so the two can differ by one.
    lines.push(format!(
        "iterations: {}/{}{} · turns: {}",
        summary.iterations, summary.max_iterations, extension_note, summary.turns
    ));
    if let Some(detail) = &summary.finish_stall_detail {
        lines.push(detail.clone());
    }
    if let Some(line) = &summary.done_check {
        lines.push(line.clone());
    }
    if let Some(bounds) = run_bounds_line(summary, crate::output::is_verbose()) {
        lines.push(bounds);
    }
    if let Some(paused) = summary.paused {
        lines.push(format!(
            "paused: {} (not counted against the wall-clock budget)",
            crate::lifecycle::projection::fmt_paused(
                i64::try_from(paused.as_millis()).unwrap_or(i64::MAX)
            )
        ));
    }
    if summary.files_changed.is_empty() {
        lines.push("files changed: none".to_string());
    } else {
        let preview: Vec<&str> = summary
            .files_changed
            .iter()
            .take(3)
            .map(String::as_str)
            .collect();
        let more = summary.files_changed.len().saturating_sub(preview.len());
        let suffix = if more > 0 {
            format!(", +{more} more")
        } else {
            String::new()
        };
        lines.push(format!(
            "files changed: {} ({}{})",
            summary.files_changed.len(),
            preview.join(", "),
            suffix
        ));
    }
    // Name what was checked: a bare count let "passed (7 checks)" vouch
    // for a waveform nothing had inspected (UX field test, 0.9.0).
    let named = |checks: usize| -> String {
        if summary.verification_checks.is_empty() {
            return format!("{checks} checks");
        }
        let shown: Vec<&str> = summary
            .verification_checks
            .iter()
            .take(4)
            .map(String::as_str)
            .collect();
        let more = summary
            .verification_checks
            .len()
            .saturating_sub(shown.len());
        let suffix = if more > 0 {
            format!(", +{more} more")
        } else {
            String::new()
        };
        format!("{checks} checks: {}{suffix}", shown.join(", "))
    };
    let verification = match summary.verification {
        // Never "passed" over a check that still fails, however old its
        // errors are (AGENTS.md rule 3).
        Some((true, checks))
            if summary.verification_informational && !summary.preexisting_failures.is_empty() =>
        {
            format!(
                "informational — no edits to verify; failing on the unchanged tree ({}): {}",
                named(checks),
                summary.preexisting_failures.join("; ")
            )
        }
        Some((true, checks)) if !summary.preexisting_failures.is_empty() => format!(
            "⚠️ failing before the task too — not caused by this change ({}): {}",
            named(checks),
            summary.preexisting_failures.join("; ")
        ),
        Some((true, checks)) => format!("passed ({})", named(checks)),
        Some((false, checks)) if summary.verification_informational => format!(
            "informational — no edits to verify; checks run during the task did not pass ({})",
            named(checks)
        ),
        Some((false, checks)) => format!("failed ({})", named(checks)),
        None => "not performed".to_string(),
    };
    lines.push(format!("verification: {verification}"));
    // Only when the task owned containers/processes/sessions.
    if let Some(resources) = &summary.resources {
        lines.push(resources.clone());
    }
    match summary.vision_calls {
        Some((0, failed)) => lines.push(format!(
            "visual check: not performed — vision tools failed {failed}× (image content not verified)"
        )),
        Some((ok, failed)) if failed > 0 => lines.push(format!(
            "visual check: {ok} succeeded, {failed} failed"
        )),
        _ => {}
    }
    if let Some(audit) = &summary.requirements_audit {
        lines.push(format!("requirements audit: {}", audit.label()));
    }
    // A review names the repository it inventoried and how much of the
    // relevant code it actually read (file_read line ranges, measured).
    if let Some(coverage) = &summary.review_coverage {
        lines.push(format!("review inventory: {}", coverage.inventory));
        lines.push(coverage.line.clone());
        if coverage.findings_recorded > 0 {
            lines.push(format!(
                "findings recorded while reading: {}",
                coverage.findings_recorded
            ));
        }
    }
    // The citation result appears ONCE: the warning note when the answer is
    // not fully grounded (it carries the unverified count, the breakdown and
    // why it was not corrected), otherwise the "Grounding:" line. Printing
    // both, plus the note in the outcome line and the banner evidence, said
    // the same thing four times (0.9.1).
    if let Some(grounding) = &summary.grounding {
        match grounding.warning_note() {
            Some(note) => {
                lines.push(note);
                for problem in grounding.problems.iter().take(5) {
                    lines.push(format!("  - {problem}"));
                }
            }
            None => lines.push(grounding.grounding_line()),
        }
    }
    if let Some(failures) = summary.context_summaries_disabled_after {
        lines.push(format!(
            "context summaries: disabled after {failures} failed summary calls \
             (result compaction / hard compression used instead)"
        ));
    }
    if let Some(failures) = summary.reflection_disabled_after {
        lines.push(format!(
            "reflection: disabled after {failures} failed reflection calls \
             (the task continued without periodic self-reflection)"
        ));
    }
    if let Some(failures) = summary.synthesis_disabled_after {
        lines.push(format!(
            "synthesis: disabled after {failures} failed synthesis calls \
             (the task continued without answer synthesis)"
        ));
    }
    if !summary.workloads.is_empty() {
        lines.push("workload quotas:".to_string());
        lines.extend(summary.workloads.iter().map(|l| format!("  {l}")));
    }
    let cost = summary
        .cost_usd
        .map(|c| {
            if summary.cost_complete {
                format!(", cost ${c:.4}")
            } else {
                format!(", known cost ${c:.4} (incomplete billing)")
            }
        })
        .unwrap_or_default();
    lines.push(format!("tokens: {} total{cost}", summary.total_tokens));
    if summary.cost_usd.is_none() {
        // The endpoint never reported a cost (self-hosted / free): say that
        // once, instead of "billing incomplete", which implied some billing
        // was missing (UX field test, 0.9.0).
        lines.push("cost: not reported by this endpoint".to_string());
    } else if !summary.cost_complete {
        lines.push(format!("billing incomplete: {} attempts without reported cost; restored or estimated usage may also lack billing provenance", summary.unmetered_attempts));
    }
    if let Some(latency) = &summary.call_latency {
        let slowest = latency
            .slowest
            .as_ref()
            .map(|s| {
                format!(
                    ", slowest {:.1}s ({}, {})",
                    s.elapsed_ms as f64 / 1000.0,
                    s.model,
                    s.path
                )
            })
            .unwrap_or_default();
        lines.push(format!(
            "model latency: {} calls, {:.1}s total{}",
            latency.call_count,
            latency.total_ms as f64 / 1000.0,
            slowest
        ));
    }
    lines.join("\n")
}

/// The run's active bounds, e.g. `bounds: iterations 400 · no wall/token/cost
/// budget set`. Token, wall-clock and cost budgets default to unset, so by
/// default a task is bounded only by its iteration cap; the summary says so
/// instead of leaving it implied. Shown in `--verbose` and whenever the run
/// hit the iteration cap (when the bound decided the outcome); `None`
/// otherwise. The defaults are unchanged — that is a maintainer decision.
fn run_bounds_line(summary: &crate::agent::RunSummary, verbose: bool) -> Option<String> {
    let cost_note = cost_budget_enforcement_note(summary);
    // A cost budget the run could not (fully) enforce is always named: the
    // user set a limit and must not assume it held (AGENTS.md Rule 3).
    if !verbose && !summary.hit_iteration_cap && cost_note.is_none() {
        return None;
    }
    let budgets = &summary.budgets;
    let mut parts = vec![format!("iterations {}", summary.max_iterations)];
    let mut unset: Vec<&str> = Vec::new();
    match budgets.max_wall_secs {
        Some(secs) => parts.push(format!("wall {secs}s")),
        None => unset.push("wall"),
    }
    match budgets.max_budget_tokens {
        Some(tokens) => parts.push(format!("tokens {tokens}")),
        None => unset.push("token"),
    }
    match budgets.max_cost_usd {
        Some(cost) => match cost_note {
            Some(note) => parts.push(format!("cost ${cost:.2} ({note})")),
            None => parts.push(format!("cost ${cost:.2}")),
        },
        None => unset.push("cost"),
    }
    if !unset.is_empty() {
        parts.push(format!("no {} budget set", unset.join("/")));
    }
    Some(format!("bounds: {}", parts.join(" · ")))
}

/// Why a configured `max_cost_usd` could not be (fully) enforced: the
/// enforcer only sees provider-reported cost, and nothing is estimated for
/// calls that reported none. `None` when no cost budget is set or every
/// call's cost was reported.
fn cost_budget_enforcement_note(summary: &crate::agent::RunSummary) -> Option<String> {
    summary.budgets.max_cost_usd?;
    if summary.cost_complete {
        return None;
    }
    Some(match summary.cost_usd {
        Some(_) if summary.unmetered_attempts > 0 => format!(
            "enforced on reported cost only: {} call(s) reported no cost",
            summary.unmetered_attempts
        ),
        Some(_) => "enforced on reported cost only: billing incomplete".to_string(),
        None => "NOT enforced: the provider reported no cost".to_string(),
    })
}

fn take_prefix_chars(input: &str, max_chars: usize) -> String {
    input.chars().take(max_chars).collect()
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
    let mut cmd = tokio::process::Command::new("cargo");
    crate::safety::process_env::sanitize_command_env(&mut cmd);
    let output = cmd
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

/// The `(key, value, source)` rows `selfware config show` prints: the core
/// endpoint/model fields and EVERY field a model profile can apply, each
/// with its provenance.
pub(crate) fn config_show_rows(
    config: &Config,
) -> Vec<(String, String, crate::config::ConfigSource)> {
    let mut rows: Vec<(String, String, crate::config::ConfigSource)> = Vec::new();

    rows.push((
        "endpoint".to_string(),
        crate::config::redact_endpoint(&config.endpoint),
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
    // Every profile-applied field has a row (N2 Rule-5 sweep): the per-call
    // cap — including its `[profile: qwen38, scaled for max_tokens=N]`
    // provenance — and the concurrency pins were applied but never shown.
    rows.push((
        "agent.max_call_secs".to_string(),
        match config.agent.max_call_secs {
            Some(secs) if secs > 0 => format!("{secs}"),
            _ => "none (uncapped)".to_string(),
        },
        config.source_of("agent.max_call_secs"),
    ));
    rows.push((
        "concurrency.max_streams".to_string(),
        format!("{}", config.concurrency.max_streams),
        config.source_of("concurrency.max_streams"),
    ));
    rows.push((
        "concurrency.max_global".to_string(),
        format!("{}", config.concurrency.max_global),
        config.source_of("concurrency.max_global"),
    ));
    rows.push((
        "agent.context_content_ratio".to_string(),
        match config.agent.context_growth_p99_tokens {
            Some(growth) => format!(
                "{:.4} (derived: 1 - {growth} p99 growth / history budget)",
                config.effective_context_content_ratio()
            ),
            None => format!("{}", config.agent.context_content_ratio),
        },
        config.source_of("agent.context_content_ratio"),
    ));
    // Per-workload turn quotas: one row per field that is set (profile or
    // user), so a profile-applied quota is never invisible.
    for kind in crate::config::TurnWorkload::ALL {
        let q = config.workloads.get(kind);
        if let Some(on) = q.enable_thinking {
            let key = format!("workloads.{kind}.enable_thinking");
            let source = config.source_of(&key);
            rows.push((key, format!("{on}"), source));
        }
        if let Some(n) = q.max_tokens {
            let key = format!("workloads.{kind}.max_tokens");
            let source = config.source_of(&key);
            rows.push((key, format!("{n}"), source));
        }
    }

    if let Some(extra) = &config.extra_body {
        // Shown through the shared config redaction: an `[extra_body.headers]`
        // credential or a secret-named key never prints in the clear.
        let mut view = serde_json::Value::Object(extra.clone());
        crate::config::model::redact_config_secrets(&mut view);
        let extra = view.as_object().cloned().unwrap_or_default();
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
    rows
}

/// Render the effective configuration with provenance annotations.
///
/// Mirrors `selfware config show`. Each row is `key = value [source]`,
/// formatted into aligned columns for readability.
pub(crate) fn config_show(config: &Config, json: bool) -> Result<()> {
    let rows = config_show_rows(config);

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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

/// Resolve a `run` invocation's task text: `--preset <id>` renders the evolve
/// preset's task + invariants; otherwise the positional task is used.
fn resolve_preset_task(preset: Option<String>, task: Option<String>) -> Result<String> {
    match (preset, task) {
        (Some(id), _) => {
            let p = crate::evolve::presets::preset(&id).ok_or_else(|| {
                let known: Vec<String> = crate::evolve::presets::presets()
                    .into_iter()
                    .map(|p| p.id.to_string())
                    .collect();
                anyhow::anyhow!("unknown preset '{id}' (available: {})", known.join(", "))
            })?;
            Ok(crate::evolve::presets::render_prompt(&p))
        }
        (None, Some(task)) => Ok(task),
        (None, None) => anyhow::bail!("a task or --preset <id> is required"),
    }
}
