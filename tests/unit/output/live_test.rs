use super::*;

/// Render in chunks of `size` bytes (at char boundaries), then finish.
fn render_chunked(text: &str, styled: bool, size: usize) -> String {
    let mut r = ProseRenderer::new(styled);
    let mut out = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        let mut cut = size.min(rest.len());
        while !rest.is_char_boundary(cut) {
            cut += 1;
        }
        out.push_str(&r.push(&rest[..cut]));
        rest = &rest[cut..];
    }
    out.push_str(&r.finish());
    out
}

// --- Markdown rendering (final answer printed raw Markdown, 0.9.1) ---

#[test]
fn styled_rendering_removes_markdown_syntax_and_adds_styling() {
    let md = "## Summary\n\nThe **fix** is in `src/lib.rs`:\n- first *point*\n1. step one\n> quoted\n---\n";
    let out = render_prose(md, true);
    let plain = strip_ansi(&out);
    assert_eq!(
        plain,
        format!(
            "Summary\n\nThe fix is in src/lib.rs:\n• first point\n1. step one\n│ quoted\n{}\n",
            "─".repeat(40)
        )
    );
    assert!(
        out.contains(&format!("{BOLD}{UNDERLINE}Summary{RESET}")),
        "{out:?}"
    );
    assert!(out.contains(&format!("{BOLD}fix{RESET}")), "{out:?}");
    assert!(out.contains(&format!("{CODE}src/lib.rs{RESET}")), "{out:?}");
    assert!(out.contains(&format!("{ITALIC}point{RESET}")), "{out:?}");
}

#[test]
fn styled_code_fences_keep_code_verbatim_including_blank_lines() {
    let md = "Run:\n```rust\nfn main() {\n\n\n    let x = **y**;\n}\n```\nDone.";
    let plain = strip_ansi(&render_prose(md, true));
    // Code is not Markdown-rendered and keeps its blank lines and indentation.
    assert_eq!(
        plain,
        "Run:\n```rust\nfn main() {\n\n\n    let x = **y**;\n}\n```\nDone.\n"
    );
}

#[test]
fn unstyled_rendering_keeps_raw_text() {
    // Plain mode / --no-color / NO_COLOR: no ANSI, Markdown untouched.
    let md = "## Title\n**bold** and `code`\n- item";
    let out = render_prose(md, false);
    assert_eq!(out, "## Title\n**bold** and `code`\n- item\n");
    assert!(!out.contains('\x1b'));
}

#[test]
fn links_show_their_target_once() {
    let out = strip_ansi(&render_prose(
        "see [docs](https://x.io) and <https://y.io>",
        true,
    ));
    assert_eq!(out, "see docs (https://x.io) and https://y.io\n");
}

#[test]
fn lines_the_parser_swallows_are_still_shown() {
    let out = strip_ansi(&render_prose("[ref]: https://x.io", true));
    assert_eq!(out, "[ref]: https://x.io\n");
}

#[test]
fn chunking_does_not_change_the_rendering() {
    let md = "# H\n\nPara with **bold across** and `code`.\n\n\n\n- a\n- b\n```\nx\n```\n";
    let whole = render_prose(md, true);
    for size in [1, 2, 3, 5, 8, 13] {
        assert_eq!(render_chunked(md, true, size), whole, "chunk size {size}");
    }
}

// --- Blank-line collapse (3-5 empty lines between blocks, 0.9.1) ---

#[test]
fn blank_line_runs_collapse_to_one_and_leading_trailing_are_dropped() {
    for styled in [false, true] {
        let out = strip_ansi(&render_prose("\n\n\nA\n\n\n\n\nB\n  \n\t\nC\n\n\n", styled));
        assert_eq!(out, "A\n\nB\n\nC\n", "styled={styled}");
    }
}

#[test]
fn renderer_holds_an_unfinished_line_until_its_newline() {
    let mut r = ProseRenderer::new(false);
    assert_eq!(r.push("Hello wor"), "");
    assert!(!r.started());
    assert_eq!(r.push("ld\nNext"), "Hello world\n");
    assert_eq!(r.finish(), "Next\n");
}

// --- Print-once ledger (answer printed twice / never, 0.9.1) ---

#[test]
fn ledger_reports_nothing_unshown_for_a_streamed_answer() {
    let mut l = AnswerLedger::new();
    l.record("The answer is **42**.\n\nBecause reasons.", true);
    // Whitespace differences (think-block stripping, trimming) do not matter.
    assert_eq!(l.unshown("The answer is **42**.\nBecause reasons."), None);
    // A fragment of what was shown is shown.
    assert_eq!(l.unshown("Because reasons."), None);
}

#[test]
fn ledger_prints_an_unstreamed_answer_in_full() {
    // Planning fast path / cache hit: nothing was streamed.
    let l = AnswerLedger::new();
    assert_eq!(l.unshown("Final text"), Some("Final text"));
    let mut l = AnswerLedger::new();
    l.record("Let me read the file.", false);
    assert_eq!(l.unshown("Totally different"), Some("Totally different"));
}

#[test]
fn ledger_prints_only_an_appended_note() {
    let mut l = AnswerLedger::new();
    l.record("Partial answer that got cut", true);
    let full = "Partial answer that got cut\n\n[NOTE: this answer was cut off]";
    assert_eq!(l.unshown(full), Some("\n\n[NOTE: this answer was cut off]"));
    let lead = "[NOTE: may be reasoning]\n\nPartial answer that got cut";
    assert_eq!(l.unshown(lead), Some("[NOTE: may be reasoning]\n\n"));
}

#[test]
fn ledger_candidate_is_the_last_tool_free_response() {
    let mut l = AnswerLedger::new();
    l.record("answer A", true);
    l.record("narration before a tool call", false);
    assert_eq!(l.candidate(), Some("answerA"));
    l.record("", true); // nothing shown: no change
    assert_eq!(l.candidate(), Some("answerA"));
}

#[test]
fn echo_gate_drops_a_word_for_word_repeat() {
    // Planning streamed the answer; execution repeats it.
    let mut g = EchoGate::new(Some(squash("Paris is the capital.\nOf France.")));
    assert_eq!(g.push("Paris is "), "");
    assert_eq!(g.push("the capital.\n"), "");
    assert_eq!(g.push("Of France."), "");
    assert_eq!(g.finish(), "");
}

#[test]
fn echo_gate_releases_everything_on_divergence() {
    let mut g = EchoGate::new(Some(squash("Paris is the capital.")));
    assert_eq!(g.push("Paris is "), "");
    assert_eq!(g.push("big."), "Paris is big.");
    assert_eq!(g.push(" More."), " More.");
    assert_eq!(g.finish(), "");
    // A repeat that stops early is shown (it is a different, shorter text).
    let mut g = EchoGate::new(Some(squash("Paris is the capital.")));
    assert_eq!(g.push("Paris is"), "");
    assert_eq!(g.finish(), "Paris is");
    // No target: pass-through.
    let mut g = EchoGate::new(None);
    assert_eq!(g.push("x"), "x");
    assert_eq!(g.finish(), "");
}
