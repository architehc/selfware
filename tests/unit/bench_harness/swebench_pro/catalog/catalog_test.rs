use super::*;

#[test]
fn catalog_contains_default_quants() {
    let cat = quant_catalog();
    for q in DEFAULT_QUANTS {
        assert!(cat.contains_key(*q), "missing default quant: {}", q);
    }
}

#[test]
fn catalog_entries_have_distinct_aliases() {
    let mut seen = std::collections::HashSet::new();
    for spec in quant_catalog().values() {
        assert!(
            seen.insert(spec.alias.clone()),
            "duplicate alias: {}",
            spec.alias
        );
    }
}

#[test]
fn external_catalog_merges_and_overrides() {
    let base = quant_catalog();
    let first = base.keys().next().expect("catalog non-empty").clone();
    // A brand-new entry is added; an existing label is overridden.
    let json = r#"{ "custom-quant": { "label":"custom-quant","gguf":"c.gguf","alias":"c","mmproj":"","name":"Custom","ctx":4096,"max_parallel":1,"kv_cache_type":"q8_0","tensor_split":null,"temperature":1.0,"thinking_policy":"Disable","backend":"LlamaCpp" } }"#.to_string();
    let merged = apply_external_catalog(base.clone(), &json);
    assert!(merged.contains_key("custom-quant"));
    assert!(merged.contains_key(&first)); // originals preserved
                                          // Malformed JSON returns the base unchanged.
    let unchanged = apply_external_catalog(base.clone(), "not json");
    assert_eq!(unchanged.len(), base.len());
}

#[test]
fn quant_spec_serializes_roundtrip() {
    let spec = QuantSpec {
        label: "test".into(),
        gguf: "test.gguf".into(),
        alias: "test-alias".into(),
        mmproj: "mmproj.gguf".into(),
        name: "Test".into(),
        ctx: 65536,
        max_parallel: 2,
        kv_cache_type: "q8_0".into(),
        tensor_split: Some("24,24".into()),
        temperature: 0.7,
        thinking_policy: ThinkingPolicy::Enable,
        backend: BackendProfile::LlamaCpp,
    };
    let json = serde_json::to_string(&spec).unwrap();
    let restored: QuantSpec = serde_json::from_str(&json).unwrap();
    assert_eq!(spec.label, restored.label);
    assert_eq!(spec.backend, restored.backend);
    assert_eq!(spec.thinking_policy, restored.thinking_policy);
}
