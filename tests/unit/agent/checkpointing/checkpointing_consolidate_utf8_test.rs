//! Regression: `consolidate_session_memory` byte-sliced logged tool-call
//! args at `&args[..args.len().min(200)]`, panicking whenever byte 200
//! fell inside a multibyte char — after task success, before the
//! checkpoint could be marked Completed (consolidation is a default
//! feature, so this hit every successful task with multibyte args).

use super::{tool_call_content_preview, truncate_bytes_char_boundary};

#[test]
fn truncate_never_splits_a_multibyte_char() {
    // 199 ASCII bytes + a 3-byte char (€) straddling byte 200.
    let mut s = "a".repeat(199);
    s.push('€'); // bytes 199..202
    s.push_str("tail");
    let out = truncate_bytes_char_boundary(&s, 200);
    assert_eq!(out.len(), 199, "must back off to the char boundary");
    assert!(out.ends_with('a'));

    // Exactly at a boundary: no backoff.
    let s2 = format!("{}€", "b".repeat(197)); // 197 + 3 = exactly 200
    let out2 = truncate_bytes_char_boundary(&s2, 200);
    assert_eq!(out2.len(), 200);
    assert!(out2.ends_with('€'));

    // Shorter than the limit: untouched.
    assert_eq!(truncate_bytes_char_boundary("short", 200), "short");
}

#[test]
fn tool_call_preview_handles_multibyte_args_straddling_byte_200() {
    // CJK chars are 3 bytes; 100 of them = 300 bytes, so byte 200 lands
    // mid-char — the exact input class that panicked before.
    let args: String = "界".repeat(100);
    let preview = tool_call_content_preview("shell_exec", &args, true);
    assert!(preview.starts_with("Tool: shell_exec | Args: "));
    assert!(preview.ends_with("| Success: true"));
    // 66 chars * 3 = 198 bytes of args survive (never a panic, never a
    // split codepoint).
    let args_part = preview
        .strip_prefix("Tool: shell_exec | Args: ")
        .unwrap()
        .strip_suffix(" | Success: true")
        .unwrap();
    assert_eq!(args_part.len(), 198);
    assert_eq!(args_part.chars().count(), 66);
}
