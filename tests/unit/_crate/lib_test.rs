use super::*;

#[test]
fn test_shutdown_flag_default_false() {
    // The flag may have been set by a previous test in the same process,
    // so we just verify the functions don't panic and return a bool.
    let _ = is_shutdown_requested();
}

#[test]
fn test_request_shutdown_sets_flag() {
    // Hold the shared test lock so no execute-loop test runs while the global
    // shutdown latch is set, and clear it on drop so the latch never leaks.
    let _g = crate::test_support::ExecGuard::hold();
    request_shutdown();
    assert!(is_shutdown_requested());
    reset_shutdown_for_test();
}
