use super::*;

#[test]
fn test_hypothesis_result_frost_on_compile_failure() {
    let result = HypothesisResult {
        id: "test".into(),
        description: "test".into(),
        compiled: false,
        sab_result: None,
        fitness: None,
        composite_score: 0.0,
        rating: GenerationRating::Frost,
        patch: String::new(),
    };
    assert_eq!(result.rating, GenerationRating::Frost);
    assert_eq!(result.composite_score, 0.0);
}

#[test]
fn test_hypothesis_result_bloom_rating() {
    let result = HypothesisResult {
        id: "bloom-test".into(),
        description: "All tests pass".into(),
        compiled: true,
        sab_result: None,
        fitness: None,
        composite_score: 100.0,
        rating: GenerationRating::Bloom,
        patch: "--- a/file\n+++ b/file".into(),
    };
    assert_eq!(result.rating, GenerationRating::Bloom);
    assert_eq!(result.composite_score, 100.0);
    assert!(result.compiled);
}

#[test]
fn test_hypothesis_result_grow_rating() {
    let result = HypothesisResult {
        id: "grow-test".into(),
        description: "Most tests pass".into(),
        compiled: true,
        sab_result: None,
        fitness: None,
        composite_score: 96.0,
        rating: GenerationRating::Grow,
        patch: String::new(),
    };
    assert_eq!(result.rating, GenerationRating::Grow);
    assert!(result.composite_score > 0.0);
}

#[test]
fn test_hypothesis_clone() {
    let h = Hypothesis {
        id: "h1".into(),
        description: "Test mutation".into(),
        patch: "diff content".into(),
        target_files: vec![PathBuf::from("src/lib.rs")],
        property_test: Some("test_prop".into()),
    };
    let cloned = h.clone();
    assert_eq!(cloned.id, "h1");
    assert_eq!(cloned.target_files.len(), 1);
    assert_eq!(cloned.property_test, Some("test_prop".into()));
}
