use assert_cmd::Command;

#[allow(deprecated)]
#[test]
fn boot_check_reports_malformed_config_and_continues_diagnostics() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("broken.toml");
    std::fs::write(&config, "model = [\n").unwrap();
    let mut cmd = Command::cargo_bin("selfware").unwrap();
    let output = cmd
        .current_dir(dir.path())
        .arg("--config")
        .arg(&config)
        .args(["boot", "--check"])
        .timeout(std::time::Duration::from_secs(120))
        .assert()
        .failure()
        .get_output()
        .clone();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[FAIL] current config"), "{stdout}");
    assert!(stdout.contains("boot model file"), "{stdout}");
    assert!(stdout.contains("canned round-trip"), "{stdout}");
    assert_eq!(std::fs::read_to_string(config).unwrap(), "model = [\n");
}

#[allow(deprecated)]
#[test]
fn wizard_remains_reachable_with_malformed_config() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("broken.toml");
    std::fs::write(&config, "model = [\n").unwrap();
    let mut cmd = Command::cargo_bin("selfware").unwrap();
    let output = cmd
        .current_dir(dir.path())
        .arg("--config")
        .arg(&config)
        .arg("boot")
        .assert()
        .failure()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("needs a terminal"), "{stderr}");
    assert!(!stderr.contains("Failed to parse config"), "{stderr}");
    assert_eq!(std::fs::read_to_string(config).unwrap(), "model = [\n");
}

#[allow(deprecated)]
#[test]
fn boot_check_applies_global_profile_override_before_doctor() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(
        &config,
        "endpoint = \"http://127.0.0.1:9/v1\"\nmodel = \"fixture\"\n",
    )
    .unwrap();
    let mut cmd = Command::cargo_bin("selfware").unwrap();
    let output = cmd
        .current_dir(dir.path())
        .arg("--config")
        .arg(&config)
        .args(["--profile", "missing-review-profile", "boot", "--check"])
        .timeout(std::time::Duration::from_secs(120))
        .assert()
        .failure()
        .get_output()
        .clone();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[FAIL] current config"), "{stdout}");
    assert!(
        stdout.contains("Unknown --profile 'missing-review-profile'"),
        "{stdout}"
    );
}
