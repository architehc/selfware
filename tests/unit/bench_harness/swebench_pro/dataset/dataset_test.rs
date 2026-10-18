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

#[test]
fn repo_slug_and_git_revision_validation_rejects_injection() {
    assert!(is_safe_repo_slug("astropy/astropy"));
    assert!(is_safe_repo_slug("django/django_rest.framework-1"));
    assert!(!is_safe_repo_slug(""));
    assert!(!is_safe_repo_slug("only_owner"));
    assert!(!is_safe_repo_slug("a/b/c"));
    assert!(!is_safe_repo_slug("../etc"));
    assert!(!is_safe_repo_slug("-flag/repo"));
    assert!(!is_safe_repo_slug("owner/-flag"));
    assert!(!is_safe_repo_slug("owner/repo with space"));

    assert!(is_safe_git_revision("deadbeef1234567890"));
    assert!(is_safe_git_revision("v1.2.3-rc_1"));
    assert!(!is_safe_git_revision(""));
    assert!(!is_safe_git_revision("--upload-pack=evil"));
    assert!(!is_safe_git_revision("-f"));
    assert!(!is_safe_git_revision("HEAD..origin/main"));
    assert!(!is_safe_git_revision("sha with space"));
}

#[test]
fn load_instances_rejects_unsafe_repo_or_base_commit_in_jsonl() {
    let dir = tempfile::tempdir().unwrap();
    let bad_commit_jsonl = dir.path().join("bad_commit.jsonl");
    std::fs::write(
        &bad_commit_jsonl,
        r#"{"instance_id":"demo-1","repo":"octocat/hello","base_commit":"--upload-pack=evil","problem_statement":"fix"}"#,
    )
    .unwrap();
    let err = load_instances(&[], 10, Some(&bad_commit_jsonl)).unwrap_err();
    assert!(
        err.to_string().contains("unsafe SWE-bench Pro base_commit"),
        "unexpected error: {err}"
    );

    let bad_repo_jsonl = dir.path().join("bad_repo.jsonl");
    std::fs::write(
        &bad_repo_jsonl,
        r#"{"instance_id":"demo-1","repo":"../outside","base_commit":"deadbeef","problem_statement":"fix"}"#,
    )
    .unwrap();
    let err = load_instances(&[], 10, Some(&bad_repo_jsonl)).unwrap_err();
    assert!(
        err.to_string().contains("unsafe SWE-bench Pro repo"),
        "unexpected error: {err}"
    );
}
