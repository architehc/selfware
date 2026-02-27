//! Hierarchical Memory System for 1M Token Context
//!
//! Provides a three-layer memory architecture:
//! - Working Memory: Immediate conversation context (~100K tokens)
//! - Episodic Memory: Recent experiences and events (~200K tokens)
//! - Semantic Memory: Codebase and long-term knowledge (~700K tokens)

use std::collections::{HashMap, VecDeque, BTreeMap};
use std::sync::Arc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use anyhow::{Result, anyhow};
use tracing::{info, warn, debug};

use crate::api::types::Message;
use crate::config::Config;
use crate::token_count::estimate_tokens_with_overhead;
use crate::vector_store::{EmbeddingBackend, VectorIndex, VectorStore};

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
    embedding: Arc<dyn EmbeddingBackend>,
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
    pub async fn new(
        budget: TokenBudget,
        embedding: Arc<dyn EmbeddingBackend>,
    ) -> Result<Self> {
        let semantic = Arc::new(RwLock::new(
            SemanticMemory::new(budget.semantic_memory, embedding.clone())
        ));
        
        Ok(Self {
            budget: budget.clone(),
            working: WorkingMemory::new(budget.working_memory),
            episodic: EpisodicMemory::new(budget.episodic_memory, embedding.clone()),
            semantic,
            usage: MemoryUsage::default(),
            metrics: MemoryMetrics::default(),
            embedding,
        })
    }
    
    /// Initialize with Selfware codebase indexing
    pub async fn initialize_selfware_index(&mut self, selfware_path: &std::path::Path) -> Result<()> {
        info!("Initializing Selfware codebase index...");
        
        let mut semantic = self.semantic.write();
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
            ContextType::Working => {
                RetrievedContext::Working(self.working.get_context())
            }
            ContextType::Episodic { limit, min_importance } => {
                let episodes = self.episodic.retrieve_relevant(
                    query,
                    limit,
                    min_importance,
                ).await?;
                RetrievedContext::Episodic(episodes)
            }
            ContextType::Semantic { max_tokens, include_related } => {
                let semantic = self.semantic.read();
                let code_context = semantic.retrieve_code_context(
                    query,
                    max_tokens,
                    include_related,
                ).await?;
                RetrievedContext::Semantic(code_context)
            }
            ContextType::Complete => {
                self.build_complete_context(query).await?
            }
        };
        
        let elapsed = start.elapsed().as_millis() as f64;
        // Update average retrieval time
        // self.metrics.avg_retrieval_time_ms = ...
        
        Ok(context)
    }
    
    /// Build complete context from all layers
    async fn build_complete_context(&self, query: &str) -> Result<RetrievedContext> {
        let working = self.working.get_context();
        
        let episodic = self.episodic.retrieve_relevant(
            query,
            10,
            Importance::Normal,
        ).await?;
        
        let semantic = {
            let sem = self.semantic.read();
            sem.retrieve_code_context(
                query,
                self.budget.semantic_memory / 4,
                true,
            ).await?
        };
        
        Ok(RetrievedContext::Complete {
            working,
            episodic,
            semantic,
        })
    }
    
    /// Get current memory statistics
    pub fn get_stats(&self) -> MemoryStats {
        MemoryStats {
            budget: self.budget.clone(),
            usage: self.usage.clone(),
            metrics: self.metrics.clone(),
            working_entries: self.working.len(),
            episodic_entries: self.episodic.len(),
            semantic_files: self.semantic.read().file_count(),
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
    Episodic { limit: usize, min_importance: Importance },
    /// Semantic memory with parameters
    Semantic { max_tokens: usize, include_related: bool },
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
    Summary { overview: String, key_functions: Vec<String> },
    Reference { path: String, summary: String },
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
        
        if let Some((idx, _)) = self.messages
            .iter()
            .enumerate()
            .filter(|(_, e)| e.compressible)
            .min_by(|a, b| {
                let age_a = (now - a.1.timestamp).max(1) as f32;
                let age_b = (now - b.1.timestamp).max(1) as f32;
                let score_a = a.1.importance / age_a;
                let score_b = b.1.importance / age_b;
                score_a.partial_cmp(&score_b).unwrap()
            }) {
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
        
        self.active_code.sort_by(|a, b| b.last_accessed.cmp(&a.last_accessed));
        self.active_code.truncate(10);
    }
    
    pub fn total_tokens(&self) -> usize {
        self.current_tokens
    }
    
    pub fn len(&self) -> usize {
        self.messages.len()
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
    vector_index: VectorIndex,
    embedding: Arc<dyn EmbeddingBackend>,
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
    pub fn new(max_tokens: usize, embedding: Arc<dyn EmbeddingBackend>) -> Self {
        Self {
            max_tokens,
            current_tokens: 0,
            tiers: EpisodicTiers {
                critical: Vec::new(),
                high: VecDeque::new(),
                normal: VecDeque::new(),
                low: VecDeque::new(),
            },
            vector_index: VectorIndex::new(embedding.clone()),
            embedding,
        }
    }
    
    pub async fn record(&mut self, mut episode: Episode) -> Result<()> {
        episode.token_count = estimate_tokens_with_overhead(&episode.content, 100);
        
        // Generate embedding
        let embedding_vec = self.embedding.embed(&episode.content).await?;
        episode.embedding_id = self.vector_index.add(embedding_vec, episode.id.clone()).await?;
        
        self.add_to_tier(episode);
        self.maintain_budget().await?;
        
        Ok(())
    }
    
    fn add_to_tier(&mut self, episode: Episode) {
        self.current_tokens += episode.token_count;
        
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
            self.vector_index.remove(&episode.embedding_id).await?;
            return Ok(true);
        }
        if let Some(episode) = self.tiers.normal.pop_front() {
            self.current_tokens -= episode.token_count;
            self.vector_index.remove(&episode.embedding_id).await?;
            return Ok(true);
        }
        if let Some(episode) = self.tiers.high.pop_front() {
            self.current_tokens -= episode.token_count;
            self.vector_index.remove(&episode.embedding_id).await?;
            return Ok(true);
        }
        Ok(false)
    }
    
    pub async fn compress_oldest(&mut self) -> Result<()> {
        // Compress oldest normal episodes
        if let Some(episode) = self.tiers.normal.pop_front() {
            let summary = self.create_summary(&episode);
            self.current_tokens -= episode.token_count;
            self.current_tokens += summary.token_count;
            // Store summary in low tier
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
        let results = self.vector_index.search(&query_embedding, limit * 2).await?;
        
        let mut episodes = Vec::new();
        for result in results {
            if let Some(episode) = self.find_episode(&result.id) {
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
        self.tiers.critical.iter()
            .chain(self.tiers.high.iter())
            .chain(self.tiers.normal.iter())
            .chain(self.tiers.low.iter())
            .find(|e| e.id == id)
            .cloned()
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
}

// ============================================================================
// Semantic Memory Implementation
// ============================================================================

/// Semantic memory for codebase and knowledge
pub struct SemanticMemory {
    max_tokens: usize,
    total_tokens: usize,
    files: HashMap<String, IndexedFile>,
    vector_store: VectorStore,
    embedding: Arc<dyn EmbeddingBackend>,
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

pub struct CodeContext {
    pub files: Vec<FileContextEntry>,
    pub total_tokens: usize,
}

pub struct FileContextEntry {
    pub path: String,
    pub content: String,
    pub relevance_score: f32,
}

impl SemanticMemory {
    pub fn new(max_tokens: usize, embedding: Arc<dyn EmbeddingBackend>) -> Self {
        Self {
            max_tokens,
            total_tokens: 0,
            files: HashMap::new(),
            vector_store: VectorStore::new(embedding.clone()),
            embedding,
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
        
        info!("Indexed {} files, {} tokens", self.files.len(), self.total_tokens);
        Ok(())
    }
    
    async fn index_directory(&mut self, dir: &std::path::Path) -> Result<()> {
        let mut entries = tokio::fs::read_dir(dir).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() && Self::is_source_file(&path) {
                self.index_file(&path).await?;
            } else if path.is_dir() {
                // Skip hidden directories and target
                let name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if !name.starts_with('.') && name != "target" {
                    self.index_directory(&path).await?;
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
    
    pub async fn retrieve_code_context(
        &self,
        query: &str,
        max_tokens: usize,
        include_related: bool,
    ) -> Result<CodeContext> {
        // Simple keyword-based retrieval for now
        // TODO: Implement semantic search with embeddings
        
        let query_lower = query.to_lowercase();
        let keywords: Vec<&str> = query_lower.split_whitespace().collect();
        
        let mut scored_files: Vec<(String, f32, usize)> = self.files
            .iter()
            .map(|(path, file)| {
                let path_lower = path.to_lowercase();
                let score = keywords.iter()
                    .filter(|k| path_lower.contains(*k))
                    .count() as f32;
                (path.clone(), score, file.token_count)
            })
            .filter(|(_, score, _)| *score > 0.0)
            .collect();
        
        scored_files.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
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
                    FileContent::Chunked(chunks) => {
                        chunks.iter().map(|c| &c.content).collect::<Vec<_>>().join("\n")
                    }
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
        .unwrap()
        .as_secs()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

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
}
