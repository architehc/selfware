use super::*;
use serde_json::json;

fn texts(lines: &[ConfirmLine]) -> Vec<String> {
    lines.iter().map(|l| l.text.clone()).collect()
}

fn kinds(lines: &[ConfirmLine], kind: LineKind) -> usize {
    lines.iter().filter(|l| l.kind == kind).count()
}

#[test]
fn file_edit_renders_path_counts_and_both_sides_not_raw_json() {
    // The live 0.9.1 prompt: raw escaped JSON, new_str only, cut at ~200 chars.
    let args = json!({
        "path": "slug.py",
        "old_str": "def slug(s):\n    return s.lower()\n",
        "new_str": "def slug(s, *, replacement_stage=None):\n    \"\"\"Make a slug.\"\"\"\n    return s.lower()\n",
    })
    .to_string();
    let lines = render_tool_call("file_edit", &args, None);
    assert_eq!(lines[0].kind, LineKind::Header);
    assert_eq!(lines[0].text, "slug.py  +2 −1 lines");
    let all = texts(&lines).join("\n");
    assert!(
        all.contains("-def slug(s):"),
        "old side must be shown: {all}"
    );
    assert!(all.contains("+def slug(s, *, replacement_stage=None):"));
    // Strings are unescaped: a real quote, no JSON escapes, no braces.
    assert!(all.contains("+    \"\"\"Make a slug.\"\"\""));
    assert!(!all.contains("\\n") && !all.contains("\\\""));
    assert!(!all.contains("\"new_str\""));
    assert_eq!(kinds(&lines, LineKind::Added), 2);
    assert_eq!(kinds(&lines, LineKind::Removed), 1);
    assert_eq!(kinds(&lines, LineKind::Context), 1);
}

#[test]
fn file_edit_accepts_argument_aliases() {
    let args = json!({"file_path": "a.rs", "old_string": "x\n", "new_string": "y\n"}).to_string();
    let lines = render_tool_call("file_edit", &args, None);
    assert_eq!(lines[0].text, "a.rs  +1 −1 lines");
}

#[test]
fn large_diff_is_capped_with_more_lines_note() {
    let new: String = (0..100).map(|i| format!("line {i}\n")).collect();
    let args = json!({"path": "big.txt", "old_str": "", "new_str": new}).to_string();
    let lines = render_tool_call("file_edit", &args, None);
    assert_eq!(lines[0].text, "big.txt  +100 −0 lines");
    let body = lines
        .iter()
        .filter(|l| {
            matches!(
                l.kind,
                LineKind::Added | LineKind::Removed | LineKind::Context
            )
        })
        .count();
    assert_eq!(body, MAX_BODY_LINES);
    assert_eq!(lines.last().unwrap().text, "… 60 more lines");
}

#[test]
fn multi_edit_shows_every_file_header_within_one_budget() {
    let args = json!({"edits": [
        {"path": "a.rs", "old_str": "a\n", "new_str": "b\n"},
        {"path": "b.rs", "old_str": "c\nd\n", "new_str": "c\n"},
    ]})
    .to_string();
    let lines = render_tool_call("file_multi_edit", &args, None);
    let headers: Vec<_> = lines
        .iter()
        .filter(|l| l.kind == LineKind::Header)
        .map(|l| l.text.clone())
        .collect();
    assert_eq!(headers, vec!["a.rs  +1 −1 lines", "b.rs  +0 −1 lines"]);
}

#[test]
fn file_write_new_file_shows_line_count_and_first_lines() {
    let content: String = (0..30).map(|i| format!("row {i}\n")).collect();
    let args = json!({"path": "new.txt", "content": content}).to_string();
    let lines = render_tool_call("file_write", &args, None);
    assert_eq!(lines[0].text, "new.txt  new file, 30 lines");
    assert_eq!(lines[1].text, "+row 0");
    assert_eq!(kinds(&lines, LineKind::Added), 20);
    assert_eq!(lines.last().unwrap().text, "… 10 more lines");
}

#[test]
fn file_write_over_existing_file_is_a_diff() {
    let args = json!({"path": "x.txt", "content": "a\nB\nc\n"}).to_string();
    let lines = render_tool_call("file_write", &args, Some("a\nb\nc\n"));
    assert_eq!(lines[0].text, "x.txt  +1 −1 lines  (overwrite)");
    let all = texts(&lines);
    assert!(all.contains(&"-b".to_string()) && all.contains(&"+B".to_string()));
}

#[test]
fn patch_apply_is_coloured_and_counted() {
    let diff = "--- a/f.rs\n+++ b/f.rs\n@@ -1,2 +1,2 @@\n ctx\n-old\n+new\n+more\n";
    let args = json!({"diff": diff}).to_string();
    let lines = render_tool_call("patch_apply", &args, None);
    assert_eq!(lines[0].text, "patch: 1 file  +2 −1 lines");
    assert_eq!(kinds(&lines, LineKind::Hunk), 1);
    assert_eq!(kinds(&lines, LineKind::Added), 2);
    assert_eq!(kinds(&lines, LineKind::Removed), 1);
}

#[test]
fn other_tools_render_key_value_lines_unescaped() {
    let args =
        json!({"command": "python3 -m unittest -k \"slug\"", "timeout_secs": 60}).to_string();
    let lines = render_tool_call("shell_exec", &args, None);
    let all = texts(&lines);
    assert!(all.contains(&"command: python3 -m unittest -k \"slug\"".to_string()));
    assert!(all.contains(&"timeout_secs: 60".to_string()));
    assert!(lines.iter().all(|l| !l.text.starts_with('{')));
}

#[test]
fn long_shell_command_is_shown_in_full_wrapped_not_cut_at_200() {
    let cmd = format!("echo {}", "x".repeat(450));
    let args = json!({"command": cmd}).to_string();
    let lines = render_tool_call("shell_exec", &args, None);
    let joined: String = lines
        .iter()
        .map(|l| {
            l.text
                .trim_start_matches("command: ")
                .trim_start()
                .to_string()
        })
        .collect();
    assert_eq!(joined, cmd);
}

#[test]
fn multiline_string_arguments_are_bounded() {
    let body: String = (0..20).map(|i| format!("l{i}\n")).collect();
    let args = json!({"body": body}).to_string();
    let lines = render_tool_call("http_request", &args, None);
    assert_eq!(lines[0].text, "body: (20 lines)");
    assert_eq!(lines.last().unwrap().text, "… 12 more lines");
}

#[test]
fn control_characters_cannot_reach_the_terminal() {
    // An ESC sequence in a model-authored argument must not repaint the prompt.
    let args =
        json!({"path": "a\u{1b}[2Jb.rs", "old_str": "x\r\n", "new_str": "\u{7}y\n"}).to_string();
    let lines = render_tool_call("file_edit", &args, None);
    for line in &lines {
        assert!(
            !line.text.chars().any(|c| c.is_control()),
            "control char leaked: {:?}",
            line.text
        );
    }
    assert!(lines[0].text.starts_with("a\\u{1b}[2Jb.rs"));
}

#[test]
fn unparseable_arguments_fall_back_to_a_bounded_raw_line() {
    let raw = format!("not json {}", "z".repeat(500));
    let lines = render_tool_call("file_edit", &raw, None);
    assert_eq!(lines[0].text, "(arguments are not a JSON object)");
    assert!(lines[1].text.chars().count() <= MAX_LINE_CHARS + 1);
}

#[test]
fn sanitize_line_truncates_with_ellipsis() {
    assert_eq!(sanitize_line("abcdef", 3), "abc…");
    assert_eq!(sanitize_line("a\tb", 10), "a  b");
}

#[test]
fn file_write_target_only_for_file_write() {
    let args = json!({"path": "p.txt", "content": ""}).to_string();
    assert_eq!(
        file_write_target("file_write", &args).as_deref(),
        Some("p.txt")
    );
    assert_eq!(file_write_target("file_edit", &args), None);
}

// ===== risk tags =====

fn shell(cmd: &str) -> RiskTag {
    classify_risk("shell_exec", &json!({ "command": cmd }))
}

#[test]
fn pip_install_is_not_tagged_like_grep() {
    // Live 0.9.1: `pip3 install -r dev.requirements.txt` looked identical to grep.
    assert_eq!(
        shell("pip3 install -r dev.requirements.txt"),
        RiskTag::InstallsPackages
    );
    assert_eq!(shell("grep -rn slug src"), RiskTag::Reads);
    assert_ne!(shell("pip3 install x").label(), shell("grep x y").label());
}

#[test]
fn shell_install_heuristics() {
    for cmd in [
        "pip install requests",
        "python3 -m pip install -e .",
        "npm install",
        "npm ci",
        "yarn add left-pad",
        "cargo install ripgrep",
        "cargo add serde",
        "brew install jq",
        "sudo apt-get install -y curl",
        "go install example.com/x@latest",
        "uv pip install foo",
    ] {
        assert_eq!(shell(cmd), RiskTag::InstallsPackages, "{cmd}");
    }
}

#[test]
fn shell_network_git_delete_and_write_heuristics() {
    assert_eq!(shell("curl -sL https://example.com"), RiskTag::Network);
    assert_eq!(shell("wget https://x/y.tgz"), RiskTag::Network);
    assert_eq!(shell("git clone https://x/y"), RiskTag::Network);
    assert_eq!(shell("git push origin main"), RiskTag::GitHistory);
    assert_eq!(shell("git commit -m wip"), RiskTag::GitHistory);
    assert_eq!(shell("git reset --hard HEAD~1"), RiskTag::GitHistory);
    assert_eq!(shell("rm -rf build"), RiskTag::DeletesFiles);
    assert_eq!(shell("find . -name '*.pyc' -delete"), RiskTag::DeletesFiles);
    assert_eq!(shell("echo hi > out.txt"), RiskTag::WritesWorkspace);
    assert_eq!(shell("sed -i 's/a/b/' f.txt"), RiskTag::WritesWorkspace);
    assert_eq!(shell("git status"), RiskTag::Reads);
    assert_eq!(shell("git log --oneline -5"), RiskTag::Reads);
}

#[test]
fn compound_commands_take_the_most_severe_segment() {
    assert_eq!(shell("ls && rm -rf target"), RiskTag::DeletesFiles);
    assert_eq!(shell("cat a | grep b; curl http://x"), RiskTag::Network);
    assert_eq!(shell("ls\ngit push"), RiskTag::GitHistory);
    // A pipe inside quotes is an argument, not a separator: the quoted
    // `rm -rf` is not a deletion segment. (The whole-command read-only
    // classifier still refuses to vouch for the text, so the tag is the
    // conservative `[runs command]`, never `[reads]`.)
    assert_eq!(shell("grep 'a | rm -rf x' file"), RiskTag::RunsCommand);
}

#[test]
fn unrecognised_commands_are_never_labelled_reads() {
    assert_eq!(shell("python3 fix.py"), RiskTag::RunsCommand);
    assert_eq!(shell("./configure"), RiskTag::RunsCommand);
    assert_eq!(shell("make"), RiskTag::RunsCommand);
}

#[test]
fn tool_level_risk_tags() {
    let none = json!({});
    assert_eq!(classify_risk("file_read", &none), RiskTag::Reads);
    assert_eq!(classify_risk("context_bulk_read", &none), RiskTag::Reads);
    assert_eq!(classify_risk("file_edit", &none), RiskTag::WritesWorkspace);
    assert_eq!(classify_risk("file_write", &none), RiskTag::WritesWorkspace);
    assert_eq!(
        classify_risk("patch_apply", &none),
        RiskTag::WritesWorkspace
    );
    assert_eq!(classify_risk("file_delete", &none), RiskTag::DeletesFiles);
    assert_eq!(classify_risk("git_commit", &none), RiskTag::GitHistory);
    assert_eq!(classify_risk("git_push", &none), RiskTag::GitHistory);
    assert_eq!(
        classify_risk("pip_install", &none),
        RiskTag::InstallsPackages
    );
    assert_eq!(classify_risk("http_request", &none), RiskTag::Network);
    assert_eq!(classify_risk("cargo_test", &none), RiskTag::RunsCommand);
    assert_eq!(
        classify_risk("cargo_clippy", &json!({"fix": true})),
        RiskTag::WritesWorkspace
    );
    assert_eq!(classify_risk("mcp_thing", &none), RiskTag::Unclassified);
    assert_eq!(RiskTag::InstallsPackages.label(), "[installs packages]");
}
