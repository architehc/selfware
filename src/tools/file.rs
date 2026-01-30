use super::Tool;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::path::Path;

pub struct FileRead;
pub struct FileWrite;
pub struct FileEdit;
pub struct DirectoryTree;

#[async_trait]
impl Tool for FileRead {
    fn name(&self) -> &str { "file_read" }
    
    fn description(&self) -> &str {
        "Read file contents. Use for examining code, configs, or any text file."
    }
    
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute or relative path to the file"
                },
                "line_range": {
                    "type": "array",
                    "items": {"type": "integer"},
                    "minItems": 2,
                    "maxItems": 2,
                    "description": "Optional [start, end] line range (1-indexed, inclusive)"
                }
            },
            "required": ["path"]
        })
    }
    
    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            path: String,
            line_range: Option<(usize, usize)>,
        }
        
        let args: Args = serde_json::from_value(args)?;
        let path = Path::new(&args.path);
        
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read file: {}", args.path))?;
            
        let total_lines = content.lines().count();
        let (start, end) = args.line_range.unwrap_or((1, total_lines));
        
        let selected_content: String = content.lines()
            .skip(start.saturating_sub(1))
            .take(end.saturating_sub(start) + 1)
            .collect::<Vec<_>>()
            .join("\n");
            
        Ok(serde_json::json!({
            "content": selected_content,
            "total_lines": total_lines,
            "truncated": args.line_range.is_some(),
            "encoding": "utf-8"
        }))
    }
}

#[async_trait]
impl Tool for FileWrite {
    fn name(&self) -> &str { "file_write" }
    
    fn description(&self) -> &str {
        "Write or overwrite entire file. Creates parent directories if needed."
    }
    
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "content": {"type": "string"},
                "backup": {"type": "boolean", "default": true}
            },
            "required": ["path", "content"]
        })
    }
    
    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            path: String,
            content: String,
            #[serde(default = "default_true")]
            backup: bool,
        }
        
        let args: Args = serde_json::from_value(args)?;
        let path = Path::new(&args.path);
        
        // Create backup if exists
        if args.backup && path.exists() {
            let backup_path = format!("{}.bak", args.path);
            fs::copy(path, &backup_path)?;
        }
        
        // Create parent directories
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        fs::write(path, &args.content)?;
        
        Ok(serde_json::json!({
            "success": true,
            "bytes_written": args.content.len(),
            "path": args.path
        }))
    }
}

#[async_trait]
impl Tool for FileEdit {
    fn name(&self) -> &str { "file_edit" }
    
    fn description(&self) -> &str {
        "Apply surgical edit to file. The old_str must match EXACTLY once. Include enough context to ensure unique match."
    }
    
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "old_str": {"type": "string", "description": "Exact string to find (must be unique)"},
                "new_str": {"type": "string", "description": "Replacement string (empty to delete)"}
            },
            "required": ["path", "old_str", "new_str"]
        })
    }
    
    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            path: String,
            old_str: String,
            new_str: String,
        }
        
        let args: Args = serde_json::from_value(args)?;
        let content = fs::read_to_string(&args.path)?;
        
        // Check for exactly one match
        let matches = content.matches(&args.old_str).count();
        if matches == 0 {
            anyhow::bail!("old_str not found in file");
        }
        if matches > 1 {
            anyhow::bail!("old_str matches {} times, expected exactly 1", matches);
        }
        
        let new_content = content.replace(&args.old_str, &args.new_str);
        fs::write(&args.path, new_content)?;
        
        Ok(serde_json::json!({
            "success": true,
            "matches_found": 1,
            "path": args.path
        }))
    }
}

#[async_trait]
impl Tool for DirectoryTree {
    fn name(&self) -> &str { "directory_tree" }
    
    fn description(&self) -> &str {
        "List directory structure. Use to understand project layout."
    }
    
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "max_depth": {"type": "integer", "default": 3},
                "include_hidden": {"type": "boolean", "default": false}
            },
            "required": ["path"]
        })
    }
    
    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            path: String,
            #[serde(default = "default_three")]
            max_depth: usize,
            #[serde(default)]
            include_hidden: bool,
        }
        
        let args: Args = serde_json::from_value(args)?;
        
        let mut entries = vec![];
        for entry in walkdir::WalkDir::new(&args.path)
            .max_depth(args.max_depth)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let metadata = entry.metadata()?;
            
            if !args.include_hidden && entry.file_name().to_str()
                .map(|s| s.starts_with('.'))
                .unwrap_or(false) {
                continue;
            }
            
            entries.push(serde_json::json!({
                "path": path.display().to_string(),
                "type": if metadata.is_dir() { "directory" } else { "file" },
                "size": metadata.len()
            }));
        }
        
        Ok(serde_json::json!({
            "root": args.path,
            "entries": entries,
            "total": entries.len()
        }))
    }
}

fn default_true() -> bool { true }
fn default_three() -> usize { 3 }
