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
