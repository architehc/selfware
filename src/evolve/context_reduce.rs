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

/// Remove `#[cfg(test)]`-gated items (inline test modules and test fns). They are
/// never part of what the code *does* at runtime, so they are pure filler in a
/// context window. Operates on comment-free source (run after `strip_comments`)
/// and matches braces to skip the whole gated block.
pub fn strip_cfg_test_blocks(src: &str) -> String {
    let lines: Vec<&str> = src.lines().collect();
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    let mut i = 0usize;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed == "#[cfg(test)]" || trimmed.starts_with("#[cfg(test)]") {
            // Skip the attribute and the item it gates. Brace counting ignores
            // string literals so a `}` inside a string can't close the block early.
            let mut j = i + 1;
            let mut depth: i32 = 0;
            let mut opened = false;
            let mut in_str = false;
            let mut escaped = false;
            while j < lines.len() {
                for c in lines[j].chars() {
                    if in_str {
                        if escaped {
                            escaped = false;
                        } else if c == '\\' {
                            escaped = true;
                        } else if c == '"' {
                            in_str = false;
                        }
                        continue;
                    }
                    match c {
                        '"' => in_str = true,
                        '{' => {
                            depth += 1;
                            opened = true;
                        }
                        '}' => depth -= 1,
                        _ => {}
                    }
                }
                if opened && depth <= 0 {
                    j += 1;
                    break;
                }
                // A brace-less gated item (e.g. `#[cfg(test)] use ...;`) ends at `;`.
                if !opened && lines[j].trim_end().ends_with(';') {
                    j += 1;
                    break;
                }
                j += 1;
            }
            i = j;
            continue;
        }
        out.push(lines[i]);
        i += 1;
    }
    out.join("\n")
}

/// Full context reduction: strip comments, then drop `#[cfg(test)]` blocks. The
/// result is behaviourally-equivalent runtime code with the human- and
/// test-only filler removed — the losslessly-droppable part of a context window.
pub fn reduce_source(src: &str) -> String {
    strip_cfg_test_blocks(&strip_comments(src))
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/context_reduce_test.rs"]
mod context_reduce_test;
