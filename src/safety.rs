use anyhow::Result;
use crate::config::SafetyConfig;
use crate::api::types::ToolCall;

pub struct SafetyChecker {
    config: SafetyConfig,
}

impl SafetyChecker {
    pub fn new(config: &SafetyConfig) -> Self {
        Self { config: config.clone() }
    }

    pub fn check_tool_call(&self, call: &ToolCall) -> Result<()> {
        match call.function.name.as_str() {
            "file_write" | "file_edit" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    self.check_path(path)?;
                }
            }
            "shell_exec" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
                
                let dangerous = ["rm -rf /", "mkfs", "dd if=", ":(){ :|:& };:"];
                for pattern in &dangerous {
                    if cmd.contains(pattern) {
                        anyhow::bail!("Dangerous command blocked: {}", pattern);
                    }
                }
            }
            _ => {}
        }
        
        Ok(())
    }

    fn check_path(&self, path: &str) -> Result<()> {
        for pattern in &self.config.denied_paths {
            if glob::Pattern::new(pattern)?.matches(path) {
                anyhow::bail!("Path matches denied pattern: {}", pattern);
            }
        }
        
        if !self.config.allowed_paths.is_empty() {
            let mut allowed = false;
            for pattern in &self.config.allowed_paths {
                if glob::Pattern::new(pattern)?.matches(path) {
                    allowed = true;
                    break;
                }
            }
            if !allowed {
                anyhow::bail!("Path not in allowed list");
            }
        }
        
        Ok(())
    }
}
