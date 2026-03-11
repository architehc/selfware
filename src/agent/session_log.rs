use anyhow::Result;
use chrono::{DateTime, Utc};
use colored::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use super::*;

const DEBUG_LOG_PREVIEW_LINES: usize = 20;
const RECENT_EVENT_LIMIT: usize = 128;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionEventType {
    SessionStart,
    SessionEnd,
    TaskStart,
    TaskEnd,
    ToolCall,
    ToolValidationFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionLogEvent {
    pub timestamp: DateTime<Utc>,
    pub session_id: String,
    pub event_type: SessionEventType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

pub struct SessionLogger {
    tx: mpsc::UnboundedSender<SessionLogEvent>,
    session_id: String,
    path: PathBuf,
    recent: Arc<Mutex<VecDeque<SessionLogEvent>>>,
}

impl SessionLogger {
    pub fn new(session_id: &str) -> Option<Self> {
        let dir = Self::default_dir()?;
        Self::new_in(session_id, dir)
    }

    fn default_dir() -> Option<PathBuf> {
        let dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("selfware")
            .join("session_logs");

        if let Err(e) = std::fs::create_dir_all(&dir) {
            warn!("Failed to create session log directory {:?}: {}", dir, e);
            return None;
        }

        Some(dir)
    }

    fn new_in(session_id: &str, dir: PathBuf) -> Option<Self> {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            warn!("Failed to create session log directory {:?}: {}", dir, e);
            return None;
        }

        let path = dir.join(format!("{}.jsonl", session_id));
        let (tx, rx) = mpsc::unbounded_channel();
        let recent = Arc::new(Mutex::new(VecDeque::with_capacity(RECENT_EVENT_LIMIT)));

        let writer_path = path.clone();
        tokio::spawn(async move {
            Self::writer_loop(rx, writer_path).await;
        });

        info!("Session execution logging enabled at {:?}", path);

        Some(Self {
            tx,
            session_id: session_id.to_string(),
            path,
            recent,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn recent_events(&self, limit: usize) -> Vec<SessionLogEvent> {
        self.recent
            .lock()
            .map(|events| {
                let len = events.len();
                let start = len.saturating_sub(limit);
                events.iter().skip(start).cloned().collect()
            })
            .unwrap_or_default()
    }

    pub fn read_recent(path: &Path, limit: usize) -> Result<Vec<SessionLogEvent>> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = std::fs::read_to_string(path)?;
        let mut lines: Vec<&str> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        if lines.len() > limit {
            lines = lines.split_off(lines.len() - limit);
        }

        let mut events = Vec::with_capacity(lines.len());
        for line in lines {
            match serde_json::from_str::<SessionLogEvent>(line) {
                Ok(event) => events.push(event),
                Err(e) => warn!("Skipping malformed session log line in {:?}: {}", path, e),
            }
        }

        Ok(events)
    }

    pub fn log(&self, mut event: SessionLogEvent) {
        event.session_id = self.session_id.clone();
        if let Ok(mut recent) = self.recent.lock() {
            if recent.len() == RECENT_EVENT_LIMIT {
                recent.pop_front();
            }
            recent.push_back(event.clone());
        }
        let _ = self.tx.send(event);
    }

    async fn writer_loop(mut rx: mpsc::UnboundedReceiver<SessionLogEvent>, path: PathBuf) {
        use tokio::io::AsyncWriteExt;

        let mut open_options = tokio::fs::OpenOptions::new();
        open_options.create(true).append(true);
        #[cfg(unix)]
        open_options.mode(0o600);

        let mut file = match open_options.open(&path).await {
            Ok(file) => file,
            Err(e) => {
                warn!("Failed to open session log {:?}: {}", path, e);
                return;
            }
        };

        while let Some(event) = rx.recv().await {
            match serde_json::to_vec(&event) {
                Ok(mut line) => {
                    line.push(b'\n');
                    if let Err(e) = file.write_all(&line).await {
                        warn!("Failed to write session log event to {:?}: {}", path, e);
                    }
                }
                Err(e) => warn!("Failed to serialize session log event: {}", e),
            }
        }

        debug!("Session log writer exited for {:?}", path);
    }
}

fn print_log_text_block(label: &str, text: Option<&str>) {
    println!("     {}:", label);
    let Some(text) = text else {
        println!("       <none>");
        return;
    };

    let lines: Vec<&str> = text.lines().collect();
    let show = lines.len().min(DEBUG_LOG_PREVIEW_LINES);
    for line in &lines[..show] {
        println!("       {}", line);
    }
    if lines.len() > show {
        println!("       ... ({} more lines)", lines.len() - show);
    }
}

fn session_event_label(kind: &SessionEventType) -> &'static str {
    match kind {
        SessionEventType::SessionStart => "session_start",
        SessionEventType::SessionEnd => "session_end",
        SessionEventType::TaskStart => "task_start",
        SessionEventType::TaskEnd => "task_end",
        SessionEventType::ToolCall => "tool_call",
        SessionEventType::ToolValidationFailed => "tool_validation_failed",
    }
}

fn outcome_label(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Success => "success",
        Outcome::Partial => "partial",
        Outcome::Failure => "failure",
        Outcome::Abandoned => "abandoned",
    }
}

impl Agent {
    pub(super) fn log_session_start_event(&self) {
        let Some(logger) = &self.session_logger else {
            return;
        };

        let cwd = std::env::current_dir()
            .ok()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| ".".to_string());

        logger.log(SessionLogEvent {
            timestamp: Utc::now(),
            session_id: String::new(),
            event_type: SessionEventType::SessionStart,
            task_id: None,
            tool_name: None,
            input: None,
            arguments: None,
            result: None,
            success: Some(true),
            duration_ms: None,
            details: Some(json!({
                "cwd": cwd,
                "model": self.config.model,
                "execution_mode": format!("{:?}", self.execution_mode()),
            })),
        });
    }

    pub(super) fn log_task_start_event(&self, task: &str) {
        let Some(logger) = &self.session_logger else {
            return;
        };

        logger.log(SessionLogEvent {
            timestamp: Utc::now(),
            session_id: String::new(),
            event_type: SessionEventType::TaskStart,
            task_id: self
                .current_checkpoint
                .as_ref()
                .map(|cp| cp.task_id.clone()),
            tool_name: None,
            input: Some(task.to_string()),
            arguments: None,
            result: None,
            success: None,
            duration_ms: None,
            details: Some(json!({
                "message_count": self.messages.len(),
                "memory_entries": self.memory.len(),
                "estimated_tokens": self.memory.total_tokens(),
                "pending_messages": self.pending_messages.len(),
                "context_files": self.context_files.len(),
                "current_step": self.loop_control.current_step(),
                "current_iteration": self.loop_control.current_iteration(),
            })),
        });
    }

    pub(super) fn log_task_outcome_event(
        &self,
        task_prompt: &str,
        outcome: Outcome,
        error: Option<&str>,
    ) {
        let Some(logger) = &self.session_logger else {
            return;
        };

        let checkpoint = self.current_checkpoint.as_ref();
        logger.log(SessionLogEvent {
            timestamp: Utc::now(),
            session_id: String::new(),
            event_type: SessionEventType::TaskEnd,
            task_id: checkpoint.map(|cp| cp.task_id.clone()),
            tool_name: None,
            input: Some(task_prompt.to_string()),
            arguments: None,
            result: error.map(ToOwned::to_owned),
            success: Some(outcome.is_positive()),
            duration_ms: None,
            details: Some(json!({
                "outcome": outcome_label(outcome),
                "message_count": self.messages.len(),
                "memory_entries": self.memory.len(),
                "estimated_tokens": self.memory.total_tokens(),
                "current_step": self.loop_control.current_step(),
                "current_iteration": self.loop_control.current_iteration(),
                "tool_calls": checkpoint.map(|cp| cp.tool_calls.len()).unwrap_or(0),
                "errors": checkpoint.map(|cp| cp.errors.len()).unwrap_or(0),
            })),
        });
    }

    pub(super) fn log_tool_validation_failure_event(
        &self,
        tool_name: &str,
        arguments: &str,
        error: &str,
        call_id: &str,
        use_native_fc: bool,
    ) {
        let Some(logger) = &self.session_logger else {
            return;
        };

        logger.log(SessionLogEvent {
            timestamp: Utc::now(),
            session_id: String::new(),
            event_type: SessionEventType::ToolValidationFailed,
            task_id: self
                .current_checkpoint
                .as_ref()
                .map(|cp| cp.task_id.clone()),
            tool_name: Some(tool_name.to_string()),
            input: None,
            arguments: Some(arguments.to_string()),
            result: Some(error.to_string()),
            success: Some(false),
            duration_ms: None,
            details: Some(json!({
                "call_id": call_id,
                "native_function_calling": use_native_fc,
            })),
        });
    }

    pub(super) fn log_session_tool_call_event(
        &self,
        tool_name: &str,
        arguments: &str,
        result: &str,
        success: bool,
        duration_ms: u64,
        truncate_result: bool,
    ) {
        let Some(logger) = &self.session_logger else {
            return;
        };

        logger.log(SessionLogEvent {
            timestamp: Utc::now(),
            session_id: String::new(),
            event_type: SessionEventType::ToolCall,
            task_id: self
                .current_checkpoint
                .as_ref()
                .map(|cp| cp.task_id.clone()),
            tool_name: Some(tool_name.to_string()),
            input: None,
            arguments: Some(arguments.to_string()),
            result: Some(result.to_string()),
            success: Some(success),
            duration_ms: Some(duration_ms),
            details: Some(json!({
                "checkpoint_result_truncated": truncate_result,
            })),
        });
    }

    pub(super) fn print_session_debug_log(&self) {
        println!();
        println!("  {} Session Debug Log", ">>".bright_cyan());
        let Some(logger) = &self.session_logger else {
            println!("  Session logging is unavailable for this run.");
            println!();
            return;
        };

        println!("  Session: {}", logger.session_id().dimmed());
        println!("  File: {}", logger.path().display().to_string().dimmed());
        let events = {
            let recent = logger.recent_events(10);
            if recent.is_empty() {
                SessionLogger::read_recent(logger.path(), 10).unwrap_or_default()
            } else {
                recent
            }
        };

        if events.is_empty() {
            println!("  No session log events recorded yet.");
        } else {
            println!("  Showing {} recent event(s)", events.len());
            for (index, event) in events.iter().enumerate() {
                let status = match event.success {
                    Some(true) => "success".bright_green(),
                    Some(false) => "failed".bright_red(),
                    None => "n/a".dimmed(),
                };
                println!(
                    "  {}. {} {} at {}",
                    index + 1,
                    session_event_label(&event.event_type).bright_white(),
                    status,
                    event.timestamp.format("%H:%M:%S").to_string().dimmed()
                );
                if let Some(task_id) = &event.task_id {
                    println!("     Task ID: {}", task_id.dimmed());
                }
                if let Some(tool_name) = &event.tool_name {
                    println!("     Tool: {}", tool_name);
                }
                if let Some(duration_ms) = event.duration_ms {
                    println!("     Duration: {}ms", duration_ms);
                }
                print_log_text_block("Input", event.input.as_deref());
                print_log_text_block("Args", event.arguments.as_deref());
                print_log_text_block("Result", event.result.as_deref());
                if let Some(details) = &event.details {
                    println!("     Details:");
                    for line in serde_json::to_string_pretty(details)
                        .unwrap_or_else(|_| details.to_string())
                        .lines()
                    {
                        println!("       {}", line);
                    }
                }
            }
        }
        println!();
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        if let Some(logger) = &self.audit_logger {
            logger.log_session_end();
        }
        if let Some(logger) = &self.session_logger {
            logger.log(SessionLogEvent {
                timestamp: Utc::now(),
                session_id: String::new(),
                event_type: SessionEventType::SessionEnd,
                task_id: self
                    .current_checkpoint
                    .as_ref()
                    .map(|cp| cp.task_id.clone()),
                tool_name: None,
                input: None,
                arguments: None,
                result: None,
                success: Some(true),
                duration_ms: None,
                details: Some(json!({
                    "message_count": self.messages.len(),
                    "memory_entries": self.memory.len(),
                    "estimated_tokens": self.memory.total_tokens(),
                    "pending_messages": self.pending_messages.len(),
                })),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn session_logger_writes_and_reads_recent_events() {
        let dir = tempdir().unwrap();
        let logger = SessionLogger::new_in("session-test", dir.path().to_path_buf()).unwrap();
        let path = logger.path().to_path_buf();

        logger.log(SessionLogEvent {
            timestamp: Utc::now(),
            session_id: String::new(),
            event_type: SessionEventType::SessionStart,
            task_id: None,
            tool_name: None,
            input: None,
            arguments: None,
            result: None,
            success: Some(true),
            duration_ms: None,
            details: Some(json!({"model": "test-model"})),
        });
        logger.log(SessionLogEvent {
            timestamp: Utc::now(),
            session_id: String::new(),
            event_type: SessionEventType::ToolCall,
            task_id: Some("task-1".to_string()),
            tool_name: Some("shell_exec".to_string()),
            input: None,
            arguments: Some("{\"command\":\"echo hi\"}".to_string()),
            result: Some("{\"exit_code\":0}".to_string()),
            success: Some(true),
            duration_ms: Some(12),
            details: None,
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let events = SessionLogger::read_recent(&path, 10).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, SessionEventType::SessionStart);
        assert_eq!(events[1].event_type, SessionEventType::ToolCall);
        assert_eq!(events[1].tool_name.as_deref(), Some("shell_exec"));
        assert_eq!(events[1].session_id, "session-test");
    }

    #[test]
    fn session_event_serializes_as_jsonl() {
        let event = SessionLogEvent {
            timestamp: Utc::now(),
            session_id: "session-1".to_string(),
            event_type: SessionEventType::ToolValidationFailed,
            task_id: Some("task-1".to_string()),
            tool_name: Some("shell_exec".to_string()),
            input: None,
            arguments: Some("{broken".to_string()),
            result: Some("Invalid JSON".to_string()),
            success: Some(false),
            duration_ms: None,
            details: Some(json!({"call_id": "call-1"})),
        };

        let encoded = serde_json::to_string(&event).unwrap();
        assert!(encoded.contains("\"tool_validation_failed\""));
        assert!(encoded.contains("\"shell_exec\""));
        assert!(encoded.contains("\"call-1\""));
    }
}
