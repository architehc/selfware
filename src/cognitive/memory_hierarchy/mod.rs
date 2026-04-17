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
            if let Err(e) = working.store(entry).await {
                tracing::warn!(error = %e, "Failed to store entry in working memory");
            }
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
                    if let Err(e) = self.short_term.store(removed).await {
                        tracing::warn!(error = %e, entry_id = entry.id, "Failed to demote entry to short-term memory");
                        continue;
                    }
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
                    if let Err(e) = self.long_term.store(removed).await {
                        tracing::warn!(error = %e, entry_id = entry.id, "Failed to demote entry to long-term memory");
                        continue;
                    }
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

#[cfg(test)]
mod tests;

#[cfg(test)]
mod inline_tests {
    use super::*;

    // ── EpisodicMemory ────────────────────────────────────────────

    #[test]
    fn test_episodic_memory_new() {
        let em = EpisodicMemory::new();
        // Just verify construction succeeds (Default impl delegates to new)
        let em2 = EpisodicMemory::default();
        drop(em);
        drop(em2);
    }

    #[tokio::test]
    async fn test_episodic_memory_add_and_retrieve() {
        let em = EpisodicMemory::new();
        let ep = Episode::new("ep-1", EpisodeType::Action, "did something");
        em.add(ep).await;

        let results = em
            .retrieve_relevant("query", 10, Importance::Low)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "ep-1");
    }

    #[tokio::test]
    async fn test_episodic_memory_filter_by_importance() {
        let em = EpisodicMemory::new();

        let low = Episode::new("ep-low", EpisodeType::Thought, "low importance")
            .with_importance(Importance::Low);
        let high = Episode::new("ep-high", EpisodeType::Success, "high importance")
            .with_importance(Importance::High);

        em.add(low).await;
        em.add(high).await;

        let results = em
            .retrieve_relevant("q", 10, Importance::High)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "ep-high");
    }

    #[tokio::test]
    async fn test_episodic_memory_truncate_limit() {
        let em = EpisodicMemory::new();
        for i in 0..5 {
            em.add(Episode::new(format!("ep-{i}"), EpisodeType::Action, "x"))
                .await;
        }

        let results = em.retrieve_relevant("q", 2, Importance::Low).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_episodic_memory_clone() {
        let em = EpisodicMemory::new();
        let em2 = em.clone();
        drop(em2);
    }

    // ── SemanticMemory ───────��────────────────────────────────────

    #[test]
    fn test_semantic_memory_new() {
        let sm = SemanticMemory::new();
        let sm2 = SemanticMemory::default();
        drop(sm);
        drop(sm2);
    }

    #[test]
    fn test_semantic_memory_clone() {
        let sm = SemanticMemory::new();
        let _sm2 = sm.clone();
    }

    #[tokio::test]
    async fn test_semantic_memory_retrieve_code_context_empty() {
        let sm = SemanticMemory::new();
        let ctx = sm
            .retrieve_code_context("anything", 1000, true)
            .await
            .unwrap();
        assert!(ctx.files.is_empty());
        assert!(ctx.symbols.is_empty());
        assert_eq!(ctx.total_tokens, 0);
    }

    /// Helper: create a non-dot-prefixed subdirectory inside a tempdir.
    /// The production `index_codebase` uses walkdir with `filter_entry`
    /// that skips directory names starting with '.', and Rust's tempfile
    /// crate creates dirs like `.tmpXXXXXX`, so we nest under a regular name.
    fn test_dir(tmp: &tempfile::TempDir) -> std::path::PathBuf {
        let dir = tmp.path().join("project");
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn test_semantic_memory_index_codebase() {
        let tmp = tempfile::tempdir().unwrap();
        let base = test_dir(&tmp);
        std::fs::write(base.join("main.rs"), "fn main() {}").unwrap();
        std::fs::write(base.join("lib.py"), "def hello(): pass").unwrap();
        std::fs::write(base.join("notes.md"), "# Notes").unwrap();
        // A file with unsupported extension should be skipped
        std::fs::write(base.join("data.bin"), "binary").unwrap();

        let sm = SemanticMemory::new();
        sm.index_codebase(&base).await.unwrap();

        let ctx = sm.retrieve_code_context("q", 100_000, false).await.unwrap();
        // Should have indexed main.rs, lib.py, notes.md but not data.bin
        assert_eq!(ctx.files.len(), 3);
    }

    #[tokio::test]
    async fn test_semantic_memory_index_skips_hidden_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let base = test_dir(&tmp);
        let hidden = base.join(".hidden");
        std::fs::create_dir_all(&hidden).unwrap();
        std::fs::write(hidden.join("secret.rs"), "fn secret() {}").unwrap();
        // Also put a normal file
        std::fs::write(base.join("visible.rs"), "fn visible() {}").unwrap();

        let sm = SemanticMemory::new();
        sm.index_codebase(&base).await.unwrap();

        let ctx = sm.retrieve_code_context("q", 100_000, false).await.unwrap();
        assert_eq!(ctx.files.len(), 1);
        assert_eq!(ctx.files[0].path, "visible.rs");
    }

    #[tokio::test]
    async fn test_semantic_memory_language_detection() {
        let tmp = tempfile::tempdir().unwrap();
        let base = test_dir(&tmp);
        std::fs::write(base.join("a.rs"), "x").unwrap();
        std::fs::write(base.join("b.py"), "x").unwrap();
        std::fs::write(base.join("c.ts"), "x").unwrap();
        std::fs::write(base.join("d.js"), "x").unwrap();
        std::fs::write(base.join("e.toml"), "x").unwrap();
        std::fs::write(base.join("f.yaml"), "x").unwrap();
        std::fs::write(base.join("g.yml"), "x").unwrap();
        std::fs::write(base.join("h.json"), "x").unwrap();
        std::fs::write(base.join("i.md"), "x").unwrap();

        let sm = SemanticMemory::new();
        sm.index_codebase(&base).await.unwrap();

        let ctx = sm.retrieve_code_context("q", 100_000, false).await.unwrap();

        let langs: std::collections::HashSet<_> =
            ctx.files.iter().map(|f| f.language.as_str()).collect();
        assert!(langs.contains("rust"));
        assert!(langs.contains("python"));
        assert!(langs.contains("javascript")); // ts and js both map to javascript
        assert!(langs.contains("toml"));
        assert!(langs.contains("yaml")); // yaml and yml
        assert!(langs.contains("json"));
        assert!(langs.contains("markdown"));
    }

    #[tokio::test]
    async fn test_semantic_memory_max_tokens_capped() {
        let tmp = tempfile::tempdir().unwrap();
        let base = test_dir(&tmp);
        std::fs::write(base.join("main.rs"), "fn main() {}").unwrap();

        let sm = SemanticMemory::new();
        sm.index_codebase(&base).await.unwrap();

        // Retrieve with a very small max_tokens
        let ctx = sm.retrieve_code_context("q", 1, false).await.unwrap();
        assert!(ctx.total_tokens <= 1);
    }

    // ── HierarchicalMemory ─────���──────────────────────────────────

    #[tokio::test]
    async fn test_hierarchical_memory_default() {
        let hm = HierarchicalMemory::default().await.unwrap();
        assert!(hm.is_within_budget());
    }

    #[tokio::test]
    async fn test_hierarchical_memory_store_and_retrieve() {
        let hm = HierarchicalMemory::default().await.unwrap();

        let id = hm.store("hello world", MemoryTier::Working).await.unwrap();
        let entry = hm.retrieve(id).await;
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().content, "hello world");
    }

    #[tokio::test]
    async fn test_hierarchical_memory_store_each_tier() {
        let hm = HierarchicalMemory::default().await.unwrap();

        let w = hm.store("w", MemoryTier::Working).await.unwrap();
        let s = hm.store("s", MemoryTier::ShortTerm).await.unwrap();
        let l = hm.store("l", MemoryTier::LongTerm).await.unwrap();
        let a = hm.store("a", MemoryTier::Archive).await.unwrap();

        assert!(hm.retrieve(w).await.is_some());
        assert!(hm.retrieve(s).await.is_some());
        assert!(hm.retrieve(l).await.is_some());
        assert!(hm.retrieve(a).await.is_some());
    }

    #[tokio::test]
    async fn test_hierarchical_memory_retrieve_missing() {
        let hm = HierarchicalMemory::default().await.unwrap();
        assert!(hm.retrieve(999999).await.is_none());
    }

    #[tokio::test]
    async fn test_hierarchical_memory_query_all_tiers() {
        let hm = HierarchicalMemory::default().await.unwrap();
        hm.store("alpha data", MemoryTier::Working).await.unwrap();
        hm.store("alpha info", MemoryTier::ShortTerm).await.unwrap();
        hm.store("alpha note", MemoryTier::LongTerm).await.unwrap();

        let q = MemoryQuery::new("alpha").with_limit(10);
        let results = hm.query(q).await;
        // All three entries should be found across tiers
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_hierarchical_memory_query_specific_tier() {
        let hm = HierarchicalMemory::default().await.unwrap();
        hm.store("foo", MemoryTier::Working).await.unwrap();
        hm.store("foo", MemoryTier::ShortTerm).await.unwrap();

        let q = MemoryQuery::new("foo").with_tier(MemoryTier::ShortTerm);
        let results = hm.query(q).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tier, MemoryTier::ShortTerm);
    }

    #[tokio::test]
    async fn test_hierarchical_memory_query_with_limit() {
        let hm = HierarchicalMemory::default().await.unwrap();
        for i in 0..5 {
            hm.store(format!("item {i}"), MemoryTier::Working)
                .await
                .unwrap();
        }

        let q = MemoryQuery::new("item").with_limit(2);
        let results = hm.query(q).await;
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_hierarchical_memory_promote_from_short_term() {
        let hm = HierarchicalMemory::default().await.unwrap();
        let id = hm.store("promote me", MemoryTier::ShortTerm).await.unwrap();

        hm.promote(id).await.unwrap();

        // After promotion, entry should be retrievable via working memory
        let entry = hm.working().retrieve(id).await;
        assert!(entry.is_some());
    }

    #[tokio::test]
    async fn test_hierarchical_memory_promote_from_long_term() {
        let hm = HierarchicalMemory::default().await.unwrap();
        let id = hm.store("promote me", MemoryTier::LongTerm).await.unwrap();

        hm.promote(id).await.unwrap();

        // After promotion from long-term, entry should be in short-term
        let entry = hm.retrieve(id).await;
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().tier, MemoryTier::ShortTerm);
    }

    #[tokio::test]
    async fn test_hierarchical_memory_promote_already_working() {
        let hm = HierarchicalMemory::default().await.unwrap();
        let id = hm.store("already top", MemoryTier::Working).await.unwrap();

        // Should be a no-op, not an error
        hm.promote(id).await.unwrap();
    }

    #[tokio::test]
    async fn test_hierarchical_memory_promote_not_found() {
        let hm = HierarchicalMemory::default().await.unwrap();
        let result = hm.promote(999999).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_hierarchical_memory_demote_from_working() {
        let hm = HierarchicalMemory::default().await.unwrap();
        let id = hm.store("demote me", MemoryTier::Working).await.unwrap();

        hm.demote(id).await.unwrap();

        // After demotion, the entry should be retrievable from short-term
        let entry = hm.short_term().retrieve(id).await;
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().tier, MemoryTier::ShortTerm);
    }

    #[tokio::test]
    async fn test_hierarchical_memory_demote_from_short_term() {
        let hm = HierarchicalMemory::default().await.unwrap();
        let id = hm.store("demote me", MemoryTier::ShortTerm).await.unwrap();

        hm.demote(id).await.unwrap();

        // After demotion, entry should be in long-term
        let entry = hm.long_term().retrieve(id).await;
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().tier, MemoryTier::LongTerm);
    }

    #[tokio::test]
    async fn test_hierarchical_memory_demote_not_found() {
        let hm = HierarchicalMemory::default().await.unwrap();
        let result = hm.demote(999999).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_hierarchical_memory_stats() {
        let hm = HierarchicalMemory::default().await.unwrap();
        hm.store("a", MemoryTier::Working).await.unwrap();
        hm.store("b", MemoryTier::ShortTerm).await.unwrap();

        let stats = hm.stats().await;
        assert_eq!(stats.working_count, 1);
        assert_eq!(stats.short_term_count, 1);
        assert_eq!(stats.total_inserts, 2);
    }

    #[tokio::test]
    async fn test_hierarchical_memory_stats_tracks_queries() {
        let hm = HierarchicalMemory::default().await.unwrap();
        hm.store("x", MemoryTier::Working).await.unwrap();

        let q = MemoryQuery::new("x");
        hm.query(q).await;

        let stats = hm.stats().await;
        assert_eq!(stats.total_queries, 1);
    }

    #[tokio::test]
    async fn test_hierarchical_memory_stats_tracks_promotions() {
        let hm = HierarchicalMemory::default().await.unwrap();
        let id = hm.store("p", MemoryTier::ShortTerm).await.unwrap();
        hm.promote(id).await.unwrap();

        let stats = hm.stats().await;
        assert_eq!(stats.total_promotions, 1);
    }

    #[tokio::test]
    async fn test_hierarchical_memory_stats_tracks_demotions() {
        let hm = HierarchicalMemory::default().await.unwrap();
        let id = hm.store("d", MemoryTier::Working).await.unwrap();
        hm.demote(id).await.unwrap();

        let stats = hm.stats().await;
        assert_eq!(stats.total_demotions, 1);
    }

    #[tokio::test]
    async fn test_hierarchical_memory_get_stats() {
        let hm = HierarchicalMemory::default().await.unwrap();
        let stats = hm.get_stats().await;
        assert_eq!(stats.total_inserts, 0);
    }

    #[tokio::test]
    async fn test_hierarchical_memory_index() {
        let hm = HierarchicalMemory::default().await.unwrap();
        let idx = hm.index();
        // next_id should return incrementing values
        let a = idx.next_id();
        let b = idx.next_id();
        assert!(b > a);
    }

    #[tokio::test]
    async fn test_hierarchical_memory_accessors() {
        let hm = HierarchicalMemory::default().await.unwrap();
        let _ = hm.working();
        let _ = hm.short_term();
        let _ = hm.long_term();
    }

    #[tokio::test]
    async fn test_hierarchical_memory_record_episode() {
        let mut hm = HierarchicalMemory::default().await.unwrap();
        let ep = Episode::new("ep-1", EpisodeType::Learning, "learned something");
        hm.record_episode(ep).await.unwrap();

        let episodes = hm
            .episodic
            .retrieve_relevant("q", 10, Importance::Low)
            .await
            .unwrap();
        assert_eq!(episodes.len(), 1);
    }

    #[tokio::test]
    async fn test_hierarchical_memory_consolidate() {
        let hm = HierarchicalMemory::default().await.unwrap();
        // Store a low-importance entry
        let id = hm.store("unimportant", MemoryTier::LongTerm).await.unwrap();
        // Set importance low by retrieving and checking
        hm.long_term.update(id, |e| e.importance = 0.1).await;

        let result = hm.consolidate().await;
        assert_eq!(result.entries_removed, 1);
    }

    #[tokio::test]
    async fn test_hierarchical_memory_is_within_budget() {
        let hm = HierarchicalMemory::default().await.unwrap();
        assert!(hm.is_within_budget());
    }

    #[tokio::test]
    async fn test_hierarchical_memory_compress_no_op() {
        let mut hm = HierarchicalMemory::default().await.unwrap();
        // When under budget, no compression needed
        let compressed = hm.compress_if_needed().await.unwrap();
        assert!(!compressed);
    }

    #[tokio::test]
    async fn test_hierarchical_memory_clone() {
        let hm = HierarchicalMemory::default().await.unwrap();
        let _hm2 = hm.clone();
    }
}
