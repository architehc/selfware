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
use crate::memory::swebench::{
    build_memory_prompt_block, index_repo, load_repo_index, save_repo_index, MemoryKey,
    SwebenchInstanceMemory,
};
use crate::memory::swebench_store::SwebenchMemoryStore;

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
    pub candidates: u32,
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

fn is_zero(n: &u32) -> bool {
    *n == 0
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
        candidates: opts.candidates,
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
    } else if result.patch_bytes > 0 {
        // A nonzero agent exit can still leave an evaluable patch. Treat it as
        // captured so resume/eval do not rerun and double-count the trial.
        (TrialState::PatchCaptured, None)
    } else if result.exit_code == 0 && !result.timed_out {
        (TrialState::PatchCaptured, None)
    } else {
        (TrialState::AgentFailed, None)
    }
}

/// Reconstruct a `PerRunResult` from disk for a trial that is being skipped
/// during resume. Missing `result.json` is treated as an unknown failed run:
/// a stale `.pred` proves only that a patch file exists, not that the agent
/// completed successfully.
fn reconstruct_result_from_disk(
    opts: &SwebenchProOpts,
    spec: &QuantSpec,
    inst: &Instance,
    trial: u32,
) -> Result<PerRunResult> {
    let trial_dir = trial_dir_for(&opts.output, &spec.label, &inst.instance_id, trial);
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

    let error = if pred_path.exists() {
        format!(
            "resume skipped stale patch without result.json: {}",
            result_path.display()
        )
    } else {
        format!(
            "resume skipped trial without result.json: {}",
            result_path.display()
        )
    };

    Ok(PerRunResult {
        instance_id: inst.instance_id.clone(),
        quant: spec.label.clone(),
        trial,
        exit_code: 1,
        timed_out: false,
        wall_secs: 0.0,
        patch_lines: lines,
        patch_bytes: bytes,
        pred_path,
        error,
        empty_diff: lines == 0 && bytes == 0,
        test_only_patch: false,
        has_source_edit: false,
        has_test_edit: false,
        syntax_check_passed: false,
        candidate_num: 0,
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
    #[serde(default, skip_serializing_if = "is_false")]
    has_test_edit: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    syntax_check_passed: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    candidate_num: u32,
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
            if catalog.contains_key(*q) {
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
                label: "plan-dummy".into(),
                gguf: "dummy.gguf".into(),
                alias: "plan-dummy".into(),
                mmproj: "dummy.mmproj.gguf".into(),
                name: "plan-dummy".into(),
                ctx: opts.ctx,
                max_parallel: opts.parallel,
                kv_cache_type: "q8_0".into(),
                tensor_split: opts.llama_opts.tensor_split.clone(),
                temperature: 1.0,
                thinking_policy: super::catalog::ThinkingPolicy::Disable,
                backend: super::catalog::BackendProfile::LlamaCpp,
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
                if let Some(spec) = catalog.get(&t.quant) {
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
        let spec = catalog.get(quant).expect("validated above").clone();

        eprintln!("{}", "=".repeat(70));
        eprintln!("QUANT: {}", quant);
        eprintln!("{}", "=".repeat(70));

        // Apply per-quant scheduling overrides.
        let mut quant_llama_opts = opts.llama_opts.clone();
        if spec.ctx > 0 {
            quant_llama_opts.ctx = spec.ctx;
        }
        if spec.max_parallel > 0 {
            quant_llama_opts.parallel = spec.max_parallel;
        }
        if let Some(ref ts) = spec.tensor_split {
            quant_llama_opts.tensor_split = Some(ts.clone());
        }

        // Concurrency clamping: never exceed the quant's max_parallel.
        let effective_concurrency = if opts.concurrency > spec.max_parallel {
            eprintln!(
                "  ⚠ concurrency {} exceeds max_parallel {} for quant {} — clamping to {}",
                opts.concurrency, spec.max_parallel, spec.label, spec.max_parallel
            );
            spec.max_parallel
        } else {
            opts.concurrency
        };
        let concurrency = effective_concurrency.max(1) as usize;

        LlamaServer::stop_existing();
        let server = match LlamaServer::boot(&spec, &quant_llama_opts) {
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
                                    &spec.label,
                                    &inst.instance_id,
                                    trial,
                                    TrialState::BootFailed,
                                    Some(res.error),
                                    Some(res.pred_path),
                                    Some(
                                        trial_dir_for(
                                            &opts.output,
                                            &spec.label,
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

        // Detect backend after successful boot and annotate plan.json.
        let endpoint = format!("http://127.0.0.1:{}/v1", opts.llama_opts.port);
        match crate::api::client::detect_backend(&endpoint) {
            Ok(backend) => {
                if let Ok(bytes) = std::fs::read(&plan_path) {
                    if let Ok(mut plan) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                        if let Some(obj) = plan.as_object_mut() {
                            obj.insert("detected_backend".into(), backend.into());
                            let _ = std::fs::write(
                                &plan_path,
                                serde_json::to_vec_pretty(&plan).unwrap_or_default(),
                            );
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("  ⚠ backend detection failed: {}", e);
            }
        }

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
        let mut out = Vec::new();
        for inst in instances {
            // Check manifest state before running.
            {
                let m = manifest.lock().unwrap();
                if let Some(t) = m.find_trial(&spec.label, &inst.instance_id, trial) {
                    if should_skip_trial(opts, Some(t)) {
                        eprintln!(
                            "  → {} (trial {}): SKIP (manifest: {:?})",
                            inst.instance_id, trial, t.state
                        );
                        match reconstruct_result_from_disk(opts, spec, inst, trial) {
                            Ok(res) => out.push(res),
                            Err(e) => {
                                eprintln!(
                                    "    {} trial {}: failed to reconstruct skipped result: {}",
                                    inst.instance_id, trial, e
                                );
                            }
                        }
                        continue;
                    }
                }
            }

            match run_one(opts, spec, inst, trial) {
                Ok((selected, mut candidates)) => {
                    let (state, error) = trial_state_from_result(&selected);
                    {
                        let mut m = manifest.lock().unwrap();
                        update_manifest_entry(
                            &mut m,
                            &spec.label,
                            &inst.instance_id,
                            trial,
                            state,
                            error,
                            Some(selected.pred_path.clone()),
                            Some(
                                trial_dir_for(&opts.output, &spec.label, &inst.instance_id, trial)
                                    .join("result.json"),
                            ),
                        );
                        if let Err(we) = m.write_atomic(manifest_path) {
                            eprintln!("    failed to write manifest: {}", we);
                        }
                    }
                    out.push(selected);
                    out.append(&mut candidates);
                }
                Err(e) => {
                    eprintln!("    {} trial {}: error: {}", inst.instance_id, trial, e);
                }
            }
        }
        return out;
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
                    if let Some(t) = m.find_trial(&spec_c.label, &inst.instance_id, trial) {
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
                    Ok((selected, mut candidates)) => {
                        let (state, error) = trial_state_from_result(&selected);
                        {
                            let mut m = manifest_c.lock().unwrap();
                            update_manifest_entry(
                                &mut m,
                                &spec_c.label,
                                &inst.instance_id,
                                trial,
                                state,
                                error,
                                Some(selected.pred_path.clone()),
                                Some(
                                    trial_dir_for(
                                        &output_c,
                                        &spec_c.label,
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
                        let mut r = results.lock().unwrap();
                        r.push(selected);
                        r.append(&mut candidates);
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
    let trial_dir = trial_dir_for(&opts.output, &spec.label, &inst.instance_id, trial);
    std::fs::create_dir_all(&trial_dir)
        .with_context(|| format!("creating {}", trial_dir.display()))?;
    let pred_path = trial_dir.join(format!("{}.pred", inst.instance_id));
    // Touch an empty pred so downstream tooling expecting the file finds it.
    if !pred_path.exists() {
        std::fs::write(&pred_path, "")?;
    }
    let result = PerRunResult {
        instance_id: inst.instance_id.clone(),
        quant: spec.label.clone(),
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
        has_test_edit: false,
        syntax_check_passed: false,
        candidate_num: 0,
    };
    std::fs::write(
        trial_dir.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    Ok(result)
}

/// Run a single candidate in `candidate_dir`.  When `candidate == 0` this is
/// the legacy single-candidate path and `candidate_dir` is the trial dir.
fn run_one_candidate(
    opts: &SwebenchProOpts,
    spec: &QuantSpec,
    inst: &Instance,
    trial: u32,
    candidate: u32,
    candidate_dir: &Path,
) -> Result<PerRunResult> {
    std::fs::create_dir_all(candidate_dir)
        .with_context(|| format!("creating {}", candidate_dir.display()))?;
    let pred_path = candidate_dir.join(format!("{}.pred", inst.instance_id));

    let run_id = if candidate > 0 {
        format!(
            "{}-{}-{}-c{}",
            spec.label, inst.instance_id, trial, candidate
        )
    } else {
        format!("{}-{}-{}", spec.label, inst.instance_id, trial)
    };
    let mut run_trace = RunTrace::new(run_id, inst.instance_id.clone(), spec.label.clone(), trial);

    if opts.skip_existing && pred_path.exists() {
        eprintln!(
            "  → {} (trial {} candidate {}): SKIP (pred exists)",
            inst.instance_id, trial, candidate
        );
        let result_path = candidate_dir.join("result.json");
        if result_path.exists() {
            let bytes = std::fs::read(&result_path)?;
            let result: PerRunResult = serde_json::from_slice(&bytes)?;
            return Ok(result);
        }
        let bytes = std::fs::metadata(&pred_path)
            .map(|m| m.len() as usize)
            .unwrap_or(0);
        let lines = std::fs::read_to_string(&pred_path)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        return Ok(PerRunResult {
            instance_id: inst.instance_id.clone(),
            quant: spec.label.clone(),
            trial,
            exit_code: 1,
            timed_out: false,
            wall_secs: 0.0,
            patch_lines: lines,
            patch_bytes: bytes,
            pred_path,
            error: format!(
                "skip-existing found .pred without result.json: {}",
                result_path.display()
            ),
            empty_diff: lines == 0 && bytes == 0,
            test_only_patch: false,
            has_source_edit: false,
            has_test_edit: false,
            syntax_check_passed: false,
            candidate_num: candidate,
        });
    }

    if candidate > 0 {
        eprintln!(
            "  → {} (trial {} candidate {})",
            inst.instance_id, trial, candidate
        );
    } else {
        eprintln!("  → {} (trial {})", inst.instance_id, trial);
    }

    let workdir = candidate_dir.join("repo");
    if let Err(e) = clone_instance(&inst.repo, &inst.base_commit, &workdir) {
        eprintln!("    clone failed: {}", e);
        let pred_path = candidate_dir.join(format!("{}.pred", inst.instance_id));
        if !pred_path.exists() {
            std::fs::write(&pred_path, "")?;
        }
        let result = PerRunResult {
            instance_id: inst.instance_id.clone(),
            quant: spec.label.clone(),
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
            has_test_edit: false,
            syntax_check_passed: false,
            candidate_num: candidate,
        };
        std::fs::write(
            candidate_dir.join("result.json"),
            serde_json::to_vec_pretty(&result)?,
        )?;
        return Ok(result);
    }

    // ── Memory store & repo index ──
    let memory_dir = opts.output.join(".selfware").join("memory");
    let store = SwebenchMemoryStore::load(&memory_dir).ok();

    // Cheap repo index on first use.
    let index = if let Some(ref idx) = load_repo_index(&inst.repo, &inst.base_commit, &memory_dir) {
        Some(idx.clone())
    } else {
        match index_repo(&inst.repo, &inst.base_commit, &workdir) {
            Ok(idx) => {
                if let Err(e) = save_repo_index(&idx, &memory_dir) {
                    eprintln!("    failed to save repo index: {}", e);
                }
                Some(idx)
            }
            Err(e) => {
                eprintln!("    repo indexing failed: {}", e);
                None
            }
        }
    };

    let profile = match opts.prompt_profile.as_str() {
        "swebench_pro" => PromptProfile::SwebenchPro,
        _ => PromptProfile::Default,
    };
    let mut prompt = profile.task_prompt(inst, &opts.prompt_mode);

    // In diagnostic mode, run localize_issue and inject top candidates.
    let mut candidate_files: Vec<String> = Vec::new();
    if opts.prompt_mode == "diagnostic" {
        match crate::tools::localize_issue::localize_issue_sync(
            &inst.problem_statement,
            workdir.to_str().unwrap_or("."),
        ) {
            Ok(candidates) if !candidates.is_empty() => {
                candidate_files = candidates.iter().map(|c| c.file.clone()).collect();
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

    // ── Memory injection (leakage-controlled) ──
    let official_mode = opts.prompt_mode == "official";
    let instance_key = MemoryKey::new(
        &inst.repo,
        &inst.base_commit,
        &inst.instance_id,
        &spec.label,
        trial,
    );
    let instance_memory = store.as_ref().and_then(|s| s.get_instance(&instance_key));
    let repo_lessons = store
        .as_ref()
        .map(|s| s.get_repo_lessons(&inst.repo))
        .unwrap_or_default();

    // Seed candidates from index test files when localize_issue didn't run.
    if candidate_files.is_empty() {
        if let Some(ref idx) = index {
            candidate_files = idx.test_files.clone();
        }
    }

    if let Some(block) = build_memory_prompt_block(
        instance_memory.as_ref(),
        &repo_lessons,
        official_mode,
        trial,
        &candidate_files,
    ) {
        // Inject the memory block before the issue description.
        prompt = block + &prompt;
    }

    std::fs::write(candidate_dir.join("prompt.txt"), &prompt)?;
    std::fs::write(
        candidate_dir.join("instance.json"),
        serde_json::to_vec_pretty(inst)?,
    )?;

    let log_path = candidate_dir.join("agent.log");
    let endpoint = format!("http://127.0.0.1:{}/v1", opts.llama_opts.port);
    let outcome = run_selfware(
        &opts.selfware_bin,
        &workdir,
        &prompt,
        &spec.alias,
        &endpoint,
        opts.scenario_timeout,
        &log_path,
        candidate_dir,
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

    let (patch_lines, patch_bytes) = outcome
        .parsed_result
        .as_ref()
        .map(|r| (r.patch_lines, r.patch_bytes))
        .unwrap_or_else(|| (patch.lines().count(), patch.len()));

    let empty_diff = patch.trim().is_empty();
    let test_only_patch = !empty_diff && is_test_only_patch(&patch);
    let has_source_edit = !empty_diff && has_source_edit_in_patch(&patch);
    let has_test_edit = !empty_diff && has_test_edit_in_patch(&patch);
    let syntax_check_passed = cheap_syntax_check(&patch);

    let trace_path = candidate_dir.join("trace.jsonl");
    if trace_path.exists() {
        if let Ok(loaded) = RunTrace::read_jsonl(&trace_path) {
            run_trace.events = loaded.events;
        }
    }
    run_trace.emit(TraceEvent::PatchCaptured {
        patch_lines,
        patch_bytes,
    });
    if let Ok(fm_content) = std::fs::read_to_string(candidate_dir.join("failure_mode.json")) {
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

    // ── Persist instance memory for retries ──
    if let Some(ref store) = store {
        let mut mem = instance_memory.unwrap_or_else(|| SwebenchInstanceMemory::new(&instance_key));
        mem.indexed_at = index.as_ref().map(|i| i.indexed_at);
        if let Err(e) = store.save_instance(&mem) {
            eprintln!("    failed to save instance memory: {}", e);
        }
    }

    let result = PerRunResult {
        instance_id: inst.instance_id.clone(),
        quant: spec.label.clone(),
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
        has_test_edit,
        syntax_check_passed,
        candidate_num: candidate,
    };
    std::fs::write(
        candidate_dir.join("result.json"),
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

/// Run one or more candidates for a single (quant, instance, trial).
/// Returns `(selected_best, vec_of_all_candidate_results)`.
fn run_one(
    opts: &SwebenchProOpts,
    spec: &QuantSpec,
    inst: &Instance,
    trial: u32,
) -> Result<(PerRunResult, Vec<PerRunResult>)> {
    let trial_dir = trial_dir_for(&opts.output, &spec.label, &inst.instance_id, trial);
    std::fs::create_dir_all(&trial_dir)
        .with_context(|| format!("creating {}", trial_dir.display()))?;
    let trial_pred = trial_dir.join(format!("{}.pred", inst.instance_id));

    // Legacy path: single candidate directly in trial_dir.
    if opts.candidates <= 1 {
        let res = run_one_candidate(opts, spec, inst, trial, 0, &trial_dir)?;
        return Ok((res, vec![]));
    }

    // Multi-candidate path.
    if opts.skip_existing && trial_pred.exists() {
        eprintln!(
            "  → {} (trial {}): SKIP (pred exists)",
            inst.instance_id, trial
        );
        let result_path = trial_dir.join("result.json");
        if result_path.exists() {
            let bytes = std::fs::read(&result_path)?;
            let result: PerRunResult = serde_json::from_slice(&bytes)?;
            return Ok((result, vec![]));
        }
        let bytes = std::fs::metadata(&trial_pred)
            .map(|m| m.len() as usize)
            .unwrap_or(0);
        let lines = std::fs::read_to_string(&trial_pred)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        let synthetic = PerRunResult {
            instance_id: inst.instance_id.clone(),
            quant: spec.label.clone(),
            trial,
            exit_code: 1,
            timed_out: false,
            wall_secs: 0.0,
            patch_lines: lines,
            patch_bytes: bytes,
            pred_path: trial_pred,
            error: format!(
                "skip-existing found .pred without result.json: {}",
                result_path.display()
            ),
            empty_diff: lines == 0 && bytes == 0,
            test_only_patch: false,
            has_source_edit: false,
            has_test_edit: false,
            syntax_check_passed: false,
            candidate_num: 0,
        };
        return Ok((synthetic, vec![]));
    }

    let mut candidate_results = Vec::new();
    for c in 1..=opts.candidates {
        let c_dir = trial_dir.join(format!("candidate_{}", c));
        match run_one_candidate(opts, spec, inst, trial, c, &c_dir) {
            Ok(res) => candidate_results.push(res),
            Err(e) => {
                eprintln!(
                    "    {} trial {} candidate {}: error: {}",
                    inst.instance_id, trial, c, e
                );
            }
        }
    }

    if candidate_results.is_empty() {
        // All candidates failed — synthesise a failure result for the trial.
        if !trial_pred.exists() {
            std::fs::write(&trial_pred, "")?;
        }
        let synthetic = PerRunResult {
            instance_id: inst.instance_id.clone(),
            quant: spec.label.clone(),
            trial,
            exit_code: -2,
            timed_out: false,
            wall_secs: 0.0,
            patch_lines: 0,
            patch_bytes: 0,
            pred_path: trial_pred.clone(),
            error: "all candidates failed".into(),
            empty_diff: true,
            test_only_patch: false,
            has_source_edit: false,
            has_test_edit: false,
            syntax_check_passed: false,
            candidate_num: 0,
        };
        std::fs::write(
            trial_dir.join("result.json"),
            serde_json::to_vec_pretty(&synthetic)?,
        )?;
        return Ok((synthetic, candidate_results));
    }

    // Honest selection: pick the best candidate using proxy metrics.
    let best = candidate_results
        .iter()
        .max_by(|a, b| {
            let a_good = a.has_source_edit && !a.has_test_edit;
            let b_good = b.has_source_edit && !b.has_test_edit;
            a_good
                .cmp(&b_good)
                .then_with(|| b.patch_lines.cmp(&a.patch_lines))
                .then_with(|| a.syntax_check_passed.cmp(&b.syntax_check_passed))
        })
        .unwrap();

    // Promote best candidate patch to trial-level pred.
    std::fs::copy(&best.pred_path, &trial_pred)?;

    let synthetic = PerRunResult {
        instance_id: inst.instance_id.clone(),
        quant: spec.label.clone(),
        trial,
        exit_code: best.exit_code,
        timed_out: best.timed_out,
        wall_secs: best.wall_secs,
        patch_lines: best.patch_lines,
        patch_bytes: best.patch_bytes,
        pred_path: trial_pred.clone(),
        error: best.error.clone(),
        empty_diff: best.empty_diff,
        test_only_patch: best.test_only_patch,
        has_source_edit: best.has_source_edit,
        has_test_edit: best.has_test_edit,
        syntax_check_passed: best.syntax_check_passed,
        candidate_num: 0,
    };
    std::fs::write(
        trial_dir.join("result.json"),
        serde_json::to_vec_pretty(&synthetic)?,
    )?;
    eprintln!(
        "    selected candidate {} → {} ({} lines)",
        best.candidate_num,
        trial_pred
            .file_name()
            .map(|s| s.to_string_lossy())
            .unwrap_or_default(),
        best.patch_lines,
    );

    Ok((synthetic, candidate_results))
}

fn diff_path_from_line(line: &str) -> Option<&str> {
    if line.starts_with("diff --git ") {
        return line.find(" b/").map(|b_start| &line[b_start + 3..]);
    }
    if let Some(path) = line.strip_prefix("+++ b/") {
        return Some(path);
    }
    if let Some(path) = line.strip_prefix("--- a/") {
        return Some(path);
    }
    None
}

fn is_test_path(path: &str) -> bool {
    let lower = path.trim_matches('"').to_ascii_lowercase();
    let mut parts = lower.split('/').filter(|part| !part.is_empty());
    if parts
        .clone()
        .any(|part| matches!(part, "test" | "tests" | "__tests__" | "spec" | "specs"))
    {
        return true;
    }

    let basename = parts.next_back().unwrap_or(lower.as_str());
    let stem = basename
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(basename);
    stem == "test"
        || stem == "spec"
        || stem.starts_with("test_")
        || stem.starts_with("test-")
        || stem.starts_with("spec_")
        || stem.starts_with("spec-")
        || stem.ends_with("_test")
        || stem.ends_with("-test")
        || stem.ends_with("_spec")
        || stem.ends_with("-spec")
        || basename.contains(".test.")
        || basename.contains(".spec.")
}

/// Parse a git diff and determine whether every modified file is a test file.
fn is_test_only_patch(patch: &str) -> bool {
    let mut has_any_file = false;
    for line in patch.lines() {
        if let Some(path) = diff_path_from_line(line) {
            if !is_test_path(path) {
                return false;
            }
            has_any_file = true;
        }
    }
    has_any_file
}

/// Parse a git diff and determine whether *any* modified file is a test file.
fn has_test_edit_in_patch(patch: &str) -> bool {
    for line in patch.lines() {
        if let Some(path) = diff_path_from_line(line) {
            if is_test_path(path) {
                return true;
            }
        }
    }
    false
}

fn is_source_path(path: &str) -> bool {
    let lower = path.trim_matches('"').to_ascii_lowercase();
    if is_test_path(&lower) {
        return false;
    }
    let Some(ext) = std::path::Path::new(&lower)
        .extension()
        .and_then(|e| e.to_str())
    else {
        return false;
    };
    matches!(
        ext,
        "py" | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "java"
            | "cs"
            | "c"
            | "cc"
            | "cpp"
            | "cxx"
            | "h"
            | "hh"
            | "hpp"
            | "sql"
            | "go"
            | "swift"
            | "rs"
    )
}

fn has_source_edit_in_patch(patch: &str) -> bool {
    for line in patch.lines() {
        if let Some(path) = diff_path_from_line(line) {
            if is_source_path(path) {
                return true;
            }
        }
    }
    false
}

/// Cheap syntax check: non-empty and no merge-conflict markers.
fn cheap_syntax_check(patch: &str) -> bool {
    let trimmed = patch.trim();
    !trimmed.is_empty()
        && !trimmed.contains("<<<<<<<")
        && !trimmed.contains("=======")
        && !trimmed.contains(">>>>>>>")
}

#[derive(Serialize)]
struct AggregateEntry {
    quant: String,
    instance_id: String,
    trials: u32,
    /// Diagnostic metric: runs that reached the agent and produced a non-empty patch.
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
    /// `pass@1` — did the honestly-selected best candidate resolve?
    pass_at_1: bool,
    /// `pass@k` oracle — did any candidate resolve?  Labelled as upper bound
    /// in the report-level metadata.
    pass_at_k_oracle: bool,
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
    /// `pass@1` — single best candidate per instance (honest selection).
    pass_at_1_rate: f64,
    /// `pass@k` oracle — best of k candidates.  **Upper bound**: may be
    /// proxy-based when official eval was not run on all candidates.
    pass_at_k_oracle_rate: f64,
    /// When `true`, `pass_at_k_oracle_rate` is based on proxy metrics
    /// because not every candidate received an official Docker evaluation.
    pass_at_k_oracle_is_proxy: bool,
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
    use super::candidate::{Candidate, CandidatePool, OfficialEvalResult};

    // Group by (quant, instance_id).
    let mut groups: BTreeMap<(String, String), Vec<&PerRunResult>> = BTreeMap::new();
    for r in runs {
        groups
            .entry((r.quant.clone(), r.instance_id.clone()))
            .or_default()
            .push(r);
    }

    let mut entries = Vec::new();
    let mut pass_at_1_count = 0usize;
    let mut pass_at_k_oracle_count = 0usize;
    let mut pass_at_k_oracle_is_proxy = false;
    let evaluated_runs = official_eval.map(|_| best_runs_by_quant_instance(runs));

    for ((quant, instance_id), group_runs) in groups {
        let key = (quant.clone(), instance_id.clone());
        // Separate selected results (candidate_num == 0) from raw candidates.
        let selected: Vec<_> = group_runs.iter().filter(|r| r.candidate_num == 0).collect();
        let total = selected.len() as u32;
        let attempted_patch = selected
            .iter()
            .filter(|r| r.error.is_empty() && r.patch_bytes > 0)
            .count() as u32;
        let empty_patch = selected.iter().filter(|r| r.empty_diff).count() as u32;
        let test_only = selected.iter().filter(|r| r.test_only_patch).count() as u32;
        let source_edit = selected.iter().filter(|r| r.has_source_edit).count() as u32;

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

        let mut walls: Vec<f64> = selected.iter().map(|r| r.wall_secs).collect();
        walls.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median_wall = median_f64(&walls);
        let best_wall = walls.first().copied().unwrap_or(0.0);

        let best_trial = selected
            .iter()
            .max_by_key(|r| r.patch_lines)
            .map(|r| r.trial)
            .unwrap_or(0);

        let mut lines: Vec<f64> = selected.iter().map(|r| r.patch_lines as f64).collect();
        lines.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median_patch_lines = median_f64(&lines);

        let official = official_eval
            .and_then(|m| m.by_pair.get(&key))
            .cloned()
            .unwrap_or_default();
        let evaluated_run = evaluated_runs.as_ref().and_then(|m| m.get(&key).copied());

        // Build CandidatePool from *all* runs (selected + candidates) for this pair.
        let candidates: Vec<Candidate> = group_runs
            .iter()
            .map(|r| {
                let patch = std::fs::read_to_string(&r.pred_path).unwrap_or_default();
                let oe = match (official_eval, evaluated_run) {
                    (Some(_), Some(evaluated))
                        if evaluated.trial == r.trial
                            && evaluated.candidate_num == r.candidate_num =>
                    {
                        Some(OfficialEvalResult {
                            resolved: official.resolved,
                        })
                    }
                    _ => None,
                };
                Candidate {
                    trial: r.trial,
                    patch,
                    patch_bytes: r.patch_bytes,
                    patch_lines: r.patch_lines,
                    has_source_edit: r.has_source_edit,
                    has_test_edit: r.has_test_edit,
                    syntax_check_passed: r.syntax_check_passed,
                    test_results: None,
                    official_eval: oe,
                }
            })
            .collect();
        let pool = CandidatePool::new(candidates);
        let pass_at_1 = pool.pass_at_1();
        let pass_at_k_oracle = pool.pass_at_k_oracle();
        if pass_at_k_oracle && !pool.has_any_official_eval() {
            pass_at_k_oracle_is_proxy = true;
        }
        if pass_at_1 {
            pass_at_1_count += 1;
        }
        if pass_at_k_oracle {
            pass_at_k_oracle_count += 1;
        }

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
            pass_at_1,
            pass_at_k_oracle,
            eval_error: official.eval_error,
        });
    }

    let attempted_patch_total = runs
        .iter()
        .filter(|r| r.candidate_num == 0)
        .filter(|r| r.error.is_empty() && r.patch_bytes > 0)
        .count();
    let selected_count = runs.iter().filter(|r| r.candidate_num == 0).count();
    let attempted_patch_rate = if selected_count == 0 {
        0.0
    } else {
        attempted_patch_total as f64 / selected_count as f64
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

    let n_instances = entries.len().max(1);
    let report = AggregateReport {
        generated_at: Utc::now().to_rfc3339(),
        total_runs: runs.len(),
        attempted_patch_rate,
        official_eval_completed,
        official_resolution_rate,
        pass_at_1_rate: pass_at_1_count as f64 / n_instances as f64,
        pass_at_k_oracle_rate: pass_at_k_oracle_count as f64 / n_instances as f64,
        pass_at_k_oracle_is_proxy,
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
        if r.candidate_num > 0 || !r.error.is_empty() || r.patch_bytes == 0 {
            continue; // candidates/infra failures/empty patches are not promoted to patches.json
        }
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

    let eval_script = opts
        .official_eval_script
        .canonicalize()
        .with_context(|| format!("resolving {}", opts.official_eval_script.display()))?;
    let raw_sample_path = opts
        .official_eval_raw_sample_path
        .canonicalize()
        .with_context(|| format!("resolving {}", opts.official_eval_raw_sample_path.display()))?;
    let scripts_dir = opts
        .official_eval_scripts_dir
        .canonicalize()
        .with_context(|| format!("resolving {}", opts.official_eval_scripts_dir.display()))?;
    let output_root = absolute_path(&opts.output)?;

    let best = best_runs_by_quant_instance(runs);
    let mut by_quant: BTreeMap<String, Vec<&PerRunResult>> = BTreeMap::new();
    for ((quant, _instance_id), run) in best {
        by_quant.entry(quant).or_default().push(run);
    }

    let eval_root = output_root.join("eval");
    std::fs::create_dir_all(&eval_root)
        .with_context(|| format!("creating {}", eval_root.display()))?;
    let normalized_raw_sample_path = prepare_official_eval_sample(
        &raw_sample_path,
        &eval_root.join("raw_sample.normalized.jsonl"),
    )?;

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
        let patch_path = patch_path
            .canonicalize()
            .with_context(|| format!("resolving {}", patch_path.display()))?;
        let eval_dir = eval_dir
            .canonicalize()
            .with_context(|| format!("resolving {}", eval_dir.display()))?;

        let mut cmd = std::process::Command::new("python3");
        cmd.arg(&eval_script)
            .arg("--raw_sample_path")
            .arg(&normalized_raw_sample_path)
            .arg("--patch_path")
            .arg(&patch_path)
            .arg("--output_dir")
            .arg(&eval_dir)
            .arg("--scripts_dir")
            .arg(&scripts_dir)
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
        if let Some(parent) = eval_script.parent() {
            cmd.current_dir(parent);
        }

        let output = cmd
            .output()
            .with_context(|| format!("spawning {}", eval_script.display()))?;
        let exit_code = output.status.code();
        let eval_error = if output.status.success() {
            String::new()
        } else {
            String::from_utf8_lossy(&output.stderr).trim().to_string()
        };
        std::fs::write(
            eval_dir.join("eval_invocation.json"),
            serde_json::to_vec_pretty(&json!({
                "script": &opts.official_eval_script,
                "resolved_script": &eval_script,
                "raw_sample_path": &normalized_raw_sample_path,
                "original_raw_sample_path": &raw_sample_path,
                "patch_path": &patch_path,
                "output_dir": &eval_dir,
                "scripts_dir": &scripts_dir,
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

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn prepare_official_eval_sample(input: &Path, output: &Path) -> Result<PathBuf> {
    if input.extension().and_then(|s| s.to_str()) != Some("jsonl") {
        return Ok(input.to_path_buf());
    }

    let data = std::fs::read_to_string(input)
        .with_context(|| format!("reading raw sample {}", input.display()))?;
    let mut normalized = String::new();
    for (line_idx, line) in data.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let mut row: serde_json::Value = serde_json::from_str(line).with_context(|| {
            format!(
                "parsing raw sample {} line {}",
                input.display(),
                line_idx + 1
            )
        })?;
        if let Some(obj) = row.as_object_mut() {
            normalize_eval_test_list(obj, "fail_to_pass", "FAIL_TO_PASS");
            normalize_eval_test_list(obj, "pass_to_pass", "PASS_TO_PASS");
        }
        normalized.push_str(&serde_json::to_string(&row)?);
        normalized.push('\n');
    }

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(output, normalized)
        .with_context(|| format!("writing normalized raw sample {}", output.display()))?;
    output
        .canonicalize()
        .with_context(|| format!("resolving {}", output.display()))
}

fn normalize_eval_test_list(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    lower_key: &str,
    upper_key: &str,
) {
    let value = obj
        .get(lower_key)
        .cloned()
        .or_else(|| obj.get(upper_key).cloned())
        .unwrap_or(serde_json::Value::Null);
    obj.insert(
        lower_key.to_string(),
        serde_json::Value::String(eval_list_literal(&value)),
    );
}

fn eval_list_literal(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) if s.trim().is_empty() => "[]".to_string(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(_) => {
            serde_json::to_string(value).unwrap_or_else(|_| "[]".to_string())
        }
        serde_json::Value::Null => "[]".to_string(),
        other => serde_json::to_string(other).unwrap_or_else(|_| "[]".to_string()),
    }
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

    fn dummy_quant(label: &str) -> QuantSpec {
        use super::super::catalog::{BackendProfile, ThinkingPolicy};

        QuantSpec {
            label: label.into(),
            gguf: "test.gguf".into(),
            alias: "test-alias".into(),
            mmproj: "mmproj.gguf".into(),
            name: "Test".into(),
            ctx: 65_536,
            max_parallel: 1,
            kv_cache_type: "q8_0".into(),
            tensor_split: None,
            temperature: 0.7,
            thinking_policy: ThinkingPolicy::Enable,
            backend: BackendProfile::LlamaCpp,
        }
    }

    fn dummy_opts(output: PathBuf) -> SwebenchProOpts {
        SwebenchProOpts {
            quants: vec!["q1".into()],
            instance_ids: vec![],
            instances: 1,
            scenario_timeout: Duration::from_secs(1),
            ctx: 65_536,
            parallel: 1,
            concurrency: 1,
            trials: 1,
            candidates: 1,
            output,
            selfware_bin: PathBuf::from("selfware"),
            skip_existing: false,
            llama_opts: LlamaServerOpts::default(),
            prompt_mode: "official".into(),
            prompt_profile: "swebench_pro".into(),
            official_eval: false,
            official_eval_script: PathBuf::from("eval.py"),
            official_eval_raw_sample_path: PathBuf::from("sample.jsonl"),
            official_eval_scripts_dir: PathBuf::from("scripts"),
            official_eval_dockerhub_username: "local".into(),
            official_eval_num_workers: 1,
            official_eval_use_local_docker: true,
            official_eval_redo: false,
            official_eval_block_network: false,
            resume: false,
            force_rerun: false,
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
    fn test_path_detection_does_not_match_substrings() {
        let patch = r#"diff --git a/src/contest.py b/src/contest.py
--- a/src/contest.py
+++ b/src/contest.py
@@ -1 +1 @@
-old
+new
"#;
        assert!(!is_test_only_patch(patch));
        assert!(!has_test_edit_in_patch(patch));
    }

    #[test]
    fn test_path_detection_matches_common_test_names() {
        assert!(is_test_path("tests/test_user.py"));
        assert!(is_test_path("src/foo/user_test.rs"));
        assert!(is_test_path("web/Button.test.tsx"));
        assert!(is_test_path("pkg/__tests__/button.js"));
        assert!(!is_test_path("src/contest.py"));
        assert!(!is_test_path("src/latest.rs"));
    }

    #[test]
    fn prepare_official_eval_sample_normalizes_uppercase_test_lists() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("sample.jsonl");
        let output = dir.path().join("out").join("sample.normalized.jsonl");
        std::fs::write(
            &input,
            r#"{"instance_id":"i1","FAIL_TO_PASS":["tests/test_a.py::test_x"],"PASS_TO_PASS":"[\"tests/test_b.py::test_y\"]"}"#,
        )
        .unwrap();

        let normalized_path = prepare_official_eval_sample(&input, &output).unwrap();
        let row: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(normalized_path).unwrap()).unwrap();

        assert_eq!(row["fail_to_pass"], r#"["tests/test_a.py::test_x"]"#);
        assert_eq!(row["pass_to_pass"], r#"["tests/test_b.py::test_y"]"#);
    }

    #[test]
    fn nonzero_agent_exit_with_patch_is_patch_captured() {
        let result = PerRunResult {
            instance_id: "i1".into(),
            quant: "q1".into(),
            trial: 1,
            exit_code: 1,
            timed_out: false,
            wall_secs: 1.0,
            patch_lines: 3,
            patch_bytes: 42,
            pred_path: PathBuf::from("i1.pred"),
            error: String::new(),
            empty_diff: false,
            test_only_patch: false,
            has_source_edit: true,
            has_test_edit: false,
            syntax_check_passed: true,
            candidate_num: 0,
        };

        let (state, error) = trial_state_from_result(&result);

        assert_eq!(state, TrialState::PatchCaptured);
        assert!(error.is_none());
    }

    #[test]
    fn median_handles_even_and_odd() {
        assert!((median_f64(&[1.0, 2.0, 3.0]) - 2.0).abs() < 1e-9);
        assert!((median_f64(&[1.0, 2.0, 3.0, 4.0]) - 2.5).abs() < 1e-9);
        assert_eq!(median_f64(&[]), 0.0);
    }

    #[test]
    fn concurrency_clamping_logic() {
        use super::super::catalog::{BackendProfile, QuantSpec, ThinkingPolicy};

        let spec = QuantSpec {
            label: "test".into(),
            gguf: "test.gguf".into(),
            alias: "test-alias".into(),
            mmproj: "mmproj.gguf".into(),
            name: "Test".into(),
            ctx: 65536,
            max_parallel: 2,
            kv_cache_type: "q8_0".into(),
            tensor_split: None,
            temperature: 0.7,
            thinking_policy: ThinkingPolicy::Enable,
            backend: BackendProfile::LlamaCpp,
        };

        // When requested concurrency is below max_parallel, keep it.
        let requested = 1;
        let effective = requested.min(spec.max_parallel).max(1);
        assert_eq!(effective, 1);

        // When requested concurrency exceeds max_parallel, clamp.
        let requested = 8;
        let effective = requested.min(spec.max_parallel).max(1);
        assert_eq!(effective, 2);

        // Zero should bump to 1.
        let requested = 0;
        let effective = requested.min(spec.max_parallel).max(1);
        assert_eq!(effective, 1);
    }

    #[test]
    fn has_test_edit_detects_test_file() {
        let patch = r#"diff --git a/tests/test_a.py b/tests/test_a.py
--- a/tests/test_a.py
+++ b/tests/test_a.py
@@ -1 +1 @@
-old
+new
"#;
        assert!(has_test_edit_in_patch(patch));
    }

    #[test]
    fn has_test_edit_detects_no_test_file() {
        let patch = r#"diff --git a/src/main.py b/src/main.py
--- a/src/main.py
+++ b/src/main.py
@@ -1 +1 @@
-old
+new
"#;
        assert!(!has_test_edit_in_patch(patch));
    }

    #[test]
    fn has_test_edit_mixed_source_and_test() {
        let patch = r#"diff --git a/src/main.py b/src/main.py
--- a/src/main.py
+++ b/src/main.py
@@ -1 +1 @@
-old
+new

diff --git a/tests/test_a.py b/tests/test_a.py
--- a/tests/test_a.py
+++ b/tests/test_a.py
@@ -1 +1 @@
-old
+new
"#;
        assert!(has_test_edit_in_patch(patch));
    }

    #[test]
    fn cheap_syntax_check_accepts_clean_patch() {
        assert!(cheap_syntax_check("diff --git a/foo.py b/foo.py\n+bar\n"));
    }

    #[test]
    fn cheap_syntax_check_rejects_merge_conflict() {
        assert!(!cheap_syntax_check(
            "<<<<<<< HEAD\nfoo\n=======\nbar\n>>>>>>> branch\n"
        ));
    }

    #[test]
    fn cheap_syntax_check_rejects_empty() {
        assert!(!cheap_syntax_check(""));
        assert!(!cheap_syntax_check("   \n  \n"));
    }

    #[test]
    fn best_runs_skips_candidates() {
        let runs = vec![
            PerRunResult {
                instance_id: "i1".into(),
                quant: "q1".into(),
                trial: 1,
                exit_code: 0,
                timed_out: false,
                wall_secs: 1.0,
                patch_lines: 10,
                patch_bytes: 100,
                pred_path: PathBuf::from("/tmp/a"),
                error: String::new(),
                empty_diff: false,
                test_only_patch: false,
                has_source_edit: true,
                has_test_edit: false,
                syntax_check_passed: true,
                candidate_num: 0,
            },
            PerRunResult {
                instance_id: "i1".into(),
                quant: "q1".into(),
                trial: 1,
                exit_code: 0,
                timed_out: false,
                wall_secs: 1.0,
                patch_lines: 50,
                patch_bytes: 500,
                pred_path: PathBuf::from("/tmp/b"),
                error: String::new(),
                empty_diff: false,
                test_only_patch: false,
                has_source_edit: true,
                has_test_edit: false,
                syntax_check_passed: true,
                candidate_num: 2,
            },
        ];
        let best = best_runs_by_quant_instance(&runs);
        assert_eq!(best.len(), 1);
        let chosen = best.values().next().unwrap();
        assert_eq!(chosen.patch_lines, 10); // candidate skipped
    }

    #[test]
    fn write_aggregate_pass_at_1_vs_pass_at_k() {
        let dir = tempfile::tempdir().unwrap();

        // Write dummy pred files
        std::fs::write(dir.path().join("i1_t1.pred"), "patch1\n").unwrap();
        std::fs::write(dir.path().join("i1_c1.pred"), "patch2\n").unwrap();
        std::fs::write(dir.path().join("i1_c2.pred"), "patch3\n").unwrap();

        let runs = vec![
            // Selected best for instance i1, trial 1
            PerRunResult {
                instance_id: "i1".into(),
                quant: "q1".into(),
                trial: 1,
                exit_code: 0,
                timed_out: false,
                wall_secs: 1.0,
                patch_lines: 2,
                patch_bytes: 10,
                pred_path: dir.path().join("i1_t1.pred"),
                error: String::new(),
                empty_diff: false,
                test_only_patch: false,
                has_source_edit: true,
                has_test_edit: false,
                syntax_check_passed: true,
                candidate_num: 0,
            },
            // Candidate 1: smaller diff, no test edits, syntax ok
            PerRunResult {
                instance_id: "i1".into(),
                quant: "q1".into(),
                trial: 1,
                exit_code: 0,
                timed_out: false,
                wall_secs: 1.0,
                patch_lines: 1,
                patch_bytes: 5,
                pred_path: dir.path().join("i1_c1.pred"),
                error: String::new(),
                empty_diff: false,
                test_only_patch: false,
                has_source_edit: true,
                has_test_edit: false,
                syntax_check_passed: true,
                candidate_num: 1,
            },
            // Candidate 2: has test edit
            PerRunResult {
                instance_id: "i1".into(),
                quant: "q1".into(),
                trial: 1,
                exit_code: 0,
                timed_out: false,
                wall_secs: 1.0,
                patch_lines: 1,
                patch_bytes: 5,
                pred_path: dir.path().join("i1_c2.pred"),
                error: String::new(),
                empty_diff: false,
                test_only_patch: false,
                has_source_edit: true,
                has_test_edit: true,
                syntax_check_passed: true,
                candidate_num: 2,
            },
        ];

        write_aggregate(dir.path(), &runs, None).unwrap();

        let agg: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.path().join("aggregate.json")).unwrap())
                .unwrap();

        // pass@1 should be false because no official eval data
        assert_eq!(
            agg["pass_at_1_rate"], 0.0,
            "pass@1 should be 0 without official eval"
        );
        // pass@k_oracle should be true because at least one candidate has
        // source edit + no test edit + syntax ok (the selected best and candidate 1)
        assert_eq!(
            agg["pass_at_k_oracle_rate"], 1.0,
            "pass@k oracle should be 1 via proxy upper bound"
        );
        assert_eq!(
            agg["pass_at_k_oracle_is_proxy"], true,
            "should be marked proxy-based"
        );

        // Entry-level checks
        let entry = &agg["entries"][0];
        assert_eq!(entry["pass_at_1"], false);
        assert_eq!(entry["pass_at_k_oracle"], true);
        // attempted_patch_rate should be based on selected results only
        assert_eq!(entry["attempted_patch_rate"], 1.0);
    }

    #[test]
    fn write_aggregate_does_not_proxy_pass_at_k_when_official_eval_failed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("selected.pred"), "selected\n").unwrap();
        std::fs::write(dir.path().join("candidate.pred"), "candidate\n").unwrap();

        let runs = vec![
            PerRunResult {
                instance_id: "i1".into(),
                quant: "q1".into(),
                trial: 1,
                exit_code: 0,
                timed_out: false,
                wall_secs: 1.0,
                patch_lines: 2,
                patch_bytes: 10,
                pred_path: dir.path().join("selected.pred"),
                error: String::new(),
                empty_diff: false,
                test_only_patch: false,
                has_source_edit: true,
                has_test_edit: false,
                syntax_check_passed: true,
                candidate_num: 0,
            },
            PerRunResult {
                instance_id: "i1".into(),
                quant: "q1".into(),
                trial: 1,
                exit_code: 0,
                timed_out: false,
                wall_secs: 1.0,
                patch_lines: 1,
                patch_bytes: 5,
                pred_path: dir.path().join("candidate.pred"),
                error: String::new(),
                empty_diff: false,
                test_only_patch: false,
                has_source_edit: true,
                has_test_edit: false,
                syntax_check_passed: true,
                candidate_num: 1,
            },
        ];

        let mut by_pair = BTreeMap::new();
        by_pair.insert(
            ("q1".to_string(), "i1".to_string()),
            OfficialEvalStatus {
                eval_completed: true,
                patch_applied: true,
                f2p_p2p_passed: false,
                resolved: false,
                eval_error: String::new(),
            },
        );

        write_aggregate(dir.path(), &runs, Some(&OfficialEvalMap { by_pair })).unwrap();

        let agg: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.path().join("aggregate.json")).unwrap())
                .unwrap();

        assert_eq!(agg["pass_at_k_oracle_rate"], 0.0);
        assert_eq!(agg["pass_at_k_oracle_is_proxy"], false);
        assert_eq!(agg["entries"][0]["pass_at_k_oracle"], false);
    }

    #[test]
    fn reconstruct_missing_result_json_is_failed_even_when_pred_exists() {
        let dir = tempfile::tempdir().unwrap();
        let opts = dummy_opts(dir.path().to_path_buf());
        let spec = dummy_quant("q1");
        let inst = dummy_instance();
        let trial_dir = trial_dir_for(&opts.output, &spec.label, &inst.instance_id, 1);
        std::fs::create_dir_all(&trial_dir).unwrap();
        std::fs::write(
            trial_dir.join(format!("{}.pred", inst.instance_id)),
            "patch\n",
        )
        .unwrap();

        let result = reconstruct_result_from_disk(&opts, &spec, &inst, 1).unwrap();

        assert_ne!(result.exit_code, 0);
        assert!(result.error.contains("without result.json"));
        assert_eq!(result.patch_bytes, 6);
    }
}
