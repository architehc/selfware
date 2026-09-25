//! Line-number prefixes for `file_read` output.
//!
//! `file_read` (by default for a `line_range` read, on request with
//! `line_numbers: true` for a whole file) prefixes every line with its 1-based ABSOLUTE line
//! number, right-aligned to the widest number shown, then a tab (` 42\tcode`,
//! `cat -n` style; see [`number_lines`] for why the pad is tight), so the
//! model never has to count lines to cite `path:line` (live validation: most
//! wrong line citations came from the model counting lines itself). The
//! numbers are metadata, not file content, which creates two obligations:
//!
//! - every consumer of a `file_read` result's `content` that needs the file's
//!   text (hashes, the context map, the work ledger) goes through
//!   [`raw_file_read_content`], which removes the prefixes the tool added;
//! - edit tools must not write the prefixes back into files when a model
//!   copies them into `old_str` / `new_str` / a diff / `file_write` content
//!   ([`strip_numbered`], [`strip_present_prefixes`], [`is_numbered_text`]).
//!
//! Numbering is lossless: each physical line keeps its own terminator
//! (`\n` or `\r\n`), so stripping a numbered text returns the exact input.

use serde_json::Value;

/// Result key telling consumers whether `content` carries line-number
/// prefixes.
pub const LINE_NUMBERS_KEY: &str = "line_numbers";

/// Prefix every physical line of `content` with its line number, starting at
/// `first_line`, right-aligned to the width of the LARGEST number in this
/// output (`  7\t…` … `120\t…`). Line terminators are preserved byte for
/// byte; an empty input stays empty.
///
/// Why not the fixed 6-column `cat -n` pad: measured with
/// `estimate_content_tokens` on serialized results for six repository files,
/// the 6-column pad cost +33–47% tokens over raw content, the tight
/// right-aligned pad +22–32% (unpadded numbers +18–26%, but no alignment).
pub fn number_lines(content: &str, first_line: usize) -> String {
    let line_count = content.split_inclusive('\n').count();
    if line_count == 0 {
        return String::new();
    }
    let width = (first_line + line_count - 1).to_string().len();
    let mut out = String::with_capacity(content.len() + line_count * (width + 1));
    for (i, segment) in content.split_inclusive('\n').enumerate() {
        use std::fmt::Write as _;
        let _ = write!(out, "{:>width$}\t", first_line + i);
        out.push_str(segment);
    }
    out
}

/// Byte length of a leading line-number prefix (`^\s*\d+\t`) on `line`, or
/// `None` when the line does not start with one. Only horizontal whitespace
/// counts as the leading `\s*` (a line never contains `\n`).
pub fn numbered_prefix_len(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t') {
        i += 1;
    }
    let digits_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits_start || i >= bytes.len() || bytes[i] != b'\t' {
        return None;
    }
    Some(i + 1)
}

/// True when `text` is non-empty and EVERY physical line of it starts with a
/// line-number prefix.
pub fn is_numbered_text(text: &str) -> bool {
    !text.is_empty()
        && text
            .split_inclusive('\n')
            .all(|segment| numbered_prefix_len(segment).is_some())
}

/// Remove the line-number prefix from every line of `text`, or `None` when
/// any line lacks one (the text is then not a numbered listing and must be
/// used as written). Line terminators are kept.
pub fn strip_numbered(text: &str) -> Option<String> {
    if !is_numbered_text(text) {
        return None;
    }
    Some(strip_present_prefixes(text))
}

/// Remove a line-number prefix from each line that has one and leave other
/// lines unchanged. Used for an edit's replacement text once its `old_str`
/// has been proven to be a numbered copy: lines the model copied carry the
/// prefix, lines it added usually do not.
pub fn strip_present_prefixes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for segment in text.split_inclusive('\n') {
        match numbered_prefix_len(segment) {
            Some(n) => out.push_str(&segment[n..]),
            None => out.push_str(segment),
        }
    }
    out
}

/// The file text carried by a `file_read` result: `content` with the
/// tool's line-number prefixes removed when the result says it is numbered
/// (`"line_numbers": true`), `content` as is otherwise. `None` when the
/// result has no string `content` (an error, an "unchanged" note, a
/// spilled summary).
pub fn raw_file_read_content(result: &Value) -> Option<String> {
    let content = result.get("content")?.as_str()?;
    if result.get(LINE_NUMBERS_KEY).and_then(Value::as_bool) == Some(true) {
        // Every line the tool emits is numbered; if something upstream
        // rewrote a line (trust gate replacement keeps the prefix), strip
        // what is present rather than handing back numbered text.
        return Some(strip_present_prefixes(content));
    }
    Some(content.to_string())
}

/// [`raw_file_read_content`] for a serialized result.
pub fn raw_file_read_content_str(result: &str) -> Option<String> {
    let value: Value = serde_json::from_str(result).ok()?;
    raw_file_read_content(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbering_is_absolute_right_aligned_and_lossless() {
        let raw = "fn a() {}\r\n\n  let x = 1;\nlast";
        let numbered = number_lines(raw, 100);
        assert_eq!(
            numbered,
            "100\tfn a() {}\r\n101\t\n102\t  let x = 1;\n103\tlast"
        );
        assert_eq!(strip_numbered(&numbered).as_deref(), Some(raw));
        assert_eq!(number_lines("", 1), "");
        assert_eq!(number_lines("a\n", 1), "1\ta\n");
        assert_eq!(strip_numbered("     1\ta\n").as_deref(), Some("a\n"));
        // Right-aligned to the widest number in the output.
        let ten: String = (1..=10).map(|i| format!("l{i}\n")).collect();
        let ten = number_lines(&ten, 1);
        assert!(ten.starts_with(" 1\tl1\n 2\tl2\n"), "{ten}");
        assert!(ten.ends_with("10\tl10\n"), "{ten}");
        assert_eq!(number_lines("x\ny", 9), " 9\tx\n10\ty");
        assert_eq!(number_lines("x", 1_234_567), "1234567\tx");
    }

    #[test]
    fn prefix_detection_requires_digits_then_tab() {
        assert_eq!(numbered_prefix_len("    42\tcode"), Some(7));
        assert_eq!(numbered_prefix_len("42\tcode"), Some(3));
        assert_eq!(numbered_prefix_len("  \tcode"), None);
        assert_eq!(numbered_prefix_len("    42 code"), None);
        assert_eq!(numbered_prefix_len("x42\tcode"), None);
        assert_eq!(numbered_prefix_len("42"), None);
        assert!(is_numbered_text("  1\ta\n  2\tb"));
        assert!(!is_numbered_text("  1\ta\nb"));
        assert!(!is_numbered_text(""));
        assert_eq!(strip_numbered("  1\ta\nb"), None);
        assert_eq!(strip_present_prefixes("  1\ta\nnew\n  2\tb"), "a\nnew\nb");
    }

    #[test]
    fn secret_redaction_is_unaffected_by_line_number_prefixes() {
        // Redaction runs on the serialized result (agent loop and MCP
        // server), where the prefix's tab is the two characters `\t`: a
        // secret right after the prefix must still be caught.
        let env = "OPENAI_API_KEY=sk-abcdefghijklmnopqrstuvwxyz123456\n\
                   password=hunter2secret\n\
                   GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123\n\
                   sk-zyxwvutsrqponmlkjihgfedcba654321\n";
        for content in [env.to_string(), number_lines(env, 1)] {
            let serialized =
                serde_json::json!({"content": content, "line_numbers": true}).to_string();
            let redacted = crate::safety::redact::redact_secrets(&serialized);
            for secret in [
                "abcdefghijklmnopqrstuvwxyz123456",
                "hunter2secret",
                "ghp_abcdefghijklmnopqrstuvwxyz0123",
                "zyxwvutsrqponmlkjihgfedcba654321",
            ] {
                assert!(!redacted.contains(secret), "{secret} leaked: {redacted}");
            }
        }
    }

    #[test]
    fn raw_content_honours_the_line_numbers_flag() {
        let numbered = serde_json::json!({"content": "1\ta\n2\tb", "line_numbers": true});
        assert_eq!(raw_file_read_content(&numbered).as_deref(), Some("a\nb"));
        // Raw results (opt-out, or older results without the flag) are
        // returned untouched even when they happen to look numbered.
        let raw = serde_json::json!({"content": "1\tid\n2\tname", "line_numbers": false});
        assert_eq!(
            raw_file_read_content(&raw).as_deref(),
            Some("1\tid\n2\tname")
        );
        let legacy = serde_json::json!({"content": "1\tid"});
        assert_eq!(raw_file_read_content(&legacy).as_deref(), Some("1\tid"));
        assert_eq!(
            raw_file_read_content(&serde_json::json!({"note": "x"})),
            None
        );
        assert_eq!(
            raw_file_read_content_str(&numbered.to_string()).as_deref(),
            Some("a\nb")
        );
    }
}
