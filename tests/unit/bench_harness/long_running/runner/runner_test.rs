use super::*;

#[test]
fn test_count_steps() {
    let output = b"Step 1 Executing...\nStep 2 Executing...\n";
    assert_eq!(count_steps(output), 2);
}

#[test]
fn test_extract_outcome() {
    let output = b"Some log\nOutcome: task_completed\nMore log\n";
    assert_eq!(extract_outcome(output), "task_completed");
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
