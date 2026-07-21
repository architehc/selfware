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
