use super::file::{
    clear_file_snapshot, is_file_stale, preserve_line_endings, read_file_with_encoding,
    resolve_safety_config, validate_tool_path,
};
use super::Tool;
use crate::api::ApiClient;
use crate::config::SafetyConfig;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use regex::Regex;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use tokio::fs;

/// Detect line ending style for FIM edit.
fn detect_line_ending_fim(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// Maximum allowed length for a FIM instruction (in characters).
const FIM_INSTRUCTION_MAX_LEN: usize = 500;

/// Sanitize a user-provided FIM instruction to prevent prompt injection.
///
/// 1. Strips FIM-specific control tokens that could alter the prompt structure.
/// 2. Strips common prompt-injection patterns (e.g. "ignore previous instructions").
/// 3. Truncates to [`FIM_INSTRUCTION_MAX_LEN`] characters.
fn sanitize_fim_instruction(raw: &str) -> String {
    // ── Step 1: Remove FIM / special model tokens ──────────────────
    let fim_tokens: &[&str] = &[
        "<|fim_prefix|>",
        "<|fim_suffix|>",
        "<|fim_middle|>",
        "<|endoftext|>",
        "<|file_separator|>",
        "<|im_start|>",
        "<|im_end|>",
        "<|pad|>",
    ];

    let mut sanitized = raw.to_string();
    for token in fim_tokens {
        // Case-insensitive replacement so e.g. "<|FIM_PREFIX|>" is also caught.
        let pattern = regex::escape(token);
        if let Ok(re) = Regex::new(&format!("(?i){}", pattern)) {
            sanitized = re.replace_all(&sanitized, "").to_string();
        }
    }

    // ── Step 2: Remove common injection patterns ───────────────────
    let injection_patterns: &[&str] = &[
        r"(?i)ignore\s+(all\s+)?previous",
        r"(?i)disregard\s+(all\s+)?(previous\s+)?instructions?",
        r"(?i)forget\s+(all\s+)?(previous\s+)?instructions?",
        r"(?i)override\s+(all\s+)?(previous\s+)?instructions?",
        r"(?i)system\s*:",
        r"(?i)user\s*:",
        r"(?i)assistant\s*:",
        r"(?i)new\s+instructions?\s*:",
    ];

    for pat in injection_patterns {
        if let Ok(re) = Regex::new(pat) {
            sanitized = re.replace_all(&sanitized, "").to_string();
        }
    }

    // Collapse any resulting runs of whitespace.
    if let Ok(re) = Regex::new(r"\s{2,}") {
        sanitized = re.replace_all(&sanitized, " ").to_string();
    }
    let sanitized = sanitized.trim().to_string();

    // ── Step 3: Truncate to max length ─────────────────────────────
    if sanitized.len() > FIM_INSTRUCTION_MAX_LEN {
        // Truncate at a char boundary to avoid panicking on multi-byte chars.
        let mut end = FIM_INSTRUCTION_MAX_LEN;
        while !sanitized.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        sanitized[..end].to_string()
    } else {
        sanitized
    }
}

/// A tool that uses Fill-in-the-Middle (FIM) to intelligently edit code.
/// Supports optional per-instance safety configuration for multi-agent
/// scenarios via [`FileFimEdit::with_safety_config`].
pub struct FileFimEdit {
    client: Arc<ApiClient>,
    /// Per-instance safety config. When `Some`, overrides the global `SAFETY_CONFIG`.
    /// When `None`, falls back to the global or default config (backward compatible).
    pub safety_config: Option<SafetyConfig>,
}

impl FileFimEdit {
    pub fn new(client: Arc<ApiClient>) -> Self {
        Self {
            client,
            safety_config: None,
        }
    }

    pub fn with_safety_config(client: Arc<ApiClient>, config: SafetyConfig) -> Self {
        Self {
            client,
            safety_config: Some(config),
        }
    }
}

#[async_trait]
impl Tool for FileFimEdit {
    fn name(&self) -> &str {
        "file_fim_edit"
    }

    fn description(&self) -> &str {
        "Use intelligent Fill-in-the-Middle (FIM) to replace a block of code. Provide path, start_line, and end_line of the block to replace, and the instruction of what should go there. The AI will intelligently generate the middle part based on context."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "start_line": {"type": "integer", "description": "1-based starting line number to replace"},
                "end_line": {"type": "integer", "description": "1-based ending line number to replace"},
                "instruction": {"type": "string", "description": "What should the model generate to replace this block?"}
            },
            "required": ["path", "start_line", "end_line", "instruction"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing path"))?;
        let start_line = args["start_line"]
            .as_u64()
            .ok_or_else(|| anyhow!("Missing start_line"))? as usize;
        let end_line = args["end_line"]
            .as_u64()
            .ok_or_else(|| anyhow!("Missing end_line"))? as usize;
        let raw_instruction = args["instruction"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing instruction"))?;

        // Sanitize instruction to prevent prompt injection (issue #62)
        let instruction = sanitize_fim_instruction(raw_instruction);

        // Validate path safety BEFORE any file I/O
        let safety = resolve_safety_config(self.safety_config.as_ref());
        validate_tool_path(path, &safety)?;

        // Stale-guard: reject if file changed since last read
        if let Some(true) = is_file_stale(path) {
            return Err(anyhow!(
                "File {} changed on disk since you last read it. Re-read the file and try again.",
                path
            ));
        }

        let (content, _) = read_file_with_encoding(Path::new(path)).await?;
        let line_ending = detect_line_ending_fim(&content);
        let lines: Vec<&str> = content.lines().collect();

        if start_line == 0
            || start_line > lines.len()
            || end_line < start_line
            || end_line > lines.len()
        {
            return Err(anyhow!("Invalid line range"));
        }

        let prefix = lines[..start_line - 1].join("\n");
        let suffix = lines[end_line..].join("\n");

        // Format prompt using Qwen's specific FIM tokens (or standard FIM).
        // The instruction is placed inside a clearly-delimited metadata block
        // *before* the FIM prefix so that it cannot be confused with file
        // content or inject additional FIM control tokens.
        let prompt = format!(
            "[SELFWARE_FIM_METADATA_BEGIN]\ninstruction={}\n[SELFWARE_FIM_METADATA_END]\n<|fim_prefix|>{}\n<|fim_suffix|>{}\n<|fim_middle|>",
            instruction, prefix, suffix
        );

        let response = self
            .client
            .completion(
                &prompt,
                Some(2048),
                Some(vec![
                    "<|file_separator|>".to_string(),
                    "<|endoftext|>".to_string(),
                ]),
            )
            .await?;

        let middle = response
            .choices
            .first()
            .map(|c| c.text.clone())
            .unwrap_or_default();

        // Validate generated output before writing
        let trimmed = middle.trim();
        if trimmed.is_empty() {
            return Err(anyhow!(
                "FIM generated empty output \u{2014} refusing to write. \
                 The model may not have understood the instruction."
            ));
        }

        let new_content = format!("{}{}{}", prefix, middle, suffix);

        // For Rust files, run a quick syntax check before writing
        if path.ends_with(".rs") {
            use tokio::process::Command;
            let check = Command::new("rustfmt")
                .args(["--edition", "2021", "--check"])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .spawn();

            if let Ok(mut child) = check {
                if let Some(ref mut stdin) = child.stdin {
                    use tokio::io::AsyncWriteExt;
                    let _ = stdin.write_all(new_content.as_bytes()).await;
                }
                // rustfmt exits 1 on parse errors (not just format diffs).
                // Only block on actual parse failures, not formatting diffs.
                // We use --check so it doesn't modify stdin; exit code 0 or 1
                // both mean "parseable". Exit code 2+ means parse error.
                if let Ok(status) = child.wait().await {
                    if status.code().unwrap_or(0) >= 2 {
                        return Err(anyhow!(
                            "FIM-generated code has Rust syntax errors (rustfmt \
                             exit code {}). Refusing to write to prevent corruption.",
                            status.code().unwrap_or(-1)
                        ));
                    }
                }
            }
            // If rustfmt is not available, proceed without the check.
        }

        // Create backup before overwriting
        let backup_path = format!("{}.fim-backup", path);
        if let Err(e) = fs::copy(path, &backup_path).await {
            // Non-fatal: warn but proceed (file might be new)
            tracing::debug!("Could not create FIM backup: {}", e);
        }

        let final_content = preserve_line_endings(&new_content, line_ending);
        fs::write(path, &final_content).await?;
        clear_file_snapshot(path);

        Ok(serde_json::json!({
            "status": "success",
            "message": format!("Successfully replaced lines {}-{} using FIM.", start_line, end_line),
            "backup": backup_path
        }))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tools/fim/fim_test.rs"]
mod tests;
