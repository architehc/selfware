//! Conflict Detection and Resolution

use crate::tool_parser::ParsedToolCall;
use std::collections::{HashMap, HashSet};

/// Identified tool call for conflict detection
#[derive(Debug, Clone)]
pub struct ToolCallRef {
    pub id: Option<String>,
    pub name: String,
    pub arguments: serde_json::Value,
}

impl ToolCallRef {
    pub fn from_parsed(call: &ParsedToolCall, id: impl Into<String>) -> Self {
        Self {
            id: Some(id.into()),
            name: call.tool_name.clone(),
            arguments: call.arguments.clone(),
        }
    }
}

/// Conflict type between two operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictType {
    ReadWrite,
    WriteRead,
    WriteWrite,
    Unordered,
}

/// Detected conflict between two tool calls
#[derive(Debug, Clone)]
pub struct Conflict {
    pub tool_call_id_a: String,
    pub tool_call_id_b: String,
    pub conflict_type: ConflictType,
    pub resource: String,
}

/// Conflict detector for tool calls
pub struct ConflictDetector {
    read_tools: HashSet<String>,
    write_tools: HashSet<String>,
}

impl ConflictDetector {
    pub fn new() -> Self {
        let mut read_tools = HashSet::new();
        read_tools.insert("file_read".to_string());
        read_tools.insert("directory_list".to_string());
        read_tools.insert("git_status".to_string());
        read_tools.insert("git_log".to_string());

        let mut write_tools = HashSet::new();
        write_tools.insert("file_write".to_string());
        write_tools.insert("file_edit".to_string());
        write_tools.insert("directory_create".to_string());
        write_tools.insert("directory_remove".to_string());
        write_tools.insert("git_commit".to_string());
        write_tools.insert("git_push".to_string());
        write_tools.insert("shell_exec".to_string());

        Self {
            read_tools,
            write_tools,
        }
    }

    pub fn detect_conflicts(&self, calls: &[ToolCallRef]) -> Vec<Conflict> {
        let mut conflicts = Vec::new();
        let resource_map = self.build_resource_map(calls);

        for (resource, call_ids) in &resource_map {
            if call_ids.len() < 2 {
                continue;
            }

            for i in 0..call_ids.len() {
                for j in (i + 1)..call_ids.len() {
                    let (id_a, tool_a) = &call_ids[i];
                    let (id_b, tool_b) = &call_ids[j];

                    if let Some(conflict_type) = self.check_conflict(tool_a, tool_b) {
                        conflicts.push(Conflict {
                            tool_call_id_a: id_a.clone(),
                            tool_call_id_b: id_b.clone(),
                            conflict_type,
                            resource: resource.clone(),
                        });
                    }
                }
            }
        }

        conflicts
    }

    fn build_resource_map(&self, calls: &[ToolCallRef]) -> HashMap<String, Vec<(String, String)>> {
        let mut map: HashMap<String, Vec<(String, String)>> = HashMap::new();

        for (idx, call) in calls.iter().enumerate() {
            let resources = self.extract_resources(call);
            let call_id = call.id.clone().unwrap_or_else(|| format!("call_{}", idx));
            for resource in resources {
                map.entry(resource)
                    .or_default()
                    .push((call_id.clone(), call.name.clone()));
            }
        }

        map
    }

    fn extract_resources(&self, call: &ToolCallRef) -> Vec<String> {
        let mut resources = Vec::new();

        match call.name.as_str() {
            "file_read" | "file_write" | "file_edit" | "file_remove" => {
                if let Some(path) = call.arguments.get("path").and_then(|v| v.as_str()) {
                    resources.push(path.to_string());
                }
            }
            "directory_list" | "directory_create" | "directory_remove" => {
                if let Some(path) = call.arguments.get("path").and_then(|v| v.as_str()) {
                    resources.push(path.to_string());
                }
            }
            "shell_exec" => {
                resources.push("*".to_string());
            }
            _ => {}
        }

        resources
    }

    fn check_conflict(&self, tool_a: &str, tool_b: &str) -> Option<ConflictType> {
        let a_reads = self.read_tools.contains(tool_a);
        let a_writes = self.write_tools.contains(tool_a);
        let b_reads = self.read_tools.contains(tool_b);
        let b_writes = self.write_tools.contains(tool_b);

        match (a_reads, a_writes, b_reads, b_writes) {
            (true, _, true, _) => None,
            (true, _, _, true) => Some(ConflictType::ReadWrite),
            (_, true, true, _) => Some(ConflictType::WriteRead),
            (_, true, _, true) => Some(ConflictType::WriteWrite),
            _ => None,
        }
    }

    pub fn add_read_tool(&mut self, tool_name: &str) {
        self.read_tools.insert(tool_name.to_string());
    }

    pub fn add_write_tool(&mut self, tool_name: &str) {
        self.write_tools.insert(tool_name.to_string());
    }

    pub fn is_read_tool(&self, tool_name: &str) -> bool {
        self.read_tools.contains(tool_name)
    }

    pub fn is_write_tool(&self, tool_name: &str) -> bool {
        self.write_tools.contains(tool_name)
    }
}

impl Default for ConflictDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Conflict resolver strategies
pub struct ConflictResolver;

impl ConflictResolver {
    pub fn resolve_by_ordering(conflicts: &[Conflict], calls: &mut [ToolCallRef]) -> anyhow::Result<()> {
        if conflicts.is_empty() {
            return Ok(());
        }

        for conflict in conflicts {
            let idx_a = calls.iter().position(|c| c.id == Some(conflict.tool_call_id_a.clone()));
            let idx_b = calls.iter().position(|c| c.id == Some(conflict.tool_call_id_b.clone()));

            if let (Some(a), Some(b)) = (idx_a, idx_b) {
                match conflict.conflict_type {
                    ConflictType::ReadWrite | ConflictType::Unordered => {
                        if b < a {
                            calls.swap(a, b);
                        }
                    }
                    ConflictType::WriteRead | ConflictType::WriteWrite => {
                        if a > b {
                            calls.swap(a, b);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn group_by_resource(conflicts: &[Conflict]) -> HashMap<String, Vec<Conflict>> {
        let mut groups: HashMap<String, Vec<Conflict>> = HashMap::new();
        for conflict in conflicts {
            groups.entry(conflict.resource.clone()).or_default().push(conflict.clone());
        }
        groups
    }
}
