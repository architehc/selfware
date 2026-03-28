use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwlDocument {
    pub version: String,
    pub name: String,
    #[serde(default)]
    pub agents: BTreeMap<String, AgentDefinition>,
    #[serde(default)]
    pub workflows: BTreeMap<String, WorkflowDefinition>,
    #[serde(default)]
    pub guardrails: Vec<Guardrail>,
    #[serde(default)]
    pub telemetry: Option<TelemetryConfig>,
    #[serde(default)]
    pub dashboard: Option<DashboardConfig>,
    #[serde(default)]
    pub state: Option<StateConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentDefinition {
    pub model: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub instruction: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowDefinition {
    #[serde(rename = "type")]
    pub workflow_type: WorkflowType,
    #[serde(default)]
    pub map: Option<MapStage>,
    #[serde(default)]
    pub reduce: Option<CodeBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkflowType {
    #[serde(rename = "sequential")]
    Sequential,
    #[serde(rename = "parallel")]
    Parallel,
    #[serde(rename = "map-reduce")]
    MapReduce,
    #[serde(rename = "conditional")]
    Conditional,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MapStage {
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub input: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeBlock {
    pub language: CodeLanguage,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CodeLanguage {
    #[serde(rename = "rust")]
    Rust,
    #[serde(rename = "python")]
    Python,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Guardrail {
    pub name: String,
    #[serde(rename = "type")]
    pub guardrail_type: String,
    pub condition: String,
    pub on_violation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryConfig {
    #[serde(default)]
    pub traces: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardConfig {
    #[serde(default)]
    pub layout: Option<String>,
    #[serde(default)]
    pub refresh: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateConfig {
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub ttl: Option<String>,
}
