use anyhow::{Context, Result};

use crate::config::Config;
use crate::ui::components::{render_header, WorkshopContext};
use crate::ui::style::{Glyphs, SelfwareStyle};

#[cfg(feature = "tui")]
use super::args::DemoScenarioKind;
use super::args::{StateCommands, StateOutputFormat};
use super::{journal_title, ONE_SHOT_CONFIRM_TIMEOUT};

const MAX_BATCH_TASK_FILE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_STATE_IMPORT_BYTES: u64 = 16 * 1024 * 1024;

pub(crate) fn read_bounded_cli_file(
    path: &std::path::Path,
    max_bytes: u64,
    label: &str,
) -> Result<String> {
    let meta = std::fs::symlink_metadata(path)
        .with_context(|| format!("Cannot inspect {label} '{}'", path.display()))?;
    if meta.file_type().is_symlink() {
        anyhow::bail!("Refusing to read symlinked {label} '{}'", path.display());
    }
    if meta.len() > max_bytes {
        anyhow::bail!(
            "Refusing to read {label} '{}': size {} bytes exceeds {} byte limit",
            path.display(),
            meta.len(),
            max_bytes
        );
    }
    std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {label} '{}'", path.display()))
}

pub(crate) fn write_cli_file_no_symlink(
    path: &std::path::Path,
    contents: impl AsRef<[u8]>,
) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        if let Ok(meta) = std::fs::symlink_metadata(parent) {
            if meta.file_type().is_symlink() {
                anyhow::bail!(
                    "Refusing to write into symlinked directory '{}'",
                    parent.display()
                );
            }
        }
    }
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            anyhow::bail!("Refusing to overwrite symlinked file '{}'", path.display());
        }
    }
    std::fs::write(path, contents).with_context(|| format!("Failed to write '{}'", path.display()))
}

/// Parse task-file contents into a list of tasks.
///
/// Each non-empty, non-comment line is a task.  Lines starting with `#`
/// are treated as comments and skipped, as are blank/whitespace-only lines.
/// Leading and trailing whitespace is trimmed from each task.
pub fn parse_task_file(contents: &str) -> Vec<String> {
    contents
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect()
}

pub(crate) async fn handle_batch_command(
    config: &Config,
    ctx: &WorkshopContext,
    quiet: bool,
    file: String,
    workers: usize,
    timeout: u64,
    output: String,
    aggregate: bool,
) -> Result<()> {
    if !quiet {
        println!("{}", render_header(ctx));
    }

    // ── Read and parse the task file ──
    let contents = match read_bounded_cli_file(
        std::path::Path::new(&file),
        MAX_BATCH_TASK_FILE_BYTES,
        "batch task file",
    ) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("✗ Cannot read task file '{}': {}", file, e);
            anyhow::bail!("Failed to read batch task file '{}': {}", file, e);
        }
    };
    let tasks = parse_task_file(&contents);
    if tasks.is_empty() {
        if !quiet {
            println!(
                "No tasks found in '{}' (empty file or all lines blank/commented).",
                file
            );
        }
        return Ok(());
    }

    if !quiet {
        println!(
            "\n{} Batch mode: {} task(s) from '{}'",
            Glyphs::gear(),
            tasks.len(),
            file
        );
        if workers > 1 {
            println!("⚠ Note: concurrent batch tasks share the same workspace and may conflict.");
            println!(
                "  Running sequentially for safety (workers={} requested).",
                workers
            );
        }
    }

    // ── Run each task sequentially for workspace safety ──
    struct TaskResult {
        index: usize,
        task: String,
        ok: bool,
        error: Option<String>,
    }

    let mut results: Vec<TaskResult> = Vec::with_capacity(tasks.len());

    for (i, task) in tasks.iter().enumerate() {
        if !quiet {
            println!("\n── Task {}/{}: {} ──", i + 1, tasks.len(), task);
        }
        let start = std::time::Instant::now();
        let mut agent = crate::agent::Agent::new(config.clone())
            .await?
            .with_confirmation_timeout(ONE_SHOT_CONFIRM_TIMEOUT);
        let run_result = if timeout > 0 {
            match tokio::time::timeout(
                std::time::Duration::from_secs(timeout),
                agent.run_task(task),
            )
            .await
            {
                Ok(res) => res,
                Err(_) => Err(anyhow::anyhow!("Task timed out after {}s", timeout)),
            }
        } else {
            agent.run_task(task).await
        };
        let duration = start.elapsed();

        let (ok, error) = match &run_result {
            Ok(()) => (true, None),
            Err(e) => (false, Some(format!("{:#}", e))),
        };

        if !quiet {
            let snippet = journal_title(task, 60);
            let status = if ok { "PASS" } else { "FAIL" };
            if ok {
                println!("  ✓ {} · {} ({}s)", status, snippet, duration.as_secs());
            } else {
                println!(
                    "  ✗ {} · {} ({}s) — {}",
                    status,
                    snippet,
                    duration.as_secs(),
                    error.as_deref().unwrap_or("unknown error")
                );
            }
        }

        results.push(TaskResult {
            index: i,
            task: task.clone(),
            ok,
            error,
        });
    }

    // ── Summary ──
    let total = results.len();
    let passed = results.iter().filter(|r| r.ok).count();
    let failed = total - passed;

    println!("\n{}", "─".repeat(60));
    println!(
        "Batch Summary: {} total, {} passed, {} failed",
        total, passed, failed
    );
    for r in &results {
        let snippet = journal_title(&r.task, 50);
        let status = if r.ok { "PASS" } else { "FAIL" };
        if r.ok {
            println!("  {} · {} · {}", r.index + 1, status, snippet);
        } else {
            println!(
                "  {} · {} · {} — {}",
                r.index + 1,
                status,
                snippet,
                r.error.as_deref().unwrap_or("unknown error")
            );
        }
    }
    println!("{}", "─".repeat(60));

    // ── Write results to output directory ──
    let output_path = std::path::Path::new(&output);
    if let Ok(meta) = std::fs::symlink_metadata(output_path) {
        if meta.file_type().is_symlink() {
            anyhow::bail!(
                "Refusing to write into symlinked output directory '{}'",
                output
            );
        }
    }
    std::fs::create_dir_all(output_path)
        .with_context(|| format!("Failed to create output directory '{}'", output))?;
    let summary_path = output_path.join("batch_summary.txt");
    let mut summary = format!(
        "Batch Summary: {} total, {} passed, {} failed\n\n",
        total, passed, failed
    );
    for r in &results {
        let status = if r.ok { "PASS" } else { "FAIL" };
        summary.push_str(&format!("{} · {} · {}", r.index + 1, status, r.task));
        if let Some(ref err) = r.error {
            summary.push_str(&format!(" — {}", err));
        }
        summary.push('\n');
    }
    write_cli_file_no_symlink(&summary_path, &summary).with_context(|| {
        format!(
            "Failed to write batch summary to '{}'",
            summary_path.display()
        )
    })?;
    if !quiet {
        println!("Results written to: {}", summary_path.display());
    }

    if aggregate {
        let aggregate_path = output_path.join("batch_results.json");
        let payload = serde_json::json!({
            "total": total,
            "passed": passed,
            "failed": failed,
            "results": results.iter().map(|r| serde_json::json!({
                "index": r.index + 1,
                "task": r.task,
                "ok": r.ok,
                "error": r.error,
            })).collect::<Vec<_>>(),
        });
        write_cli_file_no_symlink(&aggregate_path, serde_json::to_vec_pretty(&payload)?)
            .with_context(|| {
                format!(
                    "Failed to write aggregated batch JSON to '{}'",
                    aggregate_path.display()
                )
            })?;
        if !quiet {
            println!("Aggregated JSON written to: {}", aggregate_path.display());
        }
    }

    if failed > 0 {
        anyhow::bail!(
            "Batch completed with {} failed task(s) out of {}",
            failed,
            total
        );
    }

    Ok(())
}

pub(crate) fn resolve_state_dir(dir: Option<String>) -> std::path::PathBuf {
    dir.map(std::path::PathBuf::from)
        .unwrap_or_else(crate::swl::state::StateBackendType::default_state_dir)
}

pub(crate) fn sorted_json_keys(
    state: &std::collections::HashMap<String, serde_json::Value>,
) -> Vec<String> {
    let mut keys: Vec<String> = state.keys().cloned().collect();
    keys.sort();
    keys
}

pub(crate) async fn handle_state_command(command: StateCommands) -> Result<()> {
    use crate::swl::state::backend::{FileBackend, StateBackend};
    use crate::swl::state::StateManager;

    match command {
        StateCommands::Show {
            workflow,
            format,
            dir,
        } => {
            let base_dir = resolve_state_dir(dir);
            let mut manager = StateManager::new_file_based(&workflow, base_dir)?;
            manager.load().await?;
            let state = manager.get_all();

            match format {
                StateOutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(state)?);
                }
                StateOutputFormat::Text | StateOutputFormat::Table => {
                    if state.is_empty() {
                        println!("No saved state for workflow '{}'.", workflow);
                    } else {
                        println!("Workflow state: {}", workflow.emphasis());
                        for key in sorted_json_keys(state) {
                            if let Some(value) = state.get(&key) {
                                println!("  {} = {}", key, value);
                            }
                        }
                    }
                }
            }
        }
        StateCommands::List { dir } => {
            let backend = FileBackend::new(resolve_state_dir(dir))?;
            let mut workflows = backend.list().await?;
            workflows.sort();
            if workflows.is_empty() {
                println!("No saved workflow state found.");
            } else {
                for workflow in workflows {
                    println!("{workflow}");
                }
            }
        }
        StateCommands::Delete {
            workflow,
            dir,
            force,
        } => {
            if !force {
                anyhow::bail!(
                    "Refusing to delete workflow state without --force: {}",
                    workflow
                );
            }
            let backend = FileBackend::new(resolve_state_dir(dir))?;
            backend.delete(&workflow).await?;
            println!("Deleted state for workflow '{}'.", workflow);
        }
        StateCommands::Export {
            workflow,
            output,
            dir,
        } => {
            let base_dir = resolve_state_dir(dir);
            let mut manager = StateManager::new_file_based(&workflow, base_dir)?;
            manager.load().await?;
            let payload = serde_json::to_string_pretty(manager.get_all())?;
            write_cli_file_no_symlink(std::path::Path::new(&output), payload)?;
            println!("Exported state for '{}' to {}.", workflow, output);
        }
        StateCommands::Import {
            workflow,
            input,
            dir,
            force,
        } => {
            let base_dir = resolve_state_dir(dir);
            let backend = FileBackend::new(base_dir.clone())?;
            if backend.exists(&workflow).await && !force {
                anyhow::bail!(
                    "State for '{}' already exists. Re-run with --force to overwrite.",
                    workflow
                );
            }

            let content = read_bounded_cli_file(
                std::path::Path::new(&input),
                MAX_STATE_IMPORT_BYTES,
                "workflow state import file",
            )?;
            let state: std::collections::HashMap<String, serde_json::Value> =
                serde_json::from_str(&content)?;

            let mut manager = StateManager::new_file_based(&workflow, base_dir)?;
            manager.clear();
            manager.set_all(state)?;
            manager.save().await?;
            println!("Imported state for '{}' from {}.", workflow, input);
        }
    }

    Ok(())
}

#[cfg(feature = "tui")]
pub(crate) fn run_demo_scenario(scenario: DemoScenarioKind, fast: bool, quiet: bool) -> Result<()> {
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
