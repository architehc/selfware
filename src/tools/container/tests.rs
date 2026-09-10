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

#[test]
fn test_is_valid_memory() {
    assert!(is_valid_memory("512m"));
    assert!(is_valid_memory("2g"));
    assert!(is_valid_memory("1024"));
    assert!(is_valid_memory("4G"));
    assert!(!is_valid_memory(""));
    assert!(!is_valid_memory("2gb"));
    assert!(!is_valid_memory("-1g"));
    assert!(!is_valid_memory("--privileged"));
    assert!(!is_valid_memory("g"));
}

#[test]
fn test_is_valid_user() {
    assert!(is_valid_user("1000"));
    assert!(is_valid_user("1000:1000"));
    assert!(is_valid_user("65534:65534"));
    assert!(is_valid_user("node"));
    assert!(!is_valid_user(""));
    assert!(!is_valid_user("-rm"));
    assert!(!is_valid_user("root; rm -rf /"));
}

#[test]
fn test_security_flags_hardened_is_default() {
    // No profile -> hardened defaults are applied.
    let flags = security_flags(&serde_json::json!({})).unwrap();
    assert!(flags
        .windows(2)
        .any(|w| w == ["--security-opt", "no-new-privileges"]));
    assert!(flags.iter().any(|f| f == "--pids-limit"));
    assert!(flags.windows(2).any(|w| w == ["--memory", "4g"]));
    assert!(flags.windows(2).any(|w| w == ["--cap-drop", "NET_RAW"]));
    // hardened stays usable: no read-only rootfs, no forced non-root user.
    assert!(!flags.iter().any(|f| f == "--read-only"));
    assert!(!flags.iter().any(|f| f == "--user"));
}

#[test]
fn test_security_flags_sealed_adds_full_isolation() {
    let flags = security_flags(&serde_json::json!({"profile": "sealed"})).unwrap();
    assert!(flags.windows(2).any(|w| w == ["--cap-drop", "ALL"]));
    assert!(flags.iter().any(|f| f == "--read-only"));
    assert!(flags.windows(2).any(|w| w == ["--user", "65534:65534"]));
}

#[test]
fn test_security_flags_unsafe_is_empty() {
    let flags = security_flags(&serde_json::json!({"profile": "unsafe"})).unwrap();
    assert!(flags.is_empty());
}

#[test]
fn test_security_flags_overrides_and_rejects_bad_input() {
    let flags = security_flags(&serde_json::json!({"memory": "512m", "pids_limit": 256})).unwrap();
    assert!(flags.windows(2).any(|w| w == ["--memory", "512m"]));
    assert!(flags.windows(2).any(|w| w == ["--pids-limit", "256"]));
    // Adversarial values are rejected, never forwarded to the runtime.
    assert!(security_flags(&serde_json::json!({"memory": "--privileged"})).is_err());
    assert!(security_flags(&serde_json::json!({"profile": "bogus"})).is_err());
    assert!(security_flags(&serde_json::json!({"pids_limit": 0})).is_err());
    assert!(security_flags(&serde_json::json!({"profile": "sealed", "user": "-x"})).is_err());
}
