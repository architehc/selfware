use super::Tool;
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

pub struct ShellExec;

#[async_trait]
impl Tool for ShellExec {
    fn name(&self) -> &str { "shell_exec" }
    
    fn description(&self) -> &str {
        "Execute shell command. Use for builds, tests, and system operations. Runs with timeout."
    }
    
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "Command to execute"},
                "cwd": {"type": "string", "description": "Working directory"},
                "timeout_secs": {"type": "integer", "default": 60, "description": "Timeout in seconds"},
                "env": {"type": "object", "additionalProperties": {"type": "string"}}
            },
            "required": ["command"]
        })
    }
    
    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            command: String,
            cwd: Option<String>,
            #[serde(default = "default_timeout")]
            timeout_secs: u64,
            #[serde(default)]
            env: HashMap<String, String>,
        }
        
        fn default_timeout() -> u64 { 60 }
        
        let args: Args = serde_json::from_value(args)?;
        
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(&args.command);
        
        if let Some(cwd) = &args.cwd {
            cmd.current_dir(cwd);
        }
        
        cmd.envs(&args.env);
        
        let start = std::time::Instant::now();
        let output = tokio::time::timeout(
            Duration::from_secs(args.timeout_secs),
            cmd.output()
        ).await;
        
        let (exit_code, stdout, stderr, timed_out) = match output {
            Ok(Ok(output)) => (
                output.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&output.stdout).to_string(),
                String::from_utf8_lossy(&output.stderr).to_string(),
                false
            ),
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => (-1, "".to_string(), "Command timed out".to_string(), true),
        };
        
        let duration_ms = start.elapsed().as_millis() as u64;
        
        Ok(serde_json::json!({
            "exit_code": exit_code,
            "stdout": stdout.chars().take(10000).collect::<String>(),
            "stderr": stderr.chars().take(10000).collect::<String>(),
            "duration_ms": duration_ms,
            "timed_out": timed_out
        }))
    }
}
