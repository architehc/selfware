//! Semantic Code Query Engine with BM25 Ranking
//!
//! Provides search across codebase using BM25 ranking, with token-aware
//! result packing for limited context windows.

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

use super::budget::TokenBudget;
use super::parser::{parse, Language, ParsedFile, Symbol};
use crate::token_count::estimate_content_tokens;

/// BM25 ranking parameters
const BM25_K1: f64 = 1.2;
const BM25_B: f64 = 0.75;

/// Code query engine with BM25 indexing
pub struct CodeQueryEngine {
    /// Document frequency: term -> number of documents containing it
    doc_freq: HashMap<String, usize>,
    /// Total number of documents
    total_docs: usize,
    /// Average document length (in tokens)
    avg_doc_len: f64,
}

/// A ranked search result
#[derive(Debug, Clone)]
pub struct RankedResult {
    pub path: PathBuf,
    pub relevance: f64,
    pub matched_symbols: Vec<Symbol>,
}

/// Query results with token budget awareness
#[derive(Debug, Clone)]
pub struct QueryResults {
    pub results: Vec<RankedResult>,
    pub tokens_used: usize,
    pub total_matches: usize,
}

impl CodeQueryEngine {
    /// Create a new query engine
    pub fn new() -> Self {
        Self {
            doc_freq: HashMap::new(),
            total_docs: 0,
            avg_doc_len: 0.0,
        }
    }

    /// Rank files by relevance to query
    pub async fn rank_files(&self, files: &[(PathBuf, f64)], query: &str) -> Vec<(PathBuf, f64)> {
        let query_terms = tokenize(query);
        let mut ranked = Vec::new();

        for (path, base_score) in files {
            // Read and parse file
            if let Ok(content) = tokio::fs::read_to_string(path).await {
                let language = Language::detect(path, None);
                let parsed = parse(&content, language);
                
                // Calculate BM25 score
                let score = self.bm25_score(&parsed, &query_terms, content.len());
                let combined_score = score * base_score;

                ranked.push((path.clone(), combined_score));
            } else {
                // If can't read, use base score
                ranked.push((path.clone(), *base_score));
            }
        }

        // Sort by relevance descending
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked
    }

    /// Execute a query with token budget
    pub async fn search(
        &self,
        query: &str,
        files: &[PathBuf],
        budget: &TokenBudget,
    ) -> Result<QueryResults> {
        let query_terms = tokenize(query);
        let mut results = Vec::new();
        let mut tokens_used = 0;

        for path in files {
            if tokens_used >= budget.remaining() {
                break;
            }

            if let Ok(content) = tokio::fs::read_to_string(path).await {
                let language = Language::detect(path, None);
                let parsed = parse(&content, language);
                
                // Calculate relevance score before consuming symbols
                let score = self.bm25_score(&parsed, &query_terms, content.len());
                
                // Find matching symbols
                let matched: Vec<Symbol> = parsed
                    .symbols
                    .into_iter()
                    .filter(|s| self.symbol_matches(s, &query_terms))
                    .collect();

                if !matched.is_empty() {
                    
                    // Calculate token cost
                    let symbol_tokens: usize = matched.iter()
                        .map(|s| estimate_content_tokens(&s.render()))
                        .sum();

                    if tokens_used + symbol_tokens <= budget.remaining() {
                        results.push(RankedResult {
                            path: path.clone(),
                            relevance: score,
                            matched_symbols: matched,
                        });
                        tokens_used += symbol_tokens;
                    }
                }
            }
        }

        let total_matches = results.len();

        Ok(QueryResults {
            results,
            tokens_used,
            total_matches,
        })
    }

    /// Calculate BM25 score for a document
    fn bm25_score(&self, parsed: &ParsedFile, query_terms: &[String], doc_len: usize) -> f64 {
        let mut score = 0.0;
        let doc_len_norm = doc_len as f64 / self.avg_doc_len.max(1.0);

        for term in query_terms {
            let tf = self.term_frequency(parsed, term);
            let idf = self.idf(term);

            let numerator = tf * (BM25_K1 + 1.0);
            let denominator = tf + BM25_K1 * (1.0 - BM25_B + BM25_B * doc_len_norm);

            score += idf * numerator / denominator;
        }

        // Boost for documentation presence
        if parsed.module_doc.is_some() {
            score *= 1.1;
        }

        score
    }

    /// Calculate inverse document frequency
    fn idf(&self, term: &str) -> f64 {
        let doc_freq = self.doc_freq.get(term).copied().unwrap_or(1);
        let n = self.total_docs.max(doc_freq);
        
        ((n as f64 - doc_freq as f64 + 0.5) / (doc_freq as f64 + 0.5) + 1.0)
            .ln()
    }

    /// Count term frequency in parsed file
    fn term_frequency(&self, parsed: &ParsedFile, term: &str) -> f64 {
        let mut count = 0.0;
        let term_lower = term.to_lowercase();

        // Count in symbol names
        for sym in &parsed.symbols {
            if sym.name.to_lowercase().contains(&term_lower) {
                count += 3.0; // Name matches are weighted higher
            }
            if sym.signature.to_lowercase().contains(&term_lower) {
                count += 1.0;
            }
            if let Some(doc) = &sym.documentation {
                if doc.to_lowercase().contains(&term_lower) {
                    count += 0.5;
                }
            }
        }

        // Count in module documentation
        if let Some(doc) = &parsed.module_doc {
            let matches = doc.to_lowercase().matches(&term_lower).count();
            count += matches as f64 * 0.5;
        }

        count
    }

    /// Check if a symbol matches the query
    fn symbol_matches(&self, symbol: &Symbol, query_terms: &[String]) -> bool {
        let name_lower = symbol.name.to_lowercase();
        let sig_lower = symbol.signature.to_lowercase();

        query_terms.iter().any(|term| {
            let term_lower = term.to_lowercase();
            name_lower.contains(&term_lower) || sig_lower.contains(&term_lower)
        })
    }

    /// Build index from files (call this before searching for better results)
    pub async fn build_index(&mut self, files: &[PathBuf]) -> Result<()> {
        self.total_docs = files.len();
        let mut total_len = 0;
        let mut term_doc_freq: HashMap<String, usize> = HashMap::new();

        for path in files {
            if let Ok(content) = tokio::fs::read_to_string(path).await {
                total_len += content.len();
                let terms = tokenize(&content);
                
                // Count unique terms per document
                let mut seen = std::collections::HashSet::new();
                for term in terms {
                    if seen.insert(term.clone()) {
                        *term_doc_freq.entry(term).or_default() += 1;
                    }
                }
            }
        }

        self.avg_doc_len = if self.total_docs > 0 {
            total_len as f64 / self.total_docs as f64
        } else {
            0.0
        };

        self.doc_freq = term_doc_freq;

        Ok(())
    }
}

impl Default for CodeQueryEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple tokenizer for code search
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty() && s.len() > 1)
        .map(|s| s.to_string())
        .collect()
}

/// Find symbols related to a query across multiple files
pub async fn find_related_symbols(
    query: &str,
    files: &[PathBuf],
    max_results: usize,
) -> Result<Vec<(PathBuf, Symbol)>> {
    let engine = CodeQueryEngine::new();
    let budget = TokenBudget::new(usize::MAX); // No budget limit for this search
    
    let results = engine.search(query, files, &budget).await?;
    
    let mut all_symbols = Vec::new();
    for result in results.results {
        for symbol in result.matched_symbols {
            all_symbols.push((result.path.clone(), symbol));
        }
    }

    // Sort by relevance (we could add more sophisticated ranking here)
    all_symbols.truncate(max_results);
    
    Ok(all_symbols)
}

/// Extract keywords from a natural language query
pub fn extract_keywords(query: &str) -> Vec<String> {
    // Remove common stop words
    let stop_words: std::collections::HashSet<&str> = [
        "the", "a", "an", "is", "are", "was", "were", "be", "been",
        "being", "have", "has", "had", "do", "does", "did", "will",
        "would", "could", "should", "may", "might", "must", "shall",
        "can", "need", "dare", "ought", "used", "to", "of", "in",
        "for", "on", "with", "at", "by", "from", "as", "into",
        "through", "during", "before", "after", "above", "below",
        "between", "under", "and", "but", "or", "yet", "so", "if",
        "because", "although", "though", "while", "where", "when",
        "that", "which", "who", "whom", "whose", "what", "this",
        "these", "those", "i", "you", "he", "she", "it", "we", "they",
        "me", "him", "her", "us", "them", "my", "your", "his", "its",
        "our", "their", "how", "does", "work", "use", "using", "get",
    ].iter().cloned().collect();

    tokenize(query)
        .into_iter()
        .filter(|t| !stop_words.contains(t.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize() {
        let text = "Hello world! How are you?";
        let tokens = tokenize(text);
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
    }

    #[test]
    fn test_extract_keywords() {
        let query = "How does the agent loop handle max iterations?";
        let keywords = extract_keywords(query);
        assert!(keywords.contains(&"agent".to_string()));
        assert!(keywords.contains(&"loop".to_string()));
        assert!(keywords.contains(&"iterations".to_string()));
        // Stop words should be removed
        assert!(!keywords.contains(&"how".to_string()));
    }

    #[test]
    fn test_bm25_idf() {
        let engine = CodeQueryEngine {
            doc_freq: [("test".to_string(), 5)].into_iter().collect(),
            total_docs: 100,
            avg_doc_len: 100.0,
        };

        let idf = engine.idf("test");
        assert!(idf > 0.0);
    }
}
