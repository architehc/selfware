//! Container Tools Tests

use super::*;

#[test]
fn test_port_validation() {
    assert!(is_valid_port("1"));
    assert!(is_valid_port("80"));
    assert!(is_valid_port("8080"));
    assert!(is_valid_port("65535"));
    assert!(!is_valid_port("0"));
    assert!(!is_valid_port("65536"));
    assert!(!is_valid_port(""));
    assert!(!is_valid_port("abc"));
}

#[test]
fn test_validate_port_mapping() {
    assert!(validate_port_mapping("8080:80"));
    assert!(validate_port_mapping("127.0.0.1:8080:80"));
    assert!(!validate_port_mapping("80"));
    assert!(!validate_port_mapping("invalid"));
    assert!(!validate_port_mapping(""));
}

#[test]
fn test_validate_volume_spec() {
    assert!(validate_volume_spec("/host:/container"));
    assert!(validate_volume_spec("/host:/container:ro"));
    assert!(validate_volume_spec("named_volume:/container"));
    assert!(!validate_volume_spec(""));
    assert!(!validate_volume_spec("/host"));
    assert!(!validate_volume_spec("/host:/container:invalid"));
}
