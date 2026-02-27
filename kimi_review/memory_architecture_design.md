# Selfware Memory Architecture for 1M Token Context
## Comprehensive Design Document

---

## Executive Summary

This document presents a complete memory and context management system designed to maximize the utilization of Qwen3 Coder's 1M token context window for recursive self-improvement in Selfware. The architecture enables the agent to:

- Read and understand its entire codebase within context
- Maintain multi-day execution state with minimal information loss
- Efficiently self-reference (read its own source code)
- Perform sophisticated token budgeting and allocation
- Compress and summarize memories intelligently

---

## 1. Current State Analysis

### Existing Components

| Component | File | Current Limitations |
|-----------|------|---------------------|
| AgentMemory | `memory.rs` | MAX_ENTRIES = 10,000; MAX_MEMORY_TOKENS = 500,000 |
| EpisodicMemory | `cognitive/episodic.rs` | Basic episode storage, limited retrieval |
| KnowledgeGraph | `cognitive/knowledge_graph.rs` | Entity relationships, no token budgeting |
| RAG System | `cognitive/rag.rs` | max_context_tokens = 8000 |
| ContextCompressor | `agent/context.rs` | MAX_MESSAGE_COUNT = 512 |

### Key Limitations

1. **Fixed limits**: Hard-coded limits don't scale to 1M context
2. **No hierarchical memory**: Flat structure causes information loss
3. **Basic eviction**: Simple FIFO doesn't preserve important information
4. **Limited self-reference**: No systematic way to include source code in context
5. **No token budgeting**: No sophisticated allocation across memory types

---

## 2. Hierarchical Memory Architecture

### 2.1 Three-Layer Memory Model

```
+-----------------------------------------------------------------------------+
|                         1M TOKEN CONTEXT WINDOW                              |
+-----------------------------------------------------------------------------+
|  LAYER 1: WORKING MEMORY (Immediate Context)                                |
|  +---------------------------------------------------------------------+   |
|  |  * Active conversation (last N turns)                               |   |
|  |  * Current task context                                             |   |
|  |  * Recently accessed code                                           |   |
|  |  Size: ~100K tokens (10%)                                           |   |
|  +---------------------------------------------------------------------+   |
+-----------------------------------------------------------------------------+
|  LAYER 2: EPISODIC MEMORY (Recent Experiences)                              |
|  +---------------------------------------------------------------------+   |
|  |  * Session history (compressed)                                     |   |
|  |  * Tool executions and results                                      |   |
|  |  * Errors and learnings                                             |   |
|  |  Size: ~200K tokens (20%)                                           |   |
|  +---------------------------------------------------------------------+   |
+-----------------------------------------------------------------------------+
|  LAYER 3: SEMANTIC MEMORY (Knowledge & Codebase)                            |
|  +---------------------------------------------------------------------+   |
|  |  * Selfware source code (indexed)                                   |   |
|  |  * Knowledge graph (entities & relations)                           |   |
|  |  * Long-term patterns and insights                                  |   |
|  |  Size: ~700K tokens (70%)                                           |   |
|  +---------------------------------------------------------------------+   |
+-----------------------------------------------------------------------------+
```

### 2.2 Memory Hierarchy Implementation

```rust
// src/cognitive/memory_hierarchy.rs

use std::collections::{HashMap, VecDeque, BTreeMap};
use std::sync::Arc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use anyhow::Result;

/// Token budget allocation for 1M context
pub const TOTAL_CONTEXT_TOKENS: usize = 1_000_000;

/// Layer-specific token budgets
pub struct TokenBudget {
    /// Working memory: immediate context (10%)
    pub working_memory: usize,
    /// Episodic memory: recent experiences (20%)
    pub episodic_memory: usize,
    /// Semantic memory: knowledge & codebase (70%)
    pub semantic_memory: usize,
    /// Reserve for response generation (10% buffer)
    pub response_reserve: usize,
}

impl Default for TokenBudget {
    fn default() -> Self {
        Self {
            working_memory: 100_000,    // 10%
            episodic_memory: 200_000,   // 20%
            semantic_memory: 700_000,   // 70%
            response_reserve: 100_000,  // 10% buffer
        }
    }
}

impl TokenBudget {
    /// Create budget optimized for codebase understanding
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
}

/// Unified memory manager coordinating all memory layers
pub struct HierarchicalMemory {
    /// Token budget configuration
    budget: TokenBudget,
    /// Layer 1: Working memory (immediate context)
    working: WorkingMemory,
    /// Layer 2: Episodic memory (experiences)
    episodic: EpisodicMemory,
    /// Layer 3: Semantic memory (knowledge)
    semantic: SemanticMemory,
    /// Current token usage by layer
    usage: MemoryUsage,
    /// Memory statistics and metrics
    metrics: MemoryMetrics,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryUsage {
    pub working_tokens: usize,
    pub episodic_tokens: usize,
    pub semantic_tokens: usize,
    pub total_tokens: usize,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryMetrics {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub evictions: u64,
    pub compressions: u64,
    pub avg_retrieval_time_ms: f64,
}
```

---

## 3. Layer 1: Working Memory

### 3.1 Design

Working memory holds the immediate conversation context and active task state. It uses a sliding window with intelligent retention.

```rust
// src/cognitive/working_memory.rs

use std::collections::VecDeque;
use crate::api::types::Message;
use crate::token_count::estimate_tokens_with_overhead;

/// Working memory for immediate context
pub struct WorkingMemory {
    /// Maximum tokens for working memory
    max_tokens: usize,
    /// Current token count
    current_tokens: usize,
    /// Message buffer with importance scoring
    messages: VecDeque<WorkingMemoryEntry>,
    /// Active code context (files currently being edited)
    active_code: Vec<ActiveCodeContext>,
    /// Current task/focus
    current_task: Option<TaskContext>,
}

/// Entry in working memory with metadata
#[derive(Debug, Clone)]
pub struct WorkingMemoryEntry {
    /// The message content
    pub message: Message,
    /// Estimated token count
    pub token_count: usize,
    /// Importance score (0.0 - 1.0)
    pub importance: f32,
    /// Timestamp for age-based eviction
    pub timestamp: u64,
    /// Whether this entry can be compressed
    pub compressible: bool,
}

/// Active code file context
#[derive(Debug, Clone)]
pub struct ActiveCodeContext {
    /// File path
    pub path: String,
    /// File content (or summary if large)
    pub content: CodeContent,
    /// Last accessed timestamp
    pub last_accessed: u64,
    /// Edit history for this file
    pub edit_history: Vec<CodeEdit>,
}

/// Code content can be full or summarized
#[derive(Debug, Clone)]
pub enum CodeContent {
    /// Full file content
    Full(String),
    /// Summarized content with key parts
    Summary { 
        overview: String, 
        key_functions: Vec<String>,
        imports: Vec<String>,
    },
    /// Reference to semantic memory
    Reference { path: String, summary: String },
}

/// Current task context
#[derive(Debug, Clone)]
pub struct TaskContext {
    /// Task description
    pub description: String,
    /// Goal state
    pub goal: String,
    /// Progress so far
    pub progress: Vec<String>,
    /// Next steps
    pub next_steps: Vec<String>,
    /// Relevant files
    pub relevant_files: Vec<String>,
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
    
    /// Add a message to working memory
    pub fn add_message(&mut self, message: Message, importance: f32) {
        let tokens = estimate_tokens_with_overhead(&message.content, 50);
        
        // Create entry
        let entry = WorkingMemoryEntry {
            message: message.clone(),
            token_count: tokens,
            importance,
            timestamp: current_timestamp(),
            compressible: message.role != "system",
        };
        
        // Evict if necessary
        while self.current_tokens + tokens > self.max_tokens {
            self.evict_least_important();
        }
        
        self.current_tokens += tokens;
        self.messages.push_back(entry);
    }
    
    /// Evict the least important entry
    fn evict_least_important(&mut self) {
        if let Some((idx, _)) = self.messages
            .iter()
            .enumerate()
            .filter(|(_, e)| e.compressible)
            .min_by(|a, b| {
                let score_a = a.1.importance / (current_timestamp() - a.1.timestamp + 1) as f32;
                let score_b = b.1.importance / (current_timestamp() - b.1.timestamp + 1) as f32;
                score_a.partial_cmp(&score_b).unwrap()
            }) {
            if let Some(entry) = self.messages.remove(idx) {
                self.current_tokens -= entry.token_count;
            }
        }
    }
    
    /// Get context for LLM prompt
    pub fn get_context(&self) -> WorkingContext {
        WorkingContext {
            messages: self.messages.iter().map(|e| e.message.clone()).collect(),
            active_code: self.active_code.clone(),
            current_task: self.current_task.clone(),
        }
    }
    
    /// Update active code context
    pub fn set_active_code(&mut self, path: String, content: String) {
        let tokens = estimate_tokens_with_overhead(&content, 0);
        
        // If file is too large, create summary
        let code_content = if tokens > 10_000 {
            CodeContent::Reference {
                path: path.clone(),
                summary: format!("Large file ({} tokens). Use semantic search for details.", tokens),
            }
        } else {
            CodeContent::Full(content)
        };
        
        // Update or add
        if let Some(existing) = self.active_code.iter_mut().find(|c| c.path == path) {
            existing.content = code_content;
            existing.last_accessed = current_timestamp();
        } else {
            self.active_code.push(ActiveCodeContext {
                path,
                content: code_content,
                last_accessed: current_timestamp(),
                edit_history: Vec::new(),
            });
        }
        
        // Trim old active code
        self.active_code.sort_by(|a, b| b.last_accessed.cmp(&a.last_accessed));
        self.active_code.truncate(10);
    }
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
```

---

## 4. Layer 2: Episodic Memory

### 4.1 Enhanced Episodic Memory

```rust
// src/cognitive/episodic_memory.rs

use std::collections::{HashMap, VecDeque, BTreeSet};
use serde::{Deserialize, Serialize};
use crate::vector_store::{EmbeddingBackend, VectorIndex};

/// Enhanced episodic memory with 1M context support
pub struct EpisodicMemory {
    /// Maximum tokens for episodic memory
    max_tokens: usize,
    /// Current token usage
    current_tokens: usize,
    /// Episodes stored by importance tiers
    tiers: EpisodicTiers,
    /// Vector index for semantic search
    vector_index: VectorIndex,
    /// Embedding backend
    embedding: Arc<dyn EmbeddingBackend>,
    /// Episode summaries for quick retrieval
    summaries: VecDeque<EpisodeSummary>,
}

/// Episodes organized by importance
pub struct EpisodicTiers {
    /// Critical episodes (never evicted)
    critical: Vec<Episode>,
    /// High importance episodes
    high: VecDeque<Episode>,
    /// Normal importance episodes
    normal: VecDeque<Episode>,
    /// Low importance episodes (first to evict)
    low: VecDeque<Episode>,
}

/// Episode with enhanced metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    /// Unique identifier
    pub id: String,
    /// Episode type
    pub episode_type: EpisodeType,
    /// Main content
    pub content: String,
    /// Token count
    pub token_count: usize,
    /// Importance level
    pub importance: Importance,
    /// Timestamp
    pub timestamp: u64,
    /// Semantic embedding (stored separately)
    pub embedding_id: String,
    /// Related episodes
    pub related_episodes: Vec<String>,
    /// Extracted insights
    pub insights: Vec<String>,
    /// Whether this episode has been summarized
    pub is_summarized: bool,
    /// Original episode ID if this is a summary
    pub original_id: Option<String>,
}

/// Episode summary for quick access
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeSummary {
    pub id: String,
    pub episode_type: EpisodeType,
    pub summary: String,
    pub timestamp: u64,
    pub importance: Importance,
    pub token_count: usize,
}

/// Episode importance levels with retention policies
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Importance {
    /// Transient, can be forgotten quickly
    Transient = 0,
    /// Low importance, short retention
    Low = 1,
    /// Normal importance, medium retention
    Normal = 2,
    /// High importance, long retention
    High = 3,
    /// Critical, never forgotten
    Critical = 4,
}

impl EpisodicMemory {
    pub fn new(max_tokens: usize, embedding: Arc<dyn EmbeddingBackend>) -> Result<Self> {
        Ok(Self {
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
            summaries: VecDeque::new(),
        })
    }
    
    /// Record a new episode
    pub async fn record(&mut self, mut episode: Episode) -> Result<()> {
        // Calculate token count
        episode.token_count = estimate_tokens_with_overhead(&episode.content, 100);
        
        // Generate embedding
        let embedding = self.embedding.embed(&episode.content).await?;
        episode.embedding_id = self.vector_index.add(embedding, episode.id.clone()).await?;
        
        // Add to appropriate tier
        self.add_to_tier(episode);
        
        // Evict if over budget
        self.maintain_budget().await?;
        
        Ok(())
    }
    
    /// Add episode to appropriate tier
    fn add_to_tier(&mut self, episode: Episode) {
        self.current_tokens += episode.token_count;
        
        match episode.importance {
            Importance::Critical => self.tiers.critical.push(episode),
            Importance::High => self.tiers.high.push_back(episode),
            Importance::Normal => self.tiers.normal.push_back(episode),
            Importance::Low | Importance::Transient => self.tiers.low.push_back(episode),
        }
    }
    
    /// Maintain token budget through intelligent eviction
    async fn maintain_budget(&mut self) -> Result<()> {
        while self.current_tokens > self.max_tokens {
            // Try to compress first
            if self.try_compress_oldest().await? {
                continue;
            }
            
            // Then evict from lowest tier
            if !self.tiers.low.is_empty() {
                if let Some(episode) = self.tiers.low.pop_front() {
                    self.remove_episode(&episode).await?;
                }
            } else if !self.tiers.normal.is_empty() {
                if let Some(episode) = self.tiers.normal.pop_front() {
                    self.remove_episode(&episode).await?;
                }
            } else if !self.tiers.high.is_empty() {
                if let Some(episode) = self.tiers.high.pop_front() {
                    self.remove_episode(&episode).await?;
                }
            } else {
                // Can't evict critical, we're in trouble
                break;
            }
        }
        Ok(())
    }
    
    /// Try to compress the oldest compressible episode
    async fn try_compress_oldest(&mut self) -> Result<bool> {
        // Find oldest normal episode that hasn't been summarized
        if let Some(idx) = self.tiers.normal.iter()
            .position(|e| !e.is_summarized && e.importance <= Importance::Normal) {
            
            let episode = self.tiers.normal.remove(idx).unwrap();
            let summary = self.summarize_episode(&episode).await?;
            
            self.current_tokens -= episode.token_count;
            self.current_tokens += summary.token_count;
            
            self.summaries.push_back(summary);
            return Ok(true);
        }
        Ok(false)
    }
    
    /// Summarize an episode using LLM
    async fn summarize_episode(&self, episode: &Episode) -> Result<EpisodeSummary> {
        // This would call the LLM to create a summary
        // For now, create a simple summary
        let summary_text = format!(
            "[{}] {}: {}",
            episode.episode_type.as_str(),
            format_timestamp(episode.timestamp),
            &episode.content.chars().take(200).collect::<String>()
        );
        
        Ok(EpisodeSummary {
            id: format!("summary-{}", episode.id),
            episode_type: episode.episode_type.clone(),
            summary: summary_text,
            timestamp: episode.timestamp,
            importance: episode.importance,
            token_count: estimate_tokens_with_overhead(&summary_text, 50),
        })
    }
    
    /// Retrieve relevant episodes by semantic similarity
    pub async fn retrieve_relevant(
        &self, 
        query: &str, 
        limit: usize,
        min_importance: Importance
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
    
    /// Get recent episodes within token budget
    pub fn get_recent_within_budget(&self, budget: usize) -> Vec<&Episode> {
        let mut result = Vec::new();
        let mut used = 0usize;
        
        // Start with critical
        for episode in &self.tiers.critical {
            if used + episode.token_count <= budget {
                result.push(episode);
                used += episode.token_count;
            }
        }
        
        // Then high, normal, low
        for tier in [&self.tiers.high, &self.tiers.normal, &self.tiers.low] {
            for episode in tier.iter().rev() {
                if used + episode.token_count <= budget {
                    result.push(episode);
                    used += episode.token_count;
                } else {
                    break;
                }
            }
        }
        
        result
    }
    
    fn find_episode(&self, id: &str) -> Option<Episode> {
        self.tiers.critical.iter()
            .chain(self.tiers.high.iter())
            .chain(self.tiers.normal.iter())
            .chain(self.tiers.low.iter())
            .find(|e| e.id == id)
            .cloned()
    }
    
    async fn remove_episode(&mut self, episode: &Episode) -> Result<()> {
        self.current_tokens -= episode.token_count;
        self.vector_index.remove(&episode.embedding_id).await?;
        Ok(())
    }
}

fn format_timestamp(timestamp: u64) -> String {
    let datetime = chrono::DateTime::from_timestamp(timestamp as i64, 0)
        .unwrap_or_default();
    datetime.format("%Y-%m-%d %H:%M").to_string()
}
```

---

## 5. Layer 3: Semantic Memory (Codebase & Knowledge)

### 5.1 Self-Referential Codebase Index

```rust
// src/cognitive/semantic_memory.rs

use std::collections::{HashMap, HashSet, BTreeMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tree_sitter::{Parser, Query, QueryCursor};

/// Semantic memory for codebase and long-term knowledge
pub struct SemanticMemory {
    /// Maximum tokens for semantic memory
    max_tokens: usize,
    /// Current token usage
    current_tokens: usize,
    /// Selfware source code index
    selfware_index: CodebaseIndex,
    /// Project codebase index (if working on another project)
    project_index: Option<CodebaseIndex>,
    /// Knowledge graph
    knowledge_graph: KnowledgeGraph,
    /// Vector store for semantic search
    vector_store: Arc<RwLock<VectorStore>>,
    /// Embedding backend
    embedding: Arc<dyn EmbeddingBackend>,
    /// Module dependency graph
    dependency_graph: DependencyGraph,
}

/// Comprehensive codebase index
pub struct CodebaseIndex {
    /// Root path of the codebase
    root_path: PathBuf,
    /// Indexed files
    files: HashMap<String, IndexedFile>,
    /// Module structure
    modules: BTreeMap<String, ModuleInfo>,
    /// Function/struct index
    symbols: HashMap<String, SymbolInfo>,
    /// File dependency graph
    file_dependencies: HashMap<String, HashSet<String>>,
    /// Total tokens in index
    total_tokens: usize,
}

/// An indexed file with multiple representations
pub struct IndexedFile {
    /// File path (relative to root)
    pub path: String,
    /// Full content (may be chunked)
    pub content: FileContent,
    /// AST-based structure
    pub structure: FileStructure,
    /// Token count
    pub token_count: usize,
    /// Last modified
    pub last_modified: u64,
    /// Semantic embedding chunks
    pub embedding_chunks: Vec<EmbeddingChunk>,
}

/// File content with multiple representations
pub enum FileContent {
    /// Full content (for small files)
    Full(String),
    /// Chunked content (for large files)
    Chunked(Vec<ContentChunk>),
    /// Summary only (for very large files)
    Summary(FileSummary),
}

/// Content chunk for large files
pub struct ContentChunk {
    /// Chunk index
    pub index: usize,
    /// Chunk content
    pub content: String,
    /// Token count
    pub token_count: usize,
    /// Start line
    pub start_line: usize,
    /// End line
    pub end_line: usize,
    /// Chunk type (function, struct, etc.)
    pub chunk_type: ChunkType,
    /// Semantic embedding ID
    pub embedding_id: String,
}

/// File structure from AST parsing
pub struct FileStructure {
    /// Imports/use statements
    pub imports: Vec<ImportInfo>,
    /// Defined modules
    pub modules: Vec<ModuleDef>,
    /// Functions
    pub functions: Vec<FunctionInfo>,
    /// Structs
    pub structs: Vec<StructInfo>,
    /// Traits
    pub traits: Vec<TraitInfo>,
    /// Impl blocks
    pub impls: Vec<ImplInfo>,
    /// Enums
    pub enums: Vec<EnumInfo>,
    /// Constants
    pub constants: Vec<ConstantInfo>,
    /// Type aliases
    pub type_aliases: Vec<TypeAliasInfo>,
}

/// Symbol information for cross-referencing
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    /// Symbol name
    pub name: String,
    /// Symbol type
    pub symbol_type: SymbolType,
    /// Defining file
    pub file_path: String,
    /// Line number
    pub line: usize,
    /// Documentation
    pub docs: Option<String>,
    /// Signature
    pub signature: String,
    /// Token count of definition
    pub token_count: usize,
    /// Public visibility
    pub is_public: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolType {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
    Module,
    Constant,
    TypeAlias,
    Macro,
}

/// Module dependency graph
pub struct DependencyGraph {
    /// Module nodes
    nodes: HashMap<String, ModuleNode>,
    /// Dependency edges
    edges: Vec<DependencyEdge>,
}

pub struct ModuleNode {
    pub path: String,
    pub token_count: usize,
    pub public_symbols: Vec<String>,
    pub dependencies: Vec<String>,
}

pub struct DependencyEdge {
    pub from: String,
    pub to: String,
    pub edge_type: DependencyType,
}

#[derive(Debug, Clone, Copy)]
pub enum DependencyType {
    Uses,
    Implements,
    Extends,
    References,
}

impl SemanticMemory {
    pub fn new(
        max_tokens: usize,
        embedding: Arc<dyn EmbeddingBackend>,
        selfware_path: PathBuf,
    ) -> Result<Self> {
        let vector_store = Arc::new(RwLock::new(VectorStore::new(embedding.clone())));
        
        Ok(Self {
            max_tokens,
            current_tokens: 0,
            selfware_index: CodebaseIndex::new(selfware_path),
            project_index: None,
            knowledge_graph: KnowledgeGraph::new(),
            vector_store,
            embedding,
            dependency_graph: DependencyGraph::new(),
        })
    }
    
    /// Index the Selfware codebase
    pub async fn index_selfware(&mut self) -> Result<()> {
        info!("Indexing Selfware codebase...");
        
        let source_files = self.discover_source_files(&self.selfware_index.root_path).await?;
        
        for file_path in source_files {
            match self.index_file(&file_path, true).await {
                Ok(indexed) => {
                    self.selfware_index.add_file(indexed)?;
                }
                Err(e) => {
                    warn!("Failed to index {}: {}", file_path.display(), e);
                }
            }
        }
        
        // Build dependency graph
        self.build_dependency_graph().await?;
        
        info!(
            "Indexed {} files, {} tokens",
            self.selfware_index.files.len(),
            self.selfware_index.total_tokens
        );
        
        Ok(())
    }
    
    /// Index a single file
    async fn index_file(&self, path: &Path, is_selfware: bool) -> Result<IndexedFile> {
        let content = tokio::fs::read_to_string(path).await?;
        let token_count = estimate_tokens_with_overhead(&content, 0);
        
        // Parse file structure
        let structure = self.parse_file_structure(path, &content).await?;
        
        // Determine content strategy based on size
        let file_content = if token_count < 5_000 {
            // Small file: keep full content
            FileContent::Full(content.clone())
        } else if token_count < 50_000 {
            // Medium file: chunk it
            FileContent::Chunked(self.chunk_file(&content, &structure).await?)
        } else {
            // Large file: summary only
            FileContent::Summary(self.summarize_file(&content, &structure).await?)
        };
        
        // Generate embeddings for chunks
        let embedding_chunks = self.generate_embeddings(&file_content).await?;
        
        Ok(IndexedFile {
            path: path.strip_prefix(&self.selfware_index.root_path)?
                .to_string_lossy()
                .to_string(),
            content: file_content,
            structure,
            token_count,
            last_modified: get_file_modified_time(path).await?,
            embedding_chunks,
        })
    }
    
    /// Retrieve code context for a query
    pub async fn retrieve_code_context(
        &self,
        query: &str,
        max_tokens: usize,
        include_related: bool,
    ) -> Result<CodeContext> {
        // Generate query embedding
        let query_embedding = self.embedding.embed(query).await?;
        
        // Search vector store
        let search_results = {
            let store = self.vector_store.read();
            store.search(&query_embedding, 20).await?
        };
        
        // Build context within token budget
        let mut context = CodeContext::new();
        let mut used_tokens = 0usize;
        let mut included_files: HashSet<String> = HashSet::new();
        
        for result in search_results {
            if let Some(file) = self.selfware_index.files.get(&result.file_path) {
                // Check if adding this would exceed budget
                let tokens_needed = self.estimate_context_tokens(file, &result);
                
                if used_tokens + tokens_needed > max_tokens {
                    break;
                }
                
                // Add to context
                context.add_file(file, &result);
                included_files.insert(file.path.clone());
                used_tokens += tokens_needed;
                
                // Add related files if requested
                if include_related && used_tokens < max_tokens {
                    let related = self.find_related_files(&file.path);
                    for related_path in related {
                        if !included_files.contains(&related_path) {
                            if let Some(related_file) = self.selfware_index.files.get(&related_path) {
                                let related_tokens = related_file.token_count.min(1000);
                                if used_tokens + related_tokens <= max_tokens {
                                    context.add_file_summary(related_file);
                                    included_files.insert(related_path);
                                    used_tokens += related_tokens;
                                }
                            }
                        }
                    }
                }
            }
        }
        
        Ok(context)
    }
    
    /// Get selfware source code for self-improvement
    pub fn get_selfware_source(&self, module_path: Option<&str>) -> Result<SelfwareSource> {
        match module_path {
            Some(path) => {
                // Get specific module
                if let Some(file) = self.selfware_index.files.get(path) {
                    Ok(SelfwareSource::Single(file.to_context_string()))
                } else if let Some(module) = self.selfware_index.modules.get(path) {
                    // Get entire module
                    let mut content = String::new();
                    for file_path in &module.files {
                        if let Some(file) = self.selfware_index.files.get(file_path) {
                            content.push_str(&file.to_context_string());
                            content.push_str("\n\n");
                        }
                    }
                    Ok(SelfwareSource::Module { path: path.to_string(), content })
                } else {
                    Err(anyhow!("Module or file not found: {}", path))
                }
            }
            None => {
                // Get key files for self-improvement
                let key_files = vec![
                    "src/memory.rs",
                    "src/cognitive/mod.rs",
                    "src/agent/context.rs",
                    "src/cognitive/episodic.rs",
                    "src/cognitive/rag.rs",
                    "src/cognitive/knowledge_graph.rs",
                ];
                
                let mut sources = HashMap::new();
                for path in key_files {
                    if let Some(file) = self.selfware_index.files.get(path) {
                        sources.insert(path.to_string(), file.to_context_string());
                    }
                }
                
                Ok(SelfwareSource::KeyFiles(sources))
            }
        }
    }
}

/// Code context for LLM
pub struct CodeContext {
    pub files: Vec<FileContext>,
    pub total_tokens: usize,
}

pub struct FileContext {
    pub path: String,
    pub content: String,
    pub relevance_score: f32,
    pub chunk_type: Option<ChunkType>,
}

/// Selfware source code for self-improvement
pub enum SelfwareSource {
    Single(String),
    Module { path: String, content: String },
    KeyFiles(HashMap<String, String>),
}
```

---

## 6. Token Allocation Strategy

### 6.1 Dynamic Token Budgeting

```rust
// src/cognitive/token_budget.rs

/// Dynamic token budget allocator
pub struct TokenBudgetAllocator {
    /// Total available tokens
    total_tokens: usize,
    /// Current allocation by layer
    allocation: BudgetAllocation,
    /// Usage history for adaptive allocation
    usage_history: Vec<UsageSnapshot>,
    /// Task type for specialized allocation
    task_type: TaskType,
}

#[derive(Debug, Clone)]
pub struct BudgetAllocation {
    pub working: usize,
    pub episodic: usize,
    pub semantic: usize,
    pub reserve: usize,
}

#[derive(Debug, Clone)]
pub struct UsageSnapshot {
    pub timestamp: u64,
    pub working_used: usize,
    pub episodic_used: usize,
    pub semantic_used: usize,
    pub task_type: TaskType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskType {
    Conversation,
    CodeAnalysis,
    SelfImprovement,
    CodeGeneration,
    Debugging,
    Refactoring,
}

impl TokenBudgetAllocator {
    pub fn new(total_tokens: usize, task_type: TaskType) -> Self {
        let allocation = Self::allocate_for_task(total_tokens, task_type);
        
        Self {
            total_tokens,
            allocation,
            usage_history: Vec::new(),
            task_type,
        }
    }
    
    /// Allocate tokens based on task type
    fn allocate_for_task(total: usize, task: TaskType) -> BudgetAllocation {
        match task {
            TaskType::Conversation => BudgetAllocation {
                working: total * 30 / 100,    // 30% for conversation
                episodic: total * 30 / 100,   // 30% for context
                semantic: total * 30 / 100,   // 30% for knowledge
                reserve: total * 10 / 100,    // 10% reserve
            },
            TaskType::CodeAnalysis => BudgetAllocation {
                working: total * 15 / 100,
                episodic: total * 15 / 100,
                semantic: total * 60 / 100,   // Heavy on codebase
                reserve: total * 10 / 100,
            },
            TaskType::SelfImprovement => BudgetAllocation {
                working: total * 10 / 100,
                episodic: total * 10 / 100,
                semantic: total * 70 / 100,   // Maximum for self-code
                reserve: total * 10 / 100,
            },
            TaskType::CodeGeneration => BudgetAllocation {
                working: total * 20 / 100,
                episodic: total * 20 / 100,
                semantic: total * 50 / 100,
                reserve: total * 10 / 100,
            },
            TaskType::Debugging => BudgetAllocation {
                working: total * 25 / 100,
                episodic: total * 35 / 100,   // Heavy on recent events
                semantic: total * 30 / 100,
                reserve: total * 10 / 100,
            },
            TaskType::Refactoring => BudgetAllocation {
                working: total * 15 / 100,
                episodic: total * 15 / 100,
                semantic: total * 60 / 100,
                reserve: total * 10 / 100,
            },
        }
    }
    
    /// Adapt allocation based on actual usage
    pub fn adapt(&mut self) {
        if self.usage_history.len() < 5 {
            return;
        }
        
        // Calculate average usage
        let recent: Vec<_> = self.usage_history.iter().rev().take(10).collect();
        let avg_working: usize = recent.iter().map(|s| s.working_used).sum::<usize>() / recent.len();
        let avg_episodic: usize = recent.iter().map(|s| s.episodic_used).sum::<usize>() / recent.len();
        let avg_semantic: usize = recent.iter().map(|s| s.semantic_used).sum::<usize>() / recent.len();
        
        // Adjust if significantly under/over used
        let working_ratio = avg_working as f32 / self.allocation.working as f32;
        let episodic_ratio = avg_episodic as f32 / self.allocation.episodic as f32;
        let semantic_ratio = avg_semantic as f32 / self.allocation.semantic as f32;
        
        // Reallocate from underused to overused
        if working_ratio < 0.5 && semantic_ratio > 0.9 {
            // Move tokens from working to semantic
            let transfer = self.allocation.working / 4;
            self.allocation.working -= transfer;
            self.allocation.semantic += transfer;
        }
        
        if episodic_ratio < 0.5 && semantic_ratio > 0.9 {
            let transfer = self.allocation.episodic / 4;
            self.allocation.episodic -= transfer;
            self.allocation.semantic += transfer;
        }
    }
    
    /// Record usage snapshot
    pub fn record_usage(&mut self, working: usize, episodic: usize, semantic: usize) {
        self.usage_history.push(UsageSnapshot {
            timestamp: current_timestamp(),
            working_used: working,
            episodic_used: episodic,
            semantic_used: semantic,
            task_type: self.task_type,
        });
        
        // Keep history bounded
        if self.usage_history.len() > 100 {
            self.usage_history.remove(0);
        }
    }
    
    /// Get current allocation
    pub fn get_allocation(&self) -> &BudgetAllocation {
        &self.allocation
    }
    
    /// Change task type and reallocate
    pub fn set_task_type(&mut self, task_type: TaskType) {
        self.task_type = task_type;
        self.allocation = Self::allocate_for_task(self.total_tokens, task_type);
    }
}
```

---

## 7. Self-Referential Context Management

### 7.1 Agent Self-Reference System

```rust
// src/cognitive/self_reference.rs

/// System for agent to read and understand its own source code
pub struct SelfReferenceSystem {
    /// Semantic memory reference
    semantic: Arc<RwLock<SemanticMemory>>,
    /// Current self-model (what the agent knows about itself)
    self_model: SelfModel,
    /// Cache of frequently accessed self-code
    code_cache: LruCache<String, CachedCode>,
    /// Recent modifications tracked
    recent_modifications: VecDeque<CodeModification>,
}

/// The agent's model of itself
#[derive(Debug, Clone)]
pub struct SelfModel {
    /// Key modules and their purposes
    pub modules: HashMap<String, ModuleSelfModel>,
    /// Architecture understanding
    pub architecture: ArchitectureModel,
    /// Current capabilities
    pub capabilities: Vec<Capability>,
    /// Known limitations
    pub limitations: Vec<String>,
    /// Recent changes made to self
    pub recent_changes: Vec<SelfChange>,
    /// Performance characteristics
    pub performance: PerformanceModel,
}

#[derive(Debug, Clone)]
pub struct ModuleSelfModel {
    pub path: String,
    pub purpose: String,
    pub key_components: Vec<String>,
    pub dependencies: Vec<String>,
    pub token_count: usize,
    pub last_modified: u64,
}

#[derive(Debug, Clone)]
pub struct ArchitectureModel {
    pub layers: Vec<ArchitectureLayer>,
    pub data_flow: Vec<DataFlow>,
    pub design_patterns: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Capability {
    pub name: String,
    pub description: String,
    pub implementing_modules: Vec<String>,
    pub confidence: f32,
}

/// Code modification tracking
#[derive(Debug, Clone)]
pub struct CodeModification {
    pub timestamp: u64,
    pub file_path: String,
    pub modification_type: ModType,
    pub description: String,
    pub tokens_changed: i64,
}

#[derive(Debug, Clone, Copy)]
pub enum ModType {
    Added,
    Modified,
    Deleted,
    Refactored,
}

impl SelfReferenceSystem {
    pub fn new(semantic: Arc<RwLock<SemanticMemory>>) -> Self {
        Self {
            semantic,
            self_model: SelfModel::default(),
            code_cache: LruCache::new(100),
            recent_modifications: VecDeque::new(),
        }
    }
    
    /// Initialize self-model from codebase
    pub async fn initialize_self_model(&mut self) -> Result<()> {
        let semantic = self.semantic.read();
        
        // Build module models
        for (path, module) in &semantic.selfware_index.modules {
            let model = ModuleSelfModel {
                path: path.clone(),
                purpose: self.infer_module_purpose(path, module),
                key_components: module.public_symbols.clone(),
                dependencies: module.dependencies.clone(),
                token_count: module.token_count,
                last_modified: module.last_modified,
            };
            self.self_model.modules.insert(path.clone(), model);
        }
        
        // Infer architecture
        self.self_model.architecture = self.infer_architecture(&semantic);
        
        // Identify capabilities
        self.self_model.capabilities = self.identify_capabilities(&semantic);
        
        Ok(())
    }
    
    /// Get context for self-improvement task
    pub async fn get_self_improvement_context(
        &self,
        improvement_goal: &str,
        max_tokens: usize,
    ) -> Result<SelfImprovementContext> {
        let semantic = self.semantic.read();
        
        // Search for relevant code
        let relevant = semantic.retrieve_code_context(
            improvement_goal,
            max_tokens * 6 / 10, // 60% for relevant code
            true,
        ).await?;
        
        // Get self-model context (20%)
        let self_model_context = self.format_self_model(max_tokens * 2 / 10);
        
        // Get recent modifications (10%)
        let recent_mods = self.format_recent_modifications(max_tokens / 10);
        
        // Get architecture overview (10%)
        let architecture = self.format_architecture(max_tokens / 10);
        
        Ok(SelfImprovementContext {
            relevant_code: relevant,
            self_model: self_model_context,
            recent_modifications: recent_mods,
            architecture,
            goal: improvement_goal.to_string(),
        })
    }
    
    /// Read own source code for a specific module
    pub async fn read_own_code(&self, module_path: &str) -> Result<String> {
        // Check cache first
        if let Some(cached) = self.code_cache.get(module_path) {
            if !self.is_stale(cached) {
                return Ok(cached.content.clone());
            }
        }
        
        let semantic = self.semantic.read();
        let source = semantic.get_selfware_source(Some(module_path))?;
        
        let content = match source {
            SelfwareSource::Single(c) => c,
            SelfwareSource::Module { content, .. } => content,
            SelfwareSource::KeyFiles(mut map) => {
                map.remove(module_path)
                    .ok_or_else(|| anyhow!("Module not found: {}", module_path))?
            }
        };
        
        // Cache the result
        self.code_cache.put(module_path.to_string(), CachedCode {
            content: content.clone(),
            timestamp: current_timestamp(),
            token_count: estimate_tokens_with_overhead(&content, 0),
        });
        
        Ok(content)
    }
    
    /// Track a modification to self
    pub fn track_modification(&mut self, modification: CodeModification) {
        self.recent_modifications.push_back(modification);
        
        // Keep bounded
        if self.recent_modifications.len() > 100 {
            self.recent_modifications.pop_front();
        }
        
        // Update self-model
        self.update_self_model_for_modification();
    }
    
    /// Format self-model for context
    fn format_self_model(&self, max_tokens: usize) -> String {
        let mut context = String::new();
        
        context.push_str("# Selfware Self-Model\n\n");
        
        // Capabilities
        context.push_str("## Capabilities\n");
        for cap in &self.self_model.capabilities {
            context.push_str(&format!("- **{}**: {}\n", cap.name, cap.description));
        }
        context.push_str("\n");
        
        // Key modules
        context.push_str("## Key Modules\n");
        for (path, module) in &self.self_model.modules {
            if module.token_count < 5000 { // Only include smaller modules
                context.push_str(&format!(
                    "- **{}**: {} ({} tokens)\n",
                    path, module.purpose, module.token_count
                ));
            }
        }
        
        // Limit to max tokens
        let tokens = estimate_tokens_with_overhead(&context, 0);
        if tokens > max_tokens {
            // Truncate intelligently
            context = self.truncate_to_tokens(&context, max_tokens);
        }
        
        context
    }
    
    /// Infer module purpose from code analysis
    fn infer_module_purpose(&self, path: &str, module: &ModuleInfo) -> String {
        // Use module path and contents to infer purpose
        let purpose = if path.contains("memory") {
            "Memory management and context tracking"
        } else if path.contains("cognitive") {
            "Cognitive functions: learning, reasoning, knowledge"
        } else if path.contains("agent") {
            "Agent execution and control flow"
        } else if path.contains("api") {
            "API client and external communication"
        } else if path.contains("tools") {
            "Tool definitions and implementations"
        } else {
            "General functionality"
        };
        
        purpose.to_string()
    }
    
    /// Identify capabilities from code analysis
    fn identify_capabilities(&self, semantic: &SemanticMemory) -> Vec<Capability> {
        let mut capabilities = Vec::new();
        
        // Check for memory capabilities
        if semantic.selfware_index.files.contains_key("src/memory.rs") {
            capabilities.push(Capability {
                name: "Memory Management".to_string(),
                description: "Track and manage conversation context".to_string(),
                implementing_modules: vec!["src/memory.rs".to_string()],
                confidence: 0.9,
            });
        }
        
        // Check for RAG capabilities
        if semantic.selfware_index.files.contains_key("src/cognitive/rag.rs") {
            capabilities.push(Capability {
                name: "Retrieval-Augmented Generation".to_string(),
                description: "Semantic search over codebase".to_string(),
                implementing_modules: vec!["src/cognitive/rag.rs".to_string()],
                confidence: 0.9,
            });
        }
        
        // Check for self-improvement
        if semantic.selfware_index.files.contains_key("src/cognitive/self_improvement.rs") {
            capabilities.push(Capability {
                name: "Self-Improvement".to_string(),
                description: "Analyze and improve own code".to_string(),
                implementing_modules: vec!["src/cognitive/self_improvement.rs".to_string()],
                confidence: 0.85,
            });
        }
        
        capabilities
    }
}

/// Context for self-improvement tasks
pub struct SelfImprovementContext {
    pub relevant_code: CodeContext,
    pub self_model: String,
    pub recent_modifications: String,
    pub architecture: String,
    pub goal: String,
}

impl SelfImprovementContext {
    /// Format as complete prompt context
    pub fn to_prompt(&self) -> String {
        format!(
            r#"# Self-Improvement Task

## Goal
{}

## Architecture Overview
{}

## Self-Model
{}

## Recent Modifications
{}

## Relevant Code
{}
"#,
            self.goal,
            self.architecture,
            self.self_model,
            self.recent_modifications,
            self.format_code_context()
        )
    }
    
    fn format_code_context(&self) -> String {
        self.relevant_code.files.iter()
            .map(|f| format!("### {}\n{}\n", f.path, f.content))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
```

---

## 8. Memory Compression & Summarization

### 8.1 Hierarchical Compression

```rust
// src/cognitive/compression.rs

/// Hierarchical compression system for memories
pub struct MemoryCompressor {
    /// LLM client for intelligent compression
    llm: Arc<dyn LlmClient>,
    /// Compression strategies by content type
    strategies: HashMap<ContentType, Box<dyn CompressionStrategy>>,
}

/// Content types with specialized compression
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentType {
    Conversation,
    Code,
    Episode,
    Knowledge,
    Error,
    ToolResult,
}

/// Compression strategy trait
pub trait CompressionStrategy: Send + Sync {
    /// Compress content to target token count
    fn compress(&self, content: &str, target_tokens: usize) -> Result<String>;
    
    /// Estimate compression ratio
    fn estimate_ratio(&self, content: &str) -> f32;
}

/// Hierarchical compression implementation
pub struct HierarchicalCompression {
    llm: Arc<dyn LlmClient>,
}

impl CompressionStrategy for HierarchicalCompression {
    fn compress(&self, content: &str, target_tokens: usize) -> Result<String> {
        let current_tokens = estimate_tokens_with_overhead(content, 0);
        
        if current_tokens <= target_tokens {
            return Ok(content.to_string());
        }
        
        // Multi-level compression
        let ratio = current_tokens as f32 / target_tokens as f32;
        
        if ratio < 2.0 {
            // Light compression: remove redundant text
            self.light_compress(content, target_tokens)
        } else if ratio < 5.0 {
            // Medium compression: extractive summarization
            self.medium_compress(content, target_tokens)
        } else if ratio < 10.0 {
            // Heavy compression: abstractive summarization
            self.heavy_compress(content, target_tokens)
        } else {
            // Extreme compression: key points only
            self.extreme_compress(content, target_tokens)
        }
    }
    
    fn estimate_ratio(&self, content: &str) -> f32 {
        // Estimate based on content characteristics
        let code_density = content.chars().filter(|c| *c == '{' || *c == '}').count() as f32 
            / content.len().max(1) as f32;
        
        if code_density > 0.05 {
            3.0 // Code compresses well
        } else {
            2.0 // Text compresses moderately
        }
    }
}

impl HierarchicalCompression {
    fn light_compress(&self, content: &str, target: usize) -> Result<String> {
        // Remove extra whitespace, comments, etc.
        let mut compressed = content
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        
        // If still too long, truncate
        let tokens = estimate_tokens_with_overhead(&compressed, 0);
        if tokens > target {
            compressed = self.truncate_to_tokens(&compressed, target);
        }
        
        Ok(compressed)
    }
    
    fn medium_compress(&self, content: &str, target: usize) -> Result<String> {
        // Use extractive summarization: keep key sentences
        let sentences: Vec<_> = content.split('.').collect();
        let mut result = String::new();
        
        for sentence in sentences {
            let candidate = format!("{}.{}", result, sentence);
            if estimate_tokens_with_overhead(&candidate, 0) <= target {
                result = candidate;
            } else {
                break;
            }
        }
        
        Ok(result)
    }
    
    fn heavy_compress(&self, content: &str, target: usize) -> Result<String> {
        // Use LLM for abstractive summarization
        let prompt = format!(
            r#"Summarize the following content concisely, preserving all key information. 
Target length: ~{} tokens.

Content:
{}

Summary:"#,
            target, content
        );
        
        // This would call the LLM
        // For now, return medium compression
        self.medium_compress(content, target)
    }
    
    fn extreme_compress(&self, content: &str, target: usize) -> Result<String> {
        // Extract only key points
        let prompt = format!(
            r#"Extract only the most essential key points from this content.
Limit to ~{} tokens. Use bullet points.

Content:
{}

Key Points:"#,
            target, content
        );
        
        // Would call LLM
        // For now, return first portion
        self.truncate_to_tokens(content, target)
    }
    
    fn truncate_to_tokens(&self, content: &str, target: usize) -> String {
        // Rough character-to-token estimate
        let chars_per_token = 4;
        let target_chars = target * chars_per_token;
        
        if content.len() <= target_chars {
            content.to_string()
        } else {
            format!("{}...[truncated]", &content[..target_chars])
        }
    }
}

/// Episodic memory compression with importance preservation
pub struct EpisodicCompressor {
    base: HierarchicalCompression,
}

impl EpisodicCompressor {
    /// Compress episodes while preserving important details
    pub fn compress_episodes(
        &self,
        episodes: &[Episode],
        target_tokens: usize,
    ) -> Result<Vec<CompressedEpisode>> {
        let total_tokens: usize = episodes.iter().map(|e| e.token_count).sum();
        
        if total_tokens <= target_tokens {
            return Ok(episodes.iter().map(CompressedEpisode::from).collect());
        }
        
        // Sort by importance
        let mut sorted: Vec<_> = episodes.iter().collect();
        sorted.sort_by_key(|e| std::cmp::Reverse(e.importance));
        
        // Allocate tokens proportionally to importance
        let mut result = Vec::new();
        let mut used_tokens = 0usize;
        
        for episode in sorted {
            let importance_factor = (episode.importance as usize + 1) as f32 / 5.0;
            let allocated = (target_tokens as f32 * importance_factor) as usize;
            let actual = allocated.min(episode.token_count);
            
            if used_tokens + actual > target_tokens {
                break;
            }
            
            let compressed = if actual < episode.token_count {
                self.base.compress(&episode.content, actual)?
            } else {
                episode.content.clone()
            };
            
            result.push(CompressedEpisode {
                id: episode.id.clone(),
                episode_type: episode.episode_type.clone(),
                compressed_content: compressed,
                original_tokens: episode.token_count,
                compressed_tokens: actual,
                importance: episode.importance,
            });
            
            used_tokens += actual;
        }
        
        Ok(result)
    }
}

#[derive(Debug, Clone)]
pub struct CompressedEpisode {
    pub id: String,
    pub episode_type: EpisodeType,
    pub compressed_content: String,
    pub original_tokens: usize,
    pub compressed_tokens: usize,
    pub importance: Importance,
}

impl From<&Episode> for CompressedEpisode {
    fn from(e: &Episode) -> Self {
        Self {
            id: e.id.clone(),
            episode_type: e.episode_type.clone(),
            compressed_content: e.content.clone(),
            original_tokens: e.token_count,
            compressed_tokens: e.token_count,
            importance: e.importance,
        }
    }
}
```

---

## 9. Integration with Existing Cognitive Module

### 9.1 Module Integration

```rust
// src/cognitive/mod.rs (enhanced)

pub mod memory_hierarchy;
pub mod working_memory;
pub mod episodic_memory;
pub mod semantic_memory;
pub mod token_budget;
pub mod self_reference;
pub mod compression;

// Re-export existing modules
pub mod episodic;
pub mod knowledge_graph;
pub mod rag;
pub mod self_improvement;

use std::sync::Arc;
use parking_lot::RwLock;

/// Unified cognitive system with 1M context support
pub struct CognitiveSystem {
    /// Hierarchical memory manager
    pub memory: Arc<RwLock<HierarchicalMemory>>,
    /// Self-reference system
    pub self_ref: Arc<RwLock<SelfReferenceSystem>>,
    /// Token budget allocator
    pub budget: Arc<RwLock<TokenBudgetAllocator>>,
    /// Memory compressor
    pub compressor: Arc<MemoryCompressor>,
    /// Original RAG system (integrated)
    pub rag: Arc<RwLock<RagSystem>>,
    /// Knowledge graph (enhanced)
    pub knowledge: Arc<RwLock<KnowledgeGraph>>,
}

impl CognitiveSystem {
    pub async fn new(config: &Config) -> Result<Self> {
        let embedding = Arc::new(create_embedding_backend(config));
        
        // Create hierarchical memory
        let budget = TokenBudget::default();
        let memory = Arc::new(RwLock::new(
            HierarchicalMemory::new(budget, embedding.clone()).await?
        ));
        
        // Create self-reference system
        let self_ref = Arc::new(RwLock::new(
            SelfReferenceSystem::new(memory.read().semantic.clone())
        ));
        
        // Initialize self-model
        self_ref.write().initialize_self_model().await?;
        
        // Create budget allocator
        let budget_allocator = Arc::new(RwLock::new(
            TokenBudgetAllocator::new(TOTAL_CONTEXT_TOKENS, TaskType::Conversation)
        ));
        
        // Create compressor
        let compressor = Arc::new(MemoryCompressor::new(
            create_llm_client(config)
        ));
        
        // Integrate existing RAG
        let rag = Arc::new(RwLock::new(RagSystem::new(config)?));
        
        // Create enhanced knowledge graph
        let knowledge = Arc::new(RwLock::new(KnowledgeGraph::new()));
        
        Ok(Self {
            memory,
            self_ref,
            budget: budget_allocator,
            compressor,
            rag,
            knowledge,
        })
    }
    
    /// Build complete context for LLM
    pub async fn build_context(&self, query: &str) -> Result<LlmContext> {
        let budget = self.budget.read();
        let allocation = budget.get_allocation();
        
        // Get working memory context
        let working = {
            let memory = self.memory.read();
            memory.working.get_context()
        };
        
        // Get episodic context
        let episodic = {
            let memory = self.memory.read();
            memory.episodic.retrieve_relevant(
                query,
                10,
                Importance::Normal
            ).await?
        };
        
        // Get semantic/code context
        let semantic = {
            let memory = self.memory.read();
            memory.semantic.retrieve_code_context(
                query,
                allocation.semantic,
                true
            ).await?
        };
        
        // Check if we need self-reference
        let self_context = if self.is_self_improvement_query(query) {
            let self_ref = self.self_ref.read();
            Some(self_ref.get_self_improvement_context(
                query,
                allocation.semantic / 2
            ).await?)
        } else {
            None
        };
        
        Ok(LlmContext {
            working,
            episodic,
            semantic,
            self_context,
        })
    }
    
    /// Check if query is about self-improvement
    fn is_self_improvement_query(&self, query: &str) -> bool {
        let keywords = [
            "improve", "refactor", "optimize", "enhance",
            "self", "my code", "myself", "own code",
            "memory system", "cognitive", "architecture",
        ];
        
        let lower = query.to_lowercase();
        keywords.iter().any(|k| lower.contains(k))
    }
    
    /// Record an episode
    pub async fn record_episode(&self, episode: Episode) -> Result<()> {
        let mut memory = self.memory.write();
        memory.episodic.record(episode).await?;
        Ok(())
    }
    
    /// Update token budget based on usage
    pub fn adapt_budget(&self) {
        let mut budget = self.budget.write();
        
        let memory = self.memory.read();
        budget.record_usage(
            memory.usage.working_tokens,
            memory.usage.episodic_tokens,
            memory.usage.semantic_tokens,
        );
        
        budget.adapt();
    }
}

/// Complete context for LLM prompt
pub struct LlmContext {
    pub working: WorkingContext,
    pub episodic: Vec<Episode>,
    pub semantic: CodeContext,
    pub self_context: Option<SelfImprovementContext>,
}

impl LlmContext {
    /// Format as complete prompt
    pub fn to_prompt(&self) -> String {
        let mut prompt = String::new();
        
        // Working memory (conversation)
        prompt.push_str("## Conversation\n");
        for msg in &self.working.messages {
            prompt.push_str(&format!("{}: {}\n", msg.role, msg.content));
        }
        prompt.push_str("\n");
        
        // Episodic memory
        if !self.episodic.is_empty() {
            prompt.push_str("## Relevant Past Experiences\n");
            for ep in &self.episodic {
                prompt.push_str(&format!("- [{}] {}\n", 
                    ep.episode_type.as_str(), 
                    ep.content.chars().take(200).collect::<String>()
                ));
            }
            prompt.push_str("\n");
        }
        
        // Semantic/code context
        if !self.semantic.files.is_empty() {
            prompt.push_str("## Relevant Code\n");
            for file in &self.semantic.files {
                prompt.push_str(&format!("### {}\n{}\n\n", file.path, file.content));
            }
        }
        
        // Self-improvement context
        if let Some(self_ctx) = &self.self_context {
            prompt.push_str(&self_ctx.to_prompt());
        }
        
        prompt
    }
    
    /// Estimate total tokens
    pub fn estimate_tokens(&self) -> usize {
        let working: usize = self.working.messages.iter()
            .map(|m| estimate_tokens_with_overhead(&m.content, 50))
            .sum();
        
        let episodic: usize = self.episodic.iter()
            .map(|e| e.token_count)
            .sum();
        
        let semantic: usize = self.semantic.files.iter()
            .map(|f| estimate_tokens_with_overhead(&f.content, 0))
            .sum();
        
        working + episodic + semantic
    }
}
```

---

## 10. Configuration

### 10.1 Memory Configuration

```toml
# selfware.toml - Memory configuration section

[memory]
# Total context window size (1M for Qwen3 Coder)
total_context_tokens = 1_000_000

# Token budget allocation (percentages)
[memory.budget]
working_memory_percent = 10    # 100K tokens
episodic_memory_percent = 20   # 200K tokens
semantic_memory_percent = 70   # 700K tokens
response_reserve_percent = 10  # 100K buffer

# Working memory settings
[memory.working]
max_messages = 100
importance_threshold = 0.5
active_code_files = 10

# Episodic memory settings
[memory.episodic]
max_episodes = 10_000
compression_threshold = 0.8
tier_retention = { critical = "forever", high = "30d", normal = "7d", low = "1d" }

# Semantic memory settings
[memory.semantic]
index_selfware = true
max_file_tokens = 50_000
chunk_size = 5_000
embedding_model = "text-embedding-3-large"

# Self-reference settings
[memory.self_reference]
enable_self_model = true
cache_size = 100
track_modifications = true

# Compression settings
[memory.compression]
enable_auto_compression = true
compression_ratio_threshold = 2.0
preserve_code_structure = true
```

---

## 11. Performance Considerations

### 11.1 Optimization Strategies

| Strategy | Implementation | Benefit |
|----------|---------------|---------|
| Lazy Loading | Load code chunks on demand | Reduces memory footprint |
| Embedding Cache | Cache frequently used embeddings | Faster retrieval |
| Tiered Storage | Hot/cold data separation | Efficient eviction |
| Incremental Indexing | Only index changed files | Faster updates |
| Parallel Processing | Index files in parallel | Faster initial load |
| Compression | Compress old episodes | More memories retained |

---

## 12. Migration Path

### 12.1 From Current to New Architecture

```rust
// Migration utilities

pub struct MemoryMigration;

impl MemoryMigration {
    /// Migrate from old AgentMemory to new HierarchicalMemory
    pub async fn migrate_agent_memory(
        old: AgentMemory,
        config: &Config,
    ) -> Result<HierarchicalMemory> {
        let embedding = Arc::new(create_embedding_backend(config));
        let mut new = HierarchicalMemory::new(TokenBudget::default(), embedding).await?;
        
        // Migrate entries to episodic memory
        for entry in old.entries {
            let episode = Episode {
                id: generate_id(),
                episode_type: EpisodeType::Conversation,
                content: format!("[{}] {}", entry.role, entry.content),
                token_count: entry.token_estimate,
                importance: Importance::Normal,
                timestamp: parse_timestamp(&entry.timestamp)?,
                embedding_id: String::new(),
                related_episodes: Vec::new(),
                insights: Vec::new(),
                is_summarized: false,
                original_id: None,
            };
            
            new.episodic.record(episode).await?;
        }
        
        Ok(new)
    }
    
    /// Migrate RAG to semantic memory
    pub async fn migrate_rag(
        rag: RagSystem,
        semantic: &mut SemanticMemory,
    ) -> Result<()> {
        // Transfer indexed files
        for (path, doc) in rag.documents {
            semantic.index_file(&path, true).await?;
        }
        
        Ok(())
    }
}
```

---

## Summary

This memory architecture provides:

1. **Hierarchical Organization**: Three-layer memory (working/episodic/semantic) maximizes 1M context utilization
2. **Token Budgeting**: Dynamic allocation adapts to task types
3. **Self-Reference**: Agent can read and understand its own source code
4. **Efficient Retrieval**: Vector-based semantic search with importance weighting
5. **Smart Compression**: Multi-level compression preserves important information
6. **Integration**: Seamlessly extends existing cognitive module

The design enables Selfware to:
- Load its entire codebase into context for self-improvement
- Maintain multi-day execution with minimal information loss
- Efficiently retrieve relevant code and experiences
- Compress and summarize intelligently
- Adapt memory allocation based on task requirements
