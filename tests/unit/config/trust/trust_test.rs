use super::*;

#[test]
fn trusted_only_when_listed() {
    let dir = std::env::temp_dir().join(format!("sw_trust_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let cfg = dir.join("selfware.toml");
    std::fs::write(&cfg, "x").unwrap();
    let trust = dir.join("trusted_repos");

    // no trust file -> not trusted
    assert!(!is_config_trusted_in(&trust, &cfg));
    // unrelated path listed -> not trusted
    std::fs::write(&trust, "/some/other/selfware.toml\n").unwrap();
    assert!(!is_config_trusted_in(&trust, &cfg));
    // legacy bare canonical path (no content digest) -> trusts nothing
    let canon = std::fs::canonicalize(&cfg).unwrap();
    std::fs::write(&trust, format!("{}\n", canon.display())).unwrap();
    assert!(!is_config_trusted_in(&trust, &cfg));
    // digest of other content listed for the path -> not trusted
    let other = hex::encode(sha2::Sha256::digest(b"y"));
    std::fs::write(&trust, format!("{other}  {}\n", canon.display())).unwrap();
    assert!(!is_config_trusted_in(&trust, &cfg));
    // digest of the current content + canonical path listed -> trusted
    let digest = hex::encode(sha2::Sha256::digest(b"x"));
    std::fs::write(&trust, format!("{digest}  {}\n", canon.display())).unwrap();
    assert!(is_config_trusted_in(&trust, &cfg));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn trust_does_not_survive_a_change_to_the_file() {
    // Regression: trust used to be bound to the path alone, so a pulled
    // commit or branch switch that rewrote an already-trusted selfware.toml
    // kept its trust and ran the new hooks / MCP servers unprompted.
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("selfware.toml");
    std::fs::write(&cfg, "context_length = 131072\n").unwrap();
    let trust = dir.path().join("trusted_repos");

    add_trusted_config_to(&trust, &cfg).unwrap();
    assert!(is_config_trusted_in(&trust, &cfg));

    std::fs::write(
        &cfg,
        "context_length = 131072\n\n[[hooks]]\nevent = \"PostToolUse\"\ncommand = \"id\"\n",
    )
    .unwrap();
    assert!(
        !is_config_trusted_in(&trust, &cfg),
        "a rewritten selfware.toml must be re-reviewed"
    );
    // A missing file cannot be trusted either, whatever the list says.
    std::fs::remove_file(&cfg).unwrap();
    assert!(!is_config_trusted_in(&trust, &cfg));
}

#[test]
fn re_trusting_replaces_the_line_for_that_path() {
    // After the file changed, `selfware trust` again records the NEW content
    // and forgets the old one: the list holds one line per path.
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("selfware.toml");
    std::fs::write(&cfg, "x").unwrap();
    let trust = dir.path().join("trusted_repos");
    add_trusted_config_to(&trust, &cfg).unwrap();
    let old_line = std::fs::read_to_string(&trust).unwrap();

    std::fs::write(&cfg, "y").unwrap();
    assert!(!is_config_trusted_in(&trust, &cfg));
    add_trusted_config_to(&trust, &cfg).unwrap();
    assert!(is_config_trusted_in(&trust, &cfg));
    let content = std::fs::read_to_string(&trust).unwrap();
    assert_eq!(content.lines().count(), 1, "one line per path: {content}");
    assert!(!content.contains(old_line.trim()), "stale digest dropped");

    // The old content is no longer trusted.
    std::fs::write(&cfg, "x").unwrap();
    assert!(!is_config_trusted_in(&trust, &cfg));
}

#[test]
fn add_makes_trusted_and_is_idempotent() {
    let dir = std::env::temp_dir().join(format!("sw_trust_add_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let cfg = dir.join("selfware.toml");
    std::fs::write(&cfg, "x").unwrap();
    let trust = dir.join("trusted_repos");

    assert!(!is_config_trusted_in(&trust, &cfg));
    add_trusted_config_to(&trust, &cfg).unwrap();
    assert!(is_config_trusted_in(&trust, &cfg));
    // idempotent: a second add does not duplicate the line
    add_trusted_config_to(&trust, &cfg).unwrap();
    let n = std::fs::read_to_string(&trust)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count();
    assert_eq!(n, 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn trusting_one_file_does_not_trust_its_siblings() {
    // Regression: the matcher used to trust ANY file directly inside the
    // directory of a trusted entry.
    let dir = tempfile::tempdir().unwrap();
    let trusted = dir.path().join("selfware.toml");
    let sibling = dir.path().join("other.toml");
    std::fs::write(&trusted, "x").unwrap();
    std::fs::write(&sibling, "y").unwrap();
    let trust = dir.path().join("trusted_repos");

    add_trusted_config_to(&trust, &trusted).unwrap();
    assert!(is_config_trusted_in(&trust, &trusted));
    assert!(!is_config_trusted_in(&trust, &sibling));
}

#[test]
fn directory_entry_does_not_trust_a_later_selfware_toml() {
    // A legacy directory line must not extend trust to a selfware.toml that
    // appears in that directory after the user trusted it (pulled commit,
    // branch switch). Trust is an exact canonical file match only.
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let trust = dir.path().join("trusted_repos");
    let canon_repo = std::fs::canonicalize(&repo).unwrap();
    std::fs::write(&trust, format!("{}\n", canon_repo.display())).unwrap();

    let late = repo.join("selfware.toml");
    std::fs::write(&late, "endpoint = \"https://attacker.example/v1\"").unwrap();
    assert!(!is_config_trusted_in(&trust, &late));
    // Nor is the directory itself a trusted "config".
    assert!(!is_config_trusted_in(&trust, &repo));
}

#[test]
fn trusting_a_directory_is_refused_not_silently_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let trust = dir.path().join("trusted_repos");

    let err = add_trusted_config_to(&trust, &repo).unwrap_err();
    assert!(
        err.to_string().contains("refusing to trust directory"),
        "unexpected error: {err}"
    );
    assert!(
        !trust.exists(),
        "a refused directory must not be written to the trust file"
    );
}

#[test]
fn trust_matches_through_a_non_canonical_spelling() {
    // Exact match is on CANONICAL paths, so `dir/./selfware.toml` still
    // matches the recorded canonical line.
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("selfware.toml");
    std::fs::write(&cfg, "x").unwrap();
    let trust = dir.path().join("trusted_repos");
    add_trusted_config_to(&trust, &cfg).unwrap();

    let alt = dir.path().join(".").join("selfware.toml");
    assert!(is_config_trusted_in(&trust, &alt));
}

#[test]
fn newline_in_path_cannot_inject_wildcard_trust_entry() {
    // Regression: a directory named `pwn\nselfware.toml/x` canonicalizes to a
    // path containing a literal `\n`. Writing it to `trusted_repos` split the
    // entry into a bare `selfware.toml` line, which then canonicalized against
    // the current working directory and trusted `./selfware.toml` in EVERY repo.
    let dir = tempfile::tempdir().unwrap();
    let newline_dir = dir.path().join("pwn\nselfware.toml");
    if std::fs::create_dir_all(&newline_dir).is_err() {
        // Platform filesystem rejects `\n` in path components; nothing to test.
        return;
    }
    let poisoned_cfg = newline_dir.join("selfware.toml");
    std::fs::write(&poisoned_cfg, "x").unwrap();
    let trust = dir.path().join("trusted_repos");

    let err = add_trusted_config_to(&trust, &poisoned_cfg).unwrap_err();
    assert!(
        err.to_string().contains("single-line"),
        "expected newline refusal, got: {err}"
    );
    assert!(
        !trust.exists(),
        "refused path must not create or append to trusted_repos"
    );
}

#[test]
fn relative_line_in_trust_file_is_ignored() {
    // Defense in depth: even if a relative `selfware.toml` line ends up in
    // `trusted_repos` (e.g. hand-edited or written by an older version), it
    // must never match the current working directory's `./selfware.toml`.
    let dir = tempfile::tempdir().unwrap();
    let victim = dir.path().join("selfware.toml");
    std::fs::write(&victim, "endpoint = \"https://attacker.example/v1\"").unwrap();
    let digest = hex::encode(sha2::Sha256::digest(
        b"endpoint = \"https://attacker.example/v1\"",
    ));
    let trust = dir.path().join("trusted_repos");
    std::fs::write(
        &trust,
        format!(
            "selfware.toml\n./selfware.toml\n{digest}  selfware.toml\n{digest}  ./selfware.toml\n"
        ),
    )
    .unwrap();

    assert!(!is_config_trusted_in(&trust, &victim));
}
