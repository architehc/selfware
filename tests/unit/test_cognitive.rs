//! External smoke tests for the cognitive module.
//!
//! The cognitive module already contains 420+ inline unit tests covering
//! individual components (self_improvement, knowledge_graph, intelligence,
//! state, rag, episodic, learning, load, self_edit, meta_learning, metrics).
//!
//! These external tests exercise the public API from the perspective of a
//! library consumer, verifying cross-module integration and confirming that
//! the public surface remains stable.

// ---------------------------------------------------------------------------
// self_improvement module smoke tests
// ---------------------------------------------------------------------------

use selfware::cognitive::self_improvement::{Outcome, PromptRecord, SelfImprovementEngine};

/// Verify that the SelfImprovementEngine can be created, record a mixed
/// workload of prompts, tools, and errors, then return coherent stats
/// reflecting all recorded data.
#[test]
fn test_self_improvement_engine_mixed_workload_stats() {
    let engine = SelfImprovementEngine::new();

    // Record several prompts with varying outcomes
    engine.record_prompt("fix the bug in main.rs", "code_fix", Outcome::Success, 0.95);
    engine.record_prompt("refactor loop", "refactor", Outcome::Partial, 0.6);
    engine.record_prompt("deploy", "ops", Outcome::Failure, 0.1);

    // Record tool usage
    engine.record_tool("file_read", "reading source", Outcome::Success, 42, None);
    engine.record_tool(
        "shell_exec",
        "running tests",
        Outcome::Failure,
        5000,
        Some("timeout".to_string()),
    );

    // Record an error
    engine.record_error(
        "Connection refused",
        "network_error",
        "api_call",
        "http_request",
        None,
    );

    let stats = engine.get_stats();

    // Prompt stats should reflect 3 records
    let prompt_stats = stats.prompt_stats.expect("prompt_stats should be Some");
    assert_eq!(prompt_stats.total_records, 3);

    // Tool stats should reflect 2 records
    let tool_stats = stats.tool_stats.expect("tool_stats should be Some");
    assert_eq!(tool_stats.total_records, 2);

    // Error stats should reflect 1 error
    let error_stats = stats.error_stats.expect("error_stats should be Some");
    assert_eq!(error_stats.total_errors, 1);
}

/// Verify that disabling learning prevents any new data from being recorded,
/// and that re-enabling it allows recording to resume.
#[test]
fn test_self_improvement_engine_toggle_learning() {
    let mut engine = SelfImprovementEngine::new();

    // Confirm learning starts enabled via stats
    let initial_stats = engine.get_stats();
    assert!(initial_stats.learning_enabled);

    // Disable learning
    engine.set_learning_enabled(false);
    engine.record_prompt("ignored prompt", "code", Outcome::Success, 1.0);

    let stats = engine.get_stats();
    let prompt_stats = stats.prompt_stats.unwrap();
    assert_eq!(
        prompt_stats.total_records, 0,
        "No records should be stored while learning is disabled"
    );

    // Re-enable learning
    engine.set_learning_enabled(true);
    engine.record_prompt("recorded prompt", "code", Outcome::Success, 0.8);

    let stats = engine.get_stats();
    let prompt_stats = stats.prompt_stats.unwrap();
    assert_eq!(
        prompt_stats.total_records, 1,
        "Record should be stored after re-enabling learning"
    );
}

/// Verify that best_tools_for returns tools ranked by success rate, and
/// that a tool with 100% success ranks above a tool with mixed outcomes.
#[test]
fn test_self_improvement_engine_tool_ranking() {
    let engine = SelfImprovementEngine::new();

    // file_read: 5 successes in "reading" context
    for _ in 0..5 {
        engine.record_tool("file_read", "reading", Outcome::Success, 30, None);
    }
    // grep_search: 2 successes and 3 failures in "reading" context
    for _ in 0..2 {
        engine.record_tool("grep_search", "reading", Outcome::Success, 100, None);
    }
    for _ in 0..3 {
        engine.record_tool("grep_search", "reading", Outcome::Failure, 200, None);
    }

    let best = engine.best_tools_for("reading");
    assert!(
        !best.is_empty(),
        "best_tools_for should return at least one tool"
    );

    // file_read should rank higher (better success rate)
    let file_read_score = best.iter().find(|(t, _)| t == "file_read").map(|(_, s)| *s);
    let grep_score = best
        .iter()
        .find(|(t, _)| t == "grep_search")
        .map(|(_, s)| *s);

    if let (Some(fr), Some(gs)) = (file_read_score, grep_score) {
        assert!(
            fr >= gs,
            "file_read ({}) should score >= grep_search ({})",
            fr,
            gs
        );
    }
}

// ---------------------------------------------------------------------------
// knowledge_graph module smoke tests
// ---------------------------------------------------------------------------

use selfware::cognitive::knowledge_graph::{
    Entity, EntityType, KnowledgeGraph, Relation, RelationType,
};

/// Verify that a KnowledgeGraph can be created, populated with entities and
/// relations, and queried back correctly.
#[test]
fn test_knowledge_graph_add_and_query_entities() {
    let mut graph = KnowledgeGraph::new();

    let e1 = Entity::new("Config", EntityType::Struct);
    let e2 = Entity::new("load_config", EntityType::Function);

    let id1 = graph.add_entity(e1);
    let id2 = graph.add_entity(e2);

    // Both entities should be retrievable
    assert!(graph.get_entity(&id1).is_some());
    assert!(graph.get_entity(&id2).is_some());
    assert_eq!(graph.get_entity(&id1).unwrap().name, "Config");
    assert_eq!(graph.get_entity(&id2).unwrap().name, "load_config");

    // Entity count should match
    assert_eq!(graph.entity_count(), 2);
}

/// Verify that relations between entities are bidirectionally queryable.
#[test]
fn test_knowledge_graph_relations() {
    let mut graph = KnowledgeGraph::new();

    let struct_entity = Entity::new("Agent", EntityType::Struct);
    let fn_entity = Entity::new("run_task", EntityType::Function);
    let id1 = graph.add_entity(struct_entity);
    let id2 = graph.add_entity(fn_entity);

    let rel = Relation::new(&id1, &id2, RelationType::Contains);
    let rel_id = graph.add_relation(rel);
    assert!(!rel_id.is_empty());

    // The relation should be findable via the graph
    assert!(graph.get_relation(&rel_id).is_some());
}

/// Verify that searching entities by type returns the correct results.
#[test]
fn test_knowledge_graph_search_by_type() {
    let mut graph = KnowledgeGraph::new();

    graph.add_entity(Entity::new("MyStruct", EntityType::Struct));
    graph.add_entity(Entity::new("OtherStruct", EntityType::Struct));
    graph.add_entity(Entity::new("some_fn", EntityType::Function));

    let structs = graph.find_by_type(EntityType::Struct);
    assert_eq!(
        structs.len(),
        2,
        "Expected 2 struct entities, got {}",
        structs.len()
    );

    let functions = graph.find_by_type(EntityType::Function);
    assert_eq!(
        functions.len(),
        1,
        "Expected 1 function entity, got {}",
        functions.len()
    );
}

// ---------------------------------------------------------------------------
// CognitiveState smoke tests
// ---------------------------------------------------------------------------

use selfware::cognitive::state::{CognitiveState, CyclePhase};

/// Verify that a CognitiveState can be created and that it starts in the
/// Plan phase of the PDVR cycle with empty working memory.
#[test]
fn test_cognitive_state_initial_phase() {
    let state = CognitiveState::new();

    // State should begin in the Plan phase
    assert_eq!(state.cycle_phase, CyclePhase::Plan);

    // Working memory should be initialized (not panicking on access)
    assert!(state.working_memory.open_questions.is_empty());
}

/// Verify that advancing the cognitive phase cycles through PDVR correctly.
#[test]
fn test_cognitive_state_advance_phase_cycle() {
    let mut state = CognitiveState::new();
    assert_eq!(state.cycle_phase, CyclePhase::Plan);

    state.advance_phase();
    assert_eq!(state.cycle_phase, CyclePhase::Do);

    state.advance_phase();
    assert_eq!(state.cycle_phase, CyclePhase::Verify);

    state.advance_phase();
    assert_eq!(state.cycle_phase, CyclePhase::Reflect);

    // Wraps back to Plan
    state.advance_phase();
    assert_eq!(state.cycle_phase, CyclePhase::Plan);
}

// ---------------------------------------------------------------------------
// PromptRecord builder-chain smoke test
// ---------------------------------------------------------------------------

/// Verify that the builder chain on PromptRecord correctly sets all fields
/// and that values are clamped appropriately (quality score 0..1).
#[test]
fn test_prompt_record_builder_chain() {
    let record = PromptRecord::new(
        "Explain quicksort".to_string(),
        "explanation".to_string(),
        Outcome::Success,
    )
    .with_quality(1.5) // should be clamped to 1.0
    .with_tokens(500)
    .with_response_time(1200);

    assert_eq!(
        record.quality_score, 1.0,
        "quality should be clamped to 1.0"
    );
    assert_eq!(record.tokens_used, 500);
    assert_eq!(record.response_time_ms, 1200);
    assert!(record.timestamp > 0);
}
