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
pub mod rag;
pub mod self_improvement;
pub mod state;

// Re-exports for backward compatibility (cognitive.rs used to export these directly)
pub use state::{
    ApproachAttempt, ApproachOutcome, CognitiveState, CognitiveStateBuilder, CyclePhase,
    EpisodicMemory, Lesson, LessonCategory, Pattern, PlanStep, StepStatus, WorkingMemory,
};
