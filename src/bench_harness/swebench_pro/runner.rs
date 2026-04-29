//! Top-level harness runner: ties together dataset loading, llama-server
//! lifecycle, selfware subprocess invocation, patch capture, trial aggregation,
//! and `patches.json` collation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::catalog::{quant_catalog, QuantSpec};
use super::dataset::{load_instances, Instance};
use super::harness::{capture_patch, clone_instance, run_selfware, LlamaServer, LlamaServerOpts};
use crate::config::PromptProfile;

/// Caller-supplied configuration for `run_swebench_pro`.
///
/// All fields are required so the CLI layer is the single source of defaults.
#[derive(Clone, Debug)]
pub struct SwebenchProOpts {
    pub quants: Vec<String>,
    /// When non-empty, overrides `instances` and pins the run to specific IDs.
    pub instance_ids: Vec<String>,
    pub instances: usize,
    pub scenario_timeout: Duration,
    pub ctx: u32,
    pub parallel: u32,
    /// Reserved for future llama-server `--concurrency` tuning; today the only
    /// effect is that we surface it in `plan.json` for traceability.
    pub concurrency: u32,
    pub trials: u32,
    pub output: PathBuf,
    pub selfware_bin: PathBuf,
    pub skip_existing: bool,
    pub llama_opts: LlamaServerOpts,
    pub prompt_mode: String,    // "diagnostic" or "official"
    pub prompt_profile: String, // "default" or "swebench_pro"
    pub official_eval: bool,    // run Docker eval after patches
}

fn is_false(b: &bool) -> bool {
    !b
}

#[derive(Serialize, Deserialize, Clone)]
struct PerRunResult {
    instance_id: String,
    quant: String,
    trial: u32,
    exit_code: i32,
    timed_out: bool,
    wall_secs: f64,
    patch_lines: usize,
    patch_bytes: usize,
    pred_path: PathBuf,
    /// When the run never reached the agent (e.g. clone or boot failed),
    /// this string explains why.  Empty for successful or agent-failed runs.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    error: String,
    #[serde(default, skip_serializing_if = "is_false")]
    empty_diff: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    test_only_patch: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    has_source_edit: bool,
}

/// Run the SWE-bench Pro harness end-to-end.  Always returns `Ok(())` if the
/// run scheduler completed (individual instance/quant failures are recorded in
/// `result.json`); any infrastructure-level error (no quants, dataset load
/// failure, output dir unwritable) bails.
pub fn run_swebench_pro(opts: SwebenchProOpts) -> Result<()> {
    let catalog = quant_catalog();
    let valid_quants: Vec<String> = opts
        .quants
        .iter()
        .filter(|q| {
            if catalog.contains_key(q.as_str()) {
                true
            } else {
                eprintln!("  ⚠ unknown quant: {} — skipping", q);
                false
            }
        })
        .cloned()
        .collect();
    if valid_quants.is_empty() {
        bail!("no valid quants supplied");
    }

    std::fs::create_dir_all(&opts.output)
        .with_context(|| format!("creating {}", opts.output.display()))?;

    eprintln!("loading SWE-bench Pro dataset...");
    let instances = load_instances(&opts.instance_ids, opts.instances)?;
    if instances.is_empty() {
        bail!("dataset loader returned 0 instances (filters too strict?)");
    }

    eprintln!("selected {} instance(s):", instances.len());
    for inst in &instances {
        eprintln!(
            "  • {}  ({}, {}, {} chars)",
            inst.instance_id,
            inst.repo,
            inst.repo_language.as_deref().unwrap_or("?"),
            inst.problem_statement.len()
        );
    }
    eprintln!("selected {} quant(s):", valid_quants.len());
    for q in &valid_quants {
        eprintln!("  • {}", q);
    }

    let plan_path = opts.output.join("plan.json");
    let llama_server_argv = {
        let dummy_gguf = std::path::Path::new("/dev/null/dummy.gguf");
        super::harness::build_llama_server_args(
            &super::catalog::QuantSpec {
                label: "plan-dummy",
                gguf: "dummy.gguf",
                alias: "plan-dummy",
                mmproj: "dummy.mmproj.gguf",
            },
            &opts.llama_opts,
            dummy_gguf,
            None,
        )
    };
    std::fs::write(
        &plan_path,
        serde_json::to_vec_pretty(&json!({
            "started_at": Utc::now().to_rfc3339(),
            "quants": valid_quants,
            "instance_ids": instances.iter().map(|i| &i.instance_id).collect::<Vec<_>>(),
            "scenario_timeout_secs": opts.scenario_timeout.as_secs(),
            "ctx": opts.ctx,
            "parallel": opts.parallel,
            "concurrency": opts.concurrency,
            "trials": opts.trials,
            "selfware_bin": opts.selfware_bin,
            "prompt_mode": opts.prompt_mode,
            "prompt_profile": opts.prompt_profile,
            "llama_server_binary": opts.llama_opts.binary,
            "llama_server_argv": llama_server_argv,
        }))?,
    )?;

    let trials = opts.trials.max(1);
    let mut all_runs: Vec<PerRunResult> = Vec::new();
    let overall_started = Instant::now();

    // Per-quant outer loop matches Python — boot once, drive every instance × trial,
    // tear down before next quant.
    for quant in &valid_quants {
        let spec = catalog
            .get(quant.as_str())
            .expect("validated above")
            .clone();

        eprintln!("{}", "=".repeat(70));
        eprintln!("QUANT: {}", quant);
        eprintln!("{}", "=".repeat(70));

        LlamaServer::stop_existing();
        let server = match LlamaServer::boot(&spec, &opts.llama_opts) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("  ❌ boot failed: {} — recording failures and skipping", e);
                // Each (instance × trial) is "attempted" from the harness's
                // point of view, even if the server never came up.  Write a
                // result.json with exit_code=-2 so denominators are stable.
                for trial in 1..=trials {
                    for inst in &instances {
                        match record_boot_failure(&opts, &spec, inst, trial, &e.to_string()) {
                            Ok(res) => all_runs.push(res),
                            Err(rec_err) => eprintln!(
                                "    failed to record boot failure for {} trial {}: {}",
                                inst.instance_id, trial, rec_err
                            ),
                        }
                    }
                }
                continue;
            }
        };

        let concurrency = opts.concurrency.max(1) as usize;
        for trial in 1..=trials {
            let trial_results = run_trial(&opts, &spec, &instances, trial, concurrency);
            all_runs.extend(trial_results);
        }

        // Drop server explicitly so subsequent quant boot has a clean GPU.
        drop(server);
    }

    LlamaServer::stop_existing();

    // Always aggregate / write patches even if some quants failed.
    write_aggregate(&opts.output, &all_runs)?;
    write_patches_json(&opts, &all_runs)?;

    if opts.official_eval {
        run_official_eval(&opts)?;
    }

    eprintln!(
        "DONE in {:.0}s. Output: {}",
        overall_started.elapsed().as_secs_f64(),
        opts.output.display()
    );
    if !opts.official_eval {
        eprintln!("Next steps:");
        eprintln!(
            "  python /home/ivo/SWE-bench_Pro-os/swe_bench_pro_eval.py --predictions {} ...",
            opts.output.join("patches.json").display()
        );
    }
    Ok(())
}

/// Drive every (instance) for one `trial`, optionally fanning out to
/// `concurrency` worker threads against the same llama-server (which is
/// configured via `--parallel` to accept N concurrent slots).
///
/// Sequential when `concurrency <= 1` to preserve existing log ordering.
fn run_trial(
    opts: &SwebenchProOpts,
    spec: &QuantSpec,
    instances: &[Instance],
    trial: u32,
    concurrency: usize,
) -> Vec<PerRunResult> {
    if concurrency <= 1 {
        return instances
            .iter()
            .filter_map(|inst| match run_one(opts, spec, inst, trial) {
                Ok(res) => Some(res),
                Err(e) => {
                    eprintln!("    {} trial {}: error: {}", inst.instance_id, trial, e);
                    None
                }
            })
            .collect();
    }

    // Real parallelism: a tiny work-stealing pool of OS threads.  Each thread
    // dequeues an instance index from a Mutex<Vec<usize>>, runs `run_one`, and
    // appends to a shared results vec.
    use std::sync::{Arc, Mutex};

    let queue: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new((0..instances.len()).rev().collect()));
    let results: Arc<Mutex<Vec<PerRunResult>>> = Arc::new(Mutex::new(Vec::new()));
    let opts_arc = Arc::new(opts.clone());
    let spec_arc = Arc::new(spec.clone());
    let instances_arc = Arc::new(instances.to_vec());

    std::thread::scope(|scope| {
        let n = concurrency.min(instances.len()).max(1);
        let mut handles = Vec::with_capacity(n);
        for _ in 0..n {
            let queue = Arc::clone(&queue);
            let results = Arc::clone(&results);
            let opts_c = Arc::clone(&opts_arc);
            let spec_c = Arc::clone(&spec_arc);
            let instances_c = Arc::clone(&instances_arc);
            handles.push(scope.spawn(move || loop {
                let idx = {
                    let mut q = queue.lock().unwrap();
                    q.pop()
                };
                let Some(idx) = idx else { return };
                let inst = &instances_c[idx];
                match run_one(&opts_c, &spec_c, inst, trial) {
                    Ok(res) => results.lock().unwrap().push(res),
                    Err(e) => eprintln!("    {} trial {}: error: {}", inst.instance_id, trial, e),
                }
            }));
        }
        for h in handles {
            let _ = h.join();
        }
    });

    let mut out = results.lock().unwrap().clone();
    // Stable order: by (instance_id, trial) so logs/aggregate are deterministic.
    out.sort_by(|a, b| a.instance_id.cmp(&b.instance_id));
    out
}

/// Compute the per-(quant, instance, trial) directory under `output/trials/...`.
fn trial_dir_for(output: &Path, spec_label: &str, instance_id: &str, trial: u32) -> PathBuf {
    output
        .join("trials")
        .join(trial.to_string())
        .join("runs")
        .join(spec_label)
        .join(instance_id)
}

/// Persist a synthetic result.json for an (instance, trial) we never got to
/// run because the llama-server failed to boot.  exit_code=-2 marks
/// "infrastructure failure"; the aggregate counter still includes it as
/// attempted-and-failed.
fn record_boot_failure(
    opts: &SwebenchProOpts,
    spec: &QuantSpec,
    inst: &Instance,
    trial: u32,
    reason: &str,
) -> Result<PerRunResult> {
    let trial_dir = trial_dir_for(&opts.output, spec.label, &inst.instance_id, trial);
    std::fs::create_dir_all(&trial_dir)
        .with_context(|| format!("creating {}", trial_dir.display()))?;
    let pred_path = trial_dir.join(format!("{}.pred", inst.instance_id));
    // Touch an empty pred so downstream tooling expecting the file finds it.
    if !pred_path.exists() {
        std::fs::write(&pred_path, "")?;
    }
    let result = PerRunResult {
        instance_id: inst.instance_id.clone(),
        quant: spec.label.into(),
        trial,
        exit_code: -2,
        timed_out: false,
        wall_secs: 0.0,
        patch_lines: 0,
        patch_bytes: 0,
        pred_path,
        error: format!("boot failed: {}", reason),
        empty_diff: true,
        test_only_patch: false,
        has_source_edit: false,
    };
    std::fs::write(
        trial_dir.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    Ok(result)
}

fn run_one(
    opts: &SwebenchProOpts,
    spec: &QuantSpec,
    inst: &Instance,
    trial: u32,
) -> Result<PerRunResult> {
    let trial_dir = trial_dir_for(&opts.output, spec.label, &inst.instance_id, trial);
    std::fs::create_dir_all(&trial_dir)
        .with_context(|| format!("creating {}", trial_dir.display()))?;
    let pred_path = trial_dir.join(format!("{}.pred", inst.instance_id));

    if opts.skip_existing && pred_path.exists() {
        eprintln!(
            "  → {} (trial {}): SKIP (pred exists)",
            inst.instance_id, trial
        );
        // Reconstruct minimal record from the existing pred so aggregation works.
        let bytes = std::fs::metadata(&pred_path)
            .map(|m| m.len() as usize)
            .unwrap_or(0);
        let lines = std::fs::read_to_string(&pred_path)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        return Ok(PerRunResult {
            instance_id: inst.instance_id.clone(),
            quant: spec.label.into(),
            trial,
            exit_code: 0,
            timed_out: false,
            wall_secs: 0.0,
            patch_lines: lines,
            patch_bytes: bytes,
            pred_path,
            error: String::new(),
            empty_diff: lines == 0 && bytes == 0,
            test_only_patch: false,
            has_source_edit: false,
        });
    }

    eprintln!("  → {} (trial {})", inst.instance_id, trial);

    let workdir = trial_dir.join("repo");
    if let Err(e) = clone_instance(&inst.repo, &inst.base_commit, &workdir) {
        eprintln!("    clone failed: {}", e);
        // Persist the failure as a real result.json (exit_code=-2) so the
        // aggregate denominator counts it as attempted-and-failed instead of
        // silently dropping the trial.
        let pred_path = trial_dir.join(format!("{}.pred", inst.instance_id));
        if !pred_path.exists() {
            std::fs::write(&pred_path, "")?;
        }
        let result = PerRunResult {
            instance_id: inst.instance_id.clone(),
            quant: spec.label.into(),
            trial,
            exit_code: -2,
            timed_out: false,
            wall_secs: 0.0,
            patch_lines: 0,
            patch_bytes: 0,
            pred_path,
            error: format!("clone failed: {}", e),
            empty_diff: true,
            test_only_patch: false,
            has_source_edit: false,
        };
        std::fs::write(
            trial_dir.join("result.json"),
            serde_json::to_vec_pretty(&result)?,
        )?;
        return Ok(result);
    }

    let profile = match opts.prompt_profile.as_str() {
        "swebench_pro" => PromptProfile::SwebenchPro,
        _ => PromptProfile::Default,
    };
    let prompt = profile.task_prompt(inst, &opts.prompt_mode);
    std::fs::write(trial_dir.join("prompt.txt"), &prompt)?;
    std::fs::write(
        trial_dir.join("instance.json"),
        serde_json::to_vec_pretty(inst)?,
    )?;

    let log_path = trial_dir.join("agent.log");
    // Use the configured llama-server port so `--port 8001` actually points
    // selfware sub-runs at the right endpoint instead of the hard-coded 8000.
    let endpoint = format!("http://127.0.0.1:{}/v1", opts.llama_opts.port);
    let outcome = run_selfware(
        &opts.selfware_bin,
        &workdir,
        &prompt,
        spec.alias,
        &endpoint,
        opts.scenario_timeout,
        &log_path,
        &trial_dir,
    )?;
    eprintln!(
        "    agent exit={} after {:.1}s{}",
        outcome.exit_code,
        outcome.wall_secs,
        if outcome.timed_out { " (timeout)" } else { "" }
    );

    let patch = capture_patch(&workdir).unwrap_or_else(|e| {
        eprintln!("    git diff failed: {}", e);
        String::new()
    });
    std::fs::write(&pred_path, &patch)?;

    // Use structured output when available, fall back to `capture_patch`.
    let (patch_lines, patch_bytes) = outcome
        .parsed_result
        .as_ref()
        .map(|r| (r.patch_lines, r.patch_bytes))
        .unwrap_or_else(|| (patch.lines().count(), patch.len()));

    let empty_diff = patch.trim().is_empty();
    let test_only_patch = !empty_diff && is_test_only_patch(&patch);
    let has_source_edit = !empty_diff && !test_only_patch;

    let result = PerRunResult {
        instance_id: inst.instance_id.clone(),
        quant: spec.label.into(),
        trial,
        exit_code: outcome.exit_code,
        timed_out: outcome.timed_out,
        wall_secs: outcome.wall_secs,
        patch_lines,
        patch_bytes,
        pred_path: pred_path.clone(),
        error: String::new(),
        empty_diff,
        test_only_patch,
        has_source_edit,
    };
    std::fs::write(
        trial_dir.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    eprintln!(
        "    patch: {} lines, {} bytes → {}",
        result.patch_lines,
        result.patch_bytes,
        pred_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    );

    Ok(result)
}

/// Parse a git diff and determine whether every modified file is a test file.
/// A file counts as a test file when its path or basename contains "test".
fn is_test_only_patch(patch: &str) -> bool {
    let mut has_any_file = false;
    for line in patch.lines() {
        if line.starts_with("diff --git ") {
            // Extract the b/ path after "diff --git a/... b/..."
            if let Some(b_start) = line.find(" b/") {
                let path = &line[b_start + 3..];
                let basename = path.rsplit('/').next().unwrap_or(path);
                if !path.contains("test") && !basename.contains("test") {
                    return false;
                }
                has_any_file = true;
            }
        } else if line.starts_with("+++ b/") || line.starts_with("--- a/") {
            // Fallback for unified diff headers without diff --git
            let path = &line[6..];
            let basename = path.rsplit('/').next().unwrap_or(path);
            if !path.contains("test") && !basename.contains("test") {
                return false;
            }
            has_any_file = true;
        }
    }
    has_any_file
}

#[derive(Serialize)]
struct AggregateEntry {
    quant: String,
    instance_id: String,
    trials: u32,
    /// Proxy metric: runs that exited cleanly, didn't time out, and produced a non-empty patch.
    attempted_patch: u32,
    attempted_patch_rate: f64,
    empty_patch_rate: f64,
    test_only_patch_rate: f64,
    source_edit_rate: f64,
    median_wall_secs: f64,
    best_wall_secs: f64,
    /// Trial number whose patch had the most lines (proxy for "biggest attempt").
    best_trial: u32,
    median_patch_lines: f64,
}

#[derive(Serialize)]
struct AggregateReport {
    generated_at: String,
    total_runs: usize,
    entries: Vec<AggregateEntry>,
}

fn write_aggregate(output: &Path, runs: &[PerRunResult]) -> Result<()> {
    // Group by (quant, instance_id).
    let mut groups: BTreeMap<(String, String), Vec<&PerRunResult>> = BTreeMap::new();
    for r in runs {
        groups
            .entry((r.quant.clone(), r.instance_id.clone()))
            .or_default()
            .push(r);
    }

    let mut entries = Vec::new();
    for ((quant, instance_id), trials) in groups {
        let total = trials.len() as u32;
        let attempted_patch = trials
            .iter()
            .filter(|r| r.exit_code == 0 && !r.timed_out && r.patch_bytes > 0)
            .count() as u32;
        let empty_patch = trials.iter().filter(|r| r.empty_diff).count() as u32;
        let test_only = trials.iter().filter(|r| r.test_only_patch).count() as u32;
        let source_edit = trials.iter().filter(|r| r.has_source_edit).count() as u32;

        let attempted_patch_rate = if total == 0 {
            0.0
        } else {
            attempted_patch as f64 / total as f64
        };
        let empty_patch_rate = if total == 0 {
            0.0
        } else {
            empty_patch as f64 / total as f64
        };
        let test_only_patch_rate = if total == 0 {
            0.0
        } else {
            test_only as f64 / total as f64
        };
        let source_edit_rate = if total == 0 {
            0.0
        } else {
            source_edit as f64 / total as f64
        };

        let mut walls: Vec<f64> = trials.iter().map(|r| r.wall_secs).collect();
        walls.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median_wall = median_f64(&walls);
        let best_wall = walls.first().copied().unwrap_or(0.0);

        let best_trial = trials
            .iter()
            .max_by_key(|r| r.patch_lines)
            .map(|r| r.trial)
            .unwrap_or(0);

        let mut lines: Vec<f64> = trials.iter().map(|r| r.patch_lines as f64).collect();
        lines.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median_patch_lines = median_f64(&lines);

        entries.push(AggregateEntry {
            quant,
            instance_id,
            trials: total,
            attempted_patch,
            attempted_patch_rate,
            empty_patch_rate,
            test_only_patch_rate,
            source_edit_rate,
            median_wall_secs: median_wall,
            best_wall_secs: best_wall,
            best_trial,
            median_patch_lines,
        });
    }

    let report = AggregateReport {
        generated_at: Utc::now().to_rfc3339(),
        total_runs: runs.len(),
        entries,
    };
    std::fs::write(
        output.join("aggregate.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}

fn median_f64(sorted: &[f64]) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let n = sorted.len();
    if n.is_multiple_of(2) {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    } else {
        sorted[n / 2]
    }
}

/// Write a `patches.json` ready to feed into `swe_bench_pro_eval.py`.
///
/// Picks the patch with the most lines per (quant × instance) — a coarse "best
/// trial" heuristic that matches what `gather_patches.py` does today (it just
/// uses the latest patch; we improve on that by preferring non-empty diffs).
fn write_patches_json(opts: &SwebenchProOpts, runs: &[PerRunResult]) -> Result<()> {
    #[derive(Serialize)]
    struct Pred {
        instance_id: String,
        model_name_or_path: String,
        model_patch: String,
        trial: u32,
    }

    let mut best: BTreeMap<(String, String), &PerRunResult> = BTreeMap::new();
    for r in runs {
        let key = (r.quant.clone(), r.instance_id.clone());
        match best.get(&key) {
            Some(existing) if existing.patch_lines >= r.patch_lines => {}
            _ => {
                best.insert(key, r);
            }
        }
    }

    let mut preds = Vec::new();
    for ((quant, instance_id), r) in best {
        let patch = std::fs::read_to_string(&r.pred_path).unwrap_or_default();
        preds.push(Pred {
            instance_id,
            model_name_or_path: quant,
            model_patch: patch,
            trial: r.trial,
        });
    }

    std::fs::write(
        opts.output.join("patches.json"),
        serde_json::to_vec_pretty(&preds)?,
    )?;
    Ok(())
}

/// Run the official SWE-bench Pro Docker eval script against the generated
/// `patches.json`.  If the script doesn't exist the run is skipped gracefully.
fn run_official_eval(opts: &SwebenchProOpts) -> Result<()> {
    let patches_json = opts.output.join("patches.json");
    let eval_dir = opts.output.join("official_eval");
    std::fs::create_dir_all(&eval_dir)
        .with_context(|| format!("creating {}", eval_dir.display()))?;

    let script = std::path::PathBuf::from("/home/ivo/SWE-bench_Pro-os/swe_bench_pro_eval.py");
    if !script.exists() {
        let skip = json!({
            "skipped": true,
            "reason": "eval script not found",
            "script": script,
        });
        std::fs::write(
            eval_dir.join("eval_output.json"),
            serde_json::to_vec_pretty(&skip)?,
        )?;
        eprintln!("  ⚠ official eval skipped (script not found)");
        return Ok(());
    }

    eprintln!("  → running official eval...");
    let output = std::process::Command::new("python3")
        .arg(&script)
        .arg("--predictions")
        .arg(&patches_json)
        .arg("--output")
        .arg(&eval_dir)
        .output()
        .with_context(|| "spawning official eval script")?;

    let eval_out = json!({
        "stdout": String::from_utf8_lossy(&output.stdout),
        "stderr": String::from_utf8_lossy(&output.stderr),
        "exit_code": output.status.code(),
    });
    std::fs::write(
        eval_dir.join("eval_output.json"),
        serde_json::to_vec_pretty(&eval_out)?,
    )?;

    if output.status.success() {
        if let Ok(parsed) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
            std::fs::write(
                eval_dir.join("eval_results.json"),
                serde_json::to_vec_pretty(&parsed)?,
            )?;
        }
    }

    eprintln!(
        "    official eval finished (exit={})",
        output.status.code().unwrap_or(-1)
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_instance() -> Instance {
        Instance {
            instance_id: "demo-1".into(),
            repo: "octocat/hello-world".into(),
            base_commit: "deadbeef".into(),
            problem_statement: "  fix bug ".into(),
            fail_to_pass: serde_json::json!(["tests/test_a.py::test_x"]),
            selected_test_files_to_run: serde_json::json!(["tests/test_a.py"]),
            repo_language: Some("python".into()),
            extra: Default::default(),
        }
    }

    #[test]
    fn prompt_profile_diagnostic_includes_tests() {
        let p = PromptProfile::SwebenchPro.task_prompt(&dummy_instance(), "diagnostic");
        assert!(p.contains("[mode: diagnostic]"));
        assert!(p.contains("tests/test_a.py::test_x"));
        assert!(p.contains("RELEVANT TEST FILES: tests/test_a.py"));
    }

    #[test]
    fn prompt_profile_official_excludes_tests() {
        let p = PromptProfile::SwebenchPro.task_prompt(&dummy_instance(), "official");
        assert!(p.contains("[mode: official]"));
        assert!(!p.contains("tests/test_a.py::test_x"));
        assert!(!p.contains("RELEVANT TEST FILES:"));
        assert!(!p.contains("FAIL-TO-PASS"));
    }

    #[test]
    fn prompt_profile_contains_tool_contract() {
        let inst = dummy_instance();
        for mode in &["diagnostic", "official"] {
            let p = PromptProfile::SwebenchPro.task_prompt(&inst, mode);
            assert!(
                p.contains("Valid tool call ONLY"),
                "tool contract missing in {} mode",
                mode
            );
            assert!(
                p.contains("NO prose before tool XML"),
                "no-prose rule missing in {} mode",
                mode
            );
        }
    }

    #[test]
    fn prompt_profile_contains_no_test_edit_rule() {
        let inst = dummy_instance();
        for mode in &["diagnostic", "official"] {
            let p = PromptProfile::SwebenchPro.task_prompt(&inst, mode);
            assert!(
                p.contains("Do NOT modify test files"),
                "no-test-edit rule missing in {} mode",
                mode
            );
        }
    }

    #[test]
    fn prompt_profile_contains_verification_requirement() {
        let inst = dummy_instance();
        for mode in &["diagnostic", "official"] {
            let p = PromptProfile::SwebenchPro.task_prompt(&inst, mode);
            assert!(
                p.contains("confirm they pass before finishing"),
                "verification requirement missing in {} mode",
                mode
            );
        }
    }

    #[test]
    fn is_test_only_patch_detects_test_only() {
        let patch = r#"diff --git a/tests/test_a.py b/tests/test_a.py
--- a/tests/test_a.py
+++ b/tests/test_a.py
@@ -1 +1 @@
-old
+new
"#;
        assert!(is_test_only_patch(patch));
    }

    #[test]
    fn is_test_only_patch_detects_source_edit() {
        let patch = r#"diff --git a/src/main.py b/src/main.py
--- a/src/main.py
+++ b/src/main.py
@@ -1 +1 @@
-old
+new
"#;
        assert!(!is_test_only_patch(patch));
    }

    #[test]
    fn median_handles_even_and_odd() {
        assert!((median_f64(&[1.0, 2.0, 3.0]) - 2.0).abs() < 1e-9);
        assert!((median_f64(&[1.0, 2.0, 3.0, 4.0]) - 2.5).abs() < 1e-9);
        assert_eq!(median_f64(&[]), 0.0);
    }
}
