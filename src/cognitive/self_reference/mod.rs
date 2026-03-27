//! Self-Reference System
//!
//! This module provides capabilities for the system to introspect on itself,
//! understand its own state, capabilities, and propose modifications.

use std::sync::Arc;
use tokio::sync::RwLock;

pub mod introspection;
pub mod modification;
pub mod types;

pub use introspection::{IntrospectionEngine, Reflection};
pub use modification::{ModificationBuilder, ModificationEngine, ModificationResult};
pub use types::{
    IntrospectionResult, IntrospectionTarget, ModificationType, ProposedModification,
    ReferenceConfig, ReferenceLevel, ReferenceMetrics, ReferenceOperation, ReferenceState,
    RiskLevel, SelfReference, SelfReferenceRecord, ValidationResult,
};

use crate::cognitive::memory_hierarchy::{SelfModel, SemanticMemory};

/// Options for retrieving source code
#[derive(Debug, Clone, Default)]
pub struct SourceRetrievalOptions {
    pub max_files: Option<usize>,
    pub max_tokens: Option<usize>,
}

/// Code modification tracking
#[derive(Debug, Clone)]
pub struct ModificationTracker {
    modifications: Vec<crate::cognitive::memory_hierarchy::CodeModification>,
}

impl ModificationTracker {
    pub fn new() -> Self {
        Self {
            modifications: Vec::new(),
        }
    }

    pub fn track(&mut self, modification: crate::cognitive::memory_hierarchy::CodeModification) {
        self.modifications.push(modification);
    }

    pub fn get_recent(
        &self,
        limit: usize,
    ) -> Vec<&crate::cognitive::memory_hierarchy::CodeModification> {
        self.modifications
            .iter()
            .rev()
            .take(limit)
            .collect::<Vec<_>>()
    }
}

impl Default for ModificationTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Main self-reference system combining all capabilities
pub struct SelfReferenceSystem {
    pub state: ReferenceState,
    pub introspection: IntrospectionEngine,
    pub modification: ModificationEngine,
    pub self_model: Arc<RwLock<SelfModel>>,
    pub semantic: Arc<RwLock<SemanticMemory>>,
    pub selfware_path: std::path::PathBuf,
    pub modification_tracker: ModificationTracker,
}

impl SelfReferenceSystem {
    /// Create a new self-reference system
    pub fn new(semantic: Arc<RwLock<SemanticMemory>>, selfware_path: std::path::PathBuf) -> Self {
        let state = ReferenceState::new();
        let config = ReferenceConfig::default();
        let introspection = IntrospectionEngine::new(state.clone(), config.clone());
        let modification = ModificationEngine::new(state.clone(), config);
        let self_model = Arc::new(RwLock::new(SelfModel {
            version: "0.2.2".to_string(),
            capabilities: Vec::new(),
            limitations: Vec::new(),
            recent_changes: Vec::new(),
            modules: Vec::new(),
        }));

        Self {
            state,
            introspection,
            modification,
            self_model,
            semantic,
            selfware_path,
            modification_tracker: ModificationTracker::new(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: ReferenceConfig) -> Self {
        let state = ReferenceState::new();
        let introspection = IntrospectionEngine::new(state.clone(), config.clone());
        let modification = ModificationEngine::new(state.clone(), config);
        let self_model = Arc::new(RwLock::new(SelfModel {
            version: "0.2.2".to_string(),
            capabilities: Vec::new(),
            limitations: Vec::new(),
            recent_changes: Vec::new(),
            modules: Vec::new(),
        }));

        Self {
            state,
            introspection,
            modification,
            self_model,
            semantic: Arc::new(RwLock::new(SemanticMemory::new())),
            selfware_path: std::env::current_dir().unwrap_or_default(),
            modification_tracker: ModificationTracker::new(),
        }
    }

    /// Get current reference level
    pub async fn current_level(&self) -> ReferenceLevel {
        self.state.get_level().await
    }

    /// Set reference level
    pub async fn set_level(&self, level: ReferenceLevel) {
        self.state.set_level(level).await;
    }

    /// Initialize the self-model
    pub async fn initialize_self_model(&mut self) -> anyhow::Result<()> {
        let mut model = self.self_model.write().await;
        model.capabilities = vec![
            "code_analysis".to_string(),
            "memory_management".to_string(),
            "self_reflection".to_string(),
        ];
        Ok(())
    }

    /// Get improvement context for self-improvement
    pub async fn get_improvement_context(
        &self,
        goal: &str,
        max_tokens: usize,
    ) -> anyhow::Result<crate::cognitive::memory_hierarchy::SelfImprovementContext> {
        let self_model = self.self_model.read().await;
        let code_context = self
            .semantic
            .read()
            .await
            .retrieve_code_context(goal, max_tokens, true)
            .await?;

        Ok(crate::cognitive::memory_hierarchy::SelfImprovementContext {
            goal: goal.to_string(),
            self_model: format!(
                "Version: {}, Capabilities: {:?}",
                self_model.version, self_model.capabilities
            ),
            architecture: "Hierarchical memory system with self-reference".to_string(),
            recent_modifications: self
                .get_recent_modifications()
                .iter()
                .map(|m| format!("{:?}", m))
                .collect::<Vec<_>>()
                .join(", "),
            relevant_code: code_context,
            suggestions: Vec::new(),
        })
    }

    /// Read own source code
    pub async fn read_own_code(
        &self,
        module_path: &str,
        _options: &SourceRetrievalOptions,
    ) -> anyhow::Result<String> {
        let path = self.selfware_path.join("src").join(module_path);
        tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path.display(), e))
    }

    /// Track a code modification
    pub fn track_modification(
        &mut self,
        modification: crate::cognitive::memory_hierarchy::CodeModification,
    ) {
        self.modification_tracker.track(modification);
    }

    /// Get the self-model
    pub fn get_self_model(&self) -> SelfModel {
        // Since we can't block on async, return a default/empty model
        // The actual implementation should properly handle this
        SelfModel {
            version: "0.2.2".to_string(),
            capabilities: Vec::new(),
            limitations: Vec::new(),
            recent_changes: Vec::new(),
            modules: Vec::new(),
        }
    }

    /// Get recent modifications
    pub fn get_recent_modifications(
        &self,
    ) -> Vec<&crate::cognitive::memory_hierarchy::CodeModification> {
        self.modification_tracker.get_recent(10)
    }
}

impl Default for SelfReferenceSystem {
    fn default() -> Self {
        Self::new(
            Arc::new(RwLock::new(SemanticMemory::new())),
            std::env::current_dir().unwrap_or_default(),
        )
    }
}

/// Re-export commonly used types
pub mod prelude {
    pub use super::{
        IntrospectionEngine, IntrospectionTarget, ModificationEngine, ModificationType,
        ProposedModification, ReferenceConfig, ReferenceLevel, RiskLevel, SelfReference,
        SelfReferenceSystem, SourceRetrievalOptions,
    };
}
