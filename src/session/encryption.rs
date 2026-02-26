use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::Result;
use rand::RngCore;
use sha2::Sha256;
use std::sync::OnceLock;

/// Manager for at-rest encryption of sensitive data.
pub struct EncryptionManager {
    key: [u8; 32],
}

/// Application-specific salt for PBKDF2 key derivation.
const KDF_SALT: &[u8] = b"selfware-encryption-v1";

/// Number of PBKDF2 iterations. 100,000 is the OWASP minimum recommendation
/// for PBKDF2-HMAC-SHA256 as of 2023.
const KDF_ITERATIONS: u32 = 100_000;

/// Derive a 256-bit encryption key from a password using PBKDF2-HMAC-SHA256.
///
/// BREAKING CHANGE: This replaces the previous plain SHA-256 derivation.
/// Existing encrypted session data created before this change will NOT be
/// decryptable with keys derived by this function. A migration or
/// re-encryption step is required for any pre-existing data.
fn derive_key(password: &str) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), KDF_SALT, KDF_ITERATIONS, &mut key);
    key
}

static INSTANCE: OnceLock<EncryptionManager> = OnceLock::new();

impl EncryptionManager {
    /// Initialize the global encryption manager with a password
    pub fn init(password: &str) -> Result<()> {
        let key = derive_key(password);

        let manager = EncryptionManager { key };
        INSTANCE.set(manager).map_err(|_| anyhow::anyhow!("Encryption already initialized"))?;
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
        let entry = keyring::Entry::new("selfware", &whoami::username().unwrap_or_else(|_| "selfware_user".to_string()))
            .map_err(|e| anyhow::anyhow!("Keyring error: {}", e))?;
        match entry.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("Keyring error: {}", e)),
        }
    }

    /// Save password to OS keychain
    pub fn save_to_keychain(password: &str) -> Result<()> {
        let entry = keyring::Entry::new("selfware", &whoami::username().unwrap_or_else(|_| "selfware_user".to_string()))
            .map_err(|e| anyhow::anyhow!("Keyring error: {}", e))?;
        entry.set_password(password).map_err(|e| anyhow::anyhow!("Keyring error: {}", e))?;
        Ok(())
    }

    /// Create an EncryptionManager directly (test only, bypasses OnceLock)
    #[cfg(test)]
    pub fn new_for_test(password: &str) -> Self {
        let key = derive_key(password);
        EncryptionManager { key }
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
        assert_ne!(enc1, enc2, "Same plaintext should produce different ciphertext due to random nonce");
    }
}
