//! Time-Travel Debugging
//!
//! Execution history with state snapshots, reverse stepping,
//! causality tracking, and what-if analysis.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Atomic counters for unique IDs
static SNAPSHOT_COUNTER: AtomicU64 = AtomicU64::new(0);
static EVENT_COUNTER: AtomicU64 = AtomicU64::new(0);
static BRANCH_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate unique snapshot ID
fn generate_snapshot_id() -> String {
    format!("snap-{}", SNAPSHOT_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Generate unique event ID
fn generate_event_id() -> String {
    format!("event-{}", EVENT_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Generate unique branch ID
fn generate_branch_id() -> String {
    format!("branch-{}", BRANCH_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Get current timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Execution event type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    /// State change
    StateChange,
    /// Function call
    FunctionCall,
    /// Function return
    FunctionReturn,
    /// Variable assignment
    Assignment,
    /// Condition evaluation
    Condition,
    /// Loop iteration
    LoopIteration,
    /// Exception/Error
    Exception,
    /// I/O operation
    IoOperation,
    /// Assertion
    Assertion,
    /// Checkpoint
    Checkpoint,
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventType::StateChange => write!(f, "State Change"),
            EventType::FunctionCall => write!(f, "Function Call"),
            EventType::FunctionReturn => write!(f, "Function Return"),
            EventType::Assignment => write!(f, "Assignment"),
            EventType::Condition => write!(f, "Condition"),
            EventType::LoopIteration => write!(f, "Loop Iteration"),
            EventType::Exception => write!(f, "Exception"),
            EventType::IoOperation => write!(f, "I/O Operation"),
            EventType::Assertion => write!(f, "Assertion"),
            EventType::Checkpoint => write!(f, "Checkpoint"),
        }
    }
}

/// An execution event in the timeline
#[derive(Debug, Clone)]
pub struct ExecutionEvent {
    /// Unique identifier
    pub id: String,
    /// Event type
    pub event_type: EventType,
    /// Timestamp
    pub timestamp: u64,
    /// Sequence number (monotonic)
    pub sequence: u64,
    /// Description
    pub description: String,
    /// Source location (file:line)
    pub location: Option<String>,
    /// Previous state (serialized)
    pub prev_state: Option<String>,
    /// New state (serialized)
    pub new_state: Option<String>,
    /// Cause event ID
    pub caused_by: Option<String>,
    /// Effects (event IDs caused by this event)
    pub effects: Vec<String>,
    /// Tags
    pub tags: Vec<String>,
    /// Metadata
    pub metadata: HashMap<String, String>,
}

impl ExecutionEvent {
    /// Create a new execution event
    pub fn new(event_type: EventType, sequence: u64) -> Self {
        Self {
            id: generate_event_id(),
            event_type,
            timestamp: current_timestamp(),
            sequence,
            description: String::new(),
            location: None,
            prev_state: None,
            new_state: None,
            caused_by: None,
            effects: Vec::new(),
            tags: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set location
    pub fn with_location(mut self, loc: impl Into<String>) -> Self {
        self.location = Some(loc.into());
        self
    }

    /// Set state change
    pub fn with_state_change(mut self, prev: impl Into<String>, new: impl Into<String>) -> Self {
        self.prev_state = Some(prev.into());
        self.new_state = Some(new.into());
        self
    }

    /// Set cause
    pub fn caused_by(mut self, event_id: impl Into<String>) -> Self {
        self.caused_by = Some(event_id.into());
        self
    }

    /// Add effect
    pub fn with_effect(mut self, event_id: impl Into<String>) -> Self {
        self.effects.push(event_id.into());
        self
    }

    /// Add tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Check if event has a state change
    pub fn has_state_change(&self) -> bool {
        self.prev_state.is_some() || self.new_state.is_some()
    }
}

/// State snapshot
#[derive(Debug, Clone)]
pub struct StateSnapshot {
    /// Unique identifier
    pub id: String,
    /// Timestamp
    pub timestamp: u64,
    /// Sequence number at snapshot
    pub sequence: u64,
    /// Description
    pub description: String,
    /// State data (serialized)
    pub state: String,
    /// Variables at this point
    pub variables: HashMap<String, String>,
    /// Call stack
    pub call_stack: Vec<StackFrame>,
    /// Previous snapshot ID
    pub parent_id: Option<String>,
    /// Branch ID
    pub branch_id: String,
}

impl StateSnapshot {
    /// Create a new snapshot
    pub fn new(sequence: u64, state: impl Into<String>) -> Self {
        Self {
            id: generate_snapshot_id(),
            timestamp: current_timestamp(),
            sequence,
            description: String::new(),
            state: state.into(),
            variables: HashMap::new(),
            call_stack: Vec::new(),
            parent_id: None,
            branch_id: "main".to_string(),
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Add variable
    pub fn with_variable(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.variables.insert(name.into(), value.into());
        self
    }

    /// Add stack frame
    pub fn with_frame(mut self, frame: StackFrame) -> Self {
        self.call_stack.push(frame);
        self
    }

    /// Set parent
    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    /// Set branch
    pub fn on_branch(mut self, branch_id: impl Into<String>) -> Self {
        self.branch_id = branch_id.into();
        self
    }
}

/// Stack frame
#[derive(Debug, Clone)]
pub struct StackFrame {
    /// Function name
    pub function: String,
    /// File name
    pub file: Option<String>,
    /// Line number
    pub line: Option<u32>,
    /// Local variables
    pub locals: HashMap<String, String>,
}

impl StackFrame {
    /// Create a new stack frame
    pub fn new(function: impl Into<String>) -> Self {
        Self {
            function: function.into(),
            file: None,
            line: None,
            locals: HashMap::new(),
        }
    }

    /// Set file
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    /// Set line
    pub fn with_line(mut self, line: u32) -> Self {
        self.line = Some(line);
        self
    }

    /// Add local variable
    pub fn with_local(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.locals.insert(name.into(), value.into());
        self
    }

    /// Get location string
    pub fn location(&self) -> String {
        match (&self.file, self.line) {
            (Some(f), Some(l)) => format!("{}:{}", f, l),
            (Some(f), None) => f.clone(),
            (None, Some(l)) => format!("line {}", l),
            (None, None) => "unknown".to_string(),
        }
    }
}

/// Timeline branch for what-if analysis
#[derive(Debug, Clone)]
pub struct TimelineBranch {
    /// Branch ID
    pub id: String,
    /// Branch name
    pub name: String,
    /// Parent branch ID
    pub parent_id: Option<String>,
    /// Fork point (snapshot ID)
    pub fork_point: Option<String>,
    /// Created timestamp
    pub created_at: u64,
    /// Description
    pub description: String,
    /// Is active branch
    pub is_active: bool,
}

impl TimelineBranch {
    /// Create a new branch
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: generate_branch_id(),
            name: name.into(),
            parent_id: None,
            fork_point: None,
            created_at: current_timestamp(),
            description: String::new(),
            is_active: false,
        }
    }

    /// Set parent
    pub fn from_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    /// Set fork point
    pub fn at_snapshot(mut self, snapshot_id: impl Into<String>) -> Self {
        self.fork_point = Some(snapshot_id.into());
        self
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set as active
    pub fn activate(mut self) -> Self {
        self.is_active = true;
        self
    }
}

/// Causality link between events
#[derive(Debug, Clone)]
pub struct CausalityLink {
    /// Cause event ID
    pub cause_id: String,
    /// Effect event ID
    pub effect_id: String,
    /// Link type
    pub link_type: CausalityType,
    /// Confidence (0.0-1.0)
    pub confidence: f32,
    /// Description
    pub description: String,
}

impl CausalityLink {
    /// Create a new causality link
    pub fn new(cause_id: impl Into<String>, effect_id: impl Into<String>) -> Self {
        Self {
            cause_id: cause_id.into(),
            effect_id: effect_id.into(),
            link_type: CausalityType::Direct,
            confidence: 1.0,
            description: String::new(),
        }
    }

    /// Set link type
    pub fn with_type(mut self, link_type: CausalityType) -> Self {
        self.link_type = link_type;
        self
    }

    /// Set confidence
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }
}

/// Causality link type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CausalityType {
    /// Direct causation
    Direct,
    /// Indirect causation
    Indirect,
    /// Correlation (not proven causal)
    Correlation,
    /// Triggering event
    Trigger,
    /// Enabling condition
    Enabler,
    /// Preventing event
    Preventer,
}

impl std::fmt::Display for CausalityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CausalityType::Direct => write!(f, "Direct"),
            CausalityType::Indirect => write!(f, "Indirect"),
            CausalityType::Correlation => write!(f, "Correlation"),
            CausalityType::Trigger => write!(f, "Trigger"),
            CausalityType::Enabler => write!(f, "Enabler"),
            CausalityType::Preventer => write!(f, "Preventer"),
        }
    }
}

/// What-if scenario
#[derive(Debug, Clone)]
pub struct WhatIfScenario {
    /// Scenario ID
    pub id: String,
    /// Scenario name
    pub name: String,
    /// Base snapshot ID
    pub base_snapshot_id: String,
    /// Modified variables
    pub modifications: HashMap<String, String>,
    /// Branch ID for this scenario
    pub branch_id: String,
    /// Result description
    pub result: Option<String>,
    /// Comparison with original
    pub comparison: Option<ScenarioComparison>,
}

impl WhatIfScenario {
    /// Create a new scenario
    pub fn new(name: impl Into<String>, base_snapshot_id: impl Into<String>) -> Self {
        Self {
            id: generate_branch_id(),
            name: name.into(),
            base_snapshot_id: base_snapshot_id.into(),
            modifications: HashMap::new(),
            branch_id: generate_branch_id(),
            result: None,
            comparison: None,
        }
    }

    /// Add modification
    pub fn with_modification(mut self, var: impl Into<String>, value: impl Into<String>) -> Self {
        self.modifications.insert(var.into(), value.into());
        self
    }

    /// Set result
    pub fn with_result(mut self, result: impl Into<String>) -> Self {
        self.result = Some(result.into());
        self
    }

    /// Set comparison
    pub fn with_comparison(mut self, comparison: ScenarioComparison) -> Self {
        self.comparison = Some(comparison);
        self
    }
}

/// Comparison between original and what-if scenario
#[derive(Debug, Clone)]
pub struct ScenarioComparison {
    /// Variables that changed
    pub changed_variables: Vec<VariableDiff>,
    /// Events that occurred differently
    pub different_events: Vec<EventDiff>,
    /// Overall outcome difference
    pub outcome_diff: String,
}

impl ScenarioComparison {
    /// Create a new comparison
    pub fn new(outcome_diff: impl Into<String>) -> Self {
        Self {
            changed_variables: Vec::new(),
            different_events: Vec::new(),
            outcome_diff: outcome_diff.into(),
        }
    }

    /// Add changed variable
    pub fn with_variable_diff(mut self, diff: VariableDiff) -> Self {
        self.changed_variables.push(diff);
        self
    }

    /// Add different event
    pub fn with_event_diff(mut self, diff: EventDiff) -> Self {
        self.different_events.push(diff);
        self
    }
}

/// Variable difference
#[derive(Debug, Clone)]
pub struct VariableDiff {
    /// Variable name
    pub name: String,
    /// Original value
    pub original: String,
    /// New value
    pub modified: String,
}

impl VariableDiff {
    /// Create a new diff
    pub fn new(
        name: impl Into<String>,
        original: impl Into<String>,
        modified: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            original: original.into(),
            modified: modified.into(),
        }
    }
}

/// Event difference
#[derive(Debug, Clone)]
pub struct EventDiff {
    /// Event sequence
    pub sequence: u64,
    /// Original event (if any)
    pub original: Option<String>,
    /// Modified event (if any)
    pub modified: Option<String>,
    /// Difference type
    pub diff_type: DiffType,
}

impl EventDiff {
    /// Create a new diff
    pub fn new(sequence: u64, diff_type: DiffType) -> Self {
        Self {
            sequence,
            original: None,
            modified: None,
            diff_type,
        }
    }

    /// Set original
    pub fn with_original(mut self, original: impl Into<String>) -> Self {
        self.original = Some(original.into());
        self
    }

    /// Set modified
    pub fn with_modified(mut self, modified: impl Into<String>) -> Self {
        self.modified = Some(modified.into());
        self
    }
}

/// Difference type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffType {
    /// Added in modified
    Added,
    /// Removed in modified
    Removed,
    /// Changed between versions
    Changed,
    /// Same in both
    Unchanged,
}

impl std::fmt::Display for DiffType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiffType::Added => write!(f, "Added"),
            DiffType::Removed => write!(f, "Removed"),
            DiffType::Changed => write!(f, "Changed"),
            DiffType::Unchanged => write!(f, "Unchanged"),
        }
    }
}

/// Time-travel debugger
#[derive(Debug)]
pub struct TimeTravelDebugger {
    /// Events in the timeline
    events: Vec<ExecutionEvent>,
    /// Snapshots
    snapshots: HashMap<String, StateSnapshot>,
    /// Branches
    branches: HashMap<String, TimelineBranch>,
    /// Causality links
    causality: Vec<CausalityLink>,
    /// What-if scenarios
    scenarios: HashMap<String, WhatIfScenario>,
    /// Current position in timeline
    current_sequence: u64,
    /// Current branch
    current_branch: String,
    /// Max events to keep
    max_events: usize,
    /// Auto-snapshot interval
    snapshot_interval: u64,
    /// Last snapshot sequence
    last_snapshot_seq: u64,
}

impl Default for TimeTravelDebugger {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeTravelDebugger {
    /// Create a new debugger
    pub fn new() -> Self {
        let mut branches = HashMap::new();
        let main_branch = TimelineBranch::new("main").activate();
        let main_id = main_branch.id.clone();
        branches.insert(main_id.clone(), main_branch);

        Self {
            events: Vec::new(),
            snapshots: HashMap::new(),
            branches,
            causality: Vec::new(),
            scenarios: HashMap::new(),
            current_sequence: 0,
            current_branch: main_id,
            max_events: 10000,
            snapshot_interval: 100,
            last_snapshot_seq: 0,
        }
    }

    /// Set max events
    pub fn with_max_events(mut self, max: usize) -> Self {
        self.max_events = max;
        self
    }

    /// Set snapshot interval
    pub fn with_snapshot_interval(mut self, interval: u64) -> Self {
        self.snapshot_interval = interval;
        self
    }

    /// Record an event
    pub fn record(&mut self, event: ExecutionEvent) -> String {
        let id = event.id.clone();
        self.current_sequence = event.sequence;
        self.events.push(event);

        // Trim old events if needed
        if self.events.len() > self.max_events {
            self.events.remove(0);
        }

        // Auto-snapshot if needed
        if self.current_sequence - self.last_snapshot_seq >= self.snapshot_interval {
            self.create_auto_snapshot();
        }

        id
    }

    /// Create an automatic snapshot
    fn create_auto_snapshot(&mut self) {
        let snapshot = StateSnapshot::new(self.current_sequence, "auto")
            .with_description("Auto-snapshot")
            .on_branch(self.current_branch.clone());
        self.last_snapshot_seq = self.current_sequence;
        self.snapshots.insert(snapshot.id.clone(), snapshot);
    }

    /// Create a manual snapshot
    pub fn snapshot(&mut self, state: impl Into<String>, description: impl Into<String>) -> String {
        let parent = self
            .snapshots
            .values()
            .filter(|s| s.branch_id == self.current_branch)
            .max_by_key(|s| s.sequence)
            .map(|s| s.id.clone());

        let mut snapshot = StateSnapshot::new(self.current_sequence, state)
            .with_description(description)
            .on_branch(self.current_branch.clone());

        if let Some(parent_id) = parent {
            snapshot = snapshot.with_parent(parent_id);
        }

        let id = snapshot.id.clone();
        self.last_snapshot_seq = self.current_sequence;
        self.snapshots.insert(id.clone(), snapshot);
        id
    }

    /// Get a snapshot
    pub fn get_snapshot(&self, id: &str) -> Option<&StateSnapshot> {
        self.snapshots.get(id)
    }

    /// Step forward
    pub fn step_forward(&mut self) -> Option<&ExecutionEvent> {
        if let Some(event) = self
            .events
            .iter()
            .find(|e| e.sequence > self.current_sequence)
        {
            self.current_sequence = event.sequence;
            Some(event)
        } else {
            None
        }
    }

    /// Step backward
    pub fn step_backward(&mut self) -> Option<&ExecutionEvent> {
        if let Some(event) = self
            .events
            .iter()
            .rev()
            .find(|e| e.sequence < self.current_sequence)
        {
            self.current_sequence = event.sequence;
            Some(event)
        } else {
            None
        }
    }

    /// Jump to sequence
    pub fn jump_to(&mut self, sequence: u64) -> Option<&ExecutionEvent> {
        if let Some(event) = self.events.iter().find(|e| e.sequence == sequence) {
            self.current_sequence = sequence;
            Some(event)
        } else {
            None
        }
    }

    /// Jump to snapshot
    pub fn restore_snapshot(&mut self, snapshot_id: &str) -> Option<&StateSnapshot> {
        if let Some(snapshot) = self.snapshots.get(snapshot_id) {
            self.current_sequence = snapshot.sequence;
            self.current_branch = snapshot.branch_id.clone();
            Some(snapshot)
        } else {
            None
        }
    }

    /// Get current event
    pub fn current_event(&self) -> Option<&ExecutionEvent> {
        self.events
            .iter()
            .find(|e| e.sequence == self.current_sequence)
    }

    /// Get current sequence
    pub fn current_sequence(&self) -> u64 {
        self.current_sequence
    }

    /// Get events in range
    pub fn events_in_range(&self, start: u64, end: u64) -> Vec<&ExecutionEvent> {
        self.events
            .iter()
            .filter(|e| e.sequence >= start && e.sequence <= end)
            .collect()
    }

    /// Get events by type
    pub fn events_by_type(&self, event_type: EventType) -> Vec<&ExecutionEvent> {
        self.events
            .iter()
            .filter(|e| e.event_type == event_type)
            .collect()
    }

    /// Add causality link
    pub fn add_causality(&mut self, link: CausalityLink) {
        // Also update the events
        if let Some(cause) = self.events.iter_mut().find(|e| e.id == link.cause_id) {
            cause.effects.push(link.effect_id.clone());
        }
        if let Some(effect) = self.events.iter_mut().find(|e| e.id == link.effect_id) {
            effect.caused_by = Some(link.cause_id.clone());
        }
        self.causality.push(link);
    }

    /// Find causes of an event
    pub fn find_causes(&self, event_id: &str) -> Vec<&CausalityLink> {
        self.causality
            .iter()
            .filter(|l| l.effect_id == event_id)
            .collect()
    }

    /// Find effects of an event
    pub fn find_effects(&self, event_id: &str) -> Vec<&CausalityLink> {
        self.causality
            .iter()
            .filter(|l| l.cause_id == event_id)
            .collect()
    }

    /// Create a branch for what-if analysis
    pub fn create_branch(
        &mut self,
        name: impl Into<String>,
        from_snapshot: &str,
    ) -> Option<String> {
        if !self.snapshots.contains_key(from_snapshot) {
            return None;
        }

        let branch = TimelineBranch::new(name)
            .from_parent(self.current_branch.clone())
            .at_snapshot(from_snapshot);

        let id = branch.id.clone();
        self.branches.insert(id.clone(), branch);
        Some(id)
    }

    /// Switch to branch
    pub fn switch_branch(&mut self, branch_id: &str) -> bool {
        // Check if target branch exists
        if !self.branches.contains_key(branch_id) {
            return false;
        }

        // Deactivate current branch
        let current_id = self.current_branch.clone();
        if let Some(current) = self.branches.get_mut(&current_id) {
            current.is_active = false;
        }

        // Activate target branch
        if let Some(branch) = self.branches.get_mut(branch_id) {
            branch.is_active = true;
        }

        self.current_branch = branch_id.to_string();
        true
    }

    /// Get current branch
    pub fn current_branch(&self) -> Option<&TimelineBranch> {
        self.branches.get(&self.current_branch)
    }

    /// Create what-if scenario
    pub fn create_scenario(&mut self, scenario: WhatIfScenario) -> String {
        let id = scenario.id.clone();
        self.scenarios.insert(id.clone(), scenario);
        id
    }

    /// Get scenario
    pub fn get_scenario(&self, id: &str) -> Option<&WhatIfScenario> {
        self.scenarios.get(id)
    }

    /// Get event count
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Get snapshot count
    pub fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }

    /// Get branch count
    pub fn branch_count(&self) -> usize {
        self.branches.len()
    }

    /// Search events
    pub fn search_events(&self, query: &str) -> Vec<&ExecutionEvent> {
        let query_lower = query.to_lowercase();
        self.events
            .iter()
            .filter(|e| {
                e.description.to_lowercase().contains(&query_lower)
                    || e.tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }

    /// Get timeline summary
    pub fn timeline_summary(&self) -> TimelineSummary {
        let events_by_type: HashMap<EventType, usize> =
            self.events.iter().fold(HashMap::new(), |mut acc, e| {
                *acc.entry(e.event_type).or_insert(0) += 1;
                acc
            });

        TimelineSummary {
            total_events: self.events.len(),
            total_snapshots: self.snapshots.len(),
            total_branches: self.branches.len(),
            current_sequence: self.current_sequence,
            current_branch: self.current_branch.clone(),
            events_by_type,
            causality_links: self.causality.len(),
            scenarios: self.scenarios.len(),
        }
    }
}

/// Timeline summary
#[derive(Debug, Clone)]
pub struct TimelineSummary {
    /// Total events
    pub total_events: usize,
    /// Total snapshots
    pub total_snapshots: usize,
    /// Total branches
    pub total_branches: usize,
    /// Current sequence
    pub current_sequence: u64,
    /// Current branch
    pub current_branch: String,
    /// Events by type
    pub events_by_type: HashMap<EventType, usize>,
    /// Causality links
    pub causality_links: usize,
    /// Scenarios
    pub scenarios: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_display() {
        assert_eq!(format!("{}", EventType::StateChange), "State Change");
        assert_eq!(format!("{}", EventType::FunctionCall), "Function Call");
    }

    #[test]
    fn test_execution_event_creation() {
        let event = ExecutionEvent::new(EventType::Assignment, 1)
            .with_description("x = 42")
            .with_location("main.rs:10");

        assert_eq!(event.event_type, EventType::Assignment);
        assert_eq!(event.sequence, 1);
        assert!(event.location.is_some());
    }

    #[test]
    fn test_execution_event_state_change() {
        let event =
            ExecutionEvent::new(EventType::StateChange, 1).with_state_change("x = 0", "x = 1");

        assert!(event.has_state_change());
        assert_eq!(event.prev_state, Some("x = 0".to_string()));
        assert_eq!(event.new_state, Some("x = 1".to_string()));
    }

    #[test]
    fn test_execution_event_causality() {
        let event = ExecutionEvent::new(EventType::FunctionReturn, 2)
            .caused_by("event-1")
            .with_effect("event-3");

        assert_eq!(event.caused_by, Some("event-1".to_string()));
        assert!(event.effects.contains(&"event-3".to_string()));
    }

    #[test]
    fn test_state_snapshot_creation() {
        let snapshot = StateSnapshot::new(10, r#"{"x": 42}"#)
            .with_description("Before function call")
            .with_variable("x", "42");

        assert_eq!(snapshot.sequence, 10);
        assert!(snapshot.variables.contains_key("x"));
    }

    #[test]
    fn test_state_snapshot_with_stack() {
        let frame = StackFrame::new("main").with_file("main.rs").with_line(10);

        let snapshot = StateSnapshot::new(1, "state").with_frame(frame);

        assert_eq!(snapshot.call_stack.len(), 1);
    }

    #[test]
    fn test_stack_frame_location() {
        let frame1 = StackFrame::new("func").with_file("test.rs").with_line(42);

        assert_eq!(frame1.location(), "test.rs:42");

        let frame2 = StackFrame::new("func");
        assert_eq!(frame2.location(), "unknown");
    }

    #[test]
    fn test_timeline_branch_creation() {
        let branch = TimelineBranch::new("feature")
            .from_parent("main")
            .at_snapshot("snap-1")
            .with_description("Testing feature");

        assert_eq!(branch.name, "feature");
        assert_eq!(branch.parent_id, Some("main".to_string()));
    }

    #[test]
    fn test_causality_link_creation() {
        let link = CausalityLink::new("cause", "effect")
            .with_type(CausalityType::Direct)
            .with_confidence(0.9);

        assert_eq!(link.link_type, CausalityType::Direct);
        assert!((link.confidence - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_causality_type_display() {
        assert_eq!(format!("{}", CausalityType::Direct), "Direct");
        assert_eq!(format!("{}", CausalityType::Trigger), "Trigger");
    }

    #[test]
    fn test_what_if_scenario() {
        let scenario = WhatIfScenario::new("Test scenario", "snap-1")
            .with_modification("x", "100")
            .with_result("Different outcome");

        assert!(!scenario.modifications.is_empty());
        assert!(scenario.result.is_some());
    }

    #[test]
    fn test_scenario_comparison() {
        let diff = VariableDiff::new("x", "0", "100");
        let comparison = ScenarioComparison::new("Different result").with_variable_diff(diff);

        assert_eq!(comparison.changed_variables.len(), 1);
    }

    #[test]
    fn test_event_diff() {
        let diff = EventDiff::new(5, DiffType::Changed)
            .with_original("x = 1")
            .with_modified("x = 2");

        assert_eq!(diff.diff_type, DiffType::Changed);
    }

    #[test]
    fn test_diff_type_display() {
        assert_eq!(format!("{}", DiffType::Added), "Added");
        assert_eq!(format!("{}", DiffType::Removed), "Removed");
    }

    #[test]
    fn test_time_travel_debugger_creation() {
        let debugger = TimeTravelDebugger::new();

        assert_eq!(debugger.event_count(), 0);
        assert_eq!(debugger.branch_count(), 1); // main branch
        assert_eq!(debugger.current_sequence(), 0);
    }

    #[test]
    fn test_time_travel_debugger_record() {
        let mut debugger = TimeTravelDebugger::new();

        let event = ExecutionEvent::new(EventType::Assignment, 1);
        debugger.record(event);

        assert_eq!(debugger.event_count(), 1);
        assert_eq!(debugger.current_sequence(), 1);
    }

    #[test]
    fn test_time_travel_debugger_snapshot() {
        let mut debugger = TimeTravelDebugger::new();

        let id = debugger.snapshot("state", "checkpoint");

        assert_eq!(debugger.snapshot_count(), 1);
        assert!(debugger.get_snapshot(&id).is_some());
    }

    #[test]
    fn test_time_travel_debugger_step_forward() {
        let mut debugger = TimeTravelDebugger::new();

        debugger.record(ExecutionEvent::new(EventType::Assignment, 1));
        debugger.record(ExecutionEvent::new(EventType::Assignment, 2));
        debugger.current_sequence = 0;

        let event = debugger.step_forward();
        assert!(event.is_some());
        assert_eq!(debugger.current_sequence(), 1);
    }

    #[test]
    fn test_time_travel_debugger_step_backward() {
        let mut debugger = TimeTravelDebugger::new();

        debugger.record(ExecutionEvent::new(EventType::Assignment, 1));
        debugger.record(ExecutionEvent::new(EventType::Assignment, 2));

        let event = debugger.step_backward();
        assert!(event.is_some());
        assert_eq!(debugger.current_sequence(), 1);
    }

    #[test]
    fn test_time_travel_debugger_jump_to() {
        let mut debugger = TimeTravelDebugger::new();

        debugger.record(ExecutionEvent::new(EventType::Assignment, 1));
        debugger.record(ExecutionEvent::new(EventType::Assignment, 5));
        debugger.record(ExecutionEvent::new(EventType::Assignment, 10));

        let event = debugger.jump_to(5);
        assert!(event.is_some());
        assert_eq!(debugger.current_sequence(), 5);
    }

    #[test]
    fn test_time_travel_debugger_events_by_type() {
        let mut debugger = TimeTravelDebugger::new();

        debugger.record(ExecutionEvent::new(EventType::Assignment, 1));
        debugger.record(ExecutionEvent::new(EventType::FunctionCall, 2));
        debugger.record(ExecutionEvent::new(EventType::Assignment, 3));

        let assignments = debugger.events_by_type(EventType::Assignment);
        assert_eq!(assignments.len(), 2);
    }

    #[test]
    fn test_time_travel_debugger_causality() {
        let mut debugger = TimeTravelDebugger::new();

        let e1 = debugger.record(ExecutionEvent::new(EventType::Assignment, 1));
        let e2 = debugger.record(ExecutionEvent::new(EventType::Assignment, 2));

        debugger.add_causality(CausalityLink::new(&e1, &e2));

        let causes = debugger.find_causes(&e2);
        assert_eq!(causes.len(), 1);

        let effects = debugger.find_effects(&e1);
        assert_eq!(effects.len(), 1);
    }

    #[test]
    fn test_time_travel_debugger_branch() {
        let mut debugger = TimeTravelDebugger::new();

        let snap_id = debugger.snapshot("state", "checkpoint");
        let branch_id = debugger.create_branch("feature", &snap_id);

        assert!(branch_id.is_some());
        assert_eq!(debugger.branch_count(), 2);
    }

    #[test]
    fn test_time_travel_debugger_switch_branch() {
        let mut debugger = TimeTravelDebugger::new();

        let snap_id = debugger.snapshot("state", "checkpoint");
        let branch_id = debugger.create_branch("feature", &snap_id).unwrap();

        assert!(debugger.switch_branch(&branch_id));
        assert_eq!(debugger.current_branch().unwrap().name, "feature");
    }

    #[test]
    fn test_time_travel_debugger_scenario() {
        let mut debugger = TimeTravelDebugger::new();

        let snap_id = debugger.snapshot("state", "checkpoint");
        let scenario = WhatIfScenario::new("Test", &snap_id);
        let scenario_id = debugger.create_scenario(scenario);

        assert!(debugger.get_scenario(&scenario_id).is_some());
    }

    #[test]
    fn test_time_travel_debugger_search() {
        let mut debugger = TimeTravelDebugger::new();

        debugger.record(ExecutionEvent::new(EventType::Assignment, 1).with_description("x = 42"));
        debugger.record(ExecutionEvent::new(EventType::Assignment, 2).with_description("y = 10"));

        let results = debugger.search_events("x =");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_time_travel_debugger_timeline_summary() {
        let mut debugger = TimeTravelDebugger::new();

        debugger.record(ExecutionEvent::new(EventType::Assignment, 1));
        debugger.record(ExecutionEvent::new(EventType::FunctionCall, 2));
        debugger.snapshot("state", "checkpoint");

        let summary = debugger.timeline_summary();

        assert_eq!(summary.total_events, 2);
        assert_eq!(summary.total_snapshots, 1);
    }

    #[test]
    fn test_unique_snapshot_ids() {
        let s1 = StateSnapshot::new(1, "a");
        let s2 = StateSnapshot::new(2, "b");

        assert_ne!(s1.id, s2.id);
    }

    #[test]
    fn test_unique_event_ids() {
        let e1 = ExecutionEvent::new(EventType::Assignment, 1);
        let e2 = ExecutionEvent::new(EventType::Assignment, 2);

        assert_ne!(e1.id, e2.id);
    }

    #[test]
    fn test_unique_branch_ids() {
        let b1 = TimelineBranch::new("a");
        let b2 = TimelineBranch::new("b");

        assert_ne!(b1.id, b2.id);
    }

    #[test]
    fn test_stack_frame_with_locals() {
        let frame = StackFrame::new("func")
            .with_local("x", "42")
            .with_local("y", "hello");

        assert_eq!(frame.locals.len(), 2);
    }

    #[test]
    fn test_events_in_range() {
        let mut debugger = TimeTravelDebugger::new();

        debugger.record(ExecutionEvent::new(EventType::Assignment, 1));
        debugger.record(ExecutionEvent::new(EventType::Assignment, 5));
        debugger.record(ExecutionEvent::new(EventType::Assignment, 10));

        let events = debugger.events_in_range(2, 8);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_restore_snapshot() {
        let mut debugger = TimeTravelDebugger::new();

        debugger.record(ExecutionEvent::new(EventType::Assignment, 1));
        let snap_id = debugger.snapshot("state", "checkpoint");
        debugger.record(ExecutionEvent::new(EventType::Assignment, 100));

        let snapshot = debugger.restore_snapshot(&snap_id);
        assert!(snapshot.is_some());
    }

    #[test]
    fn test_auto_snapshot() {
        let mut debugger = TimeTravelDebugger::new().with_snapshot_interval(2);

        debugger.record(ExecutionEvent::new(EventType::Assignment, 1));
        debugger.record(ExecutionEvent::new(EventType::Assignment, 2));
        debugger.record(ExecutionEvent::new(EventType::Assignment, 3));

        assert!(debugger.snapshot_count() >= 1);
    }

    #[test]
    fn test_max_events() {
        let mut debugger = TimeTravelDebugger::new().with_max_events(5);

        for i in 0..10 {
            debugger.record(ExecutionEvent::new(EventType::Assignment, i));
        }

        assert!(debugger.event_count() <= 5);
    }
}
