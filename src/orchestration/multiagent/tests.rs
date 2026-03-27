//! Multi-Agent System Tests

use std::time::Duration;

use tokio::sync::mpsc;

use crate::config::Config;
use crate::orchestration::multiagent::{
    AgentInstance, AgentResult, AgentStatus, MultiAgentChat, MultiAgentConfig, MultiAgentEvent,
    MultiAgentFailurePolicy, MAX_CONCURRENT_AGENTS,
};
use crate::swarm::AgentRole;

// ============================================================================
// Configuration Tests
// ============================================================================

#[test]
fn test_multiagent_config_default() {
    let config = MultiAgentConfig::default();
    assert_eq!(config.max_concurrency, 4);
    assert_eq!(config.roles.len(), 4);
    assert!(config.streaming);
}

#[test]
fn test_multiagent_config_with_concurrency() {
    let config = MultiAgentConfig::default().with_concurrency(16);
    assert_eq!(config.max_concurrency, 16);

    // Should cap at MAX_CONCURRENT_AGENTS
    let config = MultiAgentConfig::default().with_concurrency(100);
    assert_eq!(config.max_concurrency, MAX_CONCURRENT_AGENTS);
}

#[test]
fn test_multiagent_config_with_roles() {
    let roles = vec![AgentRole::Coder, AgentRole::Tester];
    let config = MultiAgentConfig::default().with_roles(roles.clone());
    assert_eq!(config.roles.len(), 2);
    assert!(config.roles.contains(&AgentRole::Coder));
    assert!(config.roles.contains(&AgentRole::Tester));
}

#[test]
fn test_failure_policy_default_is_fail_fast() {
    let policy = MultiAgentFailurePolicy::default();
    assert_eq!(policy, MultiAgentFailurePolicy::FailFast);
}

#[test]
fn test_failure_policy_variants_not_equal() {
    assert_ne!(
        MultiAgentFailurePolicy::BestEffort,
        MultiAgentFailurePolicy::FailFast
    );
}

#[test]
fn test_failure_policy_debug() {
    let best_effort = format!("{:?}", MultiAgentFailurePolicy::BestEffort);
    let fail_fast = format!("{:?}", MultiAgentFailurePolicy::FailFast);
    assert_eq!(best_effort, "BestEffort");
    assert_eq!(fail_fast, "FailFast");
}

#[test]
fn test_config_default_failure_policy() {
    let config = MultiAgentConfig::default();
    // The config default explicitly sets BestEffort (distinct from the enum default of FailFast)
    assert_eq!(config.failure_policy, MultiAgentFailurePolicy::BestEffort);
}

#[test]
fn test_config_default_roles_are_architect_coder_tester_reviewer() {
    let config = MultiAgentConfig::default();
    assert_eq!(config.roles.len(), 4);
    assert_eq!(config.roles[0], AgentRole::Architect);
    assert_eq!(config.roles[1], AgentRole::Coder);
    assert_eq!(config.roles[2], AgentRole::Tester);
    assert_eq!(config.roles[3], AgentRole::Reviewer);
}

#[test]
fn test_with_concurrency_clamps_zero_to_one() {
    let config = MultiAgentConfig::default().with_concurrency(0);
    assert_eq!(config.max_concurrency, 1);
}

#[test]
fn test_with_concurrency_clamps_large_to_max() {
    let config = MultiAgentConfig::default().with_concurrency(1000);
    assert_eq!(config.max_concurrency, MAX_CONCURRENT_AGENTS);
}

// ============================================================================
// Agent Instance Tests
// ============================================================================

#[test]
fn test_agent_instance() {
    let agent = AgentInstance {
        id: 0,
        role: AgentRole::Coder,
        name: "Test Agent".to_string(),
        messages: vec![],
        status: AgentStatus::Idle,
        last_heartbeat: std::time::Instant::now(),
    };
    assert_eq!(agent.status, AgentStatus::Idle);
}

#[test]
fn test_agent_status_variants() {
    assert_eq!(AgentStatus::Idle, AgentStatus::Idle);
    assert_eq!(AgentStatus::Working, AgentStatus::Working);
    assert_eq!(AgentStatus::Completed, AgentStatus::Completed);
    assert_eq!(AgentStatus::Failed, AgentStatus::Failed);
    assert_ne!(AgentStatus::Idle, AgentStatus::Working);
}

#[test]
fn test_agent_status_debug() {
    let status = AgentStatus::Idle;
    let debug_str = format!("{:?}", status);
    assert_eq!(debug_str, "Idle");
}

// ============================================================================
// Agent Result Tests
// ============================================================================

#[test]
fn test_agent_result() {
    let result = AgentResult {
        agent_id: 0,
        agent_name: "Test".to_string(),
        role: AgentRole::Coder,
        content: "Hello".to_string(),
        tool_calls: vec![],
        duration: Duration::from_secs(1),
        success: true,
        error: None,
    };
    assert!(result.success);
}

#[test]
fn test_aggregate_results() {
    let results = vec![
        AgentResult {
            agent_id: 0,
            agent_name: "Agent-0".to_string(),
            role: AgentRole::Coder,
            content: "Code here".to_string(),
            tool_calls: vec![],
            duration: Duration::from_secs(1),
            success: true,
            error: None,
        },
        AgentResult {
            agent_id: 1,
            agent_name: "Agent-1".to_string(),
            role: AgentRole::Tester,
            content: "Tests here".to_string(),
            tool_calls: vec![],
            duration: Duration::from_secs(2),
            success: true,
            error: None,
        },
    ];

    let summary = MultiAgentChat::aggregate_results(&results);
    assert!(summary.contains("Agent-0"));
    assert!(summary.contains("Agent-1"));
    assert!(summary.contains("Code here"));
    assert!(summary.contains("Tests here"));
}

#[test]
fn test_aggregate_results_with_failures() {
    let results = vec![
        AgentResult {
            agent_id: 0,
            agent_name: "Success".to_string(),
            role: AgentRole::Coder,
            content: "Good output".to_string(),
            tool_calls: vec![],
            duration: Duration::from_secs(1),
            success: true,
            error: None,
        },
        AgentResult {
            agent_id: 1,
            agent_name: "Failure".to_string(),
            role: AgentRole::Tester,
            content: "".to_string(),
            tool_calls: vec![],
            duration: Duration::from_secs(2),
            success: false,
            error: Some("Error occurred".to_string()),
        },
    ];

    let summary = MultiAgentChat::aggregate_results(&results);
    assert!(summary.contains("Success"));
    assert!(summary.contains("Good output"));
    assert!(summary.contains("Failure"));
    assert!(summary.contains("FAILED"));
    assert!(summary.contains("Error occurred"));
}

#[test]
fn test_aggregate_results_empty() {
    let results: Vec<AgentResult> = vec![];
    let summary = MultiAgentChat::aggregate_results(&results);
    assert!(summary.contains("Summary"));
    assert!(!summary.contains("###")); // No agent sections
}

// ============================================================================
// Event Tests
// ============================================================================

#[test]
fn test_multiagent_event_started() {
    let event = MultiAgentEvent::AgentStarted {
        agent_id: 0,
        name: "Test".to_string(),
        task: "Do something".to_string(),
    };
    if let MultiAgentEvent::AgentStarted {
        agent_id,
        name,
        task,
    } = event
    {
        assert_eq!(agent_id, 0);
        assert_eq!(name, "Test");
        assert_eq!(task, "Do something");
    }
}

#[test]
fn test_multiagent_event_progress() {
    let event = MultiAgentEvent::AgentProgress {
        agent_id: 1,
        content: "Working...".to_string(),
    };
    if let MultiAgentEvent::AgentProgress { agent_id, content } = event {
        assert_eq!(agent_id, 1);
        assert_eq!(content, "Working...");
    }
}

#[test]
fn test_multiagent_event_tool_call() {
    let event = MultiAgentEvent::AgentToolCall {
        agent_id: 2,
        tool: "shell_exec".to_string(),
    };
    if let MultiAgentEvent::AgentToolCall { agent_id, tool } = event {
        assert_eq!(agent_id, 2);
        assert_eq!(tool, "shell_exec");
    }
}

#[test]
fn test_multiagent_event_completed() {
    let result = AgentResult {
        agent_id: 0,
        agent_name: "Agent-0".to_string(),
        role: AgentRole::Coder,
        content: "Done".to_string(),
        tool_calls: vec![],
        duration: Duration::from_secs(10),
        success: true,
        error: None,
    };
    let event = MultiAgentEvent::AgentCompleted {
        agent_id: 0,
        result: result.clone(),
    };
    if let MultiAgentEvent::AgentCompleted {
        agent_id,
        result: r,
    } = event
    {
        assert_eq!(agent_id, 0);
        assert!(r.success);
    }
}

#[test]
fn test_multiagent_event_failed() {
    let event = MultiAgentEvent::AgentFailed {
        agent_id: 3,
        error: "Network error".to_string(),
    };
    if let MultiAgentEvent::AgentFailed { agent_id, error } = event {
        assert_eq!(agent_id, 3);
        assert!(error.contains("Network"));
    }
}

#[test]
fn test_multiagent_event_all_completed() {
    let results = vec![AgentResult {
        agent_id: 0,
        agent_name: "A".to_string(),
        role: AgentRole::Coder,
        content: "".to_string(),
        tool_calls: vec![],
        duration: Duration::from_secs(1),
        success: true,
        error: None,
    }];
    let event = MultiAgentEvent::AllCompleted {
        results: results.clone(),
        total_duration: Duration::from_secs(5),
    };
    if let MultiAgentEvent::AllCompleted {
        results: r,
        total_duration,
    } = event
    {
        assert_eq!(r.len(), 1);
        assert_eq!(total_duration.as_secs(), 5);
    }
}

// ============================================================================
// MultiAgentChat Construction Tests
// ============================================================================

#[test]
fn test_multiagent_chat_new_succeeds() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default();
    let chat = MultiAgentChat::new(&config, agent_config);
    assert!(chat.is_ok());
}

#[test]
fn test_multiagent_chat_new_with_custom_concurrency() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default().with_concurrency(8);
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();
    assert_eq!(chat.config.max_concurrency, 8);
}

#[test]
fn test_multiagent_chat_with_events() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default();
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();
    let (tx, _rx) = mpsc::channel::<MultiAgentEvent>(100);
    let chat = chat.with_events(tx);
    assert!(chat.event_tx.is_some());
}

#[test]
fn test_semaphore_permits_match_clamped_concurrency() {
    let config = Config::default();

    // Concurrency 1
    let agent_config = MultiAgentConfig::default().with_concurrency(1);
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();
    assert_eq!(chat.semaphore.available_permits(), 1);

    // Concurrency 8
    let agent_config = MultiAgentConfig::default().with_concurrency(8);
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();
    assert_eq!(chat.semaphore.available_permits(), 8);

    // Concurrency 16 (max)
    let agent_config = MultiAgentConfig::default().with_concurrency(16);
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();
    assert_eq!(chat.semaphore.available_permits(), 16);

    // Concurrency 100 (clamped to 16)
    let agent_config = MultiAgentConfig::default().with_concurrency(100);
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();
    assert_eq!(chat.semaphore.available_permits(), 16);
}

// ============================================================================
// Async Tests
// ============================================================================

#[tokio::test]
async fn test_initialize_agents_creates_agents() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default();
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();

    chat.initialize_agents().await.unwrap();

    let agents = chat.agents.read().await;
    assert_eq!(agents.len(), 4);
}

#[tokio::test]
async fn test_initialize_agents_sets_correct_roles() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default();
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();

    chat.initialize_agents().await.unwrap();

    let agents = chat.agents.read().await;
    assert_eq!(agents[0].role, AgentRole::Architect);
    assert_eq!(agents[1].role, AgentRole::Coder);
    assert_eq!(agents[2].role, AgentRole::Tester);
    assert_eq!(agents[3].role, AgentRole::Reviewer);
}

#[tokio::test]
async fn test_initialize_agents_all_idle() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default();
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();

    chat.initialize_agents().await.unwrap();

    let agents = chat.agents.read().await;
    for agent in agents.iter() {
        assert_eq!(agent.status, AgentStatus::Idle);
    }
}

#[tokio::test]
async fn test_initialize_agents_clears_previous() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default();
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();

    // Initialize twice -- second call should reset
    chat.initialize_agents().await.unwrap();
    chat.initialize_agents().await.unwrap();

    let agents = chat.agents.read().await;
    assert_eq!(agents.len(), 4);
}

#[tokio::test]
async fn test_is_agent_healthy_idle_agent() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default();
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();

    chat.initialize_agents().await.unwrap();

    // Fresh idle agents should be healthy
    assert!(chat.is_agent_healthy(0).await);
    assert!(chat.is_agent_healthy(1).await);
    assert!(chat.is_agent_healthy(2).await);
    assert!(chat.is_agent_healthy(3).await);
}

#[tokio::test]
async fn test_is_agent_healthy_nonexistent_agent() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default();
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();

    chat.initialize_agents().await.unwrap();

    // Agent ID out of range should not be healthy
    assert!(!chat.is_agent_healthy(999).await);
}

#[tokio::test]
async fn test_is_agent_healthy_failed_agent() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default();
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();

    chat.initialize_agents().await.unwrap();

    // Mark agent as failed
    {
        let mut agents = chat.agents.write().await;
        agents[0].status = AgentStatus::Failed;
    }

    assert!(!chat.is_agent_healthy(0).await);
    // Other agents should still be healthy
    assert!(chat.is_agent_healthy(1).await);
}

#[tokio::test]
async fn test_results_initially_empty() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default();
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();

    let results = chat.results.lock().await;
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_initialize_agents_concurrent_safe() {
    use std::sync::Arc;

    let config = Config::default();
    let agent_config =
        MultiAgentConfig::default().with_roles(vec![AgentRole::Coder, AgentRole::Tester]);
    let chat = Arc::new(MultiAgentChat::new(&config, agent_config).unwrap());

    // Initialize from two concurrent tasks
    let chat1 = Arc::clone(&chat);
    let chat2 = Arc::clone(&chat);
    let (r1, r2) = tokio::join!(async move { chat1.initialize_agents().await }, async move {
        chat2.initialize_agents().await
    },);
    r1.unwrap();
    r2.unwrap();

    // After concurrent initialization, we should have exactly 2 agents
    let agents = chat.agents.read().await;
    assert_eq!(agents.len(), 2);
}
