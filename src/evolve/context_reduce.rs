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

/// One `fn NAME { .. }` span, as byte offsets of the body braces.
pub(crate) struct FnBody {
    /// Offset of the `fn` keyword.
    pub(crate) start: usize,
    pub(crate) name: String,
    /// Offset of the opening `{`.
    pub(crate) open: usize,
    /// Offset just past the closing `}`.
    pub(crate) close: usize,
}

/// Locate function bodies in (comment-free) source. Brace matching skips string
/// literals so a `}` in a string can't close a body early. Intended to run on the
/// output of `reduce_source`, where comments are already gone. Shared with
/// `fn_dedup`, which runs it on raw source.
pub(crate) fn scan_fn_bodies(content: &str) -> Vec<FnBody> {
    let b = content.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 3 <= b.len() {
        // Byte-wise keyword check: slicing the str here can panic when the
        // window lands inside a multi-byte char (e.g. an em-dash in a literal).
        let is_fn_kw = b[i] == b'f'
            && b[i + 1] == b'n'
            && b[i + 2] == b' '
            && (i == 0 || matches!(b[i - 1], b' ' | b'\t' | b'\n' | b'(' | b'<'));
        if !is_fn_kw {
            i += 1;
            continue;
        }
        let after = i + 3;
        let name: String = content[after..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            i += 3;
            continue;
        }
        // Find the opening brace; a `;` first means a bodyless declaration.
        let mut j = after + name.len();
        let mut open = None;
        while j < b.len() {
            match b[j] {
                b'{' => {
                    open = Some(j);
                    break;
                }
                b';' => break,
                _ => j += 1,
            }
        }
        let Some(open) = open else {
            i = (j + 1).max(i + 3);
            continue;
        };
        // Match the closing brace, ignoring string literals.
        let (mut depth, mut k) = (0i32, open);
        let (mut in_str, mut escaped) = (false, false);
        let close = loop {
            if k >= b.len() {
                break b.len();
            }
            let c = b[k];
            if in_str {
                if escaped {
                    escaped = false;
                } else if c == b'\\' {
                    escaped = true;
                } else if c == b'"' {
                    in_str = false;
                }
            } else {
                match c {
                    b'"' => in_str = true,
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            break k + 1;
                        }
                    }
                    _ => {}
                }
            }
            k += 1;
        };
        out.push(FnBody {
            start: i,
            name,
            open,
            close,
        });
        i = close;
    }
    out
}

/// Normalize a body for exact-duplicate comparison: trim each line, drop blanks.
fn normalize_body(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Deduplicate identical function bodies across an assembled multi-file context.
/// The first occurrence of a body is canonical; later identical bodies are
/// replaced with a one-line reference stub. Returns the deduped files and the
/// number of bodies elided. Run on `reduce_source` output.
pub fn dedup_context(files: &[(String, String)]) -> (Vec<(String, String)>, usize) {
    use std::collections::HashMap;
    let mut canonical: HashMap<String, String> = HashMap::new(); // body -> "path::fn"
    let mut deduped = Vec::with_capacity(files.len());
    let mut elided = 0usize;

    for (path, content) in files {
        let bodies = scan_fn_bodies(content);
        // Only bodies of a meaningful size are worth deduping.
        let mut cursor = 0usize;
        let mut out = String::with_capacity(content.len());
        for f in &bodies {
            let body = &content[f.open..f.close];
            let key = normalize_body(body);
            if key.lines().count() < 3 {
                continue; // trivial body — not worth a stub
            }
            match canonical.get(&key) {
                Some(src) => {
                    let stub = format!("{{ /* deduped: identical to {src} */ }}");
                    // Eliding must actually shrink the context. Stubs tokenize
                    // worse than code (path-dense, ~3 chars/token vs ~4.8), so
                    // require the body to be at least twice the stub's size —
                    // marginal elisions are net-negative in tokens.
                    if stub.len() * 2 >= body.len() {
                        continue;
                    }
                    out.push_str(&content[cursor..f.open]);
                    out.push_str(&stub);
                    cursor = f.close;
                    elided += 1;
                }
                None => {
                    canonical.insert(key, format!("{path}::{}", f.name));
                }
            }
        }
        out.push_str(&content[cursor..]);
        deduped.push((path.clone(), out));
    }
    (deduped, elided)
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/context_reduce_test.rs"]
mod context_reduce_test;
