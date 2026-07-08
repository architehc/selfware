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

/// Load the API key from the OS system keyring.
///
/// Returns `Ok(Some(key))` when a key is stored, `Ok(None)` when
/// the keyring has no entry, or `Err` on a keyring backend failure.
pub fn load_api_key_from_keyring() -> Result<Option<String>> {
    let user = whoami::username().unwrap_or_else(|_| "selfware_user".to_string());
    let entry = keyring::Entry::new(KEYRING_SERVICE, &user)
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
pub fn save_api_key_to_keyring(api_key: &str) -> Result<()> {
    let user = whoami::username().unwrap_or_else(|_| "selfware_user".to_string());
    let entry = keyring::Entry::new(KEYRING_SERVICE, &user)
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
