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

#[cfg(any(unix, windows))]
#[test]
fn recursive_readers_refuse_discovered_hard_links_but_exact_reads_pass() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir(root.join("src")).unwrap();
    let private = root.join("private-state.txt");
    std::fs::write(&private, "private marker\n").unwrap();
    let public_alias = root.join("src/public-looking.txt");
    std::fs::hard_link(&private, &public_alias).unwrap();

    let why = violation("grep -r marker src", root)
        .expect("a recursively discovered hard link must be refused");
    assert!(why.contains("file identity"), "{why}");

    assert_eq!(
        violation("grep marker src/public-looking.txt", root),
        None,
        "an explicitly selected file keeps the exact-read policy"
    );
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
fn explicit_private_discovery_root_is_a_violation() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    std::fs::create_dir_all(d.join(".claude")).unwrap();
    std::fs::write(d.join(".claude/.env"), "API_KEY=sk-live\n").unwrap();
    let why = violation("grep -r KEY .claude", d).expect("private root must be refused");
    assert!(why.contains("private discovery"), "{why}");
    assert!(why.contains("grep"), "{why}");
}

#[test]
fn grep_recursive_refuses_nested_private_discovery_directories() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    std::fs::create_dir_all(d.join("src/.claude")).unwrap();
    std::fs::write(d.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(d.join("src/.claude/secret"), "transcript\n").unwrap();
    let why = violation("grep -r fn src", d).expect("nested .claude must be refused");
    assert!(why.contains("private discovery"), "{why}");
    let why = violation("git grep fn src", d).expect("git grep does not skip .claude");
    assert!(why.contains("private discovery"), "{why}");

    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    std::fs::create_dir_all(d.join("src/.Selfware")).unwrap();
    std::fs::write(d.join("src/lib.rs"), "fn lib() {}\n").unwrap();
    std::fs::write(d.join("src/.Selfware/notes"), "state\n").unwrap();
    let why = violation("grep -r fn src", d).expect("nested .Selfware must be refused");
    assert!(why.contains("private discovery"), "{why}");
}

#[test]
fn git_grep_allows_a_repo_git_dir_that_grep_recursive_refuses() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    std::fs::create_dir_all(d.join("src")).unwrap();
    std::fs::create_dir_all(d.join(".git")).unwrap();
    std::fs::write(d.join(".git/.env"), "GIT_SECRET=1\n").unwrap();
    std::fs::write(d.join("src/main.rs"), "fn main() {}\n").unwrap();

    let why = violation("grep -r fn .", d).expect("grep -r reads .git");
    assert!(why.contains("private discovery"), "{why}");
    assert_eq!(violation("git grep fn", d), None);
    assert_eq!(violation("git grep fn .", d), None);
    assert_eq!(
        violation("rg fn", d),
        None,
        "rg default skips dot directories"
    );
    assert!(violation("rg -uu fn", d).is_some());
}

#[cfg(unix)]
#[test]
fn private_discovery_symlinks_are_refused_without_relying_on_is_dir() {
    use std::os::unix::fs::symlink;

    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    let private = d.join("state/.codex");
    std::fs::create_dir_all(&private).unwrap();
    std::fs::write(private.join("secret"), "transcript\n").unwrap();
    let public = d.join("public");
    std::fs::create_dir_all(&public).unwrap();
    std::fs::write(public.join("readme.txt"), "ok\n").unwrap();

    // Link name is not private; the canonical target is. `is_dir()` on the
    // unfollowed link is false.
    symlink(&private, d.join("alias")).unwrap();
    let why = violation("grep -r KEY alias", d).expect("alias target");
    assert!(why.contains("private discovery"), "{why}");

    // Link name is private even when the target is an ordinary file.
    symlink(public.join("readme.txt"), d.join(".claude")).unwrap();
    let why = violation("grep -r KEY .claude", d).expect("private link name");
    assert!(why.contains("private discovery"), "{why}");

    std::fs::create_dir_all(d.join("src")).unwrap();
    std::fs::write(d.join("src/main.rs"), "fn main() {}\n").unwrap();
    symlink(public.join("readme.txt"), d.join("src/.claude")).unwrap();
    let why = violation("grep -r fn src", d).expect("nested private symlink name");
    assert!(why.contains("private discovery"), "{why}");

    let outside = tempfile::tempdir().unwrap();
    let gitdir = outside.path().join("real-git");
    std::fs::create_dir(&gitdir).unwrap();
    std::fs::write(gitdir.join(".env"), "SECRET\n").unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    symlink(&gitdir, d.join(".git")).unwrap();
    std::fs::create_dir(d.join("src")).unwrap();
    std::fs::write(d.join("src/main.rs"), "fn main() {}\n").unwrap();
    assert_eq!(
        violation("git grep fn", d),
        None,
        "a .git symlink is still skipped by git grep"
    );
    let why = violation("grep -r fn .", d).expect("grep -r sees the .git symlink");
    assert!(why.contains("private discovery"), "{why}");
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

#[test]
fn pattern_and_flag_values_matching_existing_files_still_vet_dot() {
    let tmp = fixture();
    let d = tmp.path();
    std::fs::write(d.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(d.join(".env"), d.join("leak.txt")).unwrap();

    for cmd in [
        "rg Cargo.toml",
        "grep -r Cargo.toml",
        "rg -e Cargo.toml",
        "grep -r -f Cargo.toml pat",
        "git grep Cargo.toml",
        "git grep -e Cargo.toml",
    ] {
        assert!(
            violation(cmd, d).is_some(),
            "`{cmd}` must vet `.` even when `Cargo.toml` exists on disk"
        );
        assert!(
            strict_violation(cmd, d).is_some(),
            "`{cmd}` (strict) must vet `.` even when `Cargo.toml` exists on disk"
        );
    }
}

#[test]
fn ripgrep_ignore_files_and_overrides_disable_gitignore_pruning() {
    let tmp = fixture();
    let d = tmp.path();
    std::fs::create_dir(d.join(".git")).unwrap();
    std::fs::write(d.join(".gitignore"), "/secrets/\n").unwrap();
    assert_eq!(violation("rg KEY", d), None);

    for cmd in [
        "rg --ignore-file custom.ignore KEY",
        "rg --ignore-file-case-insensitive KEY",
        "rg --pre cat KEY",
        "rg --pre-glob '*.txt' KEY",
    ] {
        assert!(
            violation(cmd, d).is_some(),
            "`{cmd}` must widen ripgrep walk"
        );
    }

    std::fs::write(d.join(".ignore"), "!secrets/\n").unwrap();
    assert!(
        violation("rg KEY", d).is_some(),
        ".ignore negation must disable .gitignore directory pruning"
    );
    std::fs::remove_file(d.join(".ignore")).unwrap();

    std::fs::write(d.join(".rgignore"), "!secrets/\n").unwrap();
    assert!(
        violation("rg KEY", d).is_some(),
        ".rgignore negation must disable .gitignore directory pruning"
    );
}
