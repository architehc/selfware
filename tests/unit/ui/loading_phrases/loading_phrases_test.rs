use super::*;

#[test]
fn test_loading_phrases_count() {
    assert!(LOADING_PHRASES.len() >= 100);
}

#[test]
fn test_random_phrase_returns_valid() {
    let phrase = random_phrase();
    assert!(!phrase.is_empty());
    assert!(LOADING_PHRASES.contains(&phrase));
}

#[test]
fn test_all_phrases_non_empty() {
    for phrase in LOADING_PHRASES {
        assert!(!phrase.is_empty());
    }
}

#[test]
fn test_all_phrases_end_with_dots() {
    for phrase in LOADING_PHRASES {
        assert!(
            phrase.ends_with("...") || phrase.ends_with(".."),
            "Phrase '{}' should end with dots",
            phrase
        );
    }
}
