use super::*;

fn denied() -> Vec<String> {
    crate::config::safety::default_denied_paths()
}

/// A search root holding `.env`, `secrets/`, `.ssh/` next to ordinary
/// code.
fn fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    for dir in ["src", "docs", "secrets", ".ssh"] {
        std::fs::create_dir_all(d.join(dir)).unwrap();
    }
    std::fs::write(d.join(".env"), "API_KEY=sk-live\n").unwrap();
    std::fs::write(d.join("secrets/prod.txt"), "hunter2\n").unwrap();
    std::fs::write(d.join(".ssh/id_rsa"), "-----BEGIN\n").unwrap();
    std::fs::write(d.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(d.join("src/environment.rs"), "pub fn env() {}\n").unwrap();
    std::fs::write(d.join("docs/a.md"), "# a\n").unwrap();
    tmp
}

fn violation(cmd: &str, base: &Path) -> Option<String> {
    recursive_read_violation(&lenient_commands(cmd), base, &denied())
}

fn strict_violation(cmd: &str, base: &Path) -> Option<String> {
    let segments = crate::safety::shell_read::parse(cmd).expect("parses");
    let commands: Vec<Command> = segments.iter().map(Command::from_segment).collect();
    recursive_read_violation(&commands, base, &denied())
}

#[test]
fn recursive_readers_reaching_denied_files_are_refused() {
    let tmp = fixture();
    let d = tmp.path();
    for cmd in [
        "grep -r KEY .",
        "grep -rn KEY",
        "grep -R KEY .",
        "grep --recursive KEY .",
        "grep -d recurse KEY .",
        "egrep -ril key .",
        "rg KEY",
        "rg -uu KEY .",
        "rg --hidden KEY .",
        "rg --no-ignore KEY",
        "rg -g '*.env' KEY",
        "ag KEY",
        "git grep KEY",
        "grep -r KEY secrets",
        "diff -r . docs",
        "find . -type f -exec cat {} +",
        "find . -type f | xargs cat",
        "cp -r . /tmp/out-never-created",
        "tar -cf - .",
        "sh -c 'grep -r KEY .'",
        "cd src && cd .. && grep -r KEY .",
        "timeout 5 grep -r KEY .",
        "FOO=1 rg -uu KEY",
    ] {
        assert!(violation(cmd, d).is_some(), "`{cmd}` must be refused");
    }
    // The strict parser's segments give the same answer.
    for cmd in [
        "grep -r KEY .",
        "rg -uu KEY",
        "git grep KEY",
        "diff -r . docs",
    ] {
        assert!(
            strict_violation(cmd, d).is_some(),
            "`{cmd}` must be refused"
        );
    }
    let why = violation("grep -r KEY .", d).unwrap();
    assert!(why.contains("grep"), "{why}");
}

#[test]
fn recursive_readers_of_clean_roots_pass() {
    let tmp = fixture();
    let d = tmp.path();
    for cmd in [
        "grep -r fn src",
        "grep -rn KEY src/main.rs",
        "grep KEY docs/a.md",
        "grep -r fn src docs",
        "rg fn src",
        "rg -uu fn src",
        "find . -name '*.rs'",
        "find src -type f -exec wc -l {} +",
        "ls -R",
        "tree",
        "cat src/main.rs",
        "diff src/main.rs docs/a.md",
        "git grep fn -- src",
    ] {
        assert_eq!(violation(cmd, d), None, "`{cmd}` should pass");
    }
    assert_eq!(strict_violation("grep -rn fn src", d), None);
}

/// ripgrep's default walk skips hidden entries, so `.env` / `.ssh` alone
/// do not refuse it; a `.gitignore`d directory is pruned only on a plain
/// pattern and only when nothing is re-included.
#[test]
fn ripgrep_default_walk_skips_hidden_and_plain_gitignored_dirs() {
    let tmp = fixture();
    let d = tmp.path();
    std::fs::remove_dir_all(d.join("secrets")).unwrap();
    assert_eq!(violation("rg KEY", d), None);
    assert_eq!(violation("rg KEY .", d), None);
    assert!(violation("rg -. KEY", d).is_some());
    assert!(
        violation("rg -u KEY", d).is_none(),
        "-u keeps hidden skipped"
    );
    assert!(violation("rg -uu KEY", d).is_some());

    std::fs::create_dir_all(d.join("secrets")).unwrap();
    std::fs::write(d.join("secrets/prod.txt"), "x\n").unwrap();
    assert!(
        violation("rg KEY", d).is_some(),
        "not a repository: no pruning"
    );
    std::fs::create_dir(d.join(".git")).unwrap();
    std::fs::write(d.join(".gitignore"), "/secrets/\ntarget\n").unwrap();
    assert_eq!(violation("rg KEY", d), None, "gitignored dir is pruned");
    assert!(violation("rg --no-ignore KEY", d).is_some());
    std::fs::write(d.join(".gitignore"), "secrets/\n!secrets/keep\n").unwrap();
    assert!(
        violation("rg KEY", d).is_some(),
        "re-includes disable pruning"
    );
    std::fs::write(d.join(".gitignore"), "secre*\n").unwrap();
    assert!(
        violation("rg KEY", d).is_some(),
        "glob patterns are not pruned"
    );
}

#[cfg(unix)]
#[test]
fn symlinks_are_vetted_by_target() {
    let tmp = fixture();
    let d = tmp.path();
    std::os::unix::fs::symlink(d.join(".ssh/id_rsa"), d.join("docs/notes.txt")).unwrap();
    assert!(violation("grep -r x docs", d).is_some());
    assert!(violation("grep -R x docs", d).is_some());
}

#[test]
fn sensitive_components_are_matched_per_component() {
    assert_eq!(sensitive_component("src/environment.rs", false), None);
    assert_eq!(sensitive_component("envelope/.envoy", false), None);
    assert_eq!(sensitive_component(".env.example", false), Some(".env"));
    assert_eq!(sensitive_component("deploy/prod.env", false), Some(".env"));
    assert_eq!(sensitive_component("a/.ssh", true), Some(".ssh/"));
    assert_eq!(sensitive_component("a/secrets/x", false), Some("/secrets/"));
    assert_eq!(sensitive_component("certs/server.pem", false), Some(".pem"));
    assert_eq!(
        sensitive_component("proc/1/environ", false),
        Some("/environ")
    );
    assert_eq!(sensitive_component("secretsauce.rs", false), None);
}

#[test]
fn wrappers_and_assignments_are_seen_through() {
    let cmds = lenient_commands("RIPGREP_CONFIG_PATH=/x rg -n foo | head; echo $(grep -r a b)");
    assert_eq!(
        cmds[0].assignments,
        vec!["RIPGREP_CONFIG_PATH=/x".to_string()]
    );
    assert_eq!(cmds[0].words[0], "rg");
    assert!(cmds
        .iter()
        .any(|c| c.words.first().map(String::as_str) == Some("grep")));
    assert_eq!(
        strip_wrappers(&["env".into(), "A=1".into(), "grep".into()])[0],
        "grep"
    );
    assert_eq!(
        strip_wrappers(&[
            "timeout".into(),
            "-k".into(),
            "5".into(),
            "10".into(),
            "grep".into()
        ])[0],
        "grep"
    );
}
