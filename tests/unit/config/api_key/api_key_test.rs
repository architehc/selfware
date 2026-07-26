use super::*;

#[test]
fn keyring_account_is_scheme_host_port_scoped() {
    // scheme + host + effective port form the account
    assert_eq!(
        keyring_account_for_endpoint("https://openrouter.ai/api/v1"),
        "https://openrouter.ai:443"
    );
    // http and https to the SAME host must be DIFFERENT accounts (no downgrade)
    assert_ne!(
        keyring_account_for_endpoint("https://openrouter.ai/api/v1"),
        keyring_account_for_endpoint("http://openrouter.ai/api/v1")
    );
    // different hosts differ
    assert_ne!(
        keyring_account_for_endpoint("https://openrouter.ai/api/v1"),
        keyring_account_for_endpoint("https://attacker.example.com/v1")
    );
    // case / path normalize; explicit default port == implicit default port
    assert_eq!(
        keyring_account_for_endpoint("https://OpenRouter.ai/api/v1"),
        keyring_account_for_endpoint("https://openrouter.ai:443/")
    );
    // explicit non-default port is part of the scope
    assert_eq!(
        keyring_account_for_endpoint("http://127.0.0.1:1234/v1"),
        "http://127.0.0.1:1234"
    );
    assert_ne!(
        keyring_account_for_endpoint("http://127.0.0.1:1234/v1"),
        keyring_account_for_endpoint("http://127.0.0.1:9999/v1")
    );
}

#[test]
fn assert_credential_endpoint_safe_rules() {
    // remote http OR userinfo + key -> Err
    assert!(assert_credential_endpoint_safe("http://api.example.com/v1", true).is_err());
    assert!(assert_credential_endpoint_safe("https://user:pass@api.example.com/v1", true).is_err());
    // safe endpoints with key -> Ok
    assert!(assert_credential_endpoint_safe("https://api.example.com/v1", true).is_ok());
    assert!(assert_credential_endpoint_safe("http://127.0.0.1:8000/v1", true).is_ok());
    // no credential -> always Ok (even remote http)
    assert!(assert_credential_endpoint_safe("http://api.example.com/v1", false).is_ok());
}

#[test]
fn insecure_check_is_case_insensitive_on_scheme() {
    // The scheme is case-insensitive per the URL spec, and reqwest
    // normalizes it — so an uppercase/mixed-case http scheme must NOT bypass
    // the plaintext-remote refusal.
    assert!(is_insecure_remote_endpoint("HTTP://attacker.example/v1"));
    assert!(is_insecure_remote_endpoint("HtTp://attacker.example/v1"));
    assert!(assert_credential_endpoint_safe("HTTP://attacker.example/v1", true).is_err());
    // https (any case) and loopback stay allowed.
    assert!(!is_insecure_remote_endpoint("HTTPS://api.example.com/v1"));
    assert!(!is_insecure_remote_endpoint("HTTP://127.0.0.1:8000/v1"));
}

#[test]
fn authorize_request_enforces_transport_before_attaching_key() {
    let client = reqwest::Client::new();
    let url = "http://api.example.com/v1/chat/completions";
    // key + remote http -> refused (never builds a request that leaks it)
    assert!(
        authorize_request(client.post(url), "http://api.example.com/v1", Some("sk-x")).is_err()
    );
    // key + safe endpoint -> allowed
    assert!(
        authorize_request(client.post(url), "https://api.example.com/v1", Some("sk-x")).is_ok()
    );
    assert!(authorize_request(client.post(url), "http://127.0.0.1:8000/v1", Some("sk-x")).is_ok());
    // no key -> allowed regardless of transport
    assert!(authorize_request(client.post(url), "http://api.example.com/v1", None).is_ok());
}

#[test]
fn insecure_remote_endpoint_detection() {
    assert!(is_insecure_remote_endpoint("http://openrouter.ai/api/v1")); // remote http -> insecure
    assert!(!is_insecure_remote_endpoint("https://openrouter.ai/api/v1")); // https ok
    assert!(!is_insecure_remote_endpoint("http://localhost:1234/v1")); // local ok
    assert!(!is_insecure_remote_endpoint("http://127.0.0.1:8000/v1")); // loopback ok
    assert!(!is_insecure_remote_endpoint("http://[::1]:8000/v1")); // ipv6 loopback ok
}

#[test]
fn openrouter_endpoint_detection_is_host_exact() {
    assert!(is_openrouter_endpoint("https://openrouter.ai/api/v1"));
    assert!(is_openrouter_endpoint("https://OpenRouter.AI/api/v1"));
    // Lookalike hosts must NOT match (credential-safety).
    assert!(!is_openrouter_endpoint(
        "https://openrouter.ai.evil.com/api/v1"
    ));
    assert!(!is_openrouter_endpoint(
        "https://api.openrouter.ai.evil.com/v1"
    ));
    assert!(!is_openrouter_endpoint("https://api.openai.com/v1"));
    assert!(!is_openrouter_endpoint("http://127.0.0.1:8000/v1"));
    assert!(!is_openrouter_endpoint("not a url"));
}

#[test]
fn set_api_key_rejects_empty_key() {
    let config = super::super::Config::default();
    assert!(set_api_key_for_endpoint(&config, "").is_err());
    assert!(set_api_key_for_endpoint(&config, "   ").is_err());
}

#[test]
#[ignore = "touches the real OS keyring; run explicitly"]
fn keyring_roundtrip_real() {
    let endpoint = "https://openrouter.ai/api/v1";
    let key = "probe-key-123";
    crate::config::api_key::save_api_key_to_keyring(endpoint, key).unwrap();
    let loaded = crate::config::api_key::load_api_key_from_keyring(endpoint).unwrap();
    assert_eq!(loaded.as_deref(), Some(key));
}
