//! Trial manifest for resumable SWE-bench Pro runs.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum TrialState {
    Planned,
    BootFailed,
    CloneFailed,
    Running,
    AgentFailed,
    PatchCaptured,
    Evaluated,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TrialManifest {
    pub quant: String,
    pub instance_id: String,
    pub trial: u32,
    pub state: TrialState,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error: Option<String>,
    pub pred_path: Option<PathBuf>,
    pub result_path: Option<PathBuf>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SwebenchProOptsSnapshot {
    pub quants: Vec<String>,
    pub instance_ids: Vec<String>,
    pub instances: usize,
    pub scenario_timeout_secs: u64,
    pub ctx: u32,
    pub parallel: u32,
    pub concurrency: u32,
    pub trials: u32,
    pub candidates: u32,
    pub prompt_mode: String,
    pub prompt_profile: String,
    pub official_eval: bool,
    pub llama_server_binary: PathBuf,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SweepManifest {
    pub created_at: String,
    pub opts: SwebenchProOptsSnapshot,
    pub trials: Vec<TrialManifest>,
}

impl SweepManifest {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes =
            std::fs::read(path).with_context(|| format!("reading manifest {}", path.display()))?;
        let manifest: SweepManifest = serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing manifest {}", path.display()))?;
        Ok(manifest)
    }

    pub fn write_atomic(&self, path: &Path) -> Result<()> {
        write_json_atomic(path, self)
    }

    pub fn find_trial(&self, quant: &str, instance_id: &str, trial: u32) -> Option<&TrialManifest> {
        self.trials
            .iter()
            .find(|t| t.quant == quant && t.instance_id == instance_id && t.trial == trial)
    }

    pub fn find_trial_mut(
        &mut self,
        quant: &str,
        instance_id: &str,
        trial: u32,
    ) -> Option<&mut TrialManifest> {
        self.trials
            .iter_mut()
            .find(|t| t.quant == quant && t.instance_id == instance_id && t.trial == trial)
    }
}

/// Atomically write `value` as pretty JSON to `path` via a temp sibling +
/// rename. A crash mid-write can then never leave a truncated/partial file:
/// `path` either has the old contents or the complete new contents. The harness
/// relies on this for every file a later resume reads (manifest.json,
/// result.json, aggregate.json, patches.json) — a half-written result.json was
/// otherwise misread as corrupt and turned into a spurious "synthetic failure".
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let json = serde_json::to_vec_pretty(value)?;
    let temp_path = path.with_extension("json.tmp");
    std::fs::write(&temp_path, json)
        .with_context(|| format!("writing temp file {}", temp_path.display()))?;
    std::fs::rename(&temp_path, path)
        .with_context(|| format!("renaming {} into place", path.display()))?;
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/bench_harness/swebench_pro/manifest/manifest_test.rs"]
mod tests;
