use super::*;
use serde_json::json;

#[test]
fn instance_id_traversal_is_rejected() {
    // Legit SWE-bench Pro ids pass.
    assert!(is_safe_instance_id("astropy__astropy-12345"));
    assert!(is_safe_instance_id("django__django-9.2rc1"));
    assert!(is_safe_instance_id("a..b")); // embedded dots, still one component
                                          // Traversal / separators / specials are rejected.
    assert!(!is_safe_instance_id(".."));
    assert!(!is_safe_instance_id("."));
    assert!(!is_safe_instance_id(""));
    assert!(!is_safe_instance_id("../../etc/passwd"));
    assert!(!is_safe_instance_id("foo/bar"));
    assert!(!is_safe_instance_id("/abs/path"));
    assert!(!is_safe_instance_id("a/../../b"));
}

#[test]
fn coerce_handles_raw_array() {
    assert_eq!(
        coerce_string_list(&json!(["a", "b"])),
        vec!["a".to_string(), "b".to_string()]
    );
}

#[test]
fn coerce_handles_stringified_array() {
    assert_eq!(
        coerce_string_list(&json!("[\"a\", \"b\"]")),
        vec!["a".to_string(), "b".to_string()]
    );
}

#[test]
fn coerce_handles_plain_string() {
    assert_eq!(coerce_string_list(&json!("solo")), vec!["solo".to_string()]);
}

#[test]
fn coerce_handles_null() {
    assert!(coerce_string_list(&json!(null)).is_empty());
}
