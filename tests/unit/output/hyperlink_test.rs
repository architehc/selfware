use super::*;

fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/lib.rs"), "fn a() {}\n").unwrap();
    std::fs::write(dir.path().join("src/my_file.rs"), "fn b() {}\n").unwrap();
    dir
}

fn url_of(root: &Path, rel: &str) -> String {
    file_url(&root.canonicalize().unwrap().join(rel)).unwrap()
}

// --- on/off conditions ---

#[test]
fn links_only_on_a_styled_non_json_terminal() {
    assert!(hyperlinks_wanted(true, false, None, Some("xterm-256color")));
    // Plain (non-tty), --no-color and NO_COLOR all arrive as `styled=false`.
    assert!(!hyperlinks_wanted(
        false,
        false,
        None,
        Some("xterm-256color")
    ));
    assert!(!hyperlinks_wanted(true, true, None, Some("xterm-256color")));
}

#[test]
fn env_override_turns_links_off_or_on_but_never_into_plain_output() {
    for off in ["0", "false", "OFF", " no "] {
        assert!(!hyperlinks_wanted(true, false, Some(off), Some("xterm")));
    }
    for on in ["1", "true", "On"] {
        assert!(hyperlinks_wanted(true, false, Some(on), Some("dumb")));
        // Forcing never reaches plain or JSON output.
        assert!(!hyperlinks_wanted(false, false, Some(on), Some("xterm")));
        assert!(!hyperlinks_wanted(true, true, Some(on), Some("xterm")));
    }
    // An unrecognised value keeps the default.
    assert!(hyperlinks_wanted(true, false, Some("maybe"), Some("xterm")));
}

#[test]
fn terminals_without_osc8_are_skipped_by_default() {
    assert!(!hyperlinks_wanted(true, false, None, Some("dumb")));
    assert!(!hyperlinks_wanted(true, false, None, Some("linux")));
    assert!(hyperlinks_wanted(true, false, None, None));
}

// --- linking ---

#[test]
fn existing_workspace_citation_is_wrapped_in_osc8() {
    let ws = workspace();
    let linker = Linker::new(ws.path()).unwrap();
    let out = linker.link("see src/lib.rs:1 and src/lib.rs:1-3 here");
    let url = url_of(ws.path(), "src/lib.rs");
    assert!(url.starts_with("file:///"), "{url}");
    assert_eq!(
        out,
        format!(
            "see {} and {} here",
            osc8(&url, "src/lib.rs:1"),
            osc8(&url, "src/lib.rs:1-3")
        )
    );
}

#[test]
fn hash_line_and_dot_slash_citations_are_linked() {
    let ws = workspace();
    let linker = Linker::new(ws.path()).unwrap();
    let url = url_of(ws.path(), "src/lib.rs");
    assert_eq!(linker.link("src/lib.rs#L1"), osc8(&url, "src/lib.rs#L1"));
    assert_eq!(linker.link("./src/lib.rs:1"), osc8(&url, "./src/lib.rs:1"));
}

#[test]
fn missing_outside_and_lineless_paths_are_not_linked() {
    let ws = workspace();
    let linker = Linker::new(ws.path()).unwrap();
    for text in [
        "src/missing.rs:3",           // does not exist
        "../escape.rs:1",             // outside the workspace
        "/etc/hosts.txt:1",           // absolute, outside
        "src/lib.rs without a line",  // not a citation
        "https://x.com/src/lib.rs:1", // part of a URL
        "src:1 plain",
    ] {
        assert_eq!(linker.link(text), text, "{text}");
    }
}

#[test]
fn symlink_escaping_the_workspace_is_not_linked() {
    #[cfg(unix)]
    {
        let ws = workspace();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.rs"), "x\n").unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret.rs"), ws.path().join("s.rs"))
            .unwrap();
        let linker = Linker::new(ws.path()).unwrap();
        assert_eq!(linker.link("s.rs:1"), "s.rs:1");
    }
}

#[test]
fn url_escaping_keeps_the_sequence_intact() {
    let dir = tempfile::tempdir().unwrap();
    let odd = dir.path().join("a b;c");
    std::fs::create_dir_all(&odd).unwrap();
    std::fs::write(odd.join("x.rs"), "x\n").unwrap();
    let linker = Linker::new(&odd).unwrap();
    let out = linker.link("x.rs:1");
    let url = out
        .strip_prefix("\x1b]8;;")
        .and_then(|r| r.split("\x1b\\").next())
        .unwrap();
    assert!(url.contains("a%20b%3Bc/x.rs"), "{url}");
    assert!(!url.contains(' ') && !url.contains(';'));
    assert!(!url.chars().any(|c| c.is_control()));
    assert!(out.ends_with("x.rs:1\x1b]8;;\x1b\\"));
}

#[test]
fn osc8_is_invisible_to_strip_ansi() {
    let ws = workspace();
    let linker = Linker::new(ws.path()).unwrap();
    let out = linker.link("at src/lib.rs:1.");
    assert_ne!(out, "at src/lib.rs:1.");
    assert_eq!(crate::output::live::strip_ansi(&out), "at src/lib.rs:1.");
}

// --- the prose renderer ---

#[test]
fn styled_prose_links_citations_in_text_and_inline_code() {
    let ws = workspace();
    let linker = Linker::new(ws.path()).unwrap();
    let url = url_of(ws.path(), "src/my_file.rs");
    let out = crate::output::live::render_prose_linked(
        "The bug is in `src/my_file.rs:1` and src/my_file.rs:1.\n",
        true,
        Some(linker),
    );
    assert_eq!(
        out.matches(&format!("\x1b]8;;{url}\x1b\\")).count(),
        2,
        "{out:?}"
    );
    assert_eq!(
        crate::output::live::strip_ansi(&out),
        "The bug is in src/my_file.rs:1 and src/my_file.rs:1.\n"
    );
}

#[test]
fn plain_prose_never_carries_links() {
    let ws = workspace();
    let linker = Linker::new(ws.path()).unwrap();
    let out = crate::output::live::render_prose_linked("see src/lib.rs:1\n", false, Some(linker));
    assert_eq!(out, "see src/lib.rs:1\n");
}

#[test]
fn citations_inside_fences_and_markdown_links_are_not_linked() {
    let ws = workspace();
    let linker = Linker::new(ws.path()).unwrap();
    let out = crate::output::live::render_prose_linked(
        "```\nsrc/lib.rs:1\n```\n[src/lib.rs:1](https://example.com)\n",
        true,
        Some(linker),
    );
    assert!(!out.contains("\x1b]8;;"), "{out:?}");
}
