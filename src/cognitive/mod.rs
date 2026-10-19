//! Cognitive and AI capabilities module
//!
//! This module contains AI/ML and reasoning functionality including:
//! - Cognitive state and PDVR cycle
//! - RAG (Retrieval-Augmented Generation)
//! - Learning and knowledge systems
//! - Episodic memory
//! - Knowledge graphs

pub mod intelligence;
pub mod knowledge_graph;
pub mod learning;
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
pub mod compilation_manager;
pub mod dream;
pub mod dream_subprocess;
pub mod memory_system;
#[cfg(feature = "self-improvement")]
pub mod rsi_orchestrator;

// Re-exports for backward compatibility (cognitive.rs used to export these directly)
pub use state::{
    CognitiveState, CognitiveStateBuilder, CyclePhase, EpisodicMemory, Lesson, LessonCategory,
    PlanStep, StepStatus, WorkingMemory,
};

// Re-export key types for new memory architecture
pub use dream::{
    should_run_dream, DreamConfig, DreamPhase, DreamResult, DreamState, DreamStatus, DreamTrigger,
    MemoryEntry, MemorySection, MemoryStats, MemoryStore,
};
pub use dream_subprocess::{
    check_and_spawn_autodream, get_dream_status, run_dream_consolidation, spawn_autodream,
    AutoDreamConfig, AutoDreamHandle,
};

#[cfg(feature = "consolidation")]
pub use crate::consolidation;
pub use crate::memory as hierarchical_memory;
pub use compilation_manager::{CompilationSandbox, CompileResult};
pub use intelligence::{
    DependencyGraph, FileIndex, PatternDetector, ProjectIntelligence, SymbolIndex,
};
pub use knowledge_graph::SemanticLinker;
pub use learning::{CodeExplainer, ConceptExtractor, Curriculum, ExplanationLevel};
pub use memory_system::{
    frame_untrusted_file, neutralize_data_tag, MemoryFile, MemorySystem, WorkspaceGuidanceFile,
};
#[cfg(feature = "self-improvement")]
pub use meta_learning::{MetaLearner, StrategyScore};
#[cfg(feature = "self-improvement")]
pub use metrics::{MetricsStore, PerformanceSnapshot, TerminalOutcome, TerminalRunStats};
pub use rag::{ContextBuilder, RagConfig, RagEngine, RagStats, RetrievedContext};
#[cfg(feature = "self-improvement")]
pub use rsi_orchestrator::{BenchmarkReport, CycleOutcome, RSIOrchestrator, RSIState};
pub use self_improvement::{
    ErrorPatternLearner, PromptOptimizer, SelfImprovementEngine, ToolSelectionLearner,
    UsageAnalyzer,
};
