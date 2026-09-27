//! Per-step checkpointing and resume for workflow runs.
//!
//! A checkpointed run writes `<workspace>/.selfware/workflows/<run-id>.json`
//! after every top-level step: the variables, the step results and, for
//! each completed top-level step, a fingerprint of its definition.
//! `selfware workflow run <file> --resume <run-id>` reloads it, restores the
//! variables, and skips every top-level step that completed with an
//! unchanged definition — so a completed LLM step is not re-billed. Failed,
//! skipped and never-reached steps run again; a step whose definition was
//! edited since the checkpoint runs again too.
//!
//! The unit of resume is the top-level step: a `condition`, `loop` or
//! `until` block that did not complete is re-run as a whole.

use super::{
    StepResult, StepStatus, VarValue, WorkflowStatus, WorkflowStep, WorkflowStopReason,
    WorkflowTelemetry,
};
use anyhow::{anyhow, bail, Context as _, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// On-disk checkpoint format version.
pub const CHECKPOINT_FORMAT_VERSION: u32 = 1;

/// Longest accepted run id.
const MAX_RUN_ID_LEN: usize = 128;

/// Persisted state of one workflow run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowCheckpoint {
    /// Format version ([`CHECKPOINT_FORMAT_VERSION`]).
    pub version: u32,
    /// Run id (also the file stem).
    pub run_id: String,
    /// Workflow the run executes.
    pub workflow_name: String,
    /// Status when the checkpoint was written (`running` while in flight).
    pub status: WorkflowStatus,
    /// Workflow variables (inputs, step outputs, `set_var` values).
    pub variables: HashMap<String, VarValue>,
    /// Step results (including control-flow-managed and `id@n` entries).
    pub step_results: HashMap<String, StepResult>,
    /// Top-level steps that completed, mapped to their definition
    /// fingerprint ([`step_fingerprint`]).
    pub completed_steps: BTreeMap<String, String>,
    /// Why the run stopped early (budget), if it did.
    #[serde(default)]
    pub stop_reason: Option<WorkflowStopReason>,
    /// Telemetry of the most recent invocation of this run.
    #[serde(default)]
    pub telemetry: WorkflowTelemetry,
    /// RFC 3339 time of the write.
    pub updated_at: String,
}

impl WorkflowCheckpoint {
    /// Step results worth restoring on resume: only completed ones. Failed
    /// and skipped results are dropped so a stale failure can never decide
    /// the resumed run's verdict.
    pub(super) fn completed_results(&self) -> impl Iterator<Item = (&String, &StepResult)> {
        self.step_results
            .iter()
            .filter(|(_, result)| result.status == StepStatus::Completed)
    }
}

/// Directory of run checkpoints.
#[derive(Debug, Clone)]
pub struct CheckpointStore {
    dir: PathBuf,
}

impl CheckpointStore {
    /// A store rooted at `dir`.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The store of a workspace: `<root>/.selfware/workflows`.
    pub fn for_workspace(root: &Path) -> Self {
        Self::new(root.join(".selfware").join("workflows"))
    }

    /// The store's directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Checkpoint file of `run_id` (validated: a run id can never name a
    /// path outside the store).
    pub fn path_for(&self, run_id: &str) -> Result<PathBuf> {
        validate_run_id(run_id)?;
        Ok(self.dir.join(format!("{run_id}.json")))
    }

    /// Load the checkpoint of `run_id`.
    pub fn load(&self, run_id: &str) -> Result<WorkflowCheckpoint> {
        let path = self.path_for(run_id)?;
        let raw = std::fs::read_to_string(&path).with_context(|| {
            format!(
                "no checkpoint for workflow run '{run_id}' (looked for {})",
                path.display()
            )
        })?;
        let checkpoint: WorkflowCheckpoint = serde_json::from_str(&raw)
            .with_context(|| format!("corrupt workflow checkpoint {}", path.display()))?;
        if checkpoint.version != CHECKPOINT_FORMAT_VERSION {
            bail!(
                "workflow checkpoint {} has format version {}, expected {}",
                path.display(),
                checkpoint.version,
                CHECKPOINT_FORMAT_VERSION
            );
        }
        if checkpoint.run_id != run_id {
            bail!(
                "workflow checkpoint {} belongs to run '{}', not '{run_id}'",
                path.display(),
                checkpoint.run_id
            );
        }
        Ok(checkpoint)
    }

    /// Write `checkpoint` atomically (temp file + rename). The file holds
    /// workflow variables and LLM outputs, so on Unix it is created
    /// owner-only (0600).
    pub fn save(&self, checkpoint: &WorkflowCheckpoint) -> Result<PathBuf> {
        let path = self.path_for(&checkpoint.run_id)?;
        std::fs::create_dir_all(&self.dir)
            .with_context(|| format!("cannot create {}", self.dir.display()))?;
        let json = serde_json::to_vec_pretty(checkpoint)?;
        let tmp = self.dir.join(format!(".{}.json.tmp", checkpoint.run_id));
        write_private(&tmp, &json).with_context(|| format!("cannot write {}", tmp.display()))?;
        std::fs::rename(&tmp, &path)
            .with_context(|| format!("cannot move checkpoint into {}", path.display()))?;
        Ok(path)
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// A run id is 1..=128 ASCII alphanumerics, `-`, `_` or `.`, not starting
/// with `.` — never a path.
pub fn validate_run_id(run_id: &str) -> Result<()> {
    let valid = !run_id.is_empty()
        && run_id.len() <= MAX_RUN_ID_LEN
        && !run_id.starts_with('.')
        && run_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if valid {
        Ok(())
    } else {
        Err(anyhow!(
            "invalid workflow run id '{run_id}': use 1-{MAX_RUN_ID_LEN} letters, digits, '-', '_' or '.', not starting with '.'"
        ))
    }
}

/// A fresh run id: `<workflow-slug>-<UTC timestamp>-<8 hex>`.
pub fn new_run_id(workflow_name: &str) -> String {
    let mut slug: String = workflow_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(40)
        .collect();
    slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        slug = "workflow".to_string();
    }
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    format!("{slug}-{stamp}-{}", &nonce[..8])
}

/// Fingerprint of a step definition: SHA-256 of its canonical JSON (object
/// keys sorted, so `HashMap` iteration order cannot change it).
pub fn step_fingerprint(step: &WorkflowStep) -> String {
    let value = serde_json::to_value(step).unwrap_or(serde_json::Value::Null);
    let mut canonical = String::new();
    write_canonical(&value, &mut canonical);
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

fn write_canonical(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::Value::String(key.clone()).to_string());
                out.push(':');
                write_canonical(&map[key], out);
            }
            out.push('}');
        }
        serde_json::Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// Checkpointing for one run of [`super::WorkflowExecutor::execute_run`].
#[derive(Debug, Clone)]
pub struct WorkflowRun {
    /// The run id (file stem in the store).
    pub run_id: String,
    /// Where checkpoints are written.
    pub store: CheckpointStore,
    /// The checkpoint being resumed, if any.
    pub resume_from: Option<WorkflowCheckpoint>,
}

impl WorkflowRun {
    /// A new run of `workflow_name` with a fresh run id.
    pub fn fresh(store: CheckpointStore, workflow_name: &str) -> Self {
        Self {
            run_id: new_run_id(workflow_name),
            store,
            resume_from: None,
        }
    }

    /// Resume run `run_id` from its checkpoint in `store`.
    pub fn resume(store: CheckpointStore, run_id: &str) -> Result<Self> {
        let checkpoint = store.load(run_id)?;
        Ok(Self {
            run_id: run_id.to_string(),
            store,
            resume_from: Some(checkpoint),
        })
    }

    /// The workflow the resumed checkpoint belongs to.
    pub fn resumed_workflow_name(&self) -> Option<&str> {
        self.resume_from
            .as_ref()
            .map(|checkpoint| checkpoint.workflow_name.as_str())
    }

    /// Path of this run's checkpoint file.
    pub fn checkpoint_path(&self) -> Result<PathBuf> {
        self.store.path_for(&self.run_id)
    }
}

/// Write the run's current state; a failed write is logged on the run (the
/// workflow keeps going, but the log says resume is not available).
pub(super) fn save_progress(
    run: &WorkflowRun,
    workflow_name: &str,
    context: &mut super::WorkflowContext,
    completed_steps: &BTreeMap<String, String>,
) {
    let checkpoint = WorkflowCheckpoint {
        version: CHECKPOINT_FORMAT_VERSION,
        run_id: run.run_id.clone(),
        workflow_name: workflow_name.to_string(),
        status: context.status,
        variables: context.variables.clone(),
        step_results: context.step_results.clone(),
        completed_steps: completed_steps.clone(),
        stop_reason: context.stop_reason.clone(),
        telemetry: context.telemetry.clone(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(err) = run.store.save(&checkpoint) {
        tracing::warn!(run_id = %run.run_id, error = %format!("{err:#}"), "workflow checkpoint not written");
        context.log(
            super::LogLevel::Warn,
            format!(
                "Checkpoint for run '{}' not written (resume unavailable from here): {err:#}",
                run.run_id
            ),
            None,
        );
    }
}
