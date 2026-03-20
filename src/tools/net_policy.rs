//! Shared network-policy helpers for URL / IP validation.
//!
//! Both `http.rs` (HTTP request tool) and `page_controller.rs` (Playwright
//! tool) need to decide whether a target URL points at a private/internal
//! network address and whether to allow or block it. This module is the
//! single place where that logic lives.

use crate::config::is_local_endpoint;
use crate::safety::is_private_or_internal;
use anyhow::Result;
use std::net::IpAddr;

// ============================================================================
// IP helpers
// ============================================================================

/// Returns `true` when `ip` belongs to a private, internal, link-local,
/// loopback, or otherwise non-public range.
///
/// This is a thin wrapper around [`crate::safety::is_private_or_internal`]
/// so that callers inside `tools/` don't need to reach into the safety crate
/// directly.
pub fn is_private_or_internal_ip(ip: &IpAddr) -> bool {
    is_private_or_internal(*ip)
}

/// Returns `true` when `host` is a known-private hostname (e.g. `localhost`)
/// or parses to a private/internal IP address.
pub fn is_private_network_host(host: &str) -> bool {
    if host == "localhost" || host.ends_with(".localhost") {
        return true;
    }
    let bare_host = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = bare_host.parse::<IpAddr>() {
        return is_private_or_internal_ip(&ip);
    }
    false
}

/// Returns `true` for the handful of loopback/unspecified addresses that are
/// considered "trusted local" (i.e. traffic stays on the machine).
pub(crate) fn is_trusted_local_network_host(host: &str) -> bool {
    let bare_host = host.trim_start_matches('[').trim_end_matches(']');
    matches!(bare_host, "localhost" | "127.0.0.1" | "::1" | "0.0.0.0")
        || bare_host.ends_with(".localhost")
}

// ============================================================================
// Policy struct + validation
// ============================================================================

/// Describes what kind of network targets are permitted for a request.
#[derive(Debug, Clone, Copy)]
pub struct NetworkPolicy {
    pub allow_localhost: bool,
    pub allow_private: bool,
}

/// Validate a parsed URL against the network policy rules that are shared
/// between the HTTP-request tool and the page-controller tool.
///
/// * Only `http` and `https` schemes are accepted (callers that also support
///   `file://` should handle that separately before calling this function).
/// * Localhost / loopback targets are always allowed (via `is_local_endpoint`
///   or `is_trusted_local_network_host`).
/// * Other private/internal IPs are blocked unless `allow_private` is `true`.
pub fn validate_url_target(url: &url::Url, allow_private: bool) -> Result<NetworkPolicy> {
    if url.scheme() != "http" && url.scheme() != "https" {
        anyhow::bail!("Only HTTP and HTTPS URLs are allowed");
    }

    let allow_localhost = is_local_endpoint(url.as_str())
        || url.host_str().is_some_and(is_trusted_local_network_host);

    if let Some(host) = url.host_str() {
        if let Ok(ip) = host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<IpAddr>()
        {
            if is_private_or_internal_ip(&ip) && !(allow_private || allow_localhost) {
                anyhow::bail!(
                    "Blocked request to private/internal network address: {}. \
                     Set SELFWARE_ALLOW_PRIVATE_NETWORK=1 to allow.",
                    host
                );
            }
        }
    }

    Ok(NetworkPolicy {
        allow_localhost,
        allow_private,
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- is_private_or_internal_ip ----

    #[test]
    fn test_private_ip_v4() {
        assert!(is_private_or_internal_ip(&"127.0.0.1".parse().unwrap()));
        assert!(is_private_or_internal_ip(&"10.0.0.1".parse().unwrap()));
        assert!(is_private_or_internal_ip(&"192.168.1.1".parse().unwrap()));
        assert!(is_private_or_internal_ip(&"172.16.0.1".parse().unwrap()));
        assert!(is_private_or_internal_ip(&"169.254.0.1".parse().unwrap()));
        assert!(is_private_or_internal_ip(&"0.0.0.0".parse().unwrap()));
        assert!(!is_private_or_internal_ip(&"8.8.8.8".parse().unwrap()));
        assert!(!is_private_or_internal_ip(&"1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn test_private_ip_v6() {
        assert!(is_private_or_internal_ip(&"::1".parse().unwrap()));
        assert!(is_private_or_internal_ip(&"::".parse().unwrap()));
        assert!(!is_private_or_internal_ip(
            &"2606:4700::1111".parse().unwrap()
        ));
    }

    // ---- is_private_network_host ----

    #[test]
    fn test_private_network_host_localhost() {
        assert!(is_private_network_host("localhost"));
        assert!(is_private_network_host("foo.localhost"));
    }

    #[test]
    fn test_private_network_host_ip() {
        assert!(is_private_network_host("127.0.0.1"));
        assert!(is_private_network_host("10.0.0.1"));
        assert!(!is_private_network_host("8.8.8.8"));
    }

    #[test]
    fn test_private_network_host_ipv6_bracket() {
        assert!(is_private_network_host("[::1]"));
        assert!(!is_private_network_host("[2606:4700::1111]"));
    }

    #[test]
    fn test_private_network_host_random_hostname() {
        assert!(!is_private_network_host("example.com"));
    }

    // ---- validate_url_target ----

    #[test]
    fn test_validate_url_target_allows_localhost() {
        let url = url::Url::parse("http://localhost:8888/health").unwrap();
        let policy = validate_url_target(&url, false).unwrap();
        assert!(policy.allow_localhost);
        assert!(!policy.allow_private);
    }

    #[test]
    fn test_validate_url_target_blocks_private_lan_without_opt_in() {
        let url = url::Url::parse("http://192.168.1.10:8000/health").unwrap();
        let error = validate_url_target(&url, false).unwrap_err();
        assert!(error
            .to_string()
            .contains("Blocked request to private/internal network address"));
    }

    #[test]
    fn test_validate_url_target_allows_private_with_opt_in() {
        let url = url::Url::parse("http://192.168.1.10:8000/health").unwrap();
        let policy = validate_url_target(&url, true).unwrap();
        assert!(policy.allow_private);
    }

    #[test]
    fn test_validate_url_target_rejects_non_http() {
        let url = url::Url::parse("ftp://example.com/file").unwrap();
        let err = validate_url_target(&url, false).unwrap_err();
        assert!(err.to_string().contains("Only HTTP and HTTPS"));
    }

    #[test]
    fn test_validate_url_target_allows_public() {
        let url = url::Url::parse("https://example.com").unwrap();
        let policy = validate_url_target(&url, false).unwrap();
        assert!(!policy.allow_localhost);
        assert!(!policy.allow_private);
    }
}
