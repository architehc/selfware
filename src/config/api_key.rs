//! API key resolution and endpoint validation.

use anyhow::Result;

/// Where the API key was resolved from (used internally for diagnostics and
/// to decide whether a plaintext-config-file warning is appropriate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKeySource {
    /// No key found yet.
    None,
    /// Loaded from the `SELFWARE_API_KEY` environment variable.
    EnvVar,
    /// Loaded from the OS system keyring.
    Keyring,
    /// Loaded from a plaintext TOML config file on disk.
    ConfigFile,
}

/// Service name used when storing the API key in the OS keyring.
pub(crate) const KEYRING_SERVICE: &str = "selfware-api-key";

/// Derive the keyring account name that scopes a stored API key to a
/// specific endpoint HOST. Keys are stored per-endpoint so that a checkout
/// which selects a different (e.g. attacker-controlled) endpoint cannot
/// retrieve a credential saved for another endpoint. Scheme, case, port-less
/// path, and trailing slashes are normalized so trivially different
/// spellings of the same endpoint share one entry; the port IS part of the
/// scope.
pub(crate) fn keyring_account_for_endpoint(endpoint: &str) -> String {
    let no_scheme = endpoint
        .strip_prefix("https://")
        .or_else(|| endpoint.strip_prefix("http://"))
        .unwrap_or(endpoint);
    // host[:port] only — drop any path so `/api/v1` vs `/` share one entry.
    let host_port = no_scheme.split('/').next().unwrap_or(no_scheme);
    host_port.trim().to_ascii_lowercase()
}

/// Load the API key from the OS system keyring.
///
/// Returns `Ok(Some(key))` when a key is stored, `Ok(None)` when
/// the keyring has no entry, or `Err` on a keyring backend failure.
pub fn load_api_key_from_keyring(endpoint: &str) -> Result<Option<String>> {
    let account = keyring_account_for_endpoint(endpoint);
    let entry = keyring::Entry::new(KEYRING_SERVICE, &account)
        .map_err(|e| anyhow::anyhow!("Keyring error: {}", e))?;
    match entry.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow::anyhow!("Keyring error: {}", e)),
    }
}

/// Save an API key to the OS system keyring.
///
/// This is the backing implementation for storing the API key in the OS keyring.
pub fn save_api_key_to_keyring(endpoint: &str, api_key: &str) -> Result<()> {
    let account = keyring_account_for_endpoint(endpoint);
    let entry = keyring::Entry::new(KEYRING_SERVICE, &account)
        .map_err(|e| anyhow::anyhow!("Keyring error: {}", e))?;
    entry
        .set_password(api_key)
        .map_err(|e| anyhow::anyhow!("Keyring error: {}", e))?;
    Ok(())
}

/// Check whether an endpoint URL points to a local address.
/// Local addresses include localhost, 127.0.0.1, `[::1]`, and 0.0.0.0.
/// These are safe to use over plain HTTP since traffic stays on the machine.
pub fn is_local_endpoint(endpoint: &str) -> bool {
    // Extract host portion from the URL (after scheme, before port/path)
    let after_scheme = if let Some(rest) = endpoint.strip_prefix("https://") {
        rest
    } else if let Some(rest) = endpoint.strip_prefix("http://") {
        rest
    } else {
        return false;
    };

    // Handle bracketed IPv6 addresses like [::1]:8000/v1
    if after_scheme.starts_with('[') {
        // Extract the bracketed host (e.g., "[::1]")
        if let Some(bracket_end) = after_scheme.find(']') {
            let bracketed_host = &after_scheme[..=bracket_end];
            return bracketed_host == "[::1]";
        }
        return false;
    }

    // Get host (before port or path) for non-IPv6
    let host = after_scheme
        .split(':')
        .next()
        .unwrap_or(after_scheme)
        .split('/')
        .next()
        .unwrap_or(after_scheme);

    matches!(host, "localhost" | "127.0.0.1" | "0.0.0.0")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyring_account_is_endpoint_scoped() {
        let real = keyring_account_for_endpoint("https://openrouter.ai/api/v1");
        let attacker = keyring_account_for_endpoint("https://attacker.example.com/v1");
        assert_ne!(
            real, attacker,
            "different hosts must not share a keyring account"
        );
        assert_eq!(real, "openrouter.ai");
        // case / scheme / path / trailing slash normalize to the same account
        assert_eq!(
            keyring_account_for_endpoint("https://OpenRouter.ai/api/v1"),
            keyring_account_for_endpoint("http://openrouter.ai/")
        );
        // port is part of the scope
        assert_eq!(
            keyring_account_for_endpoint("http://127.0.0.1:1234/v1"),
            "127.0.0.1:1234"
        );
        assert_ne!(
            keyring_account_for_endpoint("http://127.0.0.1:1234/v1"),
            keyring_account_for_endpoint("http://127.0.0.1:9999/v1")
        );
    }
}
