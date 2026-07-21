use super::*;

#[test]
fn test_qa_language_display() {
    assert_eq!(QaLanguage::Rust.to_string(), "Rust");
    assert_eq!(QaLanguage::Python.to_string(), "Python");
    assert_eq!(QaLanguage::Node.to_string(), "Node");
    assert_eq!(QaLanguage::Go.to_string(), "Go");
    assert_eq!(QaLanguage::Unknown.to_string(), "Unknown");
}

#[test]
fn test_count_pattern() {
    assert_eq!(count_pattern("error: foo\nerror: bar", "error"), 2);
    assert_eq!(count_pattern("Warning: something", "warning"), 1);
    assert_eq!(count_pattern("all good", "error"), 0);
}

#[test]
fn test_qa_language_detect() {
    // Can't easily test without creating temp dirs, but verify the logic path
    let tmp = std::env::temp_dir().join("nonexistent_qa_test");
    let lang = QaLanguage::detect(&tmp);
    assert_eq!(lang, QaLanguage::Unknown);
}
