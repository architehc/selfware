//! Real-time Collaboration
//!
//! Multi-user collaboration features:
//! - Shared context between users
//! - Conflict resolution
//! - Presence awareness
//! - Collaborative editing

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Global counters for unique IDs
static SESSION_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
static USER_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
static OPERATION_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
static CURSOR_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn generate_session_id() -> String {
    format!(
        "sess_{}_{:x}",
        SESSION_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
        current_timestamp()
    )
}

fn generate_user_id() -> String {
    format!(
        "user_{}_{:x}",
        USER_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
        current_timestamp()
    )
}

fn generate_operation_id() -> String {
    format!(
        "op_{}_{:x}",
        OPERATION_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
        current_timestamp()
    )
}

fn generate_cursor_id() -> String {
    format!(
        "cur_{}_{:x}",
        CURSOR_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
        current_timestamp()
    )
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ============================================================================
// User & Presence
// ============================================================================

/// User presence status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PresenceStatus {
    /// User is actively working
    Active,
    /// User is idle (no recent activity)
    Idle,
    /// User is away
    Away,
    /// User is offline
    #[default]
    Offline,
    /// User is busy (do not disturb)
    Busy,
}

impl PresenceStatus {
    pub fn as_str(&self) -> &str {
        match self {
            PresenceStatus::Active => "active",
            PresenceStatus::Idle => "idle",
            PresenceStatus::Away => "away",
            PresenceStatus::Offline => "offline",
            PresenceStatus::Busy => "busy",
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, PresenceStatus::Active | PresenceStatus::Idle)
    }
}

/// User activity type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityType {
    /// Typing/editing
    Typing,
    /// Reading/reviewing
    Reading,
    /// Selecting text
    Selecting,
    /// Running code
    RunningCode,
    /// Debugging
    Debugging,
    /// In chat
    Chatting,
    /// Custom activity
    Custom(String),
}

impl ActivityType {
    pub fn as_str(&self) -> &str {
        match self {
            ActivityType::Typing => "typing",
            ActivityType::Reading => "reading",
            ActivityType::Selecting => "selecting",
            ActivityType::RunningCode => "running_code",
            ActivityType::Debugging => "debugging",
            ActivityType::Chatting => "chatting",
            ActivityType::Custom(s) => s.as_str(),
        }
    }
}

/// User cursor position
#[derive(Debug, Clone)]
pub struct CursorPosition {
    /// Cursor ID
    pub id: String,
    /// File path
    pub file: String,
    /// Line number (1-indexed)
    pub line: u32,
    /// Column number (1-indexed)
    pub column: u32,
    /// Selection end (if any)
    pub selection_end: Option<(u32, u32)>,
    /// Last update timestamp
    pub updated_at: u64,
}

impl CursorPosition {
    pub fn new(file: impl Into<String>, line: u32, column: u32) -> Self {
        Self {
            id: generate_cursor_id(),
            file: file.into(),
            line,
            column,
            selection_end: None,
            updated_at: current_timestamp(),
        }
    }

    /// Set selection
    pub fn with_selection(mut self, end_line: u32, end_column: u32) -> Self {
        self.selection_end = Some((end_line, end_column));
        self
    }

    /// Check if cursor has selection
    pub fn has_selection(&self) -> bool {
        self.selection_end.is_some()
    }

    /// Update position
    pub fn update(&mut self, line: u32, column: u32) {
        self.line = line;
        self.column = column;
        self.selection_end = None;
        self.updated_at = current_timestamp();
    }
}

/// Collaborating user
#[derive(Debug, Clone)]
pub struct CollaboratorUser {
    /// User ID
    pub id: String,
    /// Display name
    pub name: String,
    /// Email (optional)
    pub email: Option<String>,
    /// Avatar URL (optional)
    pub avatar: Option<String>,
    /// User color (for cursor/highlighting)
    pub color: String,
    /// Presence status
    pub status: PresenceStatus,
    /// Current activity
    pub activity: Option<ActivityType>,
    /// Current cursor position
    pub cursor: Option<CursorPosition>,
    /// Last activity timestamp
    pub last_activity: u64,
    /// Join timestamp
    pub joined_at: u64,
}

impl CollaboratorUser {
    pub fn new(name: impl Into<String>) -> Self {
        let now = current_timestamp();
        Self {
            id: generate_user_id(),
            name: name.into(),
            email: None,
            avatar: None,
            color: Self::generate_color(),
            status: PresenceStatus::Active,
            activity: None,
            cursor: None,
            last_activity: now,
            joined_at: now,
        }
    }

    /// Generate a random user color
    fn generate_color() -> String {
        let colors = [
            "#FF6B6B", "#4ECDC4", "#45B7D1", "#96CEB4", "#FFEAA7", "#DDA0DD", "#98D8C8", "#F7DC6F",
            "#BB8FCE", "#85C1E9", "#F8B500", "#00CED1",
        ];
        let idx = (current_timestamp() % colors.len() as u64) as usize;
        colors[idx].to_string()
    }

    /// Builder: set email
    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self
    }

    /// Builder: set avatar
    pub fn with_avatar(mut self, avatar: impl Into<String>) -> Self {
        self.avatar = Some(avatar.into());
        self
    }

    /// Builder: set color
    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.color = color.into();
        self
    }

    /// Update status
    pub fn set_status(&mut self, status: PresenceStatus) {
        self.status = status;
        self.last_activity = current_timestamp();
    }

    /// Update activity
    pub fn set_activity(&mut self, activity: ActivityType) {
        self.activity = Some(activity);
        self.last_activity = current_timestamp();
    }

    /// Clear activity
    pub fn clear_activity(&mut self) {
        self.activity = None;
    }

    /// Update cursor
    pub fn set_cursor(&mut self, cursor: CursorPosition) {
        self.cursor = Some(cursor);
        self.last_activity = current_timestamp();
    }

    /// Check if user is online
    pub fn is_online(&self) -> bool {
        self.status != PresenceStatus::Offline
    }

    /// Get time since last activity in seconds
    pub fn idle_seconds(&self) -> u64 {
        (current_timestamp() - self.last_activity) / 1000
    }
}

/// Presence manager for tracking online users
#[derive(Debug, Default)]
pub struct PresenceManager {
    /// Online users by ID
    users: HashMap<String, CollaboratorUser>,
    /// Idle timeout in seconds
    idle_timeout_secs: u64,
    /// Away timeout in seconds
    away_timeout_secs: u64,
}

impl PresenceManager {
    pub fn new() -> Self {
        Self {
            users: HashMap::new(),
            idle_timeout_secs: 60,
            away_timeout_secs: 300,
        }
    }

    /// Builder: set idle timeout
    pub fn with_idle_timeout(mut self, secs: u64) -> Self {
        self.idle_timeout_secs = secs;
        self
    }

    /// Builder: set away timeout
    pub fn with_away_timeout(mut self, secs: u64) -> Self {
        self.away_timeout_secs = secs;
        self
    }

    /// Add a user
    pub fn add_user(&mut self, user: CollaboratorUser) -> String {
        let id = user.id.clone();
        self.users.insert(id.clone(), user);
        id
    }

    /// Remove a user
    pub fn remove_user(&mut self, user_id: &str) -> Option<CollaboratorUser> {
        self.users.remove(user_id)
    }

    /// Get a user
    pub fn get_user(&self, user_id: &str) -> Option<&CollaboratorUser> {
        self.users.get(user_id)
    }

    /// Get a user mutably
    pub fn get_user_mut(&mut self, user_id: &str) -> Option<&mut CollaboratorUser> {
        self.users.get_mut(user_id)
    }

    /// Get all online users
    pub fn online_users(&self) -> Vec<&CollaboratorUser> {
        self.users.values().filter(|u| u.is_online()).collect()
    }

    /// Get users in a file
    pub fn users_in_file(&self, file: &str) -> Vec<&CollaboratorUser> {
        self.users
            .values()
            .filter(|u| u.cursor.as_ref().map(|c| c.file == file).unwrap_or(false))
            .collect()
    }

    /// Update user statuses based on idle time
    pub fn update_statuses(&mut self) {
        let now = current_timestamp();
        for user in self.users.values_mut() {
            if user.status == PresenceStatus::Offline {
                continue;
            }

            let idle_ms = now - user.last_activity;
            let idle_secs = idle_ms / 1000;

            if idle_secs > self.away_timeout_secs {
                user.status = PresenceStatus::Away;
            } else if idle_secs > self.idle_timeout_secs {
                user.status = PresenceStatus::Idle;
            }
        }
    }

    /// Get user count
    pub fn user_count(&self) -> usize {
        self.users.len()
    }

    /// Get online user count
    pub fn online_count(&self) -> usize {
        self.users.values().filter(|u| u.is_online()).count()
    }
}

// ============================================================================
// Shared Context
// ============================================================================

/// Context scope
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextScope {
    /// Visible to all in session
    Session,
    /// Visible only to specific users
    Private(HashSet<String>),
    /// Visible to a team/group
    Team(String),
}

/// Shared context item
#[derive(Debug, Clone)]
pub struct SharedContextItem {
    /// Item ID
    pub id: String,
    /// Item key/name
    pub key: String,
    /// Item value
    pub value: String,
    /// Value type
    pub value_type: String,
    /// Scope
    pub scope: ContextScope,
    /// Created by user ID
    pub created_by: String,
    /// Last modified by user ID
    pub modified_by: String,
    /// Created timestamp
    pub created_at: u64,
    /// Modified timestamp
    pub modified_at: u64,
    /// Version number
    pub version: u64,
}

impl SharedContextItem {
    pub fn new(
        key: impl Into<String>,
        value: impl Into<String>,
        created_by: impl Into<String>,
    ) -> Self {
        let now = current_timestamp();
        let created_by_str = created_by.into();
        Self {
            id: format!("ctx_{}_{:x}", current_timestamp(), rand_u32()),
            key: key.into(),
            value: value.into(),
            value_type: "string".to_string(),
            scope: ContextScope::Session,
            created_by: created_by_str.clone(),
            modified_by: created_by_str,
            created_at: now,
            modified_at: now,
            version: 1,
        }
    }

    /// Builder: set value type
    pub fn with_type(mut self, value_type: impl Into<String>) -> Self {
        self.value_type = value_type.into();
        self
    }

    /// Builder: set scope
    pub fn with_scope(mut self, scope: ContextScope) -> Self {
        self.scope = scope;
        self
    }

    /// Update value
    pub fn update(&mut self, value: impl Into<String>, modified_by: impl Into<String>) {
        self.value = value.into();
        self.modified_by = modified_by.into();
        self.modified_at = current_timestamp();
        self.version += 1;
    }
}

/// Simple pseudo-random number for IDs
fn rand_u32() -> u32 {
    (current_timestamp() % u32::MAX as u64) as u32
}

/// Shared context manager
#[derive(Debug, Default)]
pub struct SharedContext {
    /// Context items by ID
    items: HashMap<String, SharedContextItem>,
    /// Index by key
    by_key: HashMap<String, String>,
    /// History of changes
    history: VecDeque<ContextChange>,
    /// Max history size
    max_history: usize,
}

/// Context change record
#[derive(Debug, Clone)]
pub struct ContextChange {
    pub item_id: String,
    pub key: String,
    pub old_value: Option<String>,
    pub new_value: String,
    pub changed_by: String,
    pub timestamp: u64,
}

impl SharedContext {
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
            by_key: HashMap::new(),
            history: VecDeque::new(),
            max_history: 100,
        }
    }

    /// Builder: set max history size
    pub fn with_max_history(mut self, max: usize) -> Self {
        self.max_history = max;
        self
    }

    /// Set a context item
    pub fn set(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
        user_id: impl Into<String>,
    ) -> String {
        let key_str = key.into();
        let value_str = value.into();
        let user_str = user_id.into();

        if let Some(existing_id) = self.by_key.get(&key_str) {
            // Update existing
            let existing_id = existing_id.clone();
            if let Some(item) = self.items.get_mut(&existing_id) {
                let old_value = item.value.clone();
                item.update(value_str.clone(), user_str.clone());

                self.record_change(ContextChange {
                    item_id: existing_id.clone(),
                    key: key_str,
                    old_value: Some(old_value),
                    new_value: value_str,
                    changed_by: user_str,
                    timestamp: current_timestamp(),
                });

                return existing_id;
            }
        }

        // Create new
        let item = SharedContextItem::new(key_str.clone(), value_str.clone(), user_str.clone());
        let id = item.id.clone();

        self.record_change(ContextChange {
            item_id: id.clone(),
            key: key_str.clone(),
            old_value: None,
            new_value: value_str,
            changed_by: user_str,
            timestamp: current_timestamp(),
        });

        self.by_key.insert(key_str, id.clone());
        self.items.insert(id.clone(), item);
        id
    }

    /// Get a context item by key
    pub fn get(&self, key: &str) -> Option<&SharedContextItem> {
        self.by_key.get(key).and_then(|id| self.items.get(id))
    }

    /// Get a context value by key
    pub fn get_value(&self, key: &str) -> Option<&str> {
        self.get(key).map(|item| item.value.as_str())
    }

    /// Remove a context item
    pub fn remove(&mut self, key: &str) -> Option<SharedContextItem> {
        if let Some(id) = self.by_key.remove(key) {
            self.items.remove(&id)
        } else {
            None
        }
    }

    /// Get all items
    pub fn all(&self) -> Vec<&SharedContextItem> {
        self.items.values().collect()
    }

    /// Get items visible to a user
    pub fn visible_to(&self, user_id: &str) -> Vec<&SharedContextItem> {
        self.items
            .values()
            .filter(|item| match &item.scope {
                ContextScope::Session => true,
                ContextScope::Private(users) => users.contains(user_id),
                ContextScope::Team(_) => true, // Simplified: assume user is in team
            })
            .collect()
    }

    /// Record a change in history
    fn record_change(&mut self, change: ContextChange) {
        self.history.push_back(change);
        while self.history.len() > self.max_history {
            self.history.pop_front();
        }
    }

    /// Get recent changes
    pub fn recent_changes(&self, limit: usize) -> Vec<&ContextChange> {
        self.history.iter().rev().take(limit).collect()
    }

    /// Get item count
    pub fn count(&self) -> usize {
        self.items.len()
    }
}

// ============================================================================
// Conflict Resolution
// ============================================================================

/// Conflict type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictType {
    /// Same line edited by multiple users
    EditConflict,
    /// File deleted while being edited
    DeleteConflict,
    /// Concurrent renames
    RenameConflict,
    /// Version mismatch
    VersionConflict,
}

impl ConflictType {
    pub fn as_str(&self) -> &str {
        match self {
            ConflictType::EditConflict => "edit_conflict",
            ConflictType::DeleteConflict => "delete_conflict",
            ConflictType::RenameConflict => "rename_conflict",
            ConflictType::VersionConflict => "version_conflict",
        }
    }
}

/// Resolution strategy
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionStrategy {
    /// Last write wins
    LastWriteWins,
    /// First write wins
    FirstWriteWins,
    /// Merge changes
    Merge,
    /// Manual resolution required
    Manual,
    /// Keep both versions
    KeepBoth,
}

impl ResolutionStrategy {
    pub fn as_str(&self) -> &str {
        match self {
            ResolutionStrategy::LastWriteWins => "last_write_wins",
            ResolutionStrategy::FirstWriteWins => "first_write_wins",
            ResolutionStrategy::Merge => "merge",
            ResolutionStrategy::Manual => "manual",
            ResolutionStrategy::KeepBoth => "keep_both",
        }
    }
}

/// A conflict between changes
#[derive(Debug, Clone)]
pub struct Conflict {
    /// Conflict ID
    pub id: String,
    /// Conflict type
    pub conflict_type: ConflictType,
    /// File path
    pub file: String,
    /// First user's change
    pub change_a: Change,
    /// Second user's change
    pub change_b: Change,
    /// Suggested resolution
    pub suggested_resolution: ResolutionStrategy,
    /// Is resolved
    pub resolved: bool,
    /// Resolution (if resolved)
    pub resolution: Option<String>,
    /// Detected timestamp
    pub detected_at: u64,
    /// Resolved timestamp
    pub resolved_at: Option<u64>,
}

/// A change made by a user
#[derive(Debug, Clone)]
pub struct Change {
    pub user_id: String,
    pub content: String,
    pub timestamp: u64,
    pub version: u64,
}

impl Change {
    pub fn new(user_id: impl Into<String>, content: impl Into<String>, version: u64) -> Self {
        Self {
            user_id: user_id.into(),
            content: content.into(),
            timestamp: current_timestamp(),
            version,
        }
    }
}

impl Conflict {
    pub fn new(
        conflict_type: ConflictType,
        file: impl Into<String>,
        change_a: Change,
        change_b: Change,
    ) -> Self {
        Self {
            id: format!("conflict_{}_{:x}", current_timestamp(), rand_u32()),
            conflict_type,
            file: file.into(),
            change_a,
            change_b,
            suggested_resolution: ResolutionStrategy::Manual,
            resolved: false,
            resolution: None,
            detected_at: current_timestamp(),
            resolved_at: None,
        }
    }

    /// Builder: set suggested resolution
    pub fn suggest(mut self, strategy: ResolutionStrategy) -> Self {
        self.suggested_resolution = strategy;
        self
    }

    /// Resolve the conflict
    pub fn resolve(&mut self, resolution: impl Into<String>) {
        self.resolved = true;
        self.resolution = Some(resolution.into());
        self.resolved_at = Some(current_timestamp());
    }
}

/// Conflict resolver
#[derive(Debug)]
pub struct ConflictResolver {
    /// Default resolution strategy
    default_strategy: ResolutionStrategy,
    /// Pending conflicts
    conflicts: HashMap<String, Conflict>,
    /// Resolved conflicts (for history)
    resolved: VecDeque<Conflict>,
    /// Max resolved history
    max_resolved_history: usize,
}

impl Default for ConflictResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ConflictResolver {
    pub fn new() -> Self {
        Self {
            default_strategy: ResolutionStrategy::LastWriteWins,
            conflicts: HashMap::new(),
            resolved: VecDeque::new(),
            max_resolved_history: 100,
        }
    }

    /// Builder: set default strategy
    pub fn with_default_strategy(mut self, strategy: ResolutionStrategy) -> Self {
        self.default_strategy = strategy;
        self
    }

    /// Detect conflict between two changes
    pub fn detect_conflict(
        &mut self,
        file: impl Into<String>,
        change_a: Change,
        change_b: Change,
    ) -> String {
        let conflict = Conflict::new(ConflictType::EditConflict, file, change_a, change_b)
            .suggest(self.default_strategy.clone());
        let id = conflict.id.clone();
        self.conflicts.insert(id.clone(), conflict);
        id
    }

    /// Get a conflict
    pub fn get_conflict(&self, id: &str) -> Option<&Conflict> {
        self.conflicts.get(id)
    }

    /// Get all pending conflicts
    pub fn pending_conflicts(&self) -> Vec<&Conflict> {
        self.conflicts.values().filter(|c| !c.resolved).collect()
    }

    /// Resolve a conflict
    pub fn resolve(&mut self, id: &str, resolution: impl Into<String>) -> bool {
        if let Some(conflict) = self.conflicts.remove(id) {
            let mut resolved = conflict;
            resolved.resolve(resolution);
            self.resolved.push_back(resolved);
            while self.resolved.len() > self.max_resolved_history {
                self.resolved.pop_front();
            }
            true
        } else {
            false
        }
    }

    /// Auto-resolve using default strategy
    pub fn auto_resolve(&mut self, id: &str) -> Option<String> {
        if let Some(conflict) = self.conflicts.get(id) {
            let resolution = match self.default_strategy {
                ResolutionStrategy::LastWriteWins => {
                    if conflict.change_a.timestamp > conflict.change_b.timestamp {
                        conflict.change_a.content.clone()
                    } else {
                        conflict.change_b.content.clone()
                    }
                }
                ResolutionStrategy::FirstWriteWins => {
                    if conflict.change_a.timestamp < conflict.change_b.timestamp {
                        conflict.change_a.content.clone()
                    } else {
                        conflict.change_b.content.clone()
                    }
                }
                ResolutionStrategy::KeepBoth => {
                    format!(
                        "<<<<<<< USER A\n{}\n=======\n{}\n>>>>>>> USER B",
                        conflict.change_a.content, conflict.change_b.content
                    )
                }
                _ => return None,
            };

            self.resolve(id, resolution.clone());
            Some(resolution)
        } else {
            None
        }
    }

    /// Get conflict count
    pub fn conflict_count(&self) -> usize {
        self.conflicts.len()
    }
}

// ============================================================================
// Collaborative Editing
// ============================================================================

/// Operation type for OT (Operational Transformation)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationType {
    Insert,
    Delete,
    Retain,
}

/// A single operation in collaborative editing
#[derive(Debug, Clone)]
pub struct Operation {
    /// Operation ID
    pub id: String,
    /// Operation type
    pub op_type: OperationType,
    /// Position in document
    pub position: usize,
    /// Content (for insert)
    pub content: Option<String>,
    /// Length (for delete/retain)
    pub length: usize,
    /// User who made the operation
    pub user_id: String,
    /// Timestamp
    pub timestamp: u64,
    /// Version vector
    pub version: u64,
}

impl Operation {
    /// Create insert operation
    pub fn insert(position: usize, content: impl Into<String>, user_id: impl Into<String>) -> Self {
        let content_str = content.into();
        let len = content_str.len();
        Self {
            id: generate_operation_id(),
            op_type: OperationType::Insert,
            position,
            content: Some(content_str),
            length: len,
            user_id: user_id.into(),
            timestamp: current_timestamp(),
            version: 0,
        }
    }

    /// Create delete operation
    pub fn delete(position: usize, length: usize, user_id: impl Into<String>) -> Self {
        Self {
            id: generate_operation_id(),
            op_type: OperationType::Delete,
            position,
            content: None,
            length,
            user_id: user_id.into(),
            timestamp: current_timestamp(),
            version: 0,
        }
    }

    /// Create retain operation (skip characters)
    pub fn retain(length: usize, user_id: impl Into<String>) -> Self {
        Self {
            id: generate_operation_id(),
            op_type: OperationType::Retain,
            position: 0,
            content: None,
            length,
            user_id: user_id.into(),
            timestamp: current_timestamp(),
            version: 0,
        }
    }

    /// Set version
    pub fn with_version(mut self, version: u64) -> Self {
        self.version = version;
        self
    }
}

/// Collaborative document
#[derive(Debug, Clone)]
pub struct CollaborativeDocument {
    /// Document ID
    pub id: String,
    /// File path
    pub path: String,
    /// Current content
    pub content: String,
    /// Current version
    pub version: u64,
    /// Operation history
    pub history: VecDeque<Operation>,
    /// Max history size
    max_history: usize,
    /// Active editors
    pub editors: HashSet<String>,
}

impl CollaborativeDocument {
    pub fn new(path: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: format!("doc_{}_{:x}", current_timestamp(), rand_u32()),
            path: path.into(),
            content: content.into(),
            version: 0,
            history: VecDeque::new(),
            max_history: 1000,
            editors: HashSet::new(),
        }
    }

    /// Apply an operation to the document
    pub fn apply(&mut self, mut op: Operation) -> Result<(), String> {
        op.version = self.version + 1;

        match op.op_type {
            OperationType::Insert => {
                if op.position > self.content.len() {
                    return Err("Insert position out of bounds".to_string());
                }
                if let Some(ref text) = op.content {
                    self.content.insert_str(op.position, text);
                }
            }
            OperationType::Delete => {
                if op.position + op.length > self.content.len() {
                    return Err("Delete range out of bounds".to_string());
                }
                self.content.drain(op.position..op.position + op.length);
            }
            OperationType::Retain => {
                // No content change
            }
        }

        self.version = op.version;
        self.history.push_back(op);

        while self.history.len() > self.max_history {
            self.history.pop_front();
        }

        Ok(())
    }

    /// Transform an operation against another
    pub fn transform(op1: &Operation, op2: &Operation) -> Operation {
        let mut transformed = op1.clone();
        transformed.id = generate_operation_id();

        match (&op1.op_type, &op2.op_type) {
            (OperationType::Insert, OperationType::Insert) => {
                if op2.position <= op1.position {
                    transformed.position += op2.length;
                }
            }
            (OperationType::Insert, OperationType::Delete) => {
                if op2.position < op1.position {
                    let shift = op2.length.min(op1.position - op2.position);
                    transformed.position -= shift;
                }
            }
            (OperationType::Delete, OperationType::Insert) => {
                if op2.position <= op1.position {
                    transformed.position += op2.length;
                }
            }
            (OperationType::Delete, OperationType::Delete) => {
                if op2.position < op1.position {
                    let shift = op2.length.min(op1.position - op2.position);
                    transformed.position -= shift;
                } else if op2.position < op1.position + op1.length {
                    // Overlapping deletes
                    let overlap_start = op2.position.max(op1.position);
                    let overlap_end = (op2.position + op2.length).min(op1.position + op1.length);
                    if overlap_end > overlap_start {
                        transformed.length -= overlap_end - overlap_start;
                    }
                }
            }
            _ => {}
        }

        transformed
    }

    /// Add an editor
    pub fn add_editor(&mut self, user_id: impl Into<String>) {
        self.editors.insert(user_id.into());
    }

    /// Remove an editor
    pub fn remove_editor(&mut self, user_id: &str) {
        self.editors.remove(user_id);
    }

    /// Get editor count
    pub fn editor_count(&self) -> usize {
        self.editors.len()
    }

    /// Get operations since version
    pub fn operations_since(&self, version: u64) -> Vec<&Operation> {
        self.history
            .iter()
            .filter(|op| op.version > version)
            .collect()
    }
}

// ============================================================================
// Collaboration Session
// ============================================================================

/// Session permission
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Read only
    Read,
    /// Read and write
    Write,
    /// Full admin access
    Admin,
}

impl Permission {
    pub fn as_str(&self) -> &str {
        match self {
            Permission::Read => "read",
            Permission::Write => "write",
            Permission::Admin => "admin",
        }
    }

    pub fn can_write(&self) -> bool {
        matches!(self, Permission::Write | Permission::Admin)
    }

    pub fn can_admin(&self) -> bool {
        matches!(self, Permission::Admin)
    }
}

/// Collaboration session
#[derive(Debug)]
pub struct CollaborationSession {
    /// Session ID
    pub id: String,
    /// Session name
    pub name: String,
    /// Presence manager
    pub presence: PresenceManager,
    /// Shared context
    pub context: SharedContext,
    /// Conflict resolver
    pub conflicts: ConflictResolver,
    /// Open documents
    pub documents: HashMap<String, CollaborativeDocument>,
    /// User permissions
    permissions: HashMap<String, Permission>,
    /// Session owner
    pub owner_id: String,
    /// Created timestamp
    pub created_at: u64,
    /// Is session active
    pub active: bool,
}

impl CollaborationSession {
    pub fn new(name: impl Into<String>, owner: CollaboratorUser) -> Self {
        let owner_id = owner.id.clone();
        let mut presence = PresenceManager::new();
        presence.add_user(owner);

        let mut permissions = HashMap::new();
        permissions.insert(owner_id.clone(), Permission::Admin);

        Self {
            id: generate_session_id(),
            name: name.into(),
            presence,
            context: SharedContext::new(),
            conflicts: ConflictResolver::new(),
            documents: HashMap::new(),
            permissions,
            owner_id,
            created_at: current_timestamp(),
            active: true,
        }
    }

    /// Join session
    pub fn join(&mut self, user: CollaboratorUser, permission: Permission) -> String {
        let user_id = user.id.clone();
        self.presence.add_user(user);
        self.permissions.insert(user_id.clone(), permission);
        user_id
    }

    /// Leave session
    pub fn leave(&mut self, user_id: &str) {
        if let Some(user) = self.presence.get_user_mut(user_id) {
            user.set_status(PresenceStatus::Offline);
        }
        self.permissions.remove(user_id);
    }

    /// Get user permission
    pub fn get_permission(&self, user_id: &str) -> Option<Permission> {
        self.permissions.get(user_id).copied()
    }

    /// Check if user can write
    pub fn can_write(&self, user_id: &str) -> bool {
        self.permissions
            .get(user_id)
            .map(|p| p.can_write())
            .unwrap_or(false)
    }

    /// Open a document
    pub fn open_document(
        &mut self,
        path: impl Into<String>,
        content: impl Into<String>,
        user_id: &str,
    ) -> Option<&CollaborativeDocument> {
        let path_str = path.into();
        let doc = CollaborativeDocument::new(path_str.clone(), content);
        self.documents.insert(path_str.clone(), doc);

        if let Some(doc) = self.documents.get_mut(&path_str) {
            doc.add_editor(user_id);
        }

        self.documents.get(&path_str)
    }

    /// Get a document
    pub fn get_document(&self, path: &str) -> Option<&CollaborativeDocument> {
        self.documents.get(path)
    }

    /// Get a document mutably
    pub fn get_document_mut(&mut self, path: &str) -> Option<&mut CollaborativeDocument> {
        self.documents.get_mut(path)
    }

    /// Apply operation to document
    pub fn apply_operation(
        &mut self,
        path: &str,
        op: Operation,
        user_id: &str,
    ) -> Result<(), String> {
        if !self.can_write(user_id) {
            return Err("User does not have write permission".to_string());
        }

        if let Some(doc) = self.documents.get_mut(path) {
            doc.apply(op)?;
            Ok(())
        } else {
            Err("Document not found".to_string())
        }
    }

    /// Get session summary
    pub fn summary(&self) -> SessionSummary {
        SessionSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            user_count: self.presence.user_count(),
            online_count: self.presence.online_count(),
            document_count: self.documents.len(),
            conflict_count: self.conflicts.conflict_count(),
            created_at: self.created_at,
            active: self.active,
        }
    }

    /// End session
    pub fn end(&mut self) {
        self.active = false;
        for user in self.presence.users.values_mut() {
            user.set_status(PresenceStatus::Offline);
        }
    }
}

/// Session summary
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub id: String,
    pub name: String,
    pub user_count: usize,
    pub online_count: usize,
    pub document_count: usize,
    pub conflict_count: usize,
    pub created_at: u64,
    pub active: bool,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Presence Tests

    #[test]
    fn test_presence_status_as_str() {
        assert_eq!(PresenceStatus::Active.as_str(), "active");
        assert_eq!(PresenceStatus::Offline.as_str(), "offline");
    }

    #[test]
    fn test_presence_status_is_available() {
        assert!(PresenceStatus::Active.is_available());
        assert!(PresenceStatus::Idle.is_available());
        assert!(!PresenceStatus::Offline.is_available());
        assert!(!PresenceStatus::Away.is_available());
    }

    #[test]
    fn test_collaborator_user_creation() {
        let user = CollaboratorUser::new("Alice")
            .with_email("alice@example.com")
            .with_color("#FF0000");

        assert_eq!(user.name, "Alice");
        assert_eq!(user.email, Some("alice@example.com".to_string()));
        assert_eq!(user.color, "#FF0000");
        assert!(user.is_online());
    }

    #[test]
    fn test_user_status_update() {
        let mut user = CollaboratorUser::new("Bob");
        assert_eq!(user.status, PresenceStatus::Active);
        assert!(user.is_online());

        user.set_status(PresenceStatus::Away);
        assert_eq!(user.status, PresenceStatus::Away);
        // Away status is still technically online (just not actively available)
        assert!(user.is_online());

        user.set_status(PresenceStatus::Offline);
        assert_eq!(user.status, PresenceStatus::Offline);
        assert!(!user.is_online());
    }

    #[test]
    fn test_cursor_position() {
        let cursor = CursorPosition::new("file.rs", 10, 5);
        assert_eq!(cursor.file, "file.rs");
        assert_eq!(cursor.line, 10);
        assert_eq!(cursor.column, 5);
        assert!(!cursor.has_selection());

        let cursor_with_selection = cursor.with_selection(15, 20);
        assert!(cursor_with_selection.has_selection());
    }

    #[test]
    fn test_presence_manager_add_user() {
        let mut manager = PresenceManager::new();
        let user = CollaboratorUser::new("Alice");
        let id = manager.add_user(user);

        assert_eq!(manager.user_count(), 1);
        assert!(manager.get_user(&id).is_some());
    }

    #[test]
    fn test_presence_manager_online_users() {
        let mut manager = PresenceManager::new();

        let alice = CollaboratorUser::new("Alice");
        let mut bob = CollaboratorUser::new("Bob");
        bob.set_status(PresenceStatus::Offline);

        manager.add_user(alice.clone());
        manager.add_user(bob);

        assert_eq!(manager.online_count(), 1);
        assert_eq!(manager.user_count(), 2);
    }

    // Shared Context Tests

    #[test]
    fn test_shared_context_set_get() {
        let mut context = SharedContext::new();
        context.set("key1", "value1", "user1");

        let item = context.get("key1").unwrap();
        assert_eq!(item.value, "value1");
        assert_eq!(context.get_value("key1"), Some("value1"));
    }

    #[test]
    fn test_shared_context_update() {
        let mut context = SharedContext::new();
        context.set("key1", "value1", "user1");
        context.set("key1", "value2", "user2");

        let item = context.get("key1").unwrap();
        assert_eq!(item.value, "value2");
        assert_eq!(item.version, 2);
        assert_eq!(item.modified_by, "user2");
    }

    #[test]
    fn test_shared_context_remove() {
        let mut context = SharedContext::new();
        context.set("key1", "value1", "user1");
        assert_eq!(context.count(), 1);

        let removed = context.remove("key1");
        assert!(removed.is_some());
        assert_eq!(context.count(), 0);
    }

    #[test]
    fn test_shared_context_history() {
        let mut context = SharedContext::new();
        context.set("key1", "v1", "user1");
        context.set("key1", "v2", "user1");
        context.set("key2", "v1", "user2");

        let changes = context.recent_changes(10);
        assert_eq!(changes.len(), 3);
    }

    // Conflict Resolution Tests

    #[test]
    fn test_conflict_creation() {
        let change_a = Change::new("user1", "content A", 1);
        let change_b = Change::new("user2", "content B", 1);

        let conflict = Conflict::new(ConflictType::EditConflict, "file.rs", change_a, change_b);

        assert_eq!(conflict.conflict_type, ConflictType::EditConflict);
        assert!(!conflict.resolved);
    }

    #[test]
    fn test_conflict_resolve() {
        let change_a = Change::new("user1", "content A", 1);
        let change_b = Change::new("user2", "content B", 1);

        let mut conflict = Conflict::new(ConflictType::EditConflict, "file.rs", change_a, change_b);
        conflict.resolve("merged content");

        assert!(conflict.resolved);
        assert_eq!(conflict.resolution, Some("merged content".to_string()));
    }

    #[test]
    fn test_conflict_resolver_detect() {
        let mut resolver = ConflictResolver::new();

        let change_a = Change::new("user1", "A", 1);
        let change_b = Change::new("user2", "B", 1);

        let id = resolver.detect_conflict("file.rs", change_a, change_b);
        assert_eq!(resolver.conflict_count(), 1);
        assert!(resolver.get_conflict(&id).is_some());
    }

    #[test]
    fn test_conflict_auto_resolve_last_write_wins() {
        let mut resolver =
            ConflictResolver::new().with_default_strategy(ResolutionStrategy::LastWriteWins);

        let change_a = Change {
            user_id: "user1".to_string(),
            content: "A".to_string(),
            timestamp: 1000,
            version: 1,
        };
        let change_b = Change {
            user_id: "user2".to_string(),
            content: "B".to_string(),
            timestamp: 2000,
            version: 1,
        };

        let id = resolver.detect_conflict("file.rs", change_a, change_b);
        let resolution = resolver.auto_resolve(&id);

        assert_eq!(resolution, Some("B".to_string()));
        assert_eq!(resolver.conflict_count(), 0);
    }

    // Collaborative Editing Tests

    #[test]
    fn test_operation_insert() {
        let op = Operation::insert(0, "Hello", "user1");
        assert_eq!(op.op_type, OperationType::Insert);
        assert_eq!(op.position, 0);
        assert_eq!(op.content, Some("Hello".to_string()));
    }

    #[test]
    fn test_operation_delete() {
        let op = Operation::delete(5, 3, "user1");
        assert_eq!(op.op_type, OperationType::Delete);
        assert_eq!(op.position, 5);
        assert_eq!(op.length, 3);
    }

    #[test]
    fn test_collaborative_document_apply_insert() {
        let mut doc = CollaborativeDocument::new("test.txt", "Hello World");
        let op = Operation::insert(5, " Beautiful", "user1");

        doc.apply(op).unwrap();
        assert_eq!(doc.content, "Hello Beautiful World");
        assert_eq!(doc.version, 1);
    }

    #[test]
    fn test_collaborative_document_apply_delete() {
        let mut doc = CollaborativeDocument::new("test.txt", "Hello World");
        let op = Operation::delete(5, 6, "user1");

        doc.apply(op).unwrap();
        assert_eq!(doc.content, "Hello");
    }

    #[test]
    fn test_collaborative_document_transform() {
        let op1 = Operation::insert(5, "A", "user1");
        let op2 = Operation::insert(3, "B", "user2");

        let transformed = CollaborativeDocument::transform(&op1, &op2);

        // op1 should shift because op2 inserted before it
        assert_eq!(transformed.position, 6);
    }

    #[test]
    fn test_collaborative_document_editors() {
        let mut doc = CollaborativeDocument::new("test.txt", "content");
        doc.add_editor("user1");
        doc.add_editor("user2");

        assert_eq!(doc.editor_count(), 2);

        doc.remove_editor("user1");
        assert_eq!(doc.editor_count(), 1);
    }

    // Session Tests

    #[test]
    fn test_session_creation() {
        let owner = CollaboratorUser::new("Alice");
        let session = CollaborationSession::new("Test Session", owner);

        assert_eq!(session.name, "Test Session");
        assert!(session.active);
        assert_eq!(session.presence.user_count(), 1);
    }

    #[test]
    fn test_session_join() {
        let owner = CollaboratorUser::new("Alice");
        let mut session = CollaborationSession::new("Test Session", owner);

        let bob = CollaboratorUser::new("Bob");
        let bob_id = session.join(bob, Permission::Write);

        assert_eq!(session.presence.user_count(), 2);
        assert!(session.can_write(&bob_id));
    }

    #[test]
    fn test_session_permissions() {
        let owner = CollaboratorUser::new("Alice");
        let mut session = CollaborationSession::new("Test Session", owner);

        let bob = CollaboratorUser::new("Bob");
        let bob_id = session.join(bob, Permission::Read);

        assert!(!session.can_write(&bob_id));
        assert_eq!(session.get_permission(&bob_id), Some(Permission::Read));
    }

    #[test]
    fn test_session_open_document() {
        let owner = CollaboratorUser::new("Alice");
        let owner_id = owner.id.clone();
        let mut session = CollaborationSession::new("Test Session", owner);

        session.open_document("test.txt", "Hello World", &owner_id);

        let doc = session.get_document("test.txt").unwrap();
        assert_eq!(doc.content, "Hello World");
        assert_eq!(doc.editor_count(), 1);
    }

    #[test]
    fn test_session_apply_operation() {
        let owner = CollaboratorUser::new("Alice");
        let owner_id = owner.id.clone();
        let mut session = CollaborationSession::new("Test Session", owner);

        session.open_document("test.txt", "Hello", &owner_id);
        let op = Operation::insert(5, " World", &owner_id);

        session.apply_operation("test.txt", op, &owner_id).unwrap();

        let doc = session.get_document("test.txt").unwrap();
        assert_eq!(doc.content, "Hello World");
    }

    #[test]
    fn test_session_summary() {
        let owner = CollaboratorUser::new("Alice");
        let session = CollaborationSession::new("Test Session", owner);

        let summary = session.summary();
        assert_eq!(summary.name, "Test Session");
        assert_eq!(summary.user_count, 1);
        assert!(summary.active);
    }

    #[test]
    fn test_session_end() {
        let owner = CollaboratorUser::new("Alice");
        let mut session = CollaborationSession::new("Test Session", owner);

        session.end();
        assert!(!session.active);
    }

    // Permission Tests

    #[test]
    fn test_permission_can_write() {
        assert!(!Permission::Read.can_write());
        assert!(Permission::Write.can_write());
        assert!(Permission::Admin.can_write());
    }

    #[test]
    fn test_permission_can_admin() {
        assert!(!Permission::Read.can_admin());
        assert!(!Permission::Write.can_admin());
        assert!(Permission::Admin.can_admin());
    }

    // Activity Type Tests

    #[test]
    fn test_activity_type_as_str() {
        assert_eq!(ActivityType::Typing.as_str(), "typing");
        assert_eq!(ActivityType::Debugging.as_str(), "debugging");
        assert_eq!(
            ActivityType::Custom("custom".to_string()).as_str(),
            "custom"
        );
    }

    // Unique ID Tests

    #[test]
    fn test_unique_session_ids() {
        let id1 = generate_session_id();
        let id2 = generate_session_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_unique_user_ids() {
        let id1 = generate_user_id();
        let id2 = generate_user_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_unique_operation_ids() {
        let id1 = generate_operation_id();
        let id2 = generate_operation_id();
        assert_ne!(id1, id2);
    }

    // Conflict Type Tests

    #[test]
    fn test_conflict_type_as_str() {
        assert_eq!(ConflictType::EditConflict.as_str(), "edit_conflict");
        assert_eq!(ConflictType::DeleteConflict.as_str(), "delete_conflict");
    }

    // Resolution Strategy Tests

    #[test]
    fn test_resolution_strategy_as_str() {
        assert_eq!(
            ResolutionStrategy::LastWriteWins.as_str(),
            "last_write_wins"
        );
        assert_eq!(ResolutionStrategy::Merge.as_str(), "merge");
    }

    // Context Scope Tests

    #[test]
    fn test_context_scope() {
        let scope = ContextScope::Session;
        assert_eq!(scope, ContextScope::Session);

        let mut users = HashSet::new();
        users.insert("user1".to_string());
        let private = ContextScope::Private(users);
        assert!(matches!(private, ContextScope::Private(_)));
    }

    #[test]
    fn test_shared_context_visible_to() {
        let mut context = SharedContext::new();
        context.set("public", "value", "user1");

        let visible = context.visible_to("user1");
        assert_eq!(visible.len(), 1);
    }

    #[test]
    fn test_cursor_update() {
        let mut cursor = CursorPosition::new("file.rs", 1, 1);
        cursor.update(10, 5);

        assert_eq!(cursor.line, 10);
        assert_eq!(cursor.column, 5);
        assert!(!cursor.has_selection());
    }

    #[test]
    fn test_document_operations_since() {
        let mut doc = CollaborativeDocument::new("test.txt", "Hello");

        doc.apply(Operation::insert(5, " World", "user1")).unwrap();
        doc.apply(Operation::insert(11, "!", "user1")).unwrap();

        let ops = doc.operations_since(0);
        assert_eq!(ops.len(), 2);

        let ops = doc.operations_since(1);
        assert_eq!(ops.len(), 1);
    }
}
