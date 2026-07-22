//! AutoDream Background Task - Memory Consolidation Runner
//!
//! This module handles running the autoDream consolidation as an in-process
//! background tokio task, independently of the main agent loop. The task:
//!
//! - Only reads session files and writes MEMORY.md (no project modifications)
//! - Uses the user's own configured endpoint/model for summarization
//! - Runs in the background, non-blocking the main process
//! - Consolidates memories and writes to MEMORY.md
//!
//! # Architecture
//!
//! ```text
//! Main Process                    AutoDream Background Task
//! ┌─────────────┐                ┌─────────────────────┐
//! │ Session ends│───spawn───────►│ 1. Orient           │
//! │ Check gates │                │ 2. Gather Signal    │
//! │ Spawn dream │                │ 3. Consolidate      │
//! │             │◄──result──────│ 4. Prune & Index    │
//! └─────────────┘                └─────────────────────┘
//! ```

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tracing::{debug, error, info, warn};

use crate::cognitive::dream::{
    DreamConfig, DreamPhase, DreamResult, DreamState, MemoryEntry, MemoryStore,
};

/// Configuration for the autoDream subprocess
#[derive(Debug, Clone)]
pub struct AutoDreamConfig {
    /// Model to use for consolidation (should be cheapest available)
    pub model: String,
    /// API endpoint for LLM calls
    pub endpoint: String,
    /// API key for the endpoint (falls back to OPENROUTER_API_KEY or LLM_API_KEY env vars)
    pub api_key: Option<String>,
    /// Timeout for the entire dream process
    pub timeout_secs: u64,
    /// Timeout for individual LLM calls
    pub llm_timeout_secs: u64,
    /// Whether to use local model if available
    pub prefer_local: bool,
}

impl Default for AutoDreamConfig {
    fn default() -> Self {
        Self {
            model: "qwen3.5-9b".to_string(), // Cheapest reasonable model
            endpoint: "http://localhost:8000/v1".to_string(),
            api_key: None,
            timeout_secs: 300, // 5 minutes total
            llm_timeout_secs: 60,
            prefer_local: true,
        }
    }
}

impl AutoDreamConfig {
    /// Create a new autoDream config
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Set the endpoint
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }


    /// Set the API key
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Build from the user's loaded selfware config (endpoint, model, API
    /// key) so dream consolidation talks to the SAME backend as the rest of
    /// the agent — not the hardcoded `localhost:8000`/`qwen3.5-9b` default
    /// that made `/dream force` ignore the configured backend. Falls back to
    /// the built-in defaults (with a warning) when no config can be loaded.
    pub fn from_user_config() -> Self {
        match crate::config::Config::load(None) {
            Ok(cfg) => Self {
                model: cfg.model,
                endpoint: cfg.endpoint,
                api_key: cfg.api_key.as_ref().map(|k| k.expose().to_string()),
                ..Self::default()
            },
            Err(e) => {
                warn!(
                    "autoDream: could not load user config ({}); using built-in defaults",
                    e
                );
                Self::default()
            }
        }
    }
}

/// AutoDream background-task handle
pub struct AutoDreamHandle {
    /// The spawned background consolidation task
    task: tokio::task::JoinHandle<Result<DreamResult>>,
    /// Project key for this dream
    project_key: String,
    /// Start time
    start_time: std::time::Instant,
}

impl AutoDreamHandle {
    /// Check if the dream task is still running
    pub async fn is_running(&mut self) -> bool {
        !self.task.is_finished()
    }

    /// Wait for the dream to complete with timeout
    pub async fn wait_with_timeout(&mut self, timeout: Duration) -> Result<DreamResult> {
        match tokio::time::timeout(timeout, &mut self.task).await {
            Ok(Ok(Ok(result))) => {
                let duration = self.start_time.elapsed().as_secs();
                if result.success {
                    info!(
                        "AutoDream completed successfully for {} in {}s",
                        self.project_key, duration
                    );
                } else {
                    warn!(
                        "AutoDream completed with errors for {}: {:?}",
                        self.project_key, result.errors
                    );
                }
                Ok(result)
            }
            Ok(Ok(Err(e))) => {
                error!("AutoDream failed for {}: {}", self.project_key, e);
                Ok(DreamResult::failure(format!("{}", e)))
            }
            Ok(Err(join_err)) => {
                error!(
                    "AutoDream task join error for {}: {}",
                    self.project_key, join_err
                );
                Err(anyhow!("Dream task join error: {}", join_err))
            }
            Err(_) => {
                warn!("AutoDream timed out for {}", self.project_key);
                self.task.abort();
                Ok(DreamResult::failure("Dream process timed out"))
            }
        }
    }

    /// Kill the dream task
    pub async fn kill(&mut self) -> Result<()> {
        info!("Killing autoDream task for {}", self.project_key);
        self.task.abort();
        Ok(())
    }
}

/// Spawn an autoDream background task for the given project
///
/// Runs the dream consolidation as an in-process background tokio task. (The
/// previous implementation spawned a `selfware` subprocess with a
/// `--dream-consolidate` CLI flag that never existed, so every auto-dream
/// child died instantly on the argument parser's unknown-flag error.)
pub async fn spawn_autodream(
    project_path: &Path,
    project_key: &str,
    config: &AutoDreamConfig,
    dream_config: &DreamConfig,
) -> Result<AutoDreamHandle> {
    info!(
        "Spawning autoDream background task for {} at {:?}",
        project_key, project_path
    );

    let project_path = project_path.to_path_buf();
    let project_key_owned = project_key.to_string();
    let config = config.clone();
    let dream_config = dream_config.clone();

    let task = tokio::spawn(async move {
        run_dream_consolidation(&project_path, &project_key_owned, &config, &dream_config).await
    });

    Ok(AutoDreamHandle {
        task,
        project_key: project_key.to_string(),
        start_time: std::time::Instant::now(),
    })
}

/// Run the dream consolidation in-process
///
/// This is the actual implementation of the four-phase dream process. It is
/// used directly by `/dream force` and by the auto-dream background task.
pub async fn run_dream_consolidation(
    project_path: &Path,
    project_key: &str,
    config: &AutoDreamConfig,
    dream_config: &DreamConfig,
) -> Result<DreamResult> {
    let start_time = std::time::Instant::now();
    let mut phases_completed = Vec::new();
    let mut errors = Vec::new();

    info!(
        "Starting in-process dream consolidation for {}",
        project_key
    );

    // Phase 1: Orient - Load recent sessions, identify memory files
    info!("Dream Phase 1: Orient");
    let session_files = match orient_phase(project_path).await {
        Ok(files) => {
            debug!("Found {} session files to process", files.len());
            phases_completed.push(DreamPhase::Orient);
            files
        }
        Err(e) => {
            error!("Orient phase failed: {}", e);
            errors.push(format!("Orient: {}", e));
            Vec::new()
        }
    };

    // Phase 2: Gather - Collect memories from last 5 sessions
    info!("Dream Phase 2: Gather Recent Signal");
    let memories = match gather_phase(&session_files, 5).await {
        Ok(mem) => {
            debug!("Gathered {} memories", mem.len());
            phases_completed.push(DreamPhase::Gather);
            mem
        }
        Err(e) => {
            error!("Gather phase failed: {}", e);
            errors.push(format!("Gather: {}", e));
            Vec::new()
        }
    };

    // Phase 3: Consolidate - Merge similar memories via the configured LLM
    info!("Dream Phase 3: Consolidate");
    let consolidation = match consolidate_phase(&memories, config).await {
        Ok(content) => {
            debug!("Consolidated {} memories", memories.len());
            phases_completed.push(DreamPhase::Consolidate);
            Some(content)
        }
        Err(e) => {
            error!("Consolidate phase failed: {}", e);
            errors.push(format!("Consolidate: {}", e));
            None
        }
    };
    let consolidated_count = consolidation
        .as_deref()
        .map(count_consolidated_entries)
        .unwrap_or(0);

    // Phase 4: Prune & Index - merge consolidated output, cap size, re-index
    info!("Dream Phase 4: Prune & Index");
    let pruned_count =
        match prune_and_index_phase(project_key, dream_config, consolidation.as_deref()).await {
            Ok(count) => {
                debug!("Pruned {} memories", count);
                phases_completed.push(DreamPhase::PruneAndIndex);
                count
            }
            Err(e) => {
                error!("Prune & Index phase failed: {}", e);
                errors.push(format!("Prune & Index: {}", e));
                0
            }
        };

    let duration = start_time.elapsed().as_secs();
    // Honest success: when the gather phase found memories but the
    // consolidation LLM call FAILED, the dream did not do its job — report
    // failure instead of the old "success with a discarded LLM response".
    // With nothing to consolidate, the housekeeping phases alone suffice.
    let consolidate_failed = !memories.is_empty() && consolidation.is_none();
    let success = !consolidate_failed && (errors.is_empty() || phases_completed.len() >= 3);

    info!(
        "Dream consolidation completed for {}: {} phases, {} consolidated, {} pruned, {}s",
        project_key,
        phases_completed.len(),
        consolidated_count,
        pruned_count,
        duration
    );

    Ok(DreamResult {
        success,
        phases_completed,
        memories_consolidated: consolidated_count,
        memories_pruned: pruned_count,
        errors,
        duration_secs: duration,
    })
}

/// Phase 1: Orient - Load recent sessions and identify memory files
async fn orient_phase(project_path: &Path) -> Result<Vec<PathBuf>> {
    let mut session_files = Vec::new();

    // Look for session log files in .selfware directory
    let selfware_dir = project_path.join(".selfware");
    if selfware_dir.exists() {
        let mut entries = tokio::fs::read_dir(&selfware_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "json" || ext == "jsonl" {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name.contains("session") || name.contains("memory") {
                        session_files.push(path);
                    }
                }
            }
        }
    }

    // Sort by modification time (most recent first)
    session_files.sort_by(|a, b| {
        let meta_a = std::fs::metadata(a).ok();
        let meta_b = std::fs::metadata(b).ok();
        match (meta_a, meta_b) {
            (Some(ma), Some(mb)) => {
                let time_a = ma.modified().ok();
                let time_b = mb.modified().ok();
                time_b.cmp(&time_a) // Reverse for newest first
            }
            _ => std::cmp::Ordering::Equal,
        }
    });

    Ok(session_files)
}

/// Phase 2: Gather - Collect memories from recent sessions
async fn gather_phase(session_files: &[PathBuf], max_sessions: usize) -> Result<Vec<MemoryEntry>> {
    use crate::cognitive::dream::MemorySection;

    let mut all_memories = Vec::new();

    for file in session_files.iter().take(max_sessions) {
        debug!("Gathering from session file: {:?}", file);

        let content = match tokio::fs::read_to_string(file).await {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read session file {:?}: {}", file, e);
                continue;
            }
        };

        // Try to parse as session log and extract memories
        // For now, we treat each line as a potential memory
        for line in content.lines() {
            if let Some(entry) = MemoryEntry::parse(line, &MemorySection::Facts) {
                all_memories.push(entry);
            }
        }
    }

    Ok(all_memories)
}

/// Phase 3: Consolidate - Run LLM to merge similar memories
///
/// Uses the standard [`crate::api::ApiClient`] path (correct
/// `/chat/completions` URL, auth, retries, credential-endpoint safety) with
/// the endpoint/model from [`AutoDreamConfig`] — which
/// [`AutoDreamConfig::from_user_config`] populates from the user's own
/// config, not a hardcoded localhost default. The consolidated markdown is
/// RETURNED and merged into MEMORY.md by phase 4 — the old implementation
/// POSTed to the bare endpoint (404), discarded the body, and still reported
/// success.
async fn consolidate_phase(memories: &[MemoryEntry], config: &AutoDreamConfig) -> Result<String> {
    if memories.is_empty() {
        return Ok(String::new());
    }

    let prompt = crate::cognitive::dream::generate_consolidation_prompt(memories);

    // Build a Config view of the AutoDream settings and go through the
    // standard API client. `ApiClient::new` enforces credential-endpoint
    // safety (no API key over plaintext remote HTTP).
    let agent = crate::config::AgentConfig {
        step_timeout_secs: config.llm_timeout_secs.max(60),
        ..Default::default()
    };
    let client_config = crate::config::Config {
        endpoint: config.endpoint.clone(),
        model: config.model.clone(),
        api_key: config
            .api_key
            .clone()
            .filter(|k| !k.is_empty())
            .map(crate::config::RedactedString::new),
        temperature: 0.3,
        max_tokens: 2048,
        agent,
        ..Default::default()
    };

    let client = crate::api::ApiClient::new(&client_config)?;
    let response = client
        .chat(
            vec![crate::api::Message::user(prompt)],
            None,
            crate::api::ThinkingMode::Disabled,
        )
        .await?;

    let content = response
        .choices
        .first()
        .map(|c| c.message.content.text().to_string())
        .unwrap_or_default();

    if content.trim().is_empty() {
        return Err(anyhow!(
            "consolidation LLM ({}) returned empty content",
            config.model
        ));
    }

    info!(
        "consolidated {} memories via {} ({})",
        memories.len(),
        config.model,
        config.endpoint
    );
    Ok(content)
}

/// Count the consolidated bullet entries under `## ` sections in the LLM's
/// consolidation output (used for honest reporting).
fn count_consolidated_entries(content: &str) -> usize {
    let mut in_section = false;
    let mut count = 0usize;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            in_section = true;
            continue;
        }
        if in_section && trimmed.starts_with("- ") {
            count += 1;
        }
    }
    count
}

/// Phase 4: Prune & Index - Cap size and remove stale memories
///
/// `consolidated` is the LLM's consolidation output from phase 3 (when it
/// succeeded): its entries are MERGED into the store before pruning, so the
/// consolidation call is not discarded. Entries identical to an existing
/// memory line are de-duplicated.
async fn prune_and_index_phase(
    project_key: &str,
    dream_config: &DreamConfig,
    consolidated: Option<&str>,
) -> Result<usize> {
    let memory_path = dream_config.memory_file_path(project_key);

    // Load existing MEMORY.md or create new
    let mut store = if memory_path.exists() {
        let content = tokio::fs::read_to_string(&memory_path).await?;
        MemoryStore::parse(&content)
    } else {
        MemoryStore {
            entries: Vec::new(),
            sections: std::collections::HashMap::new(),
        }
    };

    // Merge the phase-3 consolidation output into the store.
    if let Some(content) = consolidated {
        let incoming = MemoryStore::parse(content);
        let mut added = 0usize;
        for entry in incoming.entries {
            let is_dup = store.entries.iter().any(|e| e.content == entry.content);
            if !is_dup {
                let idx = store.entries.len();
                store
                    .sections
                    .entry(entry.section.clone())
                    .or_default()
                    .push(idx);
                store.entries.push(entry);
                added += 1;
            }
        }
        debug!("Merged {} consolidated memories into MEMORY.md", added);
    }

    // Remove stale memories
    let pruned_stale = store.prune_stale(dream_config.stale_memory_days);
    debug!("Pruned {} stale memories", pruned_stale);

    // Cap memory size
    let pruned_size = store.cap_size(dream_config.max_memory_lines, dream_config.max_memory_size);
    debug!("Pruned {} memories for size constraints", pruned_size);

    // Save back to MEMORY.md
    let output = store.format();
    if let Some(parent) = memory_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Atomic write
    let temp_path = memory_path.with_extension(format!("tmp.{}", std::process::id()));
    tokio::fs::write(&temp_path, output).await?;
    tokio::fs::rename(&temp_path, &memory_path).await?;

    let stats = store.stats();
    info!(
        "MEMORY.md updated: {} entries, {} lines, {} bytes",
        stats.total_entries, stats.total_lines, stats.total_bytes
    );

    Ok(pruned_stale + pruned_size)
}

/// Check if autoDream should run after a session ends
///
/// This is called by the main process after a session ends.
/// It checks the three gates and spawns the background task if appropriate.
pub async fn check_and_spawn_autodream(
    project_path: &Path,
    project_key: &str,
    auto_config: &AutoDreamConfig,
    dream_config: &DreamConfig,
) -> Result<Option<AutoDreamHandle>> {
    // Load or create dream state
    let mut state = DreamState::load(&dream_config.state_path())?;

    // Record that a session ended
    state.record_session_end();

    // Check if dream should run
    let trigger = dream_config.trigger.clone();
    if !crate::cognitive::dream::should_run_dream(&mut state, &trigger) {
        // Save state (sessions count was incremented)
        state.save(&dream_config.state_path())?;

        debug!(
            "AutoDream gates not passed for {}: {} sessions, last dream {} hours ago",
            project_key,
            state.sessions_since_last_dream,
            (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                - state.last_dream_timestamp)
                / 3600
        );

        return Ok(None);
    }

    // Spawn the autoDream background task
    let handle = spawn_autodream(project_path, project_key, auto_config, dream_config).await?;

    // Save state (with lock held)
    state.save(&dream_config.state_path())?;

    info!("AutoDream spawned for {}", project_key);
    Ok(Some(handle))
}

/// Get dream status for display
pub async fn get_dream_status(dream_config: &DreamConfig) -> crate::cognitive::dream::DreamStatus {
    let state = DreamState::load(&dream_config.state_path()).unwrap_or_default();
    let trigger = &dream_config.trigger;

    crate::cognitive::dream::DreamStatus {
        last_dream_timestamp: if state.last_dream_timestamp > 0 {
            Some(state.last_dream_timestamp)
        } else {
            None
        },
        sessions_since_last_dream: state.sessions_since_last_dream,
        dream_count: state.dream_count,
        hours_until_next: state.hours_until_next(trigger),
        sessions_until_next: state.sessions_until_next(trigger),
        is_running: state.consolidation_lock,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/cognitive/dream_subprocess/dream_subprocess_test.rs"]
mod tests;
