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

        for token in query_tokens {
            if let Some(&idf) = self.idf.get(token) {
                let tf = *doc.term_freqs.get(token).unwrap_or(&0) as f32;
                if tf > 0.0 {
                    // BM25 scoring formula
                    let numerator = tf * (self.k1 + 1.0);
                    let denominator = tf + self.k1 * (1.0 - self.b + self.b * (dl / avgdl));
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
mod tests {
    use super::*;

    #[test]
    fn test_bm25_basic_search() {
        let mut index = BM25Index::new();
        index.add("doc1", "the quick brown fox jumps over the lazy dog");
        index.add("doc2", "a quick brown dog outpaces a lazy fox");
        index.add("doc3", "the lazy dog sleeps all day");

        let results = index.search("quick fox", 10);
        assert!(!results.is_empty());
        // doc1 and doc2 should rank higher than doc3
        assert!(results[0].id == "doc1" || results[0].id == "doc2");
    }

    #[test]
    fn test_bm25_empty_index() {
        let mut index = BM25Index::new();
        let results = index.search("test", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_bm25_empty_query() {
        let mut index = BM25Index::new();
        index.add("doc1", "hello world");
        let results = index.search("", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_bm25_no_matches() {
        let mut index = BM25Index::new();
        index.add("doc1", "hello world");
        let results = index.search("xyz123", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_bm25_camel_case_tokenization() {
        let mut index = BM25Index::new();
        index.add("doc1", "getUserName returns the user name");
        index.add("doc2", "setPassword changes password");

        let results = index.search("user", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "doc1");
    }

    #[test]
    fn test_bm25_snake_case_tokenization() {
        let mut index = BM25Index::new();
        index.add("doc1", "get_user_name returns the user name");
        index.add("doc2", "set_password changes password");

        let results = index.search("user", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "doc1");
    }

    #[test]
    fn test_bm25_code_search() {
        let mut index = BM25Index::new();
        index.add(
            "fn1",
            "pub fn execute_workflow(&self, name: &str) -> Result<()>",
        );
        index.add("fn2", "pub fn parse_config(path: &Path) -> Config");
        index.add("fn3", "pub fn run_tests(&self) -> TestResult");

        let results = index.search("workflow execute", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "fn1");

        let results = index.search("config", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "fn2");
    }

    #[test]
    fn test_bm25_ranking() {
        let mut index = BM25Index::new();
        // doc1 has "error" twice
        index.add("doc1", "error handling for error cases");
        // doc2 has "error" once
        index.add("doc2", "error handling");
        // doc3 has no "error"
        index.add("doc3", "success handling");

        let results = index.search("error", 10);
        assert_eq!(results.len(), 2);
        // doc1 should rank higher due to higher term frequency
        assert_eq!(results[0].id, "doc1");
        assert_eq!(results[1].id, "doc2");
    }

    #[test]
    fn test_bm25_remove() {
        let mut index = BM25Index::new();
        index.add("doc1", "hello world");
        index.add("doc2", "hello universe");

        assert!(index.remove("doc1"));
        assert!(!index.remove("doc1")); // Already removed

        let results = index.search("hello", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "doc2");
    }

    #[test]
    fn test_bm25_clear() {
        let mut index = BM25Index::new();
        index.add("doc1", "hello world");
        index.add("doc2", "hello universe");

        index.clear();
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_bm25_batch_add() {
        let mut index = BM25Index::new();
        index.add_batch(vec![
            ("doc1".to_string(), "hello world".to_string()),
            ("doc2".to_string(), "hello universe".to_string()),
        ]);

        assert_eq!(index.len(), 2);
        let results = index.search("hello", 10);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_bm25_contains() {
        let mut index = BM25Index::new();
        index.add("doc1", "hello world");

        assert!(index.contains("doc1"));
        assert!(!index.contains("doc2"));
    }

    #[test]
    fn test_bm25_get() {
        let mut index = BM25Index::new();
        index.add("doc1", "hello world");

        assert_eq!(index.get("doc1"), Some("hello world"));
        assert_eq!(index.get("doc2"), None);
    }

    #[test]
    fn test_tokenize_mixed() {
        let tokens = BM25Index::tokenize("getUserName_v2 with XMLParser");
        assert!(tokens.contains(&"get".to_string()));
        assert!(tokens.contains(&"user".to_string()));
        assert!(tokens.contains(&"name".to_string()));
        assert!(tokens.contains(&"xml".to_string()));
        assert!(tokens.contains(&"parser".to_string()));
    }

    #[test]
    fn test_split_camel_case() {
        assert_eq!(
            BM25Index::split_camel_case("getUserName"),
            vec!["get", "User", "Name"]
        );
        assert_eq!(
            BM25Index::split_camel_case("XMLParser"),
            vec!["XML", "Parser"]
        );
        assert_eq!(BM25Index::split_camel_case("ID"), vec!["ID"]);
        assert_eq!(BM25Index::split_camel_case("simple"), vec!["simple"]);
    }

    #[test]
    fn test_split_camel_case_unicode() {
        // Test that Unicode characters don't cause panics
        // Note: é is lowercase, X is uppercase, so it splits (correct behavior)
        assert_eq!(BM25Index::split_camel_case("éX"), vec!["é", "X"]);
        // All same case - no split
        assert_eq!(BM25Index::split_camel_case("日本語"), vec!["日本語"]);
        // café (lowercase) + Latte (uppercase) = split
        assert_eq!(
            BM25Index::split_camel_case("caféLatte"),
            vec!["café", "Latte"]
        );
        // αβγ (lowercase Greek) + Δ (uppercase Greek) = split
        assert_eq!(BM25Index::split_camel_case("αβγΔ"), vec!["αβγ", "Δ"]);
        // Empty string
        assert_eq!(BM25Index::split_camel_case(""), vec![""]);
        // Multi-byte chars that were causing panics before
        assert_eq!(BM25Index::split_camel_case("über"), vec!["über"]);
        assert_eq!(
            BM25Index::split_camel_case("naïveMethod"),
            vec!["naïve", "Method"]
        );
    }

    #[test]
    fn test_bm25_upsert() {
        let mut index = BM25Index::new();
        index.add("doc1", "original content");
        index.add("doc1", "updated content"); // Should replace

        assert_eq!(index.len(), 1);

        let results = index.search("original", 10);
        assert!(
            results.is_empty(),
            "original should not be found after update"
        );

        let results = index.search("updated", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "doc1");
    }

    #[test]
    fn test_bm25_remove_all() {
        let mut index = BM25Index::new();
        // Manually add duplicates by bypassing upsert (for testing)
        let text = "test content".to_string();
        let tokens = BM25Index::tokenize(&text);
        let mut term_freqs = std::collections::HashMap::new();
        for token in &tokens {
            *term_freqs.entry(token.clone()).or_insert(0u32) += 1;
        }
        // Directly push to documents to simulate duplicates
        index.documents.push(super::Document {
            id: "dup".to_string(),
            text: text.clone(),
            term_freqs: term_freqs.clone(),
            length: tokens.len() as u32,
        });
        index.documents.push(super::Document {
            id: "dup".to_string(),
            text: text.clone(),
            term_freqs: term_freqs.clone(),
            length: tokens.len() as u32,
        });
        index.dirty = true;

        assert_eq!(index.len(), 2);
        let removed = index.remove_all("dup");
        assert_eq!(removed, 2);
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_bm25_limit() {
        let mut index = BM25Index::new();
        for i in 0..100 {
            index.add(format!("doc{}", i), format!("test document number {}", i));
        }

        let results = index.search("test", 5);
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_bm25_idf_rare_terms() {
        let mut index = BM25Index::new();
        // Add many documents with "common"
        for i in 0..10 {
            index.add(format!("doc{}", i), format!("common word {}", i));
        }
        // Add one document with "rare"
        index.add("rare_doc", "rare unique term");

        let results = index.search("rare", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "rare_doc");

        // Rare term should have higher IDF
        index.rebuild();
        let rare_idf = index.idf.get("rare").unwrap_or(&0.0);
        let common_idf = index.idf.get("common").unwrap_or(&0.0);
        assert!(rare_idf > common_idf);
    }

    #[test]
    fn test_bm25_with_params() {
        let index = BM25Index::with_params(2.0, 0.5);
        assert_eq!(index.k1, 2.0);
        assert_eq!(index.b, 0.5);
    }

    #[test]
    fn test_bm25_default() {
        let index = BM25Index::default();
        assert_eq!(index.k1, 1.5);
        assert_eq!(index.b, 0.75);
    }
}
