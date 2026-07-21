use super::*;

#[test]
fn test_extract_version_semver() {
    assert_eq!(
        extract_version("rustc 1.79.0 (129f3b996 2024-06-10)"),
        Some("1.79.0".to_string())
    );
}

#[test]
fn test_extract_version_with_v_prefix() {
    assert_eq!(extract_version("v20.11.1"), Some("20.11.1".to_string()));
}

#[test]
fn test_extract_version_git() {
    assert_eq!(
        extract_version("git version 2.45.2"),
        Some("2.45.2".to_string())
    );
}

#[test]
fn test_extract_version_none() {
    assert_eq!(extract_version("no version here"), None);
}

#[test]
fn test_category_display() {
    assert_eq!(Category::Core.to_string(), "Core (Required)");
    assert_eq!(Category::Languages.to_string(), "Languages (Optional)");
    assert_eq!(Category::Platform.to_string(), "Platform Tools");
    assert_eq!(Category::Configuration.to_string(), "Configuration");
}

#[test]
fn test_overall_health_display() {
    assert_eq!(OverallHealth::Healthy.to_string(), "healthy");
    assert_eq!(OverallHealth::Degraded.to_string(), "degraded");
    assert_eq!(OverallHealth::Broken.to_string(), "broken");
}

#[test]
fn test_health_determination() {
    // All OK => Healthy
    let report = DoctorReport {
        checks: vec![DoctorCheck {
            name: "test".into(),
            category: Category::Core,
            status: CheckStatus::Ok,
            version: Some("1.0.0".into()),
            message: "ok".into(),
            fix_hint: None,
        }],
        health: OverallHealth::Healthy,
    };
    assert_eq!(report.health, OverallHealth::Healthy);
    assert_eq!(report.exit_code(), 0);

    // Missing required => Broken => exit 1
    let report = DoctorReport {
        checks: vec![DoctorCheck {
            name: "test".into(),
            category: Category::Core,
            status: CheckStatus::Missing,
            version: None,
            message: "missing".into(),
            fix_hint: Some("install it".into()),
        }],
        health: OverallHealth::Broken,
    };
    assert_eq!(report.health, OverallHealth::Broken);
    assert_eq!(report.exit_code(), 1);
}

// ---- version_at_least ----

#[test]
fn test_version_at_least_equal() {
    assert!(version_at_least("1.91.0", "1.91"));
    assert!(version_at_least("1.91.0", "1.91.0"));
}

#[test]
fn test_version_at_least_higher() {
    assert!(version_at_least("1.95.0", "1.91"));
    assert!(version_at_least("2.0.0", "1.91.0"));
}

#[test]
fn test_version_at_least_lower() {
    assert!(!version_at_least("1.79.0", "1.91"));
    assert!(!version_at_least("1.91.0", "1.95.0"));
}

#[test]
fn test_version_at_least_with_suffix() {
    // rustc 1.95.0-nightly should satisfy >= 1.91
    assert!(version_at_least("1.95.0-nightly", "1.91"));
}

// ---- install_hint_for ----

#[test]
fn test_install_hint_for_known() {
    assert!(install_hint_for("rustc", &["rustc"]).is_some());
    assert!(install_hint_for("git", &["git"]).is_some());
    assert!(install_hint_for("wmctrl", &["wmctrl"]).is_some());
    assert!(install_hint_for("xdotool", &["xdotool"]).is_some());
}

#[test]
fn test_install_hint_for_unknown() {
    assert!(install_hint_for("totally-made-up", &["totally-made-up"]).is_none());
}

// ---- glob_root ----

#[test]
fn test_glob_root_simple() {
    assert_eq!(glob_root("./**"), ".");
}

#[test]
fn test_glob_root_absolute() {
    assert_eq!(glob_root("/srv/foo/**/*.rs"), "/srv/foo");
}

#[test]
fn test_glob_root_pure_glob() {
    assert_eq!(glob_root("**"), "");
}

#[test]
fn test_glob_root_no_glob() {
    assert_eq!(glob_root("/etc/passwd"), "/etc/passwd");
}

// ---- config_checks ----

#[test]
fn test_config_checks_default_endpoint_local() {
    // A localhost endpoint with no api_key should NOT fail (treated as
    // local). Constructed explicitly because the default endpoint is now
    // the remote OpenRouter GLM-5.2 stack, which DOES require a key (that
    // path is covered by test_config_checks_remote_no_api_key_fails).
    let cfg = crate::config::Config {
        endpoint: "http://127.0.0.1:1234/v1".to_string(),
        api_key: None,
        ..crate::config::Config::default()
    };
    let checks = config_checks(&cfg);
    // endpoint URL should pass
    assert!(checks
        .iter()
        .any(|c| c.name == "endpoint URL" && c.status == CheckStatus::Ok));
    // api_key should not be Missing for local default
    let api_check = checks.iter().find(|c| c.name == "api_key").unwrap();
    assert_ne!(api_check.status, CheckStatus::Missing);
}

#[test]
fn test_config_checks_invalid_endpoint() {
    let cfg = crate::config::Config {
        endpoint: "not a url".to_string(),
        ..crate::config::Config::default()
    };
    let checks = config_checks(&cfg);
    let url_check = checks.iter().find(|c| c.name == "endpoint URL").unwrap();
    assert_eq!(url_check.status, CheckStatus::Missing);
    assert!(url_check.fix_hint.is_some());
}

#[test]
fn test_config_checks_remote_no_api_key_fails() {
    let cfg = crate::config::Config {
        endpoint: "https://api.example.com/v1".to_string(),
        api_key: None,
        ..crate::config::Config::default()
    };
    let checks = config_checks(&cfg);
    let api_check = checks.iter().find(|c| c.name == "api_key").unwrap();
    assert_eq!(api_check.status, CheckStatus::Missing);
    assert!(api_check.fix_hint.is_some());
}

#[cfg(feature = "log-analysis")]
#[test]
fn analyze_log_file_counts_errors_and_anomalies() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("selfware.log");
    // Lines shaped like real selfware logs ("<ts>  LEVEL target: msg") —
    // the Plain parser recognizes `[LEVEL]` or space-delimited ` LEVEL `.
    std::fs::write(
        &log,
        "2026-07-17T00:00:00Z  INFO selfware: boot\n\
             2026-07-17T00:00:01Z  ERROR selfware: failed to connect\n\
             2026-07-17T00:00:02Z  ERROR selfware: failed to connect\n\
             2026-07-17T00:00:03Z  WARN selfware: retry\n",
    )
    .unwrap();
    let (errors, _anomalies) = analyze_log_file(&log, 500).unwrap();
    assert_eq!(errors, 2);
}

#[tokio::test]
async fn test_run_doctor_completes() {
    // Smoke test: just ensure it doesn't panic or hang.
    let report = run_doctor(None).await;
    assert!(!report.checks.is_empty());
    // rustc must be present in a Rust build environment
    let rustc = report.checks.iter().find(|c| c.name == "rustc");
    assert!(rustc.is_some());
    assert_eq!(rustc.unwrap().status, CheckStatus::Ok);
    // MSRV check present
    assert!(report.checks.iter().any(|c| c.name == "rustc MSRV"));
}
