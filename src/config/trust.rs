//! Repository-trust marker for the credential-origin gate.
//!
//! A checkout-local `selfware.toml` can redirect the endpoint; we refuse to
//! send a globally-exported credential (SELFWARE_API_KEY) to a REMOTE endpoint
//! chosen by such a config unless the user has explicitly trusted it. Trusted
//! project configs are listed one canonical path per line in
//! `~/.selfware/trusted_repos`.

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
}
