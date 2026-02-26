//! Chat Store for Saving and Resuming Conversations
//!
//! Provides persistent storage for chat sessions so users can save,
//! list, resume, and delete named conversations.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

use crate::api::types::Message;
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
        std::fs::create_dir_all(&base).context("Failed to create chats directory")?;
        Ok(Self { chats_dir: base })
    }

    /// Fallback constructor that uses a temp directory (for when default location fails)
    pub fn fallback() -> Self {
        Self {
            chats_dir: std::env::temp_dir().join("selfware_chats"),
        }
    }

    /// Save a chat with the given name
    pub fn save(&self, name: &str, messages: &[Message], model: &str) -> Result<()> {
        // Ensure directory exists (especially for fallback mode)
        std::fs::create_dir_all(&self.chats_dir).context("Failed to create chats directory")?;

        let chat = SavedChat {
            name: name.to_string(),
            saved_at: Utc::now(),
            model: model.to_string(),
            messages: messages.to_vec(),
        };
        let path = self.chat_path(name);
        let json = serde_json::to_string_pretty(&chat)?;

        let data = if let Some(encryption) = EncryptionManager::get() {
            encryption.encrypt(json.as_bytes())?
        } else {
            json.into_bytes()
        };

        // Atomic write: write to temp file then rename, preventing corruption
        // if the process crashes mid-write or another instance writes concurrently.
        let tmp_path = path.with_extension(format!("json.tmp.{}", std::process::id()));
        {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)
                .context("Failed to create chat temp file")?;
            f.write_all(&data)
                .context("Failed to write chat temp file")?;
            f.sync_all().context("Failed to sync chat temp file")?;
        }
        if let Err(err) = std::fs::rename(&tmp_path, &path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(err).context("Failed to atomically replace chat file");
        }

        Ok(())
    }

    /// Load a saved chat by name
    pub fn load(&self, name: &str) -> Result<SavedChat> {
        let path = self.chat_path(name);
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
        Ok(chat)
    }

    /// List all saved chats
    pub fn list(&self) -> Result<Vec<ChatSummary>> {
        let mut summaries = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.chats_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
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
        summaries.sort_by(|a, b| b.saved_at.cmp(&a.saved_at));
        Ok(summaries)
    }

    /// Delete a saved chat
    pub fn delete(&self, name: &str) -> Result<()> {
        let path = self.chat_path(name);
        if path.exists() {
            std::fs::remove_file(&path).context("Failed to delete chat file")?;
            Ok(())
        } else {
            anyhow::bail!("Chat '{}' not found", name)
        }
    }

    /// Get the file path for a chat name
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_store() -> (ChatStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = ChatStore {
            chats_dir: dir.path().to_path_buf(),
        };
        (store, dir)
    }

    #[test]
    fn test_save_and_load() {
        let (store, _dir) = test_store();
        let messages = vec![
            Message::system("system prompt".to_string()),
            Message::user("hello".to_string()),
        ];
        store.save("test-chat", &messages, "test-model").unwrap();

        let loaded = store.load("test-chat").unwrap();
        assert_eq!(loaded.name, "test-chat");
        assert_eq!(loaded.model, "test-model");
        assert_eq!(loaded.messages.len(), 2);
    }

    #[test]
    fn test_list_chats() {
        let (store, _dir) = test_store();
        let messages = vec![Message::user("hello".to_string())];
        store.save("chat-a", &messages, "model-1").unwrap();
        store.save("chat-b", &messages, "model-2").unwrap();

        let list = store.list().unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_delete_chat() {
        let (store, _dir) = test_store();
        let messages = vec![Message::user("hello".to_string())];
        store.save("to-delete", &messages, "model").unwrap();
        assert!(store.delete("to-delete").is_ok());
        assert!(store.load("to-delete").is_err());
    }

    #[test]
    fn test_delete_nonexistent() {
        let (store, _dir) = test_store();
        assert!(store.delete("nonexistent").is_err());
    }

    #[test]
    fn test_load_nonexistent() {
        let (store, _dir) = test_store();
        assert!(store.load("nonexistent").is_err());
    }

    #[test]
    fn test_chat_path_sanitization() {
        let (store, _dir) = test_store();
        let path = store.chat_path("my chat/with spaces");
        assert!(!path.to_string_lossy().contains(' '));
    }
}
