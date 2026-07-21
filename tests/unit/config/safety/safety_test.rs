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
