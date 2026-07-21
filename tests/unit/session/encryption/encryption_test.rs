use super::*;

fn test_manager() -> EncryptionManager {
    EncryptionManager::new_for_test("test-password-123")
}

#[test]
fn encrypt_decrypt_roundtrip() {
    let mgr = test_manager();
    let plaintext = b"hello world";
    let encrypted = mgr.encrypt(plaintext).unwrap();
    let decrypted = mgr.decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn encrypt_decrypt_empty() {
    let mgr = test_manager();
    let plaintext = b"";
    let encrypted = mgr.encrypt(plaintext).unwrap();
    let decrypted = mgr.decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn encrypt_decrypt_large() {
    let mgr = test_manager();
    let plaintext = vec![0xABu8; 10_000];
    let encrypted = mgr.encrypt(&plaintext).unwrap();
    let decrypted = mgr.decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn decrypt_too_short() {
    let mgr = test_manager();
    let short = vec![0u8; 8];
    let result = mgr.decrypt(&short);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("too short"));
}

#[test]
fn decrypt_wrong_key() {
    let mgr_a = EncryptionManager::new_for_test("key-a");
    let mgr_b = EncryptionManager::new_for_test("key-b");
    let encrypted = mgr_a.encrypt(b"secret data").unwrap();
    let result = mgr_b.decrypt(&encrypted);
    assert!(result.is_err());
}

#[test]
fn encrypt_different_nonces() {
    let mgr = test_manager();
    let plaintext = b"same data";
    let enc1 = mgr.encrypt(plaintext).unwrap();
    let enc2 = mgr.encrypt(plaintext).unwrap();
    assert_ne!(
        enc1, enc2,
        "Same plaintext should produce different ciphertext due to random nonce"
    );
}

// ---- Per-session instance tests ----

#[test]
fn new_instance_roundtrip() {
    let key = [0x42u8; 32];
    let mgr = EncryptionManager::new_instance(key);
    let plaintext = b"per-session secret";
    let encrypted = mgr.encrypt(plaintext).unwrap();
    let decrypted = mgr.decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn new_from_password_roundtrip() {
    let mgr = EncryptionManager::new_from_password("session-password-xyz");
    let plaintext = b"multi-agent data";
    let encrypted = mgr.encrypt(plaintext).unwrap();
    let decrypted = mgr.decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn different_instances_have_different_keys() {
    let mgr_a = EncryptionManager::new_from_password("password-a");
    let mgr_b = EncryptionManager::new_from_password("password-b");
    let encrypted = mgr_a.encrypt(b"secret").unwrap();
    // mgr_b should not be able to decrypt data encrypted by mgr_a
    let result = mgr_b.decrypt(&encrypted);
    assert!(result.is_err());
}

#[test]
fn new_instance_with_raw_key() {
    let key_a = [0xAAu8; 32];
    let key_b = [0xBBu8; 32];
    let mgr_a = EncryptionManager::new_instance(key_a);
    let mgr_b = EncryptionManager::new_instance(key_b);

    let encrypted = mgr_a.encrypt(b"data").unwrap();
    assert!(mgr_b.decrypt(&encrypted).is_err());
    assert_eq!(mgr_a.decrypt(&encrypted).unwrap(), b"data");
}
