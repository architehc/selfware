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
