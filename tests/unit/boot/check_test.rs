use super::{check_model_file, plan_round_trip, BootCheck, BootCheckStatus, RoundTripPlan};

#[test]
fn model_check_skips_when_not_downloaded() {
    let dir = tempfile::tempdir().unwrap();
    let check = check_model_file(dir.path());
    assert_eq!(check.status, BootCheckStatus::Skip);
    assert!(check.detail.contains("not downloaded"));
    assert!(check.detail.contains("boot --chat"));
}

#[test]
fn model_check_fails_on_corrupt_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Qwen3-0.6B-Q8_0.gguf"), b"garbage").unwrap();
    let check = check_model_file(dir.path());
    assert_eq!(check.status, BootCheckStatus::Fail);
    assert!(check.detail.contains("mismatch"));
}

#[test]
fn round_trip_skips_without_model_or_server() {
    let model = BootCheck::skip("boot model file", "not downloaded");
    let server = BootCheck::pass("llama-server", "/usr/bin/llama-server");
    assert!(matches!(
        plan_round_trip(&model, &server),
        RoundTripPlan::Skip(_)
    ));

    let model_ok = BootCheck::pass("boot model file", "ok");
    let server_missing = BootCheck::fail("llama-server", "not found");
    assert!(matches!(
        plan_round_trip(&model_ok, &server_missing),
        RoundTripPlan::Skip(_)
    ));
}

#[test]
fn round_trip_runs_only_when_both_pass() {
    let model = BootCheck::pass("boot model file", "ok");
    let server = BootCheck::pass("llama-server", "/usr/bin/llama-server");
    match plan_round_trip(&model, &server) {
        RoundTripPlan::Run { server, .. } => {
            assert_eq!(server, std::path::PathBuf::from("/usr/bin/llama-server"))
        }
        RoundTripPlan::Skip(_) => panic!("expected Run"),
    }
}

#[test]
fn render_formats_status_lines() {
    let c = BootCheck::pass("llama-server", "/usr/bin/llama-server");
    assert_eq!(c.render(), "[PASS] llama-server — /usr/bin/llama-server");
    let c = BootCheck::skip("boot model file", "not downloaded");
    assert_eq!(c.render(), "[SKIP] boot model file — not downloaded");
}
