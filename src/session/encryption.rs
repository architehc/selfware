use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::{Context, Result};
use rand::{RngCore, thread_rng};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

/// Manager for at-rest encryption of sensitive data.
pub struct EncryptionManager {
    key: [u8; 32],
}

static INSTANCE: OnceLock<EncryptionManager> = OnceLock::new();

impl EncryptionManager {
    /// Initialize the global encryption manager with a password
    pub fn init(password: &str) -> Result<()> {
        let mut hasher = Sha256::new();
        hasher.update(password.as_bytes());
        let key: [u8; 32] = hasher.finalize().into();
        
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
        thread_rng().fill_bytes(&mut nonce_bytes);
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
        let entry = keyring::Entry::new("selfware", &whoami::username())?;
        match entry.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Save password to OS keychain
    pub fn save_to_keychain(password: &str) -> Result<()> {
        let entry = keyring::Entry::new("selfware", &whoami::username())?;
        entry.set_password(password)?;
        Ok(())
    }
}
