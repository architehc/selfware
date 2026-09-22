//! Task Checkpointing & Persistence
//!
//! Enables resumable long-running tasks by saving state to disk.
//! Captures:
//! - Task description and status
//! - Conversation messages
//! - Tool call history with timing
//! - Git state for reproducibility
//! - Error logs for debugging
//!
//! Checkpoints are stored as JSON files and can be resumed with `Agent::resume()`.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use crate::api::types::Message;
use crate::safety::redact;

/// Envelope that wraps a checkpoint with an integrity checksum.
///
/// The `sha256` field holds the hex-encoded SHA-256 hash of `payload` (the
/// compact-JSON serialized checkpoint data).  On load, the hash is recomputed
/// and compared to detect corruption or tampering.
#[derive(Debug, Serialize, Deserialize)]
struct CheckpointEnvelope {
    /// SHA-256 hex digest of the `payload` string
    sha256: String,
    /// The checkpoint data serialized as a JSON value
    payload: serde_json::Value,
}

impl CheckpointEnvelope {
    /// Get or create the HMAC key for checkpoint integrity verification.
    ///
    /// This function attempts to load an existing key from the data directory.
    /// If no key exists or the key file is invalid, a new random key is generated
    /// and persisted to disk with restrictive permissions (0o600 on Unix).
    ///
    /// # Returns
    /// A 32-byte key for HMAC-SHA-256 operations.
    fn get_hmac_key() -> Vec<u8> {
        let path = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("selfware")
            .join("checkpoint_hmac_key");

        // Try to load existing key
        if let Ok(key) = std::fs::read(&path) {
            if key.len() == 32 {
                return key;
            }
            tracing::warn!(
                "Existing HMAC key at {:?} has invalid length (expected 32, got {}). Generating new key.",
                path,
                key.len()
            );
        }

        // Generate new key
        let mut key = vec![0u8; 32];
        rand::Rng::fill_bytes(&mut rand::rng(), &mut key);

        // Attempt to persist the key with best-effort error handling
        if let Err(e) = Self::persist_hmac_key(&path, &key) {
            tracing::warn!(
                "Failed to persist HMAC key to {:?}: {}. Key will be ephemeral for this session.",
                path,
                e
            );
        }

        key
    }

    /// Persist the HMAC key to disk with appropriate permissions.
    fn persist_hmac_key(path: &PathBuf, key: &[u8]) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create HMAC key directory {:?}. Check permissions and disk space.",
                    parent
                )
            })?;
        }

        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(path)
                .with_context(|| {
                    format!(
                        "Failed to create HMAC key file {:?} with secure permissions (0o600). Check file permissions.",
                        path
                    )
                })?;
            file.write_all(key).with_context(|| {
                format!(
                    "Failed to write HMAC key to {:?}. Check disk space and permissions.",
                    path
                )
            })?;
            file.sync_all()
                .with_context(|| format!("Failed to sync HMAC key file {:?} to disk", path))?;
        }
        #[cfg(not(unix))]
        {
            std::fs::write(path, key).with_context(|| {
                format!(
                    "Failed to write HMAC key to {:?}. Check disk space and permissions.",
                    path
                )
            })?;
        }

        Ok(())
    }

    /// Create a new envelope by computing the HMAC-SHA-256 hash of the payload.
    fn wrap(payload: serde_json::Value) -> Result<Self> {
        use hmac::{Hmac, Mac};
        let canonical =
            serde_json::to_string(&payload).context("Failed to serialize payload for hashing")?;

        let mut mac = Hmac::<Sha256>::new_from_slice(&Self::get_hmac_key())
            .expect("HMAC can take key of any size");
        mac.update(canonical.as_bytes());
        let hash = hex::encode(mac.finalize().into_bytes());
        Ok(Self {
            sha256: hash,
            payload,
        })
    }

    /// Verify the integrity of the envelope by recomputing the HMAC.
    fn verify(&self) -> Result<()> {
        use hmac::{Hmac, Mac};
        let canonical = serde_json::to_string(&self.payload)
            .context("Failed to serialize payload for verification")?;

        let mut mac = Hmac::<Sha256>::new_from_slice(&Self::get_hmac_key())
            .expect("HMAC can take key of any size");
        mac.update(canonical.as_bytes());
        let expected = hex::encode(mac.finalize().into_bytes());

        if expected != self.sha256 {
            bail!(
                "Checkpoint integrity check failed: expected HMAC {}, got {}",
                expected,
                self.sha256
            );
        }
        Ok(())
    }
}

/// Current version of the checkpoint format
pub const CURRENT_CHECKPOINT_VERSION: u32 = 1;

fn default_version() -> u32 {
    0 // Legacy checkpoints have version 0
}

/// Status of a task checkpoint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    InProgress,
    Completed,
    Failed,
    Paused,
}

/// A memory entry for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub timestamp: String,
    pub role: String,
    pub content: String,
    pub token_estimate: usize,
}

/// Log of a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallLog {
    pub timestamp: DateTime<Utc>,
    pub tool_name: String,
    pub arguments: String,
    pub result: Option<String>,
    pub success: bool,
    pub duration_ms: Option<u64>,
}

/// Result of a visual verification check (used for verification results)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerificationResult {
    pub passed: bool,
    pub confidence: f32,         // VLM confidence score
    pub explanation: String,     // Why it passed/failed
    pub screenshot_hash: String, // For detecting stale screens
}

/// A persistent visual assertion that gates task progression.
/// Can be used both for pending assertions (to verify) and completed assertions (in history).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualAssertion {
    pub id: String,                       // Unique identifier for this assertion
    pub description: String,              // What to look for
    pub screenshot_path: Option<PathBuf>, // Path to reference screenshot
    pub verified: bool,                   // Whether this assertion has been verified
    pub verification_result: Option<VerificationResult>,
    pub created_at: DateTime<Utc>,
    pub verified_at: Option<DateTime<Utc>>,
    // Legacy fields for backward compatibility
    pub step: Option<usize>,
    pub tool_name: Option<String>,
    pub expected: Option<String>,
    pub observed: Option<String>,
    pub passed: Option<bool>,
    pub confidence: Option<f64>,
    pub screenshot_hash_legacy: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
}

/// Log of an error during execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorLog {
    pub timestamp: DateTime<Utc>,
    pub step: usize,
    pub error: String,
    pub recovered: bool,
}

/// Git state at checkpoint time
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitCheckpointInfo {
    pub branch: String,
    pub commit_hash: String,
    pub dirty: bool,
    pub staged_files: Vec<String>,
    pub modified_files: Vec<String>,
}

/// Anti-thrash guard counters persisted across resume. These are in-memory on
/// the agent and otherwise reset to 0 on every restart — so a watchdog that
/// auto-resumes a crash-looping task would hand it fresh rope forever, turning
/// crash-loops into amnesiac infinite loops. Persisting them lets the guards
/// (prefill breaker, mutation-gate abort, no-action abort) fire ACROSS resumes.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct GuardCounters {
    #[serde(default)]
    pub consecutive_no_action_prompts: usize,
    #[serde(default)]
    pub mutation_gate_rejections: usize,
    #[serde(default)]
    pub prefill_400_count: usize,
    /// Verification ledger (external review finding: stale verification
    /// after resume). Without these, a resumed task restarted all three
    /// counters at 0 — `last_successful >= mutation_sequence` held
    /// immediately, so the completion gate treated UNVERIFIED pre-checkpoint
    /// edits as verified.
    #[serde(default)]
    pub mutation_sequence: usize,
    #[serde(default)]
    pub last_successful_verification_mutation_sequence: usize,
    #[serde(default)]
    pub last_failed_verification_mutation_sequence: usize,
    /// Summary of the most recent failed verification, for the gate's
    /// refusal message after resume.
    #[serde(default)]
    pub last_failed_verification_summary: Option<String>,
    /// Outstanding verification failures with the scope and check identity
    /// each concerned.
    ///
    /// The summary string above survived resume but the structure did not, so
    /// a resumed run could no longer tell a failure in its OWN project from one
    /// in an enclosing workspace — every restored failure blocked. Persisting
    /// the records keeps the scoped gate working across a resume.
    #[serde(default)]
    pub verification_failures: crate::agent::verification_scope::VerificationLedger,
}

/// Represents the delta/diff between two checkpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointDelta {
    pub task_id: String,
    pub base_version: u32,
    pub target_version: u32,

    // Updates
    pub updated_at: DateTime<Utc>,
    pub status: Option<TaskStatus>,
    pub current_step: Option<usize>,
    pub current_iteration: Option<usize>,

    /// Full ledger state at the delta's target version.
    ///
    /// Not a diff: the ledger is append-only but its satisfaction marks mutate
    /// in place, so a "new obligations" list would lose discharges. Carrying
    /// the whole value is small and correct. Without it, an incremental save
    /// resumed with stale evidence while the full-save path looked fine.
    #[serde(default)]
    pub evidence_ledger: Option<crate::phi::ledger::Ledger>,

    // Context additions (we only append messages in the context window)
    pub new_messages: Vec<Message>,
    pub new_memory_entries: Vec<MemoryEntry>,
    pub new_tool_calls: Vec<ToolCallLog>,
    pub new_errors: Vec<ErrorLog>,
    pub new_visual_assertions: Vec<VisualAssertion>,

    pub updated_tokens: Option<usize>,
    // Cumulative budget consumption — carried in the delta so incremental saves
    // don't lose it (otherwise resume reconstructs a stale budget and resets it).
    #[serde(default)]
    pub cumulative_tokens: Option<usize>,
    #[serde(default)]
    pub elapsed_wall_secs: Option<u64>,
    #[serde(default)]
    pub cumulative_cost_usd: Option<f64>,
    #[serde(default)]
    pub guard_counters: Option<GuardCounters>,
    // Adaptive iteration-budget state and the chain-wide iteration total —
    // carried in the delta for the same reason as the budget totals above:
    // a grant firing between full saves must still survive a resume.
    #[serde(default)]
    pub effective_max_iterations: Option<usize>,
    #[serde(default)]
    pub extensions_granted: Option<usize>,
    #[serde(default)]
    pub cumulative_iterations: Option<usize>,
    pub git_checkpoint: Option<GitCheckpointInfo>,

    // Visual assertion state (changes are always recorded, None means no change)
    pub pending_visual_assertion: Option<Option<VisualAssertion>>,
}

/// A complete checkpoint of task state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCheckpoint {
    #[serde(default = "default_version")]
    pub version: u32,
    pub task_id: String,
    pub task_description: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: TaskStatus,
    pub current_step: usize,
    #[serde(default)]
    pub current_iteration: usize,
    /// Auto-continuation ("chain") count consumed by this task so far, so the
    /// bounded auto-continue enforcement survives a process restart: a resumed
    /// run keeps counting against `agent::loop_control::MAX_AUTO_CONTINUES`
    /// instead of getting a fresh budget of 3 chains per restart.
    #[serde(default)]
    pub auto_continue_count: usize,

    // Context state
    pub messages: Vec<Message>,
    pub memory_entries: Vec<MemoryEntry>,
    pub estimated_tokens: usize,

    /// Shadow-mode evidence ledger. `#[serde(default)]` so checkpoints written
    /// before it existed still load — an absent ledger is an empty one, which
    /// is honest: those sessions recorded nothing.
    #[serde(default)]
    pub evidence_ledger: crate::phi::ledger::Ledger,

    // Execution log
    pub tool_calls: Vec<ToolCallLog>,
    pub errors: Vec<ErrorLog>,

    // Visual assertions
    #[serde(default)]
    pub visual_assertions: Vec<VisualAssertion>,
    #[serde(default)]
    pub pending_visual_assertion: Option<VisualAssertion>, // Current assertion to verify

    // Git state
    pub git_checkpoint: Option<GitCheckpointInfo>,

    // Cumulative budget consumed across ALL run segments, so resume/recovery
    // cannot reset it (otherwise N resumes = N× the configured budget).
    /// Total tokens consumed so far across every segment of this task.
    #[serde(default)]
    pub cumulative_tokens: usize,
    /// Active wall-clock seconds consumed so far across every segment (excludes
    /// time the task was paused/not running).
    #[serde(default)]
    pub elapsed_wall_secs: u64,
    /// Total USD cost consumed so far across every segment, so a resumed run
    /// keeps counting against `max_cost_usd` instead of resetting the cap.
    #[serde(default)]
    pub cumulative_cost_usd: f64,
    /// Anti-thrash guard counters carried across resume so a crash-looping task
    /// can't reset its way out of the guards on every restart.
    #[serde(default)]
    pub guard_counters: GuardCounters,

    /// The EFFECTIVE iteration cap at checkpoint time: the configured cap
    /// plus any adaptive +25% grants earned so far (`AgentLoop::max_iterations`).
    /// Restored on resume so a productive run's earned budget survives a
    /// restart instead of silently shrinking back to the configured cap
    /// (2026-09-22 long-horizon finding). `None` on legacy checkpoints —
    /// resume then keeps the configured cap, the pre-field behavior.
    #[serde(default)]
    pub effective_max_iterations: Option<usize>,
    /// Adaptive budget grants consumed when this checkpoint was written (the
    /// +25%×4 ceiling accounting in `AgentLoop::extend_budget_once`).
    /// Persisted alongside `effective_max_iterations` so a resumed run
    /// cannot re-earn grants already spent.
    #[serde(default)]
    pub extensions_granted: usize,
    /// Chain-wide iteration count at checkpoint time: the initial run plus
    /// every auto-continue/resume segment accumulated. The per-segment loop
    /// counter resets on resume for budget fairness; THIS total powers the
    /// end-of-run summary so a chained/resumed run reports the whole task's
    /// work, not just the final segment.
    #[serde(default)]
    pub cumulative_iterations: usize,

    /// Hard budget caps themselves, carried across resume. `AgentConfig` marks
    /// these `#[serde(skip)]` (CLI-only), so without persisting them here a
    /// resume that doesn't re-pass `--max-budget-tokens`/`--max-wall-secs`/
    /// `--max-cost-usd` would run uncapped — even though the cumulative
    /// consumption above is restored. Restored on resume unless the CLI
    /// overrides them.
    #[serde(default)]
    pub max_budget_tokens: Option<usize>,
    #[serde(default)]
    pub max_wall_secs: Option<u64>,
    #[serde(default)]
    pub max_cost_usd: Option<f64>,
    /// Workspace identity: the canonical absolute working directory the task
    /// was created in. Startup auto-resume (`--autocontinue`) requires an
    /// exact match against the CURRENT working directory, so a task created
    /// in one repository can never be pulled into another. `None` on legacy
    /// checkpoints written before the field existed — those are never
    /// auto-resumed (their provenance cannot be validated), only resumed by
    /// explicit `resume <id>`.
    #[serde(default)]
    pub project_root: Option<String>,
}

impl TaskCheckpoint {
    fn touch(&mut self) {
        self.version = self.version.saturating_add(1);
        self.updated_at = Utc::now();
    }

    /// Computes a differential payload to reduce disk IO during saves
    pub fn compute_delta(&self, base: &TaskCheckpoint) -> Option<CheckpointDelta> {
        if self.task_id != base.task_id || self.version <= base.version {
            return None;
        }

        let status = (self.status != base.status).then_some(self.status.clone());
        let current_step = (self.current_step != base.current_step).then_some(self.current_step);
        let current_iteration =
            (self.current_iteration != base.current_iteration).then_some(self.current_iteration);
        let updated_tokens =
            (self.estimated_tokens != base.estimated_tokens).then_some(self.estimated_tokens);
        let cumulative_tokens =
            (self.cumulative_tokens != base.cumulative_tokens).then_some(self.cumulative_tokens);
        let elapsed_wall_secs =
            (self.elapsed_wall_secs != base.elapsed_wall_secs).then_some(self.elapsed_wall_secs);
        let cumulative_cost_usd = (self.cumulative_cost_usd != base.cumulative_cost_usd)
            .then_some(self.cumulative_cost_usd);
        let guard_counters =
            (self.guard_counters != base.guard_counters).then(|| self.guard_counters.clone());
        // The effective cap only ever GROWS in-task (grants add onto it), so
        // "changed" always means a new Some value; the flatten keeps the
        // delta field None when nothing moved.
        let effective_max_iterations = (self.effective_max_iterations
            != base.effective_max_iterations)
            .then_some(self.effective_max_iterations)
            .flatten();
        let extensions_granted =
            (self.extensions_granted != base.extensions_granted).then_some(self.extensions_granted);
        let cumulative_iterations = (self.cumulative_iterations != base.cumulative_iterations)
            .then_some(self.cumulative_iterations);
        if self.git_checkpoint != base.git_checkpoint && self.git_checkpoint.is_none() {
            // Delta format cannot encode "explicitly clear git checkpoint".
            // Force a full checkpoint write for this transition.
            return None;
        }
        let git_checkpoint = (self.git_checkpoint != base.git_checkpoint)
            .then(|| self.git_checkpoint.clone())
            .flatten();

        // Only capture appended elements. If vectors shrank or changed in place, prefer full save.
        let new_messages = if self.messages.len() >= base.messages.len() {
            self.messages[base.messages.len()..].to_vec()
        } else {
            return None;
        };
        let new_memory_entries = if self.memory_entries.len() >= base.memory_entries.len() {
            self.memory_entries[base.memory_entries.len()..].to_vec()
        } else {
            return None;
        };
        let new_tool_calls = if self.tool_calls.len() >= base.tool_calls.len() {
            self.tool_calls[base.tool_calls.len()..].to_vec()
        } else {
            return None;
        };
        let new_errors = if self.errors.len() >= base.errors.len() {
            self.errors[base.errors.len()..].to_vec()
        } else {
            return None;
        };
        let new_visual_assertions = if self.visual_assertions.len() >= base.visual_assertions.len()
        {
            self.visual_assertions[base.visual_assertions.len()..].to_vec()
        } else {
            return None;
        };

        // Check if pending visual assertion changed
        let pending_changed = self.pending_visual_assertion != base.pending_visual_assertion;
        let pending_visual_assertion =
            pending_changed.then_some(self.pending_visual_assertion.clone());

        let has_changes = status.is_some()
            || current_step.is_some()
            || current_iteration.is_some()
            || !new_messages.is_empty()
            || !new_memory_entries.is_empty()
            || !new_tool_calls.is_empty()
            || !new_errors.is_empty()
            || !new_visual_assertions.is_empty()
            || updated_tokens.is_some()
            || cumulative_tokens.is_some()
            || elapsed_wall_secs.is_some()
            || cumulative_cost_usd.is_some()
            || guard_counters.is_some()
            || effective_max_iterations.is_some()
            || extensions_granted.is_some()
            || cumulative_iterations.is_some()
            || git_checkpoint.is_some()
            || pending_changed;

        if !has_changes {
            return None;
        }

        Some(CheckpointDelta {
            evidence_ledger: Some(self.evidence_ledger.clone()),
            task_id: self.task_id.clone(),
            base_version: base.version,
            target_version: self.version,
            updated_at: self.updated_at,
            status,
            current_step,
            current_iteration,
            new_messages,
            new_memory_entries,
            new_tool_calls,
            new_errors,
            new_visual_assertions,
            updated_tokens,
            cumulative_tokens,
            elapsed_wall_secs,
            cumulative_cost_usd,
            guard_counters,
            effective_max_iterations,
            extensions_granted,
            cumulative_iterations,
            git_checkpoint,
            pending_visual_assertion,
        })
    }

    /// Applies a delta to an existing checkpoint to hydrate the full state
    pub fn apply_delta(&mut self, delta: &CheckpointDelta) -> Result<()> {
        if self.task_id != delta.task_id {
            return Err(anyhow::anyhow!("Delta task ID mismatch"));
        }
        if self.version != delta.base_version {
            return Err(anyhow::anyhow!(
                "Delta base version mismatch: expected {}, got {}",
                self.version,
                delta.base_version
            ));
        }

        self.version = delta.target_version;
        if let Some(ledger) = &delta.evidence_ledger {
            self.evidence_ledger = ledger.clone();
        }
        self.updated_at = delta.updated_at;

        if let Some(ref status) = delta.status {
            self.status = status.clone();
        }
        if let Some(step) = delta.current_step {
            self.current_step = step;
        }
        if let Some(iter) = delta.current_iteration {
            self.current_iteration = iter;
        }
        self.messages.extend(delta.new_messages.clone());
        self.memory_entries.extend(delta.new_memory_entries.clone());
        self.tool_calls.extend(delta.new_tool_calls.clone());
        self.errors.extend(delta.new_errors.clone());
        self.visual_assertions
            .extend(delta.new_visual_assertions.clone());

        if let Some(ref pending) = delta.pending_visual_assertion {
            self.pending_visual_assertion = pending.clone();
        }

        if let Some(tokens) = delta.updated_tokens {
            self.estimated_tokens = tokens;
        }
        if let Some(tokens) = delta.cumulative_tokens {
            self.cumulative_tokens = tokens;
        }
        if let Some(secs) = delta.elapsed_wall_secs {
            self.elapsed_wall_secs = secs;
        }
        if let Some(cost) = delta.cumulative_cost_usd {
            self.cumulative_cost_usd = cost;
        }
        if let Some(ref gc) = delta.guard_counters {
            self.guard_counters = gc.clone();
        }
        if let Some(cap) = delta.effective_max_iterations {
            self.effective_max_iterations = Some(cap);
        }
        if let Some(grants) = delta.extensions_granted {
            self.extensions_granted = grants;
        }
        if let Some(iterations) = delta.cumulative_iterations {
            self.cumulative_iterations = iterations;
        }
        if let Some(ref git) = delta.git_checkpoint {
            self.git_checkpoint = Some(git.clone());
        }

        Ok(())
    }
}

/// Summary of a task for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    pub task_id: String,
    pub task_description: String,
    pub status: TaskStatus,
    pub current_step: usize,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tool_call_count: usize,
    pub error_count: usize,
    /// Workspace the task was created in, when recorded (`None` for
    /// pre-feature checkpoints). Used by startup auto-resume to refuse tasks
    /// that do not belong to the current workspace.
    #[serde(default)]
    pub project_root: Option<String>,
}

impl TaskCheckpoint {
    /// Create a new checkpoint for a task
    pub fn new(task_id: String, task_description: String) -> Self {
        let now = Utc::now();
        Self {
            evidence_ledger: crate::phi::ledger::Ledger::new(),
            version: CURRENT_CHECKPOINT_VERSION,
            task_id,
            task_description,
            created_at: now,
            updated_at: now,
            status: TaskStatus::InProgress,
            current_step: 0,
            current_iteration: 0,
            auto_continue_count: 0,
            messages: Vec::new(),
            memory_entries: Vec::new(),
            estimated_tokens: 0,
            tool_calls: Vec::new(),
            errors: Vec::new(),
            visual_assertions: Vec::new(),
            pending_visual_assertion: None,
            git_checkpoint: None,
            cumulative_tokens: 0,
            elapsed_wall_secs: 0,
            cumulative_cost_usd: 0.0,
            guard_counters: GuardCounters::default(),
            effective_max_iterations: None,
            extensions_granted: 0,
            cumulative_iterations: 0,
            max_budget_tokens: None,
            max_wall_secs: None,
            max_cost_usd: None,
            // Record which workspace owns this task, canonicalized so a
            // symlinked path (e.g. /tmp → /private/tmp on macOS) compares
            // equal at selection time. None only when the cwd is unavailable.
            project_root: std::env::current_dir()
                .ok()
                .and_then(|cwd| cwd.canonicalize().ok())
                .map(|cwd| cwd.to_string_lossy().into_owned()),
        }
    }

    /// Create a summary of this checkpoint
    pub fn to_summary(&self) -> TaskSummary {
        TaskSummary {
            task_id: self.task_id.clone(),
            task_description: self.task_description.clone(),
            status: self.status.clone(),
            current_step: self.current_step,
            created_at: self.created_at,
            updated_at: self.updated_at,
            tool_call_count: self.tool_calls.len(),
            error_count: self.errors.len(),
            project_root: self.project_root.clone(),
        }
    }

    /// Add a tool call log entry
    pub fn log_tool_call(&mut self, log: ToolCallLog) {
        self.tool_calls.push(log);
        self.touch();
    }

    /// Add a visual assertion log entry
    pub fn log_visual_assertion(&mut self, assertion: VisualAssertion) {
        self.visual_assertions.push(assertion);
        self.touch();
    }

    /// Set a pending visual assertion that must be verified before continuing
    pub fn set_pending_visual_assertion(&mut self, assertion: VisualAssertion) {
        self.pending_visual_assertion = Some(assertion);
        self.touch();
    }

    /// Add an error log entry
    pub fn log_error(&mut self, step: usize, error: String, recovered: bool) {
        self.errors.push(ErrorLog {
            timestamp: Utc::now(),
            step,
            error,
            recovered,
        });
        self.touch();
    }

    /// Update the step
    pub fn set_step(&mut self, step: usize) {
        self.current_step = step;
        self.touch();
    }

    /// Update the loop iteration count
    pub fn set_iteration(&mut self, iteration: usize) {
        self.current_iteration = iteration;
        self.touch();
    }

    /// Update the status
    pub fn set_status(&mut self, status: TaskStatus) {
        self.status = status;
        self.touch();
    }

    /// Update messages
    pub fn set_messages(&mut self, messages: Vec<Message>) {
        self.messages = messages;
        self.touch();
    }

    /// Update token estimate and bump checkpoint version.
    pub fn set_estimated_tokens(&mut self, estimated_tokens: usize) {
        self.estimated_tokens = estimated_tokens;
        self.touch();
    }
}

/// Manager for saving and loading task checkpoints
pub struct CheckpointManager {
    checkpoints_dir: PathBuf,
}

/// Outcome of loading a checkpoint with explicit recovery semantics (review
/// finding: unrecoverable corruption was reported as a successful fresh
/// "resume", erasing the distinction between continuation and a new task).
#[derive(Debug)]
pub enum CheckpointLoad {
    /// Primary file (plus deltas) loaded cleanly.
    Clean(TaskCheckpoint),
    /// Primary was unreadable; restored from the `.json.bak` backup.
    RecoveredFromBackup(TaskCheckpoint),
    /// Primary and backup are both unreadable (or the task does not exist).
    /// A fresh start must be an explicit operator decision, never a silent
    /// substitute for the requested resume.
    RecoveryRequired { task_id: String, reason: String },
}

/// Maximum number of incremental deltas before forcing a compacted full write.
const MAX_DELTA_ENTRIES_BEFORE_COMPACT: usize = 24;
/// Maximum delta log size before forcing compaction.
const MAX_DELTA_FILE_BYTES: u64 = 512 * 1024;
/// Maximum number of checkpoint files to retain on disk.  Older checkpoints
/// (and their matching `.delta.jsonl`) are pruned best-effort after each save.
const MAX_CHECKPOINT_FILES: usize = 500;

/// Sanitize a `task_id` for safe use as a filename inside `checkpoints_dir`.
///
/// Returns `Ok(sanitized)` when the task_id is safe (possibly with unsafe
/// characters replaced), or `Err` when the result would still escape the
/// checkpoints directory after sanitization.
///
/// - Rejects `..` path-traversal segments.
/// - Replaces `/` and `\` with `_` so the task_id cannot contain path
///   separators.
/// - Trims leading/trailing dots and whitespace to avoid hidden files or
///   directory escape.
fn sanitize_task_id(task_id: &str) -> Result<String> {
    // Reject any ".." segment immediately — this is the primary traversal vector.
    if task_id.split('/').any(|seg| seg == "..")
        || task_id.split('\\').any(|seg| seg == "..")
        || task_id == ".."
    {
        bail!(
            "task_id contains a '..' traversal segment and is rejected: {:?}",
            task_id
        );
    }

    // Replace path separators with underscores.
    let sanitized = task_id.replace(['/', '\\'], "_");

    // Trim leading/trailing dots and whitespace to avoid hidden files or
    // degenerate names.
    let trimmed = sanitized
        .trim_matches(['.', ' ', '\t', '\n', '\r'])
        .to_string();

    if trimmed.is_empty() {
        bail!("task_id is empty after sanitization");
    }

    Ok(trimmed)
}

/// A process-wide advisory lock guarding the read→modify→write cycle of one
/// session file (a checkpoint, its delta log, a chat, …) against concurrent
/// CLI/daemon instances working on the same task.
///
/// The lock lives on a SIBLING `<target>.lock` file, never the target
/// itself: full-checkpoint writes RENAME the target (tmp → target and
/// target → `.bak`), so a lock held on the renamed inode would stop
/// protecting the file the moment its name moved. The sibling is created
/// once, is never renamed, and is intentionally never deleted — an unlinking
/// writer would strand blocked waiters on an orphaned inode while new
/// writers lock a fresh one, silently breaking mutual exclusion. Lock files
/// are tiny (usually empty) and are exempt from retention pruning.
///
/// Contract: the caller holds the guard for the WHOLE read→modify→atomic
/// write cycle; two concurrent writers for the same task serialize even when
/// one is mid-cycle.
///
/// Implementation notes:
/// * Unix: `flock(LOCK_EX)` on the sibling, opened O_CLOEXEC so a spawned
///   subprocess never inherits (and never pins) a task lock. `flock` locks
///   are released by the kernel when the descriptor closes, so a crashed
///   process cannot leave a stale lock behind. Release is explicit
///   (`LOCK_UN` on Drop) as in `crate::phi::activity`.
/// * Other platforms: advisory file locking is unavailable without an extra
///   dependency, so the guard is a documented no-op. Atomic replace
///   ([`replace_atomically`]) still prevents torn writes there.
///
/// Reentrancy: `flock` locks are per open-file-description, so a second
/// flock from the SAME thread on a fresh descriptor would block forever
/// against its own lock. A thread-local registry downgrades nested acquires
/// in the same thread to no-ops — required because e.g.
/// `recover_from_corruption` re-saves while `load_with_status` already holds
/// the lock, and `apply_deltas` locks when called from inside `save`'s lock.
pub(crate) struct FileLock {
    /// Path of the `<target>.lock` sibling file.
    lock_path: PathBuf,
    /// Whether this guard actually holds the OS lock (vs. a reentrant no-op
    /// or a non-Unix stub).
    held: bool,
    /// The open descriptor pinning the lock; closed on release.
    #[cfg(unix)]
    _file: Option<std::fs::File>,
}

thread_local! {
    /// Lock files this THREAD currently holds (see reentrancy note above).
    /// `flock` would deadlock on a same-thread second acquisition, so it is
    /// only ever exercised across threads/processes.
    static HELD_LOCKS: std::cell::RefCell<std::collections::HashSet<PathBuf>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// Sibling lock path for `target` (e.g. `task.json` → `task.json.lock`).
#[cfg(unix)]
fn lock_sibling_path(target: &std::path::Path) -> PathBuf {
    let mut os = target.as_os_str().to_os_string();
    os.push(".lock");
    PathBuf::from(os)
}

impl FileLock {
    /// Acquire the exclusive advisory lock for `target`, blocking until any
    /// other writer releases it. Must be held across the whole
    /// read→modify→atomic-write cycle.
    #[cfg(unix)]
    pub(crate) fn acquire(target: &std::path::Path) -> Result<Self> {
        Self::acquire_with(target, libc::LOCK_EX)
    }

    /// Acquire the exclusive advisory lock without blocking; errors
    /// immediately (EWOULDBLOCK) when another writer holds it. Test-only
    /// seam for proving mutual exclusion (no non-test caller yet).
    #[cfg(unix)]
    #[allow(dead_code)] // test seam — used by checkpoint/chat_store tests only
    pub(crate) fn try_acquire(target: &std::path::Path) -> Result<Self> {
        Self::acquire_with(target, libc::LOCK_EX | libc::LOCK_NB)
    }

    #[cfg(unix)]
    fn acquire_with(target: &std::path::Path, flags: i32) -> Result<Self> {
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::OpenOptionsExt;
        let lock_path = lock_sibling_path(target);

        // Reentrancy: this thread already holds the lock (nested no-op).
        if HELD_LOCKS.with(|held| held.borrow().contains(&lock_path)) {
            return Ok(Self {
                lock_path,
                held: false,
                _file: None,
            });
        }

        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .custom_flags(libc::O_CLOEXEC)
            .open(&lock_path)
            .with_context(|| format!("Failed to open advisory lock file {:?}", lock_path))?;

        if unsafe { libc::flock(file.as_raw_fd(), flags) } != 0 {
            return Err(std::io::Error::last_os_error())
                .with_context(|| format!("Failed to acquire advisory lock {:?}", lock_path));
        }

        HELD_LOCKS.with(|held| held.borrow_mut().insert(lock_path.clone()));
        Ok(Self {
            lock_path,
            held: true,
            _file: Some(file),
        })
    }

    /// Non-Unix stub: no advisory locking without an extra dependency. The
    /// atomic-replace fallback ([`replace_atomically`]) still keeps writes
    /// consistent.
    #[cfg(not(unix))]
    pub(crate) fn acquire(_target: &std::path::Path) -> Result<Self> {
        Ok(Self {
            lock_path: PathBuf::new(),
            held: false,
        })
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        if !self.held {
            return;
        }
        HELD_LOCKS.with(|held| held.borrow_mut().remove(&self.lock_path));
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            if let Some(file) = &self._file {
                unsafe {
                    libc::flock(file.as_raw_fd(), libc::LOCK_UN);
                }
            }
        }
    }
}

/// Replace `dst` with `src` (both on the same filesystem) with the
/// remove-destination-then-retry fallback for platforms where `rename`
/// refuses to overwrite an existing destination (Windows semantics).
/// On Unix this is a straight atomic rename. On any platform, a double
/// failure cleans up `src` before returning the error.
///
/// This is the shared form of the fallback convention established in
/// [`CheckpointManager::save_full_checkpoint`]; chat saves and undo
/// restores use it so they survive the same Windows rename restriction.
pub(crate) fn replace_atomically(
    src: &std::path::Path,
    dst: &std::path::Path,
) -> std::io::Result<()> {
    replace_atomically_with(src, dst, |s, d| std::fs::rename(s, d))
}

/// [`replace_atomically`] with an injectable rename operation (test seam:
/// a fake Windows-style rename can exercise the remove-then-retry sequence
/// on any platform).
fn replace_atomically_with<F>(
    src: &std::path::Path,
    dst: &std::path::Path,
    rename: F,
) -> std::io::Result<()>
where
    F: Fn(&std::path::Path, &std::path::Path) -> std::io::Result<()>,
{
    match rename(src, dst) {
        Ok(()) => Ok(()),
        Err(first_err) => {
            // On Windows, rename fails when the destination already exists:
            // remove the destination and retry before giving up.
            if dst.exists() {
                if let Err(remove_err) = fs::remove_file(dst) {
                    let _ = fs::remove_file(src);
                    return Err(std::io::Error::new(
                        remove_err.kind(),
                        format!(
                            "failed to remove existing destination {:?} for atomic replace (original rename error: {first_err})",
                            dst
                        ),
                    ));
                }
                match rename(src, dst) {
                    Ok(()) => Ok(()),
                    Err(retry_err) => {
                        let _ = fs::remove_file(src);
                        Err(std::io::Error::new(
                            retry_err.kind(),
                            format!(
                                "failed to rename {:?} from {:?} after removing the destination",
                                dst, src
                            ),
                        ))
                    }
                }
            } else {
                let _ = fs::remove_file(src);
                Err(first_err)
            }
        }
    }
}

/// Parse one line of a delta log (new envelope format, falling back to a
/// legacy bare delta), verifying envelope integrity.
fn parse_delta_line(line: &str, path: &std::path::Path, line_no: usize) -> Result<CheckpointDelta> {
    if let Ok(envelope) = serde_json::from_str::<CheckpointEnvelope>(line) {
        envelope.verify().with_context(|| {
            format!(
                "Checkpoint delta integrity check failed for {:?} line {}",
                path, line_no
            )
        })?;
        serde_json::from_value::<CheckpointDelta>(envelope.payload).with_context(|| {
            format!(
                "Failed to deserialize checkpoint delta from {:?} line {}",
                path, line_no
            )
        })
    } else {
        serde_json::from_str::<CheckpointDelta>(line).with_context(|| {
            format!(
                "Failed to deserialize legacy checkpoint delta from {:?} line {}",
                path, line_no
            )
        })
    }
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub fn new(checkpoints_dir: PathBuf) -> Result<Self> {
        // Create directory if it doesn't exist
        if !checkpoints_dir.exists() {
            fs::create_dir_all(&checkpoints_dir).with_context(|| {
                format!(
                    "Failed to create checkpoints directory: {:?}",
                    checkpoints_dir
                )
            })?;
        }
        // Checkpoints may contain conversation data and tool output — keep the
        // directory owner-only on Unix.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&checkpoints_dir, fs::Permissions::from_mode(0o700));
        }
        Ok(Self { checkpoints_dir })
    }

    /// Create a checkpoint manager with default directory
    pub fn default_path() -> Result<Self> {
        let home = dirs_home();
        let checkpoints_dir = home.join(".selfware").join("checkpoints");
        Self::new(checkpoints_dir)
    }

    /// Get the path for a checkpoint file.
    ///
    /// The `task_id` is sanitized via [`sanitize_task_id`] to prevent path
    /// traversal (e.g. `../evil` is rejected or neutralized).
    fn checkpoint_path(&self, task_id: &str) -> Result<PathBuf> {
        let safe_id = sanitize_task_id(task_id)?;
        let path = self.checkpoints_dir.join(format!("{}.json", safe_id));
        self.verify_path_in_dir(&path)?;
        Ok(path)
    }

    /// Get the path for a checkpoint delta log.
    ///
    /// The `task_id` is sanitized via [`sanitize_task_id`] to prevent path
    /// traversal.
    fn checkpoint_delta_path(&self, task_id: &str) -> Result<PathBuf> {
        let safe_id = sanitize_task_id(task_id)?;
        let path = self
            .checkpoints_dir
            .join(format!("{}.delta.jsonl", safe_id));
        self.verify_path_in_dir(&path)?;
        Ok(path)
    }

    /// Defence in depth: verify (by lexical prefix check) that `path` is
    /// inside `checkpoints_dir`.  Uses canonicalize when the path exists,
    /// otherwise falls back to a starts_with check on the components.
    fn verify_path_in_dir(&self, path: &std::path::Path) -> Result<()> {
        // If both paths exist, use canonicalize for a robust check.
        if let (Ok(canon_dir), Ok(canon_path)) = (
            std::fs::canonicalize(&self.checkpoints_dir),
            std::fs::canonicalize(path),
        ) {
            if !canon_path.starts_with(&canon_dir) {
                bail!(
                    "checkpoint path {:?} escapes checkpoints_dir {:?}",
                    path,
                    self.checkpoints_dir
                );
            }
            return Ok(());
        }
        // Fallback: `path` may not exist yet (about to be created), so it can't
        // be canonicalized directly. Canonicalize its PARENT (which does exist)
        // and re-append the file name, so BOTH sides resolve symlinks — e.g. on
        // macOS a temp dir under /var canonicalizes to /private/var, and
        // comparing a canonicalized dir against a raw /var path spuriously fails.
        let dir = self
            .checkpoints_dir
            .canonicalize()
            .unwrap_or_else(|_| self.checkpoints_dir.clone());
        let resolved_path = match (path.parent(), path.file_name()) {
            (Some(parent), Some(name)) => match parent.canonicalize() {
                Ok(canon_parent) => canon_parent.join(name),
                Err(_) => path.to_path_buf(),
            },
            _ => path.to_path_buf(),
        };
        if !resolved_path.starts_with(&dir) {
            bail!(
                "checkpoint path {:?} escapes checkpoints_dir {:?}",
                path,
                self.checkpoints_dir
            );
        }
        Ok(())
    }

    /// Save a checkpoint to disk (with secrets redacted and integrity hash).
    ///
    /// Security: The checkpoint data is run through `redact::redact_json()`
    /// before writing, which scrubs API keys, passwords, bearer tokens, and
    /// other sensitive patterns from all serialized string values.  The
    /// `TaskCheckpoint` struct intentionally does not include config-level
    /// secrets such as `api_key` -- those live only in `Config`.
    ///
    /// Integrity: An HMAC-SHA-256 digest is computed over the JSON payload and
    /// stored in a wrapper envelope so that `load()` can verify the file has
    /// not been corrupted or tampered with.
    pub fn save(&self, checkpoint: &TaskCheckpoint) -> Result<()> {
        let full_path = self.checkpoint_path(&checkpoint.task_id)?;

        // Advisory lock held across the WHOLE read → modify → atomic-write
        // cycle, so concurrent CLI/daemon instances on the same task
        // serialize instead of interleaving (delta appends, full-write
        // renames and delta-log truncation must never overlap).
        let _lock = FileLock::acquire(&full_path)?;

        // Prefer a compact delta write when possible to reduce SSD wear.
        if full_path.exists() {
            if let Ok(mut base) = self.try_load_from_path(&full_path) {
                if let Err(e) = self.apply_deltas(&checkpoint.task_id, &mut base) {
                    tracing::warn!(
                        "Failed to hydrate checkpoint with deltas before save ({}). Falling back to full save.",
                        e
                    );
                    self.save_full_checkpoint(checkpoint)?;
                    self.clear_delta_log(&checkpoint.task_id)?;
                    self.prune_old_checkpoints();
                    return Ok(());
                }

                if let Some(delta) = checkpoint.compute_delta(&base) {
                    if self.delta_is_efficient(checkpoint, &delta)? {
                        match self.append_delta(&checkpoint.task_id, &delta) {
                            Ok(()) => {
                                if self.should_compact_deltas(&checkpoint.task_id)? {
                                    self.save_full_checkpoint(checkpoint)?;
                                    self.clear_delta_log(&checkpoint.task_id)?;
                                }
                                self.prune_old_checkpoints();
                                return Ok(());
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to append checkpoint delta: {}. Falling back to full save.",
                                    e
                                );
                            }
                        }
                    }
                }
            }
        }

        // Fallback to full checkpoint write when no efficient delta exists.
        self.save_full_checkpoint(checkpoint)?;
        self.clear_delta_log(&checkpoint.task_id)?;
        self.prune_old_checkpoints();
        Ok(())
    }

    /// Persist a terminal checkpoint as a FULL write (not a delta) and clear the
    /// delta log, so the base checkpoint file itself reflects the final
    /// status/step. Used at task finalization — a delta save would leave the
    /// base frozen at in_progress/step 1 for anything that reads the base file.
    pub fn save_final(&self, checkpoint: &TaskCheckpoint) -> Result<()> {
        let _lock = FileLock::acquire(&self.checkpoint_path(&checkpoint.task_id)?)?;
        self.save_full_checkpoint(checkpoint)?;
        self.clear_delta_log(&checkpoint.task_id)?;
        self.prune_old_checkpoints();
        Ok(())
    }

    fn delta_is_efficient(
        &self,
        checkpoint: &TaskCheckpoint,
        delta: &CheckpointDelta,
    ) -> Result<bool> {
        let full_size = serde_json::to_vec(checkpoint)
            .context("Failed to estimate full checkpoint size")?
            .len();
        let delta_size = serde_json::to_vec(delta)
            .context("Failed to estimate checkpoint delta size")?
            .len();

        // Require a meaningful reduction, not just a few bytes.
        Ok(delta_size + 128 < full_size)
    }

    fn append_delta(&self, task_id: &str, delta: &CheckpointDelta) -> Result<()> {
        let path = self.checkpoint_delta_path(task_id)?;
        let mut json_value =
            serde_json::to_value(delta).context("Failed to serialize checkpoint delta")?;
        redact::redact_json(&mut json_value);
        let envelope = CheckpointEnvelope::wrap(json_value)
            .context("Failed to create checkpoint delta envelope")?;
        let line = serde_json::to_string(&envelope)
            .context("Failed to serialize checkpoint delta envelope")?;

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("Failed to open checkpoint delta log {:?}", path))?;
        file.write_all(line.as_bytes())
            .with_context(|| format!("Failed to write checkpoint delta log {:?}", path))?;
        file.write_all(b"\n")
            .with_context(|| format!("Failed to write checkpoint delta newline {:?}", path))?;
        file.sync_all()
            .with_context(|| format!("Failed to fsync checkpoint delta log {:?}", path))?;
        Ok(())
    }

    fn should_compact_deltas(&self, task_id: &str) -> Result<bool> {
        let path = self.checkpoint_delta_path(task_id)?;
        if !path.exists() {
            return Ok(false);
        }

        let metadata = fs::metadata(&path)
            .with_context(|| format!("Failed to stat checkpoint delta log {:?}", path))?;
        if metadata.len() > MAX_DELTA_FILE_BYTES {
            return Ok(true);
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read checkpoint delta log {:?}", path))?;
        let line_count = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();
        Ok(line_count >= MAX_DELTA_ENTRIES_BEFORE_COMPACT)
    }

    fn clear_delta_log(&self, task_id: &str) -> Result<()> {
        let delta_path = self.checkpoint_delta_path(task_id)?;
        if delta_path.exists() {
            fs::remove_file(&delta_path).with_context(|| {
                format!("Failed to delete checkpoint delta log {:?}", delta_path)
            })?;
        }
        Ok(())
    }

    /// Best-effort retention pruning: keep at most [`MAX_CHECKPOINT_FILES`]
    /// checkpoint `.json` files (by mtime, most recent first) and delete the
    /// rest along with their matching `.delta.jsonl` and `.json.bak` files.
    ///
    /// This is best-effort — any delete errors are logged and swallowed so a
    /// pruning failure never fails a `save`.
    fn prune_old_checkpoints(&self) {
        // Prune the per-task subdirectories FIRST — they grow independently of
        // the flat `.json` files (one `<task_id>/failure_mode.json` dir per run)
        // and the `.json` logic below early-returns when few checkpoints exist,
        // which is exactly when the dirs still pile up (observed: thousands).
        self.prune_old_task_dirs();

        let entries = match fs::read_dir(&self.checkpoints_dir) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("prune_old_checkpoints: failed to read dir: {}", e);
                return;
            }
        };

        // Collect (path, mtime) for .json files (excluding .bak and .tmp).
        let mut json_files: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            // Only consider .json files (not .json.bak, .json.tmp.*, .delta.jsonl).
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            // Skip backup files.
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if stem.ends_with(".bak") || stem.ends_with(".tmp") {
                    continue;
                }
            }
            let mtime = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            json_files.push((path, mtime));
        }

        if json_files.len() <= MAX_CHECKPOINT_FILES {
            return;
        }

        // Sort by mtime descending (most recent first).
        json_files.sort_by_key(|b| std::cmp::Reverse(b.1));

        let to_delete = &json_files[MAX_CHECKPOINT_FILES..];
        for (path, _) in to_delete {
            // Derive the delta and backup paths from the checkpoint path.
            // The path is `checkpoints_dir/<task_id>.json`.
            // Delta:  `checkpoints_dir/<task_id>.delta.jsonl`
            // Backup:  `checkpoints_dir/<task_id>.json.bak`
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();

            // Delete the checkpoint file.
            if let Err(e) = fs::remove_file(path) {
                tracing::warn!("prune_old_checkpoints: failed to delete {:?}: {}", path, e);
            }

            // Delete matching delta log.
            let delta_path = self.checkpoints_dir.join(format!("{}.delta.jsonl", stem));
            if delta_path.exists() {
                if let Err(e) = fs::remove_file(&delta_path) {
                    tracing::warn!(
                        "prune_old_checkpoints: failed to delete delta {:?}: {}",
                        delta_path,
                        e
                    );
                }
            }

            // Delete matching backup.
            let bak_path = path.with_extension("json.bak");
            if bak_path.exists() {
                if let Err(e) = fs::remove_file(&bak_path) {
                    tracing::warn!(
                        "prune_old_checkpoints: failed to delete backup {:?}: {}",
                        bak_path,
                        e
                    );
                }
            }
        }

        tracing::debug!(
            "prune_old_checkpoints: pruned {} checkpoint(s) exceeding cap of {}",
            to_delete.len(),
            MAX_CHECKPOINT_FILES,
        );
    }

    /// Cap the per-task subdirectories in the checkpoints dir at
    /// [`MAX_CHECKPOINT_FILES`], deleting the oldest (by mtime). Best-effort:
    /// errors are logged and swallowed.
    fn prune_old_task_dirs(&self) {
        let entries = match fs::read_dir(&self.checkpoints_dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        let mut dirs: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let mtime = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            dirs.push((entry.path(), mtime));
        }
        if dirs.len() <= MAX_CHECKPOINT_FILES {
            return;
        }
        dirs.sort_by_key(|b| std::cmp::Reverse(b.1)); // most recent first
        for (path, _) in &dirs[MAX_CHECKPOINT_FILES..] {
            if let Err(e) = fs::remove_dir_all(path) {
                tracing::warn!("prune_old_task_dirs: failed to remove {:?}: {}", path, e);
            }
        }
    }

    fn save_full_checkpoint(&self, checkpoint: &TaskCheckpoint) -> Result<()> {
        let path = self.checkpoint_path(&checkpoint.task_id)?;

        // Serialize to JSON value first so we can redact secrets
        let mut json_value =
            serde_json::to_value(checkpoint).context("Failed to serialize checkpoint")?;

        // Redact any secrets in the checkpoint data
        redact::redact_json(&mut json_value);

        // Wrap in an integrity envelope
        let envelope =
            CheckpointEnvelope::wrap(json_value).context("Failed to create checkpoint envelope")?;

        let json =
            serde_json::to_string_pretty(&envelope).context("Failed to format checkpoint JSON")?;

        // Atomic write: write to a temp file in the same directory, then rename.
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp_path = path.with_extension(format!(
            "json.tmp.{}.{}.{}",
            checkpoint.task_id,
            std::process::id(),
            suffix
        ));
        {
            let mut open_opts = fs::OpenOptions::new();
            open_opts.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                open_opts.mode(0o600);
            }
            let mut tmp_file = open_opts
                .open(&tmp_path)
                .with_context(|| format!("Failed to create checkpoint temp file {:?}", tmp_path))?;
            tmp_file
                .write_all(json.as_bytes())
                .with_context(|| format!("Failed to write checkpoint temp file {:?}", tmp_path))?;
            tmp_file
                .sync_all()
                .with_context(|| format!("Failed to fsync checkpoint temp file {:?}", tmp_path))?;
        }
        // Keep a backup of the previous checkpoint so it can be recovered
        if path.exists() {
            let backup_path = path.with_extension("json.bak");
            if let Err(e) = fs::rename(&path, &backup_path) {
                tracing::warn!("Failed to create checkpoint backup: {}", e);
            }
        }

        // Atomic replace (tmp → path), with the remove-destination-then-retry
        // fallback for Windows, where rename fails when the destination
        // exists. The caller holds the task's advisory lock across this
        // whole cycle.
        replace_atomically(&tmp_path, &path).with_context(|| {
            format!(
                "Failed to atomically replace checkpoint {:?} from {:?}",
                path, tmp_path
            )
        })?;
        #[cfg(unix)]
        {
            if let Some(parent) = path.parent() {
                let dir = fs::OpenOptions::new()
                    .read(true)
                    .open(parent)
                    .with_context(|| {
                        format!("Failed to open checkpoint directory for fsync {:?}", parent)
                    })?;
                dir.sync_all().with_context(|| {
                    format!("Failed to fsync checkpoint directory {:?}", parent)
                })?;
            }
        }
        Ok(())
    }

    /// Load a checkpoint from disk, verifying integrity.
    ///
    /// Supports both the new envelope format (with HMAC integrity digest) and the
    /// legacy bare-checkpoint format for backward compatibility.
    ///
    /// If the primary file is corrupted (invalid JSON, truncated, failed
    /// integrity check), this automatically attempts recovery via
    /// [`recover_from_corruption`](Self::recover_from_corruption).
    pub fn load(&self, task_id: &str) -> Result<TaskCheckpoint> {
        match self.load_with_status(task_id)? {
            CheckpointLoad::Clean(checkpoint) | CheckpointLoad::RecoveredFromBackup(checkpoint) => {
                Ok(checkpoint)
            }
            CheckpointLoad::RecoveryRequired { task_id, reason } => {
                // Honest status over optimistic success (AGENTS.md rule 3): a
                // fresh checkpoint is NOT a successful resume. The caller
                // decides whether to start a new task explicitly.
                Err(anyhow::anyhow!(
                    "checkpoint for task '{task_id}' is unrecoverable ({reason}); \
                     refusing to report a blank checkpoint as a resume — start a \
                     new task explicitly if you intend to"
                ))
            }
        }
    }

    /// Load a checkpoint with explicit recovery semantics (review finding:
    /// checkpoint loss was reported as a successful fresh "resume", erasing
    /// the distinction between continuation and a new task).
    pub fn load_with_status(&self, task_id: &str) -> Result<CheckpointLoad> {
        let path = self.checkpoint_path(task_id)?;
        // Reading may HEAL the delta log (truncating a torn tail), and
        // recovery re-saves the primary, so hold the task lock across the
        // whole load. Nested acquires (recovery → save) are no-ops via the
        // reentrancy registry.
        let _lock = FileLock::acquire(&path)?;

        // Load the PRIMARY file on its own. Only a genuine failure to read
        // or verify the primary itself may trigger backup recovery. A broken
        // DELTA log is NOT primary corruption: treating it as such used to
        // overwrite a healthy, newer primary with an older `.bak` copy and
        // discard every valid delta (crash-cascade bug — a power loss tearing
        // the final delta line cascaded into the PRIMARY being rolled back).
        let mut checkpoint = match self.try_load_from_path(&path) {
            Ok(checkpoint) => checkpoint,
            Err(primary_err) => {
                tracing::warn!(
                    "Primary checkpoint load failed for {:?}: {}. Attempting recovery.",
                    path,
                    primary_err
                );
                return match self.recover_from_corruption(task_id)? {
                    Some(checkpoint) => Ok(CheckpointLoad::RecoveredFromBackup(checkpoint)),
                    None => Ok(CheckpointLoad::RecoveryRequired {
                        task_id: task_id.to_string(),
                        reason: format!("{primary_err}"),
                    }),
                };
            }
        };

        // Delta replay: a torn trailing line is tolerated (and healed) inside
        // apply_deltas; any other failure is delta-log corruption. Either way
        // the healthy primary is preserved exactly as it is — a backup is
        // never preferred over it.
        self.apply_deltas(task_id, &mut checkpoint)?;

        Ok(CheckpointLoad::Clean(checkpoint))
    }

    /// Replay the task's delta log onto `checkpoint`.
    ///
    /// Torn-write tolerance: the log is append-only, so a crash/power loss
    /// mid-`append_delta` leaves a truncated FINAL line (no trailing
    /// newline) holding no committed delta. That line is skipped with a
    /// warning and its partial bytes are truncated from the log (self-
    /// healing: otherwise the next append would sandwich the torn record
    /// between valid ones and turn a recoverable tail into a hard error).
    /// Genuine failures on a COMPLETE or non-final line still fail the
    /// replay — and `load_with_status` never lets a delta failure masquerade
    /// as primary corruption.
    fn apply_deltas(&self, task_id: &str, checkpoint: &mut TaskCheckpoint) -> Result<()> {
        // Reading may HEAL the log (truncate a torn tail), so hold the task
        // lock across the whole replay. Nested acquires from save() etc. are
        // no-ops via the reentrancy registry.
        let _lock = FileLock::acquire(&self.checkpoint_path(task_id)?)?;
        let path = self.checkpoint_delta_path(task_id)?;
        if !path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read checkpoint delta log {:?}", path))?;

        // Torn-write signature: an interrupted append never emitted the
        // terminating newline, so the file body ends mid-record.
        let ends_with_newline = content.ends_with('\n');
        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();

        let mut torn_tail_len: Option<usize> = None;
        for (idx, line) in lines.iter().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let is_final = idx + 1 == total;
            match parse_delta_line(line, &path, idx + 1) {
                Ok(delta) => {
                    checkpoint.apply_delta(&delta).with_context(|| {
                        format!(
                            "Failed to apply checkpoint delta from {:?} line {}",
                            path,
                            idx + 1
                        )
                    })?;
                }
                Err(e) if is_final && !ends_with_newline => {
                    // The final record never completed on disk; nothing
                    // committed, nothing to lose. Skip it and heal the log.
                    tracing::warn!(
                        "Checkpoint delta log {:?} ends with a truncated record at line {} ({}); the write was interrupted mid-line — dropping the partial tail so all valid deltas still apply",
                        path,
                        idx + 1,
                        e
                    );
                    // The torn line ends the file without a newline, so its
                    // start offset (in bytes) is the heal point.
                    torn_tail_len = Some(content.len() - line.len());
                    break;
                }
                Err(e) => return Err(e),
            }
        }

        if let Some(truncate_to) = torn_tail_len {
            let file = fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .with_context(|| {
                    format!(
                        "Failed to open checkpoint delta log {:?} to repair its torn tail",
                        path
                    )
                })?;
            file.set_len(truncate_to as u64).with_context(|| {
                format!(
                    "Failed to truncate torn tail of checkpoint delta log {:?}",
                    path
                )
            })?;
        }

        Ok(())
    }

    /// Attempt to load and verify a checkpoint from a specific path.
    fn try_load_from_path(&self, path: &std::path::Path) -> Result<TaskCheckpoint> {
        let json = fs::read_to_string(path)
            .with_context(|| format!("Failed to read checkpoint from {:?}", path))?;

        // Try to parse as an envelope first (new format with integrity check)
        if let Ok(envelope) = serde_json::from_str::<CheckpointEnvelope>(&json) {
            // Verify integrity before deserializing the payload
            envelope
                .verify()
                .with_context(|| format!("Checkpoint integrity check failed for {:?}", path))?;

            let checkpoint: TaskCheckpoint = serde_json::from_value(envelope.payload)
                .context("Failed to deserialize checkpoint from envelope payload")?;
            return Ok(checkpoint);
        }

        // Fall back to legacy format (bare checkpoint without envelope)
        let checkpoint: TaskCheckpoint =
            serde_json::from_str(&json).context("Failed to deserialize checkpoint")?;
        Ok(checkpoint)
    }

    /// Attempt to recover a corrupted checkpoint.
    ///
    /// Strategy:
    /// 0. FIRST re-check the primary: recovery must never prefer an older
    ///    backup over a healthy primary. The caller only reaches this after
    ///    a primary load failure, but that failure may have been transient
    ///    (or the file repaired meanwhile) — if the primary parses NOW, it
    ///    wins and no backup is consulted or written.
    /// 1. Then try the `.json.bak` backup (created by [`Self::save`]).
    /// 2. If the backup is also unusable, return `None` — the caller decides
    ///    how to surface recovery-required state. No fresh checkpoint is
    ///    created here: a blank checkpoint must never masquerade as a resume
    ///    (review finding), and creating one would overwrite the evidence.
    ///
    /// Only genuine PRIMARY parse/integrity failures reach this function:
    /// `load_with_status` never routes a delta-log failure here, so a broken
    /// delta log can no longer roll a healthy primary back to an older
    /// backup (crash-cascade fix).
    pub fn recover_from_corruption(&self, task_id: &str) -> Result<Option<TaskCheckpoint>> {
        let _lock = FileLock::acquire(&self.checkpoint_path(task_id)?)?;
        let primary_path = self.checkpoint_path(task_id)?;

        // Step 0: never prefer an older backup over a healthier primary.
        if let Ok(checkpoint) = self.try_load_from_path(&primary_path) {
            tracing::warn!(
                "Primary checkpoint {:?} is readable despite the earlier failure; keeping it over any backup.",
                primary_path
            );
            return Ok(Some(checkpoint));
        }

        let backup_path = primary_path.with_extension("json.bak");

        // Attempt 1: try the backup file
        if backup_path.exists() {
            match self.try_load_from_path(&backup_path) {
                Ok(checkpoint) => {
                    tracing::info!(
                        "Recovered checkpoint for task '{}' from backup {:?}",
                        task_id,
                        backup_path
                    );
                    // Re-save the recovered checkpoint as the primary file so
                    // subsequent loads succeed without hitting recovery again.
                    if let Err(e) = self.save(&checkpoint) {
                        tracing::warn!(
                            "Failed to re-save recovered checkpoint for '{}': {}",
                            task_id,
                            e
                        );
                    }
                    return Ok(Some(checkpoint));
                }
                Err(e) => {
                    tracing::warn!("Backup checkpoint {:?} is also corrupt: {}", backup_path, e);
                }
            }
        }

        // Both primary and backup are unreadable. The task description, message
        // history, and audit trail are gone, and any filesystem changes made
        // before the crash are ORPHANED. Surface that loudly — and return None
        // so the caller reports RecoveryRequired instead of a fake resume.
        tracing::warn!(
            "DATA LOSS: checkpoint for task '{}' and its backup are both unreadable. \
             Prior messages/audit are lost and any uncommitted file changes from \
             before the crash are now untracked — review the working tree manually.",
            task_id
        );
        Ok(None)
    }

    /// Save a checkpoint with retry and exponential backoff.
    ///
    /// Attempts up to 3 saves with delays of 100 ms, 500 ms, and 2000 ms
    /// between failures.  Each failure is logged.  Returns the first success
    /// or the last error.
    pub fn save_with_retry(&self, checkpoint: &TaskCheckpoint) -> Result<()> {
        const DELAYS_MS: [u64; 3] = [100, 500, 2000];

        let mut last_err: Option<anyhow::Error> = None;

        for (attempt, delay_ms) in DELAYS_MS.iter().enumerate() {
            if attempt > 0 {
                if let Some(ref e) = last_err {
                    tracing::warn!(
                        "Checkpoint save attempt {}/3 failed for task '{}': {}. Retrying in {} ms.",
                        attempt,
                        checkpoint.task_id,
                        e,
                        delay_ms
                    );
                }
                std::thread::sleep(std::time::Duration::from_millis(*delay_ms));
            }

            match self.save(checkpoint) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.map_or_else(
            || {
                anyhow::anyhow!(
                    "Checkpoint save failed: all {} retry attempts exhausted",
                    DELAYS_MS.len()
                )
            },
            |e| {
                anyhow::anyhow!(
                    "Checkpoint save failed after {} attempts: {}",
                    DELAYS_MS.len(),
                    e
                )
            },
        ))
    }

    /// List all saved tasks
    pub fn list_tasks(&self) -> Result<Vec<TaskSummary>> {
        let mut summaries = Vec::new();

        if !self.checkpoints_dir.exists() {
            return Ok(summaries);
        }

        for entry in fs::read_dir(&self.checkpoints_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(mut checkpoint) = self.try_load_from_path(&path) {
                    if let Some(task_id) = path.file_stem().and_then(|s| s.to_str()) {
                        if let Err(e) = self.apply_deltas(task_id, &mut checkpoint) {
                            tracing::warn!(
                                "Skipping checkpoint {:?} due to invalid deltas: {}",
                                path,
                                e
                            );
                            continue;
                        }
                    }
                    summaries.push(checkpoint.to_summary());
                }
            }
        }

        // Sort by updated_at descending (most recent first)
        summaries.sort_by_key(|x| std::cmp::Reverse(x.updated_at));

        Ok(summaries)
    }

    /// Find the most recently updated checkpoint that is SAFE to auto-resume at
    /// startup — meaning it belongs to the CURRENT workspace and was left
    /// mid-run (or was stopped by the iteration cap while otherwise healthy).
    ///
    /// Two gates:
    /// - **Workspace**: `workspace` is the canonical absolute working
    ///   directory of the current process, and only checkpoints whose recorded
    ///   `project_root` matches it exactly are eligible. A task created in
    ///   another repository must never be pulled into this one — resuming
    ///   project A's instructions inside project B would execute A's file
    ///   edits against B's tree. Legacy checkpoints without a recorded
    ///   `project_root` cannot be validated and are skipped (they remain
    ///   reachable via explicit `resume <id>`).
    /// - **Status**: [`TaskStatus::InProgress`] qualifies (a crash or restart
    ///   leaves the status InProgress), and so does a [`TaskStatus::Failed`]
    ///   checkpoint whose terminal stop is the iteration/step-cap family —
    ///   see [`terminal_stop_allows_autochain`]. `Paused` tasks and every
    ///   other failure class stay explicit-`resume` territory: a crashed or
    ///   safety-stopped checkpoint must not auto-chain.
    ///
    /// Ordering and hydration match [`Self::list_tasks`]: delta logs are
    /// replayed (so a delta that flips a task to Completed is visible even
    /// when the base file is stale), unreadable entries are skipped, and the
    /// result is the newest eligible checkpoint by hydrated `updated_at`.
    /// Returns `Ok(None)` when no eligible checkpoint exists.
    pub fn latest_autoresumable_task(&self, workspace: &str) -> Result<Option<TaskSummary>> {
        // Summaries are already newest-first. A Failed candidate needs its
        // terminal stop reason, which the summary does not carry — hydrate
        // the full checkpoint for those (rare, and only within the current
        // workspace). An unrecoverable checkpoint is skipped, never chained.
        for summary in self.list_tasks()? {
            if summary.project_root.as_deref() != Some(workspace) {
                continue;
            }
            match summary.status {
                TaskStatus::InProgress => return Ok(Some(summary)),
                TaskStatus::Failed => match self.load(&summary.task_id) {
                    Ok(checkpoint) if terminal_stop_allows_autochain(&checkpoint) => {
                        return Ok(Some(summary));
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(
                            "auto-resume: skipping unrecoverable failed checkpoint '{}': {}",
                            summary.task_id,
                            e
                        );
                    }
                },
                _ => {}
            }
        }
        Ok(None)
    }

    /// Delete a checkpoint
    pub fn delete(&self, task_id: &str) -> Result<()> {
        let path = self.checkpoint_path(task_id)?;
        // Primary + backup + delta log are removed as one unit under the
        // task's advisory lock so a concurrent writer observes either all
        // or nothing.
        let _lock = FileLock::acquire(&path)?;
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("Failed to delete checkpoint: {:?}", path))?;
        }
        let backup_path = path.with_extension("json.bak");
        if backup_path.exists() {
            fs::remove_file(&backup_path).with_context(|| {
                format!("Failed to delete checkpoint backup: {:?}", backup_path)
            })?;
        }
        let delta_path = self.checkpoint_delta_path(task_id)?;
        if delta_path.exists() {
            fs::remove_file(&delta_path).with_context(|| {
                format!("Failed to delete checkpoint delta log: {:?}", delta_path)
            })?;
        }
        Ok(())
    }

    /// Check if a checkpoint exists (test helper)
    #[cfg(test)]
    pub fn exists(&self, task_id: &str) -> bool {
        self.checkpoint_path(task_id)
            .map(|p| p.exists())
            .unwrap_or(false)
    }

    /// Get the checkpoints directory path (test helper)
    #[cfg(test)]
    pub fn checkpoints_dir(&self) -> &PathBuf {
        &self.checkpoints_dir
    }
}

/// Whether a FAILED checkpoint's terminal stop is in the iteration/step-cap
/// family — the ONLY failure class `--autocontinue` may chain without
/// operator review. A cap stop means the run was bounded by budget
/// accounting while otherwise healthy, so chaining it continues productive
/// work; a crash leaves the status `InProgress` (already eligible), and
/// every other terminal failure — safety/killswitch stops, guard aborts
/// (READ_LOOP_NO_EDIT, WORKSPACE_STAGNATION, …), provider errors — must not
/// auto-chain. The typed `AUTO_CONTINUE_LIMIT` stop is also excluded: its
/// per-task chain budget is already spent, and re-chaining it on every
/// startup would defeat `MAX_AUTO_CONTINUES` (the bound exists to turn
/// exactly that pattern into a typed stop).
///
/// The terminal failure is the checkpoint's LAST error entry, logged
/// unrecovered by `fail_checkpoint` with the loop's reason verbatim.
fn terminal_stop_allows_autochain(checkpoint: &TaskCheckpoint) -> bool {
    checkpoint.errors.last().is_some_and(|entry| {
        !entry.recovered
            && entry.error.trim() == crate::agent::loop_control::MAX_ITERATIONS_STOP_REASON
    })
}

/// Get home directory
fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Capture current git state for checkpoint
pub fn capture_git_state(repo_path: &str) -> Option<GitCheckpointInfo> {
    let repo = git2::Repository::open(repo_path).ok()?;

    // Get current branch
    let head = repo.head().ok()?;
    let branch = head
        .shorthand()
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "HEAD".to_string());

    // Get current commit
    let commit = head.peel_to_commit().ok()?;
    let commit_hash = commit.id().to_string();

    // Check for dirty state
    let statuses = repo.statuses(None).ok()?;
    let mut staged_files = Vec::new();
    let mut modified_files = Vec::new();

    for entry in statuses.iter() {
        let status = entry.status();
        let path = entry.path().unwrap_or("").to_string();

        if status.is_index_new()
            || status.is_index_modified()
            || status.is_index_deleted()
            || status.is_index_renamed()
        {
            staged_files.push(path.clone());
        }

        if status.is_wt_new()
            || status.is_wt_modified()
            || status.is_wt_deleted()
            || status.is_wt_renamed()
        {
            modified_files.push(path);
        }
    }

    let dirty = !staged_files.is_empty() || !modified_files.is_empty();

    Some(GitCheckpointInfo {
        branch,
        commit_hash,
        dirty,
        staged_files,
        modified_files,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/session/checkpoint/checkpoint_test.rs"]
mod tests;
