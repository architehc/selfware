use crate::swl::types::schema::StateSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SwlDocument {
    pub version: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub metadata: Option<DocumentMetadata>,
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
    pub state: Option<StateSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentMetadata {
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentDefinition {
    pub model: ModelSpec,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub instruction: Option<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub output_key: Option<String>,
    #[serde(default)]
    pub sub_agents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ModelSpec {
    Simple(String),
    Detailed(ModelConfig),
}

impl ModelSpec {
    pub fn name(&self) -> &str {
        match self {
            ModelSpec::Simple(name) => name,
            ModelSpec::Detailed(config) => config.name.as_str(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelConfig {
    #[serde(default)]
    pub provider: Option<String>,
    pub name: String,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowDefinition {
    #[serde(rename = "type")]
    pub workflow_type: WorkflowType,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub steps: Vec<WorkflowStep>,
    #[serde(default)]
    pub map: Option<MapStage>,
    #[serde(default)]
    pub reduce: Option<ReduceStage>,
    #[serde(default)]
    pub merge: Option<AggregateStage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkflowType {
    #[serde(rename = "sequential")]
    Sequential,
    #[serde(rename = "parallel")]
    Parallel,
    #[serde(rename = "map_reduce", alias = "map-reduce")]
    MapReduce,
    #[serde(rename = "conditional")]
    Conditional,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowStep {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub delegate: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub input: Option<serde_yaml::Value>,
    #[serde(default)]
    pub parallel: Option<ParallelStage>,
    #[serde(default)]
    pub guard: Option<Guardrail>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParallelStage {
    #[serde(default)]
    pub branches: Vec<WorkflowStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MapStage {
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub input: Option<serde_yaml::Value>,
    #[serde(default)]
    pub parallel: Option<ParallelStage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ReduceStage {
    Code(CodeBlock),
    Aggregate(AggregateStage),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AggregateStage {
    pub agent: String,
    #[serde(default)]
    pub instruction: Option<String>,
    #[serde(default)]
    pub inputs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeBlock {
    pub language: CodeLanguage,
    #[serde(alias = "content")]
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CodeLanguage {
    #[serde(rename = "rust")]
    Rust,
    #[serde(rename = "python")]
    Python,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Guardrail {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, rename = "type")]
    pub guardrail_type: Option<String>,
    pub condition: GuardCondition,
    pub on_violation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum GuardCondition {
    Inline(String),
    Code(CodeBlock),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryConfig {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub traces: Vec<String>,
    #[serde(default)]
    pub metrics: Vec<String>,
    #[serde(default)]
    pub export: Option<TelemetryExport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryExport {
    #[serde(rename = "type")]
    pub export_type: String,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardConfig {
    #[serde(default)]
    pub layout: Option<String>,
    #[serde(default)]
    pub refresh: Option<String>,
}

#[cfg(test)]
#[path = "../../../tests/unit/swl/parser/ast/ast_test.rs"]
mod tests;
