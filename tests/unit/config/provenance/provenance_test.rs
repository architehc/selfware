use super::*;

#[test]
fn label_for_default() {
    assert_eq!(ConfigSource::Default.label(), "default");
}

#[test]
fn label_for_config_file() {
    let s = ConfigSource::ConfigFile(PathBuf::from("/tmp/foo/selfware.toml"));
    assert_eq!(s.label(), "/tmp/foo/selfware.toml");
}

#[test]
fn label_for_env_var() {
    let s = ConfigSource::EnvVar("SELFWARE_ENDPOINT".to_string());
    assert_eq!(s.label(), "SELFWARE_ENDPOINT env");
}

#[test]
fn label_for_profile() {
    let s = ConfigSource::Profile("qwen3.6-*".to_string());
    assert_eq!(s.label(), "profile: qwen3.6-*");
}

#[test]
fn label_for_cli_arg() {
    let s = ConfigSource::CliArg("--model".to_string());
    assert_eq!(s.label(), "cli: --model");
}

#[test]
fn sources_set_and_get() {
    let mut s = ConfigSources::new();
    s.set("endpoint", ConfigSource::Default);
    s.set(
        "model",
        ConfigSource::ConfigFile(PathBuf::from("/tmp/x.toml")),
    );
    assert_eq!(s.get("endpoint"), Some(&ConfigSource::Default));
    assert!(matches!(s.get("model"), Some(ConfigSource::ConfigFile(_))));
    assert_eq!(s.get("missing"), None);
    assert_eq!(s.len(), 2);
}

#[test]
fn sources_iter_is_sorted() {
    let mut s = ConfigSources::new();
    s.set("model", ConfigSource::Default);
    s.set("endpoint", ConfigSource::Default);
    s.set("temperature", ConfigSource::Default);
    let keys: Vec<&String> = s.iter().map(|(k, _)| k).collect();
    assert_eq!(keys, vec!["endpoint", "model", "temperature"]);
}

// ── display_path_for (0.9.1: long absolute config path on every command) ──

#[test]
fn config_path_is_shown_relative_to_cwd_then_home() {
    use std::path::Path;
    let cwd = Path::new("/home/u/proj");
    let home = Path::new("/home/u");
    assert_eq!(
        display_path_for(
            Path::new("/home/u/proj/selfware.toml"),
            Some(cwd),
            Some(home)
        ),
        "selfware.toml"
    );
    assert_eq!(
        display_path_for(
            Path::new("/home/u/.config/selfware/config.toml"),
            Some(cwd),
            Some(home)
        ),
        "~/.config/selfware/config.toml"
    );
    assert_eq!(
        display_path_for(Path::new("/etc/selfware.toml"), Some(cwd), Some(home)),
        "/etc/selfware.toml"
    );
    // Already relative (as given on the command line): unchanged.
    assert_eq!(
        display_path_for(Path::new("selfware.toml"), Some(cwd), Some(home)),
        "selfware.toml"
    );
}

#[test]
fn config_line_suppression_round_trips() {
    let prior = config_line_suppressed();
    set_config_line_suppressed(true);
    assert!(config_line_suppressed());
    set_config_line_suppressed(prior);
}
