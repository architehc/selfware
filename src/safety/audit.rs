//! Audit logging for tool executions and agent actions.
//!
//! Writes structured JSONL audit events to `~/.selfware/audit/YYYY-MM-DD.jsonl`.
//! Uses an mpsc channel for buffered, non-blocking writes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// A single audit event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// When the event occurred.
    pub timestamp: DateTime<Utc>,
    /// Session identifier.
    pub session_id: String,
    /// What kind of event this is.
    pub event_type: AuditEventType,
    /// Tool name (for tool events).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Hash of tool arguments (for privacy — don't log raw args).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args_hash: Option<String>,
    /// Whether the action succeeded.
    pub success: bool,
    /// Duration in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// User decision (approved, denied, auto-approved).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_decision: Option<String>,
    /// Additional context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// Types of auditable events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    /// Tool was executed.
    ToolExecution,
    /// Tool was blocked by safety checker.
    SafetyBlock,
    /// Tool was skipped by user.
    UserSkip,
    /// Session started.
    SessionStart,
    /// Session ended.
    SessionEnd,
    /// Permission was granted.
    PermissionGrant,
    /// Hook was executed.
    HookExecution,
}

/// Audit logger that writes events asynchronously via a channel.
pub struct AuditLogger {
    tx: mpsc::UnboundedSender<AuditEvent>,
    session_id: String,
}

impl AuditLogger {
    /// Create a new audit logger that writes to the default directory.
    ///
    /// Returns `None` if the audit directory cannot be created.
    pub fn new(session_id: &str) -> Option<Self> {
        let audit_dir = Self::audit_dir()?;

        let (tx, rx) = mpsc::unbounded_channel();

        // Spawn background writer task
        let dir = audit_dir.clone();
        tokio::spawn(async move {
            Self::writer_loop(rx, dir).await;
        });

        info!("Audit logging enabled at {:?}", audit_dir);

        Some(Self {
            tx,
            session_id: session_id.to_string(),
        })
    }

    /// Get the audit directory path.
    fn audit_dir() -> Option<PathBuf> {
        let dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("selfware")
            .join("audit");

        if let Err(e) = std::fs::create_dir_all(&dir) {
            warn!("Failed to create audit directory {:?}: {}", dir, e);
            return None;
        }

        Some(dir)
    }

    /// Background task that reads events and writes them to JSONL files.
    async fn writer_loop(mut rx: mpsc::UnboundedReceiver<AuditEvent>, audit_dir: PathBuf) {
        use tokio::io::AsyncWriteExt;

        let mut current_date = String::new();
        let mut file: Option<tokio::fs::File> = None;

        while let Some(event) = rx.recv().await {
            let date = event.timestamp.format("%Y-%m-%d").to_string();

            // Rotate file if the date changed
            if date != current_date {
                current_date = date.clone();
                let path = audit_dir.join(format!("{}.jsonl", date));
                match tokio::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .await
                {
                    Ok(f) => file = Some(f),
                    Err(e) => {
                        warn!("Failed to open audit file {:?}: {}", path, e);
                        continue;
                    }
                }
            }

            if let Some(ref mut f) = file {
                if let Ok(mut line) = serde_json::to_vec(&event) {
                    line.push(b'\n');
                    if let Err(e) = f.write_all(&line).await {
                        warn!("Failed to write audit event: {}", e);
                    } else {
                        // Flush per event: the writer is detached and the
                        // process can exit at any time — buffered tail events
                        // would be lost otherwise.
                        let _ = f.flush().await;
                    }
                }
            }
        }

        // Channel closed (all senders dropped) — sync before exit so no tail
        // events sit in the OS buffer.
        if let Some(ref mut f) = file {
            let _ = f.sync_all().await;
        }
        debug!("Audit writer loop exited");
    }

    /// Log a tool execution event.
    pub fn log_tool_execution(
        &self,
        tool_name: &str,
        args_hash: &str,
        success: bool,
        duration_ms: u64,
        user_decision: Option<&str>,
    ) {
        let _ = self.tx.send(AuditEvent {
            timestamp: Utc::now(),
            session_id: self.session_id.clone(),
            event_type: AuditEventType::ToolExecution,
            tool_name: Some(tool_name.to_string()),
            args_hash: Some(args_hash.to_string()),
            success,
            duration_ms: Some(duration_ms),
            user_decision: user_decision.map(String::from),
            context: None,
        });
    }

    /// Log a safety block event.
    pub fn log_safety_block(&self, tool_name: &str, reason: &str) {
        let _ = self.tx.send(AuditEvent {
            timestamp: Utc::now(),
            session_id: self.session_id.clone(),
            event_type: AuditEventType::SafetyBlock,
            tool_name: Some(tool_name.to_string()),
            args_hash: None,
            success: false,
            duration_ms: None,
            user_decision: None,
            context: Some(reason.to_string()),
        });
    }

    /// Log a session start event.
    pub fn log_session_start(&self) {
        let _ = self.tx.send(AuditEvent {
            timestamp: Utc::now(),
            session_id: self.session_id.clone(),
            event_type: AuditEventType::SessionStart,
            tool_name: None,
            args_hash: None,
            success: true,
            duration_ms: None,
            user_decision: None,
            context: None,
        });
    }

    /// Log a session end event.
    pub fn log_session_end(&self) {
        let _ = self.tx.send(AuditEvent {
            timestamp: Utc::now(),
            session_id: self.session_id.clone(),
            event_type: AuditEventType::SessionEnd,
            tool_name: None,
            args_hash: None,
            success: true,
            duration_ms: None,
            user_decision: None,
            context: None,
        });
    }
}

#[cfg(test)]
#[path = "../../tests/unit/safety/audit/audit_test.rs"]
mod tests;
