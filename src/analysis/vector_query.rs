//! Search query types and filtering for the vector store.

use super::vector_store::{ChunkType, CodeChunk};

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
