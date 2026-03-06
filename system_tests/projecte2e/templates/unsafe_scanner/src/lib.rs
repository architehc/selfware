//! A pattern scanner that uses manual memory management for "performance".
//! This module intentionally uses unsafe code with multiple bugs.
//!
//! The scanner maintains an internal buffer for pattern matching and
//! supports adding patterns and scanning text input.

use std::alloc::{alloc, dealloc, realloc, Layout};
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Result of a pattern scan
#[derive(Debug, Clone, PartialEq)]
pub struct ScanResult {
    pub pattern: String,
    pub offset: usize,
}

/// A thread-safe match counter
static TOTAL_MATCHES: AtomicUsize = AtomicUsize::new(0);

/// Reset the global match counter
pub fn reset_match_counter() {
    TOTAL_MATCHES.store(0, Ordering::Relaxed);
}

/// Get the total match count
pub fn total_matches() -> usize {
    TOTAL_MATCHES.load(Ordering::Relaxed)
}

/// Internal buffer for the scanner.
/// Uses raw pointer for manual memory management.
pub struct RawBuffer {
    ptr: *mut u8,
    len: usize,
    capacity: usize,
}

impl RawBuffer {
    /// Create a new buffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        // BUG 1: No check for zero capacity — Layout::from_size_align panics on size=0
        let layout = Layout::from_size_align(capacity, 1).unwrap();
        let ptr = unsafe { alloc(layout) };
        if ptr.is_null() {
            panic!("allocation failed");
        }
        RawBuffer {
            ptr,
            len: 0,
            capacity,
        }
    }

    /// Append bytes to the buffer, growing if needed.
    pub fn push(&mut self, data: &[u8]) {
        let new_len = self.len + data.len();
        if new_len > self.capacity {
            let new_cap = new_len.next_power_of_two();
            // BUG 2: old_layout uses `self.len` instead of `self.capacity`
            // This creates a Layout mismatch — the dealloc size won't match alloc size
            let old_layout = Layout::from_size_align(self.len, 1).unwrap();
            let new_ptr = unsafe { realloc(self.ptr, old_layout, new_cap) };
            if new_ptr.is_null() {
                panic!("reallocation failed");
            }
            // BUG 3: If realloc returns a different pointer, old ptr is freed by realloc.
            // But we also don't handle the case where realloc fails gracefully
            // (the panic above is fine, but the Layout bug in BUG 2 causes UB first).
            self.ptr = new_ptr;
            self.capacity = new_cap;
        }
        unsafe {
            ptr::copy_nonoverlapping(data.as_ptr(), self.ptr.add(self.len), data.len());
        }
        self.len = new_len;
    }

    /// Get the buffer contents as a byte slice.
    pub fn as_slice(&self) -> &[u8] {
        if self.ptr.is_null() || self.len == 0 {
            return &[];
        }
        // BUG 4: Uses self.capacity instead of self.len — reads uninitialized memory
        unsafe { std::slice::from_raw_parts(self.ptr, self.capacity) }
    }

    /// Clear the buffer without deallocating.
    pub fn clear(&mut self) {
        self.len = 0;
        // Note: does NOT zero the memory — old data remains readable
    }
}

impl Drop for RawBuffer {
    fn drop(&mut self) {
        if !self.ptr.is_null() && self.capacity > 0 {
            let layout = Layout::from_size_align(self.capacity, 1).unwrap();
            unsafe {
                dealloc(self.ptr, layout);
            }
        }
        // BUG 5: ptr is not set to null after dealloc — double-free if Drop runs twice
        // (e.g., via std::mem::ManuallyDrop misuse or panic during drop)
    }
}

// BUG 6: RawBuffer contains a raw pointer but claims Send + Sync.
// This is unsound if the buffer is shared across threads without synchronization.
unsafe impl Send for RawBuffer {}
unsafe impl Sync for RawBuffer {}

/// The main scanner struct.
pub struct Scanner {
    patterns: Vec<String>,
    buffer: RawBuffer,
    match_count: usize,
}

impl Scanner {
    /// Create a new scanner with the given initial buffer capacity.
    pub fn new(capacity: usize) -> Self {
        Scanner {
            patterns: Vec::new(),
            buffer: RawBuffer::new(capacity),
            match_count: 0,
        }
    }

    /// Add a pattern to search for.
    pub fn add_pattern(&mut self, pattern: &str) {
        // BUG 7: No dedup check — same pattern can be added multiple times,
        // causing duplicate results in scan output
        self.patterns.push(pattern.to_string());
    }

    /// Scan the input text for all registered patterns.
    /// Returns a list of matches with their positions.
    pub fn scan(&mut self, input: &str) -> Vec<ScanResult> {
        self.buffer.clear();
        self.buffer.push(input.as_bytes());

        let mut results = Vec::new();

        // BUG 8: Uses buffer.as_slice() which returns capacity-sized slice (BUG 4),
        // so we search through uninitialized memory beyond the actual input
        let haystack = self.buffer.as_slice();

        for pattern in &self.patterns {
            let pat_bytes = pattern.as_bytes();
            if pat_bytes.is_empty() {
                continue;
            }
            // BUG 9: Off-by-one in the search bound — should be `haystack.len() - pat_bytes.len() + 1`
            // but we use `haystack.len() - pat_bytes.len()` which misses matches at the very end
            if haystack.len() < pat_bytes.len() {
                continue;
            }
            let search_end = haystack.len() - pat_bytes.len();
            for i in 0..search_end {
                if &haystack[i..i + pat_bytes.len()] == pat_bytes {
                    results.push(ScanResult {
                        pattern: pattern.clone(),
                        offset: i,
                    });
                    self.match_count += 1;
                    TOTAL_MATCHES.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        results
    }

    /// Get the number of matches found so far.
    pub fn match_count(&self) -> usize {
        self.match_count
    }

    /// Get a reference to the registered patterns.
    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }

    /// Export the internal buffer contents as a String.
    /// Returns None if the buffer contains invalid UTF-8.
    pub fn export_buffer(&self) -> Option<String> {
        let slice = self.buffer.as_slice();
        // BUG 10: This uses the buggy as_slice (BUG 4) which includes uninitialized memory,
        // so the string will contain garbage bytes beyond the actual content
        String::from_utf8(slice.to_vec()).ok()
    }

    /// Resize the internal buffer to a new capacity.
    pub fn resize_buffer(&mut self, new_capacity: usize) {
        // BUG 11: Drops old buffer and creates new one, but doesn't preserve existing data.
        // Any content in the buffer is silently lost.
        self.buffer = RawBuffer::new(new_capacity);
    }
}

/// Create a scanner, scan input, and return results.
/// Convenience function for one-shot scanning.
pub fn quick_scan(patterns: &[&str], input: &str) -> Vec<ScanResult> {
    // BUG 12: Creates scanner with capacity 0, which hits BUG 1 (panics on Layout)
    let mut scanner = Scanner::new(0);
    for p in patterns {
        scanner.add_pattern(p);
    }
    scanner.scan(input)
}
