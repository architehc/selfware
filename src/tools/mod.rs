use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

pub mod cargo;
pub mod file;
pub mod git;
pub mod shell;

use cargo::{CargoCheck, CargoClippy, CargoFmt, CargoTest};
use file::{DirectoryTree, FileEdit, FileRead, FileWrite};
use git::{GitCheckpoint, GitCommit, GitDiff, GitStatus};
use shell::ShellExec;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> Value;
    async fn execute(&self, args: Value) -> Result<Value>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
        };
        
        // File operations
        registry.register(FileRead);
        registry.register(FileWrite);
        registry.register(FileEdit);
        registry.register(DirectoryTree);
        
        // Git operations
        registry.register(GitStatus);
        registry.register(GitDiff);
        registry.register(GitCommit);
        registry.register(GitCheckpoint);
        
        // Cargo/Build operations
        registry.register(CargoTest);
        registry.register(CargoCheck);
        registry.register(CargoClippy);
        registry.register(CargoFmt);
        
        // System operations
        registry.register(ShellExec);
        
        registry
    }

    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.name().to_string(), Box::new(tool));
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    pub fn list(&self) -> Vec<&dyn Tool> {
        self.tools.values().map(|t| t.as_ref()).collect()
    }

    pub fn definitions(&self) -> Vec<crate::api::types::ToolDefinition> {
        self.tools.values().map(|tool| {
            crate::api::types::ToolDefinition {
                def_type: "function".to_string(),
                function: crate::api::types::FunctionDefinition {
                    name: tool.name().to_string(),
                    description: tool.description().to_string(),
                    parameters: tool.schema(),
                },
            }
        }).collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
