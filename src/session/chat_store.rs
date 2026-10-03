//! Chat Store for Saving and Resuming Conversations
//!
//! Provides persistent storage for chat sessions so users can save,
//! list, resume, and delete named conversations.
//!
//! # Naming (collision discipline)
//!
//! Chat names map to `<name>.json` files. Only names made of alphanumerics,
//! `-` and `_` are writable: any other character (spaces, `.`, non-ASCII,
//! …) would be collapsed by the filesystem sanitization used for lookup and
//! would alias distinct sessions onto one file ("my session" and
//! "my_session" both resolving to `my_session.json`). [`ChatStore::save`]
//! therefore REJECTS such names instead of silently collapsing them, legacy
//! files written by the old sanitizer can still be loaded (by the exact name
//! recorded inside the file), and [`ChatStore::load`] verifies that the
//! loaded file's `SavedChat.name` equals the requested name — so loading one
//! session can never return a different session that merely shares a file.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::api::types::Message;
use crate::session::checkpoint::{replace_atomically, FileLock};
use crate::session::encryption::EncryptionManager;

/// A saved chat session
#[derive(Debug, Serialize, Deserialize)]
pub struct SavedChat {
    /// Name of the chat
    pub name: String,
    /// When the chat was saved
    pub saved_at: DateTime<Utc>,
    /// Model used
    pub model: String,
    /// Messages in the conversation
    pub messages: Vec<Message>,
}

/// Summary info for listing chats
#[derive(Debug, Serialize, Deserialize)]
pub struct ChatSummary {
    /// Name of the chat
    pub name: String,
    /// When the chat was saved
    pub saved_at: DateTime<Utc>,
    /// Model used
    pub model: String,
    /// Number of messages
    pub message_count: usize,
}

/// What protection was actually applied to a saved chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatSaveStatus {
    /// The serialized chat was encrypted with the active keychain-derived key.
    Encrypted,
    /// No encryption manager was available; the file is plaintext with
    /// owner-only filesystem permissions.
    Plaintext,
}

/// Persistent chat store backed by the filesystem
pub struct ChatStore {
    chats_dir: PathBuf,
}

impl ChatStore {
    /// Create a new chat store at the default location (~/.selfware/chats/)
    pub fn new() -> Result<Self> {
        let base = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("selfware")
            .join("chats");
        ensure_private_dir(&base)?;
        Ok(Self { chats_dir: base })
    }

    /// Fallback constructor that uses a temp directory (for when default location fails)
    pub fn fallback() -> Self {
        #[cfg(unix)]
        let suffix = unsafe { libc::geteuid() }.to_string();
        #[cfg(not(unix))]
        let suffix: String = whoami::username()
            .unwrap_or_else(|_| "selfware_user".to_string())
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        Self {
            chats_dir: std::env::temp_dir().join(format!("selfware_chats_{suffix}")),
        }
    }

    /// Return `true` when `name` maps to its own file under sanitized lookup
    /// (only alphanumerics, `-` and `_`). Any other character would be
    /// collapsed to `_` by [`ChatStore::chat_path`], silently aliasing
    /// distinct session names onto one file.
    ///
    /// Must stay the exact complement of the sanitizer: its keep-set is
    /// `is_alphanumeric() || '-' || '_'`, so a name is safe iff every one of
    /// its chars is in that set, and two distinct safe names can never map to
    /// the same file.
    fn is_safe_chat_name(name: &str) -> bool {
        name.chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    }

    /// Save a chat with the given name
    pub fn save(&self, name: &str, messages: &[Message], model: &str) -> Result<ChatSaveStatus> {
        if name.trim().is_empty() {
            anyhow::bail!("chat name must not be empty");
        }
        // Refuse alias-prone names instead of collapsing them: "my session"
        // and "my_session" must not become the same file on disk.
        if !Self::is_safe_chat_name(name) {
            anyhow::bail!(
                "chat name {:?} contains characters that would be folded onto \
                 another session's file on disk; use only letters, digits, '-' and '_'",
                name
            );
        }

        // Ensure directory exists (especially for fallback mode)
        ensure_private_dir(&self.chats_dir)?;

        // Advisory lock held across the read (identity check) → modify → atomic
        // write cycle so concurrent CLI/daemon instances saving the same chat
        // serialize instead of clobbering each other.
        let path = self.chat_path(name);
        let _lock = FileLock::acquire(&path)?;

        // Refuse to overwrite a file that holds a DIFFERENT session (legacy
        // sanitizer collision: e.g. a pre-fix `my session` landed in
        // `my_session.json`). `load` below verifies file identity, so only a
        // file that genuinely belongs to `name` (or does not exist) passes.
        if path.symlink_metadata().is_ok() {
            ensure_regular_chat_file(&path)?;
            self.load(name).with_context(|| {
                format!(
                    "refusing to overwrite chat file '{}' because it does not verify as session '{}'",
                    path.display(),
                    name
                )
            })?;
        }

        let chat = SavedChat {
            name: name.to_string(),
            saved_at: Utc::now(),
            model: model.to_string(),
            messages: messages.to_vec(),
        };
        let json = serde_json::to_string_pretty(&chat)?;

        let (data, status) = if let Some(encryption) = EncryptionManager::get() {
            (
                encryption.encrypt(json.as_bytes())?,
                ChatSaveStatus::Encrypted,
            )
        } else {
            // Never silently degrade to plaintext (AGENTS.md §3): the user
            // asked for keychain-backed encryption when initializing; if it
            // is unavailable they must know the bytes on disk are readable.
            tracing::warn!(
                "encryption unavailable — saving chat '{}' in plaintext",
                name
            );
            (json.into_bytes(), ChatSaveStatus::Plaintext)
        };

        // Atomic write: write to temp file then rename, preventing corruption
        // if the process crashes mid-write or another instance writes concurrently.
        let tmp_path = self.chats_dir.join(format!(
            ".{}.tmp.{}",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("chat"),
            Uuid::new_v4()
        ));
        write_private_temp_file(&tmp_path, &data)?;
        // Shared atomic replace with the preserve/install/restore fallback
        // for Windows rename semantics.
        if let Err(error) = replace_atomically(&tmp_path, &path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(error).context("Failed to atomically replace chat file");
        }
        set_owner_only_file_permissions(&path)?;

        Ok(status)
    }

    /// Load a saved chat by name
    pub fn load(&self, name: &str) -> Result<SavedChat> {
        let path = self.chat_path(name);
        ensure_regular_chat_file(&path)?;
        let data = std::fs::read(&path).with_context(|| format!("Chat '{}' not found", name))?;

        let json = if let Some(encryption) = EncryptionManager::get() {
            // Fail closed: if encryption is enabled and decryption fails, do NOT
            // silently fall back to reading the data as plain text.
            let plaintext = encryption.decrypt(&data).context(
                "Decryption failed for chat file. The file may be corrupt or tampered with.",
            )?;
            String::from_utf8(plaintext).context("Decrypted chat is not valid UTF-8")?
        } else {
            String::from_utf8(data).context("Chat file is not valid UTF-8")?
        };

        let chat: SavedChat = serde_json::from_str(&json)?;

        // Identity check: the sanitized lookup can resolve to a file written
        // under a DIFFERENT name (legacy sanitizer collisions — "my session"
        // and "my_session" both landing in my_session.json). Loading the one
        // must never silently return the other, so fail closed unless the
        // file claims the requested name.
        if chat.name != name {
            anyhow::bail!(
                "Chat '{}' resolves to {}, but that file holds session '{}' — \
                 refusing to load a different session under an alias",
                name,
                path.display(),
                chat.name
            );
        }
        Ok(chat)
    }

    /// List all saved chats
    pub fn list(&self) -> Result<Vec<ChatSummary>> {
        let mut summaries = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.chats_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    if ensure_regular_chat_file(&path).is_err() {
                        continue;
                    }
                    if let Ok(data) = std::fs::read(&path) {
                        let json_opt = if let Some(encryption) = EncryptionManager::get() {
                            // Fail closed: skip files that fail decryption.
                            match encryption.decrypt(&data) {
                                Ok(p) => String::from_utf8(p).ok(),
                                Err(_) => {
                                    tracing::warn!(
                                        "Skipping chat file {:?}: decryption failed (corrupt or tampered)",
                                        path
                                    );
                                    None
                                }
                            }
                        } else {
                            String::from_utf8(data).ok()
                        };

                        if let Some(json) = json_opt {
                            if let Ok(chat) = serde_json::from_str::<SavedChat>(&json) {
                                summaries.push(ChatSummary {
                                    name: chat.name,
                                    saved_at: chat.saved_at,
                                    model: chat.model,
                                    message_count: chat.messages.len(),
                                });
                            }
                        }
                    }
                }
            }
        }
        summaries.sort_by_key(|x| std::cmp::Reverse(x.saved_at));
        Ok(summaries)
    }

    /// Delete a saved chat
    ///
    /// Identity-validated like [`ChatStore::load`]: the sanitized lookup can
    /// land on a file recorded under a DIFFERENT name (legacy sanitizer
    /// collisions, e.g. both "my session" and "my_session" resolving to
    /// `my_session.json`), and one colliding spelling must not be able to
    /// delete another session's data. Deleting a file that verifies as the
    /// requested session — a fresh save, or a legacy file deleted under its
    /// OWN recorded name — still works (that is the repair path for stale
    /// files). A file that fails to verify (foreign session, corrupt, or
    /// unreadable) is left on disk and the delete refuses.
    pub fn delete(&self, name: &str) -> Result<()> {
        let path = self.chat_path(name);
        if path.symlink_metadata().is_err() {
            return Err(anyhow::anyhow!("Chat '{}' not found", name));
        }
        ensure_regular_chat_file(&path)?;
        // Same advisory lock as save: a delete must not interleave with a
        // concurrent read→modify→write cycle on this chat.
        let _lock = FileLock::acquire(&path)?;
        // Mirror load's identity check BEFORE touching the file: only a file
        // that verifies as `name` may be removed.
        self.load(name).with_context(|| {
            format!(
                "refusing to delete chat file '{}': it does not verify as session '{}'",
                path.display(),
                name
            )
        })?;
        std::fs::remove_file(&path).context("Failed to delete chat file")?;
        Ok(())
    }

    /// Get the file path for a chat name (sanitized LOOKUP, legacy-compatible).
    ///
    /// This sanitizer collapses non-`[alphanumeric/-/_]` chars to `_`, which
    /// historically aliased distinct names onto one file. New writes are
    /// gated by [`ChatStore::is_safe_chat_name`] so no NEW colliding files
    /// are created, and [`ChatStore::load`] verifies the file's recorded
    /// `SavedChat.name`, so this route is now only a lookup mechanism that
    /// can still find files the old sanitizer wrote.
    fn chat_path(&self, name: &str) -> PathBuf {
        // Sanitize name for filesystem
        let safe_name: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.chats_dir.join(format!("{}.json", safe_name))
    }
}

fn ensure_private_dir(path: &Path) -> Result<()> {
    match path.symlink_metadata() {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                anyhow::bail!(
                    "refusing chat directory that is a symlink or non-directory: {}",
                    path.display()
                );
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(path)
                .with_context(|| format!("Failed to create chats directory {}", path.display()))?;
        }
        Err(error) => return Err(error).context("Failed to inspect chats directory"),
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
        let directory = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
            .with_context(|| {
                format!("Failed to securely open chats directory {}", path.display())
            })?;
        let metadata = directory.metadata()?;
        let effective_uid = unsafe { libc::geteuid() };
        if metadata.uid() != effective_uid {
            anyhow::bail!(
                "refusing chats directory not owned by the current user: {}",
                path.display()
            );
        }
        directory
            .set_permissions(std::fs::Permissions::from_mode(0o700))
            .with_context(|| {
                format!("Failed to set owner-only permissions on {}", path.display())
            })?;
    }
    Ok(())
}

fn ensure_regular_chat_file(path: &Path) -> Result<()> {
    let metadata = path
        .symlink_metadata()
        .with_context(|| format!("Chat file not found: {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        anyhow::bail!(
            "refusing chat path that is a symlink or non-regular file: {}",
            path.display()
        );
    }
    Ok(())
}

fn write_private_temp_file(path: &Path, data: &[u8]) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> Result<()> {
        let mut file = options
            .open(path)
            .with_context(|| format!("Failed to create chat temp file {}", path.display()))?;
        file.write_all(data)
            .context("Failed to write chat temp file")?;
        file.sync_all().context("Failed to sync chat temp file")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
}

fn set_owner_only_file_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).with_context(
            || {
                format!(
                    "Failed to set owner-only permissions on chat file {}",
                    path.display()
                )
            },
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/session/chat_store/chat_store_test.rs"]
mod tests;
