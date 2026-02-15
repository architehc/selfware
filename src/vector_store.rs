//! Vector Memory System
//!
//! Semantic vector storage for code search and memory using embedded HNSW.
//! Local-first design - no external server required.
//!
//! Features:
//! - Code chunking strategies (functions, structs, modules)
//! - Embedding generation interface (pluggable backends)
//! - Similarity search with filters
//! - Collection management (project, session, global)
//! - Persistence to disk

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Embedding dimension (common for small models)
pub const EMBEDDING_DIM: usize = 384;

/// Maximum chunks per collection
pub const MAX_CHUNKS: usize = 100_000;

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    /// Source file path
    pub file_path: PathBuf,
    /// Start line (1-indexed)
    pub start_line: usize,
    /// End line (1-indexed)
    pub end_line: usize,
    /// Chunk type
    pub chunk_type: ChunkType,
    /// Symbol name if applicable (function name, struct name, etc.)
    pub symbol_name: Option<String>,
    /// Language identifier
    pub language: String,
    /// Hash of content for deduplication
    pub content_hash: String,
    /// Timestamp when indexed
    pub indexed_at: u64,
    /// Custom tags
    pub tags: Vec<String>,
}

impl ChunkMetadata {
    /// Create new metadata
    pub fn new(
        file_path: PathBuf,
        start_line: usize,
        end_line: usize,
        chunk_type: ChunkType,
        language: &str,
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
            file_path,
            start_line,
            end_line,
            chunk_type,
            symbol_name: None,
            language: language.to_string(),
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

        // Update file index
        self.file_index
            .entry(chunk.metadata.file_path.clone())
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
            let chunk = self.chunks.remove(idx);

            // Rebuild index (O(n) but infrequent)
            self.id_index.clear();
            for (i, c) in self.chunks.iter().enumerate() {
                self.id_index.insert(c.id.clone(), i);
            }

            // Update file index
            if let Some(file_chunks) = self.file_index.get_mut(&chunk.metadata.file_path) {
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
            // Collect indices to remove, then remove in reverse order
            let mut indices_to_remove: Vec<usize> = chunk_ids
                .iter()
                .filter_map(|id| self.id_index.get(id).copied())
                .collect();

            // Sort in reverse order so we remove from end first
            indices_to_remove.sort_by(|a, b| b.cmp(a));

            for idx in indices_to_remove {
                if idx < self.chunks.len() {
                    self.chunks.remove(idx);
                }
            }

            // Rebuild index
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

/// Trait for embedding generation
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
    vocabulary: Arc<RwLock<HashMap<String, usize>>>,
}

impl TfIdfEmbeddingProvider {
    /// Create new TF-IDF provider
    pub fn new(dimension: usize) -> Self {
        Self {
            dimension,
            vocabulary: Arc::new(RwLock::new(HashMap::new())),
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
        let read = self.vocabulary.read().unwrap();
        if let Some(&idx) = read.get(token) {
            return idx;
        }
        drop(read);

        let mut write = self.vocabulary.write().unwrap();
        let idx = write.len() % self.dimension;
        write.insert(token.to_string(), idx);
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

/// Vector index using simple brute-force search (for small collections)
/// TODO: Integrate HNSW for larger collections
pub struct VectorIndex {
    /// Embeddings matrix (row-major)
    embeddings: Vec<Vec<f32>>,
    /// Chunk IDs corresponding to embeddings
    chunk_ids: Vec<String>,
    /// Dimension
    dimension: usize,
}

impl VectorIndex {
    /// Create new index
    pub fn new(dimension: usize) -> Self {
        Self {
            embeddings: Vec::new(),
            chunk_ids: Vec::new(),
            dimension,
        }
    }

    /// Add embedding to index
    pub fn add(&mut self, chunk_id: String, embedding: Vec<f32>) -> Result<()> {
        if embedding.len() != self.dimension {
            return Err(anyhow!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.dimension,
                embedding.len()
            ));
        }

        self.embeddings.push(embedding);
        self.chunk_ids.push(chunk_id);
        Ok(())
    }

    /// Remove embedding by chunk ID
    pub fn remove(&mut self, chunk_id: &str) {
        if let Some(pos) = self.chunk_ids.iter().position(|id| id == chunk_id) {
            self.embeddings.remove(pos);
            self.chunk_ids.remove(pos);
        }
    }

    /// Search for similar embeddings
    pub fn search(&self, query: &[f32], k: usize) -> Vec<(String, f32)> {
        if query.len() != self.dimension {
            return Vec::new();
        }

        let mut scores: Vec<(usize, f32)> = self
            .embeddings
            .iter()
            .enumerate()
            .map(|(i, emb)| (i, Self::cosine_similarity(query, emb)))
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        scores
            .into_iter()
            .take(k)
            .map(|(i, score)| (self.chunk_ids[i].clone(), score))
            .collect()
    }

    /// Cosine similarity between two vectors
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a > 0.0 && norm_b > 0.0 {
            dot / (norm_a * norm_b)
        } else {
            0.0
        }
    }

    /// Get index size
    pub fn len(&self) -> usize {
        self.embeddings.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.embeddings.is_empty()
    }

    /// Clear index
    pub fn clear(&mut self) {
        self.embeddings.clear();
        self.chunk_ids.clear();
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
        let mut chunks = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Patterns for Rust constructs
        let patterns = [
            (
                regex::Regex::new(r"^\s*(pub\s+)?(async\s+)?fn\s+").unwrap(),
                ChunkType::Function,
            ),
            (
                regex::Regex::new(r"^\s*(pub\s+)?struct\s+").unwrap(),
                ChunkType::Struct,
            ),
            (
                regex::Regex::new(r"^\s*(pub\s+)?enum\s+").unwrap(),
                ChunkType::Enum,
            ),
            (
                regex::Regex::new(r"^\s*(pub\s+)?trait\s+").unwrap(),
                ChunkType::Trait,
            ),
            (regex::Regex::new(r"^\s*impl\s+").unwrap(), ChunkType::Impl),
            (
                regex::Regex::new(r"^\s*(pub\s+)?mod\s+").unwrap(),
                ChunkType::Module,
            ),
            (
                regex::Regex::new(r"^\s*#\[test\]").unwrap(),
                ChunkType::Test,
            ),
            (
                regex::Regex::new(r"^\s*(pub\s+)?const\s+").unwrap(),
                ChunkType::Constant,
            ),
            (regex::Regex::new(r"^\s*use\s+").unwrap(), ChunkType::Import),
        ];

        let mut current_start = 0;
        let mut current_type = ChunkType::CodeBlock;
        let mut brace_depth = 0;
        let mut in_block = false;

        for (line_num, line) in lines.iter().enumerate() {
            // Check for pattern starts
            for (pattern, chunk_type) in &patterns {
                if pattern.is_match(line) && !in_block {
                    // Save previous chunk if exists
                    if line_num > current_start {
                        let chunk_content: String = lines[current_start..line_num].join("\n");
                        if chunk_content.len() >= self.min_chunk_size {
                            let metadata = ChunkMetadata::new(
                                file_path.to_path_buf(),
                                current_start + 1,
                                line_num,
                                current_type,
                                "rust",
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
                    file_path.to_path_buf(),
                    current_start + 1,
                    line_num + 1,
                    current_type,
                    "rust",
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
                    file_path.to_path_buf(),
                    current_start + 1,
                    lines.len(),
                    current_type,
                    "rust",
                    &chunk_content,
                );
                chunks.push(CodeChunk::new(chunk_content, metadata));
            }
        }

        chunks
    }

    /// Extract symbol name from Rust code
    fn extract_rust_symbol(&self, content: &str, chunk_type: ChunkType) -> Option<String> {
        let first_line = content.lines().next()?;

        match chunk_type {
            ChunkType::Function => {
                let re = regex::Regex::new(r"fn\s+(\w+)").ok()?;
                re.captures(first_line)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string())
            }
            ChunkType::Struct => {
                let re = regex::Regex::new(r"struct\s+(\w+)").ok()?;
                re.captures(first_line)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string())
            }
            ChunkType::Enum => {
                let re = regex::Regex::new(r"enum\s+(\w+)").ok()?;
                re.captures(first_line)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string())
            }
            ChunkType::Trait => {
                let re = regex::Regex::new(r"trait\s+(\w+)").ok()?;
                re.captures(first_line)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string())
            }
            ChunkType::Impl => {
                let re = regex::Regex::new(r"impl(?:<[^>]+>)?\s+(?:(\w+)|(?:\w+)\s+for\s+(\w+))")
                    .ok()?;
                re.captures(first_line).and_then(|c| {
                    c.get(1)
                        .or_else(|| c.get(2))
                        .map(|m| m.as_str().to_string())
                })
            }
            ChunkType::Module => {
                let re = regex::Regex::new(r"mod\s+(\w+)").ok()?;
                re.captures(first_line)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string())
            }
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
                file_path.to_path_buf(),
                start + 1,
                end,
                ChunkType::CodeBlock,
                language,
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

/// Main vector store
pub struct VectorStore {
    /// Collections by name
    collections: HashMap<String, VectorCollection>,
    /// Vector indices by collection name
    indices: HashMap<String, VectorIndex>,
    /// Embedding provider
    provider: Arc<dyn EmbeddingProvider>,
    /// Storage path for persistence
    storage_path: Option<PathBuf>,
    /// Code chunker
    chunker: CodeChunker,
}

impl VectorStore {
    /// Create new vector store
    pub fn new(provider: Arc<dyn EmbeddingProvider>) -> Self {
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
        self.collections.get_mut(name).unwrap()
    }

    /// Get collection by name
    pub fn get_collection(&self, name: &str) -> Option<&VectorCollection> {
        self.collections.get(name)
    }

    /// List all collections
    pub fn list_collections(&self) -> Vec<&str> {
        self.collections.keys().map(|s| s.as_str()).collect()
    }

    /// Delete collection
    pub fn delete_collection(&mut self, name: &str) -> Option<VectorCollection> {
        self.indices.remove(name);
        self.collections.remove(name)
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

        let collection = self.collections.get_mut(collection_name).unwrap();
        let index = self.indices.get_mut(collection_name).unwrap();

        // Add chunks with embeddings
        for (chunk, embedding) in chunks.into_iter().zip(embeddings.into_iter()) {
            let chunk_id = chunk.id.clone();
            let chunk = chunk.with_embedding(embedding.clone());
            collection.add_chunk(chunk)?;
            index.add(chunk_id, embedding)?;
        }

        Ok(chunk_count)
    }

    /// Search across collection
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

        // Generate query embedding
        let query_embedding = self.provider.embed(query).await?;

        // Search index
        let results = index.search(&query_embedding, k * 2); // Get more for filtering

        // Build results with chunks
        let mut search_results = Vec::new();
        for (chunk_id, score) in results {
            if let Some(chunk) = collection.get_chunk(&chunk_id) {
                // Apply filter
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

                // Apply chunk type weight
                let weighted_score = score * chunk.metadata.chunk_type.weight();

                search_results.push(SearchResult {
                    chunk: chunk.clone(),
                    score: weighted_score,
                    distance: 1.0 - score,
                });
            }
        }

        // Sort by weighted score
        search_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Limit to k
        search_results.truncate(k);

        Ok(search_results)
    }

    /// Save store to disk
    pub fn save(&self) -> Result<()> {
        let storage_path = self
            .storage_path
            .as_ref()
            .ok_or_else(|| anyhow!("Storage path not set"))?;

        std::fs::create_dir_all(storage_path)?;

        // Save each collection
        for (name, collection) in &self.collections {
            let collection_path = storage_path.join(format!("{}.json", name));
            let json = serde_json::to_string_pretty(collection)?;
            std::fs::write(&collection_path, json)?;

            // Save embeddings separately (binary for efficiency)
            if let Some(index) = self.indices.get(name) {
                let index_path = storage_path.join(format!("{}.idx", name));
                let data = bincode::serialize(&(&index.embeddings, &index.chunk_ids))?;
                std::fs::write(index_path, data)?;
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
                    let (embeddings, chunk_ids): (Vec<Vec<f32>>, Vec<String>) =
                        bincode::deserialize(&data)?;

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

        assert_eq!(meta.file_path, PathBuf::from("src/lib.rs"));
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
        let provider = Arc::new(MockEmbeddingProvider::default());
        let mut store = VectorStore::new(provider);

        store.collection("test", CollectionScope::Project);

        assert!(store.get_collection("test").is_some());
        assert!(store.list_collections().contains(&"test"));
    }

    #[tokio::test]
    async fn test_vector_store_delete_collection() {
        let provider = Arc::new(MockEmbeddingProvider::default());
        let mut store = VectorStore::new(provider);

        store.collection("test", CollectionScope::Project);
        let deleted = store.delete_collection("test");

        assert!(deleted.is_some());
        assert!(store.get_collection("test").is_none());
    }

    #[tokio::test]
    async fn test_vector_store_index_file() {
        let provider = Arc::new(MockEmbeddingProvider::default());
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
        let provider = Arc::new(MockEmbeddingProvider::default());
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
        let provider = Arc::new(MockEmbeddingProvider::default());
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
        let provider = Arc::new(MockEmbeddingProvider::default());
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
        let provider = Arc::new(MockEmbeddingProvider::default());
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
            let cloned = scope.clone();
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
        let provider = Arc::new(MockEmbeddingProvider::default());
        let store = VectorStore::new(provider);

        let stats = store.stats();
        assert_eq!(stats.collection_count, 0);
        assert_eq!(stats.total_chunks, 0);
    }
}
