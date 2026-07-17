//! CLI tests that spawn the real `selfware` binary.

use predicates::boolean::PredicateBooleanExt;

#[test]
fn workflow_codegen_prints_rust_stub() {
    let mut cmd = assert_cmd::Command::cargo_bin("selfware").unwrap();
    cmd.args(["workflow", "codegen", "workflows/bug_investigation.swl"])
        .assert()
        .success()
        .stdout(predicates::str::is_empty().not());
}
