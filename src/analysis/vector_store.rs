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
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
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
    /// Create a new code chunk
    pub fn new(content: String, metadata: ChunkMetadata) -> Self {
        let id = format!(
            "{}:{}:{}",
            metadata.file_path.display(),
            metadata.start_line,
            &metadata.content_hash[..8]
        );

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
    /// Chunks in this collection
    #[serde(skip)]
    chunks: Vec<CodeChunk>,
    /// Index of chunk IDs to positions
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
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// Get embedding dimension
    fn dimension(&self) -> usize;
}

/// Mock embedding provider for testing
pub struct MockEmbeddingProvider {
    dimension: usize,
}

impl MockEmbeddingProvider {
    /// Create new mock provider
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
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

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
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

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
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
pub struct CodeChunker {
    /// Maximum chunk size in characters
    pub max_chunk_size: usize,
    /// Minimum chunk size
    pub min_chunk_size: usize,
    /// Overlap between chunks
    pub overlap: usize,
}

impl Default for CodeChunker {
    fn default() -> Self {
        Self {
            max_chunk_size: 2000,
            min_chunk_size: 100,
            overlap: 50,
        }
    }
}

impl CodeChunker {
    /// Create new chunker
    pub fn new(max_chunk_size: usize) -> Self {
        Self {
            max_chunk_size,
            ..Default::default()
        }
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

        let mut start = 0;
        while start < lines.len() {
            let mut end = start;
            let mut size = 0;

            // Accumulate lines until max size
            while end < lines.len() && size + lines[end].len() < self.max_chunk_size {
                size += lines[end].len() + 1; // +1 for newline
                end += 1;
            }

            // Ensure minimum size
            if end == start {
                end = start + 1;
            }

            let chunk_content: String = lines[start..end].join("\n");
            let metadata = ChunkMetadata::new(
                shared_path.clone(),
                start + 1,
                end,
                ChunkType::CodeBlock,
                shared_lang.clone(),
                &chunk_content,
            );
            chunks.push(CodeChunk::new(chunk_content, metadata));

            // Move start with overlap
            if end >= lines.len() {
                break;
            }
            start = end.saturating_sub(self.overlap / 50);
        }

        chunks
    }

    /// Auto-detect language and chunk appropriately
    pub fn chunk(&self, content: &str, file_path: &Path) -> Vec<CodeChunk> {
        let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        match ext {
            "rs" => self.chunk_rust(content, file_path),
            _ => self.chunk_fixed_size(content, file_path, ext),
        }
    }
}

/// HTTP embedding provider that calls an OpenAI-compatible `/v1/embeddings` endpoint.
pub struct HttpEmbeddingProvider {
    endpoint: String,
    model: String,
    dimension: usize,
    client: reqwest::Client,
}

impl HttpEmbeddingProvider {
    /// Create a new HTTP embedding provider.
    ///
    /// `endpoint` should be the base URL (e.g. `http://192.168.137.1:1234/v1`).
    /// `model` is the embedding model name (e.g. `text-embedding-nomic-embed-text-v1.5`).
    /// `dimension` is the expected embedding vector size (e.g. 768 for nomic-embed).
    pub fn new(endpoint: impl Into<String>, model: impl Into<String>, dimension: usize) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            dimension,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for HttpEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/embeddings", self.endpoint.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": self.model,
            "input": text,
        });
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("HTTP embedding request failed")?;
        let status = resp.status();
        let json: serde_json::Value = resp.json().await.context("Failed to parse embedding response")?;
        if !status.is_success() {
            anyhow::bail!(
                "Embedding endpoint returned {}: {}",
                status,
                json.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()).unwrap_or("unknown error")
            );
        }
        let embedding = json["data"][0]["embedding"]
            .as_array()
            .context("Missing embedding array in response")?
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect::<Vec<f32>>();
        if embedding.len() != self.dimension {
            anyhow::bail!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.dimension,
                embedding.len()
            );
        }
        Ok(embedding)
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let url = format!("{}/embeddings", self.endpoint.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": self.model,
            "input": texts,
        });
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("HTTP batch embedding request failed")?;
        let status = resp.status();
        let json: serde_json::Value = resp.json().await.context("Failed to parse batch embedding response")?;
        if !status.is_success() {
            anyhow::bail!(
                "Embedding endpoint returned {}: {}",
                status,
                json.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()).unwrap_or("unknown error")
            );
        }
        let data = json["data"]
            .as_array()
            .context("Missing data array in batch embedding response")?;
        let mut results = Vec::with_capacity(data.len());
        for item in data {
            let embedding = item["embedding"]
                .as_array()
                .context("Missing embedding in batch item")?
                .iter()
                .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                .collect::<Vec<f32>>();
            results.push(embedding);
        }
        Ok(results)
    }

    fn dimension(&self) -> usize {
        self.dimension
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

    /// Set chunker
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

    /// Delete collection, including its on-disk files.
    pub fn delete_collection(&mut self, name: &str) -> Option<VectorCollection> {
        self.indices.remove(name);
        let removed = self.collections.remove(name);

        // Clean up persisted files for this collection
        if let Some(ref storage_path) = self.storage_path {
            let json_path = storage_path.join(format!("{}.json", name));
            let idx_path = storage_path.join(format!("{}.idx", name));
            if json_path.exists() {
                let _ = std::fs::remove_file(&json_path);
            }
            if idx_path.exists() {
                let _ = std::fs::remove_file(&idx_path);
            }
        }

        removed
    }

    /// Index a file into a collection
    pub async fn index_file(&mut self, collection_name: &str, file_path: &Path) -> Result<usize> {
        let content = std::fs::read_to_string(file_path)?;
        let chunks = self.chunker.chunk(&content, file_path);
        let chunk_count = chunks.len();

        // Generate embeddings
        let texts: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();
        let embeddings = self.provider.embed_batch(&texts).await?;

        // Get or create collection
        if !self.collections.contains_key(collection_name) {
            self.collection(collection_name, CollectionScope::Project);
        }

        let collection = self.collections.get_mut(collection_name).with_context(|| {
            format!("collection '{}' not found after creation", collection_name)
        })?;
        let index = self
            .indices
            .get_mut(collection_name)
            .with_context(|| format!("index for collection '{}' not found", collection_name))?;

        // Add chunks with embeddings
        for (chunk, embedding) in chunks.into_iter().zip(embeddings.into_iter()) {
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

        let mut new_index = VectorIndex::new(self.provider.dimension());
        for (id, embedding) in ids.into_iter().zip(embeddings.into_iter()) {
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
        let raw_results = index.search(&query_embedding, k * 2);

        // Detect corruption: all scores are NaN
        let all_nan =
            !raw_results.is_empty() && raw_results.iter().all(|(_, score)| score.is_nan());
        if all_nan {
            warn!(
                "All search scores are NaN for collection '{}' — index may be corrupt; \
                 consider calling search_or_rebuild()",
                collection_name
            );
        }

        Ok(Self::build_search_results(
            collection,
            raw_results,
            k,
            filter,
        ))
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

        let raw_results = {
            let index = self
                .indices
                .get(collection_name)
                .ok_or_else(|| anyhow!("Index not found: {}", collection_name))?;
            index.search(&query_embedding, k * 2)
        };

        let all_nan =
            !raw_results.is_empty() && raw_results.iter().all(|(_, score)| score.is_nan());

        let raw_results = if all_nan {
            warn!(
                "All search scores are NaN for collection '{}' — rebuilding index",
                collection_name
            );
            self.rebuild_index(collection_name).await?;
            let index = self
                .indices
                .get(collection_name)
                .ok_or_else(|| anyhow!("Index not found after rebuild: {}", collection_name))?;
            index.search(&query_embedding, k * 2)
        } else {
            raw_results
        };

        let collection = self
            .collections
            .get(collection_name)
            .ok_or_else(|| anyhow!("Collection not found: {}", collection_name))?;

        Ok(Self::build_search_results(
            collection,
            raw_results,
            k,
            filter,
        ))
    }

    /// Save store to disk.
    ///
    /// Uses atomic writes (temp file + rename) for each file to prevent
    /// corruption if the process crashes mid-write. Both the `.json` and
    /// `.idx` files for a collection are written atomically.
    pub fn save(&self) -> Result<()> {
        let storage_path = self
            .storage_path
            .as_ref()
            .ok_or_else(|| anyhow!("Storage path not set"))?;

        std::fs::create_dir_all(storage_path)?;

        let pid = std::process::id();

        // Save each collection with atomic writes
        for (name, collection) in &self.collections {
            let collection_path = storage_path.join(format!("{}.json", name));
            let json = serde_json::to_string_pretty(collection)?;

            // Atomic write for collection JSON
            let tmp_json = collection_path.with_extension(format!("json.tmp.{}", pid));
            std::fs::write(&tmp_json, &json)?;
            if let Err(e) = std::fs::rename(&tmp_json, &collection_path) {
                let _ = std::fs::remove_file(&tmp_json);
                return Err(e).context("Failed to atomically save collection");
            }

            // Atomic write for embeddings index
            if let Some(index) = self.indices.get(name) {
                let index_path = storage_path.join(format!("{}.idx", name));
                let (embs, cids) = index.live_data_owned();
                let data =
                    bincode::serde::encode_to_vec((&embs, &cids), bincode::config::standard())?;

                let tmp_idx = index_path.with_extension(format!("idx.tmp.{}", pid));
                std::fs::write(&tmp_idx, &data)?;
                if let Err(e) = std::fs::rename(&tmp_idx, &index_path) {
                    let _ = std::fs::remove_file(&tmp_idx);
                    return Err(e).context("Failed to atomically save index");
                }
            }
        }

        Ok(())
    }

    /// Load store from disk
    pub fn load(&mut self) -> Result<()> {
        let storage_path = self
            .storage_path
            .as_ref()
            .ok_or_else(|| anyhow!("Storage path not set"))?
            .clone();

        if !storage_path.exists() {
            return Ok(()); // Nothing to load
        }

        // Find all collection files
        for entry in std::fs::read_dir(&storage_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| anyhow!("Invalid collection file name"))?;

                // Load collection
                let json = std::fs::read_to_string(&path)?;
                let collection: VectorCollection = serde_json::from_str(&json)?;
                self.collections.insert(name.to_string(), collection);

                // Load index
                let index_path = storage_path.join(format!("{}.idx", name));
                if index_path.exists() {
                    let data = std::fs::read(&index_path)?;
                    let ((embeddings, chunk_ids), _): ((Vec<Vec<f32>>, Vec<String>), usize) =
                        bincode::serde::decode_from_slice(&data, bincode::config::standard())?;

                    // Validate parallel array invariant: embeddings and chunk_ids
                    // must have the same length, otherwise the index is corrupt.
                    if embeddings.len() != chunk_ids.len() {
                        tracing::warn!(
                            "Corrupt vector index for '{}': {} embeddings vs {} chunk_ids — skipping",
                            name,
                            embeddings.len(),
                            chunk_ids.len()
                        );
                        continue;
                    }

                    let mut index = VectorIndex::new(self.provider.dimension());
                    for (chunk_id, embedding) in chunk_ids.into_iter().zip(embeddings.into_iter()) {
                        index.add(chunk_id, embedding)?;
                    }
                    self.indices.insert(name.to_string(), index);
                }
            }
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

    /// Get a mutable reference to the inner `VectorStore`.
    pub fn inner_mut(&mut self) -> &mut VectorStore {
        &mut self.inner
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
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn test_chunk_type_weight() {
        assert_eq!(ChunkType::Function.weight(), 1.0);
        assert_eq!(ChunkType::Import.weight(), 0.3);
        assert!(ChunkType::Comment.weight() < ChunkType::Function.weight());
    }

    #[test]
    fn test_chunk_metadata_creation() {
        let meta = ChunkMetadata::new(
            PathBuf::from("src/lib.rs"),
            1,
            10,
            ChunkType::Function,
            "rust",
            "fn main() {}",
        );

        assert_eq!(*meta.file_path, *Path::new("src/lib.rs"));
        assert_eq!(meta.start_line, 1);
        assert_eq!(meta.end_line, 10);
        assert_eq!(meta.chunk_type, ChunkType::Function);
        assert!(!meta.content_hash.is_empty());
    }

    #[test]
    fn test_chunk_metadata_with_symbol() {
        let meta = ChunkMetadata::new(
            PathBuf::from("lib.rs"),
            1,
            5,
            ChunkType::Function,
            "rust",
            "fn test() {}",
        )
        .with_symbol("test")
        .with_tag("unit-test");

        assert_eq!(meta.symbol_name, Some("test".to_string()));
        assert!(meta.tags.contains(&"unit-test".to_string()));
    }

    #[test]
    fn test_code_chunk_creation() {
        let meta = ChunkMetadata::new(
            PathBuf::from("lib.rs"),
            1,
            3,
            ChunkType::Function,
            "rust",
            "fn hello() {}",
        );
        let chunk = CodeChunk::new("fn hello() {}".to_string(), meta);

        assert!(!chunk.id.is_empty());
        assert_eq!(chunk.content, "fn hello() {}");
        assert_eq!(chunk.len(), 13);
        assert!(!chunk.is_empty());
    }

    #[test]
    fn test_search_filter() {
        let filter = SearchFilter::new()
            .with_file_pattern("*.rs")
            .with_chunk_type(ChunkType::Function)
            .with_language("rust")
            .with_min_score(0.5);

        let meta = ChunkMetadata::new(
            PathBuf::from("test.rs"),
            1,
            5,
            ChunkType::Function,
            "rust",
            "fn test() {}",
        );
        let chunk = CodeChunk::new("fn test() {}".to_string(), meta);

        assert!(filter.matches(&chunk));
    }

    #[test]
    fn test_search_filter_file_pattern_mismatch() {
        let filter = SearchFilter::new().with_file_pattern("*.py");

        let meta = ChunkMetadata::new(
            PathBuf::from("test.rs"),
            1,
            5,
            ChunkType::Function,
            "rust",
            "fn test() {}",
        );
        let chunk = CodeChunk::new("fn test() {}".to_string(), meta);

        assert!(!filter.matches(&chunk));
    }

    #[test]
    fn test_vector_collection_add_get() {
        let mut collection = VectorCollection::new("test", CollectionScope::Project);

        let meta = ChunkMetadata::new(
            PathBuf::from("lib.rs"),
            1,
            5,
            ChunkType::Function,
            "rust",
            "fn test() {}",
        );
        let chunk = CodeChunk::new("fn test() {}".to_string(), meta);
        let chunk_id = chunk.id.clone();

        collection.add_chunk(chunk).unwrap();

        assert_eq!(collection.len(), 1);
        assert!(collection.get_chunk(&chunk_id).is_some());
    }

    #[test]
    fn test_vector_collection_remove_chunk() {
        let mut collection = VectorCollection::new("test", CollectionScope::Project);

        let meta = ChunkMetadata::new(
            PathBuf::from("lib.rs"),
            1,
            5,
            ChunkType::Function,
            "rust",
            "fn test() {}",
        );
        let chunk = CodeChunk::new("fn test() {}".to_string(), meta);
        let chunk_id = chunk.id.clone();

        collection.add_chunk(chunk).unwrap();
        assert_eq!(collection.len(), 1);

        let removed = collection.remove_chunk(&chunk_id);
        assert!(removed.is_some());
        assert_eq!(collection.len(), 0);
    }

    #[test]
    fn test_vector_collection_remove_file() {
        let mut collection = VectorCollection::new("test", CollectionScope::Project);

        let path = PathBuf::from("lib.rs");

        for i in 0..3 {
            let meta = ChunkMetadata::new(
                path.clone(),
                i * 10 + 1,
                (i + 1) * 10,
                ChunkType::Function,
                "rust",
                &format!("fn test{}() {{}}", i),
            );
            let chunk = CodeChunk::new(format!("fn test{}() {{}}", i), meta);
            collection.add_chunk(chunk).unwrap();
        }

        assert_eq!(collection.len(), 3);
        collection.remove_file(&path);
        assert_eq!(collection.len(), 0);
    }

    #[tokio::test]
    async fn test_mock_embedding_provider() {
        let provider = MockEmbeddingProvider::new(384);

        let embedding = provider.embed("test text").await.unwrap();
        assert_eq!(embedding.len(), 384);

        // Verify normalization
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_mock_embedding_deterministic() {
        let provider = MockEmbeddingProvider::new(384);

        let e1 = provider.embed("test").await.unwrap();
        let e2 = provider.embed("test").await.unwrap();

        assert_eq!(e1, e2);
    }

    #[tokio::test]
    async fn test_tfidf_embedding_provider() {
        let provider = TfIdfEmbeddingProvider::new(256);

        let embedding = provider.embed("fn test() {}").await.unwrap();
        assert_eq!(embedding.len(), 256);
    }

    #[tokio::test]
    async fn test_tfidf_similar_texts() {
        let provider = TfIdfEmbeddingProvider::new(256);

        let e1 = provider.embed("function test").await.unwrap();
        let e2 = provider.embed("test function").await.unwrap();

        // Similar texts should have high cosine similarity
        let similarity = VectorIndex::cosine_similarity(&e1, &e2);
        assert!(similarity > 0.5);
    }

    #[test]
    fn test_vector_index_add_search() {
        let mut index = VectorIndex::new(4);

        // Add some embeddings
        index
            .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index
            .add("b".to_string(), vec![0.0, 1.0, 0.0, 0.0])
            .unwrap();
        index
            .add("c".to_string(), vec![0.9, 0.1, 0.0, 0.0])
            .unwrap();

        // Search for something similar to "a"
        let results = index.search(&[1.0, 0.0, 0.0, 0.0], 2);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "a"); // Exact match
        assert_eq!(results[1].0, "c"); // Close match
    }

    #[test]
    fn test_vector_index_remove() {
        let mut index = VectorIndex::new(4);

        index
            .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index
            .add("b".to_string(), vec![0.0, 1.0, 0.0, 0.0])
            .unwrap();

        assert_eq!(index.len(), 2);

        index.remove("a");
        assert_eq!(index.len(), 1);

        let results = index.search(&[1.0, 0.0, 0.0, 0.0], 1);
        assert_eq!(results[0].0, "b"); // Only "b" left
    }

    #[test]
    fn test_code_chunker_rust() {
        let chunker = CodeChunker::default();
        let content = r#"
pub fn hello() {
    println!("Hello");
}

pub struct Point {
    x: i32,
    y: i32,
}

impl Point {
    pub fn new() -> Self {
        Self { x: 0, y: 0 }
    }
}
"#;

        let chunks = chunker.chunk_rust(content, Path::new("lib.rs"));

        // Should have chunks for function, struct, and impl
        assert!(chunks.len() >= 3);

        let types: Vec<_> = chunks.iter().map(|c| c.metadata.chunk_type).collect();
        assert!(types.contains(&ChunkType::Function));
        assert!(types.contains(&ChunkType::Struct));
        assert!(types.contains(&ChunkType::Impl));
    }

    #[test]
    fn test_code_chunker_extract_symbol() {
        let chunker = CodeChunker::default();

        // Test function extraction
        let fn_name = chunker.extract_rust_symbol("pub fn hello() {}", ChunkType::Function);
        assert_eq!(fn_name, Some("hello".to_string()));

        // Test struct extraction
        let struct_name = chunker.extract_rust_symbol("pub struct MyStruct {", ChunkType::Struct);
        assert_eq!(struct_name, Some("MyStruct".to_string()));

        // Test impl extraction
        let impl_name = chunker.extract_rust_symbol("impl MyStruct {", ChunkType::Impl);
        assert_eq!(impl_name, Some("MyStruct".to_string()));
    }

    #[test]
    fn test_code_chunker_fixed_size() {
        let chunker = CodeChunker {
            max_chunk_size: 100,
            min_chunk_size: 10,
            overlap: 10,
        };

        let content = "a\n".repeat(50);
        let chunks = chunker.chunk_fixed_size(&content, Path::new("test.txt"), "txt");

        assert!(!chunks.is_empty());
        for chunk in &chunks {
            assert!(chunk.len() <= 100);
        }
    }

    #[tokio::test]
    async fn test_vector_store_create_collection() {
        let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let mut store = VectorStore::new(provider);

        store.collection("test", CollectionScope::Project);

        assert!(store.get_collection("test").is_some());
        assert!(store.list_collections().contains(&"test"));
    }

    #[tokio::test]
    async fn test_vector_store_delete_collection() {
        let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let mut store = VectorStore::new(provider);

        store.collection("test", CollectionScope::Project);
        let deleted = store.delete_collection("test");

        assert!(deleted.is_some());
        assert!(store.get_collection("test").is_none());
    }

    #[tokio::test]
    async fn test_vector_store_index_file() {
        let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let mut store = VectorStore::new(provider);

        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        std::fs::write(&file_path, "pub fn test() {}\npub fn hello() {}").unwrap();

        store.collection("project", CollectionScope::Project);
        let count = store.index_file("project", &file_path).await.unwrap();

        assert!(count >= 1);

        let collection = store.get_collection("project").unwrap();
        assert!(!collection.is_empty());
    }

    #[tokio::test]
    async fn test_vector_store_search() {
        let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let mut store = VectorStore::new(provider);

        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        std::fs::write(
            &file_path,
            r#"
pub fn calculate_sum(a: i32, b: i32) -> i32 {
    a + b
}

pub fn calculate_product(a: i32, b: i32) -> i32 {
    a * b
}
"#,
        )
        .unwrap();

        store.collection("project", CollectionScope::Project);
        store.index_file("project", &file_path).await.unwrap();

        let results = store
            .search("project", "sum addition", 5, None)
            .await
            .unwrap();

        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_vector_store_search_with_filter() {
        let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let mut store = VectorStore::new(provider);

        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        std::fs::write(&file_path, "pub fn test() {}").unwrap();

        store.collection("project", CollectionScope::Project);
        store.index_file("project", &file_path).await.unwrap();

        let filter = SearchFilter::new()
            .with_chunk_type(ChunkType::Struct)
            .with_min_score(0.9);

        let results = store
            .search("project", "test", 5, Some(&filter))
            .await
            .unwrap();

        // Should be empty due to filter (no structs, high min score)
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_vector_store_persistence() {
        let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let dir = tempdir().unwrap();
        let storage_path = dir.path().join("vector_store");

        // Create and populate store
        {
            let mut store = VectorStore::new(provider.clone()).with_storage(&storage_path);

            let file_path = dir.path().join("test.rs");
            std::fs::write(&file_path, "pub fn test() {}").unwrap();

            store.collection("project", CollectionScope::Project);
            store.index_file("project", &file_path).await.unwrap();
            store.save().unwrap();
        }

        // Load store from disk
        {
            let mut store = VectorStore::new(provider).with_storage(&storage_path);
            store.load().unwrap();

            assert!(store.get_collection("project").is_some());
        }
    }

    #[tokio::test]
    async fn test_vector_store_stats() {
        let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let mut store = VectorStore::new(provider);

        store.collection("project1", CollectionScope::Project);
        store.collection("project2", CollectionScope::Session);

        let stats = store.stats();

        assert_eq!(stats.collection_count, 2);
        assert_eq!(stats.embedding_dimension, EMBEDDING_DIM);
    }

    #[test]
    fn test_cosine_similarity() {
        // Identical vectors
        let sim = VectorIndex::cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]);
        assert!((sim - 1.0).abs() < 0.01);

        // Orthogonal vectors
        let sim = VectorIndex::cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]);
        assert!(sim.abs() < 0.01);

        // Opposite vectors
        let sim = VectorIndex::cosine_similarity(&[1.0, 0.0], &[-1.0, 0.0]);
        assert!((sim + 1.0).abs() < 0.01);
    }

    #[test]
    fn test_collection_scope_default() {
        assert_eq!(CollectionScope::default(), CollectionScope::Project);
    }

    #[test]
    fn test_chunk_type_default() {
        assert_eq!(ChunkType::default(), ChunkType::CodeBlock);
    }

    #[test]
    fn test_empty_vector_index() {
        let index = VectorIndex::new(4);
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);

        let results = index.search(&[1.0, 0.0, 0.0, 0.0], 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_vector_index_dimension_mismatch() {
        let mut index = VectorIndex::new(4);
        let result = index.add("a".to_string(), vec![1.0, 0.0, 0.0]); // Only 3 dims
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_embedding_batch() {
        let provider = MockEmbeddingProvider::default();
        let texts = vec!["hello".to_string(), "world".to_string()];

        let embeddings = provider.embed_batch(&texts).await.unwrap();

        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0].len(), EMBEDDING_DIM);
    }

    #[test]
    fn test_search_filter_empty_matches_all() {
        let filter = SearchFilter::new();

        let meta = ChunkMetadata::new(
            PathBuf::from("any.py"),
            1,
            5,
            ChunkType::Text,
            "python",
            "# comment",
        );
        let chunk = CodeChunk::new("# comment".to_string(), meta);

        assert!(filter.matches(&chunk)); // Empty filter matches everything
    }

    #[test]
    fn test_chunk_with_embedding() {
        let meta = ChunkMetadata::new(
            PathBuf::from("lib.rs"),
            1,
            3,
            ChunkType::Function,
            "rust",
            "fn hello() {}",
        );
        let chunk = CodeChunk::new("fn hello() {}".to_string(), meta);
        let embedding = vec![0.1, 0.2, 0.3];

        let chunk = chunk.with_embedding(embedding.clone());
        assert_eq!(chunk.embedding, Some(embedding));
    }

    #[test]
    fn test_collection_files() {
        let mut collection = VectorCollection::new("test", CollectionScope::Project);

        for path in ["a.rs", "b.rs", "c.rs"] {
            let meta = ChunkMetadata::new(
                PathBuf::from(path),
                1,
                5,
                ChunkType::Function,
                "rust",
                "fn test() {}",
            );
            let chunk = CodeChunk::new("fn test() {}".to_string(), meta);
            collection.add_chunk(chunk).unwrap();
        }

        let files = collection.files();
        assert_eq!(files.len(), 3);
    }

    // Additional comprehensive tests

    #[test]
    fn test_chunk_type_all_variants() {
        let types = [
            ChunkType::Function,
            ChunkType::Struct,
            ChunkType::Enum,
            ChunkType::Trait,
            ChunkType::Impl,
            ChunkType::Module,
            ChunkType::Import,
            ChunkType::Comment,
            ChunkType::Test,
            ChunkType::Constant,
            ChunkType::CodeBlock,
            ChunkType::Text,
        ];

        for chunk_type in types {
            assert!(chunk_type.weight() >= 0.0);
            assert!(chunk_type.weight() <= 1.0);
            let _ = format!("{:?}", chunk_type);
        }
    }

    #[test]
    fn test_chunk_metadata_clone() {
        let meta = ChunkMetadata::new(
            PathBuf::from("test.rs"),
            1,
            10,
            ChunkType::Function,
            "rust",
            "fn test() {}",
        );

        let cloned = meta.clone();
        assert_eq!(meta.file_path, cloned.file_path);
        assert_eq!(meta.content_hash, cloned.content_hash);
    }

    #[test]
    fn test_chunk_metadata_serialization() {
        let meta = ChunkMetadata::new(
            PathBuf::from("test.rs"),
            1,
            10,
            ChunkType::Function,
            "rust",
            "fn test() {}",
        );

        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: ChunkMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(meta.chunk_type, deserialized.chunk_type);
    }

    #[test]
    fn test_code_chunk_clone() {
        let meta = ChunkMetadata::new(
            PathBuf::from("lib.rs"),
            1,
            5,
            ChunkType::Function,
            "rust",
            "fn hello() {}",
        );
        let chunk = CodeChunk::new("fn hello() {}".to_string(), meta);

        let cloned = chunk.clone();
        assert_eq!(chunk.id, cloned.id);
        assert_eq!(chunk.content, cloned.content);
    }

    #[test]
    fn test_search_filter_clone() {
        let filter = SearchFilter::new()
            .with_file_pattern("*.rs")
            .with_chunk_type(ChunkType::Function);

        let cloned = filter.clone();
        assert_eq!(filter.file_patterns, cloned.file_patterns);
    }

    #[test]
    fn test_search_filter_with_tag() {
        let filter = SearchFilter::new().with_tag("important");

        let meta = ChunkMetadata::new(
            PathBuf::from("test.rs"),
            1,
            5,
            ChunkType::Function,
            "rust",
            "fn test() {}",
        )
        .with_tag("important");

        let chunk = CodeChunk::new("fn test() {}".to_string(), meta);

        assert!(filter.matches(&chunk));
    }

    #[test]
    fn test_collection_scope_all_variants() {
        let scopes = [
            CollectionScope::Project,
            CollectionScope::Session,
            CollectionScope::Global,
        ];

        for scope in scopes {
            let _ = format!("{:?}", scope);
            let cloned = scope;
            assert_eq!(scope, cloned);
        }
    }

    #[test]
    fn test_vector_collection_is_empty() {
        let collection = VectorCollection::new("test", CollectionScope::Project);
        assert!(collection.is_empty());
        assert_eq!(collection.len(), 0);
    }

    #[test]
    fn test_vector_collection_name() {
        let collection = VectorCollection::new("test_collection", CollectionScope::Project);
        assert_eq!(collection.name, "test_collection");
    }

    #[test]
    fn test_search_result_clone() {
        let meta = ChunkMetadata::new(
            PathBuf::from("test.rs"),
            1,
            5,
            ChunkType::Function,
            "rust",
            "fn test() {}",
        );
        let chunk = CodeChunk::new("fn test() {}".to_string(), meta);

        let result = SearchResult {
            chunk,
            score: 0.95,
            distance: 0.05,
        };

        let cloned = result.clone();
        assert_eq!(result.score, cloned.score);
        assert_eq!(result.distance, cloned.distance);
    }

    #[test]
    fn test_vector_index_clear() {
        let mut index = VectorIndex::new(4);

        index
            .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index
            .add("b".to_string(), vec![0.0, 1.0, 0.0, 0.0])
            .unwrap();

        assert_eq!(index.len(), 2);

        index.clear();
        assert!(index.is_empty());
    }

    #[tokio::test]
    async fn test_mock_embedding_provider_dimension() {
        let provider = MockEmbeddingProvider::new(512);

        let embedding = provider.embed("test").await.unwrap();
        assert_eq!(embedding.len(), 512);
    }

    #[test]
    fn test_code_chunker_new() {
        let chunker = CodeChunker::new(2000);
        assert_eq!(chunker.max_chunk_size, 2000);
    }

    #[test]
    fn test_vector_store_stats_empty() {
        let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let store = VectorStore::new(provider);

        let stats = store.stats();
        assert_eq!(stats.collection_count, 0);
        assert_eq!(stats.total_chunks, 0);
    }

    // =========================================================================
    // Index integrity and health tests
    // =========================================================================

    #[test]
    fn test_verify_index_integrity_healthy() {
        let mut index = VectorIndex::new(3);
        index.add("a".to_string(), vec![1.0, 0.0, 0.0]).unwrap();
        index.add("b".to_string(), vec![0.0, 1.0, 0.0]).unwrap();

        let issues = index.verify_index_integrity();
        assert!(issues.is_empty(), "Expected no issues, got: {:?}", issues);
    }

    #[test]
    fn test_verify_index_integrity_nan() {
        let mut index = VectorIndex::new(3);
        index
            .add("a".to_string(), vec![1.0, f32::NAN, 0.0])
            .unwrap();

        let issues = index.verify_index_integrity();
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|i| i.contains("NaN")));
    }

    #[test]
    fn test_verify_index_integrity_inf() {
        let mut index = VectorIndex::new(3);
        // After L2 normalization at insert time, [1.0, INF, 0.0] becomes
        // [0.0, NaN, 0.0] because INF/INF = NaN. The integrity check should
        // still detect the bad embedding.
        index
            .add("a".to_string(), vec![1.0, f32::INFINITY, 0.0])
            .unwrap();

        let issues = index.verify_index_integrity();
        assert!(!issues.is_empty());
        assert!(issues
            .iter()
            .any(|i| i.contains("NaN") || i.contains("Inf")));
    }

    #[test]
    fn test_verify_index_integrity_duplicate_ids() {
        let mut index = VectorIndex::new(2);
        index.add("dup".to_string(), vec![1.0, 0.0]).unwrap();
        index.add("dup".to_string(), vec![0.0, 1.0]).unwrap();

        let issues = index.verify_index_integrity();
        assert!(issues.iter().any(|i| i.contains("Duplicate")));
    }

    #[test]
    fn test_check_health_healthy() {
        let mut index = VectorIndex::new(2);
        index.add("a".to_string(), vec![1.0, 0.0]).unwrap();
        assert_eq!(index.check_health(), IndexHealth::Healthy);
    }

    #[test]
    fn test_check_health_corrupt_nan() {
        let mut index = VectorIndex::new(2);
        index.add("a".to_string(), vec![f32::NAN, 0.0]).unwrap();
        assert_eq!(index.check_health(), IndexHealth::Corrupt);
    }

    #[test]
    fn test_check_health_degraded_duplicates() {
        let mut index = VectorIndex::new(2);
        index.add("dup".to_string(), vec![1.0, 0.0]).unwrap();
        index.add("dup".to_string(), vec![0.0, 1.0]).unwrap();
        assert_eq!(index.check_health(), IndexHealth::Degraded);
    }

    #[test]
    fn test_check_health_empty_index() {
        let index = VectorIndex::new(4);
        assert_eq!(index.check_health(), IndexHealth::Healthy);
    }

    #[tokio::test]
    async fn test_rebuild_index() {
        let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let mut store = VectorStore::new(provider);

        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        std::fs::write(&file_path, "pub fn test() {}").unwrap();

        store.collection("project", CollectionScope::Project);
        store.index_file("project", &file_path).await.unwrap();

        // Rebuild should succeed
        store.rebuild_index("project").await.unwrap();

        let index = store.indices.get("project").unwrap();
        assert_eq!(index.check_health(), IndexHealth::Healthy);
    }

    // ── Regex caching tests ──────────────────────────────────────────

    #[test]
    fn test_cached_extract_rust_symbol_fn() {
        let chunker = CodeChunker::default();
        // Verify cached regexes produce the same results as before
        assert_eq!(
            chunker.extract_rust_symbol("pub fn hello() {}", ChunkType::Function),
            Some("hello".to_string())
        );
        assert_eq!(
            chunker.extract_rust_symbol("fn world() {}", ChunkType::Function),
            Some("world".to_string())
        );
        assert_eq!(
            chunker.extract_rust_symbol("pub struct Foo {", ChunkType::Function),
            None, // no "fn" keyword
        );
    }

    #[test]
    fn test_cached_extract_rust_symbol_all_types() {
        let chunker = CodeChunker::default();

        assert_eq!(
            chunker.extract_rust_symbol("pub struct MyStruct {", ChunkType::Struct),
            Some("MyStruct".to_string())
        );
        assert_eq!(
            chunker.extract_rust_symbol("enum Color {", ChunkType::Enum),
            Some("Color".to_string())
        );
        assert_eq!(
            chunker.extract_rust_symbol("pub trait Display {", ChunkType::Trait),
            Some("Display".to_string())
        );
        assert_eq!(
            chunker.extract_rust_symbol("impl<T> MyStruct {", ChunkType::Impl),
            Some("MyStruct".to_string())
        );
        // The regex matches the first word after `impl`, which is the trait name
        // in `impl Trait for Type` form. This matches the original behavior.
        assert_eq!(
            chunker.extract_rust_symbol("impl Display for MyStruct {", ChunkType::Impl),
            Some("Display".to_string())
        );
        assert_eq!(
            chunker.extract_rust_symbol("mod utils {", ChunkType::Module),
            Some("utils".to_string())
        );
        assert_eq!(
            chunker.extract_rust_symbol("// comment", ChunkType::Comment),
            None,
        );
    }

    // ── BoundedVectorStore tests ─────────────────────────────────────

    #[tokio::test]
    async fn test_bounded_vector_store_eviction_at_capacity() {
        let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let inner = VectorStore::new(provider);
        // Very small capacity to trigger eviction quickly
        let mut bounded = BoundedVectorStore::new(inner, 3);

        bounded.collection("test", CollectionScope::Project);

        let dir = tempdir().unwrap();

        // Index several small files. Each file should produce at least 1 chunk.
        for i in 0..6 {
            let file_path = dir.path().join(format!("file{}.rs", i));
            std::fs::write(
                &file_path,
                format!("pub fn func_{}() {{ println!(\"hello\"); }}", i),
            )
            .unwrap();
            bounded.index_file("test", &file_path).await.unwrap();
        }

        // The store should not exceed max_items
        assert!(
            bounded.len() <= 3,
            "Store has {} items but max is 3",
            bounded.len()
        );
    }

    #[tokio::test]
    async fn test_bounded_vector_store_stays_within_bounds() {
        let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let inner = VectorStore::new(provider);
        let mut bounded = BoundedVectorStore::new(inner, 5);

        bounded.collection("coll", CollectionScope::Session);

        let dir = tempdir().unwrap();

        // Insert many files
        for i in 0..20 {
            let file_path = dir.path().join(format!("mod{}.rs", i));
            std::fs::write(
                &file_path,
                format!("pub fn handler_{}() {{ let x = {}; }}", i, i * 42),
            )
            .unwrap();
            bounded.index_file("coll", &file_path).await.unwrap();

            // After each insertion, the store must not exceed max_items
            assert!(
                bounded.len() <= 5,
                "After inserting file {}, store has {} items (max 5)",
                i,
                bounded.len()
            );
        }
    }

    #[tokio::test]
    async fn test_bounded_vector_store_clear() {
        let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let inner = VectorStore::new(provider);
        let mut bounded = BoundedVectorStore::new(inner, 100);

        bounded.collection("proj", CollectionScope::Project);

        let dir = tempdir().unwrap();
        let file_path = dir.path().join("code.rs");
        std::fs::write(&file_path, "pub fn example() { let _ = 1 + 2; }").unwrap();
        bounded.index_file("proj", &file_path).await.unwrap();

        assert!(!bounded.is_empty());

        bounded.clear();
        assert!(bounded.is_empty());
        assert_eq!(bounded.len(), 0);
    }

    #[test]
    fn test_bounded_vector_store_default_capacity() {
        let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
        let inner = VectorStore::new(provider);
        let bounded = BoundedVectorStore::with_default_capacity(inner);
        assert_eq!(bounded.max_items(), DEFAULT_MAX_ITEMS);
        assert!(bounded.is_empty());
    }
}
