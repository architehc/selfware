//! BM25 (Best Matching 25) search index
//!
//! A fast, reliable ranking function for text search without requiring embeddings.
//! Used for code search, symbol lookup, and as a fallback/complement to vector search.

use std::collections::HashMap;

/// BM25 search index for fast text retrieval
#[derive(Debug, Clone)]
pub struct BM25Index {
    /// Documents stored as (doc_id, tokens)
    documents: Vec<Document>,
    /// Inverse document frequency for each term
    idf: HashMap<String, f32>,
    /// Average document length
    avgdl: f32,
    /// Term saturation parameter (typically 1.2-2.0)
    k1: f32,
    /// Length normalization parameter (typically 0.75)
    b: f32,
    /// Whether the index needs rebuilding
    dirty: bool,
}

/// A document in the index
#[derive(Debug, Clone)]
struct Document {
    /// Unique identifier
    id: String,
    /// Original text (for display)
    text: String,
    /// Tokenized terms with frequencies
    term_freqs: HashMap<String, u32>,
    /// Document length (number of tokens)
    length: u32,
}

/// Search result with score
#[derive(Debug, Clone)]
pub struct BM25Result {
    /// Document ID
    pub id: String,
    /// Original document text
    pub text: String,
    /// BM25 score (higher is better)
    pub score: f32,
}

impl Default for BM25Index {
    fn default() -> Self {
        Self::new()
    }
}

impl BM25Index {
    /// Create a new empty BM25 index with default parameters
    pub fn new() -> Self {
        Self::with_params(1.5, 0.75)
    }

    /// Create a new BM25 index with custom parameters
    ///
    /// # Parameters
    /// - `k1`: Term saturation (1.2-2.0 typical, higher = more weight to term frequency)
    /// - `b`: Length normalization (0.0-1.0, higher = more penalty for long documents)
    pub fn with_params(k1: f32, b: f32) -> Self {
        Self {
            documents: Vec::new(),
            idf: HashMap::new(),
            avgdl: 0.0,
            k1,
            b,
            dirty: false,
        }
    }

    /// Add a document to the index (upsert: removes existing doc with same ID first)
    ///
    /// # Arguments
    /// - `id`: Unique document identifier
    /// - `text`: Document text to index
    pub fn add(&mut self, id: impl Into<String>, text: impl Into<String>) {
        let id = id.into();
        let text = text.into();

        // Upsert: remove any existing document with the same ID
        self.remove_all(&id);

        let tokens = Self::tokenize(&text);
        let length = tokens.len() as u32;

        // Build term frequency map
        let mut term_freqs: HashMap<String, u32> = HashMap::new();
        for token in tokens {
            *term_freqs.entry(token).or_insert(0) += 1;
        }

        self.documents.push(Document {
            id,
            text,
            term_freqs,
            length,
        });
        self.dirty = true;
    }

    /// Add multiple documents at once (more efficient than individual adds)
    pub fn add_batch(&mut self, docs: impl IntoIterator<Item = (String, String)>) {
        for (id, text) in docs {
            let tokens = Self::tokenize(&text);
            let length = tokens.len() as u32;

            let mut term_freqs: HashMap<String, u32> = HashMap::new();
            for token in tokens {
                *term_freqs.entry(token).or_insert(0) += 1;
            }

            self.documents.push(Document {
                id,
                text,
                term_freqs,
                length,
            });
        }
        self.dirty = true;
    }

    /// Remove first document matching ID (returns true if found)
    pub fn remove(&mut self, id: &str) -> bool {
        if let Some(pos) = self.documents.iter().position(|d| d.id == id) {
            self.documents.remove(pos);
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Remove ALL documents matching ID (handles duplicates)
    pub fn remove_all(&mut self, id: &str) -> usize {
        let before = self.documents.len();
        self.documents.retain(|d| d.id != id);
        let removed = before - self.documents.len();
        if removed > 0 {
            self.dirty = true;
        }
        removed
    }

    /// Clear all documents
    pub fn clear(&mut self) {
        self.documents.clear();
        self.idf.clear();
        self.avgdl = 0.0;
        self.dirty = false;
    }

    /// Rebuild the index (compute IDF values)
    /// Called automatically before search if dirty
    pub fn rebuild(&mut self) {
        if self.documents.is_empty() {
            self.idf.clear();
            self.avgdl = 0.0;
            self.dirty = false;
            return;
        }

        let n = self.documents.len() as f32;

        // Compute average document length
        let total_length: u32 = self.documents.iter().map(|d| d.length).sum();
        self.avgdl = total_length as f32 / n;

        // Compute document frequency for each term
        let mut doc_freq: HashMap<String, u32> = HashMap::new();
        for doc in &self.documents {
            for term in doc.term_freqs.keys() {
                *doc_freq.entry(term.clone()).or_insert(0) += 1;
            }
        }

        // Compute IDF for each term
        // IDF = ln((N - df + 0.5) / (df + 0.5) + 1)
        self.idf.clear();
        for (term, df) in doc_freq {
            let df = df as f32;
            let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
            self.idf.insert(term, idf);
        }

        self.dirty = false;
    }

    /// Search the index and return ranked results
    ///
    /// # Arguments
    /// - `query`: Search query string
    /// - `limit`: Maximum number of results to return
    ///
    /// # Returns
    /// Vector of results sorted by score (descending)
    pub fn search(&mut self, query: &str, limit: usize) -> Vec<BM25Result> {
        if self.dirty {
            self.rebuild();
        }

        if self.documents.is_empty() {
            return Vec::new();
        }

        let query_tokens = Self::tokenize(query);
        if query_tokens.is_empty() {
            return Vec::new();
        }

        // Score each document
        let mut scores: Vec<(usize, f32)> = self
            .documents
            .iter()
            .enumerate()
            .map(|(i, doc)| (i, self.score_document(doc, &query_tokens)))
            .filter(|(_, score)| *score > 0.0)
            .collect();

        // Sort by score descending
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return top results
        scores
            .into_iter()
            .take(limit)
            .map(|(i, score)| {
                let doc = &self.documents[i];
                BM25Result {
                    id: doc.id.clone(),
                    text: doc.text.clone(),
                    score,
                }
            })
            .collect()
    }

    /// Search without modifying self (requires index to be up-to-date)
    pub fn search_immutable(&self, query: &str, limit: usize) -> Vec<BM25Result> {
        // Immutable search should still provide best-effort results even if the
        // mutable index is marked dirty.
        if self.documents.is_empty() {
            return Vec::new();
        }

        let query_tokens = Self::tokenize(query);
        if query_tokens.is_empty() {
            return Vec::new();
        }

        let mut scores: Vec<(usize, f32)> = self
            .documents
            .iter()
            .enumerate()
            .map(|(i, doc)| (i, self.score_document(doc, &query_tokens)))
            .filter(|(_, score)| *score > 0.0)
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        scores
            .into_iter()
            .take(limit)
            .map(|(i, score)| {
                let doc = &self.documents[i];
                BM25Result {
                    id: doc.id.clone(),
                    text: doc.text.clone(),
                    score,
                }
            })
            .collect()
    }

    /// Compute BM25 score for a document given query tokens
    fn score_document(&self, doc: &Document, query_tokens: &[String]) -> f32 {
        let mut score = 0.0;
        let dl = doc.length as f32;
        let avgdl = self.avgdl;

        // Guard against division by zero when document length or average
        // document length is zero.
        if dl <= 0.0 || avgdl <= 0.0 {
            return 0.0;
        }

        for token in query_tokens {
            if let Some(&idf) = self.idf.get(token) {
                let tf = *doc.term_freqs.get(token).unwrap_or(&0) as f32;
                if tf > 0.0 {
                    // BM25 scoring formula
                    let numerator = tf * (self.k1 + 1.0);
                    let denominator = tf + self.k1 * (1.0 - self.b + self.b * (dl / avgdl));
                    if denominator <= 0.0 {
                        continue;
                    }
                    score += idf * (numerator / denominator);
                }
            }
        }

        score
    }

    /// Tokenize text into searchable terms
    ///
    /// Handles:
    /// - Lowercase normalization
    /// - CamelCase splitting (getUserName -> get, user, name)
    /// - snake_case splitting
    /// - Punctuation removal
    /// - Common programming tokens
    fn tokenize(text: &str) -> Vec<String> {
        let mut tokens = Vec::new();

        // Split on whitespace and punctuation, but keep underscores for snake_case
        for word in text
            .split(|c: char| c.is_whitespace() || ".,;:!?()[]{}\"'`<>=+-*/\\|&^%$#@~".contains(c))
        {
            if word.is_empty() {
                continue;
            }

            // Split snake_case
            for part in word.split('_') {
                if part.is_empty() {
                    continue;
                }

                // Split CamelCase
                let camel_parts = Self::split_camel_case(part);
                for p in camel_parts {
                    let lower = p.to_lowercase();
                    if !lower.is_empty() && lower.len() >= 2 {
                        tokens.push(lower);
                    }
                }
            }
        }

        tokens
    }

    /// Split CamelCase into separate words (Unicode-safe using byte offsets)
    fn split_camel_case(s: &str) -> Vec<&str> {
        if s.is_empty() {
            return vec![s];
        }

        let mut parts = Vec::new();
        let mut last_byte = 0;

        // Collect (byte_offset, char) pairs
        let indexed: Vec<(usize, char)> = s.char_indices().collect();

        for i in 1..indexed.len() {
            let (prev_byte, prev_char) = indexed[i - 1];
            let (curr_byte, curr_char) = indexed[i];

            // Split on lowercase -> uppercase transition
            if prev_char.is_lowercase() && curr_char.is_uppercase() {
                if last_byte < curr_byte {
                    parts.push(&s[last_byte..curr_byte]);
                }
                last_byte = curr_byte;
            }
            // Split on uppercase -> lowercase if preceded by uppercase (e.g., XMLParser -> XML, Parser)
            else if i >= 2 {
                let (prev2_byte, prev2_char) = indexed[i - 2];
                if prev2_char.is_uppercase() && prev_char.is_uppercase() && curr_char.is_lowercase()
                {
                    if last_byte < prev_byte {
                        parts.push(&s[last_byte..prev_byte]);
                    }
                    last_byte = prev_byte;
                    let _ = prev2_byte; // silence unused warning
                }
            }
        }

        if last_byte < s.len() {
            parts.push(&s[last_byte..]);
        }

        if parts.is_empty() {
            parts.push(s);
        }

        parts
    }

    /// Get number of documents in the index
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    /// Get all unique terms in the index
    pub fn terms(&self) -> Vec<&str> {
        self.idf.keys().map(|s| s.as_str()).collect()
    }

    /// Check if a document ID exists
    pub fn contains(&self, id: &str) -> bool {
        self.documents.iter().any(|d| d.id == id)
    }

    /// Get document by ID
    pub fn get(&self, id: &str) -> Option<&str> {
        self.documents
            .iter()
            .find(|d| d.id == id)
            .map(|d| d.text.as_str())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/analysis/bm25/bm25_test.rs"]
mod tests;
