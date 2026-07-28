//! Local RAG (Retrieval-Augmented Generation) System
//!
//! Provides context-aware code understanding by combining semantic search
//! with the MCP protocol for intelligent code assistance.
//!
//! Features:
//! - Automatic codebase indexing
//! - Semantic code search
//! - Context assembly for LLM prompts
//! - Relevance ranking and filtering
//! - Incremental updates on file changes
//! - Multi-language support

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

use crate::analysis::vector_store::{
    ChunkType, CodeChunker, CollectionScope, EmbeddingBackend, SearchFilter, SearchResult,
    VectorStore,
};
use crate::token_count::estimate_content_tokens;

/// RAG configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagConfig {
    /// Maximum context tokens to include
    pub max_context_tokens: usize,
    /// Number of search results to consider
    pub top_k: usize,
    /// Minimum relevance score threshold
    pub min_score: f32,
    /// File extensions to index
    pub include_extensions: Vec<String>,
    /// Patterns to exclude
    pub exclude_patterns: Vec<String>,
    /// Whether to include file metadata in context
    pub include_metadata: bool,
    /// Whether to include line numbers
    pub include_line_numbers: bool,
    /// Deduplication threshold (similarity between chunks)
    pub dedup_threshold: f32,
    /// Maximum chunk size in tokens
    pub max_chunk_tokens: usize,
}

impl Default for RagConfig {
    fn default() -> Self {
        Self {
            max_context_tokens: 8000,
            top_k: 10,
            min_score: 0.3,
            include_extensions: vec![
                "rs".into(),
                "py".into(),
                "js".into(),
                "ts".into(),
                "go".into(),
                "java".into(),
                "c".into(),
                "cpp".into(),
                "h".into(),
                "hpp".into(),
                "md".into(),
                "txt".into(),
                "toml".into(),
                "yaml".into(),
                "json".into(),
            ],
            exclude_patterns: vec![
                "target/".into(),
                "node_modules/".into(),
                ".git/".into(),
                "__pycache__/".into(),
                "*.min.js".into(),
                "*.min.css".into(),
                "vendor/".into(),
                "dist/".into(),
                "build/".into(),
            ],
            include_metadata: true,
            include_line_numbers: true,
            dedup_threshold: 0.95,
            max_chunk_tokens: 500,
        }
    }
}

impl RagConfig {
    /// Create config for Rust projects
    pub fn rust() -> Self {
        Self {
            include_extensions: vec!["rs".into(), "toml".into(), "md".into()],
            exclude_patterns: vec!["target/".into(), ".git/".into(), ".worktrees/".into()],
            ..Default::default()
        }
    }

    /// Create config for Python projects
    pub fn python() -> Self {
        Self {
            include_extensions: vec![
                "py".into(),
                "pyi".into(),
                "txt".into(),
                "md".into(),
                "toml".into(),
                "yaml".into(),
                "yml".into(),
            ],
            exclude_patterns: vec![
                "__pycache__/".into(),
                ".git/".into(),
                "venv/".into(),
                ".venv/".into(),
                "*.pyc".into(),
            ],
            ..Default::default()
        }
    }

    /// Create config for TypeScript/JavaScript
    pub fn typescript() -> Self {
        Self {
            include_extensions: vec![
                "ts".into(),
                "tsx".into(),
                "js".into(),
                "jsx".into(),
                "json".into(),
                "md".into(),
            ],
            exclude_patterns: vec![
                "node_modules/".into(),
                ".git/".into(),
                "dist/".into(),
                "build/".into(),
                "*.min.js".into(),
            ],
            ..Default::default()
        }
    }
}

/// Indexed file information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedFile {
    /// File path
    pub path: PathBuf,
    /// Last modified time
    pub modified_at: u64,
    /// Number of chunks
    pub chunk_count: usize,
    /// File size in bytes
    pub size: u64,
    /// Language/extension
    pub language: String,
}

/// RAG index statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RagStats {
    /// Total files indexed
    pub total_files: usize,
    /// Total chunks
    pub total_chunks: usize,
    /// Total tokens (estimated)
    pub total_tokens: usize,
    /// Last full index time
    pub last_full_index: Option<u64>,
    /// Last incremental update
    pub last_update: Option<u64>,
    /// Index build time in milliseconds
    pub build_time_ms: u64,
    /// Files by language
    pub files_by_language: HashMap<String, usize>,
}

/// Retrieved context for a query
#[derive(Debug, Clone)]
pub struct RetrievedContext {
    /// Formatted context string for LLM
    pub context: String,
    /// Sources used
    pub sources: Vec<ContextSource>,
    /// Total tokens used
    pub token_count: usize,
    /// Query that was used
    pub query: String,
    /// Retrieval time in milliseconds
    pub retrieval_time_ms: u64,
}

/// A source used in context
#[derive(Debug, Clone)]
pub struct ContextSource {
    /// File path
    pub file: PathBuf,
    /// Start line
    pub start_line: usize,
    /// End line
    pub end_line: usize,
    /// Chunk type
    pub chunk_type: ChunkType,
    /// Symbol name if available
    pub symbol: Option<String>,
    /// Relevance score
    pub score: f32,
}

/// File watcher for incremental updates
pub struct FileWatcher {
    /// Files and their last known modification time
    tracked_files: HashMap<PathBuf, u64>,
    /// Root directory
    root: PathBuf,
    /// Config for filtering
    config: RagConfig,
}

impl FileWatcher {
    /// Create new file watcher
    pub fn new(root: impl Into<PathBuf>, config: RagConfig) -> Self {
        Self {
            tracked_files: HashMap::new(),
            root: root.into(),
            config,
        }
    }

    /// Scan for changes
    pub fn scan_changes(&mut self) -> Vec<FileChange> {
        let mut changes = Vec::new();
        let mut current_files: HashSet<PathBuf> = HashSet::new();

        for entry in WalkDir::new(&self.root)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Skip directories and excluded patterns
            if path.is_dir() || self.is_excluded(path) {
                continue;
            }

            // Check extension
            if !self.is_included(path) {
                continue;
            }

            current_files.insert(path.to_path_buf());

            // Get modification time
            let modified = path
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            if let Some(&prev_modified) = self.tracked_files.get(path) {
                if modified > prev_modified {
                    changes.push(FileChange::Modified(path.to_path_buf()));
                    self.tracked_files.insert(path.to_path_buf(), modified);
                }
            } else {
                changes.push(FileChange::Added(path.to_path_buf()));
                self.tracked_files.insert(path.to_path_buf(), modified);
            }
        }

        // Check for deletions
        let deleted: Vec<_> = self
            .tracked_files
            .keys()
            .filter(|p| !current_files.contains(*p))
            .cloned()
            .collect();

        for path in deleted {
            self.tracked_files.remove(&path);
            changes.push(FileChange::Deleted(path));
        }

        changes
    }

    /// Check if path is excluded
    fn is_excluded(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        for pattern in &self.config.exclude_patterns {
            if pattern.ends_with('/') {
                // Directory pattern: match a whole path component, not a
                // substring ("target/" must not exclude target_info.rs).
                let dir = pattern.trim_end_matches('/');
                if path_str.split('/').any(|component| component == dir) {
                    return true;
                }
            } else if pattern.starts_with('*') {
                // Extension pattern
                let ext = pattern.trim_start_matches("*.");
                if path.extension().is_some_and(|e| e == ext) {
                    return true;
                }
            } else if path_str.contains(pattern) {
                return true;
            }
        }
        false
    }

    /// Check if path should be included
    fn is_included(&self, path: &Path) -> bool {
        if let Some(ext) = path.extension() {
            let ext_str = ext.to_string_lossy().to_string();
            self.config.include_extensions.contains(&ext_str)
        } else {
            false
        }
    }

    /// Get tracked file count
    pub fn tracked_count(&self) -> usize {
        self.tracked_files.len()
    }
}

/// File change type
#[derive(Debug, Clone)]
pub enum FileChange {
    Added(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
}

/// Local RAG Engine
pub struct RagEngine {
    /// Vector store for semantic search
    store: VectorStore,
    /// Configuration
    config: RagConfig,
    /// Root directory
    root: PathBuf,
    /// Code chunker
    _chunker: CodeChunker,
    /// File watcher for incremental updates
    watcher: FileWatcher,
    /// Statistics
    stats: RagStats,
    /// Indexed files
    indexed_files: HashMap<PathBuf, IndexedFile>,
    /// Collection name
    collection_name: String,
}

impl RagEngine {
    /// Create new RAG engine
    pub fn new(
        root: impl Into<PathBuf>,
        provider: Arc<EmbeddingBackend>,
        config: RagConfig,
    ) -> Self {
        let root = root.into();
        let collection_name = format!(
            "rag_{}",
            root.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "default".to_string())
        );

        let watcher = FileWatcher::new(&root, config.clone());

        Self {
            store: VectorStore::new(provider),
            config: config.clone(),
            root,
            _chunker: CodeChunker::new(config.max_chunk_tokens * 4), // ~4 chars per token
            watcher,
            stats: RagStats::default(),
            indexed_files: HashMap::new(),
            collection_name,
        }
    }

    /// Set storage path for persistence
    pub fn with_storage(mut self, path: impl Into<PathBuf>) -> Self {
        self.store = self.store.with_storage(path);
        self
    }

    /// Build full index
    pub async fn build_index(&mut self) -> Result<RagStats> {
        let start = Instant::now();

        // Clear existing collection
        self.store.delete_collection(&self.collection_name);
        self.store
            .collection(&self.collection_name, CollectionScope::Project);
        self.indexed_files.clear();

        // Scan and index files
        let mut files_by_lang: HashMap<String, usize> = HashMap::new();
        let mut total_chunks = 0;
        let mut total_tokens = 0;

        for entry in WalkDir::new(&self.root)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            if path.is_dir() || self.watcher.is_excluded(path) || !self.watcher.is_included(path) {
                continue;
            }

            match self.index_file(path).await {
                Ok(chunk_count) => {
                    let lang = path
                        .extension()
                        .map(|e| e.to_string_lossy().to_string())
                        .unwrap_or_else(|| "unknown".to_string());

                    *files_by_lang.entry(lang.clone()).or_insert(0) += 1;
                    total_chunks += chunk_count;

                    // Token estimate via shared tokenizer utility
                    if let Ok(content) = std::fs::read_to_string(path) {
                        total_tokens += estimate_content_tokens(&content);
                    }

                    // Track indexed file
                    let modified = path
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);

                    let size = path.metadata().ok().map(|m| m.len()).unwrap_or(0);

                    self.indexed_files.insert(
                        path.to_path_buf(),
                        IndexedFile {
                            path: path.to_path_buf(),
                            modified_at: modified,
                            chunk_count,
                            size,
                            language: lang,
                        },
                    );
                }
                Err(e) => {
                    tracing::warn!("Failed to index {}: {}", path.display(), e);
                }
            }
        }

        let build_time = start.elapsed().as_millis() as u64;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.stats = RagStats {
            total_files: self.indexed_files.len(),
            total_chunks,
            total_tokens,
            last_full_index: Some(now),
            last_update: Some(now),
            build_time_ms: build_time,
            files_by_language: files_by_lang,
        };

        Ok(self.stats.clone())
    }

    /// Update index incrementally
    pub async fn update_index(&mut self) -> Result<Vec<FileChange>> {
        let changes = self.watcher.scan_changes();

        for change in &changes {
            match change {
                FileChange::Added(path) | FileChange::Modified(path) => {
                    // Re-index file
                    self.store
                        .collection(&self.collection_name, CollectionScope::Project)
                        .remove_file(path);

                    if let Ok(chunk_count) = self.index_file(path).await {
                        let lang = path
                            .extension()
                            .map(|e| e.to_string_lossy().to_string())
                            .unwrap_or_else(|| "unknown".to_string());

                        let modified = path
                            .metadata()
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0);

                        let size = path.metadata().ok().map(|m| m.len()).unwrap_or(0);

                        self.indexed_files.insert(
                            path.clone(),
                            IndexedFile {
                                path: path.clone(),
                                modified_at: modified,
                                chunk_count,
                                size,
                                language: lang,
                            },
                        );
                    }
                }
                FileChange::Deleted(path) => {
                    self.store
                        .collection(&self.collection_name, CollectionScope::Project)
                        .remove_file(path);
                    self.indexed_files.remove(path);
                }
            }
        }

        if !changes.is_empty() {
            self.stats.last_update = Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            );
            self.stats.total_files = self.indexed_files.len();
        }

        Ok(changes)
    }

    /// Index a single file
    async fn index_file(&mut self, path: &Path) -> Result<usize> {
        self.store.index_file(&self.collection_name, path).await
    }

    /// Retrieve relevant context for a query
    pub async fn retrieve(&self, query: &str) -> Result<RetrievedContext> {
        let start = Instant::now();

        // Search for relevant chunks
        let filter = SearchFilter::new().with_min_score(self.config.min_score);

        let results = self
            .store
            .search(
                &self.collection_name,
                query,
                self.config.top_k * 2,
                Some(&filter),
            )
            .await?;

        // Deduplicate similar results
        let deduped = self.deduplicate_results(&results);

        // Assemble context
        let (context, sources, token_count) = self.assemble_context(&deduped);

        Ok(RetrievedContext {
            context,
            sources,
            token_count,
            query: query.to_string(),
            retrieval_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Deduplicate similar results
    fn deduplicate_results<'a>(&self, results: &'a [SearchResult]) -> Vec<&'a SearchResult> {
        let mut deduped: Vec<&SearchResult> = Vec::new();
        // Index: file_path -> list of (start_line, end_line) ranges already in deduped
        let mut file_ranges: HashMap<&Path, Vec<(usize, usize)>> = HashMap::new();
        // Track content hashes we've already seen for exact-duplicate fast path
        let mut seen_hashes: HashSet<&str> = HashSet::new();

        for result in results {
            // Fast path: skip exact content duplicates via content_hash
            if !result.chunk.metadata.content_hash.is_empty()
                && !seen_hashes.insert(&result.chunk.metadata.content_hash)
            {
                continue;
            }

            // Check file-path overlap using the indexed ranges (O(1) path lookup,
            // then only compare ranges within the same file)
            let mut dominated = false;
            if let Some(ranges) = file_ranges.get(&*result.chunk.metadata.file_path) {
                for &(start, end) in ranges {
                    if start <= result.chunk.metadata.end_line
                        && result.chunk.metadata.start_line <= end
                    {
                        dominated = true;
                        break;
                    }
                }
            }

            // Content similarity check (only when not already dominated)
            if !dominated && result.score > self.config.dedup_threshold {
                for existing in &deduped {
                    if existing.score <= self.config.dedup_threshold {
                        continue;
                    }
                    // Length pre-filter: Jaccard similarity between two sets A and B is at most
                    // min(|A|,|B|) / max(|A|,|B|). Skip if that upper bound is below threshold.
                    let len_a = existing.chunk.content.len();
                    let len_b = result.chunk.content.len();
                    let (min_len, max_len) = if len_a < len_b {
                        (len_a, len_b)
                    } else {
                        (len_b, len_a)
                    };
                    if max_len == 0
                        || (min_len as f32 / max_len as f32) < self.config.dedup_threshold
                    {
                        continue;
                    }
                    let similarity =
                        self.content_similarity(&existing.chunk.content, &result.chunk.content);
                    if similarity > self.config.dedup_threshold {
                        dominated = true;
                        break;
                    }
                }
            }

            if !dominated {
                // Update the file-range index
                file_ranges
                    .entry(&*result.chunk.metadata.file_path)
                    .or_default()
                    .push((
                        result.chunk.metadata.start_line,
                        result.chunk.metadata.end_line,
                    ));
                deduped.push(result);
            }

            if deduped.len() >= self.config.top_k {
                break;
            }
        }

        deduped
    }

    /// Calculate content similarity (simple Jaccard)
    fn content_similarity(&self, a: &str, b: &str) -> f32 {
        let words_a: HashSet<_> = a.split_whitespace().collect();
        let words_b: HashSet<_> = b.split_whitespace().collect();

        let intersection = words_a.intersection(&words_b).count();
        let union = words_a.union(&words_b).count();

        if union == 0 {
            0.0
        } else {
            intersection as f32 / union as f32
        }
    }

    /// Assemble context from results
    fn assemble_context(&self, results: &[&SearchResult]) -> (String, Vec<ContextSource>, usize) {
        let mut context_parts: Vec<String> = Vec::new();
        let mut sources: Vec<ContextSource> = Vec::new();
        let mut total_tokens = 0;

        for result in results {
            if total_tokens >= self.config.max_context_tokens {
                break;
            }

            let chunk = &result.chunk;
            let meta = &chunk.metadata;

            // Format chunk
            let mut formatted = String::new();

            if self.config.include_metadata {
                formatted.push_str(&format!(
                    "// File: {} (lines {}-{})\n",
                    meta.file_path.display(),
                    meta.start_line,
                    meta.end_line
                ));

                if let Some(ref symbol) = meta.symbol_name {
                    formatted.push_str(&format!("// Symbol: {} ({:?})\n", symbol, meta.chunk_type));
                }
            }

            if self.config.include_line_numbers {
                for (i, line) in chunk.content.lines().enumerate() {
                    formatted.push_str(&format!("{:4} | {}\n", meta.start_line + i, line));
                }
            } else {
                formatted.push_str(&chunk.content);
                formatted.push('\n');
            }

            let chunk_tokens = estimate_content_tokens(&formatted);
            if total_tokens + chunk_tokens > self.config.max_context_tokens {
                break;
            }

            context_parts.push(formatted);
            total_tokens += chunk_tokens;

            sources.push(ContextSource {
                file: meta.file_path.to_path_buf(),
                start_line: meta.start_line,
                end_line: meta.end_line,
                chunk_type: meta.chunk_type,
                symbol: meta.symbol_name.clone(),
                score: result.score,
            });
        }

        (context_parts.join("\n---\n\n"), sources, total_tokens)
    }

    /// Get statistics
    pub fn stats(&self) -> &RagStats {
        &self.stats
    }

    /// Get indexed files
    pub fn indexed_files(&self) -> Vec<&IndexedFile> {
        self.indexed_files.values().collect()
    }

    /// Save index to disk
    pub fn save(&self) -> Result<()> {
        self.store.save()
    }

    /// Load index from disk
    pub fn load(&mut self) -> Result<()> {
        self.store.load()
    }

    /// Search with specific filters
    pub async fn search_with_filter(
        &self,
        query: &str,
        filter: SearchFilter,
    ) -> Result<Vec<SearchResult>> {
        self.store
            .search(
                &self.collection_name,
                query,
                self.config.top_k,
                Some(&filter),
            )
            .await
    }

    /// Get context for specific files
    pub async fn context_for_files(
        &self,
        paths: &[PathBuf],
        query: &str,
    ) -> Result<RetrievedContext> {
        let start = Instant::now();

        let mut all_results = Vec::new();
        for path in paths {
            let filter = SearchFilter::new()
                .with_file_pattern(path.to_string_lossy().to_string())
                .with_min_score(self.config.min_score);

            if let Ok(results) = self
                .store
                .search(
                    &self.collection_name,
                    query,
                    self.config.top_k,
                    Some(&filter),
                )
                .await
            {
                all_results.extend(results);
            }
        }

        // Sort by score
        all_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Take top k
        all_results.truncate(self.config.top_k);

        let refs: Vec<&SearchResult> = all_results.iter().collect();
        let (context, sources, token_count) = self.assemble_context(&refs);

        Ok(RetrievedContext {
            context,
            sources,
            token_count,
            query: query.to_string(),
            retrieval_time_ms: start.elapsed().as_millis() as u64,
        })
    }
}

/// Context builder for creating LLM prompts with RAG context
pub struct ContextBuilder {
    /// Base system prompt
    system_prompt: String,
    /// Retrieved context
    context: Option<RetrievedContext>,
    /// Additional instructions
    instructions: Vec<String>,
    /// User query
    query: Option<String>,
}

impl ContextBuilder {
    /// Create new context builder
    pub fn new() -> Self {
        Self {
            system_prompt: String::new(),
            context: None,
            instructions: Vec::new(),
            query: None,
        }
    }

    /// Set system prompt
    pub fn with_system(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    /// Add retrieved context
    pub fn with_context(mut self, context: RetrievedContext) -> Self {
        self.context = Some(context);
        self
    }

    /// Add instruction
    pub fn with_instruction(mut self, instruction: impl Into<String>) -> Self {
        self.instructions.push(instruction.into());
        self
    }

    /// Set user query
    pub fn with_query(mut self, query: impl Into<String>) -> Self {
        self.query = Some(query.into());
        self
    }

    /// Build the final prompt
    pub fn build(self) -> String {
        let mut parts = Vec::new();

        // System prompt
        if !self.system_prompt.is_empty() {
            parts.push(self.system_prompt);
        }

        // Retrieved context
        if let Some(context) = self.context {
            parts.push(format!(
                "## Relevant Code Context\n\nThe following code snippets are relevant to your query:\n\n{}",
                context.context
            ));
        }

        // Instructions
        if !self.instructions.is_empty() {
            parts.push(format!(
                "## Instructions\n\n{}",
                self.instructions.join("\n- ")
            ));
        }

        // User query
        if let Some(query) = self.query {
            parts.push(format!("## Query\n\n{}", query));
        }

        parts.join("\n\n")
    }

    /// Get estimated token count
    pub fn token_count(&self) -> usize {
        let mut count = estimate_content_tokens(&self.system_prompt);

        if let Some(ref ctx) = self.context {
            count += ctx.token_count;
        }

        for inst in &self.instructions {
            count += estimate_content_tokens(inst);
        }

        if let Some(ref q) = self.query {
            count += estimate_content_tokens(q);
        }

        count
    }
}

impl Default for ContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/cognitive/rag/rag_test.rs"]
mod tests;
