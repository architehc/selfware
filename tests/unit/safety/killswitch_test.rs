use super::*;
use tempfile::tempdir;

#[test]
fn test_in_process_killswitch_trips_and_resets() {
    let _lock = KILLSWITCH_TEST_LOCK.lock();
    std::env::remove_var(KILLSWITCH_ENV_VAR);
    reset_in_process();
    let tmp = tempdir().unwrap();
    let empty_dir = tmp.path();
    assert!(check_killswitch_with_home(Some(empty_dir), Some(empty_dir)).is_ok());

    trip_in_process("Emergency stop for testing");
    assert!(is_killswitch_active());

    match check_killswitch_with_home(Some(empty_dir), Some(empty_dir)) {
        Err(KillswitchError::InProcess { reason }) => {
            assert!(reason.contains("Emergency stop for testing"));
        }
        other => panic!("Expected InProcess killswitch error, got: {:?}", other),
    }

    reset_in_process();
    assert!(check_killswitch_with_home(Some(empty_dir), Some(empty_dir)).is_ok());
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
    let outcome = trip_file_killswitch(project_root, "RSI stability check").unwrap();
    assert!(outcome.path().exists());
    assert!(matches!(outcome, TripFileOutcome::Written { .. }));

    // Now active
    match check_killswitch(Some(project_root)) {
        Err(KillswitchError::File { path: p, reason }) => {
            assert_eq!(p, outcome.path());
            assert!(reason.contains("RSI stability check"));
        }
        other => panic!("Expected File killswitch error, got: {:?}", other),
    }

    // Remove file killswitch
    let removed = remove_file_killswitch(project_root).unwrap();
    assert!(removed);
    assert!(!outcome.path().exists());

    // Now inactive again
    assert!(check_killswitch(Some(project_root)).is_ok());
}

#[test]
fn test_env_killswitch_parser() {
    assert!(parse_env_killswitch_value("1").is_some());
    assert!(parse_env_killswitch_value("true").is_some());
    assert!(parse_env_killswitch_value("yes").is_some());
    assert!(parse_env_killswitch_value("on").is_some());
    assert!(parse_env_killswitch_value("emergency").is_some());

    assert!(parse_env_killswitch_value("0").is_none());
    assert!(parse_env_killswitch_value("false").is_none());
    assert!(parse_env_killswitch_value("no").is_none());
    assert!(parse_env_killswitch_value("off").is_none());
    assert!(parse_env_killswitch_value("").is_none());
    assert!(parse_env_killswitch_value("   ").is_none());
}

#[test]
#[cfg(unix)]
fn test_fifo_killswitch_fails_closed_without_blocking() {
    let tmp = tempdir().unwrap();
    let project_root = tmp.path();
    let ks_dir = project_root.join(".selfware");
    std::fs::create_dir_all(&ks_dir).unwrap();
    let fifo_path = ks_dir.join("KILLSWITCH");

    // Create a FIFO
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo_path)
        .status();
    if let Ok(st) = status {
        if st.success() {
            // Must fail closed immediately without hanging on open()
            let res = check_killswitch(Some(project_root));
            assert!(res.is_err());
            let err = res.unwrap_err();
            assert!(err.to_string().contains("special file present"));
        }
    }
}

#[test]
fn test_file_killswitch_bounded_read() {
    let tmp = tempdir().unwrap();
    let project_root = tmp.path();
    let ks_dir = project_root.join(".selfware");
    std::fs::create_dir_all(&ks_dir).unwrap();
    let ks_file = ks_dir.join("KILLSWITCH");

    // Write a large message (8KB)
    let large_msg = "x".repeat(8192);
    std::fs::write(&ks_file, &large_msg).unwrap();

    let res = check_killswitch(Some(project_root));
    assert!(res.is_err());
    let err = res.unwrap_err();
    match err {
        KillswitchError::File { reason, .. } => {
            // Must be bounded to at most 4096 chars
            assert!(reason.len() <= 4096);
        }
        other => panic!("Expected File killswitch error, got: {:?}", other),
    }
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

#[test]
#[cfg(unix)]
fn test_trip_file_killswitch_fifo_preservation_and_bounded_completion() {
    let _lock = KILLSWITCH_TEST_LOCK.lock();
    let tmp = tempdir().unwrap();
    let project_root = tmp.path();
    let ks_dir = project_root.join(".selfware");
    std::fs::create_dir_all(&ks_dir).unwrap();
    let fifo_path = ks_dir.join("KILLSWITCH");

    // Create a FIFO
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo_path)
        .status();
    if let Ok(st) = status {
        if st.success() {
            // Tripping over an existing FIFO must return boundedly without blocking
            let start = std::time::Instant::now();
            let trip_res = trip_file_killswitch(project_root, "Emergency halt on FIFO");
            assert!(start.elapsed() < std::time::Duration::from_millis(500));
            assert!(trip_res.is_ok());
            assert!(matches!(
                trip_res.as_ref().unwrap(),
                TripFileOutcome::PreservedExisting { .. }
            ));

            // The FIFO must be preserved as a special file (not overwritten by a regular file)
            let meta = fifo_path.symlink_metadata().unwrap();
            assert!(
                !meta.file_type().is_file(),
                "FIFO must be preserved as special file"
            );

            // check_killswitch must detect it as active
            assert!(check_killswitch(Some(project_root)).is_err());

            // remove_file_killswitch must safely unlink the FIFO without following
            let removed = remove_file_killswitch(project_root).unwrap();
            assert!(removed);
            assert!(!fifo_path.exists());
        }
    }
}

#[test]
#[cfg(unix)]
fn test_trip_file_killswitch_symlink_preservation() {
    let _lock = KILLSWITCH_TEST_LOCK.lock();
    let tmp = tempdir().unwrap();
    let project_root = tmp.path();
    let ks_dir = project_root.join(".selfware");
    std::fs::create_dir_all(&ks_dir).unwrap();

    let target_file = tmp.path().join("external_secret.txt");
    std::fs::write(&target_file, "original secret content").unwrap();

    let symlink_path = ks_dir.join("KILLSWITCH");
    std::os::unix::fs::symlink(&target_file, &symlink_path).unwrap();

    // Tripping over an existing symlink must NOT overwrite the symlink target
    let trip_res = trip_file_killswitch(project_root, "Emergency halt on symlink");
    assert!(trip_res.is_ok());
    assert!(matches!(
        trip_res.as_ref().unwrap(),
        TripFileOutcome::PreservedExisting { .. }
    ));

    let target_content = std::fs::read_to_string(&target_file).unwrap();
    assert_eq!(
        target_content, "original secret content",
        "Target of symlink must NOT be overwritten when tripping killswitch"
    );

    // Symlink itself must still exist and be active
    assert!(symlink_path
        .symlink_metadata()
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(check_killswitch(Some(project_root)).is_err());

    // remove_file_killswitch unlinks the symlink without touching the target
    let removed = remove_file_killswitch(project_root).unwrap();
    assert!(removed);
    assert!(!symlink_path.exists());
    assert!(target_file.exists());
    assert_eq!(
        std::fs::read_to_string(&target_file).unwrap(),
        "original secret content"
    );
}

#[test]
#[cfg(unix)]
fn test_global_killswitch_symlinked_home_detection() {
    let _lock = KILLSWITCH_TEST_LOCK.lock();
    let tmp = tempdir().unwrap();

    // Create real home directory with .selfware/KILLSWITCH
    let real_home = tmp.path().join("real_home");
    let ks_dir = real_home.join(".selfware");
    std::fs::create_dir_all(&ks_dir).unwrap();
    let ks_file = ks_dir.join("KILLSWITCH");
    std::fs::write(&ks_file, "Global operator emergency stop").unwrap();

    // Create a symlink pointing to the home directory
    let symlinked_home = tmp.path().join("symlinked_home");
    std::os::unix::fs::symlink(&real_home, &symlinked_home).unwrap();

    // check_killswitch_with_home must follow symlinks through home and detect the global killswitch
    let res = check_killswitch_with_home(None, Some(&symlinked_home));
    assert!(
        res.is_err(),
        "Global killswitch in symlinked home must be detected"
    );
    let err = res.unwrap_err();
    assert!(err.to_string().contains("Global operator emergency stop"));
}

#[test]
#[cfg(unix)]
fn test_global_killswitch_inspection_error_fails_closed() {
    let _lock = KILLSWITCH_TEST_LOCK.lock();
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempdir().unwrap();
    let fake_home = tmp.path().join("inaccessible_home");
    let selfware_dir = fake_home.join(".selfware");
    std::fs::create_dir_all(&selfware_dir).unwrap();
    let ks_file = selfware_dir.join("KILLSWITCH");
    std::fs::write(&ks_file, "failsafe").unwrap();

    // Inaccessible permissions (000) simulate EACCES / permission denied inspection failure
    let mut perms = std::fs::metadata(&selfware_dir).unwrap().permissions();
    perms.set_mode(0o000);
    let _ = std::fs::set_permissions(&selfware_dir, perms);

    let res = check_killswitch_with_home(None, Some(&fake_home));
    assert!(
        res.is_err(),
        "Inspection error on home killswitch must fail closed"
    );

    // Restore permissions so tempdir cleanup succeeds
    let mut restore_perms = std::fs::metadata(&selfware_dir).unwrap().permissions();
    restore_perms.set_mode(0o700);
    let _ = std::fs::set_permissions(&selfware_dir, restore_perms);
}

#[test]
fn test_safety_checker_ordinary_denied_path_does_not_trip_killswitch() {
    let _lock = KILLSWITCH_TEST_LOCK.lock();

    let config = crate::config::SafetyConfig::default();
    let checker = crate::safety::SafetyChecker::new(&config);
    let tool_call = crate::api::ToolCall {
        id: "call_blocked".to_string(),
        call_type: "function".to_string(),
        function: crate::api::ToolFunction {
            name: "file_write".to_string(),
            arguments: serde_json::json!({
                "path": ".selfware/KILLSWITCH",
                "content": "illegal attempt",
            })
            .to_string(),
        },
    };

    let res = checker.check_tool_call(&tool_call);
    assert!(res.is_err());
    let err = res.unwrap_err();
    // Must be PathDeniedPattern error, NOT KillswitchActive
    match err {
        crate::errors::SelfwareError::Safety(crate::errors::SafetyError::KillswitchActive {
            ..
        }) => panic!(
            "Ordinary path denial to .selfware/KILLSWITCH must not produce KillswitchActive error!"
        ),
        crate::errors::SelfwareError::Safety(crate::errors::SafetyError::PathDeniedPattern {
            ..
        }) => {
            // Expected!
        }
        other => panic!("Expected PathDeniedPattern error, got: {:?}", other),
    }

    // Process killswitch must still be inactive
    assert!(!is_killswitch_active());
}

#[test]
fn test_remove_file_killswitch_nonempty_directory_survives_and_errors() {
    let _lock = KILLSWITCH_TEST_LOCK.lock();
    let tmp = tempdir().unwrap();
    let project_root = tmp.path();
    let ks_dir = project_root.join(".selfware").join(KILLSWITCH_FILE_NAME);
    std::fs::create_dir_all(&ks_dir).unwrap();

    // Place an important file inside the killswitch directory
    let inner_file = ks_dir.join("user_data.txt");
    std::fs::write(&inner_file, "critical operator notes").unwrap();

    // Attempting to remove the killswitch directory must refuse and error
    let res = remove_file_killswitch(project_root);
    assert!(
        res.is_err(),
        "remove_file_killswitch must fail on nonempty directory"
    );
    let err = res.unwrap_err();
    assert!(err.to_string().contains("not empty"));
    assert!(err.to_string().contains("refusing to recursively delete"));

    // Contents must survive untouched
    assert!(inner_file.exists());
    assert_eq!(
        std::fs::read_to_string(&inner_file).unwrap(),
        "critical operator notes"
    );

    // After removing the inner file (making directory empty), remove_file_killswitch succeeds
    std::fs::remove_file(&inner_file).unwrap();
    let res_empty = remove_file_killswitch(project_root);
    assert!(res_empty.is_ok());
    assert!(res_empty.unwrap());
    assert!(!ks_dir.exists());
}
