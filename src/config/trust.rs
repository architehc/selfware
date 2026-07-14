//! Repository-trust marker for the credential-origin gate.
//!
//! A checkout-local `selfware.toml` can redirect the endpoint; we refuse to
//! send a globally-exported credential (SELFWARE_API_KEY) to a REMOTE endpoint
//! chosen by such a config unless the user has explicitly trusted it. Trusted
//! project configs are listed one canonical path per line in
//! `~/.selfware/trusted_repos`.

use anyhow::Result;
use std::path::{Path, PathBuf};

/// Path to the trusted-repos file (`~/.selfware/trusted_repos`).
pub fn trusted_repos_file() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".selfware").join("trusted_repos"))
}

/// Canonicalize a path for stable comparison; falls back to the path as-given
/// when it cannot be canonicalized.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Whether `config_path` is listed in the default trusted-repos file.
pub fn is_config_trusted(config_path: &Path) -> bool {
    match trusted_repos_file() {
        Some(f) => is_config_trusted_in(&f, config_path),
        None => false,
    }
}

/// Testable core: whether `config_path` is listed in `trust_file`.
fn is_config_trusted_in(trust_file: &Path, config_path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(trust_file) else {
        return false;
    };
    let target = canonical(config_path);
    content.lines().any(|line| {
        let line = line.trim();
        !line.is_empty() && canonical(Path::new(line)) == target
    })
}

/// Add `config_path` (canonicalized) to the default trusted-repos file.
pub fn add_trusted_config(config_path: &Path) -> Result<()> {
    let file =
        trusted_repos_file().ok_or_else(|| anyhow::anyhow!("cannot resolve home directory"))?;
    add_trusted_config_to(&file, config_path)
}

/// Testable core: append the canonical `config_path` to `trust_file` (creating
/// it 0600 / its parent 0700 on Unix). Idempotent — a path already trusted is a
/// no-op.
fn add_trusted_config_to(trust_file: &Path, config_path: &Path) -> Result<()> {
    if is_config_trusted_in(trust_file, config_path) {
        return Ok(());
    }
    if let Some(parent) = trust_file.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
        }
    }
    let mut existing = std::fs::read_to_string(trust_file).unwrap_or_default();
    if !existing.is_empty() && !existing.ends_with('\n') {
        existing.push('\n');
    }
    existing.push_str(&canonical(config_path).to_string_lossy());
    existing.push('\n');
    std::fs::write(trust_file, existing)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(trust_file, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
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
}
