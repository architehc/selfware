use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::{Context, Result};
use rand::RngCore;
use sha2::Sha256;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Manager for at-rest encryption of sensitive data.
pub struct EncryptionManager {
    key: [u8; 32],
}

/// Length of the per-installation random salt in bytes.
const SALT_LEN: usize = 32;

/// Number of PBKDF2 iterations. 100,000 is the OWASP minimum recommendation
/// for PBKDF2-HMAC-SHA256 as of 2023.
const KDF_ITERATIONS: u32 = 100_000;

/// Return the path where the per-installation salt is stored.
fn salt_file_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("selfware")
        .join("encryption_salt")
}

/// Load or generate the per-installation salt.
///
/// On first use, a cryptographically random salt is generated and persisted.
/// Subsequent calls read the existing salt. This ensures each installation
/// has a unique salt, preventing rainbow-table attacks.
fn load_or_create_salt() -> Result<Vec<u8>> {
    let path = salt_file_path();

    if path.exists() {
        let data = std::fs::read(&path).context("Failed to read encryption salt file")?;
        if data.len() == SALT_LEN {
            return Ok(data);
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create selfware data directory")?;
    }

    let mut salt = vec![0u8; SALT_LEN];
    rand::rng().fill_bytes(&mut salt);
    std::fs::write(&path, &salt).context("Failed to write encryption salt file")?;
    Ok(salt)
}

/// Derive a 256-bit encryption key from a password using PBKDF2-HMAC-SHA256
/// with a per-installation random salt.
fn derive_key(password: &str) -> [u8; 32] {
    let salt = load_or_create_salt().unwrap_or_else(|_| {
        // Last-resort fallback: derive from machine-specific data.
        let mut fallback = Vec::new();
        fallback.extend_from_slice(b"selfware-fallback-");
        if let Ok(name) = whoami::username() {
            fallback.extend_from_slice(name.as_bytes());
        }
        if let Ok(host) = whoami::hostname() {
            fallback.extend_from_slice(host.as_bytes());
        }
        fallback
    });
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), &salt, KDF_ITERATIONS, &mut key);
    key
}

/// Zero the encryption key when the manager is dropped.
///
/// This provides basic defense-in-depth against memory-scraping attacks.
/// Note: for production use, a dedicated zeroize crate would be better as
/// the compiler may optimize away simple fill operations, but we avoid
/// adding new dependencies here.
impl Drop for EncryptionManager {
    fn drop(&mut self) {
        // Use write_volatile to prevent the compiler from optimizing this away.
        for byte in self.key.iter_mut() {
            // SAFETY: we have a mutable reference to each byte in the array.
            unsafe {
                std::ptr::write_volatile(byte, 0u8);
            }
        }
    }
}

static INSTANCE: OnceLock<EncryptionManager> = OnceLock::new();

impl EncryptionManager {
    /// Create a new per-session (non-global) encryption manager from a raw
    /// 256-bit key.
    ///
    /// This enables multi-agent scenarios where different sessions use
    /// different encryption contexts.  The caller is responsible for
    /// deriving the key (e.g. via PBKDF2).
    pub fn new_instance(key: [u8; 32]) -> Self {
        EncryptionManager { key }
    }

    /// Create a new per-session encryption manager from a password.
    ///
    /// Convenience wrapper around [`new_instance`](Self::new_instance)
    /// that derives a 256-bit key via PBKDF2-HMAC-SHA256 with the
    /// installation salt.
    pub fn new_from_password(password: &str) -> Self {
        let key = derive_key(password);
        Self::new_instance(key)
    }

    /// Initialize the global encryption manager with a password.
    ///
    /// This is the legacy entry-point.  For per-session usage prefer
    /// [`new_from_password`](Self::new_from_password) or
    /// [`new_instance`](Self::new_instance).
    pub fn init(password: &str) -> Result<()> {
        let key = derive_key(password);

        let manager = EncryptionManager { key };
        INSTANCE
            .set(manager)
            .map_err(|_| anyhow::anyhow!("Encryption already initialized"))?;
        Ok(())
    }

    /// Get the global encryption manager instance
    pub fn get() -> Option<&'static EncryptionManager> {
        INSTANCE.get()
    }

    /// Encrypt data using AES-256-GCM
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let cipher = Aes256Gcm::new(&self.key.into());

        // Generate a random 12-byte nonce
        let mut nonce_bytes = [0u8; 12];
        rand::rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

        // Prepend nonce to ciphertext
        let mut result = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);

        Ok(result)
    }

    /// Decrypt data using AES-256-GCM
    pub fn decrypt(&self, encrypted_data: &[u8]) -> Result<Vec<u8>> {
        if encrypted_data.len() < 12 {
            anyhow::bail!("Encrypted data too short");
        }

        let cipher = Aes256Gcm::new(&self.key.into());

        let (nonce_bytes, ciphertext) = encrypted_data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;

        Ok(plaintext)
    }

    /// Try to load password from OS keychain
    pub fn load_from_keychain() -> Result<Option<String>> {
        let entry = keyring::Entry::new(
            "selfware",
            &whoami::username().unwrap_or_else(|_| "selfware_user".to_string()),
        )
        .map_err(|e| anyhow::anyhow!("Keyring error: {}", e))?;
        match entry.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("Keyring error: {}", e)),
        }
    }

    /// Save password to OS keychain
    pub fn save_to_keychain(password: &str) -> Result<()> {
        let entry = keyring::Entry::new(
            "selfware",
            &whoami::username().unwrap_or_else(|_| "selfware_user".to_string()),
        )
        .map_err(|e| anyhow::anyhow!("Keyring error: {}", e))?;
        entry
            .set_password(password)
            .map_err(|e| anyhow::anyhow!("Keyring error: {}", e))?;
        Ok(())
    }

    /// Create an EncryptionManager directly (test only, bypasses OnceLock)
    #[cfg(test)]
    pub fn new_for_test(password: &str) -> Self {
        Self::new_from_password(password)
    }
}

#[cfg(test)]
mod tests {
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
}
