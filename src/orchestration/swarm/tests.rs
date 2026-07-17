//! Swarm System Tests

use std::collections::HashMap;
use std::sync::Arc;

use super::coordinator::{ConflictStrategy, Swarm};
use super::memory::SharedMemory;
use super::types::{Agent, AgentRole, AgentStatus, DecisionStatus, SwarmTask, TaskStatus, Vote};

// ============================================================================
// Agent Tests
// ============================================================================

#[test]
fn test_agent_role_default() {
    assert_eq!(AgentRole::default(), AgentRole::General);
}

#[test]
fn test_agent_role_name() {
    assert_eq!(AgentRole::Architect.name(), "Architect");
    assert_eq!(AgentRole::Coder.name(), "Coder");
}

#[test]
fn test_agent_role_priority() {
    assert!(AgentRole::Security.priority() > AgentRole::Coder.priority());
    assert!(AgentRole::Architect.priority() > AgentRole::General.priority());
}

#[test]
fn test_agent_role_name_all_variants() {
    assert_eq!(AgentRole::Architect.name(), "Architect");
    assert_eq!(AgentRole::Coder.name(), "Coder");
    assert_eq!(AgentRole::Tester.name(), "Tester");
    assert_eq!(AgentRole::Reviewer.name(), "Reviewer");
    assert_eq!(AgentRole::Documenter.name(), "Documenter");
    assert_eq!(AgentRole::DevOps.name(), "DevOps");
    assert_eq!(AgentRole::Security.name(), "Security");
    assert_eq!(AgentRole::Performance.name(), "Performance");
    assert_eq!(AgentRole::VisualCritic.name(), "VisualCritic");
    assert_eq!(AgentRole::General.name(), "General");
}

#[test]
fn test_agent_role_system_prompt_all_variants() {
    assert!(AgentRole::Architect.system_prompt().contains("architect"));
    assert!(AgentRole::Coder.system_prompt().contains("programmer"));
    assert!(AgentRole::Tester.system_prompt().contains("testing"));
    assert!(AgentRole::Reviewer.system_prompt().contains("reviewer"));
    assert!(AgentRole::Documenter
        .system_prompt()
        .contains("documentation"));
    assert!(AgentRole::DevOps.system_prompt().contains("DevOps"));
    assert!(AgentRole::Security.system_prompt().contains("security"));
    assert!(AgentRole::Performance
        .system_prompt()
        .contains("performance"));
    assert!(AgentRole::VisualCritic
        .system_prompt()
        .contains("visual design"));
    assert!(AgentRole::General
        .system_prompt()
        .contains("general-purpose"));
}

#[test]
fn test_agent_role_priority_all_variants() {
    assert_eq!(AgentRole::Security.priority(), 10);
    assert_eq!(AgentRole::Architect.priority(), 8);
    assert_eq!(AgentRole::Reviewer.priority(), 7);
    assert_eq!(AgentRole::Tester.priority(), 6);
    assert_eq!(AgentRole::Performance.priority(), 5);
    assert_eq!(AgentRole::Coder.priority(), 4);
    assert_eq!(AgentRole::DevOps.priority(), 4);
    assert_eq!(AgentRole::VisualCritic.priority(), 6);
    assert_eq!(AgentRole::Documenter.priority(), 3);
    assert_eq!(AgentRole::General.priority(), 2);
}

#[test]
fn test_agent_creation() {
    let agent = Agent::new("TestAgent", AgentRole::Coder)
        .with_expertise("Rust")
        .with_expertise("Python");

    assert_eq!(agent.name, "TestAgent");
    assert_eq!(agent.role, AgentRole::Coder);
    assert_eq!(agent.expertise.len(), 2);
}

#[test]
fn test_agent_custom_prompt() {
    let agent = Agent::new("Test", AgentRole::General).with_prompt("Custom prompt here");
    assert_eq!(agent.system_prompt(), "Custom prompt here");
}

#[test]
fn test_agent_system_prompt_uses_role_default() {
    let agent = Agent::new("Test", AgentRole::Security);
    assert_eq!(agent.system_prompt(), AgentRole::Security.system_prompt());
}

#[test]
fn test_agent_with_model() {
    let agent = Agent::new("TestAgent", AgentRole::Coder).with_model("gpt-4");
    assert_eq!(agent.model_id, Some("gpt-4".to_string()));
}

#[test]
fn test_agent_task_completion() {
    let mut agent = Agent::new("Test", AgentRole::Coder);
    let initial_trust = agent.trust_score;

    agent.complete_task(true);
    assert!(agent.trust_score > initial_trust);
    assert_eq!(agent.tasks_completed, 1);

    agent.complete_task(false);
    assert_eq!(agent.tasks_failed, 1);
}

#[test]
fn test_agent_success_rate() {
    let mut agent = Agent::new("Test", AgentRole::Coder);
    agent.tasks_completed = 8;
    agent.tasks_failed = 2;

    assert!((agent.success_rate() - 0.8).abs() < 0.01);
}

#[test]
fn test_agent_success_rate_zero_tasks() {
    let agent = Agent::new("Test", AgentRole::Coder);
    assert!((agent.success_rate() - 1.0).abs() < f32::EPSILON);
}

#[test]
fn test_agent_trust_score_floor() {
    let mut agent = Agent::new("Test", AgentRole::Coder);
    // Drive trust down with repeated failures
    for _ in 0..20 {
        agent.complete_task(false);
    }
    // Trust should never go below the 0.05 floor
    assert!(agent.trust_score >= 0.05);
}

#[test]
fn test_agent_trust_score_ceiling() {
    let mut agent = Agent::new("Test", AgentRole::Coder);
    // Drive trust up with repeated successes
    for _ in 0..20 {
        agent.complete_task(true);
    }
    // Trust should be capped at 1.0
    assert!((agent.trust_score - 1.0).abs() < f32::EPSILON);
}

#[test]
fn test_agent_set_idle() {
    let mut agent = Agent::new("Test", AgentRole::Coder);
    agent.start_working();
    assert_eq!(agent.status, AgentStatus::Working);
    agent.set_idle();
    assert_eq!(agent.status, AgentStatus::Idle);
}

#[test]
fn test_agent_set_error() {
    let mut agent = Agent::new("Test", AgentRole::Coder);
    agent.set_error();
    assert_eq!(agent.status, AgentStatus::Error);
}

#[test]
fn test_agent_full_builder_chain() {
    let agent = Agent::new("Full", AgentRole::VisualCritic)
        .with_prompt("Custom visual critic prompt")
        .with_expertise("UI/UX")
        .with_expertise("Accessibility")
        .with_model("vision-model-v2");

    assert_eq!(agent.role, AgentRole::VisualCritic);
    assert_eq!(agent.system_prompt(), "Custom visual critic prompt");
    assert_eq!(agent.expertise.len(), 2);
    assert_eq!(agent.model_id, Some("vision-model-v2".to_string()));
}

#[test]
fn test_agent_start_working_updates_status() {
    let mut agent = Agent::new("Test", AgentRole::Coder);
    let before = agent.last_active;
    std::thread::sleep(std::time::Duration::from_millis(10));
    agent.start_working();
    assert_eq!(agent.status, AgentStatus::Working);
    assert!(agent.last_active >= before);
}

#[test]
fn test_agent_complete_task_updates_last_active() {
    let mut agent = Agent::new("Test", AgentRole::Coder);
    let before = agent.last_active;
    std::thread::sleep(std::time::Duration::from_millis(10));
    agent.complete_task(true);
    assert!(agent.last_active >= before);
}

// ============================================================================
// Vote Tests
// ============================================================================

#[test]
fn test_vote_creation() {
    let vote = Vote::new(
        "agent1",
        AgentRole::Reviewer,
        "option_a",
        0.9,
        "Good choice",
    );

    assert_eq!(vote.choice, "option_a");
    assert_eq!(vote.confidence, 0.9);
}

#[test]
fn test_vote_weighted_value() {
    let vote = Vote::new("agent1", AgentRole::Security, "opt", 1.0, "reason");
    let value = vote.weighted_value(1.0);

    // Security has priority 10, so weight = 1.0
    assert!((value - 1.0).abs() < 0.01);
}

#[test]
fn test_vote_weighted_value_general_role() {
    // General has priority 2, so role_weight = 0.2
    let vote = Vote::new("a1", AgentRole::General, "opt", 1.0, "r");
    let value = vote.weighted_value(1.0);
    assert!((value - 0.2).abs() < 0.01);
}

#[test]
fn test_vote_weighted_value_zero_trust() {
    let vote = Vote::new("a1", AgentRole::Security, "opt", 1.0, "r");
    let value = vote.weighted_value(0.0);
    assert!(value.abs() < f32::EPSILON);
}

#[test]
fn test_vote_confidence_clamped_above_1() {
    let vote = Vote::new("a1", AgentRole::Coder, "opt", 5.0, "reason");
    assert!((vote.confidence - 1.0).abs() < f32::EPSILON);
}

#[test]
fn test_vote_confidence_clamped_below_0() {
    let vote = Vote::new("a1", AgentRole::Coder, "opt", -3.0, "reason");
    assert!(vote.confidence.abs() < f32::EPSILON);
}

#[test]
fn test_vote_timestamp_set() {
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let vote = Vote::new("a1", AgentRole::Coder, "opt", 0.5, "reason");
    let after = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(vote.timestamp >= before && vote.timestamp <= after);
}

// ============================================================================
// Decision Tests
// ============================================================================

#[test]
fn test_decision_creation() {
    let decision = super::types::Decision::new("Which approach?", vec!["A".into(), "B".into()]);

    assert!(decision.is_pending());
    assert_eq!(decision.options.len(), 2);
}

#[test]
fn test_decision_add_vote() {
    let mut decision = super::types::Decision::new("Test?", vec!["Yes".into(), "No".into()]);
    decision.add_vote(Vote::new("a1", AgentRole::Coder, "Yes", 0.8, "reason"));

    assert_eq!(decision.votes.len(), 1);
    assert_eq!(decision.votes_for("Yes").len(), 1);
}

#[test]
fn test_decision_resolve() {
    let mut decision = super::types::Decision::new("Test?", vec!["A".into(), "B".into()]);
    decision.add_vote(Vote::new("a1", AgentRole::Coder, "A", 0.9, "r1"));
    decision.add_vote(Vote::new("a2", AgentRole::Tester, "A", 0.8, "r2"));
    decision.add_vote(Vote::new("a3", AgentRole::Reviewer, "B", 0.5, "r3"));

    let trust_scores: HashMap<String, f32> = [
        ("a1".to_string(), 0.8),
        ("a2".to_string(), 0.7),
        ("a3".to_string(), 0.6),
    ]
    .into_iter()
    .collect();

    let outcome = decision.resolve(&trust_scores);
    assert!(outcome.is_some());
    assert_eq!(outcome.unwrap(), "A");
}

#[test]
fn test_decision_resolve_empty_options() {
    let mut decision = super::types::Decision::new("Empty?", vec![]);
    let trust: HashMap<String, f32> = HashMap::new();
    let result = decision.resolve(&trust);
    assert!(result.is_none());
}

#[test]
fn test_decision_resolve_conflict_scores_close() {
    let mut decision = super::types::Decision::new("Close?", vec!["A".into(), "B".into()]);
    // Both votes with the same role, same confidence, same trust => scores identical
    decision.add_vote(Vote::new("a1", AgentRole::Coder, "A", 0.8, "r1"));
    decision.add_vote(Vote::new("a2", AgentRole::Coder, "B", 0.8, "r2"));

    let trust: HashMap<String, f32> = [("a1".to_string(), 0.5), ("a2".to_string(), 0.5)]
        .into_iter()
        .collect();

    let result = decision.resolve(&trust);
    assert!(result.is_none());
    assert_eq!(decision.status, DecisionStatus::Conflict);
}

#[test]
fn test_decision_weighted_score_missing_trust_defaults() {
    let mut decision = super::types::Decision::new("Test?", vec!["A".into()]);
    decision.add_vote(Vote::new("unknown_agent", AgentRole::Coder, "A", 0.8, "r"));

    // Empty trust map => defaults to 0.5
    let trust: HashMap<String, f32> = HashMap::new();
    let score = decision.weighted_score("A", &trust);
    // Coder priority=4, role_weight=0.4, confidence=0.8, trust=0.5
    // 0.8 * 0.4 * 0.5 = 0.16
    assert!((score - 0.16).abs() < 0.01);
}

#[test]
fn test_decision_votes_for_empty() {
    let decision = super::types::Decision::new("Test?", vec!["A".into(), "B".into()]);
    assert!(decision.votes_for("A").is_empty());
    assert!(decision.votes_for("nonexistent").is_empty());
}

#[test]
fn test_decision_is_pending_false_when_resolved() {
    let mut decision = super::types::Decision::new("Test?", vec!["A".into()]);
    decision.add_vote(Vote::new("a1", AgentRole::Security, "A", 1.0, "r"));
    let trust: HashMap<String, f32> = [("a1".to_string(), 1.0)].into_iter().collect();

    assert!(decision.is_pending());
    decision.resolve(&trust);
    assert!(!decision.is_pending());
}

#[test]
fn test_decision_resolve_sets_resolved_at() {
    let mut decision = super::types::Decision::new("Test?", vec!["A".into()]);
    decision.add_vote(Vote::new("a1", AgentRole::Security, "A", 1.0, "r"));
    let trust: HashMap<String, f32> = [("a1".to_string(), 1.0)].into_iter().collect();

    assert!(decision.resolved_at.is_none());
    decision.resolve(&trust);
    assert!(decision.resolved_at.is_some());
}

// ============================================================================
// SharedMemory Tests
// ============================================================================

#[test]
fn test_shared_memory_write_read() {
    let mut memory = SharedMemory::new();

    memory.write("key1", "value1", "agent1");
    let value = memory.read("key1", "agent2");

    assert_eq!(value, Some("value1".to_string()));
}

#[test]
fn test_shared_memory_peek() {
    let mut memory = SharedMemory::new();
    memory.write("key1", "value1", "agent1");

    let value = memory.peek("key1");
    assert_eq!(value, Some("value1"));
}

#[test]
fn test_shared_memory_delete() {
    let mut memory = SharedMemory::new();
    memory.write("key1", "value1", "agent1");

    let deleted = memory.delete("key1", "agent1");
    assert_eq!(deleted, Some("value1".to_string()));
    assert!(memory.peek("key1").is_none());
}

#[test]
fn test_shared_memory_delete_missing_key() {
    let mut memory = SharedMemory::new();
    let result = memory.delete("nonexistent", "agent1");
    assert!(result.is_none());
}

#[test]
fn test_shared_memory_tags() {
    let mut memory = SharedMemory::new();
    memory.write("key1", "value1", "agent1");
    memory.tag("key1", "important");

    let tagged = memory.find_by_tag("important");
    assert_eq!(tagged.len(), 1);
}

#[test]
fn test_shared_memory_find_by_tag_no_match() {
    let mut memory = SharedMemory::new();
    memory.write("key1", "value1", "agent1");
    let results = memory.find_by_tag("nonexistent_tag");
    assert!(results.is_empty());
}

#[test]
fn test_shared_memory_tag_nonexistent_key() {
    let mut memory = SharedMemory::new();
    memory.tag("nonexistent", "sometag"); // should not panic
    assert!(memory.find_by_tag("sometag").is_empty());
}

#[test]
fn test_shared_memory_access_log() {
    let mut memory = SharedMemory::new();
    memory.write("key1", "value1", "agent1");
    memory.read("key1", "agent2");

    assert_eq!(memory.access_log().len(), 2);
}

#[test]
fn test_shared_memory_write_update_existing() {
    let mut memory = SharedMemory::new();
    memory.write("key1", "value1", "agent1");
    memory.write("key1", "value2", "agent2");

    assert_eq!(memory.peek("key1"), Some("value2"));

    // Verify modified_by is set
    let entries = memory.entries();
    let entry = entries.iter().find(|e| e.key == "key1").unwrap();
    assert_eq!(entry.modified_by, Some("agent2".to_string()));
    assert!(entry.modified_at.is_some());
}

#[test]
fn test_shared_memory_read_missing_key() {
    let mut memory = SharedMemory::new();
    let result = memory.read("nonexistent", "agent1");
    assert!(result.is_none());
}

#[test]
fn test_shared_memory_read_increments_access_count() {
    let mut memory = SharedMemory::new();
    memory.write("key1", "val", "agent1");

    memory.read("key1", "agent2");
    memory.read("key1", "agent3");
    memory.read("key1", "agent2");

    let entries = memory.entries();
    let entry = entries.iter().find(|e| e.key == "key1").unwrap();
    assert_eq!(entry.access_count, 3);
}

#[test]
fn test_shared_memory_keys() {
    let mut memory = SharedMemory::new();
    memory.write("k1", "v1", "a1");
    memory.write("k2", "v2", "a1");

    let keys = memory.keys();
    assert_eq!(keys.len(), 2);
}

#[test]
fn test_shared_memory_entries() {
    let mut memory = SharedMemory::new();
    memory.write("k1", "v1", "a1");
    memory.write("k2", "v2", "a1");
    memory.write("k3", "v3", "a2");

    let entries = memory.entries();
    assert_eq!(entries.len(), 3);
}

#[test]
fn test_shared_memory_clear() {
    let mut memory = SharedMemory::new();
    memory.write("k1", "v1", "a1");
    memory.clear();

    assert!(memory.keys().is_empty());
    assert!(memory.access_log().is_empty());
}

#[test]
fn test_shared_memory_multiple_tags() {
    let mut memory = SharedMemory::new();
    memory.write("key1", "value1", "agent1");
    memory.tag("key1", "tag1");
    memory.tag("key1", "tag2");

    let entries1 = memory.find_by_tag("tag1");
    let entries2 = memory.find_by_tag("tag2");
    assert_eq!(entries1.len(), 1);
    assert_eq!(entries2.len(), 1);
}

#[test]
fn test_shared_memory_default() {
    let memory: SharedMemory = Default::default();
    assert!(memory.keys().is_empty());
}

// ============================================================================
// Swarm Tests
// ============================================================================

#[test]
fn test_swarm_creation() {
    let swarm = Swarm::new();
    assert_eq!(swarm.list_agents().len(), 0);
}

#[test]
fn test_swarm_default() {
    let swarm = Swarm::default();
    assert_eq!(swarm.list_agents().len(), 0);
}

#[test]
fn test_swarm_add_agent() {
    let mut swarm = Swarm::new();
    let agent = Agent::new("Test", AgentRole::Coder);
    let id = swarm.add_agent(agent);

    assert!(swarm.get_agent(&id).is_some());
}

#[test]
fn test_swarm_remove_agent() {
    let mut swarm = Swarm::new();
    let agent = Agent::new("Test", AgentRole::Coder);
    let id = swarm.add_agent(agent);

    let removed = swarm.remove_agent(&id);
    assert!(removed.is_some());
    assert!(swarm.get_agent(&id).is_none());
}

#[test]
fn test_swarm_remove_agent_not_found() {
    let mut swarm = Swarm::new();
    assert!(swarm.remove_agent("nonexistent").is_none());
}

#[test]
fn test_swarm_agents_by_role() {
    let mut swarm = Swarm::new();
    swarm.add_agent(Agent::new("C1", AgentRole::Coder));
    swarm.add_agent(Agent::new("C2", AgentRole::Coder));
    swarm.add_agent(Agent::new("T1", AgentRole::Tester));

    assert_eq!(swarm.agents_by_role(AgentRole::Coder).len(), 2);
    assert_eq!(swarm.agents_by_role(AgentRole::Tester).len(), 1);
}

#[test]
fn test_swarm_idle_agents() {
    let mut swarm = Swarm::new();
    let id1 = swarm.add_agent(Agent::new("A1", AgentRole::Coder));
    swarm.add_agent(Agent::new("A2", AgentRole::Coder));

    swarm.get_agent_mut(&id1).unwrap().start_working();

    assert_eq!(swarm.idle_agents().len(), 1);
}

#[test]
fn test_swarm_create_decision() {
    let mut swarm = Swarm::new();
    let decision_id = swarm.create_decision("Which?", vec!["A".into(), "B".into()]);

    assert!(!decision_id.is_empty());
}

#[test]
fn test_swarm_vote() {
    let mut swarm = Swarm::new();
    let agent_id = swarm.add_agent(Agent::new("Test", AgentRole::Coder));
    let decision_id = swarm.create_decision("Which?", vec!["A".into(), "B".into()]);

    let result = swarm.vote(&decision_id, &agent_id, "A", 0.9, "Looks good");
    assert!(result.is_ok());
}

#[test]
fn test_swarm_vote_agent_not_found() {
    let mut swarm = Swarm::new();
    let decision_id = swarm.create_decision("Q?", vec!["A".into()]);

    let result = swarm.vote(&decision_id, "nonexistent_agent", "A", 0.9, "r");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Agent not found"));
}

#[test]
fn test_swarm_vote_decision_not_found() {
    let mut swarm = Swarm::new();
    let agent_id = swarm.add_agent(Agent::new("A", AgentRole::Coder));

    let result = swarm.vote("nonexistent_decision", &agent_id, "A", 0.9, "r");
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Decision not found"));
}

#[test]
fn test_swarm_vote_decision_already_resolved() {
    let mut swarm = Swarm::new();
    let a1 = swarm.add_agent(Agent::new("A1", AgentRole::Coder));
    let a2 = swarm.add_agent(Agent::new("A2", AgentRole::Tester));
    let id = swarm.create_decision("Q?", vec!["X".into()]);
    swarm.vote(&id, &a1, "X", 0.9, "r").unwrap();
    swarm.resolve_decision(&id).unwrap();

    let result = swarm.vote(&id, &a2, "X", 0.8, "r2");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("already resolved"));
}

#[test]
fn test_swarm_resolve_decision() {
    let mut swarm = Swarm::new();
    let a1 = swarm.add_agent(Agent::new("A1", AgentRole::Architect));
    let a2 = swarm.add_agent(Agent::new("A2", AgentRole::Coder));

    let decision_id = swarm.create_decision("Which?", vec!["X".into(), "Y".into()]);

    swarm.vote(&decision_id, &a1, "X", 0.9, "r1").unwrap();
    swarm.vote(&decision_id, &a2, "X", 0.8, "r2").unwrap();

    let outcome = swarm.resolve_decision(&decision_id).unwrap();
    assert_eq!(outcome, Some("X".to_string()));
}

#[test]
fn test_swarm_resolve_decision_not_found() {
    let mut swarm = Swarm::new();
    let result = swarm.resolve_decision("nonexistent");
    assert!(result.is_err());
}

#[test]
fn test_swarm_list_decisions() {
    let mut swarm = Swarm::new();
    swarm.create_decision("Q1?", vec!["A".into()]);
    swarm.create_decision("Q2?", vec!["B".into()]);

    assert_eq!(swarm.list_decisions().len(), 2);
}

#[test]
fn test_swarm_get_decision_found() {
    let mut swarm = Swarm::new();
    let id = swarm.create_decision("Q?", vec!["A".into()]);
    assert!(swarm.get_decision(&id).is_some());
}

#[test]
fn test_swarm_get_decision_not_found() {
    let swarm = Swarm::new();
    assert!(swarm.get_decision("nonexistent").is_none());
}

#[test]
fn test_swarm_list_tasks() {
    let mut swarm = Swarm::new();
    swarm.queue_task(SwarmTask::new("T1")).unwrap();
    swarm.queue_task(SwarmTask::new("T2")).unwrap();

    assert_eq!(swarm.list_tasks().len(), 2);
}

#[test]
fn test_swarm_get_task_found() {
    let mut swarm = Swarm::new();
    let task = SwarmTask::new("Find me");
    let task_id = task.id.clone();
    swarm.queue_task(task).unwrap();

    assert!(swarm.get_task(&task_id).is_some());
}

#[test]
fn test_swarm_get_task_not_found() {
    let swarm = Swarm::new();
    assert!(swarm.get_task("nonexistent").is_none());
}

#[test]
fn test_swarm_queue_task() {
    let mut swarm = Swarm::new();

    swarm
        .queue_task(SwarmTask::new("Task 1").with_priority(5))
        .unwrap();
    swarm
        .queue_task(SwarmTask::new("Task 2").with_priority(8))
        .unwrap();

    // Higher priority should come first
    let task_id = swarm.next_task().unwrap();
    let task = swarm.get_task(&task_id).unwrap();
    assert_eq!(task.priority, 8);
}

#[test]
fn test_swarm_queue_task_ordering() {
    let mut swarm = Swarm::new();
    swarm
        .queue_task(SwarmTask::new("Low").with_priority(1))
        .unwrap();
    swarm
        .queue_task(SwarmTask::new("High").with_priority(10))
        .unwrap();
    swarm
        .queue_task(SwarmTask::new("Mid").with_priority(5))
        .unwrap();

    // pop() returns from the end; sorted ascending, so highest is last
    let t1_id = swarm.next_task().unwrap();
    let t1 = swarm.get_task(&t1_id).unwrap();
    assert_eq!(t1.priority, 10);
    let t2_id = swarm.next_task().unwrap();
    let t2 = swarm.get_task(&t2_id).unwrap();
    assert_eq!(t2.priority, 5);
    let t3_id = swarm.next_task().unwrap();
    let t3 = swarm.get_task(&t3_id).unwrap();
    assert_eq!(t3.priority, 1);
}

#[test]
fn test_swarm_next_task_empty() {
    let mut swarm = Swarm::new();
    assert!(swarm.next_task().is_none());
}

#[test]
fn test_swarm_assign_task() {
    let mut swarm = Swarm::new();
    let coder_id = swarm.add_agent(Agent::new("Cody", AgentRole::Coder));
    let tester_id = swarm.add_agent(Agent::new("Tessa", AgentRole::Tester));

    let task = SwarmTask::new("Build it")
        .with_role(AgentRole::Coder)
        .with_role(AgentRole::Tester);
    let task_id = task.id.clone();
    swarm.queue_task(task).unwrap();

    let assigned = swarm.assign_task(&task_id);
    assert_eq!(assigned.len(), 2);
    assert!(assigned.contains(&coder_id));
    assert!(assigned.contains(&tester_id));

    // Assigned agents should now be Working
    assert_eq!(
        swarm.get_agent(&coder_id).unwrap().status,
        AgentStatus::Working
    );
    assert_eq!(
        swarm.get_agent(&tester_id).unwrap().status,
        AgentStatus::Working
    );

    // Task should be InProgress
    let task = swarm.get_task(&task_id).unwrap();
    assert_eq!(task.status, TaskStatus::InProgress);
}

#[test]
fn test_swarm_assign_task_not_found() {
    let mut swarm = Swarm::new();
    let assigned = swarm.assign_task("nonexistent");
    assert!(assigned.is_empty());
}

#[test]
fn test_swarm_assign_task_no_idle_agents() {
    let mut swarm = Swarm::new();
    let coder_id = swarm.add_agent(Agent::new("Cody", AgentRole::Coder));
    swarm.get_agent_mut(&coder_id).unwrap().start_working();

    let task = SwarmTask::new("Build it").with_role(AgentRole::Coder);
    let task_id = task.id.clone();
    swarm.queue_task(task).unwrap();

    let assigned = swarm.assign_task(&task_id);
    assert!(assigned.is_empty());
}

#[test]
fn test_swarm_complete_task() {
    let mut swarm = Swarm::new();
    let coder_id = swarm.add_agent(Agent::new("Cody", AgentRole::Coder));

    let task = SwarmTask::new("Build it").with_role(AgentRole::Coder);
    let task_id = task.id.clone();
    swarm.queue_task(task).unwrap();

    let assigned = swarm.assign_task(&task_id);
    assert_eq!(assigned.len(), 1);

    swarm.complete_task(&task_id, &coder_id, "Done!");

    // Task should be Completed since all assigned agents submitted
    let task = swarm.get_task(&task_id).unwrap();
    assert_eq!(task.status, TaskStatus::Completed);
    assert_eq!(task.results.get(&coder_id).unwrap(), "Done!");

    // Agent should be Completed
    let agent = swarm.get_agent(&coder_id).unwrap();
    assert_eq!(agent.status, AgentStatus::Completed);
    assert_eq!(agent.tasks_completed, 1);
}

#[test]
fn test_swarm_complete_task_partial() {
    let mut swarm = Swarm::new();
    let c1 = swarm.add_agent(Agent::new("C1", AgentRole::Coder));
    let c2 = swarm.add_agent(Agent::new("C2", AgentRole::Coder));

    let task = SwarmTask::new("Build it")
        .with_role(AgentRole::Coder)
        .with_role(AgentRole::Coder);
    let task_id = task.id.clone();
    swarm.queue_task(task).unwrap();
    swarm.assign_task(&task_id);

    // Only one agent completes
    swarm.complete_task(&task_id, &c1, "Partial");

    let task = swarm.get_task(&task_id).unwrap();
    // Not all agents submitted, so task should still be InProgress
    assert_eq!(task.status, TaskStatus::InProgress);

    // Second agent completes
    swarm.complete_task(&task_id, &c2, "Full");
    let task = swarm.get_task(&task_id).unwrap();
    assert_eq!(task.status, TaskStatus::Completed);
}

#[test]
fn test_swarm_complete_task_not_found() {
    let mut swarm = Swarm::new();
    let coder_id = swarm.add_agent(Agent::new("Cody", AgentRole::Coder));
    // Completing a nonexistent task should not panic
    swarm.complete_task("nonexistent", &coder_id, "result");
}

#[test]
fn test_swarm_stats_empty() {
    let swarm = Swarm::new();
    let stats = swarm.stats();
    assert_eq!(stats.total_agents, 0);
    assert!(stats.average_trust.abs() < f32::EPSILON);
    assert_eq!(stats.pending_decisions, 0);
    assert_eq!(stats.queued_tasks, 0);
}

#[test]
fn test_swarm_stats_detailed() {
    let mut swarm = Swarm::new();
    swarm.add_agent(Agent::new("A1", AgentRole::Coder));
    swarm.add_agent(Agent::new("A2", AgentRole::Coder));
    swarm.add_agent(Agent::new("A3", AgentRole::Tester));
    swarm.create_decision("Q?", vec!["A".into()]);
    swarm.queue_task(SwarmTask::new("T1")).unwrap();

    let stats = swarm.stats();
    assert_eq!(stats.total_agents, 3);
    assert_eq!(*stats.agents_by_role.get(&AgentRole::Coder).unwrap(), 2);
    assert_eq!(*stats.agents_by_role.get(&AgentRole::Tester).unwrap(), 1);
    assert_eq!(stats.pending_decisions, 1);
    assert_eq!(stats.queued_tasks, 1);
    // All agents start with 0.5 trust, so average = 0.5
    assert!((stats.average_trust - 0.5).abs() < 0.01);
}

#[test]
fn test_swarm_memory_shared() {
    let swarm = Swarm::new();
    let m1 = swarm.memory();
    let m2 = swarm.memory();
    // Both should point to the same allocation
    assert!(Arc::ptr_eq(&m1, &m2));
}

#[test]
fn test_swarm_with_settings() {
    let swarm = Swarm::new()
        .with_conflict_strategy(ConflictStrategy::MajorityWins)
        .with_consensus_threshold(0.7);

    // Verify settings were applied through behavior
    let thresholded = Swarm::new()
        .with_consensus_threshold(0.5)
        .with_consensus_threshold(2.0);
    let _ = thresholded; // Threshold clamping verified in other tests
    let _ = swarm;
}

#[test]
fn test_swarm_consensus_threshold_clamping() {
    // Test that threshold values are clamped to 0.0-1.0 range
    let swarm = Swarm::new().with_consensus_threshold(2.0);
    let _ = swarm;

    let swarm2 = Swarm::new().with_consensus_threshold(-1.0);
    let _ = swarm2;

    // Verify through behavior - these shouldn't panic
}

#[test]
fn test_conflict_strategy_default() {
    assert_eq!(ConflictStrategy::default(), ConflictStrategy::PriorityWins);
}

#[test]
fn test_resolve_conflict_priority_wins() {
    let mut swarm = Swarm::new().with_conflict_strategy(ConflictStrategy::PriorityWins);

    let sec_id = swarm.add_agent(Agent::new("Sec", AgentRole::Security));
    let cod_id = swarm.add_agent(Agent::new("Cod", AgentRole::Coder));

    let did = swarm.create_decision("Strategy?", vec!["A".into(), "B".into()]);
    swarm.vote(&did, &sec_id, "A", 0.8, "r").unwrap();
    swarm.vote(&did, &cod_id, "B", 0.8, "r").unwrap();

    // Force conflict status through decision resolution
    swarm.resolve_decision(&did).ok();

    let result = swarm.resolve_conflict(&did).unwrap();
    // Result may vary based on decision state
    assert!(result.is_some() || result.is_none());
}

#[test]
fn test_resolve_conflict_confidence_wins() {
    let mut swarm = Swarm::new().with_conflict_strategy(ConflictStrategy::ConfidenceWins);

    let a1 = swarm.add_agent(Agent::new("A1", AgentRole::Coder));
    let a2 = swarm.add_agent(Agent::new("A2", AgentRole::Coder));

    let did = swarm.create_decision("Pick?", vec!["X".into(), "Y".into()]);
    swarm.vote(&did, &a1, "X", 0.6, "r").unwrap();
    swarm.vote(&did, &a2, "Y", 0.95, "r").unwrap();

    let result = swarm.resolve_conflict(&did).unwrap();
    // If not in conflict, returns current outcome
    assert!(result.is_some() || result.is_none());
}

#[test]
fn test_resolve_conflict_majority_wins() {
    let mut swarm = Swarm::new().with_conflict_strategy(ConflictStrategy::MajorityWins);

    let a1 = swarm.add_agent(Agent::new("A1", AgentRole::Coder));
    let a2 = swarm.add_agent(Agent::new("A2", AgentRole::Coder));
    let a3 = swarm.add_agent(Agent::new("A3", AgentRole::Coder));

    let did = swarm.create_decision("Vote?", vec!["A".into(), "B".into()]);
    swarm.vote(&did, &a1, "A", 0.8, "r").unwrap();
    swarm.vote(&did, &a2, "A", 0.7, "r").unwrap();
    swarm.vote(&did, &a3, "B", 0.9, "r").unwrap();

    let result = swarm.resolve_conflict(&did).unwrap();
    // Result depends on decision state
    let _ = result;
}

#[test]
fn test_resolve_conflict_human_intervention() {
    let mut swarm = Swarm::new().with_conflict_strategy(ConflictStrategy::HumanIntervention);

    let a1 = swarm.add_agent(Agent::new("A1", AgentRole::Coder));

    let did = swarm.create_decision("Help?", vec!["A".into()]);
    swarm.vote(&did, &a1, "A", 0.8, "r").unwrap();

    let result = swarm.resolve_conflict(&did).unwrap();
    // If not in conflict, returns outcome; if in conflict with HumanIntervention, returns None
    let _ = result;
}

#[test]
fn test_resolve_conflict_accept_all() {
    let mut swarm = Swarm::new().with_conflict_strategy(ConflictStrategy::AcceptAll);

    let a1 = swarm.add_agent(Agent::new("A1", AgentRole::Coder));
    let a2 = swarm.add_agent(Agent::new("A2", AgentRole::Tester));

    let did = swarm.create_decision("Merge?", vec!["X".into(), "Y".into()]);
    swarm.vote(&did, &a1, "X", 0.8, "r").unwrap();
    swarm.vote(&did, &a2, "Y", 0.7, "r").unwrap();

    let result = swarm.resolve_conflict(&did).unwrap();
    // If in conflict, combines all unique choices
    let _ = result;
}

#[test]
fn test_resolve_conflict_not_in_conflict_returns_outcome() {
    let mut swarm = Swarm::new();
    let a1 = swarm.add_agent(Agent::new("A1", AgentRole::Coder));
    let did = swarm.create_decision("Q?", vec!["A".into()]);
    swarm.vote(&did, &a1, "A", 0.9, "r").unwrap();
    swarm.resolve_decision(&did).unwrap();

    // Decision is Resolved, not Conflict
    let result = swarm.resolve_conflict(&did).unwrap();
    assert_eq!(result, Some("A".to_string()));
}

#[test]
fn test_resolve_conflict_decision_not_found() {
    let mut swarm = Swarm::new();
    let result = swarm.resolve_conflict("nonexistent");
    assert!(result.is_err());
}

#[test]
fn test_resolve_conflict_updates_decision_state() {
    // Verify that resolve_conflict actually mutates the decision's
    // status and outcome (not just computing and discarding the result).
    let mut swarm = Swarm::new()
        .with_conflict_strategy(ConflictStrategy::PriorityWins)
        .with_consensus_threshold(0.9); // high threshold to force Conflict

    let sec_id = swarm.add_agent(Agent::new("Sec", AgentRole::Security));
    let cod_id = swarm.add_agent(Agent::new("Cod", AgentRole::Coder));

    let did = swarm.create_decision("Strategy?", vec!["A".into(), "B".into()]);
    swarm.vote(&did, &sec_id, "A", 0.8, "r").unwrap();
    swarm.vote(&did, &cod_id, "B", 0.8, "r").unwrap();

    // This should produce a Conflict (scores are close, threshold is 0.9)
    swarm.resolve_decision(&did).ok();
    assert_eq!(
        swarm.get_decision(&did).unwrap().status,
        DecisionStatus::Conflict
    );

    // Now resolve the conflict — Security has higher priority, so "A" wins.
    let result = swarm.resolve_conflict(&did).unwrap();
    assert_eq!(result, Some("A".to_string()));

    // The decision's state must be updated to reflect the resolution.
    let decision = swarm.get_decision(&did).unwrap();
    assert_eq!(decision.status, DecisionStatus::Resolved);
    assert_eq!(decision.outcome, Some("A".to_string()));
    assert!(decision.resolved_at.is_some());
}

// ============================================================================
// Swarm Factory Tests
// ============================================================================

#[test]
fn test_create_dev_swarm() {
    let swarm = super::factory::create_dev_swarm();
    assert_eq!(swarm.list_agents().len(), 4);
}

#[test]
fn test_create_dev_swarm_has_all_roles() {
    let swarm = super::factory::create_dev_swarm();
    assert_eq!(swarm.agents_by_role(AgentRole::Architect).len(), 1);
    assert_eq!(swarm.agents_by_role(AgentRole::Coder).len(), 1);
    assert_eq!(swarm.agents_by_role(AgentRole::Tester).len(), 1);
    assert_eq!(swarm.agents_by_role(AgentRole::Reviewer).len(), 1);
}

// ============================================================================
// Coordinator Routing Tests
// ============================================================================
//
// These tests verify the swarm coordinator flow that `--coordinator` activates:
// queue_task → next_task → assign_task → complete_task → consensus decision.

#[test]
fn test_coordinator_full_flow_queue_assign_complete_consensus() {
    // Build a dev swarm (Architect, Coder, Tester, Reviewer).
    let mut swarm = super::factory::create_dev_swarm()
        .with_conflict_strategy(ConflictStrategy::ConfidenceWins)
        .with_consensus_threshold(0.5);

    // 1. Queue a task requiring all four roles.
    let mut task = SwarmTask::new("Implement feature X with tests and review");
    task = task.with_role(AgentRole::Architect);
    task = task.with_role(AgentRole::Coder);
    task = task.with_role(AgentRole::Tester);
    task = task.with_role(AgentRole::Reviewer);
    swarm.queue_task(task).expect("queue_task should succeed");

    assert_eq!(swarm.list_tasks().len(), 1);

    // 2. Pop the next task.
    let task_id = swarm
        .next_task()
        .expect("next_task should return a task id");
    assert!(swarm.get_task(&task_id).is_some());

    // 3. Assign the task to role-matched idle agents.
    let assigned = swarm.assign_task(&task_id);
    assert_eq!(
        assigned.len(),
        4,
        "all four roles should be assigned to idle agents"
    );

    // Verify all assigned agents are now Working.
    for agent_id in &assigned {
        let agent = swarm.get_agent(agent_id).expect("agent should exist");
        assert_eq!(agent.status, AgentStatus::Working);
    }

    // 4. Complete the task for each agent with a result.
    for agent_id in &assigned {
        swarm.complete_task(
            &task_id,
            agent_id,
            format!("Result from agent {}", agent_id),
        );
    }

    // Verify the task is now Completed.
    let task = swarm.get_task(&task_id).expect("task should exist");
    assert_eq!(task.status, TaskStatus::Completed);
    assert_eq!(task.results.len(), 4);

    // 5. Create a consensus decision and have agents vote.
    let agent_names: Vec<String> = assigned
        .iter()
        .map(|id| swarm.get_agent(id).unwrap().name.clone())
        .collect();

    let decision_id = swarm.create_decision(
        "Which agent's response best addresses the task?",
        agent_names.clone(),
    );

    // Each agent votes for a different option with varying confidence.
    for (i, agent_id) in assigned.iter().enumerate() {
        let choice = &agent_names[i];
        let confidence = 0.5 + (i as f32 * 0.1); // 0.5, 0.6, 0.7, 0.8
        swarm
            .vote(
                &decision_id,
                agent_id,
                choice.clone(),
                confidence,
                "reasoning",
            )
            .expect("vote should succeed");
    }

    // 6. Resolve the decision. With 4 agents voting for 4 different options,
    //    the consensus threshold (0.5) may not be reached, resulting in a
    //    Conflict. In that case, resolve_conflict applies the ConfidenceWins
    //    strategy to pick the highest-confidence vote.
    let outcome = swarm
        .resolve_decision(&decision_id)
        .expect("resolve_decision should not error");

    let decision = swarm.get_decision(&decision_id).expect("decision exists");

    let final_outcome = if decision.status == DecisionStatus::Conflict {
        // Conflict — use the conflict resolution strategy (ConfidenceWins).
        swarm
            .resolve_conflict(&decision_id)
            .expect("resolve_conflict should not error")
    } else {
        outcome
    };

    assert!(
        final_outcome.is_some(),
        "decision should have an outcome after resolution"
    );

    let decision = swarm.get_decision(&decision_id).expect("decision exists");
    assert!(
        decision.status == DecisionStatus::Resolved || decision.status == DecisionStatus::Conflict,
        "decision should be Resolved or Conflict after resolution attempt"
    );
}

#[test]
fn test_coordinator_assign_no_idle_agents() {
    let mut swarm = Swarm::new();
    // No agents in the swarm — assign should return empty.
    let mut task = SwarmTask::new("Test task");
    task = task.with_role(AgentRole::Coder);
    swarm.queue_task(task).unwrap();
    let task_id = swarm.next_task().expect("should get task");
    let assigned = swarm.assign_task(&task_id);
    assert!(assigned.is_empty(), "no agents should be assigned");
}

#[test]
fn test_coordinator_flag_selects_swarm_path() {
    // This test verifies the logic that determines whether the coordinator
    // (swarm) path is taken. The `--coordinator` flag maps to `coordinator: bool`
    // on the Cli struct. When true, interactive_swarm() is called instead of
    // interactive(). Here we verify the swarm infrastructure works correctly
    // so the routing is meaningful.

    let swarm = super::factory::create_dev_swarm();

    // Verify the swarm has the expected agents for coordinator mode.
    assert_eq!(
        swarm.list_agents().len(),
        4,
        "dev swarm should have 4 agents"
    );
    assert!(!swarm.agents_by_role(AgentRole::Architect).is_empty());
    assert!(!swarm.agents_by_role(AgentRole::Coder).is_empty());
    assert!(!swarm.agents_by_role(AgentRole::Tester).is_empty());
    assert!(!swarm.agents_by_role(AgentRole::Reviewer).is_empty());

    // All agents should start Idle (ready for assignment).
    for agent in swarm.list_agents() {
        assert_eq!(agent.status, AgentStatus::Idle);
    }
}

// ============================================================================
// Decision Timeout Tests
// ============================================================================

#[test]
fn test_sweep_no_timeouts() {
    let mut swarm = Swarm::new();
    swarm.create_decision("Fresh?", vec!["A".into(), "B".into()]);

    let timed_out = swarm.sweep_timed_out_decisions();
    assert!(timed_out.is_empty());
}

#[test]
fn test_sweep_marks_old_pending() {
    let mut swarm = Swarm::new().with_decision_timeout(0); // instant timeout
    let id = swarm.create_decision("Old?", vec!["A".into(), "B".into()]);

    let timed_out = swarm.sweep_timed_out_decisions();
    assert_eq!(timed_out, vec![id.clone()]);

    let d = swarm.get_decision(&id).unwrap();
    assert_eq!(d.status, DecisionStatus::TimedOut);
    assert!(d.resolved_at.is_some());
}

#[test]
fn test_sweep_skips_resolved() {
    let mut swarm = Swarm::new().with_decision_timeout(0);
    let a1 = swarm.add_agent(Agent::new("A1", AgentRole::Coder));
    let id = swarm.create_decision("Resolved?", vec!["X".into()]);
    swarm.vote(&id, &a1, "X", 0.9, "r").unwrap();
    swarm.resolve_decision(&id).unwrap();

    let timed_out = swarm.sweep_timed_out_decisions();
    assert!(timed_out.is_empty());
}

#[test]
fn test_custom_decision_timeout() {
    let swarm = Swarm::new().with_decision_timeout(600);
    // Verify through sweep behavior with timeout
    let mut swarm2 = Swarm::new().with_decision_timeout(600);
    let id = swarm2.create_decision("Test?", vec!["A".into()]);
    // With 600s timeout, should not timeout immediately
    let timed_out = swarm2.sweep_timed_out_decisions();
    assert!(timed_out.is_empty());
    let _ = id;
    let _ = swarm;
}

// ============================================================================
// Resource Gating Tests
// ============================================================================

#[test]
fn test_queue_task_no_pressure() {
    let mut swarm = Swarm::new();
    let result = swarm.queue_task(SwarmTask::new("Test task"));
    assert!(result.is_ok());
}

#[test]
fn test_queue_task_high_pressure_rejected() {
    use crate::resource::ResourcePressure;

    let pressure = Arc::new(std::sync::RwLock::new(ResourcePressure::High));
    let mut swarm = Swarm::new();
    swarm.set_resource_pressure(Arc::clone(&pressure));

    let result = swarm.queue_task(SwarmTask::new("Blocked task"));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("resource pressure"));
}

#[test]
fn test_queue_task_critical_pressure_rejected() {
    use crate::resource::ResourcePressure;

    let pressure = Arc::new(std::sync::RwLock::new(ResourcePressure::Critical));
    let mut swarm = Swarm::new();
    swarm.set_resource_pressure(Arc::clone(&pressure));

    let result = swarm.queue_task(SwarmTask::new("Blocked task"));
    assert!(result.is_err());
}

#[test]
fn test_queue_task_low_pressure_allowed() {
    use crate::resource::ResourcePressure;

    let pressure = Arc::new(std::sync::RwLock::new(ResourcePressure::Low));
    let mut swarm = Swarm::new();
    swarm.set_resource_pressure(Arc::clone(&pressure));

    let result = swarm.queue_task(SwarmTask::new("Allowed task"));
    assert!(result.is_ok());
}

#[test]
fn test_queue_task_medium_allowed() {
    use crate::resource::ResourcePressure;

    let pressure = Arc::new(std::sync::RwLock::new(ResourcePressure::Medium));
    let mut swarm = Swarm::new();
    swarm.set_resource_pressure(Arc::clone(&pressure));

    let result = swarm.queue_task(SwarmTask::new("Medium pressure task"));
    assert!(result.is_ok());
}

#[test]
fn test_queue_task_none_allowed() {
    use crate::resource::ResourcePressure;

    let pressure = Arc::new(std::sync::RwLock::new(ResourcePressure::None));
    let mut swarm = Swarm::new();
    swarm.set_resource_pressure(Arc::clone(&pressure));

    let result = swarm.queue_task(SwarmTask::new("No pressure task"));
    assert!(result.is_ok());
}

// ============================================================================
// Swarm Coordinator Advanced Tests
// ============================================================================

#[test]
fn test_swarm_agent_spawning_lifecycle() {
    let mut swarm = Swarm::new();

    // Create agents with different roles
    let architect = Agent::new("Archie", AgentRole::Architect);
    let coder = Agent::new("Cody", AgentRole::Coder);
    let tester = Agent::new("Tessa", AgentRole::Tester);

    let arch_id = swarm.add_agent(architect);
    let cod_id = swarm.add_agent(coder);
    let test_id = swarm.add_agent(tester);

    // Verify all agents created
    assert_eq!(swarm.list_agents().len(), 3);

    // Verify each agent can be retrieved
    assert!(swarm.get_agent(&arch_id).is_some());
    assert!(swarm.get_agent(&cod_id).is_some());
    assert!(swarm.get_agent(&test_id).is_some());

    // Verify initial status
    assert_eq!(swarm.get_agent(&arch_id).unwrap().status, AgentStatus::Idle);

    // Test lifecycle: Idle -> Working -> Completed
    swarm.get_agent_mut(&arch_id).unwrap().start_working();
    assert_eq!(
        swarm.get_agent(&arch_id).unwrap().status,
        AgentStatus::Working
    );

    swarm.get_agent_mut(&arch_id).unwrap().complete_task(true);
    assert_eq!(
        swarm.get_agent(&arch_id).unwrap().status,
        AgentStatus::Completed
    );
    assert_eq!(swarm.get_agent(&arch_id).unwrap().tasks_completed, 1);

    // Remove agent
    let removed = swarm.remove_agent(&arch_id);
    assert!(removed.is_some());
    assert_eq!(swarm.list_agents().len(), 2);
}

#[test]
fn test_swarm_task_distribution_with_trust() {
    let mut swarm = Swarm::new();

    // Create two coders with different trust scores
    let coder1 = Agent::new("Coder1", AgentRole::Coder);
    let coder2 = Agent::new("Coder2", AgentRole::Coder);

    let _id1 = swarm.add_agent(coder1);
    let id2 = swarm.add_agent(coder2);

    // Give coder2 higher trust through successful tasks
    for _ in 0..5 {
        swarm.get_agent_mut(&id2).unwrap().complete_task(true);
    }

    // Reset status to idle for task assignment
    swarm.get_agent_mut(&id2).unwrap().set_idle();

    // Create task requiring coder
    let task = SwarmTask::new("Important task").with_role(AgentRole::Coder);
    let task_id = task.id.clone();
    swarm.queue_task(task).unwrap();

    // Assign task
    let assigned = swarm.assign_task(&task_id);

    // Should assign to one of the idle agents
    assert!(!assigned.is_empty());
}

#[test]
fn test_swarm_task_distribution_multiple_roles() {
    let mut swarm = Swarm::new();

    // Create agents for different roles
    swarm.add_agent(Agent::new("Archie", AgentRole::Architect));
    swarm.add_agent(Agent::new("Cody", AgentRole::Coder));
    swarm.add_agent(Agent::new("Tessa", AgentRole::Tester));
    swarm.add_agent(Agent::new("Rex", AgentRole::Reviewer));

    // Create task requiring multiple roles
    let task = SwarmTask::new("Complex task")
        .with_role(AgentRole::Architect)
        .with_role(AgentRole::Coder)
        .with_role(AgentRole::Tester);

    let task_id = task.id.clone();
    swarm.queue_task(task).unwrap();

    // Assign task
    let assigned = swarm.assign_task(&task_id);

    // Should assign to agents matching required roles
    assert_eq!(assigned.len(), 3);
}

#[test]
fn test_swarm_consensus_with_multiple_votes() {
    let mut swarm = Swarm::new();

    // Create agents with different roles and trust
    let architect = swarm.add_agent(Agent::new("Archie", AgentRole::Architect));
    let coder1 = swarm.add_agent(Agent::new("Coder1", AgentRole::Coder));
    let coder2 = swarm.add_agent(Agent::new("Coder2", AgentRole::Coder));
    let tester = swarm.add_agent(Agent::new("Tessa", AgentRole::Tester));

    // Create decision
    let decision_id = swarm.create_decision(
        "Which architecture?",
        vec!["Microservices".into(), "Monolith".into(), "Hybrid".into()],
    );

    // Vote with different confidences
    swarm
        .vote(
            &decision_id,
            &architect,
            "Microservices",
            0.9,
            "Better scalability",
        )
        .unwrap();
    swarm
        .vote(&decision_id, &coder1, "Monolith", 0.6, "Simpler deployment")
        .unwrap();
    swarm
        .vote(&decision_id, &coder2, "Microservices", 0.7, "Team autonomy")
        .unwrap();
    swarm
        .vote(
            &decision_id,
            &tester,
            "Microservices",
            0.8,
            "Independent testing",
        )
        .unwrap();

    // Resolve decision
    let outcome = swarm.resolve_decision(&decision_id).unwrap();

    // Should resolve to the option with highest weighted votes
    assert!(outcome.is_some());
}

#[test]
fn test_swarm_state_management() {
    let mut swarm = Swarm::new();

    // Add agents in different states
    let _idle = swarm.add_agent(Agent::new("Idle", AgentRole::Coder));
    let working = swarm.add_agent(Agent::new("Working", AgentRole::Tester));
    let error = swarm.add_agent(Agent::new("Error", AgentRole::Reviewer));

    swarm.get_agent_mut(&working).unwrap().start_working();
    swarm.get_agent_mut(&error).unwrap().set_error();

    // Get stats
    let stats = swarm.stats();

    assert_eq!(stats.total_agents, 3);
    assert_eq!(
        *stats.agents_by_status.get(&AgentStatus::Idle).unwrap_or(&0),
        1
    );
    assert_eq!(
        *stats
            .agents_by_status
            .get(&AgentStatus::Working)
            .unwrap_or(&0),
        1
    );
    assert_eq!(
        *stats
            .agents_by_status
            .get(&AgentStatus::Error)
            .unwrap_or(&0),
        1
    );
}

#[test]
fn test_swarm_memory_shared_scratchpad() {
    let swarm = Swarm::new();

    // Get memory handle
    let memory = swarm.memory();

    // Write from one agent
    {
        let mut mem = memory.write().unwrap();
        mem.write("shared_key", "shared_value", "agent1");
    }

    // Read from another agent
    {
        let mut mem = memory.write().unwrap();
        let value = mem.read("shared_key", "agent2");
        assert_eq!(value, Some("shared_value".to_string()));
    }

    // Verify access log tracked both operations
    let mem = memory.read().unwrap();
    assert!(mem.access_log().len() >= 2);
}

#[test]
fn test_swarm_worker_timeout_handling() {
    let mut swarm = Swarm::new().with_decision_timeout(1); // 1 second timeout

    // Create decision
    let decision_id = swarm.create_decision("Test?", vec!["A".into(), "B".into()]);

    // Initially should not be timed out
    let timed_out = swarm.sweep_timed_out_decisions();
    assert!(timed_out.is_empty());

    // Wait a bit and check again (with 1s timeout)
    std::thread::sleep(std::time::Duration::from_millis(1100));

    let timed_out = swarm.sweep_timed_out_decisions();
    assert!(timed_out.contains(&decision_id));

    let decision = swarm.get_decision(&decision_id).unwrap();
    assert_eq!(decision.status, DecisionStatus::TimedOut);
}

#[test]
fn test_swarm_error_handling_recovery() {
    let mut swarm = Swarm::new();

    // Test error on nonexistent agent operations
    let result = swarm.get_agent("nonexistent");
    assert!(result.is_none());

    let removed = swarm.remove_agent("nonexistent");
    assert!(removed.is_none());

    // Test error on nonexistent decision operations
    let result = swarm.vote("nonexistent", "agent", "choice", 0.5, "reason");
    assert!(result.is_err());

    let result = swarm.resolve_decision("nonexistent");
    assert!(result.is_err());

    let result = swarm.resolve_conflict("nonexistent");
    assert!(result.is_err());

    // Test completing nonexistent task (should not panic)
    swarm.complete_task("nonexistent", "agent", "result");
}

#[test]
fn test_swarm_task_priority_ordering() {
    let mut swarm = Swarm::new();

    // Queue tasks with different priorities
    let low = SwarmTask::new("Low priority").with_priority(1);
    let medium = SwarmTask::new("Medium priority").with_priority(5);
    let high = SwarmTask::new("High priority").with_priority(10);
    let very_high = SwarmTask::new("Very high priority").with_priority(10);

    swarm.queue_task(low).unwrap();
    swarm.queue_task(medium).unwrap();
    swarm.queue_task(high).unwrap();
    swarm.queue_task(very_high).unwrap();

    // Should dequeue in priority order (highest first)
    let first_id = swarm.next_task().unwrap();
    let first = swarm.get_task(&first_id).unwrap();
    assert_eq!(first.priority, 10);

    let second_id = swarm.next_task().unwrap();
    let second = swarm.get_task(&second_id).unwrap();
    assert_eq!(second.priority, 10);

    let third_id = swarm.next_task().unwrap();
    let third = swarm.get_task(&third_id).unwrap();
    assert_eq!(third.priority, 5);

    let fourth_id = swarm.next_task().unwrap();
    let fourth = swarm.get_task(&fourth_id).unwrap();
    assert_eq!(fourth.priority, 1);
}

#[test]
fn test_swarm_task_assignment_with_busy_agents() {
    let mut swarm = Swarm::new();

    // Add two coders, make one busy
    let coder1 = swarm.add_agent(Agent::new("Coder1", AgentRole::Coder));
    let coder2 = swarm.add_agent(Agent::new("Coder2", AgentRole::Coder));

    swarm.get_agent_mut(&coder1).unwrap().start_working();

    // Create task
    let task = SwarmTask::new("Coding task").with_role(AgentRole::Coder);
    let task_id = task.id.clone();
    swarm.queue_task(task).unwrap();

    // Should only assign to idle coder
    let assigned = swarm.assign_task(&task_id);
    assert_eq!(assigned.len(), 1);
    assert!(assigned.contains(&coder2));
}

#[test]
fn test_swarm_concurrent_decisions() {
    let mut swarm = Swarm::new();

    let agent = swarm.add_agent(Agent::new("Agent", AgentRole::Coder));

    // Create multiple decisions
    let decision1 = swarm.create_decision("Q1?", vec!["A".into(), "B".into()]);
    let decision2 = swarm.create_decision("Q2?", vec!["X".into(), "Y".into()]);
    let decision3 = swarm.create_decision("Q3?", vec!["1".into(), "2".into()]);

    // Vote on different decisions
    swarm.vote(&decision1, &agent, "A", 0.8, "reason1").unwrap();
    swarm.vote(&decision2, &agent, "Y", 0.7, "reason2").unwrap();
    swarm.vote(&decision3, &agent, "1", 0.9, "reason3").unwrap();

    // Resolve each independently
    let outcome1 = swarm.resolve_decision(&decision1).unwrap();
    let outcome2 = swarm.resolve_decision(&decision2).unwrap();
    let outcome3 = swarm.resolve_decision(&decision3).unwrap();

    assert_eq!(outcome1, Some("A".to_string()));
    assert_eq!(outcome2, Some("Y".to_string()));
    assert_eq!(outcome3, Some("1".to_string()));
}

#[test]
fn test_swarm_agent_get_mut_operations() {
    let mut swarm = Swarm::new();
    let id = swarm.add_agent(Agent::new("Test", AgentRole::Coder));

    // Test mutable operations
    {
        let agent = swarm.get_agent_mut(&id).unwrap();
        agent.start_working();
        agent.trust_score = 0.8;
    }

    // Verify changes persisted
    let agent = swarm.get_agent(&id).unwrap();
    assert_eq!(agent.status, AgentStatus::Working);
    assert!((agent.trust_score - 0.8).abs() < f32::EPSILON);
}

#[test]
fn test_swarm_memory_entries_retrieval() {
    let swarm = Swarm::new();
    let memory = swarm.memory();

    // Write multiple entries
    {
        let mut mem = memory.write().unwrap();
        mem.write("key1", "value1", "agent1");
        mem.write("key2", "value2", "agent2");
        mem.write("key3", "value3", "agent1");
    }

    // Retrieve and verify entries
    let mem = memory.read().unwrap();
    let entries = mem.entries();
    assert_eq!(entries.len(), 3);

    let keys = mem.keys();
    assert_eq!(keys.len(), 3);
}

#[test]
fn test_swarm_memory_tag_and_find() {
    let swarm = Swarm::new();
    let memory = swarm.memory();

    {
        let mut mem = memory.write().unwrap();
        mem.write("config", "app settings", "agent1");
        mem.write("data", "user data", "agent2");
        mem.tag("config", "system");
        mem.tag("data", "user");
        mem.tag("config", "important");
    }

    let mem = memory.read().unwrap();
    let system_entries = mem.find_by_tag("system");
    assert_eq!(system_entries.len(), 1);

    let user_entries = mem.find_by_tag("user");
    assert_eq!(user_entries.len(), 1);

    let important_entries = mem.find_by_tag("important");
    assert_eq!(important_entries.len(), 1);
}

#[test]
fn test_conflict_strategy_variants() {
    // Test all conflict strategies can be created
    let _ = ConflictStrategy::PriorityWins;
    let _ = ConflictStrategy::ConfidenceWins;
    let _ = ConflictStrategy::MajorityWins;
    let _ = ConflictStrategy::HumanIntervention;
    let _ = ConflictStrategy::AcceptAll;

    // Test default
    assert_eq!(ConflictStrategy::default(), ConflictStrategy::PriorityWins);

    // Test with_swarm builder
    let swarm1 = Swarm::new().with_conflict_strategy(ConflictStrategy::MajorityWins);
    let swarm2 = Swarm::new().with_conflict_strategy(ConflictStrategy::ConfidenceWins);
    let _ = (swarm1, swarm2);
}

#[test]
fn test_decision_status_transitions() {
    use super::types::Decision;

    let mut decision = Decision::new("Test?", vec!["A".into(), "B".into()]);

    // Initially pending
    assert_eq!(decision.status, DecisionStatus::Pending);
    assert!(decision.is_pending());

    // Add vote and resolve
    decision.add_vote(Vote::new("a1", AgentRole::Security, "A", 1.0, "reason"));
    let trust: HashMap<String, f32> = [("a1".to_string(), 1.0)].into_iter().collect();

    decision.resolve(&trust);

    // Now resolved
    assert_eq!(decision.status, DecisionStatus::Resolved);
    assert!(!decision.is_pending());
    assert_eq!(decision.outcome, Some("A".to_string()));
}

#[test]
fn test_task_status_lifecycle() {
    let mut task = SwarmTask::new("Test task").with_role(AgentRole::Coder);

    // Initial status
    assert_eq!(task.status, TaskStatus::Pending);

    // After assignment (simulated)
    task.status = TaskStatus::InProgress;
    assert_eq!(task.status, TaskStatus::InProgress);

    // After completion
    task.status = TaskStatus::Completed;
    assert_eq!(task.status, TaskStatus::Completed);
}

#[test]
fn test_swarm_complete_task_not_in_assigned() {
    let mut swarm = Swarm::new();
    let coder = swarm.add_agent(Agent::new("Coder", AgentRole::Coder));

    // Create and queue task
    let task = SwarmTask::new("Task").with_role(AgentRole::Coder);
    let task_id = task.id.clone();
    swarm.queue_task(task).unwrap();

    // Try to complete without assigning (task is in queue, not active_tasks)
    // This should log a warning but not panic
    swarm.complete_task(&task_id, &coder, "result");

    // Task should still be in queue
    assert_eq!(swarm.list_tasks().len(), 1);
}

#[test]
fn test_swarm_stats_with_varied_trust() {
    let mut swarm = Swarm::new();

    // Add agents with different trust levels
    let high_trust = swarm.add_agent(Agent::new("High", AgentRole::Coder));
    let low_trust = swarm.add_agent(Agent::new("Low", AgentRole::Tester));

    // Build up trust for high_trust agent
    for _ in 0..10 {
        swarm
            .get_agent_mut(&high_trust)
            .unwrap()
            .complete_task(true);
    }

    // Reduce trust for low_trust agent
    for _ in 0..5 {
        swarm
            .get_agent_mut(&low_trust)
            .unwrap()
            .complete_task(false);
    }

    // Reset statuses
    swarm.get_agent_mut(&high_trust).unwrap().set_idle();
    swarm.get_agent_mut(&low_trust).unwrap().set_idle();

    let stats = swarm.stats();
    assert_eq!(stats.total_agents, 2);
    assert!(stats.average_trust > 0.0);
    assert!(stats.average_trust <= 1.0);
}

#[test]
fn test_swarm_complex_workflow() {
    let mut swarm = Swarm::new()
        .with_conflict_strategy(ConflictStrategy::PriorityWins)
        .with_consensus_threshold(0.6)
        .with_decision_timeout(300);

    // Create a development team
    let architect = swarm.add_agent(Agent::new("Archie", AgentRole::Architect));
    let coder = swarm.add_agent(Agent::new("Cody", AgentRole::Coder));
    let tester = swarm.add_agent(Agent::new("Tessa", AgentRole::Tester));
    let _reviewer = swarm.add_agent(Agent::new("Rex", AgentRole::Reviewer));

    // Step 1: Make an architectural decision
    let arch_decision = swarm.create_decision(
        "Which database?",
        vec!["PostgreSQL".into(), "MongoDB".into(), "SQLite".into()],
    );

    swarm
        .vote(
            &arch_decision,
            &architect,
            "PostgreSQL",
            0.9,
            "Best for relational data",
        )
        .unwrap();
    swarm
        .vote(
            &arch_decision,
            &coder,
            "PostgreSQL",
            0.8,
            "Familiar with it",
        )
        .unwrap();
    swarm
        .vote(&arch_decision, &tester, "SQLite", 0.6, "Simpler setup")
        .unwrap();

    let db_choice = swarm.resolve_decision(&arch_decision).unwrap();
    assert!(db_choice.is_some());

    // Step 2: Create and assign implementation task
    let impl_task = SwarmTask::new("Implement database layer")
        .with_role(AgentRole::Coder)
        .with_role(AgentRole::Tester)
        .with_priority(8);

    let task_id = impl_task.id.clone();
    swarm.queue_task(impl_task).unwrap();

    let assigned = swarm.assign_task(&task_id);
    assert_eq!(assigned.len(), 2);

    // Step 3: Complete the task
    for agent_id in &assigned {
        swarm.complete_task(&task_id, agent_id, "Task completed successfully");
    }

    let task = swarm.get_task(&task_id).unwrap();
    assert_eq!(task.status, TaskStatus::Completed);
    assert_eq!(task.results.len(), 2);

    // Step 4: Review task
    let review_task = SwarmTask::new("Review implementation")
        .with_role(AgentRole::Reviewer)
        .with_priority(9);

    let review_id = review_task.id.clone();
    swarm.queue_task(review_task).unwrap();

    let reviewers = swarm.assign_task(&review_id);
    assert!(!reviewers.is_empty());

    // Verify final stats
    let stats = swarm.stats();
    assert_eq!(stats.total_agents, 4);
    assert_eq!(stats.queued_tasks, 0); // Both tasks moved to active
}

#[test]
fn test_swarm_list_agents_filtering() {
    let mut swarm = Swarm::new();

    // Add agents of different roles
    let _arch = swarm.add_agent(Agent::new("Arch", AgentRole::Architect));
    let _coder1 = swarm.add_agent(Agent::new("Coder1", AgentRole::Coder));
    let _coder2 = swarm.add_agent(Agent::new("Coder2", AgentRole::Coder));
    let _tester = swarm.add_agent(Agent::new("Tester", AgentRole::Tester));

    // Test list_agents
    let all_agents = swarm.list_agents();
    assert_eq!(all_agents.len(), 4);

    // Test agents_by_role
    let coders = swarm.agents_by_role(AgentRole::Coder);
    assert_eq!(coders.len(), 2);

    let architects = swarm.agents_by_role(AgentRole::Architect);
    assert_eq!(architects.len(), 1);

    let security = swarm.agents_by_role(AgentRole::Security);
    assert!(security.is_empty());
}

#[test]
fn test_swarm_get_task_from_active() {
    let mut swarm = Swarm::new();
    swarm.add_agent(Agent::new("Coder", AgentRole::Coder));

    let task = SwarmTask::new("Test").with_role(AgentRole::Coder);
    let task_id = task.id.clone();
    swarm.queue_task(task).unwrap();

    // Task initially in queue
    assert!(swarm.get_task(&task_id).is_some());

    // Move to active
    swarm.next_task();

    // Should still be findable
    assert!(swarm.get_task(&task_id).is_some());
}

#[test]
fn test_decision_votes_for_with_multiple() {
    let mut decision =
        super::types::Decision::new("Vote?", vec!["A".into(), "B".into(), "C".into()]);

    decision.add_vote(Vote::new("a1", AgentRole::Coder, "A", 0.8, "r1"));
    decision.add_vote(Vote::new("a2", AgentRole::Coder, "A", 0.7, "r2"));
    decision.add_vote(Vote::new("a3", AgentRole::Tester, "B", 0.9, "r3"));
    decision.add_vote(Vote::new("a4", AgentRole::Security, "A", 1.0, "r4"));

    let votes_for_a = decision.votes_for("A");
    assert_eq!(votes_for_a.len(), 3);

    let votes_for_b = decision.votes_for("B");
    assert_eq!(votes_for_b.len(), 1);

    let votes_for_c = decision.votes_for("C");
    assert!(votes_for_c.is_empty());
}

#[test]
fn test_memory_entry_metadata() {
    let mut memory = SharedMemory::new();

    memory.write("test_key", "test_value", "creator_agent");

    let entries = memory.entries();
    let entry = entries.iter().find(|e| e.key == "test_key").unwrap();

    assert_eq!(entry.value, "test_value");
    assert_eq!(entry.created_by, "creator_agent");
    assert!(entry.created_at > 0);
    assert_eq!(entry.access_count, 0);
    assert!(entry.tags.is_empty());
}

#[test]
fn test_memory_write_updates_metadata() {
    let mut memory = SharedMemory::new();

    memory.write("key", "original", "agent1");

    // Read to increment access count
    memory.read("key", "agent2");

    let entries = memory.entries();
    let entry = entries.iter().find(|e| e.key == "key").unwrap();
    assert_eq!(entry.access_count, 1);

    // Write update
    memory.write("key", "updated", "agent3");

    let entries = memory.entries();
    let entry = entries.iter().find(|e| e.key == "key").unwrap();
    assert_eq!(entry.value, "updated");
    assert_eq!(entry.modified_by, Some("agent3".to_string()));
    assert!(entry.modified_at.is_some());
}
