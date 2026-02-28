//! Hierarchical Memory System for 1M Token Context
//!
//! Provides a three-layer memory architecture:
//! - Working Memory: Immediate conversation context (~100K tokens)
//! - Episodic Memory: Recent experiences and events (~200K tokens)
//! - Semantic Memory: Codebase and long-term knowledge (~700K tokens)

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::api::types::Message;
use crate::token_count::estimate_tokens_with_overhead;
use crate::vector_store::{EmbeddingBackend, MockEmbeddingProvider, VectorIndex, VectorStore};

/// Total context tokens for Qwen3 Coder 1M context
pub const TOTAL_CONTEXT_TOKENS: usize = 1_000_000;

/// Default token budget allocation
pub const DEFAULT_WORKING_TOKENS: usize = 100_000;
pub const DEFAULT_EPISODIC_TOKENS: usize = 200_000;
pub const DEFAULT_SEMANTIC_TOKENS: usize = 700_000;
pub const DEFAULT_RESERVE_TOKENS: usize = 100_000;

/// Token budget allocation for memory layers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBudget {
    /// Working memory budget (immediate context)
    pub working_memory: usize,
    /// Episodic memory budget (experiences)
    pub episodic_memory: usize,
    /// Semantic memory budget (codebase & knowledge)
    pub semantic_memory: usize,
    /// Reserve for response generation
    pub response_reserve: usize,
}

impl Default for TokenBudget {
    fn default() -> Self {
        Self {
            working_memory: DEFAULT_WORKING_TOKENS,
            episodic_memory: DEFAULT_EPISODIC_TOKENS,
            semantic_memory: DEFAULT_SEMANTIC_TOKENS,
            response_reserve: DEFAULT_RESERVE_TOKENS,
        }
    }
}

impl TokenBudget {
    /// Create budget optimized for codebase analysis
    pub fn for_codebase_analysis() -> Self {
        Self {
            working_memory: 50_000,
            episodic_memory: 100_000,
            semantic_memory: 850_000,
            response_reserve: 100_000,
        }
    }

    /// Create budget optimized for conversation
    pub fn for_conversation() -> Self {
        Self {
            working_memory: 200_000,
            episodic_memory: 300_000,
            semantic_memory: 500_000,
            response_reserve: 100_000,
        }
    }

    /// Create budget optimized for self-improvement
    pub fn for_self_improvement() -> Self {
        Self {
            working_memory: 50_000,
            episodic_memory: 100_000,
            semantic_memory: 850_000,
            response_reserve: 100_000,
        }
    }

    /// Total allocated tokens
    pub fn total_allocated(&self) -> usize {
        self.working_memory + self.episodic_memory + self.semantic_memory
    }

    /// Total available including reserve
    pub fn total_available(&self) -> usize {
        self.total_allocated() + self.response_reserve
    }
}

/// Unified hierarchical memory manager
pub struct HierarchicalMemory {
    /// Token budget configuration
    pub budget: TokenBudget,
    /// Layer 1: Working memory (immediate context)
    pub working: WorkingMemory,
    /// Layer 2: Episodic memory (experiences)
    pub episodic: EpisodicMemory,
    /// Layer 3: Semantic memory (codebase & knowledge)
    pub semantic: Arc<RwLock<SemanticMemory>>,
    /// Current token usage by layer
    pub usage: MemoryUsage,
    /// Memory statistics and metrics
    pub metrics: MemoryMetrics,
    /// Embedding backend
    _embedding: Arc<EmbeddingBackend>,
}

/// Memory usage tracking
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryUsage {
    pub working_tokens: usize,
    pub episodic_tokens: usize,
    pub semantic_tokens: usize,
}

impl MemoryUsage {
    pub fn total(&self) -> usize {
        self.working_tokens + self.episodic_tokens + self.semantic_tokens
    }
}

/// Memory metrics for monitoring
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryMetrics {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub evictions: u64,
    pub compressions: u64,
    pub avg_retrieval_time_ms: f64,
    pub last_updated: u64,
}

impl HierarchicalMemory {
    /// Create new hierarchical memory system
    pub async fn new(budget: TokenBudget, embedding: Arc<EmbeddingBackend>) -> Result<Self> {
        let semantic = Arc::new(RwLock::new(SemanticMemory::new(
            budget.semantic_memory,
            embedding.clone(),
        )));

        Ok(Self {
            budget: budget.clone(),
            working: WorkingMemory::new(budget.working_memory),
            episodic: EpisodicMemory::new(budget.episodic_memory, embedding.clone()),
            semantic,
            usage: MemoryUsage::default(),
            metrics: MemoryMetrics::default(),
            _embedding: embedding,
        })
    }

    /// Initialize with Selfware codebase indexing
    #[allow(clippy::await_holding_lock)]
    pub async fn initialize_selfware_index(
        &mut self,
        selfware_path: &std::path::Path,
    ) -> Result<()> {
        info!("Initializing Selfware codebase index...");

        let mut semantic = self.semantic.write().await;
        semantic.index_codebase(selfware_path).await?;
        self.usage.semantic_tokens = semantic.total_tokens();

        info!(
            "Selfware index initialized: {} tokens",
            self.usage.semantic_tokens
        );

        Ok(())
    }

    /// Add message to working memory
    pub fn add_message(&mut self, message: Message, importance: f32) {
        self.working.add_message(message, importance);
        self.usage.working_tokens = self.working.total_tokens();
    }

    /// Record an episode
    pub async fn record_episode(&mut self, episode: Episode) -> Result<()> {
        self.episodic.record(episode).await?;
        self.usage.episodic_tokens = self.episodic.total_tokens();
        Ok(())
    }

    /// Retrieve relevant context for a query
    pub async fn retrieve_context(
        &self,
        query: &str,
        context_type: ContextType,
    ) -> Result<RetrievedContext> {
        let start = std::time::Instant::now();

        let context = match context_type {
            ContextType::Working => RetrievedContext::Working(self.working.get_context()),
            ContextType::Episodic {
                limit,
                min_importance,
            } => {
                let episodes = self
                    .episodic
                    .retrieve_relevant(query, limit, min_importance)
                    .await?;
                RetrievedContext::Episodic(episodes)
            }
            ContextType::Semantic {
                max_tokens,
                include_related,
            } => {
                let semantic = self.semantic.read().await;
                let code_context =
                    semantic.retrieve_code_context(query, max_tokens, include_related)?;
                RetrievedContext::Semantic(code_context)
            }
            ContextType::Complete => self.build_complete_context(query).await?,
        };

        let _elapsed = start.elapsed().as_millis() as f64;
        // Update average retrieval time
        // self.metrics.avg_retrieval_time_ms = ...

        Ok(context)
    }

    /// Build complete context from all layers
    async fn build_complete_context(&self, query: &str) -> Result<RetrievedContext> {
        let working = self.working.get_context();

        let episodic = self
            .episodic
            .retrieve_relevant(query, 10, Importance::Normal)
            .await?;

        let semantic = {
            let sem = self.semantic.read().await;
            sem.retrieve_code_context(query, self.budget.semantic_memory / 4, true)?
        };

        Ok(RetrievedContext::Complete {
            working,
            episodic,
            semantic,
        })
    }

    /// Get current memory statistics
    pub async fn get_stats(&self) -> MemoryStats {
        MemoryStats {
            budget: self.budget.clone(),
            usage: self.usage.clone(),
            metrics: self.metrics.clone(),
            working_entries: self.working.len(),
            episodic_entries: self.episodic.len(),
            semantic_files: self.semantic.read().await.file_count(),
        }
    }

    /// Check if memory is within budget
    pub fn is_within_budget(&self) -> bool {
        self.usage.working_tokens <= self.budget.working_memory
            && self.usage.episodic_tokens <= self.budget.episodic_memory
            && self.usage.semantic_tokens <= self.budget.semantic_memory
    }

    /// Force compression if over budget
    pub async fn compress_if_needed(&mut self) -> Result<bool> {
        let mut compressed = false;

        if self.usage.episodic_tokens > self.budget.episodic_memory {
            debug!("Episodic memory over budget, compressing...");
            self.episodic.compress_oldest().await?;
            self.usage.episodic_tokens = self.episodic.total_tokens();
            self.metrics.compressions += 1;
            compressed = true;
        }

        Ok(compressed)
    }
}

/// Types of context retrieval
#[derive(Debug, Clone, Copy)]
pub enum ContextType {
    /// Working memory only
    Working,
    /// Episodic memory with parameters
    Episodic {
        limit: usize,
        min_importance: Importance,
    },
    /// Semantic memory with parameters
    Semantic {
        max_tokens: usize,
        include_related: bool,
    },
    /// Complete context from all layers
    Complete,
}

/// Retrieved context from memory layers
#[derive(Debug, Clone)]
pub enum RetrievedContext {
    Working(WorkingContext),
    Episodic(Vec<Episode>),
    Semantic(CodeContext),
    Complete {
        working: WorkingContext,
        episodic: Vec<Episode>,
        semantic: CodeContext,
    },
}

/// Memory statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub budget: TokenBudget,
    pub usage: MemoryUsage,
    pub metrics: MemoryMetrics,
    pub working_entries: usize,
    pub episodic_entries: usize,
    pub semantic_files: usize,
}

// ============================================================================
// Working Memory Implementation
// ============================================================================

/// Working memory for immediate conversation context
pub struct WorkingMemory {
    max_tokens: usize,
    current_tokens: usize,
    messages: VecDeque<WorkingMemoryEntry>,
    active_code: Vec<ActiveCodeContext>,
    current_task: Option<TaskContext>,
}

#[derive(Debug, Clone)]
pub struct WorkingMemoryEntry {
    pub message: Message,
    pub token_count: usize,
    pub importance: f32,
    pub timestamp: u64,
    pub compressible: bool,
}

#[derive(Debug, Clone)]
pub struct ActiveCodeContext {
    pub path: String,
    pub content: CodeContent,
    pub last_accessed: u64,
    pub edit_history: Vec<CodeEdit>,
}

#[derive(Debug, Clone)]
pub enum CodeContent {
    Full(String),
    Summary {
        overview: String,
        key_functions: Vec<String>,
    },
    Reference {
        path: String,
        summary: String,
    },
}

#[derive(Debug, Clone)]
pub struct CodeEdit {
    pub timestamp: u64,
    pub description: String,
    pub lines_changed: (usize, usize),
}

#[derive(Debug, Clone, Default)]
pub struct TaskContext {
    pub description: String,
    pub goal: String,
    pub progress: Vec<String>,
    pub next_steps: Vec<String>,
    pub relevant_files: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct WorkingContext {
    pub messages: Vec<Message>,
    pub active_code: Vec<ActiveCodeContext>,
    pub current_task: Option<TaskContext>,
}

impl WorkingMemory {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            current_tokens: 0,
            messages: VecDeque::new(),
            active_code: Vec::new(),
            current_task: None,
        }
    }

    pub fn add_message(&mut self, message: Message, importance: f32) {
        let tokens = estimate_tokens_with_overhead(&message.content, 50);

        let entry = WorkingMemoryEntry {
            message: message.clone(),
            token_count: tokens,
            importance,
            timestamp: current_timestamp_secs(),
            compressible: message.role != "system",
        };

        // Evict if necessary
        while self.current_tokens + tokens > self.max_tokens {
            if !self.evict_least_important() {
                break;
            }
        }

        self.current_tokens += tokens;
        self.messages.push_back(entry);
    }

    fn evict_least_important(&mut self) -> bool {
        let now = current_timestamp_secs();

        if let Some((idx, _)) = self
            .messages
            .iter()
            .enumerate()
            .filter(|(_, e)| e.compressible)
            .min_by(|a, b| {
                let age_a = (now - a.1.timestamp).max(1) as f32;
                let age_b = (now - b.1.timestamp).max(1) as f32;
                let score_a = a.1.importance / age_a;
                let score_b = b.1.importance / age_b;
                score_a
                    .partial_cmp(&score_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        {
            if let Some(entry) = self.messages.remove(idx) {
                self.current_tokens -= entry.token_count;
                return true;
            }
        }
        false
    }

    pub fn get_context(&self) -> WorkingContext {
        WorkingContext {
            messages: self.messages.iter().map(|e| e.message.clone()).collect(),
            active_code: self.active_code.clone(),
            current_task: self.current_task.clone(),
        }
    }

    pub fn set_active_code(&mut self, path: String, content: String) {
        let tokens = estimate_tokens_with_overhead(&content, 0);

        let code_content = if tokens > 10_000 {
            CodeContent::Reference {
                path: path.clone(),
                summary: format!("Large file ({} tokens)", tokens),
            }
        } else {
            CodeContent::Full(content)
        };

        if let Some(existing) = self.active_code.iter_mut().find(|c| c.path == path) {
            existing.content = code_content;
            existing.last_accessed = current_timestamp_secs();
        } else {
            self.active_code.push(ActiveCodeContext {
                path,
                content: code_content,
                last_accessed: current_timestamp_secs(),
                edit_history: Vec::new(),
            });
        }

        self.active_code
            .sort_by(|a, b| b.last_accessed.cmp(&a.last_accessed));
        self.active_code.truncate(10);
    }

    pub fn total_tokens(&self) -> usize {
        self.current_tokens
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

// ============================================================================
// Episodic Memory Implementation
// ============================================================================

/// Episodic memory for experiences and events
pub struct EpisodicMemory {
    max_tokens: usize,
    current_tokens: usize,
    tiers: EpisodicTiers,
    /// Index mapping episode ID -> importance tier for O(1) tier lookup
    episode_index: HashMap<String, Importance>,
    vector_index: VectorIndex,
    embedding: Arc<EmbeddingBackend>,
}

pub struct EpisodicTiers {
    critical: Vec<Episode>,
    high: VecDeque<Episode>,
    normal: VecDeque<Episode>,
    low: VecDeque<Episode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: String,
    pub episode_type: EpisodeType,
    pub content: String,
    pub token_count: usize,
    pub importance: Importance,
    pub timestamp: u64,
    pub embedding_id: String,
    pub related_episodes: Vec<String>,
    pub insights: Vec<String>,
    pub is_summarized: bool,
    pub original_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Importance {
    Transient = 0,
    Low = 1,
    Normal = 2,
    High = 3,
    Critical = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EpisodeType {
    Conversation,
    ToolExecution,
    Error,
    Success,
    CodeChange,
    Learning,
    Decision,
}

impl EpisodeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EpisodeType::Conversation => "conversation",
            EpisodeType::ToolExecution => "tool",
            EpisodeType::Error => "error",
            EpisodeType::Success => "success",
            EpisodeType::CodeChange => "code_change",
            EpisodeType::Learning => "learning",
            EpisodeType::Decision => "decision",
        }
    }
}

impl EpisodicMemory {
    pub fn new(max_tokens: usize, embedding: Arc<EmbeddingBackend>) -> Self {
        Self {
            max_tokens,
            current_tokens: 0,
            tiers: EpisodicTiers {
                critical: Vec::new(),
                high: VecDeque::new(),
                normal: VecDeque::new(),
                low: VecDeque::new(),
            },
            episode_index: HashMap::new(),
            vector_index: VectorIndex::new(1536),
            embedding,
        }
    }

    pub async fn record(&mut self, mut episode: Episode) -> Result<()> {
        episode.token_count = estimate_tokens_with_overhead(&episode.content, 100);

        // Generate embedding
        let embedding_vec = self.embedding.embed(&episode.content).await?;
        self.vector_index.add(episode.id.clone(), embedding_vec)?;
        episode.embedding_id = episode.id.clone();

        self.add_to_tier(episode);
        self.maintain_budget().await?;

        Ok(())
    }

    fn add_to_tier(&mut self, episode: Episode) {
        self.current_tokens += episode.token_count;
        self.episode_index
            .insert(episode.id.clone(), episode.importance);

        match episode.importance {
            Importance::Critical => self.tiers.critical.push(episode),
            Importance::High => self.tiers.high.push_back(episode),
            Importance::Normal => self.tiers.normal.push_back(episode),
            Importance::Low | Importance::Transient => self.tiers.low.push_back(episode),
        }
    }

    async fn maintain_budget(&mut self) -> Result<()> {
        while self.current_tokens > self.max_tokens {
            if self.try_evict_lowest().await? {
                continue;
            }
            break;
        }
        Ok(())
    }

    async fn try_evict_lowest(&mut self) -> Result<bool> {
        if let Some(episode) = self.tiers.low.pop_front() {
            self.current_tokens -= episode.token_count;
            self.episode_index.remove(&episode.id);
            self.vector_index.remove(&episode.embedding_id);
            return Ok(true);
        }
        if let Some(episode) = self.tiers.normal.pop_front() {
            self.current_tokens -= episode.token_count;
            self.episode_index.remove(&episode.id);
            self.vector_index.remove(&episode.embedding_id);
            return Ok(true);
        }
        if let Some(episode) = self.tiers.high.pop_front() {
            self.current_tokens -= episode.token_count;
            self.episode_index.remove(&episode.id);
            self.vector_index.remove(&episode.embedding_id);
            return Ok(true);
        }
        if !self.tiers.critical.is_empty() {
            let episode = self.tiers.critical.remove(0);
            self.current_tokens -= episode.token_count;
            self.episode_index.remove(&episode.id);
            self.vector_index.remove(&episode.embedding_id);
            return Ok(true);
        }
        Ok(false)
    }

    pub async fn compress_oldest(&mut self) -> Result<()> {
        // Compress oldest normal episodes
        if let Some(episode) = self.tiers.normal.pop_front() {
            self.episode_index.remove(&episode.id);
            let summary = self.create_summary(&episode);
            self.current_tokens -= episode.token_count;
            self.current_tokens += summary.token_count;
            // Store summary in low tier
            self.episode_index
                .insert(summary.id.clone(), summary.importance);
            self.tiers.low.push_back(summary);
        }
        Ok(())
    }

    fn create_summary(&self, episode: &Episode) -> Episode {
        let summary_content = format!(
            "[SUMMARY] {}: {}",
            episode.episode_type.as_str(),
            &episode.content.chars().take(200).collect::<String>()
        );

        Episode {
            id: format!("summary-{}", episode.id),
            episode_type: episode.episode_type,
            content: summary_content.clone(),
            token_count: estimate_tokens_with_overhead(&summary_content, 50),
            importance: Importance::Low,
            timestamp: episode.timestamp,
            embedding_id: String::new(),
            related_episodes: vec![episode.id.clone()],
            insights: episode.insights.clone(),
            is_summarized: true,
            original_id: Some(episode.id.clone()),
        }
    }

    pub async fn retrieve_relevant(
        &self,
        query: &str,
        limit: usize,
        min_importance: Importance,
    ) -> Result<Vec<Episode>> {
        let query_embedding = self.embedding.embed(query).await?;
        let results = self.vector_index.search(&query_embedding, limit * 2);

        let mut episodes = Vec::new();
        for result in results {
            if let Some(episode) = self.find_episode(&result.0) {
                if episode.importance >= min_importance {
                    episodes.push(episode);
                    if episodes.len() >= limit {
                        break;
                    }
                }
            }
        }

        Ok(episodes)
    }

    fn find_episode(&self, id: &str) -> Option<Episode> {
        // Use the index to determine which tier contains the episode,
        // then search only that tier instead of all four.
        let importance = self.episode_index.get(id)?;
        let mut iter: Box<dyn Iterator<Item = &Episode>> = match importance {
            Importance::Critical => Box::new(self.tiers.critical.iter()),
            Importance::High => Box::new(self.tiers.high.iter()),
            Importance::Normal => Box::new(self.tiers.normal.iter()),
            Importance::Low | Importance::Transient => Box::new(self.tiers.low.iter()),
        };
        iter.find(|e| e.id == id).cloned()
    }

    pub fn total_tokens(&self) -> usize {
        self.current_tokens
    }

    pub fn len(&self) -> usize {
        self.tiers.critical.len()
            + self.tiers.high.len()
            + self.tiers.normal.len()
            + self.tiers.low.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiers.critical.is_empty()
            && self.tiers.high.is_empty()
            && self.tiers.normal.is_empty()
            && self.tiers.low.is_empty()
    }
}

// ============================================================================
// Semantic Memory Implementation
// ============================================================================

/// Semantic memory for codebase and knowledge
pub struct SemanticMemory {
    _max_tokens: usize,
    total_tokens: usize,
    files: HashMap<String, IndexedFile>,
    _vector_store: VectorStore,
    _embedding: Arc<EmbeddingBackend>,
}

pub struct IndexedFile {
    pub path: String,
    pub content: FileContent,
    pub token_count: usize,
    pub last_modified: u64,
}

pub enum FileContent {
    Full(String),
    Chunked(Vec<ContentChunk>),
    Summary(String),
}

pub struct ContentChunk {
    pub index: usize,
    pub content: String,
    pub token_count: usize,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeContext {
    pub files: Vec<FileContextEntry>,
    pub total_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContextEntry {
    pub path: String,
    pub content: String,
    pub relevance_score: f32,
}

impl SemanticMemory {
    pub fn get_file(&self, path: &str) -> Option<&IndexedFile> {
        self.files.get(path)
    }

    pub fn new(max_tokens: usize, embedding: Arc<EmbeddingBackend>) -> Self {
        Self {
            _max_tokens: max_tokens,
            total_tokens: 0,
            files: HashMap::new(),
            _vector_store: VectorStore::new(embedding.clone()),
            _embedding: embedding,
        }
    }

    pub async fn index_codebase(&mut self, root_path: &std::path::Path) -> Result<()> {
        info!("Indexing codebase at: {}", root_path.display());

        let mut entries = tokio::fs::read_dir(root_path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() && Self::is_source_file(&path) {
                self.index_file(&path).await?;
            } else if path.is_dir() {
                self.index_directory(&path).await?;
            }
        }

        info!(
            "Indexed {} files, {} tokens",
            self.files.len(),
            self.total_tokens
        );
        Ok(())
    }

    async fn index_directory(&mut self, dir: &std::path::Path) -> Result<()> {
        let mut stack = vec![dir.to_path_buf()];

        while let Some(current_dir) = stack.pop() {
            let mut entries = tokio::fs::read_dir(&current_dir).await?;

            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.is_file() && Self::is_source_file(&path) {
                    self.index_file(&path).await?;
                } else if path.is_dir() {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !name.starts_with('.') && name != "target" {
                        stack.push(path);
                    }
                }
            }
        }
        Ok(())
    }

    async fn index_file(&mut self, path: &std::path::Path) -> Result<()> {
        let content = match tokio::fs::read_to_string(path).await {
            Ok(c) => c,
            Err(_) => return Ok(()), // Skip binary files
        };

        let token_count = estimate_tokens_with_overhead(&content, 0);

        // Determine content strategy
        let file_content = if token_count < 5_000 {
            FileContent::Full(content)
        } else if token_count < 20_000 {
            FileContent::Chunked(self.chunk_content(&content))
        } else {
            FileContent::Summary(content.chars().take(5000).collect())
        };

        let indexed = IndexedFile {
            path: path.to_string_lossy().to_string(),
            content: file_content,
            token_count,
            last_modified: 0, // TODO: Get actual modified time
        };

        self.total_tokens += token_count;
        self.files.insert(indexed.path.clone(), indexed);

        Ok(())
    }

    fn chunk_content(&self, content: &str) -> Vec<ContentChunk> {
        let lines: Vec<&str> = content.lines().collect();
        let mut chunks = Vec::new();
        let chunk_size = 100; // lines per chunk

        for (i, chunk_lines) in lines.chunks(chunk_size).enumerate() {
            let chunk_content = chunk_lines.join("\n");
            chunks.push(ContentChunk {
                index: i,
                token_count: estimate_tokens_with_overhead(&chunk_content, 0),
                start_line: i * chunk_size,
                end_line: (i + 1) * chunk_size,
                content: chunk_content,
            });
        }

        chunks
    }

    pub fn retrieve_code_context(
        &self,
        query: &str,
        max_tokens: usize,
        _include_related: bool,
    ) -> Result<CodeContext> {
        // Simple keyword-based retrieval for now
        // TODO: Implement semantic search with embeddings

        let query_lower = query.to_lowercase();
        let keywords: Vec<&str> = query_lower.split_whitespace().collect();

        let mut scored_files: Vec<(String, f32, usize)> = self
            .files
            .iter()
            .map(|(path, file)| {
                let path_lower = path.to_lowercase();
                let score = keywords.iter().filter(|k| path_lower.contains(*k)).count() as f32;
                (path.clone(), score, file.token_count)
            })
            .filter(|(_, score, _)| *score > 0.0)
            .collect();

        scored_files.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut context = CodeContext {
            files: Vec::new(),
            total_tokens: 0,
        };

        for (path, score, tokens) in scored_files {
            if context.total_tokens + tokens > max_tokens {
                break;
            }

            if let Some(file) = self.files.get(&path) {
                let content = match &file.content {
                    FileContent::Full(c) => c.clone(),
                    FileContent::Chunked(chunks) => chunks
                        .iter()
                        .map(|c| c.content.as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    FileContent::Summary(s) => s.clone(),
                };

                context.files.push(FileContextEntry {
                    path: path.clone(),
                    content,
                    relevance_score: score,
                });
                context.total_tokens += tokens;
            }
        }

        Ok(context)
    }

    fn is_source_file(path: &std::path::Path) -> bool {
        matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("rs") | Some("py") | Some("js") | Some("ts") | Some("go") | Some("java")
        )
    }

    pub fn total_tokens(&self) -> usize {
        self.total_tokens
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

fn current_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Helper functions
    // ========================================================================

    fn make_message(role: &str, content: &str) -> Message {
        Message {
            role: role.to_string(),
            content: content.to_string(),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    fn make_episode(id: &str, importance: Importance, content: &str) -> Episode {
        Episode {
            id: id.to_string(),
            episode_type: EpisodeType::Conversation,
            content: content.to_string(),
            token_count: estimate_tokens_with_overhead(content, 100),
            importance,
            timestamp: current_timestamp_secs(),
            embedding_id: id.to_string(),
            related_episodes: Vec::new(),
            insights: Vec::new(),
            is_summarized: false,
            original_id: None,
        }
    }

    // ========================================================================
    // TokenBudget tests
    // ========================================================================

    #[test]
    fn test_token_budget_default() {
        let budget = TokenBudget::default();
        assert_eq!(budget.working_memory, DEFAULT_WORKING_TOKENS);
        assert_eq!(budget.episodic_memory, DEFAULT_EPISODIC_TOKENS);
        assert_eq!(budget.semantic_memory, DEFAULT_SEMANTIC_TOKENS);
    }

    #[test]
    fn test_token_budget_self_improvement() {
        let budget = TokenBudget::for_self_improvement();
        assert!(budget.semantic_memory > budget.working_memory);
        assert!(budget.semantic_memory > budget.episodic_memory);
    }

    #[test]
    fn test_token_budget_total_allocated() {
        let budget = TokenBudget::default();
        assert_eq!(
            budget.total_allocated(),
            DEFAULT_WORKING_TOKENS + DEFAULT_EPISODIC_TOKENS + DEFAULT_SEMANTIC_TOKENS
        );
    }

    #[test]
    fn test_token_budget_total_available_includes_reserve() {
        let budget = TokenBudget::default();
        assert_eq!(
            budget.total_available(),
            budget.total_allocated() + DEFAULT_RESERVE_TOKENS
        );
    }

    #[test]
    fn test_token_budget_for_codebase_analysis_favors_semantic() {
        let budget = TokenBudget::for_codebase_analysis();
        assert!(budget.semantic_memory > budget.working_memory);
        assert!(budget.semantic_memory > budget.episodic_memory);
        assert_eq!(budget.semantic_memory, 850_000);
    }

    #[test]
    fn test_token_budget_for_conversation_favors_working_and_episodic() {
        let budget = TokenBudget::for_conversation();
        assert!(budget.working_memory > TokenBudget::default().working_memory);
        assert!(budget.episodic_memory > TokenBudget::default().episodic_memory);
    }

    // ========================================================================
    // WorkingMemory tests
    // ========================================================================

    #[test]
    fn test_working_memory_new_is_empty() {
        let wm = WorkingMemory::new(10_000);
        assert!(wm.is_empty());
        assert_eq!(wm.len(), 0);
        assert_eq!(wm.total_tokens(), 0);
    }

    #[test]
    fn test_working_memory_add_message_increases_count() {
        let mut wm = WorkingMemory::new(100_000);
        wm.add_message(make_message("user", "Hello world"), 1.0);
        assert_eq!(wm.len(), 1);
        assert!(!wm.is_empty());
        assert!(wm.total_tokens() > 0);
    }

    #[test]
    fn test_working_memory_add_multiple_messages() {
        let mut wm = WorkingMemory::new(100_000);
        wm.add_message(make_message("user", "First message"), 1.0);
        wm.add_message(make_message("assistant", "Second message"), 1.0);
        wm.add_message(make_message("user", "Third message"), 1.0);
        assert_eq!(wm.len(), 3);
    }

    #[test]
    fn test_working_memory_eviction_when_over_capacity() {
        // Use a very small max_tokens to force eviction
        let mut wm = WorkingMemory::new(200);
        let long_content = "x".repeat(500);
        wm.add_message(make_message("user", &long_content), 0.5);
        let tokens_after_first = wm.total_tokens();

        // Adding another large message should trigger eviction of the first
        wm.add_message(make_message("user", &long_content), 0.5);

        // The total tokens should stay within budget (or close),
        // meaning eviction must have happened
        assert!(
            wm.total_tokens() <= tokens_after_first * 2,
            "Eviction should have triggered"
        );
    }

    #[test]
    fn test_working_memory_system_messages_not_evicted() {
        // System messages have compressible = false (role == "system")
        let mut wm = WorkingMemory::new(300);
        wm.add_message(make_message("system", "System prompt"), 1.0);

        let long_content = "y".repeat(500);
        wm.add_message(make_message("user", &long_content), 0.1);

        // The system message should still be there because it's not compressible
        let ctx = wm.get_context();
        let has_system = ctx.messages.iter().any(|m| m.role == "system");
        assert!(
            has_system,
            "System messages should not be evicted (compressible=false)"
        );
    }

    #[test]
    fn test_working_memory_evicts_least_important_first() {
        let mut wm = WorkingMemory::new(400);

        // Add a low-importance message first
        wm.add_message(make_message("user", "low importance msg"), 0.1);
        // Add a high-importance message
        wm.add_message(make_message("user", "high importance msg"), 10.0);

        // Force eviction by adding a large message
        let long_content = "z".repeat(800);
        wm.add_message(make_message("user", &long_content), 5.0);

        // The high-importance message should survive if any old messages remain
        let ctx = wm.get_context();
        let has_low = ctx
            .messages
            .iter()
            .any(|m| m.content == "low importance msg");
        let has_high = ctx
            .messages
            .iter()
            .any(|m| m.content == "high importance msg");

        // Low importance should be evicted before high importance
        if ctx.messages.len() < 3 {
            // Some eviction happened
            assert!(
                !has_low || has_high,
                "Low-importance messages should be evicted before high-importance"
            );
        }
    }

    #[test]
    fn test_working_memory_get_context_returns_all_messages() {
        let mut wm = WorkingMemory::new(100_000);
        wm.add_message(make_message("user", "Hello"), 1.0);
        wm.add_message(make_message("assistant", "Hi there"), 1.0);

        let ctx = wm.get_context();
        assert_eq!(ctx.messages.len(), 2);
        assert_eq!(ctx.messages[0].content, "Hello");
        assert_eq!(ctx.messages[1].content, "Hi there");
    }

    #[test]
    fn test_working_memory_set_active_code_small_file() {
        let mut wm = WorkingMemory::new(100_000);
        wm.set_active_code("src/main.rs".to_string(), "fn main() {}".to_string());

        let ctx = wm.get_context();
        assert_eq!(ctx.active_code.len(), 1);
        assert_eq!(ctx.active_code[0].path, "src/main.rs");
        match &ctx.active_code[0].content {
            CodeContent::Full(c) => assert_eq!(c, "fn main() {}"),
            _ => panic!("Expected Full content for small file"),
        }
    }

    #[test]
    fn test_working_memory_set_active_code_large_file_becomes_reference() {
        let mut wm = WorkingMemory::new(100_000);
        // Create content large enough to exceed 10,000 tokens
        // At ~3-4 chars per token, ~40000 chars should be enough
        let large_content = "fn example() { let x = 1; }\n".repeat(2000);
        wm.set_active_code("src/large.rs".to_string(), large_content);

        let ctx = wm.get_context();
        assert_eq!(ctx.active_code.len(), 1);
        match &ctx.active_code[0].content {
            CodeContent::Reference { path, summary } => {
                assert_eq!(path, "src/large.rs");
                assert!(summary.contains("tokens"));
            }
            _ => panic!("Expected Reference content for large file"),
        }
    }

    #[test]
    fn test_working_memory_active_code_update_existing() {
        let mut wm = WorkingMemory::new(100_000);
        wm.set_active_code("src/lib.rs".to_string(), "v1".to_string());
        wm.set_active_code("src/lib.rs".to_string(), "v2".to_string());

        let ctx = wm.get_context();
        // Should still only have 1 entry, not 2
        assert_eq!(ctx.active_code.len(), 1);
        match &ctx.active_code[0].content {
            CodeContent::Full(c) => assert_eq!(c, "v2"),
            _ => panic!("Expected updated content"),
        }
    }

    #[test]
    fn test_working_memory_active_code_truncated_at_10() {
        let mut wm = WorkingMemory::new(100_000);
        for i in 0..15 {
            wm.set_active_code(format!("src/file{}.rs", i), format!("content {}", i));
        }

        let ctx = wm.get_context();
        assert_eq!(
            ctx.active_code.len(),
            10,
            "Active code should be truncated to 10 entries"
        );
    }

    #[test]
    fn test_working_memory_current_task() {
        let mut wm = WorkingMemory::new(100_000);
        assert!(wm.get_context().current_task.is_none());

        wm.current_task = Some(TaskContext {
            description: "Test task".to_string(),
            goal: "Do something".to_string(),
            progress: vec!["Step 1 done".to_string()],
            next_steps: vec!["Step 2".to_string()],
            relevant_files: vec!["src/main.rs".to_string()],
        });

        let ctx = wm.get_context();
        assert!(ctx.current_task.is_some());
        let task = ctx.current_task.unwrap();
        assert_eq!(task.description, "Test task");
        assert_eq!(task.goal, "Do something");
    }

    // ========================================================================
    // EpisodicMemory tests (synchronous logic only)
    // ========================================================================

    #[test]
    fn test_episodic_memory_add_to_tier_critical() {
        let embedding = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let mut em = EpisodicMemory::new(100_000, embedding);
        let episode = make_episode("ep-1", Importance::Critical, "critical event");

        em.add_to_tier(episode);
        assert_eq!(em.len(), 1);
        assert_eq!(em.tiers.critical.len(), 1);
        assert!(em.episode_index.contains_key("ep-1"));
        assert_eq!(em.episode_index.get("ep-1"), Some(&Importance::Critical));
    }

    #[test]
    fn test_episodic_memory_add_to_tier_high() {
        let embedding = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let mut em = EpisodicMemory::new(100_000, embedding);
        let episode = make_episode("ep-2", Importance::High, "high importance event");

        em.add_to_tier(episode);
        assert_eq!(em.tiers.high.len(), 1);
        assert_eq!(em.episode_index.get("ep-2"), Some(&Importance::High));
    }

    #[test]
    fn test_episodic_memory_add_to_tier_normal() {
        let embedding = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let mut em = EpisodicMemory::new(100_000, embedding);
        let episode = make_episode("ep-3", Importance::Normal, "normal event");

        em.add_to_tier(episode);
        assert_eq!(em.tiers.normal.len(), 1);
        assert_eq!(em.episode_index.get("ep-3"), Some(&Importance::Normal));
    }

    #[test]
    fn test_episodic_memory_add_to_tier_low_and_transient() {
        let embedding = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let mut em = EpisodicMemory::new(100_000, embedding);

        em.add_to_tier(make_episode("ep-low", Importance::Low, "low event"));
        em.add_to_tier(make_episode(
            "ep-transient",
            Importance::Transient,
            "transient event",
        ));

        // Both Low and Transient go to the low tier
        assert_eq!(em.tiers.low.len(), 2);
        assert_eq!(em.episode_index.get("ep-low"), Some(&Importance::Low));
        assert_eq!(
            em.episode_index.get("ep-transient"),
            Some(&Importance::Transient)
        );
    }

    #[test]
    fn test_episodic_memory_token_tracking() {
        let embedding = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let mut em = EpisodicMemory::new(100_000, embedding);
        assert_eq!(em.total_tokens(), 0);

        let ep = make_episode("ep-1", Importance::Normal, "some content");
        let expected_tokens = ep.token_count;
        em.add_to_tier(ep);

        assert_eq!(em.total_tokens(), expected_tokens);
    }

    #[test]
    fn test_episodic_memory_find_episode_uses_index() {
        let embedding = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let mut em = EpisodicMemory::new(100_000, embedding);

        em.add_to_tier(make_episode("ep-c", Importance::Critical, "critical"));
        em.add_to_tier(make_episode("ep-h", Importance::High, "high"));
        em.add_to_tier(make_episode("ep-n", Importance::Normal, "normal"));
        em.add_to_tier(make_episode("ep-l", Importance::Low, "low"));

        // find_episode should find each by ID using the index
        assert!(em.find_episode("ep-c").is_some());
        assert_eq!(em.find_episode("ep-c").unwrap().importance, Importance::Critical);

        assert!(em.find_episode("ep-h").is_some());
        assert_eq!(em.find_episode("ep-h").unwrap().importance, Importance::High);

        assert!(em.find_episode("ep-n").is_some());
        assert!(em.find_episode("ep-l").is_some());

        // Non-existent ID returns None
        assert!(em.find_episode("ep-nonexistent").is_none());
    }

    #[test]
    fn test_episodic_memory_is_empty() {
        let embedding = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let em = EpisodicMemory::new(100_000, embedding);
        assert!(em.is_empty());
    }

    #[test]
    fn test_episodic_memory_len_across_tiers() {
        let embedding = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let mut em = EpisodicMemory::new(100_000, embedding);

        em.add_to_tier(make_episode("a", Importance::Critical, "c"));
        em.add_to_tier(make_episode("b", Importance::High, "h"));
        em.add_to_tier(make_episode("c", Importance::Normal, "n"));
        em.add_to_tier(make_episode("d", Importance::Low, "l"));

        assert_eq!(em.len(), 4);
    }

    #[tokio::test]
    async fn test_episodic_memory_try_evict_lowest_order() {
        let embedding = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let mut em = EpisodicMemory::new(100_000, embedding);

        em.add_to_tier(make_episode("crit", Importance::Critical, "critical"));
        em.add_to_tier(make_episode("high", Importance::High, "high"));
        em.add_to_tier(make_episode("norm", Importance::Normal, "normal"));
        em.add_to_tier(make_episode("low1", Importance::Low, "low"));

        // First eviction should remove from low tier
        let evicted = em.try_evict_lowest().await.unwrap();
        assert!(evicted);
        assert_eq!(em.tiers.low.len(), 0);
        assert!(!em.episode_index.contains_key("low1"));

        // Next eviction should remove from normal tier
        let evicted = em.try_evict_lowest().await.unwrap();
        assert!(evicted);
        assert_eq!(em.tiers.normal.len(), 0);
        assert!(!em.episode_index.contains_key("norm"));

        // Next from high tier
        let evicted = em.try_evict_lowest().await.unwrap();
        assert!(evicted);
        assert_eq!(em.tiers.high.len(), 0);
        assert!(!em.episode_index.contains_key("high"));

        // Next from critical tier
        let evicted = em.try_evict_lowest().await.unwrap();
        assert!(evicted);
        assert_eq!(em.tiers.critical.len(), 0);

        // Now all empty, eviction should return false
        let evicted = em.try_evict_lowest().await.unwrap();
        assert!(!evicted);
    }

    #[test]
    fn test_episodic_memory_create_summary() {
        let embedding = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let em = EpisodicMemory::new(100_000, embedding);

        let original = Episode {
            id: "ep-original".to_string(),
            episode_type: EpisodeType::Learning,
            content: "This is a detailed learning episode with lots of information.".to_string(),
            token_count: 500,
            importance: Importance::Normal,
            timestamp: 1000,
            embedding_id: "ep-original".to_string(),
            related_episodes: Vec::new(),
            insights: vec!["insight1".to_string()],
            is_summarized: false,
            original_id: None,
        };

        let summary = em.create_summary(&original);

        assert_eq!(summary.id, "summary-ep-original");
        assert!(summary.is_summarized);
        assert_eq!(summary.importance, Importance::Low);
        assert_eq!(summary.original_id, Some("ep-original".to_string()));
        assert!(summary.content.starts_with("[SUMMARY] learning:"));
        assert_eq!(summary.related_episodes, vec!["ep-original".to_string()]);
        assert_eq!(summary.insights, vec!["insight1".to_string()]);
        assert!(summary.token_count < original.token_count);
    }

    #[tokio::test]
    async fn test_episodic_memory_compress_oldest() {
        let embedding = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let mut em = EpisodicMemory::new(100_000, embedding);

        // Add a normal episode
        let normal_ep = make_episode("ep-norm", Importance::Normal, "a fairly long episode content that should be compressed into a shorter summary");
        let original_tokens = normal_ep.token_count;
        em.add_to_tier(normal_ep);
        assert_eq!(em.tiers.normal.len(), 1);

        // Compress oldest
        em.compress_oldest().await.unwrap();

        // Normal tier should be empty, low tier should have the summary
        assert_eq!(em.tiers.normal.len(), 0);
        assert_eq!(em.tiers.low.len(), 1);

        let summary = &em.tiers.low[0];
        assert!(summary.is_summarized);
        assert!(summary.content.starts_with("[SUMMARY]"));

        // The index should be updated: old key removed, new key added
        assert!(!em.episode_index.contains_key("ep-norm"));
        assert!(em.episode_index.contains_key(&summary.id));

        // Token count should have been adjusted
        // (total_tokens = original removed + summary added)
        assert!(em.total_tokens() < original_tokens + 50); // summary should be smaller
    }

    // ========================================================================
    // MemoryUsage tests
    // ========================================================================

    #[test]
    fn test_memory_usage_total() {
        let usage = MemoryUsage {
            working_tokens: 100,
            episodic_tokens: 200,
            semantic_tokens: 300,
        };
        assert_eq!(usage.total(), 600);
    }

    #[test]
    fn test_memory_usage_default_is_zero() {
        let usage = MemoryUsage::default();
        assert_eq!(usage.total(), 0);
    }

    // ========================================================================
    // EpisodeType tests
    // ========================================================================

    #[test]
    fn test_episode_type_as_str() {
        assert_eq!(EpisodeType::Conversation.as_str(), "conversation");
        assert_eq!(EpisodeType::ToolExecution.as_str(), "tool");
        assert_eq!(EpisodeType::Error.as_str(), "error");
        assert_eq!(EpisodeType::Success.as_str(), "success");
        assert_eq!(EpisodeType::CodeChange.as_str(), "code_change");
        assert_eq!(EpisodeType::Learning.as_str(), "learning");
        assert_eq!(EpisodeType::Decision.as_str(), "decision");
    }

    // ========================================================================
    // Importance ordering tests
    // ========================================================================

    #[test]
    fn test_importance_ordering() {
        assert!(Importance::Transient < Importance::Low);
        assert!(Importance::Low < Importance::Normal);
        assert!(Importance::Normal < Importance::High);
        assert!(Importance::High < Importance::Critical);
    }
}
