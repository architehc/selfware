use super::build_config_content;

#[test]
fn generated_default_config_passes_agent_context_check() {
    // P0-1: the wizard's own suggested defaults (unknown model
    // `qwen3-coder`) previously produced a config the agent refused to
    // start (32k unknown-model fallback vs 64k default max_tokens →
    // max_context_tokens = 0). The generated body must carry explicit,
    // mutually-fitting token limits.
    let content = build_config_content("http://127.0.0.1:1234/v1", "qwen3-coder", "[\"./**\"]");
    let cfg: crate::config::Config =
        toml::from_str(&content).expect("generated TOML must parse into Config");
    let (budget, reserved) = cfg
        .derive_context_budget()
        .expect("wizard defaults must pass the agent's context check");
    assert_eq!(
        reserved, cfg.max_tokens,
        "wizard defaults must fit without relying on the runtime clamp"
    );
    assert!(budget >= 2048, "conversation budget too small: {budget}");
    // …and through the strict generated-validation path (fallback + fit).
    crate::config::Config::validate_generated_toml(&content).unwrap();
}

#[test]
fn allowed_paths_use_descendant_glob_that_matches_files() {
    // A bare directory allow-list entry matches only the directory itself
    // and DENIES every file inside it — the wizard must emit `<dir>/**`.
    let dir = tempfile::tempdir().unwrap();
    let entry = super::dir_allow_entry(dir.path());
    assert!(entry.ends_with("/**"), "must be a descendant glob: {entry}");

    let canonical_dir = dir.path().canonicalize().unwrap();
    let child = canonical_dir.join("src").join("main.rs");
    let child_str = child.to_string_lossy().to_string();

    let config = crate::config::SafetyConfig {
        allowed_paths: vec![entry],
        ..Default::default()
    };
    let validator =
        crate::safety::path_validator::PathValidator::new(&config, dir.path().to_path_buf());
    assert!(
        validator
            .is_path_in_allowed_list(&child_str, "src/main.rs")
            .unwrap(),
        "<dir>/** must match files inside the chosen project"
    );
    assert!(
        validator
            .is_path_in_allowed_list(&canonical_dir.to_string_lossy(), ".")
            .unwrap(),
        "<dir>/** must also match the chosen directory itself"
    );

    // Regression guard: the old bare-dir form must NOT match children.
    let bare = crate::config::SafetyConfig {
        allowed_paths: vec![dir.path().to_string_lossy().to_string()],
        ..Default::default()
    };
    let bare_validator =
        crate::safety::path_validator::PathValidator::new(&bare, dir.path().to_path_buf());
    assert!(
        !bare_validator
            .is_path_in_allowed_list(&child_str, "src/main.rs")
            .unwrap(),
        "bare dir must not match children (this was the onboarding bug)"
    );
}

#[test]
fn custom_allow_entry_appends_glob_but_keeps_existing_globs() {
    assert_eq!(super::custom_allow_entry("/tmp/proj"), "/tmp/proj/**");
    assert_eq!(super::custom_allow_entry("/tmp/proj/"), "/tmp/proj/**");
    assert_eq!(super::custom_allow_entry(" /data "), "/data/**");
    // Already-globbed entries pass through untouched.
    assert_eq!(super::custom_allow_entry("/tmp/**"), "/tmp/**");
    assert_eq!(super::custom_allow_entry("./**"), "./**");
    assert_eq!(super::custom_allow_entry("~/**"), "~/**");
}

#[test]
fn windows_style_paths_are_toml_escaped() {
    // Backslashes in a basic TOML string form invalid escapes (\U, \s);
    // the generated config must quote them so it parses on Windows.
    let entry = super::custom_allow_entry(r"C:\Users\ada\project");
    assert_eq!(entry, r"C:\Users\ada\project/**");

    let array = super::allowed_paths_toml([entry.clone()]);
    let doc: toml::Value = toml::from_str(&format!("allowed_paths = {array}"))
        .expect("escaped array must be valid TOML");
    let decoded = doc
        .get("allowed_paths")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .unwrap();
    assert_eq!(decoded, entry, "TOML round-trip must preserve the path");

    // Sanity: WITHOUT escaping the same body is invalid TOML (\U escape).
    assert!(toml::from_str::<toml::Value>(&format!("allowed_paths = [\"{entry}\"]")).is_err());

    // The full generated config with such a path must pass validation.
    let content = build_config_content("http://127.0.0.1:1234/v1", "qwen3-coder", &array);
    crate::config::Config::validate_generated_toml(&content).unwrap();
}

#[test]
fn quotes_in_paths_are_toml_escaped() {
    let array = super::allowed_paths_toml([r#"weird"path"#.to_string()]);
    let doc: toml::Value = toml::from_str(&format!("allowed_paths = {array}")).unwrap();
    let decoded = doc
        .get("allowed_paths")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .unwrap();
    assert_eq!(decoded, r#"weird"path"#);
}

#[test]
fn non_terminal_stdin_is_refused() {
    // Piped/EOF stdin must not silently write an all-defaults config.
    assert!(super::ensure_interactive_stdin(true).is_ok());
    let err = super::ensure_interactive_stdin(false).unwrap_err();
    assert!(
        err.to_string().contains("interactive"),
        "error should explain the wizard needs a terminal: {err}"
    );
}

#[test]
fn generated_config_has_no_live_execution_mode_key() {
    let content = build_config_content("http://localhost:8000/v1", "mock", "[\"./**\"]");
    // A live `execution_mode = "..."` key would be silently ignored
    // (#[serde(skip)]) and mislead the user — it must not be emitted.
    assert!(
        !content.contains("execution_mode ="),
        "wizard must not write a live execution_mode key: {content}"
    );
    // The endpoint/model still land, and the mode guidance comment is present.
    assert!(content.contains("http://localhost:8000/v1"));
    assert!(content.contains("model = \"mock\""));
    assert!(content.contains("selfware -y"));
}

#[test]
fn generated_config_has_no_api_key() {
    // The API key is stored in the OS keyring, never written to the config
    // TOML — verify the generated body has no api_key field.
    let content = build_config_content("https://openrouter.ai/api/v1", "gpt-4", "[\".\"]");
    assert!(
        !content.contains("api_key"),
        "config must never contain the api key: {content}"
    );
}

#[test]
fn generated_config_passes_loader_validation() {
    // P0-7: the wizard must never emit a config selfware rejects. Every
    // endpoint/model variant the wizard offers goes through the same
    // validation the loader applies on disk reads.
    for (endpoint, model) in [
        ("http://127.0.0.1:1234/v1", "qwen3-coder"),
        ("https://api.openai.com/v1", "gpt-4"),
        ("https://openrouter.ai/api/v1", "z-ai/glm-5.2"),
    ] {
        let content = build_config_content(endpoint, model, "[\".\"]");
        crate::config::Config::validate_generated_toml(&content)
            .unwrap_or_else(|e| panic!("generated config for {} must validate: {}", endpoint, e));
    }
}

#[test]
fn probe_endpoint_tcp_detects_listener_and_dead_port() {
    // A bound listener is reachable; a closed loopback port is not.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    assert!(super::probe_endpoint_tcp(
        &format!("http://127.0.0.1:{}/v1", port),
        std::time::Duration::from_millis(500)
    ));
    // Port 1 on loopback is (virtually) always closed → fast `false`.
    assert!(!super::probe_endpoint_tcp(
        "http://127.0.0.1:1/v1",
        std::time::Duration::from_millis(200)
    ));
    // Unparseable endpoint → false, never a panic.
    assert!(!super::probe_endpoint_tcp(
        "not a url",
        std::time::Duration::from_millis(50)
    ));
}
