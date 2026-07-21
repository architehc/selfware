use super::*;

#[test]
fn test_copy_dir_recursive_excludes_target() {
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();

    // Create some files and a target/ dir that should be skipped.
    std::fs::write(src.path().join("Cargo.toml"), "[package]\n").unwrap();
    std::fs::create_dir_all(src.path().join("src")).unwrap();
    std::fs::write(src.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::create_dir_all(src.path().join("target")).unwrap();
    std::fs::write(src.path().join("target/should_not_copy.txt"), "nope\n").unwrap();

    copy_dir_recursive(src.path(), dst.path(), Some("target")).unwrap();

    // Normal files should be copied.
    assert!(dst.path().join("Cargo.toml").exists());
    assert!(dst.path().join("src/main.rs").exists());

    // target/ should NOT be copied.
    assert!(!dst.path().join("target").exists());
}

#[test]
fn test_parse_test_counts() {
    let output = "test result: ok. 5198 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out";
    let (passed, total) = parse_test_counts(output);
    assert_eq!(passed, 5198);
    assert_eq!(total, 5200);
}

#[test]
fn test_parse_memory_string() {
    assert_eq!(parse_memory_string("256.5MiB / 4GiB"), 268_959_744);
    assert_eq!(parse_memory_string("1.5GiB / 4GiB"), 1_610_612_736);
    assert_eq!(parse_memory_string("512KiB / 4GiB"), 524_288);
}

#[test]
fn test_sandbox_config_default() {
    let cfg = SandboxConfig::default();
    assert_eq!(cfg.cpus, "2");
    assert!(!cfg.network);
    assert_eq!(cfg.memory, "4g");
    assert_eq!(cfg.image, "selfware:latest");
    assert_eq!(cfg.timeout, Duration::from_secs(3600));
}

#[test]
fn test_parse_test_counts_no_result_line() {
    let output = "running 10 tests\ntest foo ... ok\ntest bar ... ok";
    let (passed, total) = parse_test_counts(output);
    assert_eq!(passed, 0);
    assert_eq!(total, 0);
}

#[test]
fn test_parse_test_counts_all_passed() {
    let output = "test result: ok. 100 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out";
    let (passed, total) = parse_test_counts(output);
    assert_eq!(passed, 100);
    assert_eq!(total, 100);
}

#[test]
fn test_parse_test_counts_all_failed() {
    let output = "test result: FAILED. 0 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out";
    let (passed, total) = parse_test_counts(output);
    assert_eq!(passed, 0);
    assert_eq!(total, 5);
}

#[test]
fn test_parse_test_counts_with_ignored() {
    let output = "test result: ok. 80 passed; 0 failed; 20 ignored; 0 measured; 0 filtered out";
    let (passed, total) = parse_test_counts(output);
    assert_eq!(passed, 80);
    assert_eq!(total, 100); // 80 + 0 + 20
}

#[test]
fn test_parse_test_counts_empty_output() {
    let (passed, total) = parse_test_counts("");
    assert_eq!(passed, 0);
    assert_eq!(total, 0);
}

#[test]
fn test_parse_memory_string_unknown_unit() {
    // "100B / 4GiB" — no recognized unit prefix → 0
    assert_eq!(parse_memory_string("100B / 4GiB"), 0);
}

#[test]
fn test_parse_memory_string_empty() {
    assert_eq!(parse_memory_string(""), 0);
}

#[test]
fn test_parse_memory_string_no_slash() {
    // "256MiB" without the " / limit" part
    let result = parse_memory_string("256MiB");
    let expected = (256.0 * 1024.0 * 1024.0) as u64;
    assert_eq!(result, expected);
}

#[test]
fn test_parse_test_counts_multiple_result_lines() {
    // Multiple test result lines — should use the last one (rev iteration)
    let output = "\
test result: ok. 50 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 100 passed; 2 failed; 3 ignored; 0 measured; 0 filtered out";
    let (passed, total) = parse_test_counts(output);
    // Last line: 100 passed, 2 failed, 3 ignored
    assert_eq!(passed, 100);
    assert_eq!(total, 105);
}

#[test]
fn test_sandbox_error_display() {
    assert!(format!("{}", SandboxError::DockerFailed("no docker".into())).contains("no docker"));
    assert!(format!("{}", SandboxError::ExecFailed("cmd failed".into())).contains("cmd failed"));
    assert!(format!("{}", SandboxError::IoError("disk full".into())).contains("disk full"));
    assert!(format!("{}", SandboxError::Timeout).contains("timed out"));
}
