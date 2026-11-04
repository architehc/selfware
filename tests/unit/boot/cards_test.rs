use super::{find_card, CARDS};

#[test]
fn find_card_by_name() {
    let card = find_card("openrouter-free").expect("openrouter-free card exists");
    assert_eq!(card.endpoint, "https://openrouter.ai/api/v1");
    assert_eq!(card.model, "nvidia/nemotron-3-ultra-550b-a55b:free");
    assert_eq!(card.max_tokens, 65_536);
    assert_eq!(card.context_length, 1_000_000);
    assert!(card.native_function_calling);
    assert!((card.temperature - 1.0).abs() < f32::EPSILON);
}

#[test]
fn find_card_unknown_name_returns_none() {
    assert!(find_card("does-not-exist").is_none());
    assert!(find_card("").is_none());
}

#[test]
fn every_card_has_complete_fields() {
    assert!(CARDS.len() >= 5);
    for card in CARDS {
        assert!(!card.name.is_empty(), "card name");
        assert!(card.endpoint.ends_with("/v1"), "{} endpoint", card.name);
        assert!(card.max_tokens > 0, "{} max_tokens", card.name);
        assert!(card.context_length > 0, "{} context_length", card.name);
        assert!(
            card.context_length > card.max_tokens,
            "{} context must exceed max_tokens",
            card.name
        );
        assert!(!card.notes.is_empty(), "{} notes", card.name);
        assert!(!card.hint.is_empty(), "{} hint", card.name);
        // The hint is the one-line troubleshooting pointer shown on doctor
        // failure — keep it one line.
        assert!(!card.hint.contains('\n'), "{} hint is one line", card.name);
    }
}

#[test]
fn server_detected_cards_require_a_model_override() {
    let vllm = find_card("vllm").unwrap();
    assert!(vllm.needs_model_detection());
    assert!(vllm.render_config(None, None).is_err());
    assert!(vllm.render_config(Some("  "), None).is_err());
    assert!(vllm.render_config(Some("served-model"), None).is_ok());
    assert!(!find_card("ollama").unwrap().needs_model_detection());
}

/// e2e-ish: every card renders TOML that the real loader accepts and that
/// round-trips back to exactly the card's values.
#[test]
fn every_card_passes_structural_validation() {
    for card in CARDS {
        let model = if card.needs_model_detection() {
            Some("test-served-model")
        } else {
            None
        };
        let body = card
            .render_config(model, None)
            .unwrap_or_else(|e| panic!("{} renders: {}", card.name, e));
        crate::config::Config::validate_generated_toml(&body)
            .unwrap_or_else(|e| panic!("{} validates: {}", card.name, e));

        let cfg: crate::config::Config =
            toml::from_str(&body).unwrap_or_else(|e| panic!("{} parses: {}", card.name, e));
        assert_eq!(cfg.endpoint, card.endpoint, "{}", card.name);
        assert_eq!(cfg.model, model.unwrap_or(card.model), "{}", card.name);
        assert_eq!(cfg.max_tokens, card.max_tokens, "{}", card.name);
        assert_eq!(cfg.context_length, card.context_length, "{}", card.name);
        assert!(
            (cfg.temperature - card.temperature).abs() < f32::EPSILON,
            "{}",
            card.name
        );
        assert_eq!(
            cfg.agent.native_function_calling, card.native_function_calling,
            "{}",
            card.name
        );
    }
}

/// The flagship card specifically: the openrouter-free config must pass
/// structural validation untouched (no overrides).
#[test]
fn openrouter_free_card_config_passes_structural_validation() {
    let card = find_card("openrouter-free").unwrap();
    let body = card.render_config(None, None).unwrap();
    crate::config::Config::validate_generated_toml(&body).unwrap();
    let cfg: crate::config::Config = toml::from_str(&body).unwrap();
    assert_eq!(cfg.model, "nvidia/nemotron-3-ultra-550b-a55b:free");
    assert_eq!(cfg.context_length, 1_000_000);
    assert!(cfg.agent.native_function_calling);
    assert!(card.hint.contains("429"));
}

#[test]
fn context_override_replaces_card_default() {
    let ollama = find_card("ollama").unwrap();
    let body = ollama
        .render_config(Some("qwen3:8b"), Some(131_072))
        .unwrap();
    let cfg: crate::config::Config = toml::from_str(&body).unwrap();
    assert_eq!(cfg.model, "qwen3:8b");
    assert_eq!(cfg.context_length, 131_072);
}

#[test]
fn selfware_design_and_local_bundled_cards_pass_validation() {
    let cloud = find_card("selfware-design").expect("selfware-design card exists");
    assert_eq!(cloud.endpoint, "https://llm.selfware.design/v1");
    assert_eq!(cloud.model, "deepseek-v4.1-flash");
    assert_eq!(cloud.context_length, 1_048_576);
    let cloud_toml = cloud.render_config(None, None).unwrap();
    crate::config::Config::validate_generated_toml(&cloud_toml).unwrap();

    let bundled = find_card("local-bundled").expect("local-bundled card exists");
    assert_eq!(bundled.endpoint, "http://127.0.0.1:8787/v1");
    assert_eq!(bundled.model, "boot-assistant");
    assert_eq!(bundled.context_length, 8_192);
    let bundled_toml = bundled.render_config(None, None).unwrap();
    crate::config::Config::validate_generated_toml(&bundled_toml).unwrap();

    for card_name in ["local-lfm3-1gb", "local-lfm3-slq"] {
        let card = find_card(card_name).expect(card_name);
        assert_eq!(card.endpoint, "http://127.0.0.1:8787/v1");
        let toml = card.render_config(None, None).unwrap();
        crate::config::Config::validate_generated_toml(&toml).unwrap();
    }
}
