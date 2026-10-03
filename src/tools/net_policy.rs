//! Shared network-policy helpers for URL / IP validation.
//!
//! Both `http.rs` (HTTP request tool) and `page_controller.rs` (Playwright
//! tool) need to decide whether a target URL points at a private/internal
//! network address and whether to allow or block it. This module is the
//! single place where that logic lives.

use crate::config::is_local_endpoint;
use crate::safety::is_private_or_internal;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, RwLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinHandle;

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
    let bare_host = if host.starts_with('[') && host.ends_with(']') {
        let inner = &host[1..host.len() - 1];
        // Basic IPv6 validation: must contain a colon and only valid hex/colon chars
        if !inner.contains(':')
            || !inner
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.')
        {
            return false;
        }
        inner
    } else {
        host
    };
    if let Ok(ip) = bare_host.parse::<IpAddr>() {
        return is_private_or_internal_ip(&ip);
    }
    false
}

/// Returns `true` for the handful of loopback/unspecified addresses that are
/// considered "trusted local" (i.e. traffic stays on the machine).
pub(crate) fn is_trusted_local_network_host(host: &str) -> bool {
    let bare_host = normalize_host(host);
    if bare_host == "localhost" || bare_host.ends_with(".localhost") {
        return true;
    }
    match bare_host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => ip.is_loopback() || ip.is_unspecified(),
        Ok(IpAddr::V6(ip)) => ip.is_loopback() || ip.is_unspecified(),
        Err(_) => false,
    }
}

// ============================================================================
// Policy struct + validation
// ============================================================================

/// Describes what kind of network targets are permitted for a request.
#[derive(Debug, Clone)]
pub struct NetworkPolicy {
    pub allow_localhost: bool,
    pub allow_private: bool,
    local_authority: Option<LocalAuthority>,
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

    let local_authority = if allow_localhost {
        LocalAuthority::from_url(url)
    } else {
        None
    };
    Ok(NetworkPolicy {
        allow_localhost,
        allow_private,
        local_authority,
    })
}

/// Return whether an individual resolved address may be used for `host`.
///
/// `allow_localhost` is deliberately narrower than `allow_private`: it only
/// authorizes loopback-style hostnames/literals to resolve to loopback or
/// unspecified addresses. It must never turn a request that started at
/// localhost into blanket permission for a later, unrelated redirect host.
pub fn resolved_address_allowed(
    host: &str,
    ip: IpAddr,
    allow_private: bool,
    allow_localhost: bool,
) -> bool {
    if !is_private_or_internal_ip(&ip) {
        return true;
    }
    if allow_private {
        return true;
    }
    allow_localhost
        && is_trusted_local_network_host(host)
        && match ip {
            IpAddr::V4(ip) => ip.is_loopback() || ip.is_unspecified(),
            IpAddr::V6(ip) => ip.is_loopback() || ip.is_unspecified(),
        }
}

/// Check a redirect target without inheriting localhost permission from the
/// request that preceded it. Private-network opt-in is request-wide; the
/// localhost exception is evaluated afresh for every hop.
pub fn validate_redirect_target(
    url: &url::Url,
    request_policy: &NetworkPolicy,
) -> Result<NetworkPolicy> {
    let target_policy = validate_url_target(url, request_policy.allow_private)?;
    if target_policy.allow_localhost && !request_policy.allow_private {
        match request_policy.local_authority.as_ref() {
            None => anyhow::bail!("Blocked redirect from public origin to localhost/loopback"),
            Some(authority) if !authority.matches_url(url) => anyhow::bail!(
                "Blocked redirect to localhost/loopback outside the initially authorized authority"
            ),
            Some(_) => {}
        }
    }
    Ok(target_policy)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LocalAuthority {
    host: String,
    port: u16,
}

impl LocalAuthority {
    fn from_url(url: &url::Url) -> Option<Self> {
        let host = url.host_str()?;
        if !is_trusted_local_network_host(host) {
            return None;
        }
        Some(Self {
            host: normalize_host(host),
            port: url.port_or_known_default()?,
        })
    }

    fn matches(&self, host: &str, port: u16) -> bool {
        self.host == normalize_host(host) && self.port == port
    }

    fn matches_url(&self, url: &url::Url) -> bool {
        url.host_str().is_some_and(|host| {
            url.port_or_known_default()
                .is_some_and(|port| self.matches(host, port))
        })
    }
}

fn normalize_host(host: &str) -> String {
    host_for_socket_resolution(host)
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

/// Return a hostname in the form expected by Rust's socket resolvers.
///
/// [`url::Url::host_str`] includes the square brackets from a serialized IPv6
/// literal (for example, `[::1]`). `ToSocketAddrs` implementations that take a
/// separate `(host, port)` tuple expect the bare address instead. Keeping this
/// conversion at the resolver boundary also preserves the bracketed spelling
/// in policy errors and audit logs.
pub(crate) fn host_for_socket_resolution(host: &str) -> &str {
    host.strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host)
}

/// A loopback HTTP CONNECT proxy that enforces the private-network policy at
/// connection time. Browser engines otherwise perform their own DNS lookup,
/// after Rust's initial URL check, which leaves redirects, subresources and
/// DNS rebinding outside the trust boundary.
pub(crate) struct GuardedProxy {
    address: SocketAddr,
    allowed_local: Arc<RwLock<HashSet<LocalAuthority>>>,
    shutdown: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl GuardedProxy {
    /// Start a proxy. When `initial_url` is an explicit localhost/loopback
    /// URL, only that exact host and port receive the local exception.
    pub(crate) async fn start(allow_private: bool, initial_url: Option<&url::Url>) -> Result<Self> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .context("Failed to bind guarded browser proxy")?;
        let address = listener
            .local_addr()
            .context("Failed to read guarded browser proxy address")?;
        let allowed_local = Arc::new(RwLock::new(HashSet::new()));
        if let Some(authority) = initial_url.and_then(LocalAuthority::from_url) {
            allowed_local
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert(authority);
        }

        let policy = Arc::clone(&allowed_local);
        let (shutdown, mut listener_shutdown) = watch::channel(false);
        let shutdown_for_connections = shutdown.clone();
        let task = tokio::spawn(async move {
            loop {
                let accepted = tokio::select! {
                    changed = listener_shutdown.changed() => {
                        if changed.is_err() || *listener_shutdown.borrow() {
                            break;
                        }
                        continue;
                    }
                    accepted = listener.accept() => accepted,
                };
                let Ok((stream, _peer)) = accepted else { break };
                let policy = Arc::clone(&policy);
                let mut connection_shutdown = shutdown_for_connections.subscribe();
                tokio::spawn(async move {
                    if *connection_shutdown.borrow() {
                        return;
                    }
                    tokio::select! {
                        _ = connection_shutdown.changed() => {}
                        result = proxy_connection(stream, allow_private, policy) => {
                            if let Err(error) = result {
                                tracing::debug!("guarded browser proxy rejected connection: {error:#}");
                            }
                        }
                    }
                });
            }
        });

        Ok(Self {
            address,
            allowed_local,
            shutdown,
            task,
        })
    }

    pub(crate) fn url(&self) -> String {
        format!("http://{}", self.address)
    }

    /// Add a Rust-validated explicit local navigation target. This is used by
    /// the persistent page controller; page-scoped routing in the bridge keeps
    /// the permission from becoming available to unrelated tabs.
    pub(crate) fn allow_local_url(&self, url: &url::Url) {
        if let Some(authority) = LocalAuthority::from_url(url) {
            self.allowed_local
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert(authority);
        }
    }
}

impl Drop for GuardedProxy {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
        self.task.abort();
    }
}

const MAX_PROXY_HEADER_BYTES: usize = 64 * 1024;

async fn proxy_connection(
    mut downstream: TcpStream,
    allow_private: bool,
    allowed_local: Arc<RwLock<HashSet<LocalAuthority>>>,
) -> Result<()> {
    let mut request = Vec::with_capacity(4096);
    let header_end = loop {
        if request.len() >= MAX_PROXY_HEADER_BYTES {
            anyhow::bail!("proxy request headers exceed {MAX_PROXY_HEADER_BYTES} bytes");
        }
        let mut chunk = [0_u8; 4096];
        let read = downstream.read(&mut chunk).await?;
        if read == 0 {
            anyhow::bail!("proxy client closed before sending request headers");
        }
        request.extend_from_slice(&chunk[..read]);
        if let Some(end) = find_header_end(&request) {
            break end;
        }
    };

    let headers = std::str::from_utf8(&request[..header_end])
        .context("proxy request headers are not valid UTF-8")?;
    let first_line = headers
        .lines()
        .next()
        .context("proxy request is missing a request line")?;
    let mut parts = first_line.split_whitespace();
    let method = parts.next().context("proxy request is missing a method")?;
    let target = parts.next().context("proxy request is missing a target")?;
    let version = parts
        .next()
        .context("proxy request is missing an HTTP version")?;

    if method.eq_ignore_ascii_case("CONNECT") {
        let (host, port) = parse_authority(target, 443)?;
        let mut upstream = connect_permitted(&host, port, allow_private, &allowed_local).await?;
        downstream
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await?;
        if header_end < request.len() {
            upstream.write_all(&request[header_end..]).await?;
        }
        tokio::io::copy_bidirectional(&mut downstream, &mut upstream).await?;
        return Ok(());
    }

    let parsed = url::Url::parse(target).context("proxy request target is not an absolute URL")?;
    if !matches!(parsed.scheme(), "http" | "ws") {
        anyhow::bail!("proxy only accepts HTTP requests and HTTPS CONNECT tunnels");
    }
    let host = parsed.host_str().context("proxy target has no host")?;
    let port = parsed.port_or_known_default().unwrap_or(80);
    let mut upstream = connect_permitted(host, port, allow_private, &allowed_local).await?;

    let mut origin_target = parsed.path().to_string();
    if origin_target.is_empty() {
        origin_target.push('/');
    }
    if let Some(query) = parsed.query() {
        origin_target.push('?');
        origin_target.push_str(query);
    }
    let mut forwarded = format!("{method} {origin_target} {version}\r\n").into_bytes();
    for line in headers.lines().skip(1) {
        if line.is_empty() || line.to_ascii_lowercase().starts_with("proxy-connection:") {
            continue;
        }
        forwarded.extend_from_slice(line.as_bytes());
        forwarded.extend_from_slice(b"\r\n");
    }
    forwarded.extend_from_slice(b"\r\n");
    forwarded.extend_from_slice(&request[header_end..]);
    upstream.write_all(&forwarded).await?;
    tokio::io::copy_bidirectional(&mut downstream, &mut upstream).await?;
    Ok(())
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|i| i + 4)
}

fn parse_authority(authority: &str, default_port: u16) -> Result<(String, u16)> {
    let url = url::Url::parse(&format!("http://{authority}/"))
        .context("proxy CONNECT target is not a valid authority")?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        anyhow::bail!("proxy CONNECT target is not a valid authority");
    }
    let host = url.host_str().context("proxy CONNECT target has no host")?;
    Ok((host.to_string(), url.port().unwrap_or(default_port)))
}

async fn connect_permitted(
    host: &str,
    port: u16,
    allow_private: bool,
    allowed_local: &RwLock<HashSet<LocalAuthority>>,
) -> Result<TcpStream> {
    let local_allowed = allowed_local
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .any(|authority| authority.matches(host, port));
    let resolver_host = host_for_socket_resolution(host);
    let addresses: Vec<_> = tokio::net::lookup_host((resolver_host, port))
        .await
        .with_context(|| format!("Failed to resolve proxy target {host}"))?
        .collect();
    if addresses.is_empty() {
        anyhow::bail!("proxy target {host} did not resolve to any addresses");
    }

    let mut last_error = None;
    for address in addresses {
        if !resolved_address_allowed(host, address.ip(), allow_private, local_allowed) {
            continue;
        }
        match TcpStream::connect(address).await {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = Some(error),
        }
    }
    if let Some(error) = last_error {
        return Err(error).with_context(|| format!("Failed to connect to proxy target {host}"));
    }
    anyhow::bail!("Blocked proxy connection to private/internal network address for {host}")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[path = "../../tests/unit/tools/net_policy/net_policy_test.rs"]
mod tests;
