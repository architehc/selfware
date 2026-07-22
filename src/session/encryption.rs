#![allow(dead_code, unused_imports, unused_variables)]

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::{Context, Result};
use rand::Rng;
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

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .context("Failed to create encryption salt file with secure permissions")?;
        std::io::Write::write_all(&mut file, &salt).context("Failed to write encryption salt")?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&path, &salt).context("Failed to write encryption salt file")?;
    }

    Ok(salt)
}

/// Static holder for ephemeral salt (used when salt file I/O fails).
/// This ensures salt consistency across multiple key derivation calls within a session.
static EPHEMERAL_SALT: OnceLock<Vec<u8>> = OnceLock::new();

/// Get or create the per-installation salt.
///
/// If salt file I/O fails, uses a session-scoped ephemeral salt stored in
/// EPHEMERAL_SALT to ensure consistency across multiple key derivation calls.
fn get_salt() -> Result<Vec<u8>> {
    // Try to load from file first
    match load_or_create_salt() {
        Ok(salt) => Ok(salt),
        Err(e) => {
            tracing::warn!(
                "Failed to persist salt: {}. Using ephemeral salt for this session.",
                e
            );
            // Use or create ephemeral salt in memory
            Ok(EPHEMERAL_SALT
                .get_or_init(|| {
                    let mut salt = vec![0u8; SALT_LEN];
                    rand::rng().fill_bytes(&mut salt);
                    salt
                })
                .clone())
        }
    }
}

/// Derive a 256-bit encryption key from a password using PBKDF2-HMAC-SHA256
/// with a per-installation random salt.
fn derive_key(password: &str) -> [u8; 32] {
    let salt = get_salt().unwrap_or_else(|_| {
        // Double-fallback: if everything fails, use a static fallback
        tracing::error!("All salt sources failed. Using static fallback salt.");
        vec![0u8; SALT_LEN]
    });
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), &salt, KDF_ITERATIONS, &mut key);
    key
}

use zeroize::Zeroize;

/// Zero the encryption key when the manager is dropped.
///
/// This provides basic defense-in-depth against memory-scraping attacks.
impl Drop for EncryptionManager {
    fn drop(&mut self) {
        self.key.zeroize();
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


    /// Create an EncryptionManager directly (test only, bypasses OnceLock)
    #[cfg(test)]
    pub fn new_for_test(password: &str) -> Self {
        Self::new_from_password(password)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/session/encryption/encryption_test.rs"]
mod tests;
