//! AutoDream Subprocess - Background Memory Consolidation Runner
//!
//! This module handles spawning the autoDream subprocess which runs independently
//! of the main Selfware process. The subprocess:
//!
//! - Gets READ-ONLY access to the project (no modifications allowed)
//! - Uses the cheapest available model for summarization
//! - Runs in the background, non-blocking the main process
//! - Consolidates memories and writes to MEMORY.md
//!
//! # Architecture
//!
//! ```text
//! Main Process                    AutoDream Subprocess
//! ┌─────────────┐                ┌─────────────────────┐
//! │ Session ends│───spawn───────►│ 1. Orient           │
//! │ Check gates │                │ 2. Gather Signal    │
//! │ Spawn dream │                │ 3. Consolidate      │
//! │             │◄──result──────│ 4. Prune & Index    │
//! └─────────────┘                └─────────────────────┘
//! ```

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;
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

    /// Set prefer local flag
    pub fn with_prefer_local(mut self, prefer: bool) -> Self {
        self.prefer_local = prefer;
        self
    }

    /// Set the API key
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }
}

/// AutoDream subprocess handle
pub struct AutoDreamHandle {
    /// The spawned process
    process: tokio::process::Child,
    /// Project key for this dream
    project_key: String,
    /// Start time
    start_time: std::time::Instant,
}

impl AutoDreamHandle {
    /// Check if the dream process is still running
    pub async fn is_running(&mut self) -> bool {
        match self.process.try_wait() {
            Ok(None) => true,
            Ok(Some(_)) => false,
            Err(_) => false,
        }
    }

    /// Wait for the dream to complete with timeout
    pub async fn wait_with_timeout(&mut self, timeout: Duration) -> Result<DreamResult> {
        let wait_result = tokio::time::timeout(timeout, self.process.wait()).await;

        match wait_result {
            Ok(Ok(exit_status)) => {
                let duration = self.start_time.elapsed().as_secs();

                if exit_status.success() {
                    info!(
                        "AutoDream completed successfully for {} in {}s",
                        self.project_key, duration
                    );
                    Ok(DreamResult::success(vec![
                        DreamPhase::Orient,
                        DreamPhase::Gather,
                        DreamPhase::Consolidate,
                        DreamPhase::PruneAndIndex,
                    ])
                    .with_consolidated(0)) // Count would come from output parsing
                } else {
                    let code = exit_status.code().unwrap_or(-1);
                    warn!(
                        "AutoDream failed with exit code {} for {}",
                        code, self.project_key
                    );
                    Ok(DreamResult::failure(format!(
                        "Process exited with code {}",
                        code
                    )))
                }
            }
            Ok(Err(e)) => {
                error!("Failed to wait for autoDream: {}", e);
                Err(anyhow!("Process wait error: {}", e))
            }
            Err(_) => {
                warn!("AutoDream timed out for {}", self.project_key);
                let _ = self.process.start_kill();
                Ok(DreamResult::failure("Dream process timed out"))
            }
        }
    }

    /// Kill the dream process
    pub async fn kill(&mut self) -> Result<()> {
        info!("Killing autoDream process for {}", self.project_key);
        self.process.start_kill()?;
        Ok(())
    }
}

/// Spawn an autoDream subprocess for the given project
///
/// This spawns a separate process that runs the dream consolidation.
/// The subprocess has read-only access to project files.
pub async fn spawn_autodream(
    project_path: &Path,
    project_key: &str,
    config: &AutoDreamConfig,
    dream_config: &DreamConfig,
) -> Result<AutoDreamHandle> {
    info!(
        "Spawning autoDream subprocess for {} at {:?}",
        project_key, project_path
    );

    // Get the path to the current executable
    let current_exe = std::env::current_exe()?;

    // Build command arguments for the subprocess mode
    let memory_file = dream_config.memory_file_path(project_key);

    // Create the command with read-only file access
    // Note: We use --dream-mode flag to indicate this is a dream subprocess
    let mut cmd = Command::new(&current_exe);
    cmd.arg("--dream-consolidate")
        .arg("--project-path")
        .arg(project_path)
        .arg("--project-key")
        .arg(project_key)
        .arg("--memory-file")
        .arg(&memory_file)
        .arg("--model")
        .arg(&config.model)
        .arg("--endpoint")
        .arg(&config.endpoint)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null()); // No stdin for safety

    // Set environment variables for the subprocess
    // Mark as read-only mode - subprocess should not modify project files
    cmd.env("SELFWARE_DREAM_MODE", "1");
    cmd.env("SELFWARE_READONLY", "1");

    // Spawn the process
    let process = cmd
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn autoDream subprocess: {}", e))?;

    info!("AutoDream subprocess spawned with PID: {:?}", process.id());

    Ok(AutoDreamHandle {
        process,
        project_key: project_key.to_string(),
        start_time: std::time::Instant::now(),
    })
}

/// Run the dream consolidation in-process (for testing or when subprocess is disabled)
///
/// This is the actual implementation of the four-phase dream process.
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

    // Phase 3: Consolidate - Merge similar memories
    info!("Dream Phase 3: Consolidate");
    let consolidated_count = match consolidate_phase(&memories, config).await {
        Ok(count) => {
            debug!("Consolidated {} memories", count);
            phases_completed.push(DreamPhase::Consolidate);
            count
        }
        Err(e) => {
            error!("Consolidate phase failed: {}", e);
            errors.push(format!("Consolidate: {}", e));
            0
        }
    };

    // Phase 4: Prune & Index - Cap size, remove stale, re-index
    info!("Dream Phase 4: Prune & Index");
    let pruned_count =
        match prune_and_index_phase(project_key, dream_config, consolidated_count).await {
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
    let success = errors.is_empty() || phases_completed.len() >= 3; // Allow 1 phase to fail

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

/// Phase 3: Consolidate - Run LLM to merge similar memories (STUB)
///
/// ⚠️ WARNING: This is a STUB implementation. It does NOT actually call an LLM
/// to consolidate memories. It simply returns the input count without any
/// deduplication or merging.
/// TODO: Implement actual LLM call for memory consolidation
async fn consolidate_phase(memories: &[MemoryEntry], config: &AutoDreamConfig) -> Result<usize> {
    if memories.is_empty() {
        return Ok(0);
    }

    let prompt = crate::cognitive::dream::generate_consolidation_prompt(memories);
    let api_key = config
        .api_key
        .clone()
        .or_else(|| std::env::var("OPENROUTER_API_KEY").ok())
        .or_else(|| std::env::var("LLM_API_KEY").ok())
        .unwrap_or_default();

    crate::config::api_key::assert_credential_endpoint_safe(&config.endpoint, !api_key.is_empty())?;

    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "model": config.model,
        "messages": [
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.3,
        "max_tokens": 2048,
    });

    let mut request = client
        .post(&config.endpoint)
        .header("Content-Type", "application/json")
        .timeout(Duration::from_secs(config.llm_timeout_secs))
        .json(&body);
    if !api_key.is_empty() {
        request = request.header("Authorization", format!("Bearer {api_key}"));
    }

    let response = request.send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(anyhow!("consolidation LLM request failed: {status} {text}"));
    }

    let json: serde_json::Value = response.json().await?;
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("");

    if content.is_empty() {
        warn!("consolidation LLM returned empty content");
        return Ok(memories.len());
    }

    // Count consolidated bullets under each section.
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

    info!(
        "consolidated {} memories into {} entries via {}",
        memories.len(),
        count,
        config.model
    );
    Ok(count.max(1))
}

/// Phase 4: Prune & Index - Cap size and remove stale memories
async fn prune_and_index_phase(
    project_key: &str,
    dream_config: &DreamConfig,
    _new_memories_count: usize,
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
/// It checks the three gates and spawns the subprocess if appropriate.
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

    // Spawn the autoDream subprocess
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
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_autodream_config_default() {
        let config = AutoDreamConfig::default();
        assert_eq!(config.model, "qwen3.5-9b");
        assert_eq!(config.timeout_secs, 300);
        assert!(config.prefer_local);
    }

    #[tokio::test]
    async fn test_orient_phase() {
        let dir = tempdir().unwrap();

        // Create some session files
        tokio::fs::create_dir(dir.path().join(".selfware"))
            .await
            .unwrap();
        tokio::fs::write(dir.path().join(".selfware").join("session_1.json"), "{}")
            .await
            .unwrap();
        tokio::fs::write(dir.path().join(".selfware").join("memory_log.jsonl"), "")
            .await
            .unwrap();

        let files = orient_phase(dir.path()).await.unwrap();
        assert_eq!(files.len(), 2);
    }

    #[tokio::test]
    async fn test_gather_phase_empty() {
        let files: Vec<PathBuf> = Vec::new();
        let memories = gather_phase(&files, 5).await.unwrap();
        assert!(memories.is_empty());
    }

    #[tokio::test]
    async fn test_prune_and_index_creates_memory_file() {
        let dir = tempdir().unwrap();
        let dream_config = DreamConfig::new().with_base_dir(dir.path());

        let count = prune_and_index_phase("test_project", &dream_config, 0)
            .await
            .unwrap();
        assert_eq!(count, 0);

        // MEMORY.md should be created
        let memory_path = dream_config.memory_file_path("test_project");
        assert!(memory_path.exists());
    }

    #[test]
    fn test_dream_result_builder() {
        let result = DreamResult::success(vec![DreamPhase::Orient])
            .with_phase(DreamPhase::Gather)
            .with_consolidated(10)
            .with_pruned(5);

        assert!(result.success);
        assert_eq!(result.phases_completed.len(), 2);
        assert_eq!(result.memories_consolidated, 10);
        assert_eq!(result.memories_pruned, 5);
    }
}
