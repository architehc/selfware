//! Repository-trust marker for the credential-origin gate.
//!
//! A checkout-local `selfware.toml` can redirect the endpoint; we refuse to
//! send a globally-exported credential (SELFWARE_API_KEY) to a REMOTE endpoint
//! chosen by such a config unless the user has explicitly trusted it. Trusted
//! project configs are listed one per line in `~/.selfware/trusted_repos` as
//! `<sha256 of the reviewed content>  <canonical path>` (sha256sum format).

use anyhow::Result;
use sha2::{Digest, Sha256};
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

/// Hex SHA-256 of the file's current bytes; `None` when it cannot be read.
fn content_digest(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(hex::encode(Sha256::digest(&bytes)))
}

/// Split a trust line into `(digest, path)`. The digest has no spaces, so
/// the first double space is the separator; a bare-path line written by an
/// older build has no digest and yields `(None, path)`.
fn parse_trust_line(line: &str) -> (Option<&str>, &str) {
    match line.split_once("  ") {
        Some((digest, path)) => (Some(digest), path),
        None => (None, line),
    }
}

/// Whether `config_path` is listed in the default trusted-repos file.
pub fn is_config_trusted(config_path: &Path) -> bool {
    match trusted_repos_file() {
        Some(f) => is_config_trusted_in(&f, config_path),
        None => false,
    }
}

/// Testable core: whether `config_path` is listed in `trust_file`.
///
/// Trust is an EXACT canonical file match on the CONTENT the user reviewed.
/// A trust line names the one config file the user reviewed and the digest
/// of its bytes at that time; it never extends to other files in the same
/// directory, to a directory line, or to the same path once its content has
/// changed. Otherwise a `selfware.toml` that appears in that directory later
/// or is rewritten in place (a pulled commit, a branch switch, an unpacked
/// archive) would inherit trust the user never granted and could run its
/// hooks or route their API key to its endpoint. Legacy directory lines and
/// bare-path lines written by older builds therefore stop matching; the
/// credential-origin gate's refusal names the exact file to trust, so
/// re-trusting is one command.
fn is_config_trusted_in(trust_file: &Path, config_path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(trust_file) else {
        return false;
    };
    let target = canonical(config_path);
    // A directory is never a trusted config, even when listed verbatim.
    if target.is_dir() {
        return false;
    }
    let Some(digest) = content_digest(&target) else {
        return false;
    };
    content.lines().any(|line| {
        let line = line.trim();
        if line.is_empty() {
            return false;
        }
        let (recorded, path) = parse_trust_line(line);
        let entry = Path::new(path);
        // Trust entries are canonical absolute paths. Ignore relative entries so
        // a corrupted or newline-injected `selfware.toml` line cannot resolve
        // against whatever directory the process happens to be running in.
        recorded == Some(digest.as_str()) && entry.is_absolute() && canonical(entry) == target
    })
}

/// Add `config_path` (canonicalized) to the default trusted-repos file.
pub fn add_trusted_config(config_path: &Path) -> Result<()> {
    let file =
        trusted_repos_file().ok_or_else(|| anyhow::anyhow!("cannot resolve home directory"))?;
    add_trusted_config_to(&file, config_path)
}

/// Testable core: record the canonical `config_path` and the digest of its
/// current content in `trust_file` (creating it 0600 / its parent 0700 on
/// Unix). Idempotent — a path already trusted with this content is a no-op;
/// a line recorded for other content of the same path is replaced, so only
/// the content reviewed last stays trusted.
fn add_trusted_config_to(trust_file: &Path, config_path: &Path) -> Result<()> {
    // Directory trust does not exist (see `is_config_trusted_in`): refuse to
    // write a line that can never match rather than report "Trusted".
    if config_path.is_dir() {
        anyhow::bail!(
            "refusing to trust directory '{}': trust names one config file (e.g. {})",
            config_path.display(),
            config_path.join("selfware.toml").display()
        );
    }
    let target = canonical(config_path);
    let target_str = target.to_string_lossy();
    // `trusted_repos` is a newline-delimited list of absolute canonical paths.
    // A path containing `\n` or `\r` (for example inside a directory named
    // `repo\nselfware.toml`) would split into a second line — and if that tail
    // line is the relative `selfware.toml`, `canonical("selfware.toml")` would
    // match `./selfware.toml` in EVERY working directory.
    if !target.is_absolute() || target_str.contains(['\n', '\r']) {
        anyhow::bail!(
            "refusing to trust '{}': canonical path must be single-line and absolute",
            config_path.display()
        );
    }
    if is_config_trusted_in(trust_file, config_path) {
        return Ok(());
    }
    let digest = content_digest(&target).ok_or_else(|| {
        anyhow::anyhow!(
            "cannot read '{}' to record what was trusted",
            target.display()
        )
    })?;
    if let Some(parent) = trust_file.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
        }
    }
    // Drop any earlier line for this path (stale digest or legacy bare path):
    // trust follows the content reviewed now, not every content ever reviewed.
    let mut existing: String = std::fs::read_to_string(trust_file)
        .unwrap_or_default()
        .lines()
        .filter(|line| {
            let line = line.trim();
            !line.is_empty() && canonical(Path::new(parse_trust_line(line).1)) != target
        })
        .map(|line| format!("{line}\n"))
        .collect();
    existing.push_str(&format!("{digest}  {target_str}\n"));
    std::fs::write(trust_file, existing)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(trust_file, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/config/trust/trust_test.rs"]
mod tests;
