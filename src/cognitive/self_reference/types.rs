//! Self-Reference System Types

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Reference to the self
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SelfReference {
    Identity(String),
    Capability(String),
    State(String),
    History(String),
}

/// Level of self-reference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum ReferenceLevel {
    Direct,
    Meta,
    MetaMeta,
    Reflective,
}

/// Type of modification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModificationType {
    Add,
    Remove,
    Modify,
    Reorder,
    Conditional,
}

/// A proposed modification to the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedModification {
    pub id: u64,
    pub modification_type: ModificationType,
    pub target: String,
    pub description: String,
    pub reasoning: String,
    pub affected_capabilities: Vec<String>,
    pub risk_level: RiskLevel,
}

/// Risk level for modifications
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Introspection target
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IntrospectionTarget {
    CurrentState,
    Capabilities,
    Memory { category: String },
    Performance { metric: String },
    Decisions { since: u64 },
}

/// Result of introspection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntrospectionResult {
    pub target: IntrospectionTarget,
    pub data: serde_json::Value,
    pub timestamp: u64,
}

/// Validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub issues: Vec<String>,
    pub warnings: Vec<String>,
}

/// State of the self-reference system
#[derive(Debug, Clone)]
pub struct ReferenceState {
    pub(crate) references: Arc<RwLock<HashMap<String, SelfReference>>>,
    pub(crate) current_level: Arc<RwLock<ReferenceLevel>>,
    pub(crate) counter: Arc<AtomicU64>,
    pub(crate) history: Arc<RwLock<Vec<SelfReferenceRecord>>>,
}

impl ReferenceState {
    pub fn new() -> Self {
        Self {
            references: Arc::new(RwLock::new(HashMap::new())),
            current_level: Arc::new(RwLock::new(ReferenceLevel::Direct)),
            counter: Arc::new(AtomicU64::new(1)),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn next_id(&self) -> u64 {
        self.counter.fetch_add(1, Ordering::Relaxed)
    }

    pub async fn set_level(&self, level: ReferenceLevel) {
        let mut current = self.current_level.write().await;
        *current = level;
    }

    pub async fn get_level(&self) -> ReferenceLevel {
        *self.current_level.read().await
    }

    pub async fn record(&self, record: SelfReferenceRecord) {
        let mut history = self.history.write().await;
        history.push(record);
        if history.len() > 100 {
            history.remove(0);
        }
    }
}

impl Default for ReferenceState {
    fn default() -> Self {
        Self::new()
    }
}

/// Record of a self-reference operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfReferenceRecord {
    pub id: u64,
    pub timestamp: u64,
    pub operation: ReferenceOperation,
    pub level: ReferenceLevel,
}

/// Type of reference operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReferenceOperation {
    Introspect { target: IntrospectionTarget },
    Propose { modification: ProposedModification },
    Validate { target: String },
    Reflect { thought: String },
}

/// Metrics for self-reference operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReferenceMetrics {
    pub total_introspections: u64,
    pub total_proposals: u64,
    pub total_validations: u64,
    pub successful_modifications: u64,
    pub failed_modifications: u64,
    pub average_response_time_ms: f64,
}

/// Configuration for self-reference system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceConfig {
    pub max_reflection_depth: u32,
    pub enable_meta_reflection: bool,
    pub auto_validate: bool,
    pub risk_threshold: RiskLevel,
}

impl Default for ReferenceConfig {
    fn default() -> Self {
        Self {
            max_reflection_depth: 3,
            enable_meta_reflection: true,
            auto_validate: true,
            risk_threshold: RiskLevel::High,
        }
    }
}
