//! Dream System - Background Memory Consolidation
//!
//! The Dream System consolidates and cleans up episodic memory between sessions,
//! preventing memory bloat and contradictions. It runs as a background process
//! (autoDream) that merges similar memories, resolves conflicts, and prunes stale data.
//!
//! # Three-Gate System
//!
//! Dreams only run when ALL three gates pass:
//! 1. Minimum 24 hours since last dream
//! 2. Minimum 5 sessions since last dream
//! 3. Consolidation lock available (prevents concurrent dreams)
//!
//! # Four-Phase Process
//!
//! 1. **Orient**: Load recent sessions, identify memory files to process
//! 2. **Gather**: Collect all memories from last 5 sessions
//! 3. **Consolidate**: Merge similar memories, resolve conflicts, delete contradictions
//! 4. **Prune & Index**: Cap MEMORY.md size, remove stale memories, re-index

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

/// Default maximum lines for MEMORY.md
const MAX_MEMORY_LINES: usize = 200;
/// Default maximum size for MEMORY.md in bytes (~25KB)
const MAX_MEMORY_SIZE: usize = 25 * 1024;
/// Default stale memory threshold in days
const STALE_MEMORY_DAYS: u64 = 90;
/// Default minimum hours between dreams
const MIN_HOURS_BETWEEN_DREAMS: u64 = 24;
/// Default minimum sessions between dreams
const MIN_SESSIONS_BETWEEN_DREAMS: usize = 5;

/// Dream trigger configuration (three-gate system)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DreamTrigger {
    /// Minimum hours since last dream (default: 24)
    pub min_hours_since_last: u64,
    /// Minimum sessions since last dream (default: 5)
    pub min_sessions_since_last: usize,
}

impl Default for DreamTrigger {
    fn default() -> Self {
        Self {
            min_hours_since_last: MIN_HOURS_BETWEEN_DREAMS,
            min_sessions_since_last: MIN_SESSIONS_BETWEEN_DREAMS,
        }
    }
}

impl DreamTrigger {
    /// Create a new dream trigger with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set minimum hours since last dream
    pub fn with_min_hours(mut self, hours: u64) -> Self {
        self.min_hours_since_last = hours;
        self
    }

    /// Set minimum sessions since last dream
    pub fn with_min_sessions(mut self, sessions: usize) -> Self {
        self.min_sessions_since_last = sessions;
        self
    }
}

/// Persisted dream state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DreamState {
    /// Unix timestamp of last dream
    #[serde(default)]
    pub last_dream_timestamp: u64,
    /// Number of sessions since last dream
    #[serde(default)]
    pub sessions_since_last_dream: usize,
    /// Whether consolidation is currently running (lock)
    #[serde(skip)]
    pub consolidation_lock: bool,
    /// Total number of dreams run
    #[serde(default)]
    pub dream_count: usize,
    /// Lock file path for cross-process coordination
    #[serde(skip)]
    lock_file_path: Option<PathBuf>,
}

impl DreamState {
    /// Create a new dream state
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the lock file path for cross-process coordination
    pub fn with_lock_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.lock_file_path = Some(path.into());
        self
    }

    /// Get the path to the dream state file
    fn state_file_path(base_dir: &Path) -> PathBuf {
        base_dir.join("dream_state.json")
    }

    /// Load dream state from disk
    pub fn load(base_dir: &Path) -> Result<Self> {
        let path = Self::state_file_path(base_dir);
        if !path.exists() {
            return Ok(Self::new());
        }

        let content = std::fs::read_to_string(&path)?;
        let mut state: DreamState = serde_json::from_str(&content)?;
        state.lock_file_path = Some(base_dir.join("dream.lock"));

        // Check if a lock file exists from a previous run
        if state
            .lock_file_path
            .as_ref()
            .map(|p| p.exists())
            .unwrap_or(false)
        {
            warn!("Found existing dream lock file - previous dream may have crashed");
            // Clear the stale lock
            let _ = std::fs::remove_file(state.lock_file_path.as_ref().unwrap());
        }

        Ok(state)
    }

    /// Save dream state to disk
    pub fn save(&self, base_dir: &Path) -> Result<()> {
        let path = Self::state_file_path(base_dir);
        std::fs::create_dir_all(base_dir)?;

        let json = serde_json::to_string_pretty(self)?;
        let temp_path = path.with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&temp_path, json)?;
        std::fs::rename(&temp_path, &path)?;

        Ok(())
    }

    /// Acquire the consolidation lock (gate 3)
    ///
    /// Returns true if the lock was acquired, false if another dream is running
    pub fn acquire_consolidation_lock(&mut self) -> bool {
        if self.consolidation_lock {
            return false;
        }

        // Try to create lock file for cross-process coordination
        if let Some(ref lock_path) = self.lock_file_path {
            if lock_path.exists() {
                return false;
            }
            if let Err(e) = std::fs::write(lock_path, std::process::id().to_string()) {
                warn!("Failed to create dream lock file: {}", e);
                return false;
            }
        }

        self.consolidation_lock = true;
        true
    }

    /// Release the consolidation lock
    pub fn release_consolidation_lock(&mut self) {
        self.consolidation_lock = false;

        // Remove lock file
        if let Some(ref lock_path) = self.lock_file_path {
            let _ = std::fs::remove_file(lock_path);
        }
    }

    /// Record that a session has ended
    pub fn record_session_end(&mut self) {
        self.sessions_since_last_dream += 1;
    }

    /// Record that a dream has completed
    pub fn record_dream_completed(&mut self) {
        self.last_dream_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.sessions_since_last_dream = 0;
        self.dream_count += 1;
        self.release_consolidation_lock();
    }

    /// Check if the state indicates a dream should run (without checking lock)
    pub fn should_run_dream_check_gates(&self, trigger: &DreamTrigger) -> bool {
        // Gate 1: Check minimum hours since last dream
        // If never run (timestamp is 0), we don't pass this gate
        if self.last_dream_timestamp == 0 {
            debug!("Dream gate 1 failed: no previous dream recorded");
            return false;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let hours_since_last = (now - self.last_dream_timestamp) / 3600;

        if hours_since_last < trigger.min_hours_since_last {
            debug!(
                "Dream gate 1 failed: only {} hours since last dream (need {})",
                hours_since_last, trigger.min_hours_since_last
            );
            return false;
        }

        // Gate 2: Check minimum sessions since last dream
        if self.sessions_since_last_dream < trigger.min_sessions_since_last {
            debug!(
                "Dream gate 2 failed: only {} sessions since last dream (need {})",
                self.sessions_since_last_dream, trigger.min_sessions_since_last
            );
            return false;
        }

        true
    }

    /// Get hours until next dream based on trigger
    pub fn hours_until_next(&self, trigger: &DreamTrigger) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let hours_since_last = (now - self.last_dream_timestamp) / 3600;

        trigger.min_hours_since_last.saturating_sub(hours_since_last)
    }

    /// Get sessions until next dream based on trigger
    pub fn sessions_until_next(&self, trigger: &DreamTrigger) -> usize {
        trigger.min_sessions_since_last.saturating_sub(self.sessions_since_last_dream)
    }
}

impl Drop for DreamState {
    fn drop(&mut self) {
        // Clean up lock on drop if we still hold it
        if self.consolidation_lock {
            self.release_consolidation_lock();
        }
    }
}

/// Check if a dream should run (all three gates)
///
/// Returns true only if all gates pass:
/// 1. Minimum hours since last dream
/// 2. Minimum sessions since last dream
/// 3. Consolidation lock available
pub fn should_run_dream(state: &mut DreamState, trigger: &DreamTrigger) -> bool {
    // Check gates 1 and 2
    if !state.should_run_dream_check_gates(trigger) {
        return false;
    }

    // Gate 3: Try to acquire lock
    if !state.acquire_consolidation_lock() {
        debug!("Dream gate 3 failed: consolidation already in progress");
        return false;
    }

    info!("All dream gates passed - dream should run");
    true
}

/// Represents a section in MEMORY.md
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MemorySection {
    Facts,
    Preferences,
    ArchitectureDecisions,
    Unknown(String),
}

impl MemorySection {
    fn from_header(header: &str) -> Self {
        match header.trim() {
            "Facts" | "Facts (consolidated)" => MemorySection::Facts,
            "Preferences" | "Preferences (user-defined)" => MemorySection::Preferences,
            "Architecture Decisions" => MemorySection::ArchitectureDecisions,
            other => MemorySection::Unknown(other.to_string()),
        }
    }

    fn header(&self) -> &str {
        match self {
            MemorySection::Facts => "## Facts (consolidated)",
            MemorySection::Preferences => "## Preferences (user-defined)",
            MemorySection::ArchitectureDecisions => "## Architecture Decisions",
            MemorySection::Unknown(s) => s.as_str(),
        }
    }
}

/// A single memory entry
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub date: Option<String>,
    pub content: String,
    pub section: MemorySection,
    pub raw_line: String,
}

impl MemoryEntry {
    /// Parse a memory entry from a line
    pub fn parse(line: &str, current_section: &MemorySection) -> Option<Self> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }

        // Remove bullet point prefix if present (- or *)
        let content_start = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .unwrap_or(trimmed);

        // Parse date if present: [YYYY-MM-DD] or [YYYY-MM-DD HH:MM]
        let (date, content) = if let Some(end_bracket) = content_start.find(']') {
            if content_start.starts_with('[') && end_bracket > 1 {
                let date_str = &content_start[1..end_bracket];
                let rest = content_start[end_bracket + 1..].trim().to_string();
                (Some(date_str.to_string()), rest)
            } else {
                (None, content_start.to_string())
            }
        } else {
            (None, content_start.to_string())
        };

        Some(Self {
            date,
            content,
            section: current_section.clone(),
            raw_line: line.to_string(),
        })
    }

    /// Check if this memory is stale (older than threshold days)
    pub fn is_stale(&self, threshold_days: u64) -> bool {
        let Some(ref date_str) = self.date else {
            return false; // Undated memories are never stale
        };

        let parsed_date = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d").or_else(|_| {
            // Try parsing with time component
            chrono::NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M").map(|dt| dt.date())
        });

        let Ok(memory_date) = parsed_date else {
            return false;
        };

        let today = chrono::Local::now().date_naive();
        let age_days = today.signed_duration_since(memory_date).num_days();

        age_days > threshold_days as i64
    }
}

/// Parsed MEMORY.md content
#[derive(Debug, Clone)]
pub struct MemoryStore {
    pub entries: Vec<MemoryEntry>,
    pub sections: HashMap<MemorySection, Vec<usize>>, // section -> entry indices
}

impl MemoryStore {
    /// Parse MEMORY.md content
    pub fn parse(content: &str) -> Self {
        let mut entries = Vec::new();
        let mut sections: HashMap<MemorySection, Vec<usize>> = HashMap::new();
        let mut current_section = MemorySection::Facts;

        for line in content.lines() {
            // Check for section headers
            if line.starts_with("## ") {
                current_section = MemorySection::from_header(&line[3..]);
                continue;
            }

            // Parse memory entries
            if let Some(entry) = MemoryEntry::parse(line, &current_section) {
                let idx = entries.len();
                entries.push(entry);
                sections
                    .entry(current_section.clone())
                    .or_default()
                    .push(idx);
            }
        }

        Self { entries, sections }
    }

    /// Format back to MEMORY.md format
    pub fn format(&self) -> String {
        let mut output = String::from("# Project Memory\n\n");

        // Output sections in order
        let section_order = [
            MemorySection::Facts,
            MemorySection::Preferences,
            MemorySection::ArchitectureDecisions,
        ];

        for section in &section_order {
            if let Some(indices) = self.sections.get(section) {
                if !indices.is_empty() {
                    output.push_str(section.header());
                    output.push('\n');
                    for &idx in indices {
                        if let Some(entry) = self.entries.get(idx) {
                            output.push_str(&entry.raw_line);
                            output.push('\n');
                        }
                    }
                    output.push('\n');
                }
            }
        }

        // Handle unknown sections
        for (section, indices) in &self.sections {
            if matches!(section, MemorySection::Unknown(_)) {
                output.push_str(section.header());
                output.push('\n');
                for &idx in indices {
                    if let Some(entry) = self.entries.get(idx) {
                        output.push_str(&entry.raw_line);
                        output.push('\n');
                    }
                }
                output.push('\n');
            }
        }

        output
    }

    /// Remove stale memories
    pub fn prune_stale(&mut self, threshold_days: u64) -> usize {
        let mut removed = 0;
        let mut new_entries = Vec::new();
        let mut old_to_new: HashMap<usize, usize> = HashMap::new();

        for (old_idx, entry) in self.entries.iter().enumerate() {
            if entry.is_stale(threshold_days) {
                removed += 1;
            } else {
                let new_idx = new_entries.len();
                new_entries.push(entry.clone());
                old_to_new.insert(old_idx, new_idx);
            }
        }

        // Update section indices
        for indices in self.sections.values_mut() {
            let new_indices: Vec<usize> = indices
                .iter()
                .filter_map(|&old_idx| old_to_new.get(&old_idx).copied())
                .collect();
            *indices = new_indices;
        }

        self.entries = new_entries;
        removed
    }

    /// Cap memory size by removing oldest entries
    pub fn cap_size(&mut self, max_lines: usize, max_bytes: usize) -> usize {
        let content = self.format();

        if content.lines().count() <= max_lines && content.len() <= max_bytes {
            return 0;
        }

        let mut removed = 0;
        let mut new_entries = Vec::new();
        let mut old_to_new: HashMap<usize, usize> = HashMap::new();

        // Keep entries that are not Facts (Preferences and Architecture are more important)
        let important_indices: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| !matches!(e.section, MemorySection::Facts))
            .map(|(i, _)| i)
            .collect();

        for &idx in &important_indices {
            let new_idx = new_entries.len();
            old_to_new.insert(idx, new_idx);
            new_entries.push(self.entries[idx].clone());
        }

        // Add Facts entries until we hit the limit
        let fact_indices = self
            .sections
            .get(&MemorySection::Facts)
            .cloned()
            .unwrap_or_default();
        for idx in fact_indices {
            if !important_indices.contains(&idx) {
                let test_entries: Vec<MemoryEntry> = new_entries
                    .iter()
                    .chain(std::iter::once(&self.entries[idx]))
                    .cloned()
                    .collect();

                let test_store = MemoryStore {
                    entries: test_entries,
                    sections: HashMap::new(), // Simplified check
                };
                let test_content = test_store.format();

                if test_content.lines().count() > max_lines || test_content.len() > max_bytes {
                    removed += 1;
                } else {
                    let new_idx = new_entries.len();
                    old_to_new.insert(idx, new_idx);
                    new_entries.push(self.entries[idx].clone());
                }
            }
        }

        // Update section indices
        for indices in self.sections.values_mut() {
            let new_indices: Vec<usize> = indices
                .iter()
                .filter_map(|&old_idx| old_to_new.get(&old_idx).copied())
                .collect();
            *indices = new_indices;
        }

        self.entries = new_entries;
        removed
    }

    /// Get memory statistics
    pub fn stats(&self) -> MemoryStats {
        let total_entries = self.entries.len();
        let facts_count = self
            .sections
            .get(&MemorySection::Facts)
            .map(|v| v.len())
            .unwrap_or(0);
        let preferences_count = self
            .sections
            .get(&MemorySection::Preferences)
            .map(|v| v.len())
            .unwrap_or(0);
        let architecture_count = self
            .sections
            .get(&MemorySection::ArchitectureDecisions)
            .map(|v| v.len())
            .unwrap_or(0);

        let content = self.format();

        MemoryStats {
            total_entries,
            facts_count,
            preferences_count,
            architecture_count,
            total_lines: content.lines().count(),
            total_bytes: content.len(),
        }
    }
}

/// Memory statistics
#[derive(Debug, Clone, Copy)]
pub struct MemoryStats {
    pub total_entries: usize,
    pub facts_count: usize,
    pub preferences_count: usize,
    pub architecture_count: usize,
    pub total_lines: usize,
    pub total_bytes: usize,
}

/// Configuration for the dream process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DreamConfig {
    /// Maximum lines for MEMORY.md
    pub max_memory_lines: usize,
    /// Maximum bytes for MEMORY.md
    pub max_memory_size: usize,
    /// Days after which memories are considered stale
    pub stale_memory_days: u64,
    /// Base directory for dream state
    pub base_dir: PathBuf,
    /// Trigger configuration
    pub trigger: DreamTrigger,
}

impl Default for DreamConfig {
    fn default() -> Self {
        Self {
            max_memory_lines: MAX_MEMORY_LINES,
            max_memory_size: MAX_MEMORY_SIZE,
            stale_memory_days: STALE_MEMORY_DAYS,
            base_dir: Self::default_base_dir(),
            trigger: DreamTrigger::default(),
        }
    }
}

impl DreamConfig {
    /// Get the default base directory (~/.selfware/memory)
    fn default_base_dir() -> PathBuf {
        dirs::home_dir()
            .map(|h| h.join(".selfware").join("memory"))
            .unwrap_or_else(|| PathBuf::from(".selfware").join("memory"))
    }

    /// Create a new dream config
    pub fn new() -> Self {
        Self::default()
    }

    /// Set custom base directory
    pub fn with_base_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.base_dir = path.into();
        self
    }

    /// Get the path to MEMORY.md for a project
    pub fn memory_file_path(&self, project_key: &str) -> PathBuf {
        self.base_dir.join(format!("{}_MEMORY.md", project_key))
    }

    /// Get the path to dream state
    pub fn state_path(&self) -> PathBuf {
        self.base_dir.clone()
    }
}

/// The four phases of the dream process
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DreamPhase {
    Orient,
    Gather,
    Consolidate,
    PruneAndIndex,
}

impl std::fmt::Display for DreamPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DreamPhase::Orient => write!(f, "Orient"),
            DreamPhase::Gather => write!(f, "Gather Recent Signal"),
            DreamPhase::Consolidate => write!(f, "Consolidate"),
            DreamPhase::PruneAndIndex => write!(f, "Prune & Index"),
        }
    }
}

/// Result of a dream run
#[derive(Debug, Clone)]
pub struct DreamResult {
    pub success: bool,
    pub phases_completed: Vec<DreamPhase>,
    pub memories_consolidated: usize,
    pub memories_pruned: usize,
    pub errors: Vec<String>,
    pub duration_secs: u64,
}

impl DreamResult {
    /// Create a successful result
    pub fn success(phases: Vec<DreamPhase>) -> Self {
        Self {
            success: true,
            phases_completed: phases,
            memories_consolidated: 0,
            memories_pruned: 0,
            errors: Vec::new(),
            duration_secs: 0,
        }
    }

    /// Create a failed result
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            phases_completed: Vec::new(),
            memories_consolidated: 0,
            memories_pruned: 0,
            errors: vec![error.into()],
            duration_secs: 0,
        }
    }

    /// Add a completed phase
    pub fn with_phase(mut self, phase: DreamPhase) -> Self {
        self.phases_completed.push(phase);
        self
    }

    /// Set consolidation count
    pub fn with_consolidated(mut self, count: usize) -> Self {
        self.memories_consolidated = count;
        self
    }

    /// Set prune count
    pub fn with_pruned(mut self, count: usize) -> Self {
        self.memories_pruned = count;
        self
    }
}

/// Generate LLM prompt for memory consolidation
pub fn generate_consolidation_prompt(memories: &[MemoryEntry]) -> String {
    let memory_text = memories
        .iter()
        .map(|m| {
            if let Some(ref date) = m.date {
                format!("- [{}] {}", date, m.content)
            } else {
                format!("- {}", m.content)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"You are a memory consolidation assistant. Your task is to:

1. Merge similar memories into single, more comprehensive entries
2. Convert relative dates to absolute dates (YYYY-MM-DD format)
3. Delete or flag contradicted facts (keep the most recent)
4. Resolve conflicts by keeping the most specific and accurate information

Input memories:
{}

Consolidation rules:
- Group related facts together
- Remove duplicates
- Update outdated information
- Keep user preferences intact
- Preserve architecture decisions

Output format:
## Facts (consolidated)
- [YYYY-MM-DD] Consolidated fact here
- [YYYY-MM-DD] Another consolidated fact

## Preferences (user-defined)
- Preference 1
- Preference 2

## Architecture Decisions
- Decision 1 with rationale

Only output the consolidated memory content, no explanations."#,
        memory_text
    )
}

/// Dream status for UI display
#[derive(Debug, Clone)]
pub struct DreamStatus {
    pub last_dream_timestamp: Option<u64>,
    pub sessions_since_last_dream: usize,
    pub dream_count: usize,
    pub hours_until_next: u64,
    pub sessions_until_next: usize,
    pub is_running: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_dream_trigger_default() {
        let trigger = DreamTrigger::default();
        assert_eq!(trigger.min_hours_since_last, 24);
        assert_eq!(trigger.min_sessions_since_last, 5);
    }

    #[test]
    fn test_dream_trigger_builder() {
        let trigger = DreamTrigger::new().with_min_hours(48).with_min_sessions(10);
        assert_eq!(trigger.min_hours_since_last, 48);
        assert_eq!(trigger.min_sessions_since_last, 10);
    }

    #[test]
    fn test_dream_state_gate_checks() {
        let trigger = DreamTrigger::default();

        // New state should not pass gates (0 hours, 0 sessions)
        let state = DreamState::new();
        assert!(!state.should_run_dream_check_gates(&trigger));

        // State with enough hours but not enough sessions
        let mut state = DreamState::new();
        state.last_dream_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - (25 * 3600); // 25 hours ago
        assert!(!state.should_run_dream_check_gates(&trigger));

        // State with enough sessions but not enough hours
        let mut state = DreamState::new();
        state.sessions_since_last_dream = 10;
        assert!(!state.should_run_dream_check_gates(&trigger));

        // State with both requirements met
        let mut state = DreamState::new();
        state.last_dream_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - (25 * 3600);
        state.sessions_since_last_dream = 5;
        assert!(state.should_run_dream_check_gates(&trigger));
    }

    #[test]
    fn test_should_run_dream_with_lock() {
        let trigger = DreamTrigger::default();
        let mut state = DreamState::new();

        // Set up state to pass gates 1 and 2
        state.last_dream_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - (25 * 3600);
        state.sessions_since_last_dream = 5;

        // Should acquire lock and return true
        assert!(should_run_dream(&mut state, &trigger));
        assert!(state.consolidation_lock);

        // Should fail now because lock is held
        assert!(!should_run_dream(&mut state, &trigger));

        // Release lock
        state.release_consolidation_lock();
        assert!(!state.consolidation_lock);

        // Should succeed again
        assert!(should_run_dream(&mut state, &trigger));
    }

    #[test]
    fn test_dream_state_persistence() {
        let dir = tempdir().unwrap();
        let mut state = DreamState::new();
        state.last_dream_timestamp = 12345;
        state.sessions_since_last_dream = 3;
        state.dream_count = 5;

        // Save state
        state.save(dir.path()).unwrap();

        // Load state
        let loaded = DreamState::load(dir.path()).unwrap();
        assert_eq!(loaded.last_dream_timestamp, 12345);
        assert_eq!(loaded.sessions_since_last_dream, 3);
        assert_eq!(loaded.dream_count, 5);
        assert!(!loaded.consolidation_lock);
    }

    #[test]
    fn test_memory_entry_parse() {
        let entry =
            MemoryEntry::parse("- [2026-03-31] Project uses tokio", &MemorySection::Facts).unwrap();
        assert_eq!(entry.date, Some("2026-03-31".to_string()));
        assert_eq!(entry.content, "Project uses tokio");

        let entry = MemoryEntry::parse(
            "- Always use 4-space indentation",
            &MemorySection::Preferences,
        )
        .unwrap();
        assert_eq!(entry.date, None);
        assert_eq!(entry.content, "Always use 4-space indentation");

        assert!(MemoryEntry::parse("", &MemorySection::Facts).is_none());
        assert!(MemoryEntry::parse("## Header", &MemorySection::Facts).is_none());
    }

    #[test]
    fn test_memory_store_parse_and_format() {
        let content = r#"# Project Memory

## Facts (consolidated)
- [2026-03-31] Project uses tokio for async runtime
- [2026-03-30] Prefer anyhow::Result over custom errors

## Preferences (user-defined)
- Always use 4-space indentation
- Never commit without tests

## Architecture Decisions
- Chose HNSW over flat vector search for performance
"#;

        let store = MemoryStore::parse(content);
        assert_eq!(store.entries.len(), 5); // 2 facts + 2 preferences + 1 architecture
        assert_eq!(
            store.sections.get(&MemorySection::Facts).map(|v| v.len()),
            Some(2)
        );
        assert_eq!(
            store
                .sections
                .get(&MemorySection::Preferences)
                .map(|v| v.len()),
            Some(2)
        );
        assert_eq!(
            store
                .sections
                .get(&MemorySection::ArchitectureDecisions)
                .map(|v| v.len()),
            Some(1)
        );

        // Format and re-parse should give same structure
        let reformatted = store.format();
        let reparsed = MemoryStore::parse(&reformatted);
        assert_eq!(reparsed.entries.len(), store.entries.len());
    }

    #[test]
    fn test_memory_entry_is_stale() {
        // Old entry (100 days ago)
        let old_date = (chrono::Local::now() - chrono::Duration::days(100))
            .format("%Y-%m-%d")
            .to_string();
        let old_entry = MemoryEntry {
            date: Some(old_date.clone()),
            content: "Old fact".to_string(),
            section: MemorySection::Facts,
            raw_line: format!("- [{}] Old fact", old_date),
        };
        assert!(old_entry.is_stale(90));

        // Recent entry (10 days ago)
        let recent_date = (chrono::Local::now() - chrono::Duration::days(10))
            .format("%Y-%m-%d")
            .to_string();
        let recent_entry = MemoryEntry {
            date: Some(recent_date.clone()),
            content: "Recent fact".to_string(),
            section: MemorySection::Facts,
            raw_line: format!("- [{}] Recent fact", recent_date),
        };
        assert!(!recent_entry.is_stale(90));

        // Undated entry is never stale
        let undated_entry = MemoryEntry {
            date: None,
            content: "Preference".to_string(),
            section: MemorySection::Preferences,
            raw_line: "- Preference".to_string(),
        };
        assert!(!undated_entry.is_stale(90));
    }

    #[test]
    fn test_memory_store_prune_stale() {
        let content = r#"# Project Memory

## Facts (consolidated)
- [2020-01-01] Very old fact
- [2026-03-31] Recent fact

## Preferences (user-defined)
- Always use spaces
"#;

        let mut store = MemoryStore::parse(content);
        let removed = store.prune_stale(90);
        assert_eq!(removed, 1);
        assert_eq!(store.entries.len(), 2);
    }

    #[test]
    fn test_generate_consolidation_prompt() {
        let memories = vec![
            MemoryEntry {
                date: Some("2026-03-31".to_string()),
                content: "Project uses tokio".to_string(),
                section: MemorySection::Facts,
                raw_line: "- [2026-03-31] Project uses tokio".to_string(),
            },
            MemoryEntry {
                date: None,
                content: "Use 4-space indent".to_string(),
                section: MemorySection::Preferences,
                raw_line: "- Use 4-space indent".to_string(),
            },
        ];

        let prompt = generate_consolidation_prompt(&memories);
        assert!(prompt.contains("Project uses tokio"));
        assert!(prompt.contains("memory consolidation"));
        assert!(prompt.contains("Facts (consolidated)"));
    }

    #[test]
    fn test_dream_config_default() {
        let config = DreamConfig::default();
        assert_eq!(config.max_memory_lines, 200);
        assert_eq!(config.max_memory_size, 25 * 1024);
        assert_eq!(config.stale_memory_days, 90);
    }

    #[test]
    fn test_dream_phase_display() {
        assert_eq!(DreamPhase::Orient.to_string(), "Orient");
        assert_eq!(DreamPhase::Consolidate.to_string(), "Consolidate");
    }
}
