//! Encrypted wrapper for KvStore.
//!
//! Provides transparent AES-256-GCM encryption of values at rest.
//! The encryption key is derived from a passphrase via PBKDF2-HMAC-SHA256.

use super::store::{KvStore, KvStoreError, Result};
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::RngCore;
use sha2::Sha256;

/// Number of PBKDF2 iterations for key derivation.
const KDF_ITERATIONS: u32 = 100_000;

/// Salt length in bytes.
const SALT_LEN: usize = 16;

/// AES-GCM nonce length in bytes.
const NONCE_LEN: usize = 12;

/// A wrapper around `KvStore` that encrypts values before writing and decrypts
/// on read. Encryption is performed using AES-256-GCM with a key derived from
/// a passphrase via PBKDF2-HMAC-SHA256.
///
/// The encrypted format stored in the inner `KvStore` is:
/// `base64(salt ++ nonce ++ ciphertext ++ tag)`
///
/// When `enabled` is `false`, the wrapper is a transparent pass-through.
pub struct EncryptedStore {
    inner: KvStore,
    passphrase: String,
    enabled: bool,
}

impl EncryptedStore {
    /// Create a new encrypted store wrapping an existing `KvStore`.
    ///
    /// If `enabled` is `false`, values are stored in plaintext (pass-through).
    pub fn new(inner: KvStore, passphrase: impl Into<String>, enabled: bool) -> Self {
        Self {
            inner,
            passphrase: passphrase.into(),
            enabled,
        }
    }

    /// Create an encrypted store backed by a file path.
    pub fn with_path(
        path: impl AsRef<std::path::Path>,
        passphrase: impl Into<String>,
        enabled: bool,
    ) -> Result<Self> {
        let inner = KvStore::with_path(path)?;
        Ok(Self::new(inner, passphrase, enabled))
    }

    /// Derive a 256-bit key from the passphrase and salt.
    fn derive_key(passphrase: &str, salt: &[u8]) -> [u8; 32] {
        let mut key = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), salt, KDF_ITERATIONS, &mut key);
        key
    }

    /// Encrypt a plaintext value, returning a base64-encoded blob.
    fn encrypt_value(&self, plaintext: &str) -> Result<String> {
        let mut salt = [0u8; SALT_LEN];
        rand::rng().fill_bytes(&mut salt);

        let key = Self::derive_key(&self.passphrase, &salt);
        let cipher = Aes256Gcm::new(&key.into());

        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| KvStoreError::StorageError(format!("Encryption failed: {}", e)))?;

        // Pack: salt ++ nonce ++ ciphertext
        let mut blob = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
        blob.extend_from_slice(&salt);
        blob.extend_from_slice(&nonce_bytes);
        blob.extend_from_slice(&ciphertext);

        Ok(base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &blob,
        ))
    }

    /// Decrypt a base64-encoded encrypted blob back to plaintext.
    fn decrypt_value(&self, encoded: &str) -> Result<String> {
        let blob = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            encoded,
        )
        .map_err(|e| KvStoreError::StorageError(format!("Base64 decode failed: {}", e)))?;

        if blob.len() < SALT_LEN + NONCE_LEN {
            return Err(KvStoreError::StorageError(
                "Encrypted data too short".to_string(),
            ));
        }

        let (salt, rest) = blob.split_at(SALT_LEN);
        let (nonce_bytes, ciphertext) = rest.split_at(NONCE_LEN);

        let key = Self::derive_key(&self.passphrase, salt);
        let cipher = Aes256Gcm::new(&key.into());
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| KvStoreError::StorageError(format!("Decryption failed: {}", e)))?;

        String::from_utf8(plaintext)
            .map_err(|e| KvStoreError::StorageError(format!("UTF-8 decode failed: {}", e)))
    }

    /// Insert a key-value pair. The value is encrypted if encryption is enabled.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) -> Result<()> {
        let value = value.into();
        if self.enabled {
            let encrypted = self.encrypt_value(&value)?;
            self.inner.insert(key, encrypted)
        } else {
            self.inner.insert(key, value)
        }
    }

    /// Insert or update a key-value pair.
    pub fn upsert(&mut self, key: impl Into<String>, value: impl Into<String>) -> Result<()> {
        let value = value.into();
        if self.enabled {
            let encrypted = self.encrypt_value(&value)?;
            self.inner.upsert(key, encrypted)
        } else {
            self.inner.upsert(key, value)
        }
    }

    /// Get and decrypt a value by key.
    pub fn get(&self, key: &str) -> Result<String> {
        let stored = self.inner.get(key)?;
        if self.enabled {
            self.decrypt_value(&stored)
        } else {
            Ok(stored)
        }
    }

    /// Remove a key and return the decrypted value.
    pub fn remove(&mut self, key: &str) -> Result<String> {
        let stored = self.inner.remove(key)?;
        if self.enabled {
            self.decrypt_value(&stored)
        } else {
            Ok(stored)
        }
    }

    /// Check if a key exists.
    pub fn contains_key(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Whether encryption is currently enabled.
    pub fn is_encrypted(&self) -> bool {
        self.enabled
    }

    /// Get a reference to the inner (unencrypted) store.
    pub fn inner(&self) -> &KvStore {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let inner = KvStore::new();
        let mut store = EncryptedStore::new(inner, "test-passphrase", true);

        store.insert("secret_key", "secret_value").unwrap();

        let value = store.get("secret_key").unwrap();
        assert_eq!(value, "secret_value");
    }

    #[test]
    fn test_encrypt_decrypt_empty_value() {
        let inner = KvStore::new();
        let mut store = EncryptedStore::new(inner, "pass", true);

        store.insert("empty", "").unwrap();
        assert_eq!(store.get("empty").unwrap(), "");
    }

    #[test]
    fn test_encrypt_decrypt_unicode() {
        let inner = KvStore::new();
        let mut store = EncryptedStore::new(inner, "pass", true);

        let unicode_value = "Hello, world! Rust is great.";
        store.insert("uni", unicode_value).unwrap();
        assert_eq!(store.get("uni").unwrap(), unicode_value);
    }

    #[test]
    fn test_encrypted_values_are_not_plaintext() {
        let inner = KvStore::new();
        let mut store = EncryptedStore::new(inner, "pass", true);

        store.insert("key", "my secret data").unwrap();

        // The raw stored value should NOT be the plaintext
        let raw = store.inner().get("key").unwrap();
        assert_ne!(raw, "my secret data");
    }

    #[test]
    fn test_disabled_encryption_is_passthrough() {
        let inner = KvStore::new();
        let mut store = EncryptedStore::new(inner, "pass", false);

        store.insert("key", "plaintext").unwrap();
        assert_eq!(store.get("key").unwrap(), "plaintext");

        // Raw value should be plaintext when encryption is disabled
        let raw = store.inner().get("key").unwrap();
        assert_eq!(raw, "plaintext");
    }

    #[test]
    fn test_wrong_passphrase_fails() {
        let inner = KvStore::new();
        let mut store = EncryptedStore::new(inner, "correct-pass", true);

        store.insert("key", "secret").unwrap();

        // Reconstruct with wrong passphrase (sharing the same inner store)
        // We simulate by creating a new store, copying the raw value
        let raw = store.inner().get("key").unwrap();

        let mut inner2 = KvStore::new();
        inner2.insert("key", raw).unwrap();
        let store2 = EncryptedStore::new(inner2, "wrong-pass", true);

        let result = store2.get("key");
        assert!(result.is_err(), "Decryption with wrong passphrase should fail");
    }

    #[test]
    fn test_upsert_encrypted() {
        let inner = KvStore::new();
        let mut store = EncryptedStore::new(inner, "pass", true);

        store.upsert("key", "value1").unwrap();
        assert_eq!(store.get("key").unwrap(), "value1");

        store.upsert("key", "value2").unwrap();
        assert_eq!(store.get("key").unwrap(), "value2");
    }

    #[test]
    fn test_remove_encrypted() {
        let inner = KvStore::new();
        let mut store = EncryptedStore::new(inner, "pass", true);

        store.insert("key", "value").unwrap();
        let removed = store.remove("key").unwrap();
        assert_eq!(removed, "value");
        assert!(store.is_empty());
    }

    #[test]
    fn test_persistence_encrypted() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("encrypted_store.json");

        // Write encrypted data
        {
            let mut store = EncryptedStore::with_path(&path, "passphrase", true).unwrap();
            store.insert("k1", "v1").unwrap();
            store.insert("k2", "v2").unwrap();
        }

        // Read it back with the same passphrase
        {
            let store = EncryptedStore::with_path(&path, "passphrase", true).unwrap();
            assert_eq!(store.get("k1").unwrap(), "v1");
            assert_eq!(store.get("k2").unwrap(), "v2");
        }
    }

    #[test]
    fn test_is_encrypted_flag() {
        let store_on = EncryptedStore::new(KvStore::new(), "pass", true);
        assert!(store_on.is_encrypted());

        let store_off = EncryptedStore::new(KvStore::new(), "pass", false);
        assert!(!store_off.is_encrypted());
    }

    #[test]
    fn test_contains_key_and_len() {
        let inner = KvStore::new();
        let mut store = EncryptedStore::new(inner, "pass", true);

        assert!(store.is_empty());
        assert_eq!(store.len(), 0);

        store.insert("a", "1").unwrap();
        store.insert("b", "2").unwrap();

        assert!(store.contains_key("a"));
        assert!(!store.contains_key("c"));
        assert_eq!(store.len(), 2);
    }
}
