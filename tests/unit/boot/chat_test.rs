use super::{build_messages, keywords, retrieve_snippet, GROUNDING_PROMPT};

#[test]
fn bundled_setup_docs_are_available_without_a_checkout() {
    let snippet = super::retrieve_bundled_snippet("endpoint setup").unwrap();
    assert!(snippet.contains("selfware boot"));
    assert!(snippet.contains("recipe"));
    assert!(super::retrieve_bundled_snippet("xyzzy frobnicate").is_none());
}

#[cfg(unix)]
#[test]
fn retrieval_does_not_follow_symlink_cycles_or_external_files() {
    use std::os::unix::fs::symlink;
    let docs = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(
        outside.path().join("inject.md"),
        "endpoint EXTERNAL_OVERRIDE",
    )
    .unwrap();
    symlink(docs.path(), docs.path().join("cycle")).unwrap();
    symlink(
        outside.path().join("inject.md"),
        docs.path().join("linked.md"),
    )
    .unwrap();
    assert!(retrieve_snippet(docs.path(), "endpoint").is_none());
}

fn write_docs(dir: &tempfile::TempDir) {
    std::fs::write(
        dir.path().join("configuration.md"),
        "# Configuration\n\nThe endpoint key selects the OpenAI-compatible server. \
         Set context_length to the model's real window.\n\n\
         Unrelated paragraph about telemetry and spans.\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("nested")).unwrap();
    std::fs::write(
        dir.path().join("nested").join("doctor.md"),
        "# Doctor\n\n`selfware llm-doctor` checks endpoint reachability, model availability, \
         and tool calling.\n",
    )
    .unwrap();
}

#[test]
fn grounding_prompt_defers_configs_to_cards() {
    assert!(GROUNDING_PROMPT.contains("recipe cards"));
    assert!(GROUNDING_PROMPT.contains("selfware llm-doctor"));
}

#[test]
fn keywords_filter_short_tokens_and_stopwords() {
    let keys = keywords("How do I set the endpoint for selfware?");
    assert!(keys.contains(&"endpoint".to_string()));
    assert!(!keys
        .iter()
        .any(|k| k == "how" || k == "selfware" || k == "set"));
}

#[test]
fn retrieve_snippet_picks_best_keyword_paragraph() {
    let dir = tempfile::tempdir().unwrap();
    write_docs(&dir);
    let snippet = retrieve_snippet(dir.path(), "what does llm-doctor check?").unwrap();
    assert!(snippet.contains("llm-doctor"));
    // It picked the doctor paragraph, not the telemetry one.
    assert!(!snippet.contains("telemetry"));
}

#[test]
fn retrieve_snippet_searches_nested_dirs() {
    let dir = tempfile::tempdir().unwrap();
    write_docs(&dir);
    let snippet = retrieve_snippet(dir.path(), "endpoint context_length window").unwrap();
    assert!(snippet.contains("context_length"));
}

#[test]
fn retrieve_snippet_none_when_nothing_matches() {
    let dir = tempfile::tempdir().unwrap();
    write_docs(&dir);
    assert!(retrieve_snippet(dir.path(), "xyzzy frobnicate").is_none());
    // No keywords at all → None, no panic.
    assert!(retrieve_snippet(dir.path(), "a an I").is_none());
    // Missing dir → None.
    assert!(retrieve_snippet(dir.path().join("nope").as_path(), "endpoint").is_none());
}

#[test]
fn build_messages_embeds_grounding_and_snippet() {
    let body = build_messages("my question", Some("the docs say X"));
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2);
    let system = messages[0]["content"].as_str().unwrap();
    assert!(system.contains("recipe cards"));
    assert!(system.contains("DOCUMENTATION SNIPPET"));
    assert!(system.contains("the docs say X"));
    assert_eq!(messages[1]["role"].as_str().unwrap(), "user");
    assert_eq!(messages[1]["content"].as_str().unwrap(), "my question");
    assert_eq!(body["model"].as_str().unwrap(), "boot-assistant");
}

#[test]
fn build_messages_without_snippet_uses_plain_grounding() {
    let body = build_messages("q", None);
    let system = body["messages"][0]["content"].as_str().unwrap();
    assert_eq!(system, GROUNDING_PROMPT);
}
