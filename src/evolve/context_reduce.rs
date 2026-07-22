//! Prompt-size reduction for smaller models.
//!
//! Smaller models have tighter context windows and pay more, relatively, for
//! filler. The cheapest large win is dropping comments and doc-comments — they
//! help humans, not the model's understanding of what the code *does*. This
//! strips `//`, `///`, `//!` and `/* … */` while preserving string literals and
//! code, then removes the blank lines left behind.
//!
//! It is a heuristic (raw strings containing `//` are rare and may over-strip),
//! so it is opt-in — used when a caller asks for compact context.

/// Remove comments from Rust source, preserving code and string literals.
pub fn strip_comments(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    let mut start = 0usize; // start of the current run of kept code

    while i < b.len() {
        match b[i] {
            b'"' => {
                // String literal: skip over it (kept as code, honoring escapes).
                i += 1;
                while i < b.len() {
                    match b[i] {
                        b'\\' => i += 2,
                        b'"' => {
                            i += 1;
                            break;
                        }
                        _ => i += 1,
                    }
                }
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                out.push_str(&src[start..i]); // emit code before the comment
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                start = i; // resume at the newline
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                out.push_str(&src[start..i]);
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(b.len());
                start = i;
            }
            _ => i += 1,
        }
    }
    out.push_str(&src[start..]);

    // Drop the blank lines comment removal leaves behind.
    let mut result = String::with_capacity(out.len());
    for line in out.lines() {
        if !line.trim().is_empty() {
            result.push_str(line.trim_end());
            result.push('\n');
        }
    }
    result
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/context_reduce_test.rs"]
mod context_reduce_test;
