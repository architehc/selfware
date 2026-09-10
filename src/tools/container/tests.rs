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
    // Valid specs
    assert!(validate_volume_spec("/host:/container"));
    assert!(validate_volume_spec("/host:/container:ro"));
    assert!(validate_volume_spec("named_volume:/container"));

    // Syntax failures
    assert!(!validate_volume_spec(""));
    assert!(!validate_volume_spec("/host"));
    assert!(!validate_volume_spec("/host:/container:invalid"));

    // Host path security refusals
    assert!(!validate_volume_spec("/:/host"));
    assert!(!validate_volume_spec("/:/host:rw"));
    assert!(!validate_volume_spec(
        "/var/run/docker.sock:/var/run/docker.sock"
    ));
    assert!(!validate_volume_spec("docker.sock:/var/run/docker.sock"));
    assert!(!validate_volume_spec("/etc:/etc:ro"));
    assert!(!validate_volume_spec("/etc/shadow:/etc/shadow:ro"));
    assert!(!validate_volume_spec("/proc/sys:/mnt/sys:rw"));
    assert!(!validate_volume_spec("/dev/sda:/dev/sda"));
    assert!(!validate_volume_spec("/root/.ssh:/root/.ssh"));
    assert!(!validate_volume_spec("~/.ssh:/root/.ssh:ro"));
    assert!(!validate_volume_spec("~/.aws:/root/.aws:ro"));
    assert!(!validate_volume_spec("../:/app"));
    assert!(!validate_volume_spec("./src/../../etc:/app"));
    assert!(!validate_volume_spec(".git:/app/.git"));
}
