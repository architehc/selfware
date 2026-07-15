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
/// specific endpoint by (scheme, host, effective port).  Keys are stored
/// per-endpoint so that a checkout which selects a different (e.g.
/// attacker-controlled) endpoint cannot retrieve a credential saved for
/// another endpoint.  The scheme is kept so that an `http://` URL cannot
/// retrieve a key stored for the same host over `https://`.
pub(crate) fn keyring_account_for_endpoint(endpoint: &str) -> String {
    if let Ok(url) = url::Url::parse(endpoint) {
        let scheme = url.scheme().to_ascii_lowercase();
        let host = url.host_str().unwrap_or("").to_ascii_lowercase();
        // Effective port: explicit port, else the scheme's default (443/80).
        let port = url.port_or_known_default().unwrap_or(0);
        format!("{scheme}://{host}:{port}")
    } else {
        // Unparseable endpoint: fall back to the raw string, normalized.
        endpoint.trim().to_ascii_lowercase()
    }
}

/// Whether sending a credential to this endpoint would cross plaintext HTTP to
/// a REMOTE host — a downgrade / exfiltration risk, since a checkout-local
/// config can choose the endpoint. Local HTTP (localhost / loopback) is allowed
/// because traffic stays on the machine. Note: `https://...` does NOT start with
/// `http://`, so only real http endpoints match.
pub fn is_insecure_remote_endpoint(endpoint: &str) -> bool {
    endpoint.starts_with("http://") && !is_local_endpoint(endpoint)
}

/// Assert it is safe to send a credential to `endpoint`. Fails when a credential
/// is present AND the endpoint would leak it — plaintext HTTP to a remote host,
/// or a URL embedding userinfo (user:pass@host). The single choke point every
/// authenticated request path should call before sending.
pub fn assert_credential_endpoint_safe(endpoint: &str, has_credential: bool) -> Result<()> {
    if has_credential && (endpoint_has_userinfo(endpoint) || is_insecure_remote_endpoint(endpoint))
    {
        anyhow::bail!(
            "Refusing to send the API key to endpoint '{}': it would go over plaintext HTTP to a \
             remote host or via an embedded-credential URL. Use https:// or a local endpoint \
             (localhost / 127.0.0.1).",
            endpoint
        );
    }
    Ok(())
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

/// Check whether an endpoint URL points to a local address (safe for plain
/// HTTP because traffic stays on the machine). Parses with url::Url and derives
/// locality from the PARSED host, so a host-spoofing userinfo URL such as
/// `http://localhost@attacker.example/` (real host `attacker.example`) is NOT
/// treated as local. Only http/https endpoints are considered.
pub fn is_local_endpoint(endpoint: &str) -> bool {
    let Ok(url) = url::Url::parse(endpoint) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    // Userinfo (user[:pass]@host) is a host-spoof vector — never local.
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    match url.host() {
        Some(url::Host::Domain(h)) => h.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback() || ip.is_unspecified(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

/// Whether the endpoint URL embeds userinfo (`user[:pass]@host`). Such URLs are
/// a host-spoofing vector — `http://localhost@attacker/` parses to host
/// `attacker` while a naive string check sees `localhost`. Callers refuse to
/// send a credential to such an endpoint.
pub fn endpoint_has_userinfo(endpoint: &str) -> bool {
    url::Url::parse(endpoint)
        .map(|u| !u.username().is_empty() || u.password().is_some())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyring_account_is_scheme_host_port_scoped() {
        // scheme + host + effective port form the account
        assert_eq!(
            keyring_account_for_endpoint("https://openrouter.ai/api/v1"),
            "https://openrouter.ai:443"
        );
        // http and https to the SAME host must be DIFFERENT accounts (no downgrade)
        assert_ne!(
            keyring_account_for_endpoint("https://openrouter.ai/api/v1"),
            keyring_account_for_endpoint("http://openrouter.ai/api/v1")
        );
        // different hosts differ
        assert_ne!(
            keyring_account_for_endpoint("https://openrouter.ai/api/v1"),
            keyring_account_for_endpoint("https://attacker.example.com/v1")
        );
        // case / path normalize; explicit default port == implicit default port
        assert_eq!(
            keyring_account_for_endpoint("https://OpenRouter.ai/api/v1"),
            keyring_account_for_endpoint("https://openrouter.ai:443/")
        );
        // explicit non-default port is part of the scope
        assert_eq!(
            keyring_account_for_endpoint("http://127.0.0.1:1234/v1"),
            "http://127.0.0.1:1234"
        );
        assert_ne!(
            keyring_account_for_endpoint("http://127.0.0.1:1234/v1"),
            keyring_account_for_endpoint("http://127.0.0.1:9999/v1")
        );
    }

    #[test]
    fn assert_credential_endpoint_safe_rules() {
        // remote http OR userinfo + key -> Err
        assert!(assert_credential_endpoint_safe("http://api.example.com/v1", true).is_err());
        assert!(
            assert_credential_endpoint_safe("https://user:pass@api.example.com/v1", true).is_err()
        );
        // safe endpoints with key -> Ok
        assert!(assert_credential_endpoint_safe("https://api.example.com/v1", true).is_ok());
        assert!(assert_credential_endpoint_safe("http://127.0.0.1:8000/v1", true).is_ok());
        // no credential -> always Ok (even remote http)
        assert!(assert_credential_endpoint_safe("http://api.example.com/v1", false).is_ok());
    }

    #[test]
    fn insecure_remote_endpoint_detection() {
        assert!(is_insecure_remote_endpoint("http://openrouter.ai/api/v1")); // remote http -> insecure
        assert!(!is_insecure_remote_endpoint("https://openrouter.ai/api/v1")); // https ok
        assert!(!is_insecure_remote_endpoint("http://localhost:1234/v1")); // local ok
        assert!(!is_insecure_remote_endpoint("http://127.0.0.1:8000/v1")); // loopback ok
        assert!(!is_insecure_remote_endpoint("http://[::1]:8000/v1")); // ipv6 loopback ok
    }
}
