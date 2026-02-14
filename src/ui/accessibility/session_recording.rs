//! Session Recording & Workflow Tools
//!
//! Provides session recording/replay, persistent undo,
//! workflow templates, and code provenance tracking.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);
static EVENT_COUNTER: AtomicU64 = AtomicU64::new(1);
static TEMPLATE_COUNTER: AtomicU64 = AtomicU64::new(1);

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ============================================================================
// Session Recording
// ============================================================================

/// Event type in a session
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    /// User input (command, message)
    UserInput,
    /// Agent response
    AgentResponse,
    /// Tool execution
    ToolExecution,
    /// File read
    FileRead,
    /// File write
    FileWrite,
    /// File edit
    FileEdit,
    /// Command execution
    CommandExecution,
    /// Error occurred
    Error,
    /// Checkpoint created
    Checkpoint,
    /// Thinking/reasoning
    Thinking,
}

/// Session event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    /// Event ID
    pub event_id: String,
    /// Event type
    pub event_type: EventType,
    /// Timestamp
    pub timestamp: u64,
    /// Event data (JSON serialized)
    pub data: String,
    /// Parent event ID (for nested events)
    pub parent_id: Option<String>,
    /// Duration in milliseconds
    pub duration_ms: Option<u64>,
    /// Metadata
    pub metadata: HashMap<String, String>,
}

impl SessionEvent {
    pub fn new(event_type: EventType, data: impl Into<String>) -> Self {
        let event_id = format!("evt_{}", EVENT_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            event_id,
            event_type,
            timestamp: current_timestamp(),
            data: data.into(),
            parent_id: None,
            duration_ms: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Recording session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingSession {
    /// Session ID
    pub session_id: String,
    /// Session name
    pub name: String,
    /// Description
    pub description: String,
    /// Start time
    pub started_at: u64,
    /// End time
    pub ended_at: Option<u64>,
    /// Events in order
    pub events: Vec<SessionEvent>,
    /// Session metadata
    pub metadata: HashMap<String, String>,
    /// Tags
    pub tags: Vec<String>,
}

impl RecordingSession {
    pub fn new(name: impl Into<String>) -> Self {
        let session_id = format!("sess_{}", SESSION_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            session_id,
            name: name.into(),
            description: String::new(),
            started_at: current_timestamp(),
            ended_at: None,
            events: Vec::new(),
            metadata: HashMap::new(),
            tags: Vec::new(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn record(&mut self, event: SessionEvent) {
        self.events.push(event);
    }

    pub fn end(&mut self) {
        self.ended_at = Some(current_timestamp());
    }

    pub fn duration(&self) -> Duration {
        let end = self.ended_at.unwrap_or_else(current_timestamp);
        Duration::from_secs(end - self.started_at)
    }

    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    pub fn events_by_type(&self, event_type: EventType) -> Vec<&SessionEvent> {
        self.events
            .iter()
            .filter(|e| e.event_type == event_type)
            .collect()
    }

    pub fn errors(&self) -> Vec<&SessionEvent> {
        self.events_by_type(EventType::Error)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Session recorder
#[derive(Debug, Clone)]
pub struct SessionRecorder {
    /// Current session
    pub current: Option<RecordingSession>,
    /// Past sessions
    pub sessions: Vec<RecordingSession>,
    /// Recording enabled
    pub enabled: bool,
    /// Max sessions to keep
    pub max_sessions: usize,
}

impl SessionRecorder {
    pub fn new() -> Self {
        Self {
            current: None,
            sessions: Vec::new(),
            enabled: true,
            max_sessions: 100,
        }
    }

    pub fn start_session(&mut self, name: impl Into<String>) -> &RecordingSession {
        if let Some(mut current) = self.current.take() {
            current.end();
            self.sessions.push(current);
        }

        // Trim old sessions if needed
        while self.sessions.len() >= self.max_sessions {
            self.sessions.remove(0);
        }

        self.current = Some(RecordingSession::new(name));
        self.current.as_ref().unwrap()
    }

    pub fn record(&mut self, event: SessionEvent) {
        if self.enabled {
            if let Some(ref mut session) = self.current {
                session.record(event);
            }
        }
    }

    pub fn end_session(&mut self) {
        if let Some(mut current) = self.current.take() {
            current.end();
            self.sessions.push(current);
        }
    }

    pub fn current_session(&self) -> Option<&RecordingSession> {
        self.current.as_ref()
    }

    pub fn get_session(&self, session_id: &str) -> Option<&RecordingSession> {
        self.sessions.iter().find(|s| s.session_id == session_id)
    }

    pub fn recent_sessions(&self, count: usize) -> Vec<&RecordingSession> {
        self.sessions.iter().rev().take(count).collect()
    }

    pub fn search_sessions(&self, query: &str) -> Vec<&RecordingSession> {
        let query_lower = query.to_lowercase();
        self.sessions
            .iter()
            .filter(|s| {
                s.name.to_lowercase().contains(&query_lower)
                    || s.description.to_lowercase().contains(&query_lower)
                    || s.tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }
}

impl Default for SessionRecorder {
    fn default() -> Self {
        Self::new()
    }
}

/// Session player for replay
#[derive(Debug, Clone)]
pub struct SessionPlayer {
    /// Session to replay
    session: RecordingSession,
    /// Current event index
    current_index: usize,
    /// Playback speed multiplier
    pub speed: f64,
    /// Paused state
    pub paused: bool,
}

impl SessionPlayer {
    pub fn new(session: RecordingSession) -> Self {
        Self {
            session,
            current_index: 0,
            speed: 1.0,
            paused: false,
        }
    }

    pub fn next_event(&mut self) -> Option<&SessionEvent> {
        if self.current_index < self.session.events.len() {
            let event = &self.session.events[self.current_index];
            self.current_index += 1;
            Some(event)
        } else {
            None
        }
    }

    pub fn previous_event(&mut self) -> Option<&SessionEvent> {
        if self.current_index > 0 {
            self.current_index -= 1;
            Some(&self.session.events[self.current_index])
        } else {
            None
        }
    }

    pub fn seek(&mut self, index: usize) {
        self.current_index = index.min(self.session.events.len());
    }

    pub fn seek_to_timestamp(&mut self, timestamp: u64) {
        for (i, event) in self.session.events.iter().enumerate() {
            if event.timestamp >= timestamp {
                self.current_index = i;
                return;
            }
        }
        self.current_index = self.session.events.len();
    }

    pub fn reset(&mut self) {
        self.current_index = 0;
    }

    pub fn is_complete(&self) -> bool {
        self.current_index >= self.session.events.len()
    }

    pub fn progress(&self) -> f64 {
        if self.session.events.is_empty() {
            1.0
        } else {
            self.current_index as f64 / self.session.events.len() as f64
        }
    }
}

// ============================================================================
// Persistent Undo
// ============================================================================

/// Change type for undo
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    Create,
    Modify,
    Delete,
    Move,
}

/// Change record for undo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeRecord {
    /// Change ID
    pub change_id: String,
    /// File path
    pub file_path: PathBuf,
    /// Change type
    pub change_type: ChangeType,
    /// Content before (None for create)
    pub before: Option<String>,
    /// Content after (None for delete)
    pub after: Option<String>,
    /// Timestamp
    pub timestamp: u64,
    /// Session ID
    pub session_id: Option<String>,
    /// Description
    pub description: String,
    /// Line range (start, end)
    pub line_range: Option<(u32, u32)>,
}

impl ChangeRecord {
    pub fn create(file_path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        Self {
            change_id: format!("chg_{}", EVENT_COUNTER.fetch_add(1, Ordering::SeqCst)),
            file_path: file_path.into(),
            change_type: ChangeType::Create,
            before: None,
            after: Some(content.into()),
            timestamp: current_timestamp(),
            session_id: None,
            description: String::new(),
            line_range: None,
        }
    }

    pub fn modify(
        file_path: impl Into<PathBuf>,
        before: impl Into<String>,
        after: impl Into<String>,
    ) -> Self {
        Self {
            change_id: format!("chg_{}", EVENT_COUNTER.fetch_add(1, Ordering::SeqCst)),
            file_path: file_path.into(),
            change_type: ChangeType::Modify,
            before: Some(before.into()),
            after: Some(after.into()),
            timestamp: current_timestamp(),
            session_id: None,
            description: String::new(),
            line_range: None,
        }
    }

    pub fn delete(file_path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        Self {
            change_id: format!("chg_{}", EVENT_COUNTER.fetch_add(1, Ordering::SeqCst)),
            file_path: file_path.into(),
            change_type: ChangeType::Delete,
            before: Some(content.into()),
            after: None,
            timestamp: current_timestamp(),
            session_id: None,
            description: String::new(),
            line_range: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn with_line_range(mut self, start: u32, end: u32) -> Self {
        self.line_range = Some((start, end));
        self
    }

    pub fn age_days(&self) -> u64 {
        (current_timestamp() - self.timestamp) / 86400
    }

    pub fn invert(&self) -> ChangeRecord {
        ChangeRecord {
            change_id: format!("chg_{}", EVENT_COUNTER.fetch_add(1, Ordering::SeqCst)),
            file_path: self.file_path.clone(),
            change_type: match self.change_type {
                ChangeType::Create => ChangeType::Delete,
                ChangeType::Delete => ChangeType::Create,
                ChangeType::Modify => ChangeType::Modify,
                ChangeType::Move => ChangeType::Move,
            },
            before: self.after.clone(),
            after: self.before.clone(),
            timestamp: current_timestamp(),
            session_id: self.session_id.clone(),
            description: format!("Undo: {}", self.description),
            line_range: self.line_range,
        }
    }
}

/// Persistent undo history
#[derive(Debug, Clone)]
pub struct UndoHistory {
    /// Change records
    pub changes: Vec<ChangeRecord>,
    /// Maximum age to keep (days)
    pub max_age_days: u64,
    /// Maximum changes to keep
    pub max_changes: usize,
    /// Redo stack
    pub redo_stack: Vec<ChangeRecord>,
}

impl UndoHistory {
    pub fn new() -> Self {
        Self {
            changes: Vec::new(),
            max_age_days: 14, // 2 weeks default
            max_changes: 10000,
            redo_stack: Vec::new(),
        }
    }

    pub fn with_retention(mut self, days: u64) -> Self {
        self.max_age_days = days;
        self
    }

    pub fn record(&mut self, change: ChangeRecord) {
        // Clear redo stack on new change
        self.redo_stack.clear();

        self.changes.push(change);
        self.cleanup();
    }

    pub fn undo(&mut self) -> Option<ChangeRecord> {
        if let Some(change) = self.changes.pop() {
            let inverted = change.invert();
            self.redo_stack.push(change);
            Some(inverted)
        } else {
            None
        }
    }

    pub fn redo(&mut self) -> Option<ChangeRecord> {
        if let Some(change) = self.redo_stack.pop() {
            self.changes.push(change.clone());
            Some(change)
        } else {
            None
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.changes.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    fn cleanup(&mut self) {
        // Remove old changes
        let cutoff = current_timestamp() - (self.max_age_days * 86400);
        self.changes.retain(|c| c.timestamp >= cutoff);

        // Trim if too many
        while self.changes.len() > self.max_changes {
            self.changes.remove(0);
        }
    }

    pub fn changes_for_file(&self, file_path: &PathBuf) -> Vec<&ChangeRecord> {
        self.changes
            .iter()
            .filter(|c| &c.file_path == file_path)
            .collect()
    }

    pub fn changes_in_session(&self, session_id: &str) -> Vec<&ChangeRecord> {
        self.changes
            .iter()
            .filter(|c| c.session_id.as_deref() == Some(session_id))
            .collect()
    }

    pub fn changes_since(&self, timestamp: u64) -> Vec<&ChangeRecord> {
        self.changes
            .iter()
            .filter(|c| c.timestamp >= timestamp)
            .collect()
    }

    pub fn undo_to_timestamp(&mut self, target_timestamp: u64) -> Vec<ChangeRecord> {
        let mut undone = Vec::new();

        while let Some(last) = self.changes.last() {
            if last.timestamp <= target_timestamp {
                break;
            }
            if let Some(inverted) = self.undo() {
                undone.push(inverted);
            }
        }

        undone
    }
}

impl Default for UndoHistory {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Workflow Templates
// ============================================================================

/// Workflow step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// Step name
    pub name: String,
    /// Step description
    pub description: String,
    /// Action to perform
    pub action: WorkflowAction,
    /// Parameters
    pub parameters: HashMap<String, String>,
    /// Condition (expression)
    pub condition: Option<String>,
}

/// Workflow action types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowAction {
    RunCommand(String),
    CreateFile(String, String),       // path, content
    EditFile(String, String, String), // path, search, replace
    DeleteFile(String),
    UserPrompt(String),
    WaitForApproval,
    RunTests,
    CommitChanges(String),
    Custom(String),
}

/// Workflow template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowTemplate {
    /// Template ID
    pub template_id: String,
    /// Template name
    pub name: String,
    /// Description
    pub description: String,
    /// Author
    pub author: String,
    /// Version
    pub version: String,
    /// Category
    pub category: String,
    /// Steps
    pub steps: Vec<WorkflowStep>,
    /// Input parameters
    pub inputs: Vec<TemplateInput>,
    /// Tags
    pub tags: Vec<String>,
    /// Downloads count
    pub downloads: u64,
    /// Rating (1-5)
    pub rating: f64,
    /// Created timestamp
    pub created_at: u64,
}

/// Template input parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateInput {
    /// Parameter name
    pub name: String,
    /// Description
    pub description: String,
    /// Type
    pub input_type: InputType,
    /// Default value
    pub default: Option<String>,
    /// Required
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputType {
    String,
    Path,
    Boolean,
    Choice,
    Number,
}

impl WorkflowTemplate {
    pub fn new(name: impl Into<String>, author: impl Into<String>) -> Self {
        let template_id = format!("tmpl_{}", TEMPLATE_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            template_id,
            name: name.into(),
            description: String::new(),
            author: author.into(),
            version: "1.0.0".to_string(),
            category: "general".to_string(),
            steps: Vec::new(),
            inputs: Vec::new(),
            tags: Vec::new(),
            downloads: 0,
            rating: 0.0,
            created_at: current_timestamp(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = category.into();
        self
    }

    pub fn add_step(&mut self, step: WorkflowStep) {
        self.steps.push(step);
    }

    pub fn add_input(&mut self, input: TemplateInput) {
        self.inputs.push(input);
    }

    pub fn to_yaml(&self) -> String {
        let mut yaml = String::new();
        yaml.push_str(&format!("name: {}\n", self.name));
        yaml.push_str(&format!("version: {}\n", self.version));
        yaml.push_str(&format!("author: {}\n", self.author));
        yaml.push_str(&format!("description: |\n  {}\n\n", self.description));

        if !self.inputs.is_empty() {
            yaml.push_str("inputs:\n");
            for input in &self.inputs {
                yaml.push_str(&format!("  {}:\n", input.name));
                yaml.push_str(&format!("    type: {:?}\n", input.input_type));
                yaml.push_str(&format!("    required: {}\n", input.required));
                if let Some(ref default) = input.default {
                    yaml.push_str(&format!("    default: {}\n", default));
                }
            }
        }

        yaml.push_str("\nsteps:\n");
        for step in self.steps.iter() {
            yaml.push_str(&format!("  - name: {}\n", step.name));
            yaml.push_str(&format!("    description: {}\n", step.description));
            yaml.push_str(&format!("    action: {:?}\n", step.action));
        }

        yaml
    }
}

/// Template marketplace
#[derive(Debug, Clone)]
pub struct TemplateMarketplace {
    /// Available templates
    pub templates: HashMap<String, WorkflowTemplate>,
    /// Categories
    pub categories: Vec<String>,
}

impl TemplateMarketplace {
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
            categories: vec![
                "general".to_string(),
                "refactoring".to_string(),
                "testing".to_string(),
                "deployment".to_string(),
                "documentation".to_string(),
            ],
        }
    }

    pub fn publish(&mut self, template: WorkflowTemplate) {
        self.templates
            .insert(template.template_id.clone(), template);
    }

    pub fn get_template(&self, id: &str) -> Option<&WorkflowTemplate> {
        self.templates.get(id)
    }

    pub fn search(&self, query: &str) -> Vec<&WorkflowTemplate> {
        let query_lower = query.to_lowercase();
        self.templates
            .values()
            .filter(|t| {
                t.name.to_lowercase().contains(&query_lower)
                    || t.description.to_lowercase().contains(&query_lower)
                    || t.tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&query_lower))
            })
            .collect()
    }

    pub fn by_category(&self, category: &str) -> Vec<&WorkflowTemplate> {
        self.templates
            .values()
            .filter(|t| t.category == category)
            .collect()
    }

    pub fn popular(&self, limit: usize) -> Vec<&WorkflowTemplate> {
        let mut sorted: Vec<_> = self.templates.values().collect();
        sorted.sort_by(|a, b| b.downloads.cmp(&a.downloads));
        sorted.truncate(limit);
        sorted
    }

    pub fn top_rated(&self, limit: usize) -> Vec<&WorkflowTemplate> {
        let mut sorted: Vec<_> = self.templates.values().collect();
        sorted.sort_by(|a, b| {
            b.rating
                .partial_cmp(&a.rating)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted.truncate(limit);
        sorted
    }
}

impl Default for TemplateMarketplace {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Code Provenance
// ============================================================================

/// Code origin type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CodeOrigin {
    /// Written by human
    HumanWritten,
    /// Generated by AI
    AiGenerated,
    /// Copied from external source
    ExternalCopy,
    /// Generated from template
    TemplateGenerated,
    /// Modified by AI (originally human)
    AiModified,
    /// Migrated from legacy code
    Migrated,
    /// Unknown origin
    Unknown,
}

impl CodeOrigin {
    pub fn requires_review(&self) -> bool {
        matches!(
            self,
            CodeOrigin::AiGenerated | CodeOrigin::ExternalCopy | CodeOrigin::AiModified
        )
    }
}

/// Code provenance record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceRecord {
    /// File path
    pub file_path: PathBuf,
    /// Line start
    pub line_start: u32,
    /// Line end
    pub line_end: u32,
    /// Origin type
    pub origin: CodeOrigin,
    /// Source (URL, template name, etc.)
    pub source: Option<String>,
    /// Author (human or AI model)
    pub author: String,
    /// Timestamp
    pub timestamp: u64,
    /// Session ID
    pub session_id: Option<String>,
    /// License (for external copies)
    pub license: Option<String>,
    /// Verified by human
    pub verified: bool,
    /// Review notes
    pub notes: String,
}

impl ProvenanceRecord {
    pub fn new(
        file_path: impl Into<PathBuf>,
        line_start: u32,
        line_end: u32,
        origin: CodeOrigin,
    ) -> Self {
        Self {
            file_path: file_path.into(),
            line_start,
            line_end,
            origin,
            source: None,
            author: String::new(),
            timestamp: current_timestamp(),
            session_id: None,
            license: None,
            verified: false,
            notes: String::new(),
        }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = author.into();
        self
    }

    pub fn with_license(mut self, license: impl Into<String>) -> Self {
        self.license = Some(license.into());
        self
    }

    pub fn verify(&mut self) {
        self.verified = true;
    }

    pub fn line_count(&self) -> u32 {
        self.line_end - self.line_start + 1
    }

    pub fn overlaps(&self, start: u32, end: u32) -> bool {
        !(end < self.line_start || start > self.line_end)
    }
}

/// Provenance tracker
#[derive(Debug, Clone)]
pub struct ProvenanceTracker {
    /// Provenance records by file
    pub records: HashMap<PathBuf, Vec<ProvenanceRecord>>,
}

impl ProvenanceTracker {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
        }
    }

    pub fn track(&mut self, record: ProvenanceRecord) {
        self.records
            .entry(record.file_path.clone())
            .or_default()
            .push(record);
    }

    pub fn get_provenance(&self, file_path: &PathBuf, line: u32) -> Option<&ProvenanceRecord> {
        self.records.get(file_path).and_then(|records| {
            records
                .iter()
                .find(|r| line >= r.line_start && line <= r.line_end)
        })
    }

    pub fn file_provenance(&self, file_path: &PathBuf) -> Vec<&ProvenanceRecord> {
        self.records
            .get(file_path)
            .map(|r| r.iter().collect())
            .unwrap_or_default()
    }

    pub fn ai_generated_lines(&self, file_path: &PathBuf) -> u32 {
        self.records
            .get(file_path)
            .map(|records| {
                records
                    .iter()
                    .filter(|r| r.origin == CodeOrigin::AiGenerated)
                    .map(|r| r.line_count())
                    .sum()
            })
            .unwrap_or(0)
    }

    pub fn human_written_lines(&self, file_path: &PathBuf) -> u32 {
        self.records
            .get(file_path)
            .map(|records| {
                records
                    .iter()
                    .filter(|r| r.origin == CodeOrigin::HumanWritten)
                    .map(|r| r.line_count())
                    .sum()
            })
            .unwrap_or(0)
    }

    pub fn unverified_ai_code(&self) -> Vec<&ProvenanceRecord> {
        self.records
            .values()
            .flatten()
            .filter(|r| r.origin == CodeOrigin::AiGenerated && !r.verified)
            .collect()
    }

    pub fn external_copies(&self) -> Vec<&ProvenanceRecord> {
        self.records
            .values()
            .flatten()
            .filter(|r| r.origin == CodeOrigin::ExternalCopy)
            .collect()
    }

    pub fn statistics(&self) -> ProvenanceStats {
        let mut stats = ProvenanceStats::default();

        for records in self.records.values() {
            for record in records {
                let lines = record.line_count();
                match record.origin {
                    CodeOrigin::HumanWritten => stats.human_lines += lines,
                    CodeOrigin::AiGenerated => stats.ai_generated_lines += lines,
                    CodeOrigin::ExternalCopy => stats.external_copy_lines += lines,
                    CodeOrigin::TemplateGenerated => stats.template_lines += lines,
                    CodeOrigin::AiModified => stats.ai_modified_lines += lines,
                    CodeOrigin::Migrated => stats.migrated_lines += lines,
                    CodeOrigin::Unknown => stats.unknown_lines += lines,
                }

                if !record.verified && record.origin.requires_review() {
                    stats.pending_review += lines;
                }
            }
        }

        stats
    }
}

impl Default for ProvenanceTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Provenance statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProvenanceStats {
    pub human_lines: u32,
    pub ai_generated_lines: u32,
    pub external_copy_lines: u32,
    pub template_lines: u32,
    pub ai_modified_lines: u32,
    pub migrated_lines: u32,
    pub unknown_lines: u32,
    pub pending_review: u32,
}

impl ProvenanceStats {
    pub fn total_lines(&self) -> u32 {
        self.human_lines
            + self.ai_generated_lines
            + self.external_copy_lines
            + self.template_lines
            + self.ai_modified_lines
            + self.migrated_lines
            + self.unknown_lines
    }

    pub fn ai_percentage(&self) -> f64 {
        let total = self.total_lines();
        if total == 0 {
            0.0
        } else {
            (self.ai_generated_lines + self.ai_modified_lines) as f64 / total as f64 * 100.0
        }
    }

    pub fn human_percentage(&self) -> f64 {
        let total = self.total_lines();
        if total == 0 {
            0.0
        } else {
            self.human_lines as f64 / total as f64 * 100.0
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Session recording tests
    #[test]
    fn test_session_event() {
        let event = SessionEvent::new(EventType::UserInput, "test command")
            .with_metadata("source", "terminal");

        assert!(event.event_id.starts_with("evt_"));
        assert_eq!(event.event_type, EventType::UserInput);
        assert!(event.metadata.contains_key("source"));
    }

    #[test]
    fn test_recording_session() {
        let mut session = RecordingSession::new("Test Session");
        session.record(SessionEvent::new(EventType::UserInput, "cmd1"));
        session.record(SessionEvent::new(EventType::AgentResponse, "response1"));

        assert_eq!(session.event_count(), 2);
        assert_eq!(session.events_by_type(EventType::UserInput).len(), 1);
    }

    #[test]
    fn test_session_recorder() {
        let mut recorder = SessionRecorder::new();
        recorder.start_session("Session 1");
        recorder.record(SessionEvent::new(EventType::ToolExecution, "test"));
        recorder.end_session();

        assert_eq!(recorder.sessions.len(), 1);
        assert!(recorder.current.is_none());
    }

    #[test]
    fn test_session_player() {
        let mut session = RecordingSession::new("Test");
        session.record(SessionEvent::new(EventType::UserInput, "1"));
        session.record(SessionEvent::new(EventType::AgentResponse, "2"));
        session.record(SessionEvent::new(EventType::ToolExecution, "3"));

        let mut player = SessionPlayer::new(session);

        assert_eq!(player.next_event().unwrap().data, "1");
        assert_eq!(player.next_event().unwrap().data, "2");
        assert!(!player.is_complete());
        assert!(player.progress() > 0.0);

        player.reset();
        assert_eq!(player.progress(), 0.0);
    }

    // Undo history tests
    #[test]
    fn test_change_record_create() {
        let change =
            ChangeRecord::create("test.txt", "content").with_description("Created test file");

        assert_eq!(change.change_type, ChangeType::Create);
        assert!(change.before.is_none());
        assert!(change.after.is_some());
    }

    #[test]
    fn test_change_record_invert() {
        let create = ChangeRecord::create("test.txt", "content");
        let inverted = create.invert();

        assert_eq!(inverted.change_type, ChangeType::Delete);
        assert!(inverted.after.is_none());
        assert!(inverted.before.is_some());
    }

    #[test]
    fn test_undo_history() {
        let mut history = UndoHistory::new();

        history.record(ChangeRecord::create("a.txt", "a content"));
        history.record(ChangeRecord::create("b.txt", "b content"));

        assert!(history.can_undo());
        assert!(!history.can_redo());

        let undone = history.undo().unwrap();
        assert_eq!(undone.change_type, ChangeType::Delete);
        assert!(history.can_redo());

        let redone = history.redo().unwrap();
        assert_eq!(redone.change_type, ChangeType::Create);
    }

    #[test]
    fn test_undo_history_retention() {
        let history = UndoHistory::new().with_retention(7);
        assert_eq!(history.max_age_days, 7);
    }

    // Workflow template tests
    #[test]
    fn test_workflow_template() {
        let template = WorkflowTemplate::new("Test Workflow", "test_author")
            .with_description("A test workflow")
            .with_category("testing");

        assert!(template.template_id.starts_with("tmpl_"));
        assert_eq!(template.category, "testing");
    }

    #[test]
    fn test_workflow_step() {
        let step = WorkflowStep {
            name: "Run tests".to_string(),
            description: "Execute test suite".to_string(),
            action: WorkflowAction::RunTests,
            parameters: HashMap::new(),
            condition: None,
        };

        assert_eq!(step.action, WorkflowAction::RunTests);
    }

    #[test]
    fn test_template_marketplace() {
        let mut marketplace = TemplateMarketplace::new();

        let mut template = WorkflowTemplate::new("Test", "author");
        template.downloads = 100;
        template.rating = 4.5;
        marketplace.publish(template);

        assert_eq!(marketplace.templates.len(), 1);
        assert_eq!(marketplace.popular(10).len(), 1);
        assert_eq!(marketplace.top_rated(10).len(), 1);
    }

    #[test]
    fn test_marketplace_search() {
        let mut marketplace = TemplateMarketplace::new();

        let mut template = WorkflowTemplate::new("Refactor Component", "author");
        template.tags.push("react".to_string());
        marketplace.publish(template);

        assert_eq!(marketplace.search("refactor").len(), 1);
        assert_eq!(marketplace.search("react").len(), 1);
        assert_eq!(marketplace.search("nonexistent").len(), 0);
    }

    // Provenance tests
    #[test]
    fn test_code_origin() {
        assert!(CodeOrigin::AiGenerated.requires_review());
        assert!(CodeOrigin::ExternalCopy.requires_review());
        assert!(!CodeOrigin::HumanWritten.requires_review());
    }

    #[test]
    fn test_provenance_record() {
        let record = ProvenanceRecord::new("src/lib.rs", 1, 50, CodeOrigin::AiGenerated)
            .with_author("claude")
            .with_source("session_123");

        assert_eq!(record.line_count(), 50);
        assert!(!record.verified);
        assert!(record.overlaps(25, 75));
        assert!(!record.overlaps(51, 100));
    }

    #[test]
    fn test_provenance_tracker() {
        let mut tracker = ProvenanceTracker::new();

        tracker.track(ProvenanceRecord::new(
            "src/main.rs",
            1,
            100,
            CodeOrigin::HumanWritten,
        ));

        tracker.track(ProvenanceRecord::new(
            "src/main.rs",
            101,
            200,
            CodeOrigin::AiGenerated,
        ));

        let path = PathBuf::from("src/main.rs");
        assert_eq!(tracker.human_written_lines(&path), 100);
        assert_eq!(tracker.ai_generated_lines(&path), 100);

        let provenance = tracker.get_provenance(&path, 50).unwrap();
        assert_eq!(provenance.origin, CodeOrigin::HumanWritten);
    }

    #[test]
    fn test_provenance_statistics() {
        let mut tracker = ProvenanceTracker::new();

        tracker.track(ProvenanceRecord::new(
            "a.rs",
            1,
            100,
            CodeOrigin::HumanWritten,
        ));
        tracker.track(ProvenanceRecord::new(
            "b.rs",
            1,
            50,
            CodeOrigin::AiGenerated,
        ));

        let stats = tracker.statistics();

        assert_eq!(stats.human_lines, 100);
        assert_eq!(stats.ai_generated_lines, 50);
        assert_eq!(stats.total_lines(), 150);
        assert!(stats.ai_percentage() > 30.0);
        assert!(stats.human_percentage() > 60.0);
    }

    #[test]
    fn test_unverified_ai_code() {
        let mut tracker = ProvenanceTracker::new();

        let mut verified = ProvenanceRecord::new("a.rs", 1, 50, CodeOrigin::AiGenerated);
        verified.verify();
        tracker.track(verified);

        tracker.track(ProvenanceRecord::new(
            "b.rs",
            1,
            50,
            CodeOrigin::AiGenerated,
        ));

        assert_eq!(tracker.unverified_ai_code().len(), 1);
    }

    #[test]
    fn test_external_copies_tracking() {
        let mut tracker = ProvenanceTracker::new();

        tracker.track(
            ProvenanceRecord::new("vendor.rs", 1, 200, CodeOrigin::ExternalCopy)
                .with_source("https://example.com/code")
                .with_license("MIT"),
        );

        let copies = tracker.external_copies();
        assert_eq!(copies.len(), 1);
        assert!(copies[0].license.is_some());
    }

    #[test]
    fn test_change_type_variants() {
        let types = [
            ChangeType::Create,
            ChangeType::Modify,
            ChangeType::Delete,
            ChangeType::Move,
        ];
        for t in types {
            let _ = format!("{:?}", t);
        }
    }

    #[test]
    fn test_change_type_eq() {
        assert_eq!(ChangeType::Create, ChangeType::Create);
        assert_ne!(ChangeType::Create, ChangeType::Delete);
    }

    #[test]
    fn test_change_record_modify() {
        let change = ChangeRecord::modify("test.txt", "before content", "after content");
        assert_eq!(change.change_type, ChangeType::Modify);
        assert!(change.before.is_some());
        assert!(change.after.is_some());
    }

    #[test]
    fn test_change_record_delete() {
        let change = ChangeRecord::delete("test.txt", "content to delete");
        assert_eq!(change.change_type, ChangeType::Delete);
        assert!(change.before.is_some());
        assert!(change.after.is_none());
    }

    #[test]
    fn test_change_record_invert_modify() {
        let modify = ChangeRecord::modify("test.txt", "before", "after");
        let inverted = modify.invert();

        assert_eq!(inverted.change_type, ChangeType::Modify);
        assert_eq!(inverted.before, Some("after".to_string()));
        assert_eq!(inverted.after, Some("before".to_string()));
    }

    #[test]
    fn test_change_record_invert_delete() {
        let delete = ChangeRecord::delete("test.txt", "content");
        let inverted = delete.invert();

        assert_eq!(inverted.change_type, ChangeType::Create);
    }

    #[test]
    fn test_undo_history_empty() {
        let history = UndoHistory::new();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
    }

    #[test]
    fn test_undo_history_undo_returns_none_when_empty() {
        let mut history = UndoHistory::new();
        assert!(history.undo().is_none());
    }

    #[test]
    fn test_undo_history_redo_returns_none_when_empty() {
        let mut history = UndoHistory::new();
        assert!(history.redo().is_none());
    }

    #[test]
    fn test_workflow_action_variants() {
        let actions = [
            WorkflowAction::RunTests,
            WorkflowAction::WaitForApproval,
            WorkflowAction::Custom("test".to_string()),
        ];
        for a in actions {
            let _ = format!("{:?}", a);
        }
    }

    #[test]
    fn test_code_origin_variants() {
        let origins = [
            CodeOrigin::HumanWritten,
            CodeOrigin::AiGenerated,
            CodeOrigin::ExternalCopy,
            CodeOrigin::TemplateGenerated,
            CodeOrigin::AiModified,
        ];
        for o in origins {
            let _ = format!("{:?}", o);
        }
    }

    #[test]
    fn test_code_origin_requires_review() {
        assert!(!CodeOrigin::HumanWritten.requires_review());
        assert!(CodeOrigin::AiGenerated.requires_review());
        assert!(CodeOrigin::ExternalCopy.requires_review());
        assert!(CodeOrigin::AiModified.requires_review());
    }

    #[test]
    fn test_provenance_record_verify() {
        let mut record = ProvenanceRecord::new("test.rs", 1, 50, CodeOrigin::AiGenerated);
        assert!(!record.verified);

        record.verify();
        assert!(record.verified);
    }

    #[test]
    fn test_provenance_record_overlaps() {
        let record = ProvenanceRecord::new("test.rs", 10, 50, CodeOrigin::AiGenerated);

        assert!(record.overlaps(10, 50)); // Exact match
        assert!(record.overlaps(1, 20)); // Starts before, ends inside
        assert!(record.overlaps(40, 60)); // Starts inside, ends after
        assert!(record.overlaps(1, 100)); // Completely contains
        assert!(record.overlaps(20, 40)); // Completely contained
        assert!(!record.overlaps(1, 9)); // Ends before
        assert!(!record.overlaps(51, 60)); // Starts after
    }

    #[test]
    fn test_provenance_stats_percentages() {
        let stats = ProvenanceStats {
            human_lines: 1000,
            ai_generated_lines: 500,
            external_copy_lines: 100,
            template_lines: 0,
            ai_modified_lines: 0,
            migrated_lines: 0,
            unknown_lines: 0,
            pending_review: 0,
        };

        assert_eq!(stats.total_lines(), 1600);
        assert!((stats.human_percentage() - 62.5).abs() < 0.1);
        assert!((stats.ai_percentage() - 31.25).abs() < 0.1);
    }

    #[test]
    fn test_provenance_stats_zero_lines() {
        let stats = ProvenanceStats {
            human_lines: 0,
            ai_generated_lines: 0,
            external_copy_lines: 0,
            template_lines: 0,
            ai_modified_lines: 0,
            migrated_lines: 0,
            unknown_lines: 0,
            pending_review: 0,
        };

        assert_eq!(stats.total_lines(), 0);
        assert_eq!(stats.human_percentage(), 0.0);
    }

    #[test]
    fn test_workflow_template_clone() {
        let template = WorkflowTemplate::new("Test", "author");
        let cloned = template.clone();
        assert_eq!(template.template_id, cloned.template_id);
        assert_eq!(template.name, cloned.name);
    }

    #[test]
    fn test_change_record_clone() {
        let change = ChangeRecord::create("test.txt", "content");
        let cloned = change.clone();
        assert_eq!(change.file_path, cloned.file_path);
        assert_eq!(change.change_type, cloned.change_type);
    }
}
