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
use super::manifest::{SwebenchProOptsSnapshot, SweepManifest, TrialManifest, TrialState};
use super::trace::{RunTrace, TraceEvent};
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
    pub official_eval_script: PathBuf,
    pub official_eval_raw_sample_path: PathBuf,
    pub official_eval_scripts_dir: PathBuf,
    pub official_eval_dockerhub_username: String,
    pub official_eval_num_workers: u32,
    pub official_eval_use_local_docker: bool,
    pub official_eval_redo: bool,
    pub official_eval_block_network: bool,
    /// Resume from an existing manifest.json in the output directory.
    pub resume: bool,
    /// Re-run trials that are already Evaluated (requires --resume or auto-detect).
    pub force_rerun: bool,
}

fn is_false(b: &bool) -> bool {
    !b
}

fn opts_to_snapshot(opts: &SwebenchProOpts) -> SwebenchProOptsSnapshot {
    SwebenchProOptsSnapshot {
        quants: opts.quants.clone(),
        instance_ids: opts.instance_ids.clone(),
        instances: opts.instances,
        scenario_timeout_secs: opts.scenario_timeout.as_secs(),
        ctx: opts.ctx,
        parallel: opts.parallel,
        concurrency: opts.concurrency,
        trials: opts.trials,
        prompt_mode: opts.prompt_mode.clone(),
        prompt_profile: opts.prompt_profile.clone(),
        official_eval: opts.official_eval,
        llama_server_binary: opts.llama_opts.binary.clone(),
    }
}

/// Determine whether a trial should be skipped based on its manifest state.
fn should_skip_trial(opts: &SwebenchProOpts, trial: Option<&TrialManifest>) -> bool {
    let Some(t) = trial else {
        return false;
    };
    match t.state {
        TrialState::Evaluated => !opts.force_rerun,
        TrialState::PatchCaptured => true, // agent already ran; eval runs at end if needed
        TrialState::Planned
        | TrialState::BootFailed
        | TrialState::CloneFailed
        | TrialState::AgentFailed
        | TrialState::Running => false,
    }
}

/// Derive the manifest state from a completed run result.
fn trial_state_from_result(result: &PerRunResult) -> (TrialState, Option<String>) {
    if !result.error.is_empty() {
        if result.error.contains("boot failed") {
            (TrialState::BootFailed, Some(result.error.clone()))
        } else if result.error.contains("clone failed") {
            (TrialState::CloneFailed, Some(result.error.clone()))
        } else {
            (TrialState::AgentFailed, Some(result.error.clone()))
        }
    } else if result.exit_code != 0 || result.timed_out {
        (TrialState::AgentFailed, None)
    } else {
        (TrialState::PatchCaptured, None)
    }
}

/// Reconstruct a `PerRunResult` from disk for a trial that is being skipped
/// during resume. Falls back to a synthetic record if `result.json` is missing.
fn reconstruct_result_from_disk(
    opts: &SwebenchProOpts,
    spec: &QuantSpec,
    inst: &Instance,
    trial: u32,
) -> Result<PerRunResult> {
    let trial_dir = trial_dir_for(&opts.output, spec.label, &inst.instance_id, trial);
    let result_path = trial_dir.join("result.json");
    if result_path.exists() {
        let bytes = std::fs::read(&result_path)?;
        let result: PerRunResult = serde_json::from_slice(&bytes)?;
        return Ok(result);
    }

    let pred_path = trial_dir.join(format!("{}.pred", inst.instance_id));
    let (bytes, lines) = if pred_path.exists() {
        let bytes = std::fs::metadata(&pred_path)
            .map(|m| m.len() as usize)
            .unwrap_or(0);
        let lines = std::fs::read_to_string(&pred_path)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        (bytes, lines)
    } else {
        (0, 0)
    };

    Ok(PerRunResult {
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
    })
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

/// Create a fresh manifest pre-populated with every trial in `Planned` state.
fn create_manifest(
    opts: &SwebenchProOpts,
    quants: &[String],
    instances: &[Instance],
    trials: u32,
) -> Result<SweepManifest> {
    let mut trial_manifests = Vec::with_capacity(quants.len() * instances.len() * trials as usize);
    for quant in quants {
        for inst in instances {
            for trial in 1..=trials {
                trial_manifests.push(TrialManifest {
                    quant: quant.clone(),
                    instance_id: inst.instance_id.clone(),
                    trial,
                    state: TrialState::Planned,
                    started_at: None,
                    completed_at: None,
                    error: None,
                    pred_path: None,
                    result_path: None,
                });
            }
        }
    }
    Ok(SweepManifest {
        created_at: Utc::now().to_rfc3339(),
        opts: opts_to_snapshot(opts),
        trials: trial_manifests,
    })
}

/// Update a single trial entry in the manifest (in-memory only).
fn update_manifest_entry(
    manifest: &mut SweepManifest,
    quant: &str,
    instance_id: &str,
    trial: u32,
    state: TrialState,
    error: Option<String>,
    pred_path: Option<PathBuf>,
    result_path: Option<PathBuf>,
) {
    let now = Utc::now().to_rfc3339();
    if let Some(t) = manifest.find_trial_mut(quant, instance_id, trial) {
        t.state = state;
        t.completed_at = Some(now);
        t.error = error;
        t.pred_path = pred_path;
        t.result_path = result_path;
    }
}

/// Run the SWE-bench Pro harness end-to-end.  Always returns `Ok(())` if the
/// run scheduler completed (individual instance/quant failures are recorded in
/// `result.json`); any infrastructure-level error (no quants, dataset load
/// failure, output dir unwritable) bails.
pub fn run_swebench_pro(opts: SwebenchProOpts) -> Result<()> {
    if opts.official_eval && opts.prompt_mode != "official" {
        bail!(
            "--official-eval requires --prompt-mode official; diagnostic prompts include oracle test fields"
        );
    }

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
            "leaky_oracle_prompt": opts.prompt_mode != "official",
            "llama_server_binary": opts.llama_opts.binary,
            "llama_server_argv": llama_server_argv,
            "official_eval": opts.official_eval,
            "official_eval_script": opts.official_eval_script,
            "official_eval_raw_sample_path": opts.official_eval_raw_sample_path,
            "official_eval_scripts_dir": opts.official_eval_scripts_dir,
            "official_eval_dockerhub_username": opts.official_eval_dockerhub_username,
            "official_eval_num_workers": opts.official_eval_num_workers,
            "official_eval_use_local_docker": opts.official_eval_use_local_docker,
        }))?,
    )?;

    let trials = opts.trials.max(1);
    let mut all_runs: Vec<PerRunResult> = Vec::new();
    let overall_started = Instant::now();

    // ── Manifest lifecycle ──
    let manifest_path = opts.output.join("manifest.json");
    let manifest = if manifest_path.exists() && (opts.resume || opts.force_rerun) {
        let existing = SweepManifest::load(&manifest_path)?;
        let snapshot = opts_to_snapshot(&opts);
        if existing.opts != snapshot {
            bail!(
                "manifest opts mismatch: existing manifest was created with different options. \
                 Use a different --output directory or delete {} to start fresh.",
                manifest_path.display()
            );
        }
        eprintln!(
            "Resuming from existing manifest ({} trials)",
            existing.trials.len()
        );
        existing
    } else if manifest_path.exists() {
        // Auto-detect resume when manifest exists and opts match.
        let existing = SweepManifest::load(&manifest_path)?;
        let snapshot = opts_to_snapshot(&opts);
        if existing.opts == snapshot {
            eprintln!(
                "Auto-resuming from existing manifest ({} trials)",
                existing.trials.len()
            );
            existing
        } else {
            eprintln!("Warning: existing manifest has different opts; starting fresh sweep.");
            create_manifest(&opts, &valid_quants, &instances, trials)?
        }
    } else {
        create_manifest(&opts, &valid_quants, &instances, trials)?
    };

    // Seed all_runs with results from previously-completed trials so aggregate
    // and patches.json reflect the full sweep even when resuming.
    for t in &manifest.trials {
        if matches!(
            t.state,
            TrialState::Evaluated
                | TrialState::PatchCaptured
                | TrialState::BootFailed
                | TrialState::CloneFailed
                | TrialState::AgentFailed
        ) {
            // Try to load the existing result.json; if missing, synthesise.
            if let Some(ref result_path) = t.result_path {
                if result_path.exists() {
                    if let Ok(bytes) = std::fs::read(result_path) {
                        if let Ok(result) = serde_json::from_slice::<PerRunResult>(&bytes) {
                            all_runs.push(result);
                            continue;
                        }
                    }
                }
            }
            // Find the instance to build a synthetic result.
            if let Some(inst) = instances.iter().find(|i| i.instance_id == t.instance_id) {
                if let Some(spec) = catalog.get(t.quant.as_str()) {
                    if let Ok(synthetic) =
                        reconstruct_result_from_disk(&opts, &spec.clone(), inst, t.trial)
                    {
                        all_runs.push(synthetic);
                    }
                }
            }
        }
    }

    let manifest_arc = std::sync::Arc::new(std::sync::Mutex::new(manifest));

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
                // point of view, even if the server never came up.
                let mut m = manifest_arc.lock().unwrap();
                for trial in 1..=trials {
                    for inst in &instances {
                        match record_boot_failure(&opts, &spec, inst, trial, &e.to_string()) {
                            Ok(res) => {
                                all_runs.push(res.clone());
                                update_manifest_entry(
                                    &mut m,
                                    spec.label,
                                    &inst.instance_id,
                                    trial,
                                    TrialState::BootFailed,
                                    Some(res.error),
                                    Some(res.pred_path),
                                    Some(
                                        trial_dir_for(
                                            &opts.output,
                                            spec.label,
                                            &inst.instance_id,
                                            trial,
                                        )
                                        .join("result.json"),
                                    ),
                                );
                            }
                            Err(rec_err) => eprintln!(
                                "    failed to record boot failure for {} trial {}: {}",
                                inst.instance_id, trial, rec_err
                            ),
                        }
                    }
                }
                if let Err(we) = m.write_atomic(&manifest_path) {
                    eprintln!("    failed to write manifest after boot failure: {}", we);
                }
                continue;
            }
        };

        let concurrency = opts.concurrency.max(1) as usize;
        for trial in 1..=trials {
            let trial_results = run_trial(
                &opts,
                &spec,
                &instances,
                trial,
                concurrency,
                &manifest_arc,
                &manifest_path,
            );
            all_runs.extend(trial_results);
        }

        // Drop server explicitly so subsequent quant boot has a clean GPU.
        drop(server);
    }

    LlamaServer::stop_existing();

    // Always collate patches even if some quants failed.
    write_patches_json(&opts, &all_runs)?;
    let official_eval = if opts.official_eval {
        Some(run_official_eval(&opts, &all_runs)?)
    } else {
        None
    };

    // After official eval, mark PatchCaptured trials as Evaluated.
    if opts.official_eval {
        let mut m = manifest_arc.lock().unwrap();
        for t in &mut m.trials {
            if t.state == TrialState::PatchCaptured {
                t.state = TrialState::Evaluated;
                t.completed_at = Some(Utc::now().to_rfc3339());
            }
        }
        if let Err(we) = m.write_atomic(&manifest_path) {
            eprintln!("    failed to write manifest after eval: {}", we);
        }
    }

    write_aggregate(&opts.output, &all_runs, official_eval.as_ref())?;

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
    manifest: &std::sync::Arc<std::sync::Mutex<SweepManifest>>,
    manifest_path: &Path,
) -> Vec<PerRunResult> {
    if concurrency <= 1 {
        return instances
            .iter()
            .filter_map(|inst| {
                // Check manifest state before running.
                {
                    let m = manifest.lock().unwrap();
                    if let Some(t) = m.find_trial(spec.label, &inst.instance_id, trial) {
                        if should_skip_trial(opts, Some(t)) {
                            eprintln!(
                                "  → {} (trial {}): SKIP (manifest: {:?})",
                                inst.instance_id, trial, t.state
                            );
                            return match reconstruct_result_from_disk(opts, spec, inst, trial) {
                                Ok(res) => Some(res),
                                Err(e) => {
                                    eprintln!(
                                        "    {} trial {}: failed to reconstruct skipped result: {}",
                                        inst.instance_id, trial, e
                                    );
                                    None
                                }
                            };
                        }
                    }
                }

                match run_one(opts, spec, inst, trial) {
                    Ok(res) => {
                        let (state, error) = trial_state_from_result(&res);
                        {
                            let mut m = manifest.lock().unwrap();
                            update_manifest_entry(
                                &mut m,
                                spec.label,
                                &inst.instance_id,
                                trial,
                                state,
                                error,
                                Some(res.pred_path.clone()),
                                Some(
                                    trial_dir_for(
                                        &opts.output,
                                        spec.label,
                                        &inst.instance_id,
                                        trial,
                                    )
                                    .join("result.json"),
                                ),
                            );
                            if let Err(we) = m.write_atomic(manifest_path) {
                                eprintln!("    failed to write manifest: {}", we);
                            }
                        }
                        Some(res)
                    }
                    Err(e) => {
                        eprintln!("    {} trial {}: error: {}", inst.instance_id, trial, e);
                        None
                    }
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
    let manifest_arc = Arc::clone(manifest);
    let manifest_path_buf = manifest_path.to_path_buf();
    let output_arc = Arc::new(opts.output.clone());

    std::thread::scope(|scope| {
        let n = concurrency.min(instances.len()).max(1);
        let mut handles = Vec::with_capacity(n);
        for _ in 0..n {
            let queue = Arc::clone(&queue);
            let results = Arc::clone(&results);
            let opts_c = Arc::clone(&opts_arc);
            let spec_c = Arc::clone(&spec_arc);
            let instances_c = Arc::clone(&instances_arc);
            let manifest_c = Arc::clone(&manifest_arc);
            let manifest_path_c = manifest_path_buf.clone();
            let output_c = Arc::clone(&output_arc);
            handles.push(scope.spawn(move || loop {
                let idx = {
                    let mut q = queue.lock().unwrap();
                    q.pop()
                };
                let Some(idx) = idx else { return };
                let inst = &instances_c[idx];

                // Check manifest state before running.
                {
                    let m = manifest_c.lock().unwrap();
                    if let Some(t) = m.find_trial(spec_c.label, &inst.instance_id, trial) {
                        if should_skip_trial(&opts_c, Some(t)) {
                            eprintln!(
                                "  → {} (trial {}): SKIP (manifest: {:?})",
                                inst.instance_id, trial, t.state
                            );
                            if let Ok(res) =
                                reconstruct_result_from_disk(&opts_c, &spec_c, inst, trial)
                            {
                                results.lock().unwrap().push(res);
                            }
                            continue;
                        }
                    }
                }

                match run_one(&opts_c, &spec_c, inst, trial) {
                    Ok(res) => {
                        let (state, error) = trial_state_from_result(&res);
                        {
                            let mut m = manifest_c.lock().unwrap();
                            update_manifest_entry(
                                &mut m,
                                spec_c.label,
                                &inst.instance_id,
                                trial,
                                state,
                                error,
                                Some(res.pred_path.clone()),
                                Some(
                                    trial_dir_for(
                                        &output_c,
                                        spec_c.label,
                                        &inst.instance_id,
                                        trial,
                                    )
                                    .join("result.json"),
                                ),
                            );
                            if let Err(we) = m.write_atomic(&manifest_path_c) {
                                eprintln!("    failed to write manifest: {}", we);
                            }
                        }
                        results.lock().unwrap().push(res);
                    }
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

    let mut run_trace = RunTrace::new(
        format!("{}-{}-{}", spec.label, inst.instance_id, trial),
        inst.instance_id.clone(),
        spec.label.into(),
        trial,
    );

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
    let mut prompt = profile.task_prompt(inst, &opts.prompt_mode);

    // In diagnostic mode, run localize_issue and inject top candidates.
    if opts.prompt_mode == "diagnostic" {
        match crate::tools::localize_issue::localize_issue_sync(
            &inst.problem_statement,
            workdir.to_str().unwrap_or("."),
        ) {
            Ok(candidates) if !candidates.is_empty() => {
                let top: Vec<String> = candidates
                    .iter()
                    .take(3)
                    .map(|c| {
                        if c.function.is_empty() {
                            format!("- {} (score: {:.1}, {})", c.file, c.score, c.reason)
                        } else {
                            format!(
                                "- {}::{} (score: {:.1}, {})",
                                c.file, c.function, c.score, c.reason
                            )
                        }
                    })
                    .collect();
                prompt.push_str("\n\nSuggested files to investigate:\n");
                prompt.push_str(&top.join("\n"));
                prompt.push('\n');
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("    localize_issue failed: {}", e);
            }
        }
    }

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

    // Load trace events written by the subprocess and enrich them.
    let trace_path = trial_dir.join("trace.jsonl");
    if trace_path.exists() {
        if let Ok(loaded) = RunTrace::read_jsonl(&trace_path) {
            run_trace.events = loaded.events;
        }
    }
    run_trace.emit(TraceEvent::PatchCaptured {
        patch_lines,
        patch_bytes,
    });
    if let Ok(fm_content) = std::fs::read_to_string(trial_dir.join("failure_mode.json")) {
        if let Ok(fm) = serde_json::from_str::<serde_json::Value>(&fm_content) {
            if let Some(kind) = fm.get("kind").and_then(|v| v.as_str()) {
                let evidence = fm
                    .get("evidence")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                run_trace.emit(TraceEvent::FailureClassified {
                    kind: kind.to_string(),
                    evidence,
                });
            }
        }
    }
    if let Err(e) = run_trace.write_jsonl(&trace_path) {
        eprintln!("    failed to write trace.jsonl: {}", e);
    }

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
    /// Official Docker eval fields.  These are only meaningful when
    /// `--official-eval --prompt-mode official` was used.
    eval_completed: bool,
    patch_applied: bool,
    f2p_p2p_passed: bool,
    resolved: bool,
    official_resolution_rate: f64,
    #[serde(skip_serializing_if = "String::is_empty")]
    eval_error: String,
}

#[derive(Serialize)]
struct AggregateReport {
    generated_at: String,
    total_runs: usize,
    attempted_patch_rate: f64,
    official_eval_completed: bool,
    official_resolution_rate: f64,
    entries: Vec<AggregateEntry>,
}

#[derive(Clone, Debug, Default)]
struct OfficialEvalStatus {
    eval_completed: bool,
    patch_applied: bool,
    f2p_p2p_passed: bool,
    resolved: bool,
    eval_error: String,
}

#[derive(Clone, Debug, Default)]
struct OfficialEvalMap {
    by_pair: BTreeMap<(String, String), OfficialEvalStatus>,
}

fn write_aggregate(
    output: &Path,
    runs: &[PerRunResult],
    official_eval: Option<&OfficialEvalMap>,
) -> Result<()> {
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

        let official = official_eval
            .and_then(|m| m.by_pair.get(&(quant.clone(), instance_id.clone())))
            .cloned()
            .unwrap_or_default();

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
            eval_completed: official.eval_completed,
            patch_applied: official.patch_applied,
            f2p_p2p_passed: official.f2p_p2p_passed,
            resolved: official.resolved,
            official_resolution_rate: if official.resolved { 1.0 } else { 0.0 },
            eval_error: official.eval_error,
        });
    }

    let attempted_patch_total = runs
        .iter()
        .filter(|r| r.exit_code == 0 && !r.timed_out && r.patch_bytes > 0)
        .count();
    let attempted_patch_rate = if runs.is_empty() {
        0.0
    } else {
        attempted_patch_total as f64 / runs.len() as f64
    };

    let official_eval_completed = official_eval
        .map(|m| !m.by_pair.is_empty() && m.by_pair.values().all(|s| s.eval_completed))
        .unwrap_or(false);
    let official_resolution_rate = official_eval
        .map(|m| {
            if m.by_pair.is_empty() {
                0.0
            } else {
                m.by_pair.values().filter(|s| s.resolved).count() as f64 / m.by_pair.len() as f64
            }
        })
        .unwrap_or(0.0);

    let report = AggregateReport {
        generated_at: Utc::now().to_rfc3339(),
        total_runs: runs.len(),
        attempted_patch_rate,
        official_eval_completed,
        official_resolution_rate,
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

fn best_runs_by_quant_instance(runs: &[PerRunResult]) -> BTreeMap<(String, String), &PerRunResult> {
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
    best
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
        patch: String,
        prefix: String,
        model_name_or_path: String,
        model_patch: String,
        trial: u32,
    }

    let best = best_runs_by_quant_instance(runs);

    let mut preds = Vec::new();
    for ((quant, instance_id), r) in best {
        let patch = std::fs::read_to_string(&r.pred_path).unwrap_or_default();
        preds.push(Pred {
            instance_id,
            patch: patch.clone(),
            prefix: quant.clone(),
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
/// per-quant `patches.json` files.  The evaluator's `eval_results.json` is
/// keyed only by instance_id, so running each quant separately avoids collisions
/// when the same instance appears for multiple quants.
fn run_official_eval(opts: &SwebenchProOpts, runs: &[PerRunResult]) -> Result<OfficialEvalMap> {
    #[derive(Serialize)]
    struct Pred {
        instance_id: String,
        patch: String,
        prefix: String,
    }

    #[derive(Serialize)]
    struct OfficialEvalQuantSummary {
        quant: String,
        eval_dir: PathBuf,
        patch_path: PathBuf,
        exit_code: Option<i32>,
        eval_completed: bool,
        evaluated: usize,
        resolved: usize,
        eval_error: String,
    }

    if !opts.official_eval_script.exists() {
        bail!(
            "official eval script not found: {}",
            opts.official_eval_script.display()
        );
    }
    if !opts.official_eval_raw_sample_path.exists() {
        bail!(
            "official eval raw sample not found: {}",
            opts.official_eval_raw_sample_path.display()
        );
    }
    if !opts.official_eval_scripts_dir.exists() {
        bail!(
            "official eval scripts dir not found: {}",
            opts.official_eval_scripts_dir.display()
        );
    }

    let best = best_runs_by_quant_instance(runs);
    let mut by_quant: BTreeMap<String, Vec<&PerRunResult>> = BTreeMap::new();
    for ((quant, _instance_id), run) in best {
        by_quant.entry(quant).or_default().push(run);
    }

    let eval_root = opts.output.join("eval");
    std::fs::create_dir_all(&eval_root)
        .with_context(|| format!("creating {}", eval_root.display()))?;

    let mut statuses = OfficialEvalMap::default();
    let mut summaries = Vec::new();
    eprintln!("  → running official eval per quant...");

    for (quant, quant_runs) in by_quant {
        let safe_quant = safe_path_component(&quant);
        let eval_dir = eval_root.join(&safe_quant).join("trial_best");
        std::fs::create_dir_all(&eval_dir)
            .with_context(|| format!("creating {}", eval_dir.display()))?;
        let patch_path = eval_dir.join("patches.json");

        let mut preds = Vec::new();
        for run in &quant_runs {
            let patch = std::fs::read_to_string(&run.pred_path).unwrap_or_default();
            preds.push(Pred {
                instance_id: run.instance_id.clone(),
                patch,
                prefix: safe_quant.clone(),
            });
        }
        std::fs::write(&patch_path, serde_json::to_vec_pretty(&preds)?)?;

        let mut cmd = std::process::Command::new("python3");
        cmd.arg(&opts.official_eval_script)
            .arg("--raw_sample_path")
            .arg(&opts.official_eval_raw_sample_path)
            .arg("--patch_path")
            .arg(&patch_path)
            .arg("--output_dir")
            .arg(&eval_dir)
            .arg("--scripts_dir")
            .arg(&opts.official_eval_scripts_dir)
            .arg("--dockerhub_username")
            .arg(&opts.official_eval_dockerhub_username)
            .arg("--num_workers")
            .arg(opts.official_eval_num_workers.max(1).to_string());
        if opts.official_eval_use_local_docker {
            cmd.arg("--use_local_docker");
        }
        if opts.official_eval_redo {
            cmd.arg("--redo");
        }
        if opts.official_eval_block_network {
            cmd.arg("--block_network");
        }
        if let Some(parent) = opts.official_eval_script.parent() {
            cmd.current_dir(parent);
        }

        let output = cmd
            .output()
            .with_context(|| format!("spawning {}", opts.official_eval_script.display()))?;
        let exit_code = output.status.code();
        let eval_error = if output.status.success() {
            String::new()
        } else {
            String::from_utf8_lossy(&output.stderr).trim().to_string()
        };
        std::fs::write(
            eval_dir.join("eval_invocation.json"),
            serde_json::to_vec_pretty(&json!({
                "script": opts.official_eval_script,
                "raw_sample_path": opts.official_eval_raw_sample_path,
                "patch_path": patch_path,
                "output_dir": eval_dir,
                "scripts_dir": opts.official_eval_scripts_dir,
                "dockerhub_username": opts.official_eval_dockerhub_username,
                "num_workers": opts.official_eval_num_workers.max(1),
                "use_local_docker": opts.official_eval_use_local_docker,
                "exit_code": exit_code,
                "stdout": String::from_utf8_lossy(&output.stdout),
                "stderr": String::from_utf8_lossy(&output.stderr),
            }))?,
        )?;

        let eval_results_path = eval_dir.join("eval_results.json");
        let eval_results: BTreeMap<String, bool> = std::fs::read_to_string(&eval_results_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let eval_completed = output.status.success() && !eval_results.is_empty();

        for run in &quant_runs {
            let resolved = eval_results
                .get(run.instance_id.as_str())
                .copied()
                .unwrap_or(false);
            statuses.by_pair.insert(
                (quant.clone(), run.instance_id.clone()),
                OfficialEvalStatus {
                    eval_completed,
                    patch_applied: eval_results.contains_key(run.instance_id.as_str()),
                    f2p_p2p_passed: resolved,
                    resolved,
                    eval_error: if eval_completed {
                        String::new()
                    } else {
                        eval_error.clone()
                    },
                },
            );
        }

        summaries.push(OfficialEvalQuantSummary {
            quant,
            eval_dir,
            patch_path,
            exit_code,
            eval_completed,
            evaluated: eval_results.len(),
            resolved: eval_results.values().filter(|v| **v).count(),
            eval_error,
        });
    }

    let total_evaluated: usize = summaries.iter().map(|s| s.evaluated).sum();
    let total_resolved: usize = summaries.iter().map(|s| s.resolved).sum();
    std::fs::write(
        eval_root.join("official_eval_summary.json"),
        serde_json::to_vec_pretty(&json!({
            "generated_at": Utc::now().to_rfc3339(),
            "total_evaluated": total_evaluated,
            "total_resolved": total_resolved,
            "official_resolution_rate": if total_evaluated == 0 {
                0.0
            } else {
                total_resolved as f64 / total_evaluated as f64
            },
            "quants": summaries,
        }))?,
    )?;

    Ok(statuses)
}

fn safe_path_component(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
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
