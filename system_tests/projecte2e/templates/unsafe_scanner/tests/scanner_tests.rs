use unsafe_scanner::{quick_scan, reset_match_counter, total_matches, Scanner, ScanResult};

// ── Basic Functionality ──────────────────────────────────────────────

#[test]
fn test_basic_scan_finds_pattern() {
    let mut scanner = Scanner::new(128);
    scanner.add_pattern("hello");
    let results = scanner.scan("say hello world");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].pattern, "hello");
    assert_eq!(results[0].offset, 4);
}

#[test]
fn test_scan_multiple_patterns() {
    let mut scanner = Scanner::new(128);
    scanner.add_pattern("cat");
    scanner.add_pattern("dog");
    let results = scanner.scan("the cat and the dog");
    assert_eq!(results.len(), 2);
    let patterns: Vec<&str> = results.iter().map(|r| r.pattern.as_str()).collect();
    assert!(patterns.contains(&"cat"));
    assert!(patterns.contains(&"dog"));
}

#[test]
fn test_scan_no_match() {
    let mut scanner = Scanner::new(64);
    scanner.add_pattern("xyz");
    let results = scanner.scan("hello world");
    assert!(results.is_empty());
}

// ── BUG 1: Zero capacity ────────────────────────────────────────────

#[test]
fn test_zero_capacity_does_not_panic() {
    // quick_scan uses capacity 0 — this should NOT panic
    let results = quick_scan(&["hello"], "hello world");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].pattern, "hello");
}

#[test]
fn test_scanner_zero_capacity() {
    let mut scanner = Scanner::new(0);
    scanner.add_pattern("test");
    let results = scanner.scan("this is a test");
    assert_eq!(results.len(), 1);
}

// ── BUG 4 + 8 + 10: as_slice reads uninitialized memory ────────────

#[test]
fn test_as_slice_returns_only_written_data() {
    let mut scanner = Scanner::new(1024);
    scanner.add_pattern("x");
    let input = "short";
    scanner.scan(input);
    // export_buffer should return exactly the input, not garbage
    let exported = scanner.export_buffer().expect("buffer should be valid UTF-8");
    assert_eq!(exported, input);
}

#[test]
fn test_buffer_does_not_leak_old_data() {
    let mut scanner = Scanner::new(64);
    scanner.add_pattern("secret");

    // First scan with sensitive data
    scanner.scan("my secret password");
    // Second scan with shorter data
    scanner.scan("hello");

    let exported = scanner.export_buffer().expect("valid utf8");
    // Should only contain "hello", not leftover "secret password"
    assert_eq!(exported, "hello");
    assert!(!exported.contains("secret"));
}

// ── BUG 7: Duplicate patterns ──────────────────────────────────────

#[test]
fn test_duplicate_patterns_not_duplicated() {
    let mut scanner = Scanner::new(64);
    scanner.add_pattern("test");
    scanner.add_pattern("test");
    scanner.add_pattern("test");
    let results = scanner.scan("this is a test");
    // Should find "test" once, not three times
    assert_eq!(results.len(), 1);
}

// ── BUG 9: Off-by-one at end of input ──────────────────────────────

#[test]
fn test_pattern_at_end_of_input() {
    let mut scanner = Scanner::new(64);
    scanner.add_pattern("end");
    let results = scanner.scan("the end");
    assert_eq!(results.len(), 1, "pattern at very end of input must be found");
    assert_eq!(results[0].offset, 4);
}

#[test]
fn test_pattern_is_entire_input() {
    let mut scanner = Scanner::new(64);
    scanner.add_pattern("exact");
    let results = scanner.scan("exact");
    assert_eq!(results.len(), 1, "pattern matching entire input must be found");
    assert_eq!(results[0].offset, 0);
}

// ── BUG 2 + 3: Realloc with wrong layout ───────────────────────────

#[test]
fn test_scan_with_buffer_growth() {
    let mut scanner = Scanner::new(4); // tiny buffer forces realloc
    scanner.add_pattern("pattern");
    let long_input = "a".repeat(100) + "pattern" + &"b".repeat(100);
    let results = scanner.scan(&long_input);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].offset, 100);
}

#[test]
fn test_multiple_reallocs() {
    let mut scanner = Scanner::new(2); // very tiny
    scanner.add_pattern("x");
    // Force many reallocs by scanning progressively larger inputs
    for size in [10, 50, 200, 1000] {
        let input = "x".repeat(size);
        let results = scanner.scan(&input);
        assert_eq!(results.len(), size, "all x's should match in input of size {size}");
    }
}

// ── BUG 11: resize_buffer loses data ────────────────────────────────

#[test]
fn test_resize_preserves_patterns() {
    let mut scanner = Scanner::new(64);
    scanner.add_pattern("hello");
    scanner.scan("hello world");
    assert_eq!(scanner.match_count(), 1);

    scanner.resize_buffer(128);
    // After resize, scanning should still work with existing patterns
    let results = scanner.scan("hello again");
    assert_eq!(results.len(), 1);
}

// ── BUG 5: Double-free safety ───────────────────────────────────────

#[test]
fn test_drop_is_safe() {
    // Create and drop many scanners to stress-test memory management
    for _ in 0..100 {
        let mut scanner = Scanner::new(64);
        scanner.add_pattern("test");
        scanner.scan("test data");
        drop(scanner);
    }
    // If we get here without SIGSEGV/double-free, we're good
}

// ── BUG 6: Send + Sync safety ──────────────────────────────────────

#[test]
fn test_scanner_send_across_threads() {
    let mut scanner = Scanner::new(64);
    scanner.add_pattern("thread");

    let handle = std::thread::spawn(move || {
        let results = scanner.scan("thread safety test");
        assert_eq!(results.len(), 1);
        scanner.match_count()
    });
    let count = handle.join().expect("thread should not panic");
    assert_eq!(count, 1);
}

// ── Match counter ───────────────────────────────────────────────────

#[test]
fn test_global_match_counter() {
    reset_match_counter();
    let mut scanner = Scanner::new(64);
    scanner.add_pattern("a");
    scanner.scan("aaa");
    assert_eq!(scanner.match_count(), 3);
    assert_eq!(total_matches(), 3);
}

// ── Empty pattern handling ──────────────────────────────────────────

#[test]
fn test_empty_pattern_ignored() {
    let mut scanner = Scanner::new(64);
    scanner.add_pattern("");
    scanner.add_pattern("real");
    let results = scanner.scan("real data");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].pattern, "real");
}

// ── Large input ─────────────────────────────────────────────────────

#[test]
fn test_large_input_scan() {
    let mut scanner = Scanner::new(16);
    scanner.add_pattern("needle");
    let mut haystack = "hay ".repeat(10_000);
    haystack.push_str("needle");
    haystack.push_str(&"hay ".repeat(1000));
    let results = scanner.scan(&haystack);
    assert_eq!(results.len(), 1);
}

// ── Pattern at various positions ────────────────────────────────────

#[test]
fn test_pattern_at_start() {
    let mut scanner = Scanner::new(64);
    scanner.add_pattern("start");
    let results = scanner.scan("start of string");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].offset, 0);
}

#[test]
fn test_multiple_occurrences() {
    let mut scanner = Scanner::new(64);
    scanner.add_pattern("ab");
    let results = scanner.scan("ab cd ab ef ab");
    assert_eq!(results.len(), 3);
}
