//! Cognitive and AI capabilities module
//!
//! This module contains AI/ML and reasoning functionality including:
//! - Cognitive state and PDVR cycle
//! - RAG (Retrieval-Augmented Generation)
//! - Learning and knowledge systems
//! - Episodic memory
//! - Knowledge graphs

pub mod state;
pub mod load;
pub mod rag;
pub mod learning;
pub mod episodic;
pub mod knowledge_graph;
pub mod intelligence;
pub mod self_improvement;

// Re-exports for backward compatibility (cognitive.rs used to export these directly)
pub use state::{CognitiveState, CognitiveStateBuilder, WorkingMemory, CyclePhase, PlanStep, StepStatus, ApproachAttempt, ApproachOutcome, EpisodicMemory, Lesson, LessonCategory, Pattern};
