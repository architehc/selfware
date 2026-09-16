use super::*;
use parking_lot::Mutex;
use tempfile::tempdir;

static TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn test_in_process_killswitch_trips_and_resets() {
    let _lock = TEST_LOCK.lock();
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
    let _lock = TEST_LOCK.lock();
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
    let _lock = TEST_LOCK.lock();
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
