use super::*;

#[test]
fn open_creates_the_scratch_tree_and_writes_into_it() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = ScratchDir::open(tmp.path(), &["tool_results"], true).unwrap();
    assert_eq!(
        dir.path(),
        tmp.path().join(".selfware").join("tool_results")
    );

    dir.write("a.json", b"one").unwrap();
    dir.write("a.json", b"two").unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.json")).unwrap(),
        "two"
    );

    assert!(dir.write_new("b.txt", b"first").unwrap());
    assert!(!dir.write_new("b.txt", b"second").unwrap());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("b.txt")).unwrap(),
        "first"
    );

    dir.append("log.jsonl", b"1\n").unwrap();
    dir.append("log.jsonl", b"2\n").unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("log.jsonl")).unwrap(),
        "1\n2\n"
    );

    assert!(dir.exists("a.json"));
    assert!(!dir.exists("missing"));
}

#[test]
fn open_without_create_reports_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let err = ScratchDir::open(tmp.path(), &["turns"], false).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    assert!(!tmp.path().join(".selfware").exists());
}

#[test]
fn names_must_be_single_components() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(ScratchDir::open(tmp.path(), &["a/b"], true).is_err());
    assert!(ScratchDir::open(tmp.path(), &[".."], true).is_err());
    let dir = ScratchDir::open(tmp.path(), &[], true).unwrap();
    assert!(dir.write("../escape", b"x").is_err());
    assert!(dir.write("", b"x").is_err());
    assert!(!tmp.path().join("escape").exists());
}

/// A hostile checkout: `.selfware/tool_results` points outside the
/// workspace, `.gitignore` dangles, and `turns/evidence.jsonl` points at an
/// existing file the user owns.
#[cfg(unix)]
#[test]
fn symlinked_components_and_leaves_are_refused() {
    use std::os::unix::fs::symlink;
    let tmp = tempfile::tempdir().unwrap();
    let ws = tmp.path().join("ws");
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(ws.join(".selfware").join("turns")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    symlink(&outside, ws.join(".selfware").join("tool_results")).unwrap();
    symlink(
        tmp.path().join("planted"),
        ws.join(".selfware").join(".gitignore"),
    )
    .unwrap();
    let victim = tmp.path().join("victim.conf");
    std::fs::write(&victim, "[user]\n").unwrap();
    symlink(
        &victim,
        ws.join(".selfware").join("turns").join("evidence.jsonl"),
    )
    .unwrap();

    // A symlinked directory component is refused, with or without `create`.
    assert!(ScratchDir::open(&ws, &["tool_results"], true).is_err());
    assert!(ScratchDir::open(&ws, &["tool_results"], false).is_err());
    assert!(std::fs::read_dir(&outside).unwrap().next().is_none());

    // A symlinked leaf is refused too, for every mode.
    let root = ScratchDir::open(&ws, &[], false).unwrap();
    assert!(root.exists(".gitignore"), "a dangling link is a taken slot");
    assert!(
        !root.write_new(".gitignore", b"*\n").unwrap(),
        "O_EXCL: the link is 'there'"
    );
    assert!(root.write(".gitignore", b"*\n").is_err());
    assert!(!tmp.path().join("planted").exists());

    let turns = ScratchDir::open(&ws, &["turns"], true).unwrap();
    assert!(turns.append("evidence.jsonl", b"{}\n").is_err());
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "[user]\n");
}

/// `.selfware` itself shipped as a symlink is refused outright.
#[cfg(unix)]
#[test]
fn symlinked_selfware_dir_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let ws = tmp.path().join("ws");
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&ws).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, ws.join(".selfware")).unwrap();

    assert!(ScratchDir::open(&ws, &["turns"], true).is_err());
    assert!(std::fs::read_dir(&outside).unwrap().next().is_none());
}

/// The workspace root itself may be reached through a symlink (macOS
/// `/tmp -> /private/tmp`); only components below it are pinned.
#[cfg(unix)]
#[test]
fn symlinked_workspace_root_is_allowed() {
    let tmp = tempfile::tempdir().unwrap();
    let real = tmp.path().join("real");
    std::fs::create_dir_all(&real).unwrap();
    let link = tmp.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let dir = ScratchDir::open(&link, &["turns"], true).unwrap();
    dir.write("turn_0001.json", b"{}").unwrap();
    assert!(real
        .join(".selfware")
        .join("turns")
        .join("turn_0001.json")
        .is_file());
}
