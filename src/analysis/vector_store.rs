//! Vector Memory System
//!
//! Semantic vector storage for code search and memory.
//! Local-first design - no external server required.
//!
//! Uses HNSW (Hierarchical Navigable Small World) graphs via `hnsw_rs`
//! for O(log N) approximate nearest-neighbour search.
//!
//! Features:
//! - Code chunking strategies (functions, structs, modules)
//! - Embedding generation interface (pluggable backends)
//! - Similarity search with filters
//! - Collection management (project, session, global)
//! - Persistence to disk

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::warn;

// ---------------------------------------------------------------------------
// Serde helpers for Arc<Path> and Arc<str>
// ---------------------------------------------------------------------------

mod arc_path_serde {
    use super::*;

    pub fn serialize<S: Serializer>(path: &Arc<Path>, serializer: S) -> Result<S::Ok, S::Error> {
        path.as_ref().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Arc<Path>, D::Error> {
        let pb = PathBuf::deserialize(deserializer)?;
        Ok(Arc::from(pb.as_path()))
    }
}

mod arc_str_serde {
    use super::*;

    pub fn serialize<S: Serializer>(s: &Arc<str>, serializer: S) -> Result<S::Ok, S::Error> {
        s.as_ref().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Arc<str>, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Arc::from(s.as_str()))
    }
}

/// Embedding dimension (common for small models)
pub const EMBEDDING_DIM: usize = 384;

/// Maximum chunks per collection
pub const MAX_CHUNKS: usize = 100_000;

/// Maximum vocabulary size for TF-IDF provider before eviction occurs
pub const MAX_VOCABULARY_SIZE: usize = 50_000;

/// Chunk types for code organization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ChunkType {
    /// Function or method definition
    Function,
    /// Struct or class definition
    Struct,
    /// Enum definition
    Enum,
    /// Trait or interface definition
    Trait,
    /// Implementation block
    Impl,
    /// Module or namespace
    Module,
    /// Import statements
    Import,
    /// Comment or documentation
    Comment,
    /// Test function
    Test,
    /// Constant or static
    Constant,
    /// Generic code block
    #[default]
    CodeBlock,
    /// Plain text (non-code)
    Text,
}

impl ChunkType {
    /// Get weight for relevance scoring
    pub fn weight(&self) -> f32 {
        match self {
            Self::Function => 1.0,
            Self::Struct => 1.0,
            Self::Enum => 0.9,
            Self::Trait => 1.0,
            Self::Impl => 0.8,
            Self::Module => 0.7,
            Self::Import => 0.3,
            Self::Comment => 0.5,
            Self::Test => 0.8,
            Self::Constant => 0.6,
            Self::CodeBlock => 0.7,
            Self::Text => 0.5,
        }
    }
}

/// Metadata for a code chunk
///
/// `file_path` and `language` use `Arc` to avoid duplicating the same
/// strings across many chunks originating from the same source file.
/// Cloning an `Arc` is a cheap pointer copy instead of a heap allocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    /// Source file path (shared across chunks from the same file)
    #[serde(with = "arc_path_serde")]
    pub file_path: Arc<Path>,
    /// Start line (1-indexed)
    pub start_line: usize,
    /// End line (1-indexed)
    pub end_line: usize,
    /// Byte offset within `start_line` for pieces split from one oversized
    /// line. Older persisted chunks predate this field and start at offset 0.
    #[serde(default)]
    pub byte_offset: usize,
    /// Chunk type
    pub chunk_type: ChunkType,
    /// Symbol name if applicable (function name, struct name, etc.)
    pub symbol_name: Option<String>,
    /// Language identifier (shared across chunks from the same file)
    #[serde(with = "arc_str_serde")]
    pub language: Arc<str>,
    /// Hash of content for deduplication
    pub content_hash: String,
    /// Timestamp when indexed
    pub indexed_at: u64,
    /// Custom tags
    pub tags: Vec<String>,
}

impl ChunkMetadata {
    /// Create new metadata.
    ///
    /// Accepts `Into<Arc<Path>>` and `Into<Arc<str>>` so callers can pass
    /// a `PathBuf`, `&Path`, or a pre-existing `Arc<Path>` (cheap clone for
    /// batches of chunks from the same file). Same for language strings.
    pub fn new(
        file_path: impl Into<Arc<Path>>,
        start_line: usize,
        end_line: usize,
        chunk_type: ChunkType,
        language: impl Into<Arc<str>>,
        content: &str,
    ) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let content_hash = hex::encode(hasher.finalize());

        let indexed_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            file_path: file_path.into(),
            start_line,
            end_line,
            byte_offset: 0,
            chunk_type,
            symbol_name: None,
            language: language.into(),
            content_hash,
            indexed_at,
            tags: Vec::new(),
        }
    }

    /// Set symbol name
    pub fn with_symbol(mut self, name: impl Into<String>) -> Self {
        self.symbol_name = Some(name.into());
        self
    }

    /// Add tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }
}

/// A chunk of code with its embedding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChunk {
    /// Unique identifier
    pub id: String,
    /// The actual content
    pub content: String,
    /// Metadata about the chunk
    pub metadata: ChunkMetadata,
    /// Embedding vector (if computed)
    #[serde(skip)]
    pub embedding: Option<Vec<f32>>,
}

impl CodeChunk {
    fn stable_id(metadata: &ChunkMetadata) -> String {
        format!(
            "{}:{}:{}:{}:{}",
            metadata.file_path.display(),
            metadata.start_line,
            metadata.end_line,
            metadata.byte_offset,
            metadata.content_hash
        )
    }

    fn refresh_id(&mut self) {
        self.id = Self::stable_id(&self.metadata);
    }

    /// Create a new code chunk
    pub fn new(content: String, metadata: ChunkMetadata) -> Self {
        let id = Self::stable_id(&metadata);

        Self {
            id,
            content,
            metadata,
            embedding: None,
        }
    }

    /// Set embedding
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    /// Get content length
    pub fn len(&self) -> usize {
        self.content.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }
}

/// Search result with similarity score
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The matching chunk
    pub chunk: CodeChunk,
    /// Similarity score (0.0 - 1.0)
    pub score: f32,
    /// Distance from query
    pub distance: f32,
}

/// Filter for search queries
#[derive(Debug, Clone, Default)]
pub struct SearchFilter {
    /// Filter by file paths (glob patterns)
    pub file_patterns: Vec<String>,
    /// Filter by chunk types
    pub chunk_types: Vec<ChunkType>,
    /// Filter by language
    pub languages: Vec<String>,
    /// Filter by tags
    pub tags: Vec<String>,
    /// Minimum score threshold
    pub min_score: Option<f32>,
}

impl SearchFilter {
    /// Create new filter
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by file pattern
    pub fn with_file_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.file_patterns.push(pattern.into());
        self
    }

    /// Filter by chunk type
    pub fn with_chunk_type(mut self, chunk_type: ChunkType) -> Self {
        self.chunk_types.push(chunk_type);
        self
    }

    /// Filter by language
    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.languages.push(language.into());
        self
    }

    /// Filter by tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Set minimum score
    pub fn with_min_score(mut self, score: f32) -> Self {
        self.min_score = Some(score);
        self
    }

    /// Check if a chunk matches the filter
    pub fn matches(&self, chunk: &CodeChunk) -> bool {
        // Check file patterns
        if !self.file_patterns.is_empty() {
            let path_str = chunk.metadata.file_path.to_string_lossy();
            let matches = self.file_patterns.iter().any(|pattern| {
                glob::Pattern::new(pattern)
                    .map(|p| p.matches(&path_str))
                    .unwrap_or(false)
            });
            if !matches {
                return false;
            }
        }

        // Check chunk types
        if !self.chunk_types.is_empty() && !self.chunk_types.contains(&chunk.metadata.chunk_type) {
            return false;
        }

        // Check languages
        if !self.languages.is_empty()
            && !self
                .languages
                .iter()
                .any(|l| l.eq_ignore_ascii_case(&chunk.metadata.language))
        {
            return false;
        }

        // Check tags
        if !self.tags.is_empty() && !self.tags.iter().any(|t| chunk.metadata.tags.contains(t)) {
            return false;
        }

        true
    }
}

/// Collection scope for organizing chunks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum CollectionScope {
    /// Project-specific (tied to a git repo or directory)
    #[default]
    Project,
    /// Session-specific (temporary, cleared on restart)
    Session,
    /// Global (shared across all projects)
    Global,
}

/// Health status of a vector index
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexHealth {
    /// Index is consistent: no NaN/Inf, no duplicates, dimensions match
    Healthy,
    /// Index has minor issues (e.g., duplicate IDs) but is still usable
    Degraded,
    /// Index is corrupt (e.g., NaN/Inf values, dimension mismatches) and must be rebuilt
    Corrupt,
}

/// Vector collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorCollection {
    /// Collection name
    pub name: String,
    /// Collection scope
    pub scope: CollectionScope,
    /// Chunks in this collection (persisted so RAG survives restarts)
    chunks: Vec<CodeChunk>,
    /// Index of chunk IDs to positions — rebuilt after deserialization
    #[serde(skip)]
    id_index: HashMap<String, usize>,
    /// File path to chunk IDs index
    file_index: HashMap<PathBuf, Vec<String>>,
    /// Created timestamp
    pub created_at: u64,
    /// Last updated timestamp
    pub updated_at: u64,
}

impl VectorCollection {
    /// Create new collection
    pub fn new(name: impl Into<String>, scope: CollectionScope) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            name: name.into(),
            scope,
            chunks: Vec::new(),
            id_index: HashMap::new(),
            file_index: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Add a chunk to the collection
    pub fn add_chunk(&mut self, chunk: CodeChunk) -> Result<()> {
        if self.chunks.len() >= MAX_CHUNKS {
            return Err(anyhow!(
                "Collection {} is full (max {} chunks)",
                self.name,
                MAX_CHUNKS
            ));
        }

        // Update file index (convert Arc<Path> to PathBuf for the index key)
        self.file_index
            .entry(chunk.metadata.file_path.to_path_buf())
            .or_default()
            .push(chunk.id.clone());

        // Add to chunks
        let idx = self.chunks.len();
        self.id_index.insert(chunk.id.clone(), idx);
        self.chunks.push(chunk);

        self.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(())
    }

    /// Get chunk by ID
    pub fn get_chunk(&self, id: &str) -> Option<&CodeChunk> {
        self.id_index.get(id).map(|&idx| &self.chunks[idx])
    }

    /// Remove chunk by ID
    pub fn remove_chunk(&mut self, id: &str) -> Option<CodeChunk> {
        if let Some(&idx) = self.id_index.get(id) {
            // Use swap_remove for O(1) removal instead of O(N) shift
            let chunk = self.chunks.swap_remove(idx);
            self.id_index.remove(id);

            // If the removed element wasn't the last one, update the index
            // for the element that was swapped into position `idx`
            if idx < self.chunks.len() {
                self.id_index.insert(self.chunks[idx].id.clone(), idx);
            }

            // Update file index
            if let Some(file_chunks) = self.file_index.get_mut(chunk.metadata.file_path.as_ref()) {
                file_chunks.retain(|cid| cid != id);
            }

            self.updated_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            Some(chunk)
        } else {
            None
        }
    }

    /// Remove all chunks for a file
    pub fn remove_file(&mut self, path: &Path) {
        if let Some(chunk_ids) = self.file_index.remove(path) {
            let ids_to_remove: HashSet<&String> = chunk_ids.iter().collect();

            // Retain only chunks not in the removal set -- O(N) single pass
            self.chunks.retain(|c| !ids_to_remove.contains(&c.id));

            // Rebuild id_index after bulk removal
            self.id_index.clear();
            for (i, c) in self.chunks.iter().enumerate() {
                self.id_index.insert(c.id.clone(), i);
            }

            self.updated_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
        }
    }

    fn chunk_ids_for_file(&self, path: &Path) -> Vec<String> {
        self.file_index.get(path).cloned().unwrap_or_default()
    }

    /// Rebuild the derived ID and file indexes from the `chunks` vector.
    ///
    /// Must be called after deserialization because `id_index` is
    /// `#[serde(skip)]` — it is derivable from `chunks` but not persisted.
    pub fn rebuild_id_index(&mut self) {
        self.id_index.clear();
        self.file_index.clear();
        for (i, chunk) in self.chunks.iter().enumerate() {
            self.id_index.insert(chunk.id.clone(), i);
            self.file_index
                .entry(chunk.metadata.file_path.to_path_buf())
                .or_default()
                .push(chunk.id.clone());
        }
    }

    /// Get all chunks
    pub fn chunks(&self) -> &[CodeChunk] {
        &self.chunks
    }

    /// Get chunk count
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Get files in collection
    pub fn files(&self) -> Vec<&PathBuf> {
        self.file_index.keys().collect()
    }
}

/// Trait for embedding generation.
///
/// NOTE: Prefer using `EmbeddingBackend` enum dispatch instead of
/// `Arc<dyn EmbeddingProvider>` for new code. The trait is retained
/// as documentation of the interface contract.
#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embedding for text
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Generate embeddings for multiple texts
    ///
    /// Default implementation embeds one text at a time; providers with a
    /// native batch endpoint (e.g. `HttpEmbeddingProvider`) override this.
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }

    /// Get embedding dimension
    fn dimension(&self) -> usize;
}

/// Mock embedding provider for testing
pub struct MockEmbeddingProvider {
    dimension: usize,
    #[cfg(test)]
    fail_on_substring: Option<String>,
}

impl MockEmbeddingProvider {
    /// Create new mock provider
    pub fn new(dimension: usize) -> Self {
        Self {
            dimension,
            #[cfg(test)]
            fail_on_substring: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn failing_on(dimension: usize, marker: impl Into<String>) -> Self {
        Self {
            dimension,
            fail_on_substring: Some(marker.into()),
        }
    }
}

impl Default for MockEmbeddingProvider {
    fn default() -> Self {
        Self::new(EMBEDDING_DIM)
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        #[cfg(test)]
        if self
            .fail_on_substring
            .as_deref()
            .is_some_and(|marker| text.contains(marker))
        {
            anyhow::bail!("mock embedding failure for configured marker");
        }

        // Generate deterministic embedding based on text hash
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        let hash = hasher.finalize();

        let mut embedding = vec![0.0f32; self.dimension];
        for (i, byte) in hash.iter().cycle().take(self.dimension).enumerate() {
            embedding[i] = (*byte as f32 - 128.0) / 128.0;
        }

        // Normalize
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut embedding {
                *x /= norm;
            }
        }

        Ok(embedding)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

/// Simple TF-IDF based embedding provider (no external dependencies)
pub struct TfIdfEmbeddingProvider {
    dimension: usize,
    /// Maps token -> dimension index
    vocabulary: Arc<RwLock<HashMap<String, usize>>>,
    /// Tracks usage count per token for eviction decisions
    usage_counts: Arc<RwLock<HashMap<String, u64>>>,
}

impl TfIdfEmbeddingProvider {
    /// Create new TF-IDF provider
    pub fn new(dimension: usize) -> Self {
        Self {
            dimension,
            vocabulary: Arc::new(RwLock::new(HashMap::new())),
            usage_counts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn tokenize(text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|s| s.len() > 1)
            .map(String::from)
            .collect()
    }

    fn get_or_create_index(&self, token: &str) -> usize {
        // Fast path: token already in vocabulary
        {
            let read = self.vocabulary.read().unwrap_or_else(|e| e.into_inner());
            if let Some(&idx) = read.get(token) {
                drop(read);
                // Increment usage count
                let mut counts = self.usage_counts.write().unwrap_or_else(|e| e.into_inner());
                *counts.entry(token.to_string()).or_default() += 1;
                return idx;
            }
        }

        // Slow path: insert new token
        let mut write = self.vocabulary.write().unwrap_or_else(|e| e.into_inner());
        // Double-check after acquiring write lock
        if let Some(&idx) = write.get(token) {
            drop(write);
            let mut counts = self.usage_counts.write().unwrap_or_else(|e| e.into_inner());
            *counts.entry(token.to_string()).or_default() += 1;
            return idx;
        }

        let idx = write.len() % self.dimension;
        write.insert(token.to_string(), idx);

        // Evict least-used terms if vocabulary exceeds the cap
        if write.len() > MAX_VOCABULARY_SIZE {
            let mut counts = self.usage_counts.write().unwrap_or_else(|e| e.into_inner());
            let evict_count = write.len() - MAX_VOCABULARY_SIZE;

            warn!(
                "TF-IDF vocabulary exceeded cap of {}; evicting {} least-used terms",
                MAX_VOCABULARY_SIZE, evict_count
            );

            // Find the least-used terms to evict
            let mut terms_by_usage: Vec<(String, u64)> = write
                .keys()
                .map(|k| {
                    let count = counts.get(k).copied().unwrap_or(0);
                    (k.clone(), count)
                })
                .collect();
            terms_by_usage.sort_by_key(|(_, count)| *count);

            for (term, _) in terms_by_usage.into_iter().take(evict_count) {
                // Don't evict the token we just inserted
                if term != token {
                    write.remove(&term);
                    counts.remove(&term);
                }
            }
        }

        // Track usage for the newly inserted token
        drop(write);
        let mut counts = self.usage_counts.write().unwrap_or_else(|e| e.into_inner());
        *counts.entry(token.to_string()).or_default() += 1;

        idx
    }
}

impl Default for TfIdfEmbeddingProvider {
    fn default() -> Self {
        Self::new(EMBEDDING_DIM)
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for TfIdfEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let tokens = Self::tokenize(text);
        let mut embedding = vec![0.0f32; self.dimension];

        // Count term frequencies
        let mut tf: HashMap<String, f32> = HashMap::new();
        for token in &tokens {
            *tf.entry(token.clone()).or_default() += 1.0;
        }

        // Build embedding
        for (token, count) in tf {
            let idx = self.get_or_create_index(&token);
            embedding[idx] += count / tokens.len() as f32;
        }

        // Normalize
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut embedding {
                *x /= norm;
            }
        }

        Ok(embedding)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

/// Default ef_search value for HNSW queries (width of the lowest-level search).
const HNSW_EF_SEARCH: usize = 50;

/// Vector index backed by HNSW (Hierarchical Navigable Small World) graphs.
///
/// Uses `hnsw_rs` for O(log N) approximate nearest-neighbour search.
/// Deletions are handled via a soft-delete set that filters results;
/// the index is compacted (rebuilt) when the deletion ratio exceeds 30%.
pub struct VectorIndex {
    /// Embeddings (row-major, L2-normalized at insert time).
    /// Kept for serialization and integrity checks.
    embeddings: Vec<Vec<f32>>,
    /// Chunk IDs corresponding to each embedding slot.
    chunk_ids: Vec<String>,
    /// Expected embedding dimension.
    dimension: usize,
    /// HNSW graph.  `None` until the first valid vector is inserted.
    hnsw: Option<hnsw_rs::hnsw::Hnsw<'static, f32, hnsw_rs::anndists::dist::DistCosine>>,
    /// Indices that have been logically deleted but still live in the HNSW
    /// graph.  Filtered out at query time and compacted periodically.
    deleted: HashSet<usize>,
}

impl VectorIndex {
    /// Create a new, empty index for vectors of the given `dimension`.
    pub fn new(dimension: usize) -> Self {
        Self {
            embeddings: Vec::new(),
            chunk_ids: Vec::new(),
            dimension,
            hnsw: None,
            deleted: HashSet::new(),
        }
    }

    /// Number of live (non-deleted) entries.
    pub fn len(&self) -> usize {
        self.embeddings.len() - self.deleted.len()
    }

    /// Whether the index contains zero live entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Embedding width accepted by this index.
    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// Add an embedding to the index.
    ///
    /// The embedding is L2-normalized at insert time so that the HNSW
    /// dot-product distance equals cosine distance.
    pub fn add(&mut self, chunk_id: String, mut embedding: Vec<f32>) -> Result<()> {
        if embedding.len() != self.dimension {
            return Err(anyhow!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.dimension,
                embedding.len()
            ));
        }

        Self::l2_normalize(&mut embedding);

        let idx = self.embeddings.len();
        // Only insert into HNSW if the vector is finite (no NaN/Inf).
        let is_finite = embedding.iter().all(|v| v.is_finite());
        if is_finite {
            let hnsw = self.hnsw.get_or_insert_with(|| {
                hnsw_rs::hnsw::Hnsw::new(
                    16,  // max_nb_connection
                    256, // max_elements hint (will grow automatically)
                    16,  // max_layer
                    200, // ef_construction
                    hnsw_rs::anndists::dist::DistCosine,
                )
            });
            hnsw.insert((&embedding, idx));
        }

        self.embeddings.push(embedding);
        self.chunk_ids.push(chunk_id);
        Ok(())
    }

    /// Remove an embedding by chunk ID.
    ///
    /// The entry is soft-deleted (filtered from search results).
    /// When the fraction of deleted entries exceeds 30 %, the HNSW
    /// graph is compacted automatically on the next `search()`.
    pub fn remove(&mut self, chunk_id: &str) {
        if let Some(pos) = self
            .chunk_ids
            .iter()
            .enumerate()
            .filter(|(i, _)| !self.deleted.contains(i))
            .find(|(_, id)| id.as_str() == chunk_id)
            .map(|(i, _)| i)
        {
            self.deleted.insert(pos);
        }
    }

    /// Search for the `k` most similar embeddings to `query`.
    ///
    /// Returns `(chunk_id, cosine_similarity)` pairs sorted by
    /// descending similarity.
    pub fn search(&self, query: &[f32], k: usize) -> Vec<(String, f32)> {
        if query.len() != self.dimension || k == 0 {
            return Vec::new();
        }

        let hnsw = match self.hnsw.as_ref() {
            Some(h) => h,
            None => return Vec::new(),
        };

        // Normalize the query so dot-product == cosine similarity.
        let mut normed = query.to_vec();
        Self::l2_normalize(&mut normed);

        // Ask HNSW for extra candidates to compensate for deleted entries.
        let extra = self.deleted.len().min(k * 2);
        let ef = HNSW_EF_SEARCH.max(k + extra);
        let neighbours = hnsw.search(&normed, k + extra, ef);

        let mut results: Vec<(String, f32)> = neighbours
            .into_iter()
            .filter(|n| !self.deleted.contains(&n.d_id))
            .map(|n| {
                let similarity = 1.0 - n.distance; // DistCosine returns 1-cosine_similarity
                (self.chunk_ids[n.d_id].clone(), similarity)
            })
            .take(k)
            .collect();

        // HNSW already returns neighbours sorted by distance (ascending),
        // which maps to similarity descending, but let us be explicit.
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// Clear the entire index.
    pub fn clear(&mut self) {
        self.embeddings.clear();
        self.chunk_ids.clear();
        self.deleted.clear();
        self.hnsw = None;
    }

    /// Compact the index: rebuild the HNSW graph, dropping deleted entries.
    ///
    /// Called automatically when the deletion ratio exceeds the threshold,
    /// but can also be invoked manually.
    #[allow(dead_code)]
    pub fn compact(&mut self) {
        if self.deleted.is_empty() {
            return;
        }
        let mut new_embeddings = Vec::with_capacity(self.len());
        let mut new_chunk_ids = Vec::with_capacity(self.len());

        for (i, (emb, cid)) in self
            .embeddings
            .iter()
            .zip(self.chunk_ids.iter())
            .enumerate()
        {
            if !self.deleted.contains(&i) {
                new_embeddings.push(emb.clone());
                new_chunk_ids.push(cid.clone());
            }
        }

        self.embeddings = new_embeddings;
        self.chunk_ids = new_chunk_ids;
        self.deleted.clear();

        // Rebuild the HNSW from scratch.
        self.hnsw = None;
        if !self.embeddings.is_empty() {
            let hnsw = hnsw_rs::hnsw::Hnsw::new(
                16,
                self.embeddings.len().max(256),
                16,
                200,
                hnsw_rs::anndists::dist::DistCosine,
            );
            for (i, emb) in self.embeddings.iter().enumerate() {
                if emb.iter().all(|v| v.is_finite()) {
                    hnsw.insert((emb, i));
                }
            }
            self.hnsw = Some(hnsw);
        }
    }

    // ------------------------------------------------------------------
    // Serialization helpers
    // ------------------------------------------------------------------

    /// Return the live (non-deleted) embeddings and chunk IDs for
    /// serialization.  Filters out soft-deleted entries.
    pub(crate) fn live_data_owned(&self) -> (Vec<Vec<f32>>, Vec<String>) {
        if self.deleted.is_empty() {
            return (self.embeddings.clone(), self.chunk_ids.clone());
        }
        let mut embs = Vec::with_capacity(self.len());
        let mut ids = Vec::with_capacity(self.len());
        for (i, (e, c)) in self
            .embeddings
            .iter()
            .zip(self.chunk_ids.iter())
            .enumerate()
        {
            if !self.deleted.contains(&i) {
                embs.push(e.clone());
                ids.push(c.clone());
            }
        }
        (embs, ids)
    }

    // ------------------------------------------------------------------
    // Static helpers
    // ------------------------------------------------------------------

    /// Dot product between two vectors.
    #[inline]
    fn dot_product(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }

    /// L2-normalize a vector in place.  Zero vectors are left unchanged.
    fn l2_normalize(v: &mut [f32]) {
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        }
    }

    /// Cosine similarity between two arbitrary vectors.
    ///
    /// Normalizes both inputs before computing the dot product.
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let mut na = a.to_vec();
        let mut nb = b.to_vec();
        Self::l2_normalize(&mut na);
        Self::l2_normalize(&mut nb);
        Self::dot_product(&na, &nb)
    }

    // ------------------------------------------------------------------
    // Integrity / health
    // ------------------------------------------------------------------

    /// Verify index integrity, returning a list of issues found.
    ///
    /// Checks for:
    /// - Mismatched embedding dimensions
    /// - NaN or Inf values in vectors
    /// - Duplicate chunk IDs
    /// - Empty embedding vectors
    pub fn verify_index_integrity(&self) -> Vec<String> {
        let mut issues = Vec::new();

        // Check for duplicate IDs (among live entries only)
        let mut seen_ids = HashSet::new();
        for (i, id) in self.chunk_ids.iter().enumerate() {
            if self.deleted.contains(&i) {
                continue;
            }
            if !seen_ids.insert(id.as_str()) {
                issues.push(format!("Duplicate chunk ID: {}", id));
            }
        }

        // Check each live embedding
        for (i, embedding) in self.embeddings.iter().enumerate() {
            if self.deleted.contains(&i) {
                continue;
            }
            let id = self
                .chunk_ids
                .get(i)
                .map(|s| s.as_str())
                .unwrap_or("<missing>");

            // Dimension mismatch
            if embedding.len() != self.dimension {
                issues.push(format!(
                    "Dimension mismatch for '{}': expected {}, got {}",
                    id,
                    self.dimension,
                    embedding.len()
                ));
            }

            // Empty vector
            if embedding.is_empty() {
                issues.push(format!("Empty embedding vector for '{}'", id));
                continue;
            }

            // NaN / Inf values
            let has_nan = embedding.iter().any(|v| v.is_nan());
            let has_inf = embedding.iter().any(|v| v.is_infinite());
            if has_nan {
                issues.push(format!("NaN values in embedding for '{}'", id));
            }
            if has_inf {
                issues.push(format!("Inf values in embedding for '{}'", id));
            }
        }

        // Parallel array length mismatch (raw arrays, ignoring deletions)
        if self.embeddings.len() != self.chunk_ids.len() {
            issues.push(format!(
                "Array length mismatch: {} embeddings vs {} chunk_ids",
                self.embeddings.len(),
                self.chunk_ids.len()
            ));
        }

        issues
    }

    /// Check overall health of the index.
    pub fn check_health(&self) -> IndexHealth {
        let issues = self.verify_index_integrity();
        if issues.is_empty() {
            return IndexHealth::Healthy;
        }

        // NaN, Inf, dimension mismatch, or array length mismatch => Corrupt
        let has_corrupt = issues.iter().any(|issue| {
            issue.contains("NaN")
                || issue.contains("Inf")
                || issue.contains("Dimension mismatch")
                || issue.contains("Array length mismatch")
                || issue.contains("Empty embedding")
        });

        if has_corrupt {
            IndexHealth::Corrupt
        } else {
            // Only duplicates or other minor issues
            IndexHealth::Degraded
        }
    }
}

/// Code chunker for splitting code into meaningful pieces
#[derive(Clone)]
pub struct CodeChunker {
    /// Maximum chunk size in measured tokens.
    pub max_chunk_size: usize,
    /// Minimum chunk size in characters (a filtering threshold, not a
    /// context-size projection).
    pub min_chunk_size: usize,
    /// Maximum measured-token overlap between adjacent fallback chunks.
    pub overlap: usize,
}

impl Default for CodeChunker {
    fn default() -> Self {
        Self {
            max_chunk_size: 500,
            min_chunk_size: 100,
            overlap: 50,
        }
    }
}

impl CodeChunker {
    /// Create new chunker
    pub fn new(max_chunk_tokens: usize) -> Self {
        Self {
            max_chunk_size: max_chunk_tokens,
            ..Default::default()
        }
    }

    /// Split one line that is itself larger than the measured token budget.
    /// Line-based chunking alone cannot bound minified/generated sources, and
    /// recursively feeding the same line back into `chunk_fixed_size` would
    /// never make it smaller.
    fn split_oversized_line(&self, line: &str) -> Vec<(usize, String)> {
        let max_tokens = self.max_chunk_size.max(1);
        let mut boundaries: Vec<usize> = line.char_indices().map(|(index, _)| index).collect();
        boundaries.push(line.len());

        let mut pieces = Vec::new();
        let mut start = 0usize;
        while start + 1 < boundaries.len() {
            let start_byte = boundaries[start];
            let mut low = start + 1;
            let mut high = boundaries.len() - 1;
            let mut best = start + 1;

            // Token counts are effectively monotonic for prefixes. Search for
            // the largest measured prefix that fits, then verify the result
            // below so a tokenizer edge case can never emit an oversized
            // piece when a smaller character boundary is available.
            while low <= high {
                let mid = low + (high - low) / 2;
                let candidate = &line[start_byte..boundaries[mid]];
                if crate::token_count::estimate_content_tokens(candidate) <= max_tokens {
                    best = mid;
                    low = mid + 1;
                } else {
                    high = mid.saturating_sub(1);
                }
            }
            while best > start + 1
                && crate::token_count::estimate_content_tokens(&line[start_byte..boundaries[best]])
                    > max_tokens
            {
                best -= 1;
            }

            pieces.push((start_byte, line[start_byte..boundaries[best]].to_string()));
            start = best;
        }
        pieces
    }

    /// Chunk Rust code by functions, structs, etc.
    pub fn chunk_rust(&self, content: &str, file_path: &Path) -> Vec<CodeChunk> {
        static PATTERNS: once_cell::sync::Lazy<Vec<(regex::Regex, ChunkType)>> =
            once_cell::sync::Lazy::new(|| {
                [
                    (r"^\s*(pub\s+)?(async\s+)?fn\s+", ChunkType::Function),
                    (r"^\s*(pub\s+)?struct\s+", ChunkType::Struct),
                    (r"^\s*(pub\s+)?enum\s+", ChunkType::Enum),
                    (r"^\s*(pub\s+)?trait\s+", ChunkType::Trait),
                    (r"^\s*impl\s+", ChunkType::Impl),
                    (r"^\s*(pub\s+)?mod\s+", ChunkType::Module),
                    (r"^\s*#\[test\]", ChunkType::Test),
                    (r"^\s*(pub\s+)?const\s+", ChunkType::Constant),
                    (r"^\s*use\s+", ChunkType::Import),
                ]
                .into_iter()
                .filter_map(|(pat, ct)| regex::Regex::new(pat).ok().map(|re| (re, ct)))
                .collect()
            });

        let mut chunks = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Pre-allocate shared Arc for the file path and language so all
        // chunks from this file share the same allocation (cheap clone).
        let shared_path: Arc<Path> = Arc::from(file_path);
        let shared_lang: Arc<str> = Arc::from("rust");

        let mut current_start = 0;
        let mut current_type = ChunkType::CodeBlock;
        let mut brace_depth = 0;
        let mut in_block = false;

        for (line_num, line) in lines.iter().enumerate() {
            // Check for pattern starts
            for (pattern, chunk_type) in PATTERNS.iter() {
                if pattern.is_match(line) && !in_block {
                    // Save previous chunk if exists
                    if line_num > current_start {
                        let chunk_content: String = lines[current_start..line_num].join("\n");
                        if chunk_content.len() >= self.min_chunk_size {
                            let metadata = ChunkMetadata::new(
                                shared_path.clone(),
                                current_start + 1,
                                line_num,
                                current_type,
                                shared_lang.clone(),
                                &chunk_content,
                            );
                            chunks.push(CodeChunk::new(chunk_content, metadata));
                        }
                    }
                    current_start = line_num;
                    current_type = *chunk_type;
                    in_block = true;
                    break;
                }
            }

            // Track brace depth for block detection
            brace_depth += line.chars().filter(|c| *c == '{').count() as i32;
            brace_depth -= line.chars().filter(|c| *c == '}').count() as i32;

            if in_block && brace_depth <= 0 {
                // End of block
                let chunk_content: String = lines[current_start..=line_num].join("\n");

                // Extract symbol name
                let symbol_name = self.extract_rust_symbol(&chunk_content, current_type);

                let mut metadata = ChunkMetadata::new(
                    shared_path.clone(),
                    current_start + 1,
                    line_num + 1,
                    current_type,
                    shared_lang.clone(),
                    &chunk_content,
                );

                if let Some(name) = symbol_name {
                    metadata = metadata.with_symbol(name);
                }

                chunks.push(CodeChunk::new(chunk_content, metadata));
                current_start = line_num + 1;
                current_type = ChunkType::CodeBlock;
                in_block = false;
                brace_depth = 0;
            }
        }

        // Handle remaining content
        if current_start < lines.len() {
            let chunk_content: String = lines[current_start..].join("\n");
            if chunk_content.len() >= self.min_chunk_size {
                let metadata = ChunkMetadata::new(
                    shared_path.clone(),
                    current_start + 1,
                    lines.len(),
                    current_type,
                    shared_lang.clone(),
                    &chunk_content,
                );
                chunks.push(CodeChunk::new(chunk_content, metadata));
            }
        }

        chunks
    }

    /// Extract symbol name from Rust code
    fn extract_rust_symbol(&self, content: &str, chunk_type: ChunkType) -> Option<String> {
        use std::sync::LazyLock;

        static SYM_FN_RE: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"fn\s+(\w+)").expect("invalid fn regex"));
        static SYM_STRUCT_RE: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"struct\s+(\w+)").expect("invalid struct regex"));
        static SYM_ENUM_RE: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"enum\s+(\w+)").expect("invalid enum regex"));
        static SYM_TRAIT_RE: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"trait\s+(\w+)").expect("invalid trait regex"));
        static SYM_IMPL_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
            regex::Regex::new(r"impl(?:<[^>]+>)?\s+(?:(\w+)|(?:\w+)\s+for\s+(\w+))")
                .expect("invalid impl regex")
        });
        static SYM_MOD_RE: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"mod\s+(\w+)").expect("invalid mod regex"));

        let first_line = content.lines().next()?;

        match chunk_type {
            ChunkType::Function => SYM_FN_RE
                .captures(first_line)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string()),
            ChunkType::Struct => SYM_STRUCT_RE
                .captures(first_line)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string()),
            ChunkType::Enum => SYM_ENUM_RE
                .captures(first_line)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string()),
            ChunkType::Trait => SYM_TRAIT_RE
                .captures(first_line)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string()),
            ChunkType::Impl => SYM_IMPL_RE.captures(first_line).and_then(|c| {
                c.get(1)
                    .or_else(|| c.get(2))
                    .map(|m| m.as_str().to_string())
            }),
            ChunkType::Module => SYM_MOD_RE
                .captures(first_line)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string()),
            _ => None,
        }
    }

    /// Chunk by fixed size with overlap (fallback for unknown languages)
    pub fn chunk_fixed_size(
        &self,
        content: &str,
        file_path: &Path,
        language: &str,
    ) -> Vec<CodeChunk> {
        let mut chunks = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Pre-allocate shared Arc for the file path and language so all
        // chunks from this file share the same allocation (cheap clone).
        let shared_path: Arc<Path> = Arc::from(file_path);
        let shared_lang: Arc<str> = Arc::from(language);

        let max_tokens = self.max_chunk_size.max(1);
        let mut start = 0;
        while start < lines.len() {
            let mut end = start;

            // Accumulate lines using the shared tokenizer. A single line may
            // exceed the budget, but must still make forward progress.
            while end < lines.len() {
                let candidate = lines[start..=end].join("\n");
                let tokens = crate::token_count::estimate_content_tokens(&candidate);
                if end > start && tokens > max_tokens {
                    break;
                }
                end += 1;
                if tokens >= max_tokens {
                    break;
                }
            }

            // Ensure minimum size
            if end == start {
                end = start + 1;
            }

            let chunk_content: String = lines[start..end].join("\n");
            if end == start + 1
                && crate::token_count::estimate_content_tokens(&chunk_content) > max_tokens
            {
                for (byte_offset, piece) in self.split_oversized_line(&chunk_content) {
                    let mut metadata = ChunkMetadata::new(
                        shared_path.clone(),
                        start + 1,
                        end,
                        ChunkType::CodeBlock,
                        shared_lang.clone(),
                        &piece,
                    );
                    metadata.byte_offset = byte_offset;
                    chunks.push(CodeChunk::new(piece, metadata));
                }
                start = end;
                continue;
            }
            let metadata = ChunkMetadata::new(
                shared_path.clone(),
                start + 1,
                end,
                ChunkType::CodeBlock,
                shared_lang.clone(),
                &chunk_content,
            );
            chunks.push(CodeChunk::new(chunk_content, metadata));

            // Move start with a measured overlap, always advancing at least
            // one line so a large overlap cannot loop forever.
            if end >= lines.len() {
                break;
            }
            let mut overlap_start = end;
            while overlap_start > start + 1 {
                let candidate = lines[overlap_start - 1..end].join("\n");
                if crate::token_count::estimate_content_tokens(&candidate) > self.overlap {
                    break;
                }
                overlap_start -= 1;
            }
            start = overlap_start.max(start + 1);
        }

        chunks
    }

    /// Auto-detect language and chunk appropriately
    pub fn chunk(&self, content: &str, file_path: &Path) -> Vec<CodeChunk> {
        let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let chunks = match ext {
            "rs" => self.chunk_rust(content, file_path),
            _ => self.chunk_fixed_size(content, file_path, ext),
        };

        let mut bounded = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            if crate::token_count::estimate_content_tokens(&chunk.content)
                <= self.max_chunk_size.max(1)
            {
                bounded.push(chunk);
                continue;
            }

            let base_line = chunk.metadata.start_line.saturating_sub(1);
            let chunk_type = chunk.metadata.chunk_type;
            let symbol_name = chunk.metadata.symbol_name.clone();
            let mut pieces = self.chunk_fixed_size(
                &chunk.content,
                chunk.metadata.file_path.as_ref(),
                chunk.metadata.language.as_ref(),
            );
            for (piece_index, piece) in pieces.iter_mut().enumerate() {
                piece.metadata.start_line += base_line;
                piece.metadata.end_line += base_line;
                piece.metadata.chunk_type = chunk_type;
                if piece_index == 0 {
                    piece.metadata.symbol_name.clone_from(&symbol_name);
                }
                piece.refresh_id();
            }
            bounded.extend(pieces);
        }
        bounded
    }
}

/// HTTP embedding provider that calls an OpenAI-compatible `/v1/embeddings` endpoint.
pub struct HttpEmbeddingProvider {
    endpoint: String,
    model: String,
    /// Zero means the endpoint's first successful response will establish the
    /// dimension.  Keeping this atomic makes concurrent first requests agree
    /// on one observed value instead of racing different index shapes.
    dimension: AtomicUsize,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl HttpEmbeddingProvider {
    /// Create a new HTTP embedding provider.
    ///
    /// `endpoint` should be the base URL (e.g. `http://192.168.137.1:1234/v1`).
    /// `model` is the embedding model name (e.g. `text-embedding-nomic-embed-text-v1.5`).
    /// `dimension` is the expected embedding vector size (e.g. 768 for nomic-embed).
    pub fn new(endpoint: impl Into<String>, model: impl Into<String>, dimension: usize) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("static embedding HTTP client configuration must be valid");
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            dimension: AtomicUsize::new(dimension),
            api_key: None,
            client,
        }
    }

    /// Attach a bearer token for endpoints that require auth (e.g. OpenRouter).
    pub fn with_api_key(mut self, api_key: Option<String>) -> Self {
        self.api_key = api_key;
        self
    }

    fn request(&self, body: &serde_json::Value) -> Result<reqwest::RequestBuilder> {
        let url = format!("{}/embeddings", self.endpoint.trim_end_matches('/'));
        let req = self.client.post(&url).json(body);
        // Route through the shared authenticated-request choke point: it
        // refuses to send the key over plaintext HTTP to a remote host (or
        // via a userinfo URL) before attaching the bearer token.
        crate::config::api_key::authorize_request(req, &self.endpoint, self.api_key.as_deref())
    }

    fn accept_dimension(&self, observed: usize) -> Result<()> {
        if observed == 0 {
            anyhow::bail!("Embedding endpoint returned an empty vector");
        }
        match self
            .dimension
            .compare_exchange(0, observed, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => Ok(()),
            Err(expected) if expected == observed => Ok(()),
            Err(expected) => anyhow::bail!(
                "Embedding dimension mismatch: expected {}, got {}",
                expected,
                observed
            ),
        }
    }

    async fn bounded_body(
        mut response: reqwest::Response,
    ) -> Result<(reqwest::StatusCode, String)> {
        const MAX_EMBEDDING_RESPONSE_BYTES: usize = 64 * 1024 * 1024;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_EMBEDDING_RESPONSE_BYTES as u64)
        {
            anyhow::bail!(
                "Embedding response exceeds {} byte limit",
                MAX_EMBEDDING_RESPONSE_BYTES
            );
        }

        let status = response.status();
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .context("Failed to read embedding response")?
        {
            if body.len().saturating_add(chunk.len()) > MAX_EMBEDDING_RESPONSE_BYTES {
                anyhow::bail!(
                    "Embedding response exceeds {} byte limit",
                    MAX_EMBEDDING_RESPONSE_BYTES
                );
            }
            body.extend_from_slice(&chunk);
        }
        let text = String::from_utf8(body).context("Embedding response was not valid UTF-8")?;
        Ok((status, text))
    }

    fn parse_embeddings(
        json: &serde_json::Value,
        expected_count: usize,
        expected_dimension: usize,
    ) -> Result<Vec<Vec<f32>>> {
        let data = json["data"]
            .as_array()
            .context("Missing data array in embedding response")?;
        if data.len() != expected_count {
            anyhow::bail!(
                "Embedding response count mismatch: expected {}, got {}",
                expected_count,
                data.len()
            );
        }

        let indexed = data.iter().any(|item| item.get("index").is_some());
        let mut ordered: Vec<Option<Vec<f32>>> = vec![None; expected_count];
        let mut observed_dimension = None;
        for (position, item) in data.iter().enumerate() {
            let response_index = if indexed {
                item.get("index")
                    .and_then(serde_json::Value::as_u64)
                    .context("Every batch embedding item must have an integer index")?
                    as usize
            } else {
                position
            };
            if response_index >= expected_count {
                anyhow::bail!(
                    "Embedding response index {} is out of range for {} inputs",
                    response_index,
                    expected_count
                );
            }
            if ordered[response_index].is_some() {
                anyhow::bail!("Duplicate embedding response index {}", response_index);
            }

            let values = item["embedding"]
                .as_array()
                .context("Missing embedding in response item")?;
            let embedding = values
                .iter()
                .enumerate()
                .map(|(component, value)| {
                    let value = value.as_f64().with_context(|| {
                        format!("Embedding component {} is not numeric", component)
                    })? as f32;
                    if !value.is_finite() {
                        anyhow::bail!("Embedding component {} is not finite", component);
                    }
                    Ok(value)
                })
                .collect::<Result<Vec<_>>>()?;
            if expected_dimension > 0 && embedding.len() != expected_dimension {
                anyhow::bail!(
                    "Embedding dimension mismatch: expected {}, got {}",
                    expected_dimension,
                    embedding.len()
                );
            }
            if let Some(observed) = observed_dimension {
                if embedding.len() != observed {
                    anyhow::bail!(
                        "Embedding batch contains mixed dimensions: {} and {}",
                        observed,
                        embedding.len()
                    );
                }
            } else {
                observed_dimension = Some(embedding.len());
            }
            ordered[response_index] = Some(embedding);
        }

        ordered
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                value.ok_or_else(|| anyhow!("Missing embedding response index {}", index))
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for HttpEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let body = serde_json::json!({
            "model": self.model,
            "input": text,
        });
        let resp = self
            .request(&body)?
            .send()
            .await
            .context("HTTP embedding request failed")?;
        let (status, body_text) = Self::bounded_body(resp).await?;
        if !status.is_success() {
            anyhow::bail!("Embedding endpoint returned {}: {}", status, body_text);
        }
        let json: serde_json::Value =
            serde_json::from_str(&body_text).context("Failed to parse embedding response")?;
        let mut embeddings =
            Self::parse_embeddings(&json, 1, self.dimension.load(Ordering::Acquire))?;
        let embedding = embeddings
            .pop()
            .ok_or_else(|| anyhow!("Embedding response did not contain a vector"))?;
        self.accept_dimension(embedding.len())?;
        Ok(embedding)
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let body = serde_json::json!({
            "model": self.model,
            "input": texts,
        });
        let resp = self
            .request(&body)?
            .send()
            .await
            .context("HTTP batch embedding request failed")?;
        let (status, body_text) = Self::bounded_body(resp).await?;
        if !status.is_success() {
            anyhow::bail!("Embedding endpoint returned {}: {}", status, body_text);
        }
        let json: serde_json::Value =
            serde_json::from_str(&body_text).context("Failed to parse batch embedding response")?;
        let results =
            Self::parse_embeddings(&json, texts.len(), self.dimension.load(Ordering::Acquire))?;
        let observed = results
            .first()
            .map(Vec::len)
            .ok_or_else(|| anyhow!("Embedding response did not contain vectors"))?;
        self.accept_dimension(observed)?;
        Ok(results)
    }

    fn dimension(&self) -> usize {
        self.dimension.load(Ordering::Acquire)
    }
}

/// Enum dispatch for embedding providers.
///
/// Prefer using this enum over `Arc<dyn EmbeddingProvider>` trait objects.
/// It avoids dynamic dispatch overhead and is easier to reason about.
pub enum EmbeddingBackend {
    /// Mock provider for testing (deterministic hash-based embeddings)
    Mock(MockEmbeddingProvider),
    /// TF-IDF based embedding provider (no external dependencies)
    TfIdf(TfIdfEmbeddingProvider),
    /// HTTP provider calling an OpenAI-compatible `/v1/embeddings` endpoint
    Http(HttpEmbeddingProvider),
}

impl EmbeddingBackend {
    /// Generate embedding for text
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        match self {
            Self::Mock(p) => p.embed(text).await,
            Self::TfIdf(p) => p.embed(text).await,
            Self::Http(p) => p.embed(text).await,
        }
    }

    /// Generate embeddings for multiple texts
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        match self {
            Self::Mock(p) => p.embed_batch(texts).await,
            Self::TfIdf(p) => p.embed_batch(texts).await,
            Self::Http(p) => p.embed_batch(texts).await,
        }
    }

    /// Get embedding dimension
    pub fn dimension(&self) -> usize {
        match self {
            Self::Mock(p) => p.dimension(),
            Self::TfIdf(p) => p.dimension(),
            Self::Http(p) => p.dimension(),
        }
    }

    fn accept_dimension(&self, observed: usize) -> Result<()> {
        match self {
            Self::Http(provider) => provider.accept_dimension(observed),
            _ if self.dimension() == observed => Ok(()),
            _ => anyhow::bail!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.dimension(),
                observed
            ),
        }
    }
}

const VECTOR_STORE_FORMAT_VERSION: u32 = 1;
const VECTOR_STORE_MANIFEST_FILE: &str = ".vector-store-manifest.json";
const VECTOR_STORE_COLLECTIONS_FILE: &str = "collections.json";
const VECTOR_STORE_INDICES_FILE: &str = "indices.bin";
const VECTOR_STORE_GENERATION_PREFIX: &str = ".vector-store-generation-";

#[derive(Debug, Serialize, Deserialize)]
struct VectorStoreManifest {
    version: u32,
    generation: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedCollections {
    version: u32,
    collections: HashMap<String, VectorCollection>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedVectorIndex {
    dimension: usize,
    embeddings: Vec<Vec<f32>>,
    chunk_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedIndices {
    version: u32,
    indices: HashMap<String, PersistedVectorIndex>,
}

/// Main vector store
pub struct VectorStore {
    /// Collections by name
    collections: HashMap<String, VectorCollection>,
    /// Vector indices by collection name
    indices: HashMap<String, VectorIndex>,
    /// Embedding provider
    provider: Arc<EmbeddingBackend>,
    /// Storage path for persistence
    storage_path: Option<PathBuf>,
    /// Code chunker
    chunker: CodeChunker,
}

impl VectorStore {
    /// Create new vector store
    pub fn new(provider: Arc<EmbeddingBackend>) -> Self {
        Self {
            collections: HashMap::new(),
            indices: HashMap::new(),
            provider,
            storage_path: None,
            chunker: CodeChunker::default(),
        }
    }

    /// Set storage path for persistence
    pub fn with_storage(mut self, path: impl Into<PathBuf>) -> Self {
        self.storage_path = Some(path.into());
        self
    }

    /// Configure the chunker used for subsequent file indexing.
    pub fn with_chunker(mut self, chunker: CodeChunker) -> Self {
        self.chunker = chunker;
        self
    }

    /// Create or get collection
    pub fn collection(&mut self, name: &str, scope: CollectionScope) -> &mut VectorCollection {
        if !self.collections.contains_key(name) {
            let collection = VectorCollection::new(name, scope);
            let index = VectorIndex::new(self.provider.dimension());
            self.collections.insert(name.to_string(), collection);
            self.indices.insert(name.to_string(), index);
        }
        self.collections
            .get_mut(name)
            .unwrap_or_else(|| unreachable!("collection was just inserted"))
    }

    /// Get collection by name
    pub fn get_collection(&self, name: &str) -> Option<&VectorCollection> {
        self.collections.get(name)
    }

    /// List all collections
    pub fn list_collections(&self) -> Vec<&str> {
        self.collections.keys().map(|s| s.as_str()).collect()
    }

    /// Delete a collection from the live store.
    ///
    /// Persistence is snapshot-based: callers that configured storage must
    /// call [`Self::save`] to durably publish the deletion. Keeping deletion
    /// and persistence explicit also lets a full-index rebuild replace the
    /// in-memory collection without briefly publishing an empty snapshot.
    pub fn delete_collection(&mut self, name: &str) -> Option<VectorCollection> {
        self.indices.remove(name);
        self.collections.remove(name)
    }

    /// Create an empty, non-persistent store with the same embedding provider
    /// and chunking policy for staging a rebuild.
    ///
    /// Keeping staging outside the live maps means cancellation simply drops
    /// the partial store; it cannot leak a temporary collection into a later
    /// persistence snapshot.
    pub(crate) fn staging_store(&self) -> Self {
        Self {
            collections: HashMap::new(),
            indices: HashMap::new(),
            provider: Arc::clone(&self.provider),
            storage_path: None,
            chunker: self.chunker.clone(),
        }
    }

    /// Atomically publish a fully built collection from a staging store.
    ///
    /// Both halves of the staged collection are checked before either live map
    /// is changed. Callers can therefore keep the last-good live collection
    /// searchable until every file and embedding has succeeded.
    pub(crate) fn publish_staged_collection(
        &mut self,
        mut staged: Self,
        collection_name: &str,
    ) -> Result<()> {
        if !staged.collections.contains_key(collection_name) {
            anyhow::bail!("Staging collection not found: {collection_name}");
        }
        if !staged.indices.contains_key(collection_name) {
            anyhow::bail!("Index for staging collection not found: {collection_name}");
        }

        // No fallible work remains after these removals. The caller holds
        // `&mut self`, so no observer can see the two map insertions separately.
        let collection = staged
            .collections
            .remove(collection_name)
            .expect("staging collection existence checked above");
        let index = staged
            .indices
            .remove(collection_name)
            .expect("staging index existence checked above");
        self.collections
            .insert(collection_name.to_string(), collection);
        self.indices.insert(collection_name.to_string(), index);
        Ok(())
    }

    fn validate_embedding_values(
        embeddings: &[Vec<f32>],
        expected_count: usize,
    ) -> Result<Option<usize>> {
        if embeddings.len() != expected_count {
            anyhow::bail!(
                "Embedding response count mismatch: expected {}, got {}",
                expected_count,
                embeddings.len()
            );
        }
        let Some(first) = embeddings.first() else {
            return Ok(None);
        };
        let dimension = first.len();
        if dimension == 0 {
            anyhow::bail!("Embedding batch contains an empty vector");
        }
        for (index, embedding) in embeddings.iter().enumerate() {
            if embedding.len() != dimension {
                anyhow::bail!(
                    "Embedding batch item {} has dimension {}, expected {}",
                    index,
                    embedding.len(),
                    dimension
                );
            }
            if embedding.iter().any(|value| !value.is_finite()) {
                anyhow::bail!("Embedding batch item {} contains a non-finite value", index);
            }
        }
        Ok(Some(dimension))
    }

    fn validate_embedding_batch(
        &self,
        embeddings: &[Vec<f32>],
        expected_count: usize,
    ) -> Result<usize> {
        let observed = Self::validate_embedding_values(embeddings, expected_count)?;
        if let Some(dimension) = observed {
            // Dynamic providers learn their width only after every vector in
            // the response has passed cardinality, shape, and finite-value
            // validation. A malformed response must not pin future requests.
            self.provider.accept_dimension(dimension)?;
            Ok(dimension)
        } else {
            Ok(self.provider.dimension())
        }
    }

    /// Remove a file from both the persisted collection and its HNSW index.
    pub fn remove_file(&mut self, collection_name: &str, file_path: &Path) -> Result<usize> {
        let chunk_ids = self
            .collections
            .get(collection_name)
            .with_context(|| format!("collection '{}' not found", collection_name))?
            .chunk_ids_for_file(file_path);
        let index = self
            .indices
            .get_mut(collection_name)
            .with_context(|| format!("index for collection '{}' not found", collection_name))?;
        for chunk_id in &chunk_ids {
            index.remove(chunk_id);
        }
        self.collections
            .get_mut(collection_name)
            .expect("collection existence checked above")
            .remove_file(file_path);
        Ok(chunk_ids.len())
    }

    /// Index a file into a collection
    pub async fn index_file(&mut self, collection_name: &str, file_path: &Path) -> Result<usize> {
        let content = String::from_utf8(Self::read_regular_file(file_path, "indexed source file")?)
            .with_context(|| {
                format!("Indexed source file is not UTF-8: {}", file_path.display())
            })?;
        let chunks = self.chunker.chunk(&content, file_path);
        let chunk_count = chunks.len();

        // Generate embeddings
        let texts: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();
        let embeddings = self.provider.embed_batch(&texts).await?;
        let embedding_dimension = self.validate_embedding_batch(&embeddings, chunk_count)?;

        // Get or create collection
        if !self.collections.contains_key(collection_name) {
            self.collection(collection_name, CollectionScope::Project);
        }

        let old_ids = self
            .collections
            .get(collection_name)
            .with_context(|| format!("collection '{}' not found after creation", collection_name))?
            .chunk_ids_for_file(file_path);
        let collection_len = self
            .collections
            .get(collection_name)
            .map(VectorCollection::len)
            .unwrap_or_default();
        let projected_len = collection_len
            .saturating_sub(old_ids.len())
            .saturating_add(chunk_count);
        if projected_len > MAX_CHUNKS {
            anyhow::bail!(
                "Collection {} would exceed its {} chunk limit",
                collection_name,
                MAX_CHUNKS
            );
        }

        let index = self
            .indices
            .get_mut(collection_name)
            .with_context(|| format!("index for collection '{}' not found", collection_name))?;
        if index.is_empty() && index.dimension == 0 && embedding_dimension > 0 {
            *index = VectorIndex::new(embedding_dimension);
        }
        if embedding_dimension > 0 && index.dimension != embedding_dimension {
            anyhow::bail!(
                "Index dimension mismatch: expected {}, got {}",
                index.dimension,
                embedding_dimension
            );
        }

        // All fallible generation and validation is complete. Replace the old
        // file in both stores only now, preserving the last good version if an
        // embedding request failed or returned a malformed batch.
        for chunk_id in &old_ids {
            index.remove(chunk_id);
        }
        let collection = self.collections.get_mut(collection_name).with_context(|| {
            format!("collection '{}' not found after creation", collection_name)
        })?;
        collection.remove_file(file_path);

        // Add chunks with embeddings
        for (chunk, embedding) in chunks.into_iter().zip(embeddings) {
            let chunk_id = chunk.id.clone();
            let chunk = chunk.with_embedding(embedding.clone());
            collection.add_chunk(chunk)?;
            index.add(chunk_id, embedding)?;
        }

        Ok(chunk_count)
    }

    /// Rebuild the index for a collection from its stored chunks.
    ///
    /// This discards the current index and reconstructs it by re-embedding
    /// every chunk in the collection. Useful when `check_health()` reports
    /// `IndexHealth::Corrupt`.
    pub async fn rebuild_index(&mut self, collection_name: &str) -> Result<()> {
        let collection = self
            .collections
            .get(collection_name)
            .ok_or_else(|| anyhow!("Collection not found: {}", collection_name))?;

        let texts: Vec<String> = collection
            .chunks()
            .iter()
            .map(|c| c.content.clone())
            .collect();
        let ids: Vec<String> = collection.chunks().iter().map(|c| c.id.clone()).collect();

        let embeddings = self.provider.embed_batch(&texts).await?;
        let dimension = self.validate_embedding_batch(&embeddings, ids.len())?;

        let mut new_index = VectorIndex::new(dimension);
        for (id, embedding) in ids.into_iter().zip(embeddings) {
            new_index.add(id, embedding)?;
        }

        self.indices.insert(collection_name.to_string(), new_index);
        warn!(
            "Rebuilt vector index for collection '{}' ({} vectors)",
            collection_name,
            texts.len()
        );
        Ok(())
    }

    /// Build `SearchResult` entries from raw `(chunk_id, score)` pairs.
    fn build_search_results(
        collection: &VectorCollection,
        raw_results: Vec<(String, f32)>,
        k: usize,
        filter: Option<&SearchFilter>,
    ) -> Vec<SearchResult> {
        let mut search_results = Vec::new();
        for (chunk_id, score) in raw_results {
            if let Some(chunk) = collection.get_chunk(&chunk_id) {
                if let Some(filter) = filter {
                    if !filter.matches(chunk) {
                        continue;
                    }
                    if let Some(min_score) = filter.min_score {
                        if score < min_score {
                            continue;
                        }
                    }
                }

                let weighted_score = score * chunk.metadata.chunk_type.weight();
                search_results.push(SearchResult {
                    chunk: chunk.clone(),
                    score: weighted_score,
                    distance: 1.0 - score,
                });
            }
        }

        search_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        search_results.truncate(k);
        search_results
    }

    /// Filter-aware search with progressive candidate expansion.
    ///
    /// When a [`SearchFilter`] rejects most of the top candidates, a fixed
    /// `k*2` pool can starve the result set.  This helper starts with a
    /// pool of `k*2` candidates and doubles it until either enough results
    /// pass the filter **or** the entire index has been examined.
    ///
    /// Returns `(results, all_nan)` where `all_nan` is `true` when every
    /// raw similarity score from the initial search was `NaN` (a corruption
    /// indicator that callers can use to trigger an index rebuild).
    fn search_filtered(
        index: &VectorIndex,
        query_embedding: &[f32],
        k: usize,
        filter: Option<&SearchFilter>,
        collection: &VectorCollection,
    ) -> (Vec<SearchResult>, bool) {
        let total = index.len();
        if total == 0 || k == 0 {
            return (Vec::new(), false);
        }

        let mut pool = (k * 2).max(k);

        loop {
            let raw = index.search(query_embedding, pool.min(total));

            // Detect corruption on the initial (smallest) search: if every
            // score is NaN the index is corrupt and expansion won't help.
            let all_nan = !raw.is_empty() && raw.iter().all(|(_, score)| score.is_nan());
            if all_nan {
                return (Vec::new(), true);
            }

            let results = Self::build_search_results(collection, raw, k, filter);

            // Enough results, or we've already searched the entire index.
            if results.len() >= k || pool >= total {
                return (results, false);
            }

            // Expand the candidate pool and retry.
            pool = pool.saturating_mul(2);
        }
    }

    /// Search across collection.
    ///
    /// If all raw similarity scores are NaN a warning is logged. Callers
    /// that hold a mutable reference can use [`Self::search_or_rebuild`] instead
    /// to automatically rebuild the index and retry.
    pub async fn search(
        &self,
        collection_name: &str,
        query: &str,
        k: usize,
        filter: Option<&SearchFilter>,
    ) -> Result<Vec<SearchResult>> {
        let collection = self
            .collections
            .get(collection_name)
            .ok_or_else(|| anyhow!("Collection not found: {}", collection_name))?;

        let index = self
            .indices
            .get(collection_name)
            .ok_or_else(|| anyhow!("Index not found: {}", collection_name))?;

        let query_embedding = self.provider.embed(query).await?;

        let (results, all_nan) =
            Self::search_filtered(index, &query_embedding, k, filter, collection);

        // Detect corruption: all raw similarity scores are NaN
        if all_nan {
            warn!(
                "All search scores are NaN for collection '{}' — index may be corrupt; \
                 consider calling search_or_rebuild()",
                collection_name
            );
        }

        Ok(results)
    }

    /// Search with automatic index rebuild on corruption.
    ///
    /// If the initial search produces only NaN scores the index is rebuilt
    /// from the source chunks and the search is retried once.
    pub async fn search_or_rebuild(
        &mut self,
        collection_name: &str,
        query: &str,
        k: usize,
        filter: Option<&SearchFilter>,
    ) -> Result<Vec<SearchResult>> {
        let query_embedding = self.provider.embed(query).await?;

        let (results, all_nan) = {
            let index = self
                .indices
                .get(collection_name)
                .ok_or_else(|| anyhow!("Index not found: {}", collection_name))?;
            let collection = self
                .collections
                .get(collection_name)
                .ok_or_else(|| anyhow!("Collection not found: {}", collection_name))?;
            Self::search_filtered(index, &query_embedding, k, filter, collection)
        };

        let results = if all_nan {
            warn!(
                "All search scores are NaN for collection '{}' — rebuilding index",
                collection_name
            );
            self.rebuild_index(collection_name).await?;
            let index = self
                .indices
                .get(collection_name)
                .ok_or_else(|| anyhow!("Index not found after rebuild: {}", collection_name))?;
            let collection = self
                .collections
                .get(collection_name)
                .ok_or_else(|| anyhow!("Collection not found: {}", collection_name))?;
            let (results, _) =
                Self::search_filtered(index, &query_embedding, k, filter, collection);
            results
        } else {
            results
        };

        Ok(results)
    }

    fn validate_generation_id(generation: &str) -> Result<()> {
        if generation.len() != 32 || !generation.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            anyhow::bail!("Invalid vector-store generation identifier");
        }
        Ok(())
    }

    fn generation_path(storage_path: &Path, generation: &str) -> Result<PathBuf> {
        Self::validate_generation_id(generation)?;
        Ok(storage_path.join(format!("{}{}", VECTOR_STORE_GENERATION_PREFIX, generation)))
    }

    fn write_new_file(path: &Path, bytes: &[u8]) -> Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(path)
            .with_context(|| format!("Failed to create vector-store file {:?}", path))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    }

    fn sync_directory(path: &Path) -> Result<()> {
        #[cfg(unix)]
        {
            std::fs::File::open(path)?.sync_all()?;
        }
        #[cfg(not(unix))]
        let _ = path;
        Ok(())
    }

    fn validate_collection_index(
        name: &str,
        collection: &VectorCollection,
        index: &PersistedVectorIndex,
    ) -> Result<()> {
        if collection.name != name {
            anyhow::bail!(
                "Collection key '{}' does not match persisted name '{}'",
                name,
                collection.name
            );
        }

        let collection_ids: HashSet<&str> = collection
            .chunks
            .iter()
            .map(|chunk| chunk.id.as_str())
            .collect();
        if collection_ids.len() != collection.chunks.len() {
            anyhow::bail!("Collection '{}' contains duplicate chunk IDs", name);
        }
        for chunk in &collection.chunks {
            let mut hasher = Sha256::new();
            hasher.update(chunk.content.as_bytes());
            let observed_hash = hex::encode(hasher.finalize());
            if chunk.metadata.content_hash != observed_hash {
                anyhow::bail!(
                    "Collection '{}' contains a chunk whose content hash does not match its content",
                    name
                );
            }
            if chunk.id != CodeChunk::stable_id(&chunk.metadata) {
                anyhow::bail!(
                    "Collection '{}' contains a chunk whose ID does not match its metadata",
                    name
                );
            }
            if chunk.metadata.start_line == 0 || chunk.metadata.end_line < chunk.metadata.start_line
            {
                anyhow::bail!("Collection '{}' contains an invalid chunk line range", name);
            }
        }
        let index_ids: HashSet<&str> = index.chunk_ids.iter().map(String::as_str).collect();
        if index_ids.len() != index.chunk_ids.len() {
            anyhow::bail!("Vector index '{}' contains duplicate chunk IDs", name);
        }
        if collection_ids != index_ids {
            anyhow::bail!(
                "Collection '{}' chunk IDs do not exactly match its vector index",
                name
            );
        }

        let observed = Self::validate_embedding_values(&index.embeddings, index.chunk_ids.len())?;
        match observed {
            Some(observed) if index.dimension != observed => anyhow::bail!(
                "Vector index '{}' declares dimension {}, but contains dimension {}",
                name,
                index.dimension,
                observed
            ),
            _ => {}
        }
        Ok(())
    }

    fn stage_snapshot(
        &self,
        mut collections: HashMap<String, VectorCollection>,
        persisted_indices: HashMap<String, PersistedVectorIndex>,
    ) -> Result<(
        HashMap<String, VectorCollection>,
        HashMap<String, VectorIndex>,
    )> {
        let collection_names: HashSet<&str> = collections.keys().map(String::as_str).collect();
        let index_names: HashSet<&str> = persisted_indices.keys().map(String::as_str).collect();
        if collection_names != index_names {
            anyhow::bail!("Persisted collection and vector-index name sets do not match");
        }

        let mut learned_dimension = None;
        for (name, collection) in &collections {
            let persisted_index = persisted_indices
                .get(name)
                .ok_or_else(|| anyhow!("Missing vector index for collection '{}'", name))?;
            Self::validate_collection_index(name, collection, persisted_index)?;
            if persisted_index.dimension > 0 {
                if let Some(previous) = learned_dimension {
                    if previous != persisted_index.dimension {
                        anyhow::bail!(
                            "Persisted vector indexes use mixed dimensions: {} and {}",
                            previous,
                            persisted_index.dimension
                        );
                    }
                } else {
                    learned_dimension = Some(persisted_index.dimension);
                }
            }
        }

        if let Some(dimension) = learned_dimension {
            let configured = self.provider.dimension();
            if configured != 0 && configured != dimension {
                anyhow::bail!(
                    "Embedding dimension mismatch: expected {}, got {}",
                    configured,
                    dimension
                );
            }
        }

        // Build every HNSW index before changing either the provider or the
        // live maps. A corrupt later collection therefore cannot leave a
        // partially loaded store.
        let mut indices = HashMap::with_capacity(persisted_indices.len());
        for (name, persisted) in persisted_indices {
            let mut index = VectorIndex::new(persisted.dimension);
            for (chunk_id, embedding) in persisted.chunk_ids.into_iter().zip(persisted.embeddings) {
                index.add(chunk_id, embedding)?;
            }
            indices.insert(name, index);
        }
        for collection in collections.values_mut() {
            collection.rebuild_id_index();
        }

        // This is the only fallible state mutation, and it happens after the
        // full snapshot has parsed, validated, and built successfully.
        if let Some(dimension) = learned_dimension {
            self.provider.accept_dimension(dimension)?;
        }
        Ok((collections, indices))
    }

    fn load_generation(
        &self,
        storage_path: &Path,
        manifest: VectorStoreManifest,
    ) -> Result<(
        HashMap<String, VectorCollection>,
        HashMap<String, VectorIndex>,
    )> {
        if manifest.version != VECTOR_STORE_FORMAT_VERSION {
            anyhow::bail!(
                "Unsupported vector-store manifest version {}",
                manifest.version
            );
        }
        let generation_path = Self::generation_path(storage_path, &manifest.generation)?;
        let generation_metadata = std::fs::symlink_metadata(&generation_path)
            .context("Failed to inspect vector-store generation")?;
        if !generation_metadata.file_type().is_dir() {
            anyhow::bail!("Vector-store generation is not a real directory");
        }
        let collection_bytes = Self::read_regular_file(
            &generation_path.join(VECTOR_STORE_COLLECTIONS_FILE),
            "vector-store collection generation",
        )?;
        let persisted_collections: PersistedCollections = serde_json::from_slice(&collection_bytes)
            .context("Failed to parse vector-store collection generation")?;
        if persisted_collections.version != VECTOR_STORE_FORMAT_VERSION {
            anyhow::bail!(
                "Unsupported vector-store collection version {}",
                persisted_collections.version
            );
        }

        let index_bytes = Self::read_regular_file(
            &generation_path.join(VECTOR_STORE_INDICES_FILE),
            "vector-store index generation",
        )?;
        let (persisted_indices, consumed): (PersistedIndices, usize) =
            bincode::serde::decode_from_slice(&index_bytes, bincode::config::standard())
                .context("Failed to parse vector-store index generation")?;
        if consumed != index_bytes.len() {
            anyhow::bail!("Vector-store index generation contains trailing data");
        }
        if persisted_indices.version != VECTOR_STORE_FORMAT_VERSION {
            anyhow::bail!(
                "Unsupported vector-store index version {}",
                persisted_indices.version
            );
        }
        self.stage_snapshot(persisted_collections.collections, persisted_indices.indices)
    }

    fn read_regular_file(path: &Path, label: &str) -> Result<Vec<u8>> {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(
                windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
            );
        }
        #[cfg(not(any(unix, windows)))]
        {
            let metadata = std::fs::symlink_metadata(path)
                .with_context(|| format!("Failed to inspect {label}"))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                anyhow::bail!("Refusing {label} that is a symlink or non-regular file");
            }
        }

        // Validate the opened handle rather than checking the path and then
        // opening it. On Unix O_NOFOLLOW also closes the final-component swap
        // window in which a repository file could become an external symlink
        // between enumeration and ingestion.
        let mut file = options
            .open(path)
            .with_context(|| format!("Failed to open {label}"))?;
        if !file
            .metadata()
            .with_context(|| format!("Failed to inspect opened {label}"))?
            .is_file()
        {
            anyhow::bail!("Refusing {label} that is a symlink or non-regular file");
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .with_context(|| format!("Failed to read {label}"))?;
        Ok(bytes)
    }

    /// Return whether the persisted chunks for `file_path` are exactly the
    /// chunks produced from the file's current bytes and the current chunker.
    /// Persisted RAG content is model-visible, so path eligibility alone is
    /// insufficient: a stale cache must not replay text that has since been
    /// removed from an otherwise allowed source file.
    pub(crate) fn file_chunks_match_current(
        &self,
        collection_name: &str,
        file_path: &Path,
    ) -> Result<bool> {
        let collection = self
            .collections
            .get(collection_name)
            .with_context(|| format!("Collection not found: {collection_name}"))?;
        let persisted_ids = collection.chunk_ids_for_file(file_path);
        let content = String::from_utf8(Self::read_regular_file(
            file_path,
            "current indexed source file",
        )?)
        .with_context(|| format!("Indexed source file is not UTF-8: {}", file_path.display()))?;
        let current_chunks = self.chunker.chunk(&content, file_path);
        if persisted_ids.len() != current_chunks.len() {
            return Ok(false);
        }
        let current_ids: HashSet<&str> = current_chunks
            .iter()
            .map(|chunk| chunk.id.as_str())
            .collect();
        Ok(persisted_ids
            .iter()
            .all(|chunk_id| current_ids.contains(chunk_id.as_str())))
    }

    fn load_legacy(
        &self,
        storage_path: &Path,
    ) -> Result<
        Option<(
            HashMap<String, VectorCollection>,
            HashMap<String, VectorIndex>,
        )>,
    > {
        let mut collections = HashMap::new();
        let mut indices = HashMap::new();
        for entry in std::fs::read_dir(storage_path)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json")
                || path.file_name().and_then(|name| name.to_str())
                    == Some(VECTOR_STORE_MANIFEST_FILE)
            {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| anyhow!("Invalid legacy collection file name"))?
                .to_string();
            let json =
                String::from_utf8(Self::read_regular_file(&path, "legacy vector collection")?)
                    .context("Legacy vector collection is not valid UTF-8")?;
            let collection: VectorCollection = serde_json::from_str(&json)
                .with_context(|| format!("Failed to parse legacy collection '{}'", name))?;

            let index_path = storage_path.join(format!("{}.idx", name));
            let data = Self::read_regular_file(&index_path, "legacy vector index")
                .with_context(|| format!("Missing legacy vector index for '{}'", name))?;
            let ((embeddings, chunk_ids), consumed): ((Vec<Vec<f32>>, Vec<String>), usize) =
                bincode::serde::decode_from_slice(&data, bincode::config::standard())
                    .with_context(|| format!("Failed to parse legacy vector index '{}'", name))?;
            if consumed != data.len() {
                anyhow::bail!("Legacy vector index '{}' contains trailing data", name);
            }
            let dimension = Self::validate_embedding_values(&embeddings, chunk_ids.len())?
                .unwrap_or(self.provider.dimension());
            collections.insert(name.clone(), collection);
            indices.insert(
                name,
                PersistedVectorIndex {
                    dimension,
                    embeddings,
                    chunk_ids,
                },
            );
        }

        if collections.is_empty() {
            return Ok(None);
        }
        self.stage_snapshot(collections, indices).map(Some)
    }

    fn write_generation(
        &self,
        temporary_generation: &Path,
        final_generation: &Path,
        persisted_indices: HashMap<String, PersistedVectorIndex>,
    ) -> Result<()> {
        let collections = PersistedCollections {
            version: VECTOR_STORE_FORMAT_VERSION,
            collections: self.collections.clone(),
        };
        let collection_bytes = serde_json::to_vec_pretty(&collections)?;
        Self::write_new_file(
            &temporary_generation.join(VECTOR_STORE_COLLECTIONS_FILE),
            &collection_bytes,
        )?;

        let indices = PersistedIndices {
            version: VECTOR_STORE_FORMAT_VERSION,
            indices: persisted_indices,
        };
        let index_bytes = bincode::serde::encode_to_vec(&indices, bincode::config::standard())?;
        Self::write_new_file(
            &temporary_generation.join(VECTOR_STORE_INDICES_FILE),
            &index_bytes,
        )?;
        Self::sync_directory(temporary_generation)?;
        std::fs::rename(temporary_generation, final_generation)
            .context("Failed to publish vector-store generation")?;
        let storage_path = final_generation
            .parent()
            .ok_or_else(|| anyhow!("Vector-store generation has no parent directory"))?;
        Self::sync_directory(storage_path)?;
        Ok(())
    }

    /// Remove only the generation that was authoritative before the current
    /// save. Sweeping every unreferenced directory could delete a concurrent
    /// writer's fully written generation before it publishes its manifest.
    fn prune_previous_generation(storage_path: &Path, previous: &str, current: &str) {
        if previous == current {
            return;
        }
        let previous_path = match Self::generation_path(storage_path, previous) {
            Ok(path) => path,
            Err(error) => {
                warn!("Skipping invalid previous vector-store generation: {error}");
                return;
            }
        };
        let metadata = match std::fs::symlink_metadata(&previous_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => {
                warn!(
                    "Failed to inspect previous vector-store generation {:?}: {}",
                    previous_path, error
                );
                return;
            }
        };

        // Never follow or unlink a symlink (or any unexpected non-directory)
        // merely because its name resembles one of our generations.
        if !metadata.file_type().is_dir() {
            warn!(
                "Skipping non-directory previous vector-store generation {:?}",
                previous_path
            );
            return;
        }
        if let Err(error) = std::fs::remove_dir_all(&previous_path) {
            warn!(
                "Failed to prune previous vector-store generation {:?}: {}",
                previous_path, error
            );
            return;
        }
        if let Err(error) = Self::sync_directory(storage_path) {
            warn!(
                "Failed to sync vector-store directory after pruning {:?}: {}",
                previous_path, error
            );
        }
    }

    /// Save the complete store as one immutable generation.
    ///
    /// The generation's JSON and index files are fully written and synced
    /// before one small manifest is atomically replaced. A crash can leave an
    /// unreferenced generation, but it cannot publish only one half of the
    /// collection/index pair.
    pub fn save(&self) -> Result<()> {
        let storage_path = self
            .storage_path
            .as_ref()
            .ok_or_else(|| anyhow!("Storage path not set"))?;
        std::fs::create_dir_all(storage_path)?;

        let manifest_path = storage_path.join(VECTOR_STORE_MANIFEST_FILE);
        let _lock = crate::session::checkpoint::FileLock::acquire(&manifest_path)?;
        let previous_generation = match std::fs::symlink_metadata(&manifest_path) {
            Ok(metadata) if metadata.file_type().is_file() => {
                let bytes = Self::read_regular_file(&manifest_path, "vector-store manifest")?;
                let manifest: VectorStoreManifest = serde_json::from_slice(&bytes)
                    .context("Failed to parse existing vector-store manifest")?;
                (manifest.version == VECTOR_STORE_FORMAT_VERSION).then_some(manifest.generation)
            }
            Ok(_) => anyhow::bail!("Vector-store manifest is not a regular file"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error).context("Failed to inspect vector-store manifest"),
        };

        let mut persisted_indices = HashMap::with_capacity(self.indices.len());
        for (name, index) in &self.indices {
            let (embeddings, chunk_ids) = index.live_data_owned();
            persisted_indices.insert(
                name.clone(),
                PersistedVectorIndex {
                    dimension: index.dimension,
                    embeddings,
                    chunk_ids,
                },
            );
        }
        for (name, collection) in &self.collections {
            let index = persisted_indices
                .get(name)
                .ok_or_else(|| anyhow!("Missing vector index for collection '{}'", name))?;
            Self::validate_collection_index(name, collection, index)?;
        }
        let collection_names: HashSet<&str> = self.collections.keys().map(String::as_str).collect();
        let index_names: HashSet<&str> = persisted_indices.keys().map(String::as_str).collect();
        if collection_names != index_names {
            anyhow::bail!("Live collection and vector-index name sets do not match");
        }

        let generation = uuid::Uuid::new_v4().simple().to_string();
        let final_generation = Self::generation_path(storage_path, &generation)?;
        let temporary_generation = storage_path.join(format!(
            ".vector-store-generation-{}.tmp",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir(&temporary_generation)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                &temporary_generation,
                std::fs::Permissions::from_mode(0o700),
            )?;
        }

        let write_generation =
            self.write_generation(&temporary_generation, &final_generation, persisted_indices);
        if let Err(error) = write_generation {
            let _ = std::fs::remove_dir_all(&temporary_generation);
            return Err(error);
        }

        let manifest = VectorStoreManifest {
            version: VECTOR_STORE_FORMAT_VERSION,
            generation,
        };
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
        let temporary_manifest = storage_path.join(format!(
            ".vector-store-manifest-{}.tmp",
            uuid::Uuid::new_v4().simple()
        ));
        Self::write_new_file(&temporary_manifest, &manifest_bytes)?;
        crate::session::checkpoint::replace_atomically(&temporary_manifest, &manifest_path)
            .context("Failed to publish vector-store manifest")?;
        Self::sync_directory(storage_path)?;
        if let Some(previous) = previous_generation {
            Self::prune_previous_generation(storage_path, &previous, &manifest.generation);
        }
        Ok(())
    }

    /// Load a fully validated store snapshot from disk.
    pub fn load(&mut self) -> Result<()> {
        let storage_path = self
            .storage_path
            .as_ref()
            .ok_or_else(|| anyhow!("Storage path not set"))?
            .clone();
        if !storage_path.exists() {
            return Ok(());
        }

        let manifest_path = storage_path.join(VECTOR_STORE_MANIFEST_FILE);
        let _lock = crate::session::checkpoint::FileLock::acquire(&manifest_path)?;
        let staged = if manifest_path.exists() {
            let manifest_bytes = Self::read_regular_file(&manifest_path, "vector-store manifest")?;
            let manifest: VectorStoreManifest = serde_json::from_slice(&manifest_bytes)
                .context("Failed to parse vector-store manifest")?;
            Some(self.load_generation(&storage_path, manifest)?)
        } else {
            self.load_legacy(&storage_path)?
        };

        if let Some((collections, indices)) = staged {
            self.collections = collections;
            self.indices = indices;
        }
        Ok(())
    }

    /// Get store statistics
    pub fn stats(&self) -> VectorStoreStats {
        let mut total_chunks = 0;
        let mut total_files = 0;
        let mut collections = Vec::new();

        for (name, collection) in &self.collections {
            total_chunks += collection.len();
            total_files += collection.files().len();
            collections.push(CollectionStats {
                name: name.clone(),
                chunk_count: collection.len(),
                file_count: collection.files().len(),
                scope: collection.scope,
            });
        }

        VectorStoreStats {
            total_chunks,
            total_files,
            collection_count: self.collections.len(),
            collections,
            embedding_dimension: self.provider.dimension(),
        }
    }
}

/// Statistics for vector store
#[derive(Debug, Clone)]
pub struct VectorStoreStats {
    pub total_chunks: usize,
    pub total_files: usize,
    pub collection_count: usize,
    pub collections: Vec<CollectionStats>,
    pub embedding_dimension: usize,
}

/// Statistics for a collection
#[derive(Debug, Clone)]
pub struct CollectionStats {
    pub name: String,
    pub chunk_count: usize,
    pub file_count: usize,
    pub scope: CollectionScope,
}

// ---------------------------------------------------------------------------
// BoundedVectorStore — capacity-limited wrapper with FIFO eviction
// ---------------------------------------------------------------------------

/// Default maximum number of items across all collections in a bounded store.
pub const DEFAULT_MAX_ITEMS: usize = 10_000;

/// A capacity-limited wrapper around [`VectorStore`] that evicts the oldest
/// items (FIFO order) when the total number of chunks exceeds `max_items`.
///
/// This prevents unbounded memory growth in long-running processes that
/// continuously index new files without explicitly pruning old data.
pub struct BoundedVectorStore {
    /// The underlying store that does the real work.
    inner: VectorStore,
    /// Maximum total chunks allowed across all collections.
    max_items: usize,
    /// Tracks insertion order for FIFO eviction.
    /// Each entry is `(collection_name, chunk_id)`.
    insertion_order: std::sync::Mutex<std::collections::VecDeque<(String, String)>>,
}

impl BoundedVectorStore {
    /// Create a new bounded store wrapping the given `VectorStore`.
    pub fn new(inner: VectorStore, max_items: usize) -> Self {
        Self {
            inner,
            max_items,
            insertion_order: std::sync::Mutex::new(std::collections::VecDeque::new()),
        }
    }

    /// Create with the default capacity ([`DEFAULT_MAX_ITEMS`]).
    pub fn with_default_capacity(inner: VectorStore) -> Self {
        Self::new(inner, DEFAULT_MAX_ITEMS)
    }

    /// Current total chunk count across all collections.
    pub fn len(&self) -> usize {
        self.inner.stats().total_chunks
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Maximum capacity.
    pub fn max_items(&self) -> usize {
        self.max_items
    }

    /// Clear all collections and the insertion-order tracker.
    pub fn clear(&mut self) {
        let names: Vec<String> = self
            .inner
            .list_collections()
            .iter()
            .map(|s| s.to_string())
            .collect();
        for name in names {
            self.inner.delete_collection(&name);
        }
        if let Ok(mut order) = self.insertion_order.lock() {
            order.clear();
        }
    }

    /// Evict the oldest items until total count is below `max_items`.
    fn evict_if_needed(&mut self) {
        let mut current = self.len();
        if current <= self.max_items {
            return;
        }

        let mut order = self
            .insertion_order
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        while current > self.max_items {
            if let Some((collection_name, chunk_id)) = order.pop_front() {
                // Remove from the collection
                if let Some(collection) = self.inner.collections.get_mut(&collection_name) {
                    if collection.remove_chunk(&chunk_id).is_some() {
                        // Also remove from the vector index
                        if let Some(index) = self.inner.indices.get_mut(&collection_name) {
                            index.remove(&chunk_id);
                        }
                        current -= 1;
                    }
                }
            } else {
                // No more tracked items; nothing to evict
                break;
            }
        }
    }

    /// Index a file, evicting oldest items if the store exceeds capacity.
    pub async fn index_file(&mut self, collection_name: &str, file_path: &Path) -> Result<usize> {
        let count = self.inner.index_file(collection_name, file_path).await?;

        // Record insertion order for the newly added chunks
        if let Some(collection) = self.inner.get_collection(collection_name) {
            let mut order = self
                .insertion_order
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            // The last `count` chunks in the collection are the newly added ones.
            let chunks = collection.chunks();
            let start = chunks.len().saturating_sub(count);
            for chunk in &chunks[start..] {
                order.push_back((collection_name.to_string(), chunk.id.clone()));
            }
        }

        self.evict_if_needed();
        Ok(count)
    }

    /// Get a reference to the inner `VectorStore`.
    pub fn inner(&self) -> &VectorStore {
        &self.inner
    }

    /// Delegate: create or get a collection.
    pub fn collection(&mut self, name: &str, scope: CollectionScope) -> &mut VectorCollection {
        self.inner.collection(name, scope)
    }

    /// Delegate: search across a collection.
    pub async fn search(
        &self,
        collection_name: &str,
        query: &str,
        k: usize,
        filter: Option<&SearchFilter>,
    ) -> Result<Vec<SearchResult>> {
        self.inner.search(collection_name, query, k, filter).await
    }

    /// Delegate: get store statistics.
    pub fn stats(&self) -> VectorStoreStats {
        self.inner.stats()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/analysis/vector_store/vector_store_test.rs"]
mod tests;
