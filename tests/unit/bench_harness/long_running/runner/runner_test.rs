use super::*;

#[test]
fn test_count_steps() {
    let output = b"Step 1 Executing...\nStep 2 Executing...\n";
    assert_eq!(count_steps(output), 2);

    // Current piped/plain and TTY step_start formats:
    let modern_output = b"[Planning \xc2\xb7 0s elapsed]\n\
                          [Step 1] \xe2\x86\x92 file_read Cargo.toml...\n\
                          [Step 2 \xc2\xb7 5s elapsed]\n\
                          [Step 2] \xe2\x86\x92 cargo_test...\n\
                          \xf0\x9f\x93\x9d Step 3 \xe2\x86\x92 final answer...\n";
    assert_eq!(count_steps(modern_output), 3);
}

#[test]
fn test_extract_outcome() {
    let output = b"Some log\nOutcome: task_completed\nMore log\n";
    assert_eq!(extract_outcome(output), "task_completed");

    // Current `── Run summary ──` lowercase format:
    let summary_output = b"\xe2\x94\x80\xe2\x94\x80 Run summary \xe2\x94\x80\xe2\x94\x80\n\
                           outcome: completed\n\
                           iterations: 4/80 \xc2\xb7 turns: 5\n";
    assert_eq!(extract_outcome(summary_output), "completed");
}

#[test]
fn rust_test_counts_sum_every_test_binary() {
    let out = "running 10 tests\ntest a ... ok\n\
               test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n\
               running 5 tests\n\
               test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n\
               left: 7\n\
               test result: FAILED. 40 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n";
    assert_eq!(parse_test_counts(&ProjectType::Rust, out), (55, 1));
    assert_eq!(parse_test_counts(&ProjectType::Template, out), (55, 1));
}

#[test]
fn pytest_counts_read_the_closing_tally() {
    let out = "tests/test_a.py::test_one PASSED\n\
               tests/test_a.py::test_two FAILED\n\
               ========== 1 failed, 12 passed, 1 skipped in 0.40s ==========\n";
    assert_eq!(parse_test_counts(&ProjectType::Python, out), (12, 1));
    let all_pass = "=================== 7 passed in 0.10s ===================\n";
    assert_eq!(parse_test_counts(&ProjectType::Python, all_pass), (7, 0));
}

#[test]
fn pytest_counts_fall_back_to_per_test_lines() {
    let out = "tests/test_a.py::test_one PASSED\ntests/test_a.py::test_two PASSED\n\
               tests/test_a.py::test_three FAILED\n";
    assert_eq!(parse_test_counts(&ProjectType::Python, out), (2, 1));
}

#[test]
fn go_test_counts_per_test_lines() {
    let out = "--- PASS: TestA (0.00s)\n--- FAIL: TestB (0.00s)\n--- PASS: TestC (0.00s)\n";
    assert_eq!(parse_test_counts(&ProjectType::Go, out), (2, 1));
}

#[test]
fn check_output_succeeded_requires_zero_exit_and_accepts_stderr_finished() {
    // Cargo writes "Finished `dev` profile ..." to stderr, not stdout.
    assert!(check_output_succeeded(
        &ProjectType::Rust,
        true,
        "",
        "    Checking foo v0.1.0\n    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.12s\n"
    ));
    assert!(check_output_succeeded(
        &ProjectType::Template,
        true,
        "",
        "    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.12s\n"
    ));
    // Non-zero exit must never be reported as compiling even when stderr lacks "error".
    assert!(!check_output_succeeded(
        &ProjectType::Rust,
        false,
        "",
        "Killed by signal 9\n"
    ));
    assert!(!check_output_succeeded(&ProjectType::Go, false, "", ""));
}
