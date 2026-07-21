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
fn test_private_network_host_ipv6_bracket_invalid() {
    // Garbage inside brackets should not be treated as a valid host
    assert!(!is_private_network_host("[not-valid-ipv6]"));
    assert!(!is_private_network_host("[12345]"));
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
