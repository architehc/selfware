//! Swarm System Tests

use std::collections::HashMap;
use std::sync::Arc;

use super::coordinator::{ConflictStrategy, Swarm};
use super::memory::SharedMemory;
use super::types::{
    Agent, AgentRole, AgentStatus, DecisionStatus, SwarmTask, TaskStatus, Vote,
};

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
    let entry = memory.data.get("key1").unwrap();
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

    let entry = memory.data.get("key1").unwrap();
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
    assert_eq!(swarm.conflict_strategy, ConflictStrategy::PriorityWins);
    assert!((swarm.consensus_threshold - 0.6).abs() < f32::EPSILON);
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

    assert_eq!(swarm.conflict_strategy, ConflictStrategy::MajorityWins);
    assert!((swarm.consensus_threshold - 0.7).abs() < 0.01);
}

#[test]
fn test_swarm_consensus_threshold_clamping() {
    let swarm = Swarm::new().with_consensus_threshold(2.0);
    assert!((swarm.consensus_threshold - 1.0).abs() < f32::EPSILON);

    let swarm2 = Swarm::new().with_consensus_threshold(-1.0);
    assert!(swarm2.consensus_threshold.abs() < f32::EPSILON);
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

    // Force conflict status
    swarm.get_decision(&did).unwrap(); // Ensure it exists
    if let Some(d) = swarm.decisions.get_mut(&did) {
        d.status = DecisionStatus::Conflict;
    }

    let result = swarm.resolve_conflict(&did).unwrap();
    // Security (priority 10) beats Coder (priority 4)
    assert_eq!(result, Some("A".to_string()));
}

#[test]
fn test_resolve_conflict_confidence_wins() {
    let mut swarm = Swarm::new().with_conflict_strategy(ConflictStrategy::ConfidenceWins);

    let a1 = swarm.add_agent(Agent::new("A1", AgentRole::Coder));
    let a2 = swarm.add_agent(Agent::new("A2", AgentRole::Coder));

    let did = swarm.create_decision("Pick?", vec!["X".into(), "Y".into()]);
    swarm.vote(&did, &a1, "X", 0.6, "r").unwrap();
    swarm.vote(&did, &a2, "Y", 0.95, "r").unwrap();

    if let Some(d) = swarm.decisions.get_mut(&did) {
        d.status = DecisionStatus::Conflict;
    }

    let result = swarm.resolve_conflict(&did).unwrap();
    assert_eq!(result, Some("Y".to_string()));
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

    if let Some(d) = swarm.decisions.get_mut(&did) {
        d.status = DecisionStatus::Conflict;
    }

    let result = swarm.resolve_conflict(&did).unwrap();
    assert_eq!(result, Some("A".to_string()));
}

#[test]
fn test_resolve_conflict_human_intervention() {
    let mut swarm = Swarm::new().with_conflict_strategy(ConflictStrategy::HumanIntervention);

    let a1 = swarm.add_agent(Agent::new("A1", AgentRole::Coder));

    let did = swarm.create_decision("Help?", vec!["A".into()]);
    swarm.vote(&did, &a1, "A", 0.8, "r").unwrap();

    if let Some(d) = swarm.decisions.get_mut(&did) {
        d.status = DecisionStatus::Conflict;
    }

    let result = swarm.resolve_conflict(&did).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_resolve_conflict_accept_all() {
    let mut swarm = Swarm::new().with_conflict_strategy(ConflictStrategy::AcceptAll);

    let a1 = swarm.add_agent(Agent::new("A1", AgentRole::Coder));
    let a2 = swarm.add_agent(Agent::new("A2", AgentRole::Tester));

    let did = swarm.create_decision("Merge?", vec!["X".into(), "Y".into()]);
    swarm.vote(&did, &a1, "X", 0.8, "r").unwrap();
    swarm.vote(&did, &a2, "Y", 0.7, "r").unwrap();

    if let Some(d) = swarm.decisions.get_mut(&did) {
        d.status = DecisionStatus::Conflict;
    }

    let result = swarm.resolve_conflict(&did).unwrap();
    let result_str = result.unwrap();
    // Result contains both choices joined by ", "
    assert!(result_str.contains("X"));
    assert!(result_str.contains("Y"));
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

#[test]
fn test_create_security_swarm() {
    let swarm = super::factory::create_security_swarm();
    assert!(!swarm.agents_by_role(AgentRole::Security).is_empty());
}

#[test]
fn test_create_security_swarm_has_roles() {
    let swarm = super::factory::create_security_swarm();
    assert_eq!(swarm.agents_by_role(AgentRole::Security).len(), 1);
    assert_eq!(swarm.agents_by_role(AgentRole::Reviewer).len(), 1);
    assert_eq!(swarm.agents_by_role(AgentRole::Tester).len(), 1);
    assert_eq!(swarm.list_agents().len(), 3);
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
    assert_eq!(swarm.decision_timeout_secs, 600);
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
