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
    // Parse the scheme rather than string-matching "http://": `url::Url`
    // normalizes the scheme to lowercase, so `HTTP://attacker/...` (which reqwest
    // also normalizes and sends in the clear) is still caught. A raw
    // `starts_with("http://")` was case-sensitive and bypassable.
    match url::Url::parse(endpoint) {
        Ok(url) => url.scheme() == "http" && !is_local_endpoint(endpoint),
        // Unparseable: fall back to a case-insensitive prefix check so an
        // uppercase/mixed-case scheme still can't slip through.
        Err(_) => {
            endpoint
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("http://")
                && !is_local_endpoint(endpoint)
        }
    }
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

/// The single authenticated-request choke point: verify `endpoint` is safe to
/// receive the credential (plaintext-HTTP-to-remote / embedded-userinfo refusal
/// via [`assert_credential_endpoint_safe`]) and then attach `Authorization:
/// Bearer` when a key is present. Every bearer path should build its request
/// through this so the transport/origin check cannot be silently skipped.
pub fn authorize_request(
    req: reqwest::RequestBuilder,
    endpoint: &str,
    api_key: Option<&str>,
) -> Result<reqwest::RequestBuilder> {
    assert_credential_endpoint_safe(endpoint, api_key.is_some())?;
    Ok(match api_key {
        Some(key) => req.bearer_auth(key),
        None => req,
    })
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

/// Whether the endpoint's URL host is exactly `openrouter.ai`. Uses proper URL
/// parsing (never substring matching) so lookalike hosts such as
/// `openrouter.ai.evil.com` do NOT match — this gates which endpoints may
/// receive the `OPENROUTER_API_KEY` credential.
pub fn is_openrouter_endpoint(endpoint: &str) -> bool {
    url::Url::parse(endpoint)
        .map(|u| {
            u.host_str()
                .map(|h| h.eq_ignore_ascii_case("openrouter.ai"))
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

/// Store an API key in the OS keyring for the endpoint of the active
/// (already-loaded) configuration.
///
/// This is the backing implementation for `selfware config set-key <KEY>`:
/// it makes the keyring path (previously reachable only via the `init`
/// wizard) available as a one-shot command. The key is scoped to
/// `config.endpoint` via `keyring_account_for_endpoint`, which is exactly
/// the account `Config::load` probes — so the next run picks the key up
/// without any plaintext `api_key` on disk. Returns the endpoint the key
/// was stored for, so the caller can tell the user where it applies.
pub fn set_api_key_for_endpoint(config: &super::Config, api_key: &str) -> Result<String> {
    let trimmed = api_key.trim();
    anyhow::ensure!(!trimmed.is_empty(), "API key must not be empty");
    save_api_key_to_keyring(&config.endpoint, trimmed)?;
    // Read-back verification: on some macOS environments (notably non-GUI
    // sessions) the keychain backend accepts the write but the item is not
    // retrievable afterwards. Report that honestly instead of claiming success.
    match load_api_key_from_keyring(&config.endpoint) {
        Ok(Some(_)) => Ok(config.endpoint.clone()),
        Ok(None) => anyhow::bail!(
            "keychain write did not persist (backend returned no entry on read-back). \
             Use `api_key` in selfware.toml (chmod 600) or SELFWARE_API_KEY instead."
        ),
        Err(e) => anyhow::bail!(
            "keychain read-back failed ({e}). \
             Use `api_key` in selfware.toml (chmod 600) or SELFWARE_API_KEY instead."
        ),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/config/api_key/api_key_test.rs"]
mod tests;
