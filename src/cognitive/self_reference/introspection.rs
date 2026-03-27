//! Self-Introspection Capabilities

use super::types::{
    IntrospectionResult, IntrospectionTarget, ReferenceConfig, ReferenceLevel, ReferenceState,
    SelfReference,
};
use crate::cognitive::memory_hierarchy::types::{MemoryQuery, MemoryTier};
use crate::cognitive::memory_hierarchy::HierarchicalMemory;

/// Introspection engine for self-examination
pub struct IntrospectionEngine {
    state: ReferenceState,
    config: ReferenceConfig,
}

impl IntrospectionEngine {
    pub fn new(state: ReferenceState, config: ReferenceConfig) -> Self {
        Self { state, config }
    }

    pub fn with_default_config(state: ReferenceState) -> Self {
        Self {
            state,
            config: ReferenceConfig::default(),
        }
    }

    /// Perform introspection on a target
    pub async fn introspect(
        &self,
        target: IntrospectionTarget,
    ) -> anyhow::Result<IntrospectionResult> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let data = match &target {
            IntrospectionTarget::CurrentState => {
                serde_json::json!({
                    "level": self.state.get_level().await,
                    "reference_count": self.state.references.read().await.len(),
                    "history_length": self.state.history.read().await.len(),
                })
            }
            IntrospectionTarget::Capabilities => {
                serde_json::json!({
                    "introspection": true,
                    "modification": true,
                    "validation": true,
                    "reflection": self.config.enable_meta_reflection,
                    "max_depth": self.config.max_reflection_depth,
                })
            }
            IntrospectionTarget::Memory { category } => {
                serde_json::json!({
                    "category": category,
                    "note": "Use memory_hierarchy directly for full access",
                })
            }
            IntrospectionTarget::Performance { metric } => {
                serde_json::json!({
                    "metric": metric,
                    "status": "operational",
                })
            }
            IntrospectionTarget::Decisions { since } => {
                let history = self.state.history.read().await;
                let recent: Vec<_> = history
                    .iter()
                    .filter(|r| r.timestamp >= *since)
                    .map(|r| serde_json::to_value(r).unwrap_or_default())
                    .collect();
                serde_json::json!({ "decisions": recent })
            }
        };

        Ok(IntrospectionResult {
            target,
            data,
            timestamp,
        })
    }

    /// Reflect on self at a given level
    pub async fn reflect(
        &self,
        thought: &str,
        level: ReferenceLevel,
    ) -> anyhow::Result<Reflection> {
        let current = self.state.get_level().await;

        if level > current && !self.can_elevate(level).await {
            return Err(anyhow::anyhow!(
                "Cannot elevate to {:?} from {:?}",
                level,
                current
            ));
        }

        let reflection = Reflection {
            thought: thought.to_string(),
            level,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs(),
            meta_thought: if self.config.enable_meta_reflection && level >= ReferenceLevel::Meta {
                Some(format!("Reflecting on: {}", thought))
            } else {
                None
            },
        };

        self.state.set_level(level).await;

        Ok(reflection)
    }

    async fn can_elevate(&self, target: ReferenceLevel) -> bool {
        let current = self.state.get_level().await;
        let diff = target as i32 - current as i32;
        diff <= self.config.max_reflection_depth as i32
    }

    /// Query memory for context
    pub async fn query_memory(
        &self,
        memory: &HierarchicalMemory,
        query: &str,
    ) -> anyhow::Result<Vec<String>> {
        let mem_query = MemoryQuery::new(query).with_tier(MemoryTier::Working);
        let results = memory.query(mem_query).await;
        Ok(results.into_iter().map(|r| r.content).collect())
    }

    /// Get current self-reference
    pub async fn get_self(&self) -> Option<SelfReference> {
        self.state.get_reference("self").await
    }

    /// List all references
    pub async fn list_references(&self) -> Vec<(String, SelfReference)> {
        let refs = self.state.references.read().await;
        refs.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }
}

/// A reflection result
#[derive(Debug, Clone)]
pub struct Reflection {
    pub thought: String,
    pub level: ReferenceLevel,
    pub timestamp: u64,
    pub meta_thought: Option<String>,
}

impl Reflection {
    pub fn depth(&self) -> u32 {
        match self.level {
            ReferenceLevel::Direct => 0,
            ReferenceLevel::Meta => 1,
            ReferenceLevel::MetaMeta => 2,
            ReferenceLevel::Reflective => 3,
        }
    }

    pub fn is_meta(&self) -> bool {
        self.meta_thought.is_some()
    }
}
