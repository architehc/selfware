//! Memory Hierarchy System
//!
//! This module provides a hierarchical memory system with working, short-term,
//! long-term, and archive tiers for storing and retrieving information.

pub mod long_term;
pub mod short_term;
pub mod types;

pub use long_term::{ArchiveMemory, LongTermMemory};
pub use short_term::{ShortTermMemory, WorkingMemory};
pub use types::{
    ActiveCodeCollection, ActiveCodeContext, ChangeType, CodeContent, CodeContext, CodeEdit,
    CodeModification, ConsolidationResult, Episode, EpisodeType, FileContext, FileContextEntry,
    Importance, MemoryConfig, MemoryEntry, MemoryIndex, MemoryMetrics, MemoryQuery, MemoryStats,
    MemoryTier, MemoryUsage, SelfImprovementContext, SelfModel, SymbolContext, TaskContext,
    TierTransition, TokenBudget, WorkingContext, TOTAL_CONTEXT_TOKENS,
};

use std::sync::Arc;
use tokio::sync::RwLock;

/// Main hierarchical memory system
pub struct HierarchicalMemory {
    config: Arc<RwLock<MemoryConfig>>,
    index: Arc<MemoryIndex>,
    pub working: WorkingMemory,
    pub short_term: ShortTermMemory,
    pub long_term: LongTermMemory,
    archive: ArchiveMemory,
    stats: Arc<RwLock<MemoryStats>>,
    pub episodic: EpisodicMemory,
    pub semantic: Arc<RwLock<SemanticMemory>>,
    pub budget: TokenBudget,
    pub usage: MemoryUsage,
}

/// Episodic memory for storing experiences
#[derive(Clone)]
pub struct EpisodicMemory {
    episodes: Arc<RwLock<Vec<Episode>>>,
}

impl EpisodicMemory {
    pub fn new() -> Self {
        Self {
            episodes: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn retrieve_relevant(
        &self,
        _query: &str,
        limit: usize,
        min_importance: Importance,
    ) -> anyhow::Result<Vec<Episode>> {
        let episodes = self.episodes.read().await;
        let mut result: Vec<_> = episodes
            .iter()
            .filter(|e| e.importance >= min_importance)
            .cloned()
            .collect();
        result.truncate(limit);
        Ok(result)
    }

    pub async fn add(&self, episode: Episode) {
        let mut episodes = self.episodes.write().await;
        episodes.push(episode);
    }
}

impl Default for EpisodicMemory {
    fn default() -> Self {
        Self::new()
    }
}

/// Semantic memory for code context
pub struct SemanticMemory {
    code_context: Arc<RwLock<CodeContext>>,
}

impl SemanticMemory {
    pub fn new() -> Self {
        Self {
            code_context: Arc::new(RwLock::new(CodeContext::new())),
        }
    }

    pub async fn retrieve_code_context(
        &self,
        _query: &str,
        max_tokens: usize,
        _include_symbols: bool,
    ) -> anyhow::Result<CodeContext> {
        let ctx = self.code_context.read().await;
        Ok(CodeContext {
            files: ctx.files.clone(),
            symbols: ctx.symbols.clone(),
            total_tokens: ctx.total_tokens.min(max_tokens),
        })
    }

    /// Index file paths from a codebase directory into semantic memory.
    ///
    /// Walks the directory tree and records each source file as a `FileContext`
    /// entry. Only indexes paths (no full content) to keep memory footprint low.
    ///
    /// TODO: Add content summarization per file once an LLM summarizer is wired in.
    /// TODO: Extract symbol-level context (functions, structs) for richer queries.
    pub async fn index_codebase(&self, path: &std::path::Path) -> anyhow::Result<()> {
        use walkdir::WalkDir;

        let source_extensions = ["rs", "toml", "yaml", "yml", "json", "md", "py", "ts", "js"];
        let mut files = Vec::new();
        let mut total_tokens = 0usize;

        for entry in WalkDir::new(path)
            .max_depth(8)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                // Skip hidden dirs, target, node_modules
                !name.starts_with('.') && name != "target" && name != "node_modules"
            })
        {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let ext = entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            if !source_extensions.contains(&ext) {
                continue;
            }

            let rel_path = entry
                .path()
                .strip_prefix(path)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .to_string();

            // Estimate ~2 tokens per path component as lightweight index
            let path_tokens = rel_path.len() / 4 + 1;
            total_tokens += path_tokens;

            let lang = match ext {
                "rs" => "rust",
                "py" => "python",
                "ts" | "js" => "javascript",
                "toml" => "toml",
                "yaml" | "yml" => "yaml",
                "json" => "json",
                "md" => "markdown",
                _ => "unknown",
            };

            files.push(types::FileContext {
                path: rel_path,
                content: String::new(), // Path-only index; content loaded on demand
                language: lang.to_string(),
                estimated_tokens: path_tokens,
                relevance_score: 0.5, // Neutral until queried
            });
        }

        let mut ctx = self.code_context.write().await;
        ctx.files = files;
        ctx.total_tokens = total_tokens;
        tracing::info!(
            file_count = ctx.files.len(),
            total_tokens,
            "Indexed codebase into semantic memory"
        );

        Ok(())
    }
}

impl Default for SemanticMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for SemanticMemory {
    fn clone(&self) -> Self {
        Self::new()
    }
}

impl HierarchicalMemory {
    /// Create a new hierarchical memory system
    pub async fn new(
        config: MemoryConfig,
        _embedding: Arc<crate::vector_store::EmbeddingBackend>,
    ) -> anyhow::Result<Self> {
        let config = Arc::new(RwLock::new(config));
        let index = Arc::new(MemoryIndex::new());
        let stats = Arc::new(RwLock::new(MemoryStats::default()));

        let cfg = config.read().await;
        let working = WorkingMemory::new(cfg.working_capacity, index.clone());
        let short_term = ShortTermMemory::with_config(&cfg, index.clone());
        let long_term = LongTermMemory::with_config(&cfg, index.clone());
        drop(cfg);

        let archive = ArchiveMemory::new();
        let episodic = EpisodicMemory::new();
        let semantic = Arc::new(RwLock::new(SemanticMemory::new()));
        let budget = TokenBudget::default();
        let usage = MemoryUsage::default();

        Ok(Self {
            config,
            index,
            working,
            short_term,
            long_term,
            archive,
            stats,
            episodic,
            semantic,
            budget,
            usage,
        })
    }

    /// Create with default configuration
    pub async fn default() -> anyhow::Result<Self> {
        use crate::vector_store::{EmbeddingBackend, MockEmbeddingProvider};
        Self::new(
            MemoryConfig::default(),
            Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default())),
        )
        .await
    }

    /// Add a message to working memory, triggering compression if over budget
    pub fn add_message(&mut self, message: crate::api::types::Message, importance: f32) {
        let tokens = crate::token_count::estimate_content_tokens(message.content.text());
        self.working.add_message(message, tokens);
        self.usage.working_tokens += tokens;
        self.usage.total_used += tokens;

        // Store as a memory entry with the given importance
        let id = self.index.next_id();
        let entry = MemoryEntry::new(id, format!("[msg] {tokens} tokens"), MemoryTier::Working)
            .with_importance(importance);
        // Fire-and-forget store (working memory is append-heavy)
        let working = self.working.clone();
        tokio::spawn(async move {
            let _ = working.store(entry).await;
        });
    }

    /// Record an episode
    pub async fn record_episode(&mut self, episode: Episode) -> anyhow::Result<()> {
        self.episodic.add(episode).await;
        Ok(())
    }

    /// Get memory statistics
    pub async fn get_stats(&self) -> MemoryStats {
        self.stats.read().await.clone()
    }

    /// Compress memory if over budget.
    ///
    /// Checks each layer against its token budget and evicts the least-important,
    /// oldest-accessed entries until the layer fits. Returns `true` if any
    /// evictions occurred.
    pub async fn compress_if_needed(&mut self) -> anyhow::Result<bool> {
        let mut compressed = false;

        // --- Working memory compression ---
        if self.usage.working_tokens > self.budget.working_memory {
            let mut entries = self.working.entries().await;
            // Sort by importance ASC, then accessed_at ASC (evict least important & oldest first)
            entries.sort_by(|a, b| {
                a.importance
                    .partial_cmp(&b.importance)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.accessed_at.cmp(&b.accessed_at))
            });

            let mut tokens_to_free = self
                .usage
                .working_tokens
                .saturating_sub(self.budget.working_memory);
            for entry in &entries {
                if tokens_to_free == 0 {
                    break;
                }
                let entry_tokens = crate::token_count::estimate_content_tokens(&entry.content);
                // Demote to short-term instead of dropping
                if let Some(mut removed) = self.working.remove(entry.id).await {
                    removed.tier = MemoryTier::ShortTerm;
                    let _ = self.short_term.store(removed).await;
                    let freed = entry_tokens.min(tokens_to_free);
                    tokens_to_free = tokens_to_free.saturating_sub(freed);
                    self.usage.working_tokens =
                        self.usage.working_tokens.saturating_sub(entry_tokens);
                    compressed = true;
                }
            }
        }

        // --- Short-term memory compression (evict to long-term) ---
        let st_count = self.short_term.count().await;
        let config = self.config.read().await;
        let st_cap = config.short_term_capacity;
        drop(config);

        if st_count > st_cap {
            let mut entries = self.short_term.entries().await;
            entries.sort_by(|a, b| {
                a.importance
                    .partial_cmp(&b.importance)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.accessed_at.cmp(&b.accessed_at))
            });

            let to_evict = st_count.saturating_sub(st_cap);
            for entry in entries.iter().take(to_evict) {
                if let Some(mut removed) = self.short_term.remove(entry.id).await {
                    removed.tier = MemoryTier::LongTerm;
                    let _ = self.long_term.store(removed).await;
                    compressed = true;
                }
            }
        }

        // Recalculate total usage
        self.usage.total_used = self.usage.working_tokens
            + self.usage.episodic_tokens
            + self.usage.semantic_tokens
            + self.usage.self_tokens;

        Ok(compressed)
    }

    /// Check if memory is within budget
    pub fn is_within_budget(&self) -> bool {
        self.usage.total_used
            < self.budget.working_memory + self.budget.episodic_memory + self.budget.semantic_memory
    }

    /// Initialize selfware codebase index
    pub async fn initialize_selfware_index(
        &mut self,
        _path: &std::path::Path,
    ) -> anyhow::Result<()> {
        let semantic = self.semantic.write().await;
        semantic.index_codebase(_path).await?;
        Ok(())
    }

    /// Store content in the specified tier
    pub async fn store(&self, content: impl Into<String>, tier: MemoryTier) -> anyhow::Result<u64> {
        let id = self.index.next_id();
        let entry = MemoryEntry::new(id, content, tier);

        {
            let mut stats = self.stats.write().await;
            stats.total_inserts += 1;
        }

        match tier {
            MemoryTier::Working => self.working.store(entry).await,
            MemoryTier::ShortTerm => self.short_term.store(entry).await,
            MemoryTier::LongTerm => self.long_term.store(entry).await,
            MemoryTier::Archive => self.archive.store(entry).await,
        }
    }

    /// Retrieve an entry by ID (searches all tiers)
    pub async fn retrieve(&self, id: u64) -> Option<MemoryEntry> {
        if let Some(entry) = self.working.retrieve(id).await {
            return Some(entry);
        }
        if let Some(entry) = self.short_term.retrieve(id).await {
            return Some(entry);
        }
        if let Some(entry) = self.long_term.retrieve(id).await {
            return Some(entry);
        }
        self.archive.retrieve(id).await
    }

    /// Query across all tiers
    pub async fn query(&self, query: MemoryQuery) -> Vec<MemoryEntry> {
        {
            let mut stats = self.stats.write().await;
            stats.total_queries += 1;
        }

        let mut results = Vec::new();

        if query.tier.is_none() || query.tier == Some(MemoryTier::Working) {
            results.extend(self.working.query(&query).await);
        }
        if query.tier.is_none() || query.tier == Some(MemoryTier::ShortTerm) {
            results.extend(self.short_term.query(&query).await);
        }
        if query.tier.is_none() || query.tier == Some(MemoryTier::LongTerm) {
            results.extend(self.long_term.query(&query).await);
        }

        results.sort_by(|a, b| {
            let tier_order = |t: MemoryTier| match t {
                MemoryTier::Working => 0,
                MemoryTier::ShortTerm => 1,
                MemoryTier::LongTerm => 2,
                MemoryTier::Archive => 3,
            };
            tier_order(a.tier).cmp(&tier_order(b.tier)).then_with(|| {
                b.importance
                    .partial_cmp(&a.importance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        });

        if let Some(limit) = query.limit {
            results.truncate(limit);
        }

        results
    }

    /// Promote an entry to a higher tier
    pub async fn promote(&self, id: u64) -> anyhow::Result<()> {
        if self.working.retrieve(id).await.is_some() {
            return Ok(());
        }

        if let Some(mut entry) = self.short_term.retrieve(id).await {
            self.short_term.remove(id).await;
            entry.tier = MemoryTier::Working;
            self.working.store(entry).await?;
            {
                let mut stats = self.stats.write().await;
                stats.total_promotions += 1;
            }
            return Ok(());
        }

        if let Some(mut entry) = self.long_term.retrieve(id).await {
            self.long_term.remove(id).await;
            entry.tier = MemoryTier::ShortTerm;
            self.short_term.store(entry).await?;
            {
                let mut stats = self.stats.write().await;
                stats.total_promotions += 1;
            }
            return Ok(());
        }

        Err(anyhow::anyhow!("Entry not found"))
    }

    /// Demote an entry to a lower tier
    pub async fn demote(&self, id: u64) -> anyhow::Result<()> {
        if let Some(mut entry) = self.working.retrieve(id).await {
            self.working.remove(id).await;
            entry.tier = MemoryTier::ShortTerm;
            self.short_term.store(entry).await?;
            {
                let mut stats = self.stats.write().await;
                stats.total_demotions += 1;
            }
            return Ok(());
        }

        if let Some(mut entry) = self.short_term.retrieve(id).await {
            self.short_term.remove(id).await;
            entry.tier = MemoryTier::LongTerm;
            self.long_term.store(entry).await?;
            {
                let mut stats = self.stats.write().await;
                stats.total_demotions += 1;
            }
            return Ok(());
        }

        Err(anyhow::anyhow!("Entry not found"))
    }

    /// Get current statistics
    pub async fn stats(&self) -> MemoryStats {
        let mut stats = self.stats.read().await.clone();
        stats.working_count = self.working.count().await;
        stats.short_term_count = self.short_term.count().await;
        stats.long_term_count = self.long_term.count().await;
        stats
    }

    /// Get the memory index
    pub fn index(&self) -> &MemoryIndex {
        &self.index
    }

    /// Access working memory directly
    pub fn working(&self) -> &WorkingMemory {
        &self.working
    }

    /// Access short-term memory directly
    pub fn short_term(&self) -> &ShortTermMemory {
        &self.short_term
    }

    /// Access long-term memory directly
    pub fn long_term(&self) -> &LongTermMemory {
        &self.long_term
    }

    /// Consolidate long-term memories
    pub async fn consolidate(&self) -> ConsolidationResult {
        self.long_term.consolidate().await
    }
}

impl Clone for HierarchicalMemory {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            index: self.index.clone(),
            working: self.working.clone(),
            short_term: self.short_term.clone(),
            long_term: self.long_term.clone(),
            archive: self.archive.clone(),
            stats: self.stats.clone(),
            episodic: self.episodic.clone(),
            semantic: self.semantic.clone(),
            budget: self.budget.clone(),
            usage: self.usage.clone(),
        }
    }
}

/// Re-export commonly used types
pub mod prelude {
    pub use super::{HierarchicalMemory, MemoryConfig, MemoryEntry, MemoryQuery, MemoryTier};
}
