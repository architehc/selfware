use super::*;

#[test]
fn test_default_denied_paths_includes_git_hooks_and_config() {
    let denied = default_denied_paths();
    assert!(
        denied.contains(&"**/.git/hooks/**".to_string()),
        "default_denied_paths must deny **/.git/hooks/**"
    );
    assert!(
        denied.contains(&"**/.git/config".to_string()),
        "default_denied_paths must deny **/.git/config"
    );
}

#[test]
fn test_union_with_default_denied_paths_adds_without_removing() {
    // The stale 3-entry list old generated configs emitted.
    let merged = union_with_default_denied_paths(vec![
        "**/.env".to_string(),
        "**/secrets/**".to_string(),
        "**/.ssh/**".to_string(),
        "**/vault/**".to_string(),
    ]);
    // Every default survives (explicit entries can only add restrictions)...
    for default in default_denied_paths() {
        assert!(
            merged.contains(&default),
            "default entry '{default}' must survive the union"
        );
    }
    // ...the custom entry is added, and duplicates are not duplicated.
    assert!(merged.contains(&"**/vault/**".to_string()));
    assert_eq!(merged.len(), default_denied_paths().len() + 1);
}

#[test]
fn test_default_denied_paths_toml_parses_back_to_defaults() {
    let literal = default_denied_paths_toml();
    let value: toml::Value = toml::from_str(&format!("denied_paths = {literal}")).unwrap();
    let parsed: Vec<String> = value["denied_paths"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap().to_string())
        .collect();
    assert_eq!(parsed, default_denied_paths());
}
