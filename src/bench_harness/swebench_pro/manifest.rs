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
mod tests {
    use super::*;
    use std::io::Read;

    fn dummy_snapshot() -> SwebenchProOptsSnapshot {
        SwebenchProOptsSnapshot {
            quants: vec!["q1".into()],
            instance_ids: vec!["i1".into()],
            instances: 1,
            scenario_timeout_secs: 900,
            ctx: 262_144,
            parallel: 2,
            concurrency: 1,
            trials: 3,
            candidates: 1,
            prompt_mode: "diagnostic".into(),
            prompt_profile: "swebench_pro".into(),
            official_eval: true,
            llama_server_binary: PathBuf::from("/bin/llama-server"),
        }
    }

    #[test]
    fn manifest_roundtrip() {
        let manifest = SweepManifest {
            created_at: "2024-01-01T00:00:00Z".into(),
            opts: dummy_snapshot(),
            trials: vec![TrialManifest {
                quant: "q1".into(),
                instance_id: "i1".into(),
                trial: 1,
                state: TrialState::Evaluated,
                started_at: Some("2024-01-01T00:00:00Z".into()),
                completed_at: Some("2024-01-01T00:01:00Z".into()),
                error: None,
                pred_path: Some(PathBuf::from("/tmp/pred")),
                result_path: Some(PathBuf::from("/tmp/result.json")),
            }],
        };

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        manifest.write_atomic(&path).unwrap();

        let loaded = SweepManifest::load(&path).unwrap();
        assert_eq!(loaded.opts, manifest.opts);
        assert_eq!(loaded.trials.len(), 1);
        assert_eq!(loaded.trials[0].state, TrialState::Evaluated);
    }

    #[test]
    fn atomic_write_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");

        let manifest = SweepManifest {
            created_at: "2024-01-01T00:00:00Z".into(),
            opts: SwebenchProOptsSnapshot {
                quants: vec![],
                instance_ids: vec![],
                instances: 0,
                scenario_timeout_secs: 0,
                ctx: 0,
                parallel: 0,
                concurrency: 0,
                trials: 0,
                candidates: 0,
                prompt_mode: "".into(),
                prompt_profile: "".into(),
                official_eval: false,
                llama_server_binary: PathBuf::from(""),
            },
            trials: vec![],
        };

        manifest.write_atomic(&path).unwrap();
        assert!(path.exists());

        let temp_path = dir.path().join("manifest.json.tmp");
        assert!(!temp_path.exists());

        let mut file = std::fs::File::open(&path).unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        let _: SweepManifest = serde_json::from_str(&content).unwrap();
    }

    #[test]
    fn write_json_atomic_overwrites_completely_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("result.json");

        // A larger value first, then a smaller one: an atomic replace must yield
        // exactly the new contents (no leftover tail from the old file, which a
        // truncating in-place rewrite could leave) and no .tmp sibling.
        write_json_atomic(&path, &serde_json::json!({"a": 1, "b": 2, "c": 3})).unwrap();
        write_json_atomic(&path, &serde_json::json!({"a": 1})).unwrap();

        assert!(
            !dir.path().join("result.json.tmp").exists(),
            "temp left behind"
        );
        let back: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(back, serde_json::json!({"a": 1}));
    }

    #[test]
    fn find_trial_mutates_correct_entry() {
        let mut manifest = SweepManifest {
            created_at: "2024-01-01T00:00:00Z".into(),
            opts: SwebenchProOptsSnapshot {
                quants: vec![],
                instance_ids: vec![],
                instances: 0,
                scenario_timeout_secs: 0,
                ctx: 0,
                parallel: 0,
                concurrency: 0,
                trials: 0,
                candidates: 0,
                prompt_mode: "".into(),
                prompt_profile: "".into(),
                official_eval: false,
                llama_server_binary: PathBuf::from(""),
            },
            trials: vec![
                TrialManifest {
                    quant: "q1".into(),
                    instance_id: "i1".into(),
                    trial: 1,
                    state: TrialState::Planned,
                    started_at: None,
                    completed_at: None,
                    error: None,
                    pred_path: None,
                    result_path: None,
                },
                TrialManifest {
                    quant: "q1".into(),
                    instance_id: "i1".into(),
                    trial: 2,
                    state: TrialState::Evaluated,
                    started_at: None,
                    completed_at: None,
                    error: None,
                    pred_path: None,
                    result_path: None,
                },
            ],
        };

        assert!(manifest.find_trial("q1", "i1", 1).is_some());
        assert!(manifest.find_trial("q1", "i1", 2).is_some());
        assert!(manifest.find_trial("q1", "i1", 3).is_none());

        if let Some(t) = manifest.find_trial_mut("q1", "i1", 1) {
            t.state = TrialState::BootFailed;
        }
        assert_eq!(manifest.trials[0].state, TrialState::BootFailed);
        assert_eq!(manifest.trials[1].state, TrialState::Evaluated);
    }

    #[test]
    fn resume_logic_skip_evaluated_unless_force() {
        let manifest = SweepManifest {
            created_at: "2024-01-01T00:00:00Z".into(),
            opts: SwebenchProOptsSnapshot {
                quants: vec![],
                instance_ids: vec![],
                instances: 0,
                scenario_timeout_secs: 0,
                ctx: 0,
                parallel: 0,
                concurrency: 0,
                trials: 0,
                candidates: 0,
                prompt_mode: "".into(),
                prompt_profile: "".into(),
                official_eval: false,
                llama_server_binary: PathBuf::from(""),
            },
            trials: vec![
                TrialManifest {
                    quant: "q1".into(),
                    instance_id: "i1".into(),
                    trial: 1,
                    state: TrialState::Evaluated,
                    started_at: None,
                    completed_at: None,
                    error: None,
                    pred_path: None,
                    result_path: None,
                },
                TrialManifest {
                    quant: "q1".into(),
                    instance_id: "i1".into(),
                    trial: 2,
                    state: TrialState::BootFailed,
                    started_at: None,
                    completed_at: None,
                    error: None,
                    pred_path: None,
                    result_path: None,
                },
                TrialManifest {
                    quant: "q1".into(),
                    instance_id: "i1".into(),
                    trial: 3,
                    state: TrialState::PatchCaptured,
                    started_at: None,
                    completed_at: None,
                    error: None,
                    pred_path: None,
                    result_path: None,
                },
            ],
        };

        // Evaluated should be skipped
        let t = manifest.find_trial("q1", "i1", 1).unwrap();
        assert!(matches!(t.state, TrialState::Evaluated));

        // BootFailed should be re-run
        let t = manifest.find_trial("q1", "i1", 2).unwrap();
        assert!(matches!(
            t.state,
            TrialState::Planned
                | TrialState::BootFailed
                | TrialState::CloneFailed
                | TrialState::AgentFailed
        ));

        // PatchCaptured should be skipped (agent already ran)
        let t = manifest.find_trial("q1", "i1", 3).unwrap();
        assert!(matches!(t.state, TrialState::PatchCaptured));
    }
}
