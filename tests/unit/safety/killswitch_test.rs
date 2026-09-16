use super::*;
use tempfile::tempdir;

#[test]
fn test_in_process_killswitch_trips_and_resets() {
    let _lock = KILLSWITCH_TEST_LOCK.lock();
    std::env::remove_var(KILLSWITCH_ENV_VAR);
    reset_in_process();
    assert!(!is_killswitch_active());

    trip_in_process("Emergency stop for testing");
    assert!(is_killswitch_active());

    match check_killswitch(None) {
        Err(KillswitchError::InProcess { reason }) => {
            assert!(reason.contains("Emergency stop for testing"));
        }
        other => panic!("Expected InProcess killswitch error, got: {:?}", other),
    }

    reset_in_process();
    assert!(!is_killswitch_active());
}

#[test]
fn test_file_killswitch_trips_and_removes() {
    let _lock = KILLSWITCH_TEST_LOCK.lock();
    std::env::remove_var(KILLSWITCH_ENV_VAR);
    reset_in_process();
    let tmp = tempdir().unwrap();
    let project_root = tmp.path();

    // Initially inactive for this root
    assert!(check_killswitch(Some(project_root)).is_ok());

    // Trip file killswitch
    let path = trip_file_killswitch(project_root, "RSI stability check").unwrap();
    assert!(path.exists());

    // Now active
    match check_killswitch(Some(project_root)) {
        Err(KillswitchError::File { path: p, reason }) => {
            assert_eq!(p, path);
            assert!(reason.contains("RSI stability check"));
        }
        other => panic!("Expected File killswitch error, got: {:?}", other),
    }

    // Remove file killswitch
    let removed = remove_file_killswitch(project_root).unwrap();
    assert!(removed);
    assert!(!path.exists());

    // Now inactive again
    assert!(check_killswitch(Some(project_root)).is_ok());
}

#[test]
fn test_env_killswitch_values() {
    let _lock = KILLSWITCH_TEST_LOCK.lock();
    std::env::remove_var(KILLSWITCH_ENV_VAR);
    reset_in_process();

    // 1 triggers
    std::env::set_var(KILLSWITCH_ENV_VAR, "1");
    assert!(check_killswitch(None).is_err());

    // "true" triggers
    std::env::set_var(KILLSWITCH_ENV_VAR, "true");
    assert!(check_killswitch(None).is_err());

    // "0" does NOT trigger
    std::env::set_var(KILLSWITCH_ENV_VAR, "0");
    // Unless file exists
    let tmp = tempdir().unwrap();
    assert!(check_killswitch(Some(tmp.path())).is_ok());

    // "false" does NOT trigger
    std::env::set_var(KILLSWITCH_ENV_VAR, "false");
    assert!(check_killswitch(Some(tmp.path())).is_ok());

    std::env::remove_var(KILLSWITCH_ENV_VAR);
}

#[test]
fn test_file_killswitch_fail_closed_on_directory_or_symlink() {
    let _lock = KILLSWITCH_TEST_LOCK.lock();
    std::env::remove_var(KILLSWITCH_ENV_VAR);
    reset_in_process();
    let tmp = tempdir().unwrap();
    let project_root = tmp.path();

    // 1. Directory at .selfware/KILLSWITCH must fail closed
    let killswitch_path = project_root.join(".selfware").join("KILLSWITCH");
    std::fs::create_dir_all(&killswitch_path).unwrap();

    let res = check_killswitch(Some(project_root));
    assert!(
        res.is_err(),
        "Must fail closed if KILLSWITCH is a directory"
    );
    std::fs::remove_dir(&killswitch_path).unwrap();

    // 2. Symlink at .selfware/KILLSWITCH must fail closed
    #[cfg(unix)]
    {
        let fake_target = tmp.path().join("fake_target");
        std::fs::write(&fake_target, "symlink-target").unwrap();
        std::os::unix::fs::symlink(&fake_target, &killswitch_path).unwrap();

        let res_sym = check_killswitch(Some(project_root));
        assert!(
            res_sym.is_err(),
            "Must fail closed if KILLSWITCH is a symlink"
        );
        std::fs::remove_file(&killswitch_path).unwrap();
    }
}

#[test]
fn test_safety_checker_blocks_tool_calls_when_killswitch_active() {
    let _lock = KILLSWITCH_TEST_LOCK.lock();
    std::env::remove_var(KILLSWITCH_ENV_VAR);
    reset_in_process();

    let config = crate::config::SafetyConfig::default();
    let checker = crate::safety::SafetyChecker::new(&config);
    let tool_call = crate::api::ToolCall {
        id: "call_1".to_string(),
        call_type: "function".to_string(),
        function: crate::api::ToolFunction {
            name: "file_read".to_string(),
            arguments: serde_json::json!({ "path": "src/lib.rs" }).to_string(),
        },
    };

    // Allowed when killswitch is inactive
    assert!(checker.check_tool_call(&tool_call).is_ok());

    // Trip killswitch
    trip_in_process("Halt all tools");

    // Rejected when killswitch is active
    let res = checker.check_tool_call(&tool_call);
    assert!(res.is_err());
    match res {
        Err(crate::errors::SelfwareError::Safety(
            crate::errors::SafetyError::KillswitchActive { reason },
        )) => {
            assert!(reason.contains("Halt all tools"));
        }
        other => panic!("Expected KillswitchActive, got: {:?}", other),
    }

    reset_in_process();
    assert!(checker.check_tool_call(&tool_call).is_ok());
}
