//! Episodic Memory System
//!
//! Session-based memory with semantic retrieval for learning from past
//! experiences and reconstructing context from fragments.
//!
//! Features:
//! - Episode recording and storage
//! - Semantic similarity search
//! - Pattern detection across episodes
//! - Context reconstruction
//! - Long-term memory consolidation

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::token_count::estimate_content_tokens;
use crate::vector_store::{EmbeddingBackend, VectorIndex};

/// Episode type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum EpisodeType {
    /// User request/conversation
    #[default]
    Conversation,
    /// Tool execution
    ToolExecution,
    /// Error occurrence
    Error,
    /// Successful task completion
    Success,
    /// Code change
    CodeChange,
    /// Learning/insight
    Learning,
    /// Decision made
    Decision,
}

/// Episode importance level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum Importance {
    Low = 1,
    #[default]
    Normal = 2,
    High = 3,
    Critical = 4,
}

/// A memory episode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    /// Unique identifier
    pub id: String,
    /// Episode type
    pub episode_type: EpisodeType,
    /// Main content/summary
    pub content: String,
    /// Detailed context
    pub context: HashMap<String, String>,
    /// Importance level
    pub importance: Importance,
    /// Timestamp
    pub timestamp: u64,
    /// Session ID this episode belongs to
    pub session_id: String,
    /// Related episode IDs
    pub related_episodes: Vec<String>,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Outcome (for decisions/actions)
    pub outcome: Option<EpisodeOutcome>,
    /// Access count (for importance decay)
    pub access_count: u32,
    /// Last accessed timestamp
    pub last_accessed: u64,
}

/// Outcome of an episode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeOutcome {
    /// Whether the episode led to success
    pub success: bool,
    /// Description of outcome
    pub description: String,
    /// Lessons learned
    pub lessons: Vec<String>,
}

impl Episode {
    /// Create new episode
    pub fn new(
        episode_type: EpisodeType,
        content: impl Into<String>,
        session_id: impl Into<String>,
    ) -> Self {
        let content = content.into();
        let session_id = session_id.into();

        // Generate ID from content hash + timestamp
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        hasher.update(now.to_le_bytes());
        let id = hex::encode(&hasher.finalize()[..8]);

        Self {
            id,
            episode_type,
            content,
            context: HashMap::new(),
            importance: Importance::Normal,
            timestamp: now,
            session_id,
            related_episodes: Vec::new(),
            tags: Vec::new(),
            outcome: None,
            access_count: 0,
            last_accessed: now,
        }
    }

    /// Add context
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }

    /// Set importance
    pub fn with_importance(mut self, importance: Importance) -> Self {
        self.importance = importance;
        self
    }

    /// Add tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Add related episode
    pub fn with_related(mut self, episode_id: impl Into<String>) -> Self {
        self.related_episodes.push(episode_id.into());
        self
    }

    /// Set outcome
    pub fn with_outcome(mut self, outcome: EpisodeOutcome) -> Self {
        self.outcome = Some(outcome);
        self
    }

    /// Record access
    pub fn record_access(&mut self) {
        self.access_count += 1;
        self.last_accessed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Calculate recency score (0.0 - 1.0)
    pub fn recency_score(&self) -> f32 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let age_hours = (now - self.timestamp) as f32 / 3600.0;

        // Exponential decay with half-life of 24 hours
        (-age_hours / 24.0).exp()
    }

    /// Calculate relevance score
    pub fn relevance_score(&self, base_similarity: f32) -> f32 {
        let importance_weight = match self.importance {
            Importance::Low => 0.5,
            Importance::Normal => 1.0,
            Importance::High => 1.5,
            Importance::Critical => 2.0,
        };

        let recency = self.recency_score();
        let access_bonus = (self.access_count as f32 * 0.1).min(0.5);

        base_similarity * importance_weight * (0.5 + 0.5 * recency) + access_bonus
    }

    /// Get searchable text
    pub fn searchable_text(&self) -> String {
        let mut parts = vec![self.content.clone()];

        for value in self.context.values() {
            parts.push(value.clone());
        }

        parts.extend(self.tags.clone());

        if let Some(ref outcome) = self.outcome {
            parts.push(outcome.description.clone());
            parts.extend(outcome.lessons.clone());
        }

        parts.join(" ")
    }
}

/// Session information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Session ID
    pub id: String,
    /// Session start time
    pub started_at: u64,
    /// Session end time (None if active)
    pub ended_at: Option<u64>,
    /// Working directory
    pub working_dir: PathBuf,
    /// Session summary
    pub summary: Option<String>,
    /// Main task/goal
    pub task: Option<String>,
    /// Episode count
    pub episode_count: usize,
    /// Success rate
    pub success_rate: f32,
}

impl Session {
    /// Create new session
    pub fn new(working_dir: impl Into<PathBuf>) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            id,
            started_at: now,
            ended_at: None,
            working_dir: working_dir.into(),
            summary: None,
            task: None,
            episode_count: 0,
            success_rate: 0.0,
        }
    }

    /// Set task
    pub fn with_task(mut self, task: impl Into<String>) -> Self {
        self.task = Some(task.into());
        self
    }

    /// End session
    pub fn end(&mut self, summary: impl Into<String>) {
        self.ended_at = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
        self.summary = Some(summary.into());
    }

    /// Get duration in seconds
    pub fn duration_secs(&self) -> u64 {
        let end = self.ended_at.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        });
        end.saturating_sub(self.started_at)
    }

    /// Check if session is active
    pub fn is_active(&self) -> bool {
        self.ended_at.is_none()
    }
}

/// Pattern detected across episodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    /// Pattern ID
    pub id: String,
    /// Pattern description
    pub description: String,
    /// Episodes that exhibit this pattern
    pub episode_ids: Vec<String>,
    /// Frequency (number of occurrences)
    pub frequency: usize,
    /// Pattern type
    pub pattern_type: PatternType,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Suggested action based on pattern
    pub suggestion: Option<String>,
}

/// Type of detected pattern
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PatternType {
    /// Recurring error
    RecurringError,
    /// Successful approach
    SuccessfulApproach,
    /// Common workflow
    #[default]
    Workflow,
    /// User preference
    Preference,
    /// Anti-pattern to avoid
    AntiPattern,
}

impl Pattern {
    /// Create new pattern
    pub fn new(description: impl Into<String>, pattern_type: PatternType) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.into(),
            episode_ids: Vec::new(),
            frequency: 0,
            pattern_type,
            confidence: 0.5,
            suggestion: None,
        }
    }

    /// Add episode
    pub fn add_episode(&mut self, episode_id: impl Into<String>) {
        self.episode_ids.push(episode_id.into());
        self.frequency = self.episode_ids.len();
        // Increase confidence with more occurrences
        self.confidence = (self.confidence + 0.1).min(1.0);
    }

    /// Set suggestion
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }
}

/// Memory retrieval result
#[derive(Debug, Clone)]
pub struct MemoryResult {
    /// Retrieved episode
    pub episode: Episode,
    /// Similarity score
    pub similarity: f32,
    /// Relevance score (weighted)
    pub relevance: f32,
}

/// Episodic memory store
pub struct EpisodicMemory {
    /// Episodes indexed by ID
    episodes: HashMap<String, Episode>,
    /// Sessions indexed by ID
    sessions: HashMap<String, Session>,
    /// Current session ID
    current_session: Option<String>,
    /// Episode embeddings index
    index: VectorIndex,
    /// Embedding provider
    provider: Arc<EmbeddingBackend>,
    /// Detected patterns
    patterns: Vec<Pattern>,
    /// Storage path
    storage_path: Option<PathBuf>,
    /// Configuration
    config: EpisodicMemoryConfig,
    /// Recent episodes queue (for working memory)
    recent: VecDeque<String>,
}

/// Configuration for episodic memory
#[derive(Debug, Clone)]
pub struct EpisodicMemoryConfig {
    /// Maximum episodes to store
    pub max_episodes: usize,
    /// Maximum recent episodes in working memory
    pub max_recent: usize,
    /// Minimum importance to keep during cleanup
    pub min_importance_to_keep: Importance,
    /// Age threshold for cleanup (seconds)
    pub age_threshold_secs: u64,
    /// Pattern detection threshold (minimum occurrences)
    pub pattern_threshold: usize,
}

impl Default for EpisodicMemoryConfig {
    fn default() -> Self {
        Self {
            max_episodes: 10000,
            max_recent: 50,
            min_importance_to_keep: Importance::Normal,
            age_threshold_secs: 7 * 24 * 3600, // 7 days
            pattern_threshold: 3,
        }
    }
}

impl EpisodicMemory {
    /// Create new episodic memory
    pub fn new(provider: Arc<EmbeddingBackend>) -> Self {
        Self {
            episodes: HashMap::new(),
            sessions: HashMap::new(),
            current_session: None,
            index: VectorIndex::new(provider.dimension()),
            provider,
            patterns: Vec::new(),
            storage_path: None,
            config: EpisodicMemoryConfig::default(),
            recent: VecDeque::new(),
        }
    }

    /// Set storage path
    pub fn with_storage(mut self, path: impl Into<PathBuf>) -> Self {
        self.storage_path = Some(path.into());
        self
    }

    /// Set configuration
    pub fn with_config(mut self, config: EpisodicMemoryConfig) -> Self {
        self.config = config;
        self
    }

    /// Start a new session
    pub fn start_session(&mut self, working_dir: impl Into<PathBuf>) -> String {
        let session = Session::new(working_dir);
        let id = session.id.clone();
        self.sessions.insert(id.clone(), session);
        self.current_session = Some(id.clone());
        id
    }

    /// Start session with task
    pub fn start_session_with_task(
        &mut self,
        working_dir: impl Into<PathBuf>,
        task: impl Into<String>,
    ) -> String {
        let session = Session::new(working_dir).with_task(task);
        let id = session.id.clone();
        self.sessions.insert(id.clone(), session);
        self.current_session = Some(id.clone());
        id
    }

    /// End current session
    pub fn end_session(&mut self, summary: impl Into<String>) {
        if let Some(ref session_id) = self.current_session {
            if let Some(session) = self.sessions.get_mut(session_id) {
                // Calculate success rate
                let session_episodes: Vec<&Episode> = self
                    .episodes
                    .values()
                    .filter(|e| &e.session_id == session_id)
                    .collect();

                let successes = session_episodes
                    .iter()
                    .filter(|e| e.outcome.as_ref().map(|o| o.success).unwrap_or(false))
                    .count();

                session.episode_count = session_episodes.len();
                if !session_episodes.is_empty() {
                    session.success_rate = successes as f32 / session_episodes.len() as f32;
                }

                session.end(summary);
            }
        }
        self.current_session = None;
    }

    /// Get current session
    pub fn current_session(&self) -> Option<&Session> {
        self.current_session
            .as_ref()
            .and_then(|id| self.sessions.get(id))
    }

    /// Record an episode
    pub async fn record(&mut self, mut episode: Episode) -> Result<String> {
        // Ensure session ID
        if episode.session_id.is_empty() {
            if let Some(ref session_id) = self.current_session {
                episode.session_id = session_id.clone();
            }
        }

        // Generate embedding
        let text = episode.searchable_text();
        let embedding = self.provider.embed(&text).await?;

        let id = episode.id.clone();

        // Add to index
        self.index.add(id.clone(), embedding)?;

        // Add to recent
        self.recent.push_back(id.clone());
        while self.recent.len() > self.config.max_recent {
            self.recent.pop_front();
        }

        // Store episode
        self.episodes.insert(id.clone(), episode);

        // Cleanup if needed
        if self.episodes.len() > self.config.max_episodes {
            self.cleanup();
        }

        Ok(id)
    }

    /// Record a conversation episode
    pub async fn record_conversation(&mut self, content: impl Into<String>) -> Result<String> {
        let session_id = self
            .current_session
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let episode = Episode::new(EpisodeType::Conversation, content, session_id);
        self.record(episode).await
    }

    /// Record a tool execution
    pub async fn record_tool_execution(
        &mut self,
        tool: impl Into<String>,
        args: impl Into<String>,
        result: impl Into<String>,
        success: bool,
    ) -> Result<String> {
        let session_id = self
            .current_session
            .clone()
            .unwrap_or_else(|| "default".to_string());

        let tool = tool.into();
        let content = format!("Tool: {}", tool);

        let episode = Episode::new(EpisodeType::ToolExecution, content, session_id)
            .with_context("tool", &tool)
            .with_context("args", args)
            .with_context("result", result)
            .with_outcome(EpisodeOutcome {
                success,
                description: if success {
                    "Tool executed successfully".to_string()
                } else {
                    "Tool execution failed".to_string()
                },
                lessons: Vec::new(),
            });

        self.record(episode).await
    }

    /// Record an error
    pub async fn record_error(
        &mut self,
        error: impl Into<String>,
        context: impl Into<String>,
    ) -> Result<String> {
        let session_id = self
            .current_session
            .clone()
            .unwrap_or_else(|| "default".to_string());

        let episode = Episode::new(EpisodeType::Error, error, session_id)
            .with_context("error_context", context)
            .with_importance(Importance::High)
            .with_tag("error");

        self.record(episode).await
    }

    /// Record a success
    pub async fn record_success(
        &mut self,
        description: impl Into<String>,
        lessons: Vec<String>,
    ) -> Result<String> {
        let session_id = self
            .current_session
            .clone()
            .unwrap_or_else(|| "default".to_string());

        let episode = Episode::new(EpisodeType::Success, description, session_id)
            .with_importance(Importance::High)
            .with_outcome(EpisodeOutcome {
                success: true,
                description: "Task completed successfully".to_string(),
                lessons,
            });

        self.record(episode).await
    }

    /// Record a learning/insight
    pub async fn record_learning(&mut self, insight: impl Into<String>) -> Result<String> {
        let session_id = self
            .current_session
            .clone()
            .unwrap_or_else(|| "default".to_string());

        let episode = Episode::new(EpisodeType::Learning, insight, session_id)
            .with_importance(Importance::High)
            .with_tag("learning");

        self.record(episode).await
    }

    /// Retrieve similar episodes
    pub async fn retrieve(&self, query: &str, limit: usize) -> Result<Vec<MemoryResult>> {
        let embedding = self.provider.embed(query).await?;
        let search_results = self.index.search(&embedding, limit * 2);

        let mut results: Vec<MemoryResult> = search_results
            .into_iter()
            .filter_map(|(id, similarity)| {
                self.episodes.get(&id).map(|episode| {
                    let relevance = episode.relevance_score(similarity);
                    MemoryResult {
                        episode: episode.clone(),
                        similarity,
                        relevance,
                    }
                })
            })
            .collect();

        // Sort by relevance
        results.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results.truncate(limit);
        Ok(results)
    }

    /// Retrieve from recent memory
    pub fn retrieve_recent(&self, limit: usize) -> Vec<&Episode> {
        self.recent
            .iter()
            .rev()
            .take(limit)
            .filter_map(|id| self.episodes.get(id))
            .collect()
    }

    /// Retrieve by type
    pub fn retrieve_by_type(&self, episode_type: EpisodeType, limit: usize) -> Vec<&Episode> {
        let mut episodes: Vec<&Episode> = self
            .episodes
            .values()
            .filter(|e| e.episode_type == episode_type)
            .collect();

        episodes.sort_by_key(|x| std::cmp::Reverse(x.timestamp));
        episodes.truncate(limit);
        episodes
    }

    /// Retrieve errors from current session
    pub fn session_errors(&self) -> Vec<&Episode> {
        if let Some(ref session_id) = self.current_session {
            self.episodes
                .values()
                .filter(|e| &e.session_id == session_id && e.episode_type == EpisodeType::Error)
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get episode by ID
    pub fn get(&self, id: &str) -> Option<&Episode> {
        self.episodes.get(id)
    }

    /// Get episode mutably
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Episode> {
        self.episodes.get_mut(id)
    }

    /// Mark episode as accessed
    pub fn access(&mut self, id: &str) {
        if let Some(episode) = self.episodes.get_mut(id) {
            episode.record_access();
        }
    }

    /// Detect patterns across episodes
    pub fn detect_patterns(&mut self) {
        // Detect recurring errors
        let error_episodes: Vec<&Episode> = self
            .episodes
            .values()
            .filter(|e| e.episode_type == EpisodeType::Error)
            .collect();

        let mut error_patterns: HashMap<String, Vec<String>> = HashMap::new();
        for episode in error_episodes {
            // Group by error content (simplified)
            let key = episode
                .content
                .split_whitespace()
                .take(5)
                .collect::<Vec<_>>()
                .join(" ");
            error_patterns
                .entry(key)
                .or_default()
                .push(episode.id.clone());
        }

        for (description, episode_ids) in error_patterns {
            if episode_ids.len() >= self.config.pattern_threshold {
                let mut pattern = Pattern::new(
                    format!("Recurring error: {}", description),
                    PatternType::RecurringError,
                );
                for id in episode_ids {
                    pattern.add_episode(id);
                }
                pattern.suggestion =
                    Some("Consider addressing the root cause of this error".to_string());
                self.patterns.push(pattern);
            }
        }

        // Detect successful approaches
        let success_episodes: Vec<&Episode> = self
            .episodes
            .values()
            .filter(|e| e.outcome.as_ref().map(|o| o.success).unwrap_or(false))
            .collect();

        if success_episodes.len() >= self.config.pattern_threshold {
            // Look for common tags/context
            let mut tag_counts: HashMap<String, Vec<String>> = HashMap::new();
            for episode in success_episodes {
                for tag in &episode.tags {
                    tag_counts
                        .entry(tag.clone())
                        .or_default()
                        .push(episode.id.clone());
                }
            }

            for (tag, episode_ids) in tag_counts {
                if episode_ids.len() >= self.config.pattern_threshold {
                    let mut pattern = Pattern::new(
                        format!("Successful approach with '{}'", tag),
                        PatternType::SuccessfulApproach,
                    );
                    for id in episode_ids {
                        pattern.add_episode(id);
                    }
                    self.patterns.push(pattern);
                }
            }
        }
    }

    /// Get detected patterns
    pub fn patterns(&self) -> &[Pattern] {
        &self.patterns
    }

    /// Get patterns by type
    pub fn patterns_by_type(&self, pattern_type: PatternType) -> Vec<&Pattern> {
        self.patterns
            .iter()
            .filter(|p| p.pattern_type == pattern_type)
            .collect()
    }

    /// Reconstruct context from related episodes
    pub async fn reconstruct_context(&self, query: &str, max_tokens: usize) -> Result<String> {
        let memories = self.retrieve(query, 10).await?;

        let mut context_parts = Vec::new();
        let mut token_count = 0;

        for memory in memories {
            let episode = &memory.episode;
            let text = format!(
                "[{:?} - {}]\n{}\n",
                episode.episode_type,
                chrono::DateTime::from_timestamp(episode.timestamp as i64, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "Unknown".to_string()),
                episode.content
            );

            let estimated_tokens = estimate_content_tokens(&text);
            if token_count + estimated_tokens > max_tokens {
                break;
            }

            context_parts.push(text);
            token_count += estimated_tokens;
        }

        Ok(context_parts.join("\n---\n"))
    }

    /// Cleanup old/low-importance episodes
    fn cleanup(&mut self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let threshold = now - self.config.age_threshold_secs;

        // Remove old, low-importance episodes
        let to_remove: Vec<String> = self
            .episodes
            .iter()
            .filter(|(_, e)| {
                e.timestamp < threshold && e.importance < self.config.min_importance_to_keep
            })
            .map(|(id, _)| id.clone())
            .collect();

        for id in to_remove {
            self.episodes.remove(&id);
            self.index.remove(&id);
        }
    }

    /// Save to disk
    pub fn save(&self) -> Result<()> {
        let path = self
            .storage_path
            .as_ref()
            .ok_or_else(|| anyhow!("Storage path not set"))?;

        std::fs::create_dir_all(path)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Owner-only: episodic memory holds task data.
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
        }

        fn atomic_write(path: &std::path::Path, json: &str) -> Result<()> {
            // Unique per write so concurrent saves in one process never collide.
            static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let seq = TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let tmp_path = path.with_extension(format!("tmp.{}.{}", std::process::id(), seq));
            std::fs::write(&tmp_path, json)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600))?;
            }
            std::fs::rename(&tmp_path, path)?;
            Ok(())
        }

        // Save episodes
        let episodes_path = path.join("episodes.json");
        atomic_write(
            &episodes_path,
            &serde_json::to_string_pretty(&self.episodes)?,
        )?;

        // Save sessions
        let sessions_path = path.join("sessions.json");
        atomic_write(
            &sessions_path,
            &serde_json::to_string_pretty(&self.sessions)?,
        )?;

        // Save patterns
        let patterns_path = path.join("patterns.json");
        atomic_write(
            &patterns_path,
            &serde_json::to_string_pretty(&self.patterns)?,
        )?;

        Ok(())
    }

    /// Load from disk
    pub fn load(&mut self) -> Result<()> {
        let path = self
            .storage_path
            .as_ref()
            .ok_or_else(|| anyhow!("Storage path not set"))?
            .clone();

        if !path.exists() {
            return Ok(());
        }

        // Load episodes
        let episodes_path = path.join("episodes.json");
        if episodes_path.exists() {
            let json = std::fs::read_to_string(&episodes_path)?;
            self.episodes = serde_json::from_str(&json)?;
        }

        // Load sessions
        let sessions_path = path.join("sessions.json");
        if sessions_path.exists() {
            let json = std::fs::read_to_string(&sessions_path)?;
            self.sessions = serde_json::from_str(&json)?;
        }

        // Load patterns
        let patterns_path = path.join("patterns.json");
        if patterns_path.exists() {
            let json = std::fs::read_to_string(&patterns_path)?;
            self.patterns = serde_json::from_str(&json)?;
        }

        Ok(())
    }

    /// Get memory statistics
    pub fn stats(&self) -> MemoryStats {
        let by_type: HashMap<EpisodeType, usize> =
            self.episodes.values().fold(HashMap::new(), |mut acc, e| {
                *acc.entry(e.episode_type).or_insert(0) += 1;
                acc
            });

        let avg_importance = if self.episodes.is_empty() {
            0.0
        } else {
            self.episodes
                .values()
                .map(|e| e.importance as u8 as f32)
                .sum::<f32>()
                / self.episodes.len() as f32
        };

        MemoryStats {
            total_episodes: self.episodes.len(),
            total_sessions: self.sessions.len(),
            active_session: self.current_session.is_some(),
            recent_count: self.recent.len(),
            pattern_count: self.patterns.len(),
            episodes_by_type: by_type,
            average_importance: avg_importance,
        }
    }
}

/// Memory statistics
#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub total_episodes: usize,
    pub total_sessions: usize,
    pub active_session: bool,
    pub recent_count: usize,
    pub pattern_count: usize,
    pub episodes_by_type: HashMap<EpisodeType, usize>,
    pub average_importance: f32,
}

#[cfg(test)]
#[path = "../../tests/unit/cognitive/episodic/episodic_test.rs"]
mod tests;
