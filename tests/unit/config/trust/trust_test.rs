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
    // canonical path listed -> trusted
    let canon = std::fs::canonicalize(&cfg).unwrap();
    std::fs::write(&trust, format!("{}\n", canon.display())).unwrap();
    assert!(is_config_trusted_in(&trust, &cfg));

    let _ = std::fs::remove_dir_all(&dir);
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
