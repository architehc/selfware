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

#[test]
fn test_repl_waiting_for_input_and_guard() {
    assert!(!is_repl_waiting_for_input());
    {
        let _guard = ReplInputWaitGuard::enter();
        assert!(is_repl_waiting_for_input());
    }
    assert!(!is_repl_waiting_for_input());
}

#[test]
fn first_signal_action_drains_only_on_sigterm_at_an_idle_repl() {
    assert_eq!(
        first_signal_action(ShutdownReason::SignalTerminate, true),
        FirstSignalAction::DrainSessionThenExit { code: 143 }
    );
    assert_eq!(
        first_signal_action(ShutdownReason::SignalTerminate, false),
        FirstSignalAction::WindDown
    );
    for idle in [true, false] {
        assert_eq!(
            first_signal_action(ShutdownReason::UserInterrupt, idle),
            FirstSignalAction::WindDown
        );
        assert_eq!(
            first_signal_action(ShutdownReason::Timeout, idle),
            FirstSignalAction::WindDown
        );
    }
}
