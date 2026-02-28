//! Swarm UI State Management
//!
//! Manages the shared state between the swarm system and the TUI.

use crate::orchestration::swarm::{
    Agent, AgentRole, AgentStatus, Decision, DecisionStatus, MemoryEntry, Swarm, SwarmTask,
    TaskStatus,
};
use crate::ui::tui::animation::agent_avatar::{ActivityLevel, AgentRole as AvatarRole};
use std::sync::{Arc, RwLock};
use tracing::warn;

/// Safely truncate a string to at most `max_bytes` bytes, ensuring the result
/// ends on a valid UTF-8 character boundary.
///
/// If `max_bytes >= s.len()`, the entire string is returned. Otherwise, the
/// returned slice is the longest prefix of `s` that is at most `max_bytes`
/// bytes and ends on a char boundary.
pub fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if max_bytes >= s.len() {
        return s;
    }
    // Find the largest index <= max_bytes that is a char boundary
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// UI-friendly representation of an agent
#[derive(Debug, Clone)]
pub struct AgentUiState {
    pub id: String,
    pub name: String,
    pub role: AgentRole,
    pub status: AgentStatus,
    pub activity: ActivityLevel,
    pub trust_score: f32,
    pub tokens_processed: u64,
    pub current_task: Option<String>,
    pub position: (u16, u16),
    pub success_rate: f32,
}

impl AgentUiState {
    /// Convert agent status to activity level
    fn status_to_activity(status: AgentStatus) -> ActivityLevel {
        match status {
            AgentStatus::Idle => ActivityLevel::Idle,
            AgentStatus::Working => ActivityLevel::High,
            AgentStatus::Waiting => ActivityLevel::Medium,
            AgentStatus::Completed => ActivityLevel::Complete,
            AgentStatus::Error => ActivityLevel::Error,
            AgentStatus::Paused => ActivityLevel::Idle,
        }
    }

    /// Create from a swarm Agent
    pub fn from_agent(agent: &Agent) -> Self {
        Self {
            id: agent.id.clone(),
            name: agent.name.clone(),
            role: agent.role,
            status: agent.status,
            activity: Self::status_to_activity(agent.status),
            trust_score: agent.trust_score,
            tokens_processed: 0,
            current_task: None,
            position: (0, 0),
            success_rate: agent.success_rate(),
        }
    }

    /// Get avatar role for UI rendering
    pub fn avatar_role(&self) -> Option<AvatarRole> {
        match self.role {
            AgentRole::Architect => Some(AvatarRole::Architect),
            AgentRole::Coder => Some(AvatarRole::Coder),
            AgentRole::Tester => Some(AvatarRole::Tester),
            AgentRole::Reviewer => Some(AvatarRole::Reviewer),
            AgentRole::Documenter => Some(AvatarRole::Documenter),
            AgentRole::DevOps => Some(AvatarRole::DevOps),
            AgentRole::Security => Some(AvatarRole::Security),
            AgentRole::Performance => Some(AvatarRole::Performance),
            _ => None,
        }
    }
}

/// View of a memory entry
#[derive(Debug, Clone)]
pub struct MemoryEntryView {
    pub key: String,
    pub value_preview: String,
    pub created_by: String,
    pub modified_by: Option<String>,
    pub tags: Vec<String>,
    pub access_count: u32,
}

impl MemoryEntryView {
    /// Create from a MemoryEntry
    pub fn from_entry(entry: &MemoryEntry) -> Self {
        let preview = if entry.value.chars().count() > 50 {
            format!("{}...", entry.value.chars().take(50).collect::<String>())
        } else {
            entry.value.clone()
        };

        Self {
            key: entry.key.clone(),
            value_preview: preview,
            created_by: entry.created_by.clone(),
            modified_by: entry.modified_by.clone(),
            tags: entry.tags.clone(),
            access_count: entry.access_count,
        }
    }
}

/// View of an active decision
#[derive(Debug, Clone)]
pub struct DecisionView {
    pub id: String,
    pub question: String,
    pub options: Vec<String>,
    pub vote_count: usize,
    pub status: DecisionStatus,
    pub outcome: Option<String>,
}

impl DecisionView {
    /// Create from a Decision
    pub fn from_decision(decision: &Decision) -> Self {
        Self {
            id: decision.id.clone(),
            question: decision.question.clone(),
            options: decision.options.clone(),
            vote_count: decision.votes.len(),
            status: decision.status,
            outcome: decision.outcome.clone(),
        }
    }
}

/// View of a task
#[derive(Debug, Clone)]
pub struct TaskView {
    pub id: String,
    pub description: String,
    pub priority: u8,
    pub status: TaskStatus,
    pub assigned_agents: Vec<String>,
    pub result_count: usize,
}

impl TaskView {
    /// Create from a SwarmTask
    pub fn from_task(task: &SwarmTask) -> Self {
        Self {
            id: task.id.clone(),
            description: task.description.clone(),
            priority: task.priority,
            status: task.status,
            assigned_agents: task.assigned_agents.clone(),
            result_count: task.results.len(),
        }
    }
}

/// Swarm event for the event log
#[derive(Debug, Clone)]
pub struct SwarmEvent {
    pub timestamp: String,
    pub event_type: EventType,
    pub message: String,
    pub agent_id: Option<String>,
}

/// Types of swarm events
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    AgentStarted,
    AgentCompleted,
    AgentError,
    TaskCreated,
    TaskCompleted,
    DecisionCreated,
    DecisionResolved,
    MemoryUpdated,
    ConsensusReached,
    ConflictDetected,
    VoteCast,
}

impl EventType {
    /// Get icon for event type
    pub fn icon(&self) -> &'static str {
        match self {
            EventType::AgentStarted => "▶",
            EventType::AgentCompleted => "✓",
            EventType::AgentError => "✗",
            EventType::TaskCreated => "📝",
            EventType::TaskCompleted => "✓",
            EventType::DecisionCreated => "⚖️",
            EventType::DecisionResolved => "✓",
            EventType::MemoryUpdated => "💾",
            EventType::ConsensusReached => "🤝",
            EventType::ConflictDetected => "⚠️",
            EventType::VoteCast => "🗳️",
        }
    }
}

/// Swarm statistics summary
#[derive(Debug, Clone, Default)]
pub struct SwarmStats {
    pub total_agents: usize,
    pub active_agents: usize,
    pub idle_agents: usize,
    pub pending_tasks: usize,
    pub completed_tasks: usize,
    pub pending_decisions: usize,
    pub average_trust: f32,
    pub memory_entries: usize,
}

/// Main swarm UI state
pub struct SwarmUiState {
    pub agents: Vec<AgentUiState>,
    pub memory_entries: Vec<MemoryEntryView>,
    pub decisions: Vec<DecisionView>,
    pub tasks: Vec<TaskView>,
    pub events: Vec<SwarmEvent>,
    pub stats: SwarmStats,
    swarm: Arc<RwLock<Swarm>>,
}

impl SwarmUiState {
    /// Create new swarm UI state
    pub fn new(swarm: Arc<RwLock<Swarm>>) -> Self {
        Self {
            agents: Vec::new(),
            memory_entries: Vec::new(),
            decisions: Vec::new(),
            tasks: Vec::new(),
            events: Vec::new(),
            stats: SwarmStats::default(),
            swarm,
        }
    }

    /// Sync state from the underlying swarm
    pub fn sync(&mut self) {
        // First, collect all the data we need
        let (agents_data, swarm_stats_opt, memory_entries_opt, decisions_data, tasks_data) = {
            let swarm = match self.swarm.read() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!("Swarm RwLock was poisoned during read; recovering inner data");
                    poisoned.into_inner()
                }
            };

            let agents: Vec<_> = swarm
                .list_agents()
                .iter()
                .map(|a| AgentUiState::from_agent(a))
                .collect();

            let stats = swarm.stats();

            let memory = swarm.memory();
            let mem = match memory.read() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!("Shared memory RwLock was poisoned during read; recovering inner data");
                    poisoned.into_inner()
                }
            };
            let entries = Some(
                mem.entries()
                    .iter()
                    .map(|e| MemoryEntryView::from_entry(e))
                    .collect::<Vec<_>>(),
            );

            let decisions: Vec<_> = swarm
                .list_decisions()
                .iter()
                .map(|d| DecisionView::from_decision(d))
                .collect();

            let tasks: Vec<_> = swarm
                .list_tasks()
                .iter()
                .map(|t| TaskView::from_task(t))
                .collect();

            (agents, Some(stats), entries, decisions, tasks)
        };

        // Now update self with the collected data
        self.agents = agents_data;
        self.decisions = decisions_data;
        self.tasks = tasks_data;

        // Calculate positions for visualization
        self.calculate_agent_positions();

        // Update memory entries (always Some now that we recover from poisoned locks)
        if let Some(entries) = memory_entries_opt {
            self.memory_entries = entries;
        }

        // Update stats (always Some now that we recover from poisoned locks)
        if let Some(swarm_stats) = swarm_stats_opt {
            self.update_stats(&swarm_stats);
        }

        self.stats.memory_entries = self.memory_entries.len();
    }

    /// Calculate positions for agent visualization
    fn calculate_agent_positions(&mut self) {
        let cols = 2u16;
        for (i, agent) in self.agents.iter_mut().enumerate() {
            let col = (i as u16) % cols;
            let row = (i as u16) / cols;
            agent.position = (col * 15, row * 5);
        }
    }

    /// Update statistics from swarm stats
    fn update_stats(&mut self, swarm_stats: &crate::orchestration::swarm::SwarmStats) {
        use crate::orchestration::swarm::AgentStatus;

        self.stats = SwarmStats {
            total_agents: swarm_stats.total_agents,
            active_agents: swarm_stats
                .agents_by_status
                .get(&AgentStatus::Working)
                .copied()
                .unwrap_or(0),
            idle_agents: swarm_stats
                .agents_by_status
                .get(&AgentStatus::Idle)
                .copied()
                .unwrap_or(0),
            pending_tasks: swarm_stats.queued_tasks,
            completed_tasks: swarm_stats
                .agents_by_status
                .get(&AgentStatus::Completed)
                .copied()
                .unwrap_or(0),
            pending_decisions: swarm_stats.pending_decisions,
            average_trust: swarm_stats.average_trust,
            memory_entries: self.memory_entries.len(),
        };
    }

    /// Add an event to the log
    pub fn add_event(
        &mut self,
        event_type: EventType,
        message: impl Into<String>,
        agent_id: Option<String>,
    ) {
        self.events.push(SwarmEvent {
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            event_type,
            message: message.into(),
            agent_id,
        });

        // Keep only last 100 events
        if self.events.len() > 100 {
            self.events.remove(0);
        }
    }

    /// Get agent by ID
    pub fn get_agent(&self, id: &str) -> Option<&AgentUiState> {
        self.agents.iter().find(|a| a.id == id)
    }

    /// Get mutable agent by ID
    pub fn get_agent_mut(&mut self, id: &str) -> Option<&mut AgentUiState> {
        self.agents.iter_mut().find(|a| a.id == id)
    }

    /// Get swarm reference
    pub fn swarm(&self) -> Arc<RwLock<Swarm>> {
        Arc::clone(&self.swarm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::swarm::{create_dev_swarm, Agent, AgentRole};

    #[test]
    fn test_safe_truncate_ascii() {
        assert_eq!(safe_truncate("hello world", 5), "hello");
        assert_eq!(safe_truncate("hello", 10), "hello");
        assert_eq!(safe_truncate("hello", 5), "hello");
        assert_eq!(safe_truncate("", 5), "");
        assert_eq!(safe_truncate("hello", 0), "");
    }

    #[test]
    fn test_safe_truncate_multibyte() {
        // "hello" in Japanese: each char is 3 bytes in UTF-8
        let s = "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}"; // こんにちは
        assert_eq!(s.len(), 15); // 5 chars * 3 bytes each

        // Truncate at 6 bytes: should get first 2 chars (6 bytes exactly)
        assert_eq!(safe_truncate(s, 6), "\u{3053}\u{3093}");

        // Truncate at 7 bytes: would split a 3-byte char, should round down to 6
        assert_eq!(safe_truncate(s, 7), "\u{3053}\u{3093}");

        // Truncate at 8 bytes: still in the middle of 3rd char, round down to 6
        assert_eq!(safe_truncate(s, 8), "\u{3053}\u{3093}");

        // Truncate at 9 bytes: exactly 3 chars
        assert_eq!(safe_truncate(s, 9), "\u{3053}\u{3093}\u{306b}");
    }

    #[test]
    fn test_safe_truncate_emoji() {
        // Emoji can be 4 bytes
        let s = "\u{1F600}\u{1F601}\u{1F602}"; // 3 emoji, 4 bytes each = 12 bytes
        assert_eq!(s.len(), 12);

        assert_eq!(safe_truncate(s, 4), "\u{1F600}");
        assert_eq!(safe_truncate(s, 5), "\u{1F600}"); // 5 is mid-char, rounds to 4
        assert_eq!(safe_truncate(s, 8), "\u{1F600}\u{1F601}");
        assert_eq!(safe_truncate(s, 12), s);
        assert_eq!(safe_truncate(s, 100), s);
    }

    #[test]
    fn test_safe_truncate_mixed() {
        let s = "ab\u{00e9}cd"; // a, b, e-acute (2 bytes), c, d = 6 bytes
        assert_eq!(s.len(), 6);

        assert_eq!(safe_truncate(s, 2), "ab");
        assert_eq!(safe_truncate(s, 3), "ab"); // 3 is mid-char of e-acute, rounds to 2
        assert_eq!(safe_truncate(s, 4), "ab\u{00e9}");
        assert_eq!(safe_truncate(s, 5), "ab\u{00e9}c");
    }

    #[test]
    fn test_agent_ui_state_from_agent() {
        let agent = Agent::new("Test", AgentRole::Coder);
        let ui_state = AgentUiState::from_agent(&agent);

        assert_eq!(ui_state.name, "Test");
        assert_eq!(ui_state.role, AgentRole::Coder);
        assert_eq!(ui_state.activity, ActivityLevel::Idle);
    }

    #[test]
    fn test_activity_from_status() {
        assert_eq!(
            AgentUiState::status_to_activity(AgentStatus::Working),
            ActivityLevel::High
        );
        assert_eq!(
            AgentUiState::status_to_activity(AgentStatus::Error),
            ActivityLevel::Error
        );
        assert_eq!(
            AgentUiState::status_to_activity(AgentStatus::Completed),
            ActivityLevel::Complete
        );
    }

    #[test]
    fn test_memory_entry_view() {
        use crate::orchestration::swarm::MemoryEntry;

        let entry = MemoryEntry {
            key: "test".to_string(),
            value: "This is a very long value that should be truncated!!!".to_string(),
            created_by: "agent1".to_string(),
            created_at: 0,
            modified_by: None,
            modified_at: None,
            access_count: 5,
            tags: vec!["tag1".to_string()],
        };

        let view = MemoryEntryView::from_entry(&entry);
        assert!(view.value_preview.ends_with("..."));
        assert_eq!(view.access_count, 5);
    }

    #[test]
    fn test_event_type_icon() {
        assert_eq!(EventType::AgentStarted.icon(), "▶");
        assert_eq!(EventType::ConsensusReached.icon(), "🤝");
        assert_eq!(EventType::ConflictDetected.icon(), "⚠️");
    }

    #[test]
    fn test_swarm_ui_state_sync() {
        let swarm = Arc::new(RwLock::new(create_dev_swarm()));
        let mut state = SwarmUiState::new(swarm);

        state.sync();

        assert_eq!(state.agents.len(), 4); // dev swarm has 4 agents
        assert!(state.stats.total_agents > 0);
    }

    #[test]
    fn test_add_event() {
        let swarm = Arc::new(RwLock::new(create_dev_swarm()));
        let mut state = SwarmUiState::new(swarm);

        state.add_event(
            EventType::AgentStarted,
            "Test message",
            Some("agent1".to_string()),
        );

        assert_eq!(state.events.len(), 1);
        assert_eq!(state.events[0].event_type, EventType::AgentStarted);
    }

    #[test]
    fn test_event_log_limit() {
        let swarm = Arc::new(RwLock::new(create_dev_swarm()));
        let mut state = SwarmUiState::new(swarm);

        for i in 0..150 {
            state.add_event(EventType::MemoryUpdated, format!("Event {}", i), None);
        }

        assert_eq!(state.events.len(), 100);
    }

    #[test]
    fn test_get_agent() {
        let swarm = Arc::new(RwLock::new(create_dev_swarm()));
        let mut state = SwarmUiState::new(swarm);
        state.sync();

        if let Some(first_agent) = state.agents.first() {
            let id = first_agent.id.clone();
            assert!(state.get_agent(&id).is_some());
            assert!(state.get_agent("nonexistent").is_none());
        }
    }

    #[test]
    fn test_get_agent_mut() {
        let swarm = Arc::new(RwLock::new(create_dev_swarm()));
        let mut state = SwarmUiState::new(swarm);
        state.sync();

        if let Some(first_agent) = state.agents.first() {
            let id = first_agent.id.clone();
            let agent = state.get_agent_mut(&id).unwrap();
            agent.trust_score = 0.99;
            assert!((state.get_agent(&id).unwrap().trust_score - 0.99).abs() < f32::EPSILON);
        }
        assert!(state.get_agent_mut("nonexistent").is_none());
    }

    #[test]
    fn test_avatar_role_mapping() {
        use crate::ui::tui::animation::agent_avatar::AgentRole as AvatarRole;

        let cases = vec![
            (AgentRole::Architect, Some(AvatarRole::Architect)),
            (AgentRole::Coder, Some(AvatarRole::Coder)),
            (AgentRole::Tester, Some(AvatarRole::Tester)),
            (AgentRole::Reviewer, Some(AvatarRole::Reviewer)),
            (AgentRole::Documenter, Some(AvatarRole::Documenter)),
            (AgentRole::DevOps, Some(AvatarRole::DevOps)),
            (AgentRole::Security, Some(AvatarRole::Security)),
            (AgentRole::Performance, Some(AvatarRole::Performance)),
            (AgentRole::General, None),
        ];

        for (role, expected) in cases {
            let agent = Agent::new("Test", role);
            let ui_state = AgentUiState::from_agent(&agent);
            assert_eq!(
                ui_state.avatar_role(),
                expected,
                "avatar_role mismatch for {:?}",
                role
            );
        }
    }

    #[test]
    fn test_status_to_activity_all_variants() {
        assert_eq!(
            AgentUiState::status_to_activity(AgentStatus::Idle),
            ActivityLevel::Idle
        );
        assert_eq!(
            AgentUiState::status_to_activity(AgentStatus::Working),
            ActivityLevel::High
        );
        assert_eq!(
            AgentUiState::status_to_activity(AgentStatus::Waiting),
            ActivityLevel::Medium
        );
        assert_eq!(
            AgentUiState::status_to_activity(AgentStatus::Completed),
            ActivityLevel::Complete
        );
        assert_eq!(
            AgentUiState::status_to_activity(AgentStatus::Error),
            ActivityLevel::Error
        );
        assert_eq!(
            AgentUiState::status_to_activity(AgentStatus::Paused),
            ActivityLevel::Idle
        );
    }

    #[test]
    fn test_agent_ui_state_from_agent_fields() {
        let mut agent = Agent::new("Alice", AgentRole::Architect);
        agent.trust_score = 0.9;
        agent.status = AgentStatus::Working;

        let ui = AgentUiState::from_agent(&agent);
        assert_eq!(ui.id, agent.id);
        assert_eq!(ui.name, "Alice");
        assert_eq!(ui.role, AgentRole::Architect);
        assert_eq!(ui.status, AgentStatus::Working);
        assert_eq!(ui.activity, ActivityLevel::High);
        assert!((ui.trust_score - 0.9).abs() < f32::EPSILON);
        assert_eq!(ui.tokens_processed, 0);
        assert!(ui.current_task.is_none());
        assert_eq!(ui.position, (0, 0));
        // New agent with no tasks has success_rate 1.0
        assert!((ui.success_rate - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_decision_view_from_decision() {
        use crate::orchestration::swarm::{Decision, DecisionStatus, Vote};

        let mut decision = Decision::new(
            "Use Rust or Go?",
            vec!["Rust".to_string(), "Go".to_string()],
        );
        // Add a vote
        decision
            .votes
            .push(Vote::new("agent1", AgentRole::Coder, "Rust", 0.9, "Performance"));

        let view = DecisionView::from_decision(&decision);
        assert_eq!(view.id, decision.id);
        assert_eq!(view.question, "Use Rust or Go?");
        assert_eq!(view.options, vec!["Rust".to_string(), "Go".to_string()]);
        assert_eq!(view.vote_count, 1);
        assert_eq!(view.status, DecisionStatus::Pending);
        assert!(view.outcome.is_none());
    }

    #[test]
    fn test_decision_view_with_outcome() {
        use crate::orchestration::swarm::{Decision, DecisionStatus};

        let mut decision = Decision::new("Framework?", vec!["A".to_string(), "B".to_string()]);
        decision.status = DecisionStatus::Resolved;
        decision.outcome = Some("A".to_string());

        let view = DecisionView::from_decision(&decision);
        assert_eq!(view.status, DecisionStatus::Resolved);
        assert_eq!(view.outcome, Some("A".to_string()));
    }

    #[test]
    fn test_task_view_from_task() {
        use crate::orchestration::swarm::{SwarmTask, TaskStatus};

        let task = SwarmTask::new("Implement auth")
            .with_role(AgentRole::Coder)
            .with_priority(8);

        let view = TaskView::from_task(&task);
        assert_eq!(view.id, task.id);
        assert_eq!(view.description, "Implement auth");
        assert_eq!(view.priority, 8);
        assert_eq!(view.status, TaskStatus::Pending);
        assert!(view.assigned_agents.is_empty());
        assert_eq!(view.result_count, 0);
    }

    #[test]
    fn test_task_view_with_results() {
        use crate::orchestration::swarm::SwarmTask;

        let mut task = SwarmTask::new("Build feature");
        task.results
            .insert("agent1".to_string(), "Done".to_string());
        task.results
            .insert("agent2".to_string(), "Done".to_string());
        task.assigned_agents = vec!["agent1".to_string(), "agent2".to_string()];

        let view = TaskView::from_task(&task);
        assert_eq!(view.result_count, 2);
        assert_eq!(view.assigned_agents.len(), 2);
    }

    #[test]
    fn test_memory_entry_view_short_value() {
        use crate::orchestration::swarm::MemoryEntry;

        let entry = MemoryEntry {
            key: "k".to_string(),
            value: "short".to_string(),
            created_by: "a1".to_string(),
            created_at: 0,
            modified_by: Some("a2".to_string()),
            modified_at: Some(100),
            access_count: 0,
            tags: vec![],
        };

        let view = MemoryEntryView::from_entry(&entry);
        assert_eq!(view.value_preview, "short");
        assert!(!view.value_preview.ends_with("..."));
        assert_eq!(view.key, "k");
        assert_eq!(view.created_by, "a1");
        assert_eq!(view.modified_by, Some("a2".to_string()));
        assert!(view.tags.is_empty());
    }

    #[test]
    fn test_memory_entry_view_exactly_50_chars() {
        use crate::orchestration::swarm::MemoryEntry;

        // Exactly 50 chars: should NOT truncate
        let value: String = "a".repeat(50);
        let entry = MemoryEntry {
            key: "k".to_string(),
            value: value.clone(),
            created_by: "a1".to_string(),
            created_at: 0,
            modified_by: None,
            modified_at: None,
            access_count: 0,
            tags: vec![],
        };

        let view = MemoryEntryView::from_entry(&entry);
        assert_eq!(view.value_preview, value);
        assert!(!view.value_preview.ends_with("..."));
    }

    #[test]
    fn test_memory_entry_view_51_chars_truncates() {
        use crate::orchestration::swarm::MemoryEntry;

        let value: String = "b".repeat(51);
        let entry = MemoryEntry {
            key: "k".to_string(),
            value,
            created_by: "a1".to_string(),
            created_at: 0,
            modified_by: None,
            modified_at: None,
            access_count: 0,
            tags: vec![],
        };

        let view = MemoryEntryView::from_entry(&entry);
        assert!(view.value_preview.ends_with("..."));
        // 50 chars + "..." = 53 chars
        assert_eq!(view.value_preview.chars().count(), 53);
    }

    #[test]
    fn test_calculate_agent_positions() {
        let swarm = Arc::new(RwLock::new(create_dev_swarm()));
        let mut state = SwarmUiState::new(swarm);
        state.sync();

        // Dev swarm has 4 agents, arranged in 2 columns
        assert_eq!(state.agents.len(), 4);
        // Agent 0: col=0, row=0 => (0*15, 0*5) = (0, 0)
        assert_eq!(state.agents[0].position, (0, 0));
        // Agent 1: col=1, row=0 => (1*15, 0*5) = (15, 0)
        assert_eq!(state.agents[1].position, (15, 0));
        // Agent 2: col=0, row=1 => (0*15, 1*5) = (0, 5)
        assert_eq!(state.agents[2].position, (0, 5));
        // Agent 3: col=1, row=1 => (1*15, 1*5) = (15, 5)
        assert_eq!(state.agents[3].position, (15, 5));
    }

    #[test]
    fn test_swarm_stats_default() {
        let stats = SwarmStats::default();
        assert_eq!(stats.total_agents, 0);
        assert_eq!(stats.active_agents, 0);
        assert_eq!(stats.idle_agents, 0);
        assert_eq!(stats.pending_tasks, 0);
        assert_eq!(stats.completed_tasks, 0);
        assert_eq!(stats.pending_decisions, 0);
        assert!((stats.average_trust - 0.0).abs() < f32::EPSILON);
        assert_eq!(stats.memory_entries, 0);
    }

    #[test]
    fn test_event_type_all_icons() {
        // Verify every variant has a non-empty icon
        let variants = vec![
            EventType::AgentStarted,
            EventType::AgentCompleted,
            EventType::AgentError,
            EventType::TaskCreated,
            EventType::TaskCompleted,
            EventType::DecisionCreated,
            EventType::DecisionResolved,
            EventType::MemoryUpdated,
            EventType::ConsensusReached,
            EventType::ConflictDetected,
            EventType::VoteCast,
        ];
        for v in variants {
            assert!(!v.icon().is_empty(), "icon empty for {:?}", v);
        }
    }

    #[test]
    fn test_add_event_without_agent_id() {
        let swarm = Arc::new(RwLock::new(create_dev_swarm()));
        let mut state = SwarmUiState::new(swarm);

        state.add_event(EventType::MemoryUpdated, "Memory changed", None);

        assert_eq!(state.events.len(), 1);
        assert!(state.events[0].agent_id.is_none());
        assert_eq!(state.events[0].message, "Memory changed");
        assert!(!state.events[0].timestamp.is_empty());
    }

    #[test]
    fn test_event_log_overflow_preserves_latest() {
        let swarm = Arc::new(RwLock::new(create_dev_swarm()));
        let mut state = SwarmUiState::new(swarm);

        for i in 0..110 {
            state.add_event(EventType::MemoryUpdated, format!("Event {}", i), None);
        }

        assert_eq!(state.events.len(), 100);
        // The oldest events (0..9) should have been evicted
        assert_eq!(state.events[0].message, "Event 10");
        assert_eq!(state.events[99].message, "Event 109");
    }

    #[test]
    fn test_swarm_ui_state_new_is_empty() {
        let swarm = Arc::new(RwLock::new(create_dev_swarm()));
        let state = SwarmUiState::new(swarm);

        // Before sync, all collections are empty
        assert!(state.agents.is_empty());
        assert!(state.memory_entries.is_empty());
        assert!(state.decisions.is_empty());
        assert!(state.tasks.is_empty());
        assert!(state.events.is_empty());
    }

    #[test]
    fn test_sync_updates_stats() {
        use crate::orchestration::swarm::Swarm;

        let mut swarm = Swarm::new();
        swarm.add_agent(Agent::new("A1", AgentRole::Coder));
        swarm.add_agent(Agent::new("A2", AgentRole::Tester));
        let swarm = Arc::new(RwLock::new(swarm));
        let mut state = SwarmUiState::new(swarm);

        state.sync();

        assert_eq!(state.stats.total_agents, 2);
        // Both agents start Idle
        assert_eq!(state.stats.idle_agents, 2);
        assert_eq!(state.stats.active_agents, 0);
    }

    #[test]
    fn test_swarm_ref_is_shared() {
        let swarm = Arc::new(RwLock::new(create_dev_swarm()));
        let state = SwarmUiState::new(Arc::clone(&swarm));

        // swarm() returns a clone of the Arc
        let s = state.swarm();
        assert!(Arc::ptr_eq(&s, &swarm));
    }
}
