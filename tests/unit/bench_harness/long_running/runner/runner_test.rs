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
