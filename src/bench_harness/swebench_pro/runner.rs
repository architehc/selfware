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
use super::dataset::{coerce_string_list, load_instances, Instance};
use super::harness::{capture_patch, clone_instance, run_selfware, LlamaServer, LlamaServerOpts};

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
}

#[derive(Serialize, Deserialize)]
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
                eprintln!("  ❌ boot failed: {} — skipping this quant", e);
                continue;
            }
        };

        for trial in 1..=trials {
            for inst in &instances {
                match run_one(&opts, &spec, inst, trial) {
                    Ok(res) => all_runs.push(res),
                    Err(e) => eprintln!("    {} trial {}: error: {}", inst.instance_id, trial, e),
                }
            }
        }

        // Drop server explicitly so subsequent quant boot has a clean GPU.
        drop(server);
    }

    LlamaServer::stop_existing();

    // Always aggregate / write patches even if some quants failed.
    write_aggregate(&opts.output, &all_runs)?;
    write_patches_json(&opts, &all_runs)?;

    eprintln!(
        "DONE in {:.0}s. Output: {}",
        overall_started.elapsed().as_secs_f64(),
        opts.output.display()
    );
    eprintln!("Next steps:");
    eprintln!(
        "  python /home/ivo/SWE-bench_Pro-os/swe_bench_pro_eval.py --predictions {} ...",
        opts.output.join("patches.json").display()
    );
    Ok(())
}

fn run_one(
    opts: &SwebenchProOpts,
    spec: &QuantSpec,
    inst: &Instance,
    trial: u32,
) -> Result<PerRunResult> {
    let trial_dir = opts
        .output
        .join("trials")
        .join(trial.to_string())
        .join("runs")
        .join(spec.label)
        .join(&inst.instance_id);
    std::fs::create_dir_all(&trial_dir)
        .with_context(|| format!("creating {}", trial_dir.display()))?;
    let pred_path = trial_dir.join(format!("{}.pred", inst.instance_id));

    if opts.skip_existing && pred_path.exists() {
        eprintln!("  → {} (trial {}): SKIP (pred exists)", inst.instance_id, trial);
        // Reconstruct minimal record from the existing pred so aggregation works.
        let bytes = std::fs::metadata(&pred_path).map(|m| m.len() as usize).unwrap_or(0);
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
        });
    }

    eprintln!("  → {} (trial {})", inst.instance_id, trial);

    let workdir = trial_dir.join("repo");
    if let Err(e) = clone_instance(&inst.repo, &inst.base_commit, &workdir) {
        eprintln!("    clone failed: {}", e);
        bail!("clone failed: {}", e);
    }

    let prompt = build_prompt(inst);
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

    let result = PerRunResult {
        instance_id: inst.instance_id.clone(),
        quant: spec.label.into(),
        trial,
        exit_code: outcome.exit_code,
        timed_out: outcome.timed_out,
        wall_secs: outcome.wall_secs,
        patch_lines: patch.lines().count(),
        patch_bytes: patch.len(),
        pred_path: pred_path.clone(),
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

/// Build the "fix this issue" prompt — string-for-string the same as run.py's
/// `build_prompt`, so a Rust run produces an identical agent prompt.
pub fn build_prompt(inst: &Instance) -> String {
    let problem = inst
        .problem_statement
        .trim()
        .trim_matches('"')
        .replace("\\n", "\n");
    let tests = coerce_string_list(&inst.selected_test_files_to_run);
    let fail = coerce_string_list(&inst.fail_to_pass);

    let fail_str = fail
        .iter()
        .map(|t| format!("  - {}", t))
        .collect::<Vec<_>>()
        .join("\n");
    let test_str = tests.join(", ");

    format!(
        "You are working on a real codebase in the current directory. Resolve this issue:\n\n{problem}\n\nThe fix needs to make these tests pass:\n{fail_str}\n\nRelevant test files: {test_str}\n\nSteps:\n1. Read the failing test files to understand the expected behavior.\n2. Read the implementation files mentioned in the tests.\n3. Make the smallest code change that resolves the issue.\n4. Do NOT modify the test files themselves.\n5. Run the failing tests if possible to verify your fix.\n6. When done, summarize what you changed."
    )
}

#[derive(Serialize)]
struct AggregateEntry {
    quant: String,
    instance_id: String,
    trials: u32,
    successes: u32,
    pass_rate: f64,
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
        let successes = trials
            .iter()
            .filter(|r| r.exit_code == 0 && !r.timed_out && r.patch_bytes > 0)
            .count() as u32;
        let pass_rate = if total == 0 {
            0.0
        } else {
            successes as f64 / total as f64
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
            successes,
            pass_rate,
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
    fn build_prompt_includes_problem_and_tests() {
        let p = build_prompt(&dummy_instance());
        assert!(p.contains("fix bug"));
        assert!(p.contains("tests/test_a.py::test_x"));
        assert!(p.contains("Relevant test files: tests/test_a.py"));
        assert!(p.contains("Do NOT modify the test files themselves."));
    }

    #[test]
    fn median_handles_even_and_odd() {
        assert!((median_f64(&[1.0, 2.0, 3.0]) - 2.0).abs() < 1e-9);
        assert!((median_f64(&[1.0, 2.0, 3.0, 4.0]) - 2.5).abs() < 1e-9);
        assert_eq!(median_f64(&[]), 0.0);
    }
}
