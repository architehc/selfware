use super::*;
use std::time::Duration;

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
fn test_private_network_host_ipv6_bracket_invalid() {
    // Garbage inside brackets should not be treated as a valid host
    assert!(!is_private_network_host("[not-valid-ipv6]"));
    assert!(!is_private_network_host("[12345]"));
}

#[test]
fn socket_resolution_host_strips_url_ipv6_brackets() {
    let url = url::Url::parse("https://[2606:4700:4700::1111]/").unwrap();
    let serialized_host = url.host_str().unwrap();

    // WHATWG URL serialization retains IPv6 brackets, while Rust's
    // `(host, port)` socket resolvers require the bare literal.
    assert_eq!(serialized_host, "[2606:4700:4700::1111]");
    assert_eq!(
        host_for_socket_resolution(serialized_host),
        "2606:4700:4700::1111"
    );
}

#[test]
fn socket_resolution_host_leaves_dns_and_ipv4_hosts_unchanged() {
    assert_eq!(host_for_socket_resolution("example.com"), "example.com");
    assert_eq!(host_for_socket_resolution("192.0.2.1"), "192.0.2.1");
    assert_eq!(host_for_socket_resolution("[malformed"), "[malformed");
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

#[test]
fn localhost_permission_does_not_allow_private_dns_for_other_hosts() {
    assert!(!resolved_address_allowed(
        "redirect.example",
        "169.254.169.254".parse().unwrap(),
        false,
        true,
    ));
    assert!(!resolved_address_allowed(
        "redirect.example",
        "10.0.0.1".parse().unwrap(),
        false,
        true,
    ));
    assert!(resolved_address_allowed(
        "redirect.example",
        "1.1.1.1".parse().unwrap(),
        false,
        true,
    ));
}

#[test]
fn localhost_permission_is_limited_to_loopback_answers() {
    assert!(resolved_address_allowed(
        "localhost",
        "127.0.0.1".parse().unwrap(),
        false,
        true,
    ));
    assert!(!resolved_address_allowed(
        "localhost",
        "192.168.1.10".parse().unwrap(),
        false,
        true,
    ));
}

#[test]
fn public_request_cannot_redirect_to_localhost() {
    let initial =
        validate_url_target(&url::Url::parse("https://example.com").unwrap(), false).unwrap();
    let redirect = url::Url::parse("http://127.0.0.1/admin").unwrap();
    let error = validate_redirect_target(&redirect, &initial).unwrap_err();
    assert!(error.to_string().contains("public origin"));
}

#[test]
fn explicit_local_request_can_redirect_within_exact_authority() {
    let initial =
        validate_url_target(&url::Url::parse("http://localhost:8000").unwrap(), false).unwrap();
    let redirect = url::Url::parse("http://localhost:8000/next").unwrap();
    assert!(validate_redirect_target(&redirect, &initial).is_ok());
}

#[test]
fn explicit_local_request_cannot_redirect_to_alias_or_different_port() {
    let initial =
        validate_url_target(&url::Url::parse("http://localhost:8000").unwrap(), false).unwrap();
    for redirect in [
        "http://127.0.0.1:8000/next",
        "http://localhost:2375/next",
        "http://other.localhost:8000/next",
    ] {
        let error =
            validate_redirect_target(&url::Url::parse(redirect).unwrap(), &initial).unwrap_err();
        assert!(error.to_string().contains("initially authorized authority"));
    }
}

#[tokio::test]
async fn guarded_proxy_blocks_private_connect_without_explicit_local_target() {
    let upstream = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let port = upstream.local_addr().unwrap().port();
    let proxy = GuardedProxy::start(false, None).await.unwrap();
    let proxy_address = proxy.url().trim_start_matches("http://").to_string();
    let mut client = TcpStream::connect(proxy_address).await.unwrap();
    client
        .write_all(format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut response = [0_u8; 64];
    let read = tokio::time::timeout(Duration::from_secs(1), client.read(&mut response)).await;
    assert!(
        !matches!(read, Ok(Ok(size)) if response[..size].starts_with(b"HTTP/1.1 200")),
        "private CONNECT must not establish a tunnel"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), upstream.accept())
            .await
            .is_err(),
        "blocked CONNECT must never reach the local service"
    );
}

#[tokio::test]
async fn guarded_proxy_allows_only_explicit_local_authority() {
    let upstream = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let port = upstream.local_addr().unwrap().port();
    let initial = url::Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap();
    let proxy = GuardedProxy::start(false, Some(&initial)).await.unwrap();
    let proxy_address = proxy.url().trim_start_matches("http://").to_string();
    let mut client = TcpStream::connect(proxy_address).await.unwrap();
    client
        .write_all(format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let (mut server, _) = tokio::time::timeout(Duration::from_secs(1), upstream.accept())
        .await
        .unwrap()
        .unwrap();
    let mut response = [0_u8; 64];
    let size = tokio::time::timeout(Duration::from_secs(1), client.read(&mut response))
        .await
        .unwrap()
        .unwrap();
    assert!(response[..size].starts_with(b"HTTP/1.1 200"));
    client.write_all(b"ping").await.unwrap();
    let mut ping = [0_u8; 4];
    server.read_exact(&mut ping).await.unwrap();
    assert_eq!(&ping, b"ping");
}

#[tokio::test]
async fn dropping_guarded_proxy_closes_active_tunnels() {
    let upstream = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let port = upstream.local_addr().unwrap().port();
    let initial = url::Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap();
    let proxy = GuardedProxy::start(false, Some(&initial)).await.unwrap();
    let proxy_address = proxy.url().trim_start_matches("http://").to_string();
    let mut client = TcpStream::connect(proxy_address).await.unwrap();
    client
        .write_all(format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let (mut server, _) = tokio::time::timeout(Duration::from_secs(1), upstream.accept())
        .await
        .unwrap()
        .unwrap();
    let mut response = [0_u8; 64];
    let size = client.read(&mut response).await.unwrap();
    assert!(response[..size].starts_with(b"HTTP/1.1 200"));

    drop(proxy);
    let mut byte = [0_u8; 1];
    let client_closed = tokio::time::timeout(Duration::from_secs(1), client.read(&mut byte))
        .await
        .expect("client side of tunnel remained open after proxy drop");
    assert!(matches!(client_closed, Ok(0) | Err(_)));
    let server_closed = tokio::time::timeout(Duration::from_secs(1), server.read(&mut byte))
        .await
        .expect("upstream side of tunnel remained open after proxy drop");
    assert!(matches!(server_closed, Ok(0) | Err(_)));
}
