use super::*;
use crate::output::live::strip_ansi;

#[test]
fn fence_language_reads_the_info_string() {
    assert_eq!(fence_language("```rust"), Some("rust"));
    assert_eq!(fence_language("  ~~~ py title=x"), Some("py"));
    assert_eq!(fence_language("```{.python}"), Some("python"));
    assert_eq!(fence_language("```rust,ignore"), Some("rust"));
    assert_eq!(fence_language("```"), None);
    assert_eq!(fence_language("```   "), None);
}

#[test]
fn requested_languages_resolve_to_a_grammar_or_stay_plain() {
    for lang in [
        "rust",
        "rs",
        "python",
        "py",
        "js",
        "javascript",
        "ts",
        "typescript",
        "json",
        "sh",
        "bash",
        "shell",
    ] {
        assert!(
            CodeHighlighter::for_language(lang, true).is_some(),
            "{lang} has no grammar"
        );
    }
    // No bundled TOML grammar / unknown tag: plain code colour (None).
    assert!(CodeHighlighter::for_language("toml", true).is_none());
    assert!(CodeHighlighter::for_language("no-such-lang", true).is_none());
}

#[test]
fn highlighting_adds_colour_but_never_changes_the_text() {
    let mut h = CodeHighlighter::for_language("rust", true).unwrap();
    let line = r#"fn main() { let s = "hi"; // note"#;
    let out = h.line(line);
    assert_ne!(out, line);
    assert!(out.contains("\x1b[38;2;"), "{out:?}");
    assert!(out.ends_with("\x1b[0m"));
    assert_eq!(strip_ansi(&out), line);
}

#[test]
fn without_truecolor_the_256_colour_palette_is_used() {
    let mut h = CodeHighlighter::for_language("python", false).unwrap();
    let out = h.line("def f(x): return 'a'");
    assert!(out.contains("\x1b[38;5;"), "{out:?}");
    assert!(!out.contains("\x1b[38;2;"));
    assert_eq!(strip_ansi(&out), "def f(x): return 'a'");
}

#[test]
fn state_carries_across_lines_of_one_block() {
    let mut h = CodeHighlighter::for_language("python", true).unwrap();
    h.line("s = \"\"\"start");
    // Inside the triple-quoted string: the whole line is string-coloured,
    // not highlighted as code (`def` is not a keyword here).
    let inside = h.line("def not_code");
    let mut fresh = CodeHighlighter::for_language("python", true).unwrap();
    assert_ne!(inside, fresh.line("def not_code"));
}

#[test]
fn truecolor_detection_and_palette_mapping() {
    assert!(truecolor_from(Some("truecolor")));
    assert!(truecolor_from(Some("24bit")));
    assert!(!truecolor_from(Some("")));
    assert!(!truecolor_from(None));
    assert_eq!(ansi256(0, 0, 0), 16);
    assert_eq!(ansi256(255, 255, 255), 231);
    assert_eq!(ansi256(255, 0, 0), 196);
    assert_eq!(ansi256(128, 128, 128), 244);
}

#[test]
fn prose_renderer_highlights_styled_fences_only() {
    let md = "Code:\n```rust\nfn a() {}\n\nlet x = 1;\n```\nafter\n";
    let styled = crate::output::live::render_prose_linked(md, true, None);
    assert!(styled.contains("\x1b[38;"), "{styled:?}");
    assert_eq!(
        strip_ansi(&styled),
        "Code:\n```rust\nfn a() {}\n\nlet x = 1;\n```\nafter\n"
    );
    // Plain output: the raw text, no escapes at all.
    let plain = crate::output::live::render_prose_linked(md, false, None);
    assert_eq!(plain, md);
    assert!(!plain.contains('\x1b'));
}

#[test]
fn unknown_language_fence_keeps_the_plain_code_colour() {
    let styled = crate::output::live::render_prose_linked("```toml\na = 1\n```\n", true, None);
    assert!(!styled.contains("\x1b[38;"), "{styled:?}");
    assert!(styled.contains("\x1b[36ma = 1\x1b[0m"));
}

#[test]
fn many_lines_stay_aligned_with_their_text() {
    let mut h = CodeHighlighter::for_language("rust", false).unwrap();
    for i in 0..200 {
        let line = format!("let v{i} = {i}; // line {i}");
        assert_eq!(strip_ansi(&h.line(&line)), line);
    }
}
