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
        discovered: false,
    };

    let idf = engine.idf("test");
    assert!(idf > 0.0);
}

#[cfg(unix)]
#[tokio::test]
async fn discovered_reads_skip_hard_links_and_keep_a_singly_linked_neighbor() {
    use super::super::budget::TokenBudget;

    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let neighbor = root.path().join("neighbor.rs");
    std::fs::write(&neighbor, "pub fn visible_neighbor() {}\n").unwrap();
    let secret = outside.path().join("secret.rs");
    std::fs::write(&secret, "pub fn hard_link_secret_symbol() {}\n").unwrap();
    let linked = root.path().join("linked.rs");
    std::fs::hard_link(&secret, &linked).unwrap();

    let files = vec![neighbor, linked.clone()];
    let mut engine = CodeQueryEngine::new();
    engine.discovered = true;
    engine.build_index(&files).await.unwrap();
    let budget = TokenBudget::new(usize::MAX);
    let results = engine
        .search("visible_neighbor hard_link_secret_symbol", &files, &budget)
        .await
        .unwrap();
    let rendered = format!("{results:?}");
    assert!(rendered.contains("visible_neighbor"), "{rendered}");
    assert!(!rendered.contains("hard_link_secret_symbol"), "{rendered}");

    let exact = CodeQueryEngine::new();
    let exact_results = exact
        .search(
            "hard_link_secret_symbol",
            std::slice::from_ref(&linked),
            &budget,
        )
        .await
        .unwrap();
    let exact_rendered = format!("{exact_results:?}");
    assert!(
        exact_rendered.contains("hard_link_secret_symbol"),
        "{exact_rendered}"
    );
}
