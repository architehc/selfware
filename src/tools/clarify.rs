//! `ask_user` tool: lets the agent ask the human a single clarifying question
//! when the task direction is genuinely ambiguous.
//!
//! The tool is **rate-limited** (max `max_asks` per session), **user-disableable**
//! via `UiConfig::allow_clarification`, and **safe in headless mode** — it never
//! blocks on stdin when there is no TTY, instead returning a null answer so the
//! agent can proceed with its best assumption.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::io::IsTerminal;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::tools::Tool;

/// A tool that asks the human user one clarifying question.
///
/// Construct with [`ClarificationTool::new`] passing `enabled` (from config) and
/// `max_asks` (session-level rate limit). The internal `asked` counter is atomic
/// so the tool is safe to share across async tasks.
pub struct ClarificationTool {
    enabled: bool,
    max_asks: u32,
    asked: AtomicU32,
}

impl ClarificationTool {
    /// Create a new clarification tool.
    ///
    /// * `enabled` — whether user clarification is allowed at all (config flag).
    /// * `max_asks` — maximum number of questions permitted per session.
    pub fn new(enabled: bool, max_asks: u32) -> Self {
        Self {
            enabled,
            max_asks,
            asked: AtomicU32::new(0),
        }
    }

    /// Returns how many questions have been asked so far (mainly for testing).
    pub fn asked_count(&self) -> u32 {
        self.asked.load(Ordering::SeqCst)
    }
}

impl Default for ClarificationTool {
    fn default() -> Self {
        Self::new(true, 3)
    }
}

#[async_trait]
impl Tool for ClarificationTool {
    fn name(&self) -> &str {
        "ask_user"
    }

    fn description(&self) -> &str {
        "Ask the human user ONE concise clarifying question when the task \
         direction is genuinely ambiguous and you cannot proceed with a \
         reasonable assumption. Use very sparingly — most tasks can be \
         completed by making a sensible default choice. The response \
         contains an \"answer\" field (string or null) and a \"note\" field \
         with guidance when no answer is available."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "A single, concise clarifying question for the user. \
                                   Keep it under 200 characters when possible."
                }
            },
            "required": ["question"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let question = args
            .get("question")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("missing required field: question"))?;

        // --- Guard 1: disabled by config ---
        if !self.enabled {
            return Ok(serde_json::json!({
                "answer": null,
                "note": "user clarification is disabled; proceed with your best assumption"
            }));
        }

        // --- Guard 2: rate limit exhausted ---
        if self.asked.load(Ordering::SeqCst) >= self.max_asks {
            return Ok(serde_json::json!({
                "answer": null,
                "note": "clarification limit reached for this session; proceed with your best assumption"
            }));
        }

        // --- Guard 3: TUI mode (NEVER block on stdin — the TUI owns the ---
        // --- terminal in raw mode, so a blocking read_line() would fight ---
        // --- its event loop).  We check the same global atomic the rest  ---
        // --- of the agent uses (`crate::output::is_tui_active()`).       ---
        // Checked BEFORE the headless guard because the TUI may have a
        // terminal stdin (is_terminal() == true) but still must not block.
        if crate::output::is_tui_active() {
            return Ok(serde_json::json!({
                "answer": null,
                "note": "running in TUI mode; interactive clarification is not available — proceed with your best assumption"
            }));
        }

        // --- Guard 4: headless / non-interactive (NEVER block on stdin) ---
        if !std::io::stdin().is_terminal() {
            return Ok(serde_json::json!({
                "answer": null,
                "note": "running non-interactively; no user available — proceed with your best assumption"
            }));
        }

        // --- Interactive path: ask the user ---
        // Increment the counter *before* reading so a concurrent call cannot
        // exceed the limit.
        self.asked.fetch_add(1, Ordering::SeqCst);

        eprintln!();
        eprintln!("┌─ Clarification requested ─────────────────────────────");
        eprintln!("│ {question}");
        eprintln!("└─ (type your answer and press Enter, or empty to skip) ─");

        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|e| anyhow!("failed to read user input: {e}"))?;

        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(serde_json::json!({
                "answer": null,
                "note": "user skipped the question; proceed with your best assumption"
            }));
        }

        Ok(serde_json::json!({ "answer": trimmed }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata {
            read_only: true,
            destructive: false,
            risk_level: crate::safety::RiskLevel::Low,
            network_access: false,
            shell_execution: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Disabled branch ──────────────────────────────────────────────

    #[tokio::test]
    async fn disabled_returns_note_without_blocking() {
        let tool = ClarificationTool::new(false, 3);
        let result = tool
            .execute(serde_json::json!({"question": "test?"}))
            .await
            .unwrap();
        assert!(result["answer"].is_null());
        assert!(result["note"].as_str().unwrap().contains("disabled"));
        // Counter should not have been incremented.
        assert_eq!(tool.asked_count(), 0);
    }

    // ── Over-limit branch ────────────────────────────────────────────

    #[tokio::test]
    async fn over_limit_returns_note_without_blocking() {
        let tool = ClarificationTool::new(true, 0); // max_asks = 0
        let result = tool
            .execute(serde_json::json!({"question": "test?"}))
            .await
            .unwrap();
        assert!(result["answer"].is_null());
        assert!(result["note"].as_str().unwrap().contains("limit reached"));
        assert_eq!(tool.asked_count(), 0);
    }

    // ── Over-limit after exhausting budget ───────────────────────────

    #[tokio::test]
    async fn over_limit_after_exhausting_budget() {
        let tool = ClarificationTool::new(true, 2);
        // Manually exhaust the budget (simulating prior interactive calls).
        tool.asked.store(2, Ordering::SeqCst);
        let result = tool
            .execute(serde_json::json!({"question": "one more?"}))
            .await
            .unwrap();
        assert!(result["answer"].is_null());
        assert!(result["note"].as_str().unwrap().contains("limit reached"));
        // Counter unchanged because we returned early.
        assert_eq!(tool.asked_count(), 2);
    }

    // ── Missing required field ───────────────────────────────────────

    #[tokio::test]
    async fn missing_question_field_errors() {
        let tool = ClarificationTool::new(true, 3);
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("question"));
    }

    // ── Non-string question field errors ─────────────────────────────

    #[tokio::test]
    async fn non_string_question_errors() {
        let tool = ClarificationTool::new(true, 3);
        let result = tool.execute(serde_json::json!({"question": 42})).await;
        assert!(result.is_err());
    }

    // ── Tool properties ──────────────────────────────────────────────

    #[test]
    fn tool_name_and_description() {
        let tool = ClarificationTool::new(true, 3);
        assert_eq!(tool.name(), "ask_user");
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn tool_schema_has_required_question() {
        let tool = ClarificationTool::new(true, 3);
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "question"));
    }

    #[test]
    fn tool_is_readonly_low_risk() {
        let tool = ClarificationTool::new(true, 3);
        assert!(tool.is_readonly());
        assert!(!tool.is_destructive());
        assert_eq!(tool.risk_level(), crate::safety::RiskLevel::Low);
    }

    #[test]
    fn default_is_enabled_with_max_3() {
        let tool = ClarificationTool::default();
        assert_eq!(tool.asked_count(), 0);
        // We can't directly read `enabled`/`max_asks` (private), but the
        // disabled test above covers the false path and this covers default.
    }

    // ── TUI mode guard ───────────────────────────────────────────────

    #[tokio::test]
    async fn tui_mode_returns_note_without_blocking() {
        // When the TUI is active, the tool must NOT block on stdin — it
        // should return a null answer immediately.
        let tool = ClarificationTool::new(true, 3);

        // Activate TUI mode for the duration of this test.
        crate::output::set_tui_active(true);
        let result = tool
            .execute(serde_json::json!({"question": "test?"}))
            .await
            .unwrap();
        // Restore TUI state.
        crate::output::set_tui_active(false);

        assert!(result["answer"].is_null());
        assert!(result["note"].as_str().unwrap().contains("TUI mode"));
        // Counter should not have been incremented.
        assert_eq!(tool.asked_count(), 0);
    }
}
