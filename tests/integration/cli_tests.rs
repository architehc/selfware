use assert_cmd::Command;

#[allow(deprecated)]
#[test]
fn test_cli_version() {
    let mut cmd = Command::cargo_bin("selfware").unwrap();
    cmd.arg("--version").assert().success();
}

#[allow(deprecated)]
#[test]
fn test_cli_help() {
    let mut cmd = Command::cargo_bin("selfware").unwrap();
    cmd.arg("--help").assert().success();
}

#[allow(deprecated)]
#[test]
fn test_cli_headless_requires_prompt() {
    let mut cmd = Command::cargo_bin("selfware").unwrap();
    cmd.arg("-p").assert().failure();
}

#[allow(deprecated)]
#[test]
fn test_cli_status_json() {
    let mut cmd = Command::cargo_bin("selfware").unwrap();
    cmd.arg("status")
        .arg("--output-format=json")
        .assert()
        .success();
}
