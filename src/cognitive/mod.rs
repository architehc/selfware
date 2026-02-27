//! Cognitive and AI capabilities module
//!
//! This module contains AI/ML and reasoning functionality including:
//! - Cognitive state and PDVR cycle
//! - RAG (Retrieval-Augmented Generation)
//! - Learning and knowledge systems
//! - Episodic memory
//! - Knowledge graphs

pub mod episodic;
pub mod intelligence;
pub mod knowledge_graph;
pub mod learning;
pub mod load;
#[cfg(feature = "self-improvement")]
pub mod meta_learning;
#[cfg(feature = "self-improvement")]
pub mod metrics;
pub mod rag;
#[cfg(feature = "self-improvement")]
pub mod self_edit;
pub mod self_improvement;
pub mod state;

// New modules for 1M context support
pub mod memory_hierarchy;
pub mod token_budget;
pub mod self_reference;
pub mod cognitive_system;
pub mod compilation_manager;
pub mod rsi_orchestrator;

// Re-exports for backward compatibility (cognitive.rs used to export these directly)
pub use state::{
    ApproachAttempt, ApproachOutcome, CognitiveState, CognitiveStateBuilder, CyclePhase,
    EpisodicMemory, Lesson, LessonCategory, Pattern, PlanStep, StepStatus, WorkingMemory,
};

// Re-export key types for new memory architecture
pub use memory_hierarchy::{
    HierarchicalMemory, TokenBudget, TOTAL_CONTEXT_TOKENS,
    Episode, Importance, EpisodeType,
};
pub use token_budget::{
    TokenBudgetAllocator, TaskType,
};
pub use self_reference::{
    SelfReferenceSystem, SelfImprovementContext, SelfModel,
};
pub use cognitive_system::{
    CognitiveSystem, LlmContext, ContextBuildOptions,
};
