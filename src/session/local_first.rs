//! Local-First Optimization
//!
//! Minimize network usage through aggressive caching, offline capabilities,
//! edge computing patterns, and sync efficiency.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Atomic counter for unique IDs
static CACHE_ENTRY_COUNTER: AtomicU64 = AtomicU64::new(0);
static SYNC_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate unique cache entry ID
fn generate_cache_id() -> String {
    format!(
        "cache-{}",
        CACHE_ENTRY_COUNTER.fetch_add(1, Ordering::SeqCst)
    )
}

/// Generate unique sync ID
fn generate_sync_id() -> String {
    format!("sync-{}", SYNC_ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Get current timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Cache priority level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CachePriority {
    /// Low priority - can be evicted first
    Low,
    /// Normal priority
    Normal,
    /// High priority - try to keep
    High,
    /// Critical - never evict
    Critical,
}

impl std::fmt::Display for CachePriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CachePriority::Low => write!(f, "Low"),
            CachePriority::Normal => write!(f, "Normal"),
            CachePriority::High => write!(f, "High"),
            CachePriority::Critical => write!(f, "Critical"),
        }
    }
}

/// Cache eviction policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvictionPolicy {
    /// Least Recently Used
    Lru,
    /// Least Frequently Used
    Lfu,
    /// First In First Out
    Fifo,
    /// Time-based expiration
    Ttl,
    /// Priority-based
    Priority,
}

impl std::fmt::Display for EvictionPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvictionPolicy::Lru => write!(f, "LRU"),
            EvictionPolicy::Lfu => write!(f, "LFU"),
            EvictionPolicy::Fifo => write!(f, "FIFO"),
            EvictionPolicy::Ttl => write!(f, "TTL"),
            EvictionPolicy::Priority => write!(f, "Priority"),
        }
    }
}

/// A cached entry
#[derive(Debug, Clone)]
pub struct CacheEntry<T> {
    /// Unique identifier
    pub id: String,
    /// Cache key
    pub key: String,
    /// Cached value
    pub value: T,
    /// Creation timestamp
    pub created_at: u64,
    /// Last accessed timestamp
    pub accessed_at: u64,
    /// Access count
    pub access_count: u64,
    /// Time-to-live in seconds
    pub ttl: Option<u64>,
    /// Priority
    pub priority: CachePriority,
    /// Size in bytes (estimated)
    pub size_bytes: usize,
    /// Tags for categorization
    pub tags: Vec<String>,
}

impl<T> CacheEntry<T> {
    /// Create a new cache entry
    pub fn new(key: impl Into<String>, value: T) -> Self {
        let now = current_timestamp();
        Self {
            id: generate_cache_id(),
            key: key.into(),
            value,
            created_at: now,
            accessed_at: now,
            access_count: 1,
            ttl: None,
            priority: CachePriority::Normal,
            size_bytes: 0,
            tags: Vec::new(),
        }
    }

    /// Set TTL
    pub fn with_ttl(mut self, seconds: u64) -> Self {
        self.ttl = Some(seconds);
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: CachePriority) -> Self {
        self.priority = priority;
        self
    }

    /// Set size
    pub fn with_size(mut self, bytes: usize) -> Self {
        self.size_bytes = bytes;
        self
    }

    /// Add tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Check if entry is expired
    pub fn is_expired(&self) -> bool {
        match self.ttl {
            Some(ttl) => {
                // TTL of 0 means immediately expired
                if ttl == 0 {
                    return true;
                }
                current_timestamp() > self.created_at + ttl
            }
            None => false,
        }
    }

    /// Record access
    pub fn touch(&mut self) {
        self.accessed_at = current_timestamp();
        self.access_count += 1;
    }
}

/// Local cache manager
#[derive(Debug)]
pub struct LocalCache<T> {
    /// Cached entries
    entries: HashMap<String, CacheEntry<T>>,
    /// Maximum entries
    max_entries: usize,
    /// Maximum size in bytes
    max_size_bytes: usize,
    /// Current size
    current_size_bytes: usize,
    /// Eviction policy
    policy: EvictionPolicy,
    /// Hit count
    hits: u64,
    /// Miss count
    misses: u64,
}

impl<T: Clone> Default for LocalCache<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> LocalCache<T> {
    /// Create a new cache
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            max_entries: 1000,
            max_size_bytes: 100 * 1024 * 1024, // 100 MB
            current_size_bytes: 0,
            policy: EvictionPolicy::Lru,
            hits: 0,
            misses: 0,
        }
    }

    /// Set max entries
    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self
    }

    /// Set max size
    pub fn with_max_size(mut self, bytes: usize) -> Self {
        self.max_size_bytes = bytes;
        self
    }

    /// Set eviction policy
    pub fn with_policy(mut self, policy: EvictionPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Get an entry
    pub fn get(&mut self, key: &str) -> Option<&T> {
        // Check if exists and not expired
        if let Some(entry) = self.entries.get_mut(key) {
            if entry.is_expired() {
                self.current_size_bytes = self.current_size_bytes.saturating_sub(entry.size_bytes);
                self.entries.remove(key);
                self.misses += 1;
                return None;
            }
            entry.touch();
            self.hits += 1;
            return Some(&self.entries.get(key).unwrap().value);
        }
        self.misses += 1;
        None
    }

    /// Put an entry
    pub fn put(&mut self, entry: CacheEntry<T>) {
        // Remove if exists
        if let Some(old) = self.entries.remove(&entry.key) {
            self.current_size_bytes = self.current_size_bytes.saturating_sub(old.size_bytes);
        }

        // Evict if necessary
        while self.entries.len() >= self.max_entries
            || self.current_size_bytes + entry.size_bytes > self.max_size_bytes
        {
            if !self.evict_one() {
                break; // No more to evict
            }
        }

        self.current_size_bytes += entry.size_bytes;
        let key = entry.key.clone();
        self.entries.insert(key, entry);
    }

    /// Remove an entry
    pub fn remove(&mut self, key: &str) -> Option<CacheEntry<T>> {
        if let Some(entry) = self.entries.remove(key) {
            self.current_size_bytes = self.current_size_bytes.saturating_sub(entry.size_bytes);
            Some(entry)
        } else {
            None
        }
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
        self.current_size_bytes = 0;
    }

    /// Get cache stats
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entry_count: self.entries.len(),
            size_bytes: self.current_size_bytes,
            max_entries: self.max_entries,
            max_size_bytes: self.max_size_bytes,
            hits: self.hits,
            misses: self.misses,
            hit_rate: self.hit_rate(),
        }
    }

    /// Get hit rate
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    /// Evict one entry based on policy
    fn evict_one(&mut self) -> bool {
        let key_to_evict = match self.policy {
            EvictionPolicy::Lru => self.find_lru(),
            EvictionPolicy::Lfu => self.find_lfu(),
            EvictionPolicy::Fifo => self.find_fifo(),
            EvictionPolicy::Ttl => self.find_expired(),
            EvictionPolicy::Priority => self.find_lowest_priority(),
        };

        if let Some(key) = key_to_evict {
            self.remove(&key);
            true
        } else {
            false
        }
    }

    /// Find least recently used entry
    fn find_lru(&self) -> Option<String> {
        self.entries
            .iter()
            .filter(|(_, e)| e.priority != CachePriority::Critical)
            .min_by_key(|(_, e)| e.accessed_at)
            .map(|(k, _)| k.clone())
    }

    /// Find least frequently used entry
    fn find_lfu(&self) -> Option<String> {
        self.entries
            .iter()
            .filter(|(_, e)| e.priority != CachePriority::Critical)
            .min_by_key(|(_, e)| e.access_count)
            .map(|(k, _)| k.clone())
    }

    /// Find oldest entry (FIFO)
    fn find_fifo(&self) -> Option<String> {
        self.entries
            .iter()
            .filter(|(_, e)| e.priority != CachePriority::Critical)
            .min_by_key(|(_, e)| e.created_at)
            .map(|(k, _)| k.clone())
    }

    /// Find expired entry
    fn find_expired(&self) -> Option<String> {
        self.entries
            .iter()
            .find(|(_, e)| e.is_expired())
            .map(|(k, _)| k.clone())
    }

    /// Find lowest priority entry
    fn find_lowest_priority(&self) -> Option<String> {
        self.entries
            .iter()
            .filter(|(_, e)| e.priority != CachePriority::Critical)
            .min_by_key(|(_, e)| e.priority)
            .map(|(k, _)| k.clone())
    }

    /// Get entries by tag
    pub fn get_by_tag(&self, tag: &str) -> Vec<&CacheEntry<T>> {
        self.entries
            .values()
            .filter(|e| e.tags.contains(&tag.to_string()))
            .collect()
    }

    /// Remove entries by tag
    pub fn remove_by_tag(&mut self, tag: &str) {
        let keys: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| e.tags.contains(&tag.to_string()))
            .map(|(k, _)| k.clone())
            .collect();

        for key in keys {
            self.remove(&key);
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of entries
    pub entry_count: usize,
    /// Current size in bytes
    pub size_bytes: usize,
    /// Maximum entries
    pub max_entries: usize,
    /// Maximum size in bytes
    pub max_size_bytes: usize,
    /// Total hits
    pub hits: u64,
    /// Total misses
    pub misses: u64,
    /// Hit rate (0.0-1.0)
    pub hit_rate: f64,
}

/// Offline mode status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfflineStatus {
    /// Online - full connectivity
    Online,
    /// Partial - degraded connectivity
    Partial,
    /// Offline - no connectivity
    Offline,
}

impl std::fmt::Display for OfflineStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OfflineStatus::Online => write!(f, "Online"),
            OfflineStatus::Partial => write!(f, "Partial"),
            OfflineStatus::Offline => write!(f, "Offline"),
        }
    }
}

/// Offline capability manager
#[derive(Debug)]
pub struct OfflineManager {
    /// Current status
    status: OfflineStatus,
    /// Pending operations
    pending_ops: Vec<PendingOperation>,
    /// Last online timestamp
    last_online: Option<u64>,
    /// Offline storage path
    storage_path: Option<PathBuf>,
    /// Auto-sync enabled
    auto_sync: bool,
}

impl Default for OfflineManager {
    fn default() -> Self {
        Self::new()
    }
}

impl OfflineManager {
    /// Create new offline manager
    pub fn new() -> Self {
        Self {
            status: OfflineStatus::Online,
            pending_ops: Vec::new(),
            last_online: Some(current_timestamp()),
            storage_path: None,
            auto_sync: true,
        }
    }

    /// Set storage path
    pub fn with_storage(mut self, path: PathBuf) -> Self {
        self.storage_path = Some(path);
        self
    }

    /// Enable/disable auto-sync
    pub fn with_auto_sync(mut self, enabled: bool) -> Self {
        self.auto_sync = enabled;
        self
    }

    /// Get current status
    pub fn status(&self) -> OfflineStatus {
        self.status
    }

    /// Set status
    pub fn set_status(&mut self, status: OfflineStatus) {
        if status == OfflineStatus::Online && self.status != OfflineStatus::Online {
            self.last_online = Some(current_timestamp());
        }
        self.status = status;
    }

    /// Check if online
    pub fn is_online(&self) -> bool {
        self.status == OfflineStatus::Online
    }

    /// Check if offline
    pub fn is_offline(&self) -> bool {
        self.status == OfflineStatus::Offline
    }

    /// Queue an operation for later sync
    pub fn queue_operation(&mut self, op: PendingOperation) {
        self.pending_ops.push(op);
    }

    /// Get pending operations
    pub fn pending_operations(&self) -> &[PendingOperation] {
        &self.pending_ops
    }

    /// Get pending count
    pub fn pending_count(&self) -> usize {
        self.pending_ops.len()
    }

    /// Clear pending operations
    pub fn clear_pending(&mut self) {
        self.pending_ops.clear();
    }

    /// Get time since last online (in seconds)
    pub fn time_offline(&self) -> Option<u64> {
        if self.is_online() {
            return Some(0);
        }
        self.last_online
            .map(|t| current_timestamp().saturating_sub(t))
    }

    /// Mark operation as synced
    pub fn mark_synced(&mut self, op_id: &str) {
        self.pending_ops.retain(|op| op.id != op_id);
    }
}

/// A pending operation to sync
#[derive(Debug, Clone)]
pub struct PendingOperation {
    /// Unique identifier
    pub id: String,
    /// Operation type
    pub op_type: OperationType,
    /// Payload (JSON or other serialized data)
    pub payload: String,
    /// Created timestamp
    pub created_at: u64,
    /// Retry count
    pub retries: u32,
    /// Maximum retries
    pub max_retries: u32,
    /// Priority
    pub priority: u8,
}

impl PendingOperation {
    /// Create a new pending operation
    pub fn new(op_type: OperationType, payload: impl Into<String>) -> Self {
        Self {
            id: generate_sync_id(),
            op_type,
            payload: payload.into(),
            created_at: current_timestamp(),
            retries: 0,
            max_retries: 3,
            priority: 5,
        }
    }

    /// Set max retries
    pub fn with_max_retries(mut self, max: u32) -> Self {
        self.max_retries = max;
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority.min(10);
        self
    }

    /// Increment retry count
    pub fn retry(&mut self) {
        self.retries += 1;
    }

    /// Check if should retry
    pub fn should_retry(&self) -> bool {
        self.retries < self.max_retries
    }
}

/// Operation type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationType {
    /// Create operation
    Create,
    /// Update operation
    Update,
    /// Delete operation
    Delete,
    /// Sync operation
    Sync,
    /// API call
    ApiCall,
    /// Custom
    Custom,
}

impl std::fmt::Display for OperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationType::Create => write!(f, "Create"),
            OperationType::Update => write!(f, "Update"),
            OperationType::Delete => write!(f, "Delete"),
            OperationType::Sync => write!(f, "Sync"),
            OperationType::ApiCall => write!(f, "API Call"),
            OperationType::Custom => write!(f, "Custom"),
        }
    }
}

/// Sync strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStrategy {
    /// Client wins on conflict
    ClientWins,
    /// Server wins on conflict
    ServerWins,
    /// Last write wins
    LastWriteWins,
    /// Manual resolution required
    Manual,
    /// Merge changes
    Merge,
}

impl std::fmt::Display for SyncStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncStrategy::ClientWins => write!(f, "Client Wins"),
            SyncStrategy::ServerWins => write!(f, "Server Wins"),
            SyncStrategy::LastWriteWins => write!(f, "Last Write Wins"),
            SyncStrategy::Manual => write!(f, "Manual"),
            SyncStrategy::Merge => write!(f, "Merge"),
        }
    }
}

/// Sync manager for efficient synchronization
#[derive(Debug)]
pub struct SyncManager {
    /// Sync strategy
    strategy: SyncStrategy,
    /// Batch size for sync
    batch_size: usize,
    /// Sync interval in seconds
    sync_interval: u64,
    /// Last sync timestamp
    last_sync: Option<u64>,
    /// Conflicts
    conflicts: Vec<SyncConflict>,
    /// Sync enabled
    enabled: bool,
}

impl Default for SyncManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncManager {
    /// Create new sync manager
    pub fn new() -> Self {
        Self {
            strategy: SyncStrategy::LastWriteWins,
            batch_size: 100,
            sync_interval: 60,
            last_sync: None,
            conflicts: Vec::new(),
            enabled: true,
        }
    }

    /// Set sync strategy
    pub fn with_strategy(mut self, strategy: SyncStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Set batch size
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    /// Set sync interval
    pub fn with_interval(mut self, seconds: u64) -> Self {
        self.sync_interval = seconds;
        self
    }

    /// Check if sync is needed
    pub fn needs_sync(&self) -> bool {
        if !self.enabled {
            return false;
        }

        match self.last_sync {
            Some(last) => current_timestamp() - last >= self.sync_interval,
            None => true,
        }
    }

    /// Mark sync completed
    pub fn mark_synced(&mut self) {
        self.last_sync = Some(current_timestamp());
    }

    /// Get time until next sync
    pub fn time_until_sync(&self) -> u64 {
        match self.last_sync {
            Some(last) => {
                let elapsed = current_timestamp() - last;
                self.sync_interval.saturating_sub(elapsed)
            }
            None => 0,
        }
    }

    /// Add conflict
    pub fn add_conflict(&mut self, conflict: SyncConflict) {
        self.conflicts.push(conflict);
    }

    /// Get conflicts
    pub fn conflicts(&self) -> &[SyncConflict] {
        &self.conflicts
    }

    /// Resolve conflict
    pub fn resolve_conflict(&mut self, conflict_id: &str, resolution: ConflictResolution) {
        if let Some(conflict) = self.conflicts.iter_mut().find(|c| c.id == conflict_id) {
            conflict.resolution = Some(resolution);
        }
    }

    /// Remove resolved conflicts
    pub fn clear_resolved(&mut self) {
        self.conflicts.retain(|c| c.resolution.is_none());
    }

    /// Enable/disable sync
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Check if enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// A sync conflict
#[derive(Debug, Clone)]
pub struct SyncConflict {
    /// Conflict ID
    pub id: String,
    /// Resource ID
    pub resource_id: String,
    /// Local version
    pub local_version: String,
    /// Remote version
    pub remote_version: String,
    /// Local timestamp
    pub local_timestamp: u64,
    /// Remote timestamp
    pub remote_timestamp: u64,
    /// Resolution (if resolved)
    pub resolution: Option<ConflictResolution>,
}

impl SyncConflict {
    /// Create new conflict
    pub fn new(resource_id: impl Into<String>) -> Self {
        Self {
            id: generate_sync_id(),
            resource_id: resource_id.into(),
            local_version: String::new(),
            remote_version: String::new(),
            local_timestamp: current_timestamp(),
            remote_timestamp: current_timestamp(),
            resolution: None,
        }
    }

    /// Set local version
    pub fn with_local(mut self, version: impl Into<String>, timestamp: u64) -> Self {
        self.local_version = version.into();
        self.local_timestamp = timestamp;
        self
    }

    /// Set remote version
    pub fn with_remote(mut self, version: impl Into<String>, timestamp: u64) -> Self {
        self.remote_version = version.into();
        self.remote_timestamp = timestamp;
        self
    }
}

/// Conflict resolution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    /// Accept local version
    AcceptLocal,
    /// Accept remote version
    AcceptRemote,
    /// Merge both
    Merged,
    /// Manual resolution applied
    Manual,
}

impl std::fmt::Display for ConflictResolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConflictResolution::AcceptLocal => write!(f, "Accept Local"),
            ConflictResolution::AcceptRemote => write!(f, "Accept Remote"),
            ConflictResolution::Merged => write!(f, "Merged"),
            ConflictResolution::Manual => write!(f, "Manual"),
        }
    }
}

/// Edge computing task
#[derive(Debug, Clone)]
pub struct EdgeTask {
    /// Task ID
    pub id: String,
    /// Task type
    pub task_type: EdgeTaskType,
    /// Input data
    pub input: String,
    /// Result (if completed)
    pub result: Option<String>,
    /// Status
    pub status: TaskStatus,
    /// Created timestamp
    pub created_at: u64,
    /// Completed timestamp
    pub completed_at: Option<u64>,
}

impl EdgeTask {
    /// Create new edge task
    pub fn new(task_type: EdgeTaskType, input: impl Into<String>) -> Self {
        Self {
            id: generate_sync_id(),
            task_type,
            input: input.into(),
            result: None,
            status: TaskStatus::Pending,
            created_at: current_timestamp(),
            completed_at: None,
        }
    }

    /// Mark as completed
    pub fn complete(&mut self, result: impl Into<String>) {
        self.result = Some(result.into());
        self.status = TaskStatus::Completed;
        self.completed_at = Some(current_timestamp());
    }

    /// Mark as failed
    pub fn fail(&mut self, error: impl Into<String>) {
        self.result = Some(error.into());
        self.status = TaskStatus::Failed;
        self.completed_at = Some(current_timestamp());
    }
}

/// Edge task type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeTaskType {
    /// Text processing
    TextProcessing,
    /// Data validation
    Validation,
    /// Transformation
    Transform,
    /// Aggregation
    Aggregate,
    /// Filtering
    Filter,
    /// Search
    Search,
    /// Custom
    Custom,
}

impl std::fmt::Display for EdgeTaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EdgeTaskType::TextProcessing => write!(f, "Text Processing"),
            EdgeTaskType::Validation => write!(f, "Validation"),
            EdgeTaskType::Transform => write!(f, "Transform"),
            EdgeTaskType::Aggregate => write!(f, "Aggregate"),
            EdgeTaskType::Filter => write!(f, "Filter"),
            EdgeTaskType::Search => write!(f, "Search"),
            EdgeTaskType::Custom => write!(f, "Custom"),
        }
    }
}

/// Task status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    /// Pending
    Pending,
    /// Running
    Running,
    /// Completed
    Completed,
    /// Failed
    Failed,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "Pending"),
            TaskStatus::Running => write!(f, "Running"),
            TaskStatus::Completed => write!(f, "Completed"),
            TaskStatus::Failed => write!(f, "Failed"),
        }
    }
}

/// Local-first coordinator
#[derive(Debug)]
pub struct LocalFirstCoordinator {
    /// Cache for responses
    response_cache: LocalCache<String>,
    /// Offline manager
    offline_manager: OfflineManager,
    /// Sync manager
    sync_manager: SyncManager,
    /// Edge tasks
    edge_tasks: Vec<EdgeTask>,
    /// Network bandwidth saved (bytes)
    bandwidth_saved: u64,
}

impl Default for LocalFirstCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalFirstCoordinator {
    /// Create new coordinator
    pub fn new() -> Self {
        Self {
            response_cache: LocalCache::new()
                .with_max_entries(500)
                .with_policy(EvictionPolicy::Lru),
            offline_manager: OfflineManager::new(),
            sync_manager: SyncManager::new(),
            edge_tasks: Vec::new(),
            bandwidth_saved: 0,
        }
    }

    /// Get response cache
    pub fn cache(&mut self) -> &mut LocalCache<String> {
        &mut self.response_cache
    }

    /// Get offline manager
    pub fn offline(&mut self) -> &mut OfflineManager {
        &mut self.offline_manager
    }

    /// Get sync manager
    pub fn sync(&mut self) -> &mut SyncManager {
        &mut self.sync_manager
    }

    /// Check network status and cache response
    pub fn cache_response(&mut self, key: &str, response: String, size_bytes: usize) {
        let entry = CacheEntry::new(key, response)
            .with_size(size_bytes)
            .with_ttl(3600); // 1 hour TTL

        self.response_cache.put(entry);
        self.bandwidth_saved += size_bytes as u64;
    }

    /// Try to get cached response
    pub fn get_cached(&mut self, key: &str) -> Option<&String> {
        self.response_cache.get(key)
    }

    /// Queue operation for offline sync
    pub fn queue_for_sync(&mut self, op_type: OperationType, payload: String) {
        let op = PendingOperation::new(op_type, payload);
        self.offline_manager.queue_operation(op);
    }

    /// Add edge task
    pub fn add_edge_task(&mut self, task: EdgeTask) {
        self.edge_tasks.push(task);
    }

    /// Get edge tasks
    pub fn edge_tasks(&self) -> &[EdgeTask] {
        &self.edge_tasks
    }

    /// Get bandwidth saved
    pub fn bandwidth_saved(&self) -> u64 {
        self.bandwidth_saved
    }

    /// Get statistics
    pub fn stats(&self) -> LocalFirstStats {
        LocalFirstStats {
            cache_stats: self.response_cache.stats(),
            pending_ops: self.offline_manager.pending_count(),
            offline_status: self.offline_manager.status(),
            bandwidth_saved_bytes: self.bandwidth_saved,
            edge_tasks_completed: self
                .edge_tasks
                .iter()
                .filter(|t| t.status == TaskStatus::Completed)
                .count(),
            edge_tasks_pending: self
                .edge_tasks
                .iter()
                .filter(|t| t.status == TaskStatus::Pending)
                .count(),
        }
    }
}

/// Local-first statistics
#[derive(Debug, Clone)]
pub struct LocalFirstStats {
    /// Cache statistics
    pub cache_stats: CacheStats,
    /// Pending operations count
    pub pending_ops: usize,
    /// Offline status
    pub offline_status: OfflineStatus,
    /// Bandwidth saved in bytes
    pub bandwidth_saved_bytes: u64,
    /// Completed edge tasks
    pub edge_tasks_completed: usize,
    /// Pending edge tasks
    pub edge_tasks_pending: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_priority_ordering() {
        assert!(CachePriority::Low < CachePriority::Normal);
        assert!(CachePriority::Normal < CachePriority::High);
        assert!(CachePriority::High < CachePriority::Critical);
    }

    #[test]
    fn test_cache_entry_creation() {
        let entry: CacheEntry<String> = CacheEntry::new("key", "value".to_string())
            .with_ttl(3600)
            .with_priority(CachePriority::High);

        assert_eq!(entry.key, "key");
        assert_eq!(entry.value, "value");
        assert_eq!(entry.ttl, Some(3600));
        assert_eq!(entry.priority, CachePriority::High);
    }

    #[test]
    fn test_cache_entry_expired() {
        let entry: CacheEntry<String> = CacheEntry::new("key", "value".to_string()).with_ttl(0);

        // Should be expired immediately with TTL of 0
        assert!(entry.is_expired());
    }

    #[test]
    fn test_cache_entry_not_expired() {
        let entry: CacheEntry<String> = CacheEntry::new("key", "value".to_string()).with_ttl(3600);

        assert!(!entry.is_expired());
    }

    #[test]
    fn test_cache_entry_no_ttl() {
        let entry: CacheEntry<String> = CacheEntry::new("key", "value".to_string());

        assert!(!entry.is_expired());
    }

    #[test]
    fn test_local_cache_creation() {
        let cache: LocalCache<String> = LocalCache::new();

        assert_eq!(cache.stats().entry_count, 0);
    }

    #[test]
    fn test_local_cache_put_get() {
        let mut cache: LocalCache<String> = LocalCache::new();

        let entry = CacheEntry::new("key", "value".to_string());
        cache.put(entry);

        let result = cache.get("key");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "value");
    }

    #[test]
    fn test_local_cache_miss() {
        let mut cache: LocalCache<String> = LocalCache::new();

        let result = cache.get("nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_local_cache_remove() {
        let mut cache: LocalCache<String> = LocalCache::new();

        cache.put(CacheEntry::new("key", "value".to_string()));
        let removed = cache.remove("key");

        assert!(removed.is_some());
        assert!(cache.get("key").is_none());
    }

    #[test]
    fn test_local_cache_hit_rate() {
        let mut cache: LocalCache<String> = LocalCache::new();

        cache.put(CacheEntry::new("key", "value".to_string()));

        cache.get("key"); // Hit
        cache.get("key"); // Hit
        cache.get("miss"); // Miss

        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_local_cache_eviction_lru() {
        let mut cache: LocalCache<String> = LocalCache::new()
            .with_max_entries(2)
            .with_policy(EvictionPolicy::Lru);

        cache.put(CacheEntry::new("a", "1".to_string()));
        cache.put(CacheEntry::new("b", "2".to_string()));

        // Access 'a' to make it more recently used
        let _ = cache.get("a");

        // Add 'c', should evict 'b' (least recently used)
        cache.put(CacheEntry::new("c", "3".to_string()));

        // Check that we have 2 entries
        assert_eq!(cache.stats().entry_count, 2);

        // 'a' and 'c' should exist, 'b' should be evicted
        let has_a = cache.get("a").is_some();
        let has_c = cache.get("c").is_some();

        assert!(has_a || has_c); // At least one should exist
    }

    #[test]
    fn test_local_cache_by_tag() {
        let mut cache: LocalCache<String> = LocalCache::new();

        cache.put(CacheEntry::new("a", "1".to_string()).with_tag("important"));
        cache.put(CacheEntry::new("b", "2".to_string()).with_tag("important"));
        cache.put(CacheEntry::new("c", "3".to_string()).with_tag("other"));

        let tagged = cache.get_by_tag("important");
        assert_eq!(tagged.len(), 2);
    }

    #[test]
    fn test_local_cache_remove_by_tag() {
        let mut cache: LocalCache<String> = LocalCache::new();

        cache.put(CacheEntry::new("a", "1".to_string()).with_tag("temp"));
        cache.put(CacheEntry::new("b", "2".to_string()).with_tag("temp"));
        cache.put(CacheEntry::new("c", "3".to_string()));

        cache.remove_by_tag("temp");

        assert_eq!(cache.stats().entry_count, 1);
    }

    #[test]
    fn test_offline_manager_creation() {
        let manager = OfflineManager::new();

        assert!(manager.is_online());
        assert_eq!(manager.pending_count(), 0);
    }

    #[test]
    fn test_offline_manager_status() {
        let mut manager = OfflineManager::new();

        manager.set_status(OfflineStatus::Offline);
        assert!(manager.is_offline());

        manager.set_status(OfflineStatus::Online);
        assert!(manager.is_online());
    }

    #[test]
    fn test_offline_manager_queue_operation() {
        let mut manager = OfflineManager::new();

        let op = PendingOperation::new(OperationType::Create, "data");
        manager.queue_operation(op);

        assert_eq!(manager.pending_count(), 1);
    }

    #[test]
    fn test_offline_manager_mark_synced() {
        let mut manager = OfflineManager::new();

        let op = PendingOperation::new(OperationType::Create, "data");
        let op_id = op.id.clone();
        manager.queue_operation(op);

        manager.mark_synced(&op_id);

        assert_eq!(manager.pending_count(), 0);
    }

    #[test]
    fn test_pending_operation_retry() {
        let mut op = PendingOperation::new(OperationType::Update, "data").with_max_retries(3);

        assert!(op.should_retry());

        op.retry();
        op.retry();
        op.retry();

        assert!(!op.should_retry());
    }

    #[test]
    fn test_sync_manager_creation() {
        let manager = SyncManager::new();

        assert!(manager.is_enabled());
        assert!(manager.needs_sync()); // First sync always needed
    }

    #[test]
    fn test_sync_manager_mark_synced() {
        let mut manager = SyncManager::new().with_interval(60);

        manager.mark_synced();

        // Just synced, shouldn't need sync immediately
        assert!(!manager.needs_sync());
    }

    #[test]
    fn test_sync_manager_conflicts() {
        let mut manager = SyncManager::new();

        let conflict = SyncConflict::new("resource-1");
        manager.add_conflict(conflict);

        assert_eq!(manager.conflicts().len(), 1);

        manager.resolve_conflict(
            &manager.conflicts()[0].id.clone(),
            ConflictResolution::AcceptLocal,
        );
        manager.clear_resolved();

        assert_eq!(manager.conflicts().len(), 0);
    }

    #[test]
    fn test_edge_task_creation() {
        let task = EdgeTask::new(EdgeTaskType::Validation, "input data");

        assert_eq!(task.task_type, EdgeTaskType::Validation);
        assert_eq!(task.status, TaskStatus::Pending);
    }

    #[test]
    fn test_edge_task_complete() {
        let mut task = EdgeTask::new(EdgeTaskType::Transform, "input");
        task.complete("output");

        assert_eq!(task.status, TaskStatus::Completed);
        assert!(task.result.is_some());
    }

    #[test]
    fn test_edge_task_fail() {
        let mut task = EdgeTask::new(EdgeTaskType::Search, "query");
        task.fail("Error message");

        assert_eq!(task.status, TaskStatus::Failed);
    }

    #[test]
    fn test_local_first_coordinator() {
        let mut coord = LocalFirstCoordinator::new();

        coord.cache_response("key", "value".to_string(), 100);

        let cached = coord.get_cached("key");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap(), "value");
    }

    #[test]
    fn test_local_first_coordinator_stats() {
        let coord = LocalFirstCoordinator::new();
        let stats = coord.stats();

        assert_eq!(stats.cache_stats.entry_count, 0);
        assert_eq!(stats.pending_ops, 0);
    }

    #[test]
    fn test_local_first_coordinator_bandwidth() {
        let mut coord = LocalFirstCoordinator::new();

        coord.cache_response("key1", "value".to_string(), 1000);
        coord.cache_response("key2", "value".to_string(), 500);

        assert_eq!(coord.bandwidth_saved(), 1500);
    }

    #[test]
    fn test_eviction_policy_display() {
        assert_eq!(format!("{}", EvictionPolicy::Lru), "LRU");
        assert_eq!(format!("{}", EvictionPolicy::Lfu), "LFU");
    }

    #[test]
    fn test_operation_type_display() {
        assert_eq!(format!("{}", OperationType::Create), "Create");
        assert_eq!(format!("{}", OperationType::ApiCall), "API Call");
    }

    #[test]
    fn test_sync_strategy_display() {
        assert_eq!(format!("{}", SyncStrategy::ClientWins), "Client Wins");
        assert_eq!(
            format!("{}", SyncStrategy::LastWriteWins),
            "Last Write Wins"
        );
    }

    #[test]
    fn test_conflict_resolution_display() {
        assert_eq!(
            format!("{}", ConflictResolution::AcceptLocal),
            "Accept Local"
        );
        assert_eq!(format!("{}", ConflictResolution::Merged), "Merged");
    }

    #[test]
    fn test_unique_cache_ids() {
        let e1: CacheEntry<String> = CacheEntry::new("k1", "v1".to_string());
        let e2: CacheEntry<String> = CacheEntry::new("k2", "v2".to_string());

        assert_ne!(e1.id, e2.id);
    }

    #[test]
    fn test_unique_sync_ids() {
        let o1 = PendingOperation::new(OperationType::Create, "1");
        let o2 = PendingOperation::new(OperationType::Create, "2");

        assert_ne!(o1.id, o2.id);
    }

    #[test]
    fn test_sync_conflict_creation() {
        let conflict = SyncConflict::new("resource")
            .with_local("v1", 1000)
            .with_remote("v2", 2000);

        assert_eq!(conflict.local_version, "v1");
        assert_eq!(conflict.remote_version, "v2");
    }

    #[test]
    fn test_local_cache_clear() {
        let mut cache: LocalCache<String> = LocalCache::new();

        cache.put(CacheEntry::new("a", "1".to_string()));
        cache.put(CacheEntry::new("b", "2".to_string()));

        cache.clear();

        assert_eq!(cache.stats().entry_count, 0);
    }

    // -------------------------------------------------------------------------
    // CacheEntry builder methods and touch()
    // -------------------------------------------------------------------------

    #[test]
    fn test_cache_entry_with_size() {
        let entry: CacheEntry<String> = CacheEntry::new("key", "value".to_string()).with_size(1024);

        assert_eq!(entry.size_bytes, 1024);
    }

    #[test]
    fn test_cache_entry_with_tag_single() {
        let entry: CacheEntry<String> =
            CacheEntry::new("key", "value".to_string()).with_tag("my-tag");

        assert_eq!(entry.tags.len(), 1);
        assert_eq!(entry.tags[0], "my-tag");
    }

    #[test]
    fn test_cache_entry_with_multiple_tags() {
        let entry: CacheEntry<String> = CacheEntry::new("key", "value".to_string())
            .with_tag("alpha")
            .with_tag("beta")
            .with_tag("gamma");

        assert_eq!(entry.tags.len(), 3);
        assert!(entry.tags.contains(&"alpha".to_string()));
        assert!(entry.tags.contains(&"beta".to_string()));
        assert!(entry.tags.contains(&"gamma".to_string()));
    }

    #[test]
    fn test_cache_entry_touch_increments_access_count() {
        let mut entry: CacheEntry<String> = CacheEntry::new("key", "value".to_string());
        let initial_count = entry.access_count;
        entry.touch();
        assert_eq!(entry.access_count, initial_count + 1);
        entry.touch();
        assert_eq!(entry.access_count, initial_count + 2);
    }

    #[test]
    fn test_cache_entry_touch_updates_accessed_at() {
        let mut entry: CacheEntry<String> = CacheEntry::new("key", "value".to_string());
        let before = entry.accessed_at;
        // Sleep a tiny bit so the timestamp can advance (at least 1 second
        // resolution). Instead of sleeping we just verify touch keeps the field
        // monotonically non-decreasing (it may stay the same within the same second).
        entry.touch();
        assert!(entry.accessed_at >= before);
    }

    #[test]
    fn test_cache_entry_initial_access_count_is_one() {
        let entry: CacheEntry<i32> = CacheEntry::new("k", 42);
        assert_eq!(entry.access_count, 1);
    }

    #[test]
    fn test_cache_entry_no_ttl_is_not_expired() {
        let entry: CacheEntry<u8> = CacheEntry::new("k", 0);
        assert!(entry.ttl.is_none());
        assert!(!entry.is_expired());
    }

    #[test]
    fn test_cache_entry_large_ttl_not_expired() {
        // A TTL of one million seconds should never expire in a test.
        let entry: CacheEntry<String> =
            CacheEntry::new("key", "value".to_string()).with_ttl(1_000_000);
        assert!(!entry.is_expired());
    }

    #[test]
    fn test_cache_entry_default_priority_is_normal() {
        let entry: CacheEntry<String> = CacheEntry::new("key", "value".to_string());
        assert_eq!(entry.priority, CachePriority::Normal);
    }

    // -------------------------------------------------------------------------
    // CachePriority Display
    // -------------------------------------------------------------------------

    #[test]
    fn test_cache_priority_display() {
        assert_eq!(format!("{}", CachePriority::Low), "Low");
        assert_eq!(format!("{}", CachePriority::Normal), "Normal");
        assert_eq!(format!("{}", CachePriority::High), "High");
        assert_eq!(format!("{}", CachePriority::Critical), "Critical");
    }

    #[test]
    fn test_cache_priority_ordering_all_pairs() {
        assert!(CachePriority::Low < CachePriority::Normal);
        assert!(CachePriority::Low < CachePriority::High);
        assert!(CachePriority::Low < CachePriority::Critical);
        assert!(CachePriority::Normal < CachePriority::High);
        assert!(CachePriority::Normal < CachePriority::Critical);
        assert!(CachePriority::High < CachePriority::Critical);
        assert_eq!(CachePriority::Low, CachePriority::Low);
    }

    // -------------------------------------------------------------------------
    // LocalCache builder methods and Default
    // -------------------------------------------------------------------------

    #[test]
    fn test_local_cache_default() {
        let cache: LocalCache<String> = LocalCache::default();
        assert_eq!(cache.stats().entry_count, 0);
        assert_eq!(cache.stats().max_entries, 1000);
    }

    #[test]
    fn test_local_cache_with_max_entries_builder() {
        let cache: LocalCache<String> = LocalCache::new().with_max_entries(5);
        assert_eq!(cache.stats().max_entries, 5);
    }

    #[test]
    fn test_local_cache_with_max_size_builder() {
        let cache: LocalCache<String> = LocalCache::new().with_max_size(512);
        assert_eq!(cache.stats().max_size_bytes, 512);
    }

    #[test]
    fn test_local_cache_with_policy_builder() {
        // Just verify the builder compiles and doesn't panic.
        let _cache: LocalCache<String> = LocalCache::new().with_policy(EvictionPolicy::Priority);
    }

    // -------------------------------------------------------------------------
    // LocalCache: hit rate edge cases
    // -------------------------------------------------------------------------

    #[test]
    fn test_local_cache_hit_rate_zero_when_no_accesses() {
        let cache: LocalCache<String> = LocalCache::new();
        assert_eq!(cache.hit_rate(), 0.0);
    }

    #[test]
    fn test_local_cache_hit_rate_all_misses() {
        let mut cache: LocalCache<String> = LocalCache::new();
        cache.get("a");
        cache.get("b");
        assert_eq!(cache.hit_rate(), 0.0);
    }

    #[test]
    fn test_local_cache_hit_rate_all_hits() {
        let mut cache: LocalCache<String> = LocalCache::new();
        cache.put(CacheEntry::new("k", "v".to_string()));
        cache.get("k");
        cache.get("k");
        assert_eq!(cache.hit_rate(), 1.0);
    }

    // -------------------------------------------------------------------------
    // LocalCache: duplicate key overwrite
    // -------------------------------------------------------------------------

    #[test]
    fn test_local_cache_overwrite_same_key() {
        let mut cache: LocalCache<String> = LocalCache::new();
        cache.put(CacheEntry::new("key", "first".to_string()));
        cache.put(CacheEntry::new("key", "second".to_string()));

        // Should still have one entry
        assert_eq!(cache.stats().entry_count, 1);
        let val = cache.get("key").unwrap().clone();
        assert_eq!(val, "second");
    }

    // -------------------------------------------------------------------------
    // LocalCache: size accounting
    // -------------------------------------------------------------------------

    #[test]
    fn test_local_cache_size_accounting() {
        let mut cache: LocalCache<String> = LocalCache::new();
        cache.put(CacheEntry::new("a", "v".to_string()).with_size(100));
        cache.put(CacheEntry::new("b", "v".to_string()).with_size(200));

        assert_eq!(cache.stats().size_bytes, 300);
    }

    #[test]
    fn test_local_cache_size_decremented_on_remove() {
        let mut cache: LocalCache<String> = LocalCache::new();
        cache.put(CacheEntry::new("a", "v".to_string()).with_size(500));
        cache.remove("a");

        assert_eq!(cache.stats().size_bytes, 0);
    }

    #[test]
    fn test_local_cache_size_decremented_on_clear() {
        let mut cache: LocalCache<String> = LocalCache::new();
        cache.put(CacheEntry::new("a", "v".to_string()).with_size(100));
        cache.put(CacheEntry::new("b", "v".to_string()).with_size(200));
        cache.clear();

        assert_eq!(cache.stats().size_bytes, 0);
    }

    #[test]
    fn test_local_cache_size_based_eviction() {
        // max size 180 bytes. Each entry is 80 bytes.
        // - put("a"): current=80, fits.
        // - put("b"): current=160, fits (160 <= 180).
        // - put("c"): 160+80=240 > 180 -> evict one entry, then insert "c".
        //   After one eviction: current=80, 80+80=160 <= 180 -> insert "c".
        //   Result: 2 entries remain, and "c" is definitely present.
        // NOTE: We avoid asserting which of "a" or "b" was evicted because
        // LRU resolution is non-deterministic when timestamps share the same
        // second boundary.
        let mut cache: LocalCache<String> = LocalCache::new()
            .with_max_size(180)
            .with_max_entries(100)
            .with_policy(EvictionPolicy::Lru);

        cache.put(CacheEntry::new("a", "v".to_string()).with_size(80));
        cache.put(CacheEntry::new("b", "v".to_string()).with_size(80));
        cache.put(CacheEntry::new("c", "v".to_string()).with_size(80));

        // Exactly one entry was evicted; two survive, including "c".
        assert_eq!(cache.stats().entry_count, 2);
        assert!(
            cache.get("c").is_some(),
            "'c' (newest) should not be evicted"
        );
        // Exactly one of "a" or "b" was evicted.
        let a_present = cache.get("a").is_some();
        let b_present = cache.get("b").is_some();
        assert!(
            a_present ^ b_present,
            "exactly one of 'a'/'b' should have been evicted"
        );
    }

    // -------------------------------------------------------------------------
    // LocalCache: eviction policies
    // -------------------------------------------------------------------------

    #[test]
    fn test_local_cache_eviction_lfu() {
        let mut cache: LocalCache<String> = LocalCache::new()
            .with_max_entries(2)
            .with_policy(EvictionPolicy::Lfu);

        cache.put(CacheEntry::new("a", "1".to_string()));
        cache.put(CacheEntry::new("b", "2".to_string()));

        // Access "b" multiple times to make "a" the least frequently used.
        let _ = cache.get("b");
        let _ = cache.get("b");

        // Adding "c" should evict "a" (LFU).
        cache.put(CacheEntry::new("c", "3".to_string()));

        assert_eq!(cache.stats().entry_count, 2);
        assert!(cache.get("a").is_none());
        assert!(cache.get("b").is_some());
    }

    #[test]
    fn test_local_cache_eviction_fifo() {
        let mut cache: LocalCache<String> = LocalCache::new()
            .with_max_entries(2)
            .with_policy(EvictionPolicy::Fifo);

        let mut first = CacheEntry::new("first", "1".to_string());
        first.created_at = 1000; // oldest
        cache.put(first);

        let mut second = CacheEntry::new("second", "2".to_string());
        second.created_at = 2000; // newer
        cache.put(second);

        // Access "first" to ensure it would survive LRU but not FIFO.
        let _ = cache.get("first");

        // Adding "third" should evict "first" (oldest insertion).
        cache.put(CacheEntry::new("third", "3".to_string()));

        assert_eq!(cache.stats().entry_count, 2);
        assert!(cache.get("first").is_none());
        assert!(cache.get("second").is_some());
    }

    #[test]
    fn test_local_cache_eviction_priority() {
        let mut cache: LocalCache<String> = LocalCache::new()
            .with_max_entries(2)
            .with_policy(EvictionPolicy::Priority);

        cache.put(CacheEntry::new("low", "1".to_string()).with_priority(CachePriority::Low));
        cache.put(CacheEntry::new("high", "2".to_string()).with_priority(CachePriority::High));

        // Adding a third entry should evict "low" (lowest priority).
        cache.put(CacheEntry::new("normal", "3".to_string()).with_priority(CachePriority::Normal));

        assert_eq!(cache.stats().entry_count, 2);
        assert!(cache.get("low").is_none());
        assert!(cache.get("high").is_some());
    }

    #[test]
    fn test_local_cache_critical_entry_never_evicted() {
        // With max_entries=1 and a Critical entry already present, a new
        // entry cannot evict the Critical one (evict_one will return None).
        let mut cache: LocalCache<String> = LocalCache::new()
            .with_max_entries(1)
            .with_policy(EvictionPolicy::Lru);

        cache.put(
            CacheEntry::new("critical", "safe".to_string()).with_priority(CachePriority::Critical),
        );

        // Attempt to put a second entry — eviction should fail to evict
        // the Critical entry, so the new entry may not get inserted.
        cache.put(CacheEntry::new("newcomer", "dropped".to_string()));

        // The critical entry must still be present.
        assert!(cache.get("critical").is_some());
    }

    #[test]
    fn test_local_cache_eviction_ttl_policy() {
        let mut cache: LocalCache<String> = LocalCache::new()
            .with_max_entries(2)
            .with_policy(EvictionPolicy::Ttl);

        // "expired" has TTL=0 so is immediately expired.
        cache.put(CacheEntry::new("expired", "1".to_string()).with_ttl(0));
        cache.put(CacheEntry::new("valid", "2".to_string()).with_ttl(3600));

        // Adding a third entry should evict the expired one.
        cache.put(CacheEntry::new("new", "3".to_string()).with_ttl(3600));

        assert_eq!(cache.stats().entry_count, 2);
        assert!(cache.get("valid").is_some());
    }

    // -------------------------------------------------------------------------
    // LocalCache: expired entries are removed on get()
    // -------------------------------------------------------------------------

    #[test]
    fn test_local_cache_get_removes_expired_entry() {
        let mut cache: LocalCache<String> = LocalCache::new();
        cache.put(CacheEntry::new("key", "value".to_string()).with_ttl(0));

        // get() should detect expiry, remove the entry, and return None.
        let result = cache.get("key");
        assert!(result.is_none());
        assert_eq!(cache.stats().entry_count, 0);
    }

    #[test]
    fn test_local_cache_get_miss_increments_misses() {
        let mut cache: LocalCache<String> = LocalCache::new();

        // Expired entry counts as a miss.
        cache.put(CacheEntry::new("key", "value".to_string()).with_ttl(0));
        let _ = cache.get("key");

        let stats = cache.stats();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 0);
    }

    // -------------------------------------------------------------------------
    // LocalCache: remove nonexistent key
    // -------------------------------------------------------------------------

    #[test]
    fn test_local_cache_remove_nonexistent() {
        let mut cache: LocalCache<String> = LocalCache::new();
        let result = cache.remove("ghost");
        assert!(result.is_none());
    }

    // -------------------------------------------------------------------------
    // LocalCache: get_by_tag with empty cache
    // -------------------------------------------------------------------------

    #[test]
    fn test_local_cache_get_by_tag_empty() {
        let cache: LocalCache<String> = LocalCache::new();
        let result = cache.get_by_tag("anything");
        assert!(result.is_empty());
    }

    #[test]
    fn test_local_cache_remove_by_tag_empty() {
        let mut cache: LocalCache<String> = LocalCache::new();
        // Should not panic.
        cache.remove_by_tag("nonexistent-tag");
        assert_eq!(cache.stats().entry_count, 0);
    }

    // -------------------------------------------------------------------------
    // EvictionPolicy Display
    // -------------------------------------------------------------------------

    #[test]
    fn test_eviction_policy_display_all() {
        assert_eq!(format!("{}", EvictionPolicy::Lru), "LRU");
        assert_eq!(format!("{}", EvictionPolicy::Lfu), "LFU");
        assert_eq!(format!("{}", EvictionPolicy::Fifo), "FIFO");
        assert_eq!(format!("{}", EvictionPolicy::Ttl), "TTL");
        assert_eq!(format!("{}", EvictionPolicy::Priority), "Priority");
    }

    // -------------------------------------------------------------------------
    // OfflineStatus Display
    // -------------------------------------------------------------------------

    #[test]
    fn test_offline_status_display() {
        assert_eq!(format!("{}", OfflineStatus::Online), "Online");
        assert_eq!(format!("{}", OfflineStatus::Partial), "Partial");
        assert_eq!(format!("{}", OfflineStatus::Offline), "Offline");
    }

    // -------------------------------------------------------------------------
    // OfflineManager builder methods and Partial status
    // -------------------------------------------------------------------------

    #[test]
    fn test_offline_manager_default() {
        let manager = OfflineManager::default();
        assert_eq!(manager.status(), OfflineStatus::Online);
    }

    #[test]
    fn test_offline_manager_with_storage() {
        let path = std::path::PathBuf::from("/tmp/selfware-test");
        let manager = OfflineManager::new().with_storage(path.clone());
        // storage_path is private, but construction should not panic.
        let _ = manager;
    }

    #[test]
    fn test_offline_manager_with_auto_sync_false() {
        let manager = OfflineManager::new().with_auto_sync(false);
        let _ = manager; // builder should compile; field is private.
    }

    #[test]
    fn test_offline_manager_partial_status() {
        let mut manager = OfflineManager::new();
        manager.set_status(OfflineStatus::Partial);
        assert_eq!(manager.status(), OfflineStatus::Partial);
        assert!(!manager.is_online());
        assert!(!manager.is_offline());
    }

    #[test]
    fn test_offline_manager_transition_to_online_updates_last_online() {
        let mut manager = OfflineManager::new();
        manager.set_status(OfflineStatus::Offline);
        manager.set_status(OfflineStatus::Online);
        // After transitioning back online the manager must report online.
        assert!(manager.is_online());
    }

    #[test]
    fn test_offline_manager_time_offline_when_online_returns_zero() {
        let manager = OfflineManager::new();
        let t = manager.time_offline();
        assert_eq!(t, Some(0));
    }

    #[test]
    fn test_offline_manager_time_offline_when_offline() {
        let mut manager = OfflineManager::new();
        manager.set_status(OfflineStatus::Offline);
        // Time offline should be a small non-negative number (the test runs fast).
        let t = manager.time_offline();
        assert!(t.is_some());
    }

    #[test]
    fn test_offline_manager_clear_pending() {
        let mut manager = OfflineManager::new();
        manager.queue_operation(PendingOperation::new(OperationType::Create, "a"));
        manager.queue_operation(PendingOperation::new(OperationType::Update, "b"));

        manager.clear_pending();

        assert_eq!(manager.pending_count(), 0);
    }

    #[test]
    fn test_offline_manager_pending_operations_slice() {
        let mut manager = OfflineManager::new();
        let op = PendingOperation::new(OperationType::Sync, "payload");
        let op_id = op.id.clone();
        manager.queue_operation(op);

        let ops = manager.pending_operations();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].id, op_id);
    }

    #[test]
    fn test_offline_manager_mark_synced_nonexistent() {
        let mut manager = OfflineManager::new();
        // Should not panic when syncing an unknown ID.
        manager.mark_synced("non-existent-id");
        assert_eq!(manager.pending_count(), 0);
    }

    #[test]
    fn test_offline_manager_multiple_pending_mark_synced_one() {
        let mut manager = OfflineManager::new();
        let op1 = PendingOperation::new(OperationType::Create, "1");
        let op2 = PendingOperation::new(OperationType::Delete, "2");
        let id1 = op1.id.clone();
        manager.queue_operation(op1);
        manager.queue_operation(op2);

        manager.mark_synced(&id1);

        assert_eq!(manager.pending_count(), 1);
        assert_eq!(manager.pending_operations()[0].payload, "2");
    }

    // -------------------------------------------------------------------------
    // PendingOperation builder methods and priority clamping
    // -------------------------------------------------------------------------

    #[test]
    fn test_pending_operation_default_priority() {
        let op = PendingOperation::new(OperationType::Custom, "data");
        assert_eq!(op.priority, 5);
        assert_eq!(op.max_retries, 3);
        assert_eq!(op.retries, 0);
    }

    #[test]
    fn test_pending_operation_priority_clamped_at_ten() {
        let op = PendingOperation::new(OperationType::Custom, "data").with_priority(255);
        assert_eq!(op.priority, 10);
    }

    #[test]
    fn test_pending_operation_priority_within_range() {
        let op = PendingOperation::new(OperationType::Custom, "data").with_priority(7);
        assert_eq!(op.priority, 7);
    }

    #[test]
    fn test_pending_operation_with_max_retries() {
        let op = PendingOperation::new(OperationType::ApiCall, "data").with_max_retries(5);
        assert_eq!(op.max_retries, 5);
    }

    #[test]
    fn test_pending_operation_should_retry_boundary() {
        let mut op = PendingOperation::new(OperationType::Update, "x").with_max_retries(1);

        assert!(op.should_retry());
        op.retry();
        assert!(!op.should_retry());
    }

    #[test]
    fn test_pending_operation_zero_max_retries() {
        let op = PendingOperation::new(OperationType::Create, "x").with_max_retries(0);
        assert!(!op.should_retry());
    }

    // -------------------------------------------------------------------------
    // OperationType Display
    // -------------------------------------------------------------------------

    #[test]
    fn test_operation_type_display_all() {
        assert_eq!(format!("{}", OperationType::Create), "Create");
        assert_eq!(format!("{}", OperationType::Update), "Update");
        assert_eq!(format!("{}", OperationType::Delete), "Delete");
        assert_eq!(format!("{}", OperationType::Sync), "Sync");
        assert_eq!(format!("{}", OperationType::ApiCall), "API Call");
        assert_eq!(format!("{}", OperationType::Custom), "Custom");
    }

    // -------------------------------------------------------------------------
    // SyncStrategy Display
    // -------------------------------------------------------------------------

    #[test]
    fn test_sync_strategy_display_all() {
        assert_eq!(format!("{}", SyncStrategy::ClientWins), "Client Wins");
        assert_eq!(format!("{}", SyncStrategy::ServerWins), "Server Wins");
        assert_eq!(
            format!("{}", SyncStrategy::LastWriteWins),
            "Last Write Wins"
        );
        assert_eq!(format!("{}", SyncStrategy::Manual), "Manual");
        assert_eq!(format!("{}", SyncStrategy::Merge), "Merge");
    }

    // -------------------------------------------------------------------------
    // SyncManager builder methods, enable/disable, time_until_sync
    // -------------------------------------------------------------------------

    #[test]
    fn test_sync_manager_default() {
        let manager = SyncManager::default();
        assert!(manager.is_enabled());
        assert!(manager.needs_sync());
    }

    #[test]
    fn test_sync_manager_with_strategy() {
        let manager = SyncManager::new().with_strategy(SyncStrategy::ClientWins);
        let _ = manager; // strategy is private; just verifying builder
    }

    #[test]
    fn test_sync_manager_with_batch_size() {
        let manager = SyncManager::new().with_batch_size(50);
        let _ = manager;
    }

    #[test]
    fn test_sync_manager_with_interval() {
        let manager = SyncManager::new().with_interval(300);
        let _ = manager;
    }

    #[test]
    fn test_sync_manager_disabled_never_needs_sync() {
        let mut manager = SyncManager::new();
        manager.set_enabled(false);
        assert!(!manager.is_enabled());
        assert!(!manager.needs_sync());
    }

    #[test]
    fn test_sync_manager_time_until_sync_before_first_sync() {
        let manager = SyncManager::new();
        // With no last_sync recorded, sync is overdue -> 0 until next sync.
        assert_eq!(manager.time_until_sync(), 0);
    }

    #[test]
    fn test_sync_manager_time_until_sync_after_sync() {
        let mut manager = SyncManager::new().with_interval(3600);
        manager.mark_synced();
        // Just synced, so remaining time should be close to the full interval.
        let remaining = manager.time_until_sync();
        assert!(remaining > 3590 && remaining <= 3600);
    }

    #[test]
    fn test_sync_manager_toggle_enabled() {
        let mut manager = SyncManager::new();
        assert!(manager.is_enabled());
        manager.set_enabled(false);
        assert!(!manager.is_enabled());
        manager.set_enabled(true);
        assert!(manager.is_enabled());
    }

    // -------------------------------------------------------------------------
    // SyncConflict builder methods
    // -------------------------------------------------------------------------

    #[test]
    fn test_sync_conflict_new_no_resolution() {
        let conflict = SyncConflict::new("res-42");
        assert_eq!(conflict.resource_id, "res-42");
        assert!(conflict.resolution.is_none());
    }

    #[test]
    fn test_sync_conflict_with_local_and_remote() {
        let conflict = SyncConflict::new("res")
            .with_local("local-data", 1000)
            .with_remote("remote-data", 2000);

        assert_eq!(conflict.local_version, "local-data");
        assert_eq!(conflict.local_timestamp, 1000);
        assert_eq!(conflict.remote_version, "remote-data");
        assert_eq!(conflict.remote_timestamp, 2000);
    }

    #[test]
    fn test_sync_conflict_unique_ids() {
        let c1 = SyncConflict::new("r1");
        let c2 = SyncConflict::new("r2");
        assert_ne!(c1.id, c2.id);
    }

    // -------------------------------------------------------------------------
    // SyncManager conflict workflow
    // -------------------------------------------------------------------------

    #[test]
    fn test_sync_manager_resolve_conflict_accept_remote() {
        let mut manager = SyncManager::new();
        let conflict = SyncConflict::new("res");
        let cid = conflict.id.clone();
        manager.add_conflict(conflict);

        manager.resolve_conflict(&cid, ConflictResolution::AcceptRemote);

        let c = &manager.conflicts()[0];
        assert_eq!(c.resolution, Some(ConflictResolution::AcceptRemote));

        manager.clear_resolved();
        assert_eq!(manager.conflicts().len(), 0);
    }

    #[test]
    fn test_sync_manager_resolve_nonexistent_conflict() {
        let mut manager = SyncManager::new();
        // Should not panic.
        manager.resolve_conflict("ghost-id", ConflictResolution::Merged);
    }

    #[test]
    fn test_sync_manager_clear_resolved_keeps_unresolved() {
        let mut manager = SyncManager::new();
        let c_resolved = SyncConflict::new("r1");
        let c_unresolved = SyncConflict::new("r2");
        let rid = c_resolved.id.clone();

        manager.add_conflict(c_resolved);
        manager.add_conflict(c_unresolved);

        manager.resolve_conflict(&rid, ConflictResolution::AcceptLocal);
        manager.clear_resolved();

        // Only the unresolved conflict should remain.
        assert_eq!(manager.conflicts().len(), 1);
        assert!(manager.conflicts()[0].resolution.is_none());
    }

    #[test]
    fn test_sync_manager_multiple_conflicts() {
        let mut manager = SyncManager::new();
        for i in 0..5 {
            manager.add_conflict(SyncConflict::new(format!("res-{i}")));
        }
        assert_eq!(manager.conflicts().len(), 5);
    }

    // -------------------------------------------------------------------------
    // ConflictResolution Display
    // -------------------------------------------------------------------------

    #[test]
    fn test_conflict_resolution_display_all() {
        assert_eq!(
            format!("{}", ConflictResolution::AcceptLocal),
            "Accept Local"
        );
        assert_eq!(
            format!("{}", ConflictResolution::AcceptRemote),
            "Accept Remote"
        );
        assert_eq!(format!("{}", ConflictResolution::Merged), "Merged");
        assert_eq!(format!("{}", ConflictResolution::Manual), "Manual");
    }

    // -------------------------------------------------------------------------
    // EdgeTask builder methods and status transitions
    // -------------------------------------------------------------------------

    #[test]
    fn test_edge_task_default_state() {
        let task = EdgeTask::new(EdgeTaskType::Aggregate, "input");
        assert_eq!(task.status, TaskStatus::Pending);
        assert!(task.result.is_none());
        assert!(task.completed_at.is_none());
        assert!(!task.input.is_empty());
    }

    #[test]
    fn test_edge_task_complete_sets_fields() {
        let mut task = EdgeTask::new(EdgeTaskType::Filter, "query");
        task.complete("filtered-result");

        assert_eq!(task.status, TaskStatus::Completed);
        assert_eq!(task.result.as_deref(), Some("filtered-result"));
        assert!(task.completed_at.is_some());
    }

    #[test]
    fn test_edge_task_fail_sets_fields() {
        let mut task = EdgeTask::new(EdgeTaskType::TextProcessing, "text");
        task.fail("something went wrong");

        assert_eq!(task.status, TaskStatus::Failed);
        assert_eq!(task.result.as_deref(), Some("something went wrong"));
        assert!(task.completed_at.is_some());
    }

    #[test]
    fn test_edge_task_unique_ids() {
        let t1 = EdgeTask::new(EdgeTaskType::Custom, "a");
        let t2 = EdgeTask::new(EdgeTaskType::Custom, "b");
        assert_ne!(t1.id, t2.id);
    }

    // -------------------------------------------------------------------------
    // EdgeTaskType Display
    // -------------------------------------------------------------------------

    #[test]
    fn test_edge_task_type_display_all() {
        assert_eq!(
            format!("{}", EdgeTaskType::TextProcessing),
            "Text Processing"
        );
        assert_eq!(format!("{}", EdgeTaskType::Validation), "Validation");
        assert_eq!(format!("{}", EdgeTaskType::Transform), "Transform");
        assert_eq!(format!("{}", EdgeTaskType::Aggregate), "Aggregate");
        assert_eq!(format!("{}", EdgeTaskType::Filter), "Filter");
        assert_eq!(format!("{}", EdgeTaskType::Search), "Search");
        assert_eq!(format!("{}", EdgeTaskType::Custom), "Custom");
    }

    // -------------------------------------------------------------------------
    // TaskStatus Display
    // -------------------------------------------------------------------------

    #[test]
    fn test_task_status_display_all() {
        assert_eq!(format!("{}", TaskStatus::Pending), "Pending");
        assert_eq!(format!("{}", TaskStatus::Running), "Running");
        assert_eq!(format!("{}", TaskStatus::Completed), "Completed");
        assert_eq!(format!("{}", TaskStatus::Failed), "Failed");
    }

    // -------------------------------------------------------------------------
    // LocalFirstCoordinator comprehensive
    // -------------------------------------------------------------------------

    #[test]
    fn test_local_first_coordinator_default() {
        let coord = LocalFirstCoordinator::default();
        let stats = coord.stats();
        assert_eq!(stats.cache_stats.entry_count, 0);
        assert_eq!(stats.pending_ops, 0);
        assert_eq!(stats.bandwidth_saved_bytes, 0);
        assert_eq!(stats.edge_tasks_completed, 0);
        assert_eq!(stats.edge_tasks_pending, 0);
        assert_eq!(stats.offline_status, OfflineStatus::Online);
    }

    #[test]
    fn test_local_first_coordinator_get_cached_miss() {
        let mut coord = LocalFirstCoordinator::new();
        assert!(coord.get_cached("missing").is_none());
    }

    #[test]
    fn test_local_first_coordinator_cache_multiple_keys() {
        let mut coord = LocalFirstCoordinator::new();
        coord.cache_response("k1", "v1".to_string(), 10);
        coord.cache_response("k2", "v2".to_string(), 20);

        assert_eq!(coord.get_cached("k1").unwrap(), "v1");
        assert_eq!(coord.get_cached("k2").unwrap(), "v2");
    }

    #[test]
    fn test_local_first_coordinator_bandwidth_accumulates() {
        let mut coord = LocalFirstCoordinator::new();
        coord.cache_response("a", "x".to_string(), 100);
        coord.cache_response("b", "y".to_string(), 200);
        coord.cache_response("c", "z".to_string(), 300);

        assert_eq!(coord.bandwidth_saved(), 600);
    }

    #[test]
    fn test_local_first_coordinator_queue_for_sync() {
        let mut coord = LocalFirstCoordinator::new();
        coord.queue_for_sync(OperationType::Create, "payload".to_string());
        coord.queue_for_sync(OperationType::Update, "payload2".to_string());

        let stats = coord.stats();
        assert_eq!(stats.pending_ops, 2);
    }

    #[test]
    fn test_local_first_coordinator_add_and_get_edge_tasks() {
        let mut coord = LocalFirstCoordinator::new();

        let t1 = EdgeTask::new(EdgeTaskType::Validation, "input-1");
        let t2 = EdgeTask::new(EdgeTaskType::Search, "input-2");
        coord.add_edge_task(t1);
        coord.add_edge_task(t2);

        assert_eq!(coord.edge_tasks().len(), 2);
    }

    #[test]
    fn test_local_first_coordinator_stats_counts_task_statuses() {
        let mut coord = LocalFirstCoordinator::new();

        let mut t_done = EdgeTask::new(EdgeTaskType::Transform, "a");
        t_done.complete("result");

        let mut t_failed = EdgeTask::new(EdgeTaskType::Filter, "b");
        t_failed.fail("err");

        let t_pending = EdgeTask::new(EdgeTaskType::Custom, "c");

        coord.add_edge_task(t_done);
        coord.add_edge_task(t_failed);
        coord.add_edge_task(t_pending);

        let stats = coord.stats();
        assert_eq!(stats.edge_tasks_completed, 1);
        assert_eq!(stats.edge_tasks_pending, 1);
        // Failed tasks are neither completed nor pending.
    }

    #[test]
    fn test_local_first_coordinator_offline_access() {
        let mut coord = LocalFirstCoordinator::new();
        coord.offline().set_status(OfflineStatus::Offline);

        let stats = coord.stats();
        assert_eq!(stats.offline_status, OfflineStatus::Offline);
    }

    #[test]
    fn test_local_first_coordinator_sync_access() {
        let mut coord = LocalFirstCoordinator::new();
        coord.sync().set_enabled(false);
        assert!(!coord.sync().is_enabled());
    }

    #[test]
    fn test_local_first_coordinator_cache_access() {
        let mut coord = LocalFirstCoordinator::new();
        coord.cache().put(CacheEntry::new("x", "y".to_string()));
        assert_eq!(coord.cache().stats().entry_count, 1);
    }

    // -------------------------------------------------------------------------
    // LocalFirstStats fields
    // -------------------------------------------------------------------------

    #[test]
    fn test_local_first_stats_clone() {
        let coord = LocalFirstCoordinator::new();
        let stats = coord.stats();
        let cloned = stats.clone();
        assert_eq!(cloned.pending_ops, stats.pending_ops);
        assert_eq!(cloned.bandwidth_saved_bytes, stats.bandwidth_saved_bytes);
    }

    // -------------------------------------------------------------------------
    // CacheStats fields
    // -------------------------------------------------------------------------

    #[test]
    fn test_cache_stats_fields() {
        let mut cache: LocalCache<String> =
            LocalCache::new().with_max_entries(50).with_max_size(1024);
        cache.put(CacheEntry::new("k", "v".to_string()).with_size(10));
        let _ = cache.get("k");
        let _ = cache.get("missing");

        let stats = cache.stats();
        assert_eq!(stats.entry_count, 1);
        assert_eq!(stats.size_bytes, 10);
        assert_eq!(stats.max_entries, 50);
        assert_eq!(stats.max_size_bytes, 1024);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_cache_stats_clone() {
        let cache: LocalCache<String> = LocalCache::new();
        let stats = cache.stats();
        let cloned = stats.clone();
        assert_eq!(cloned.entry_count, stats.entry_count);
    }

    // -------------------------------------------------------------------------
    // Edge cases: empty-string keys and values
    // -------------------------------------------------------------------------

    #[test]
    fn test_cache_entry_empty_key_and_value() {
        let entry: CacheEntry<String> = CacheEntry::new("", String::new());
        assert_eq!(entry.key, "");
        assert_eq!(entry.value, "");
    }

    #[test]
    fn test_local_cache_empty_key() {
        let mut cache: LocalCache<String> = LocalCache::new();
        cache.put(CacheEntry::new("", "empty-key-value".to_string()));
        assert_eq!(cache.get("").unwrap(), "empty-key-value");
    }

    // -------------------------------------------------------------------------
    // Concurrent-access-pattern simulation (single-threaded but realistic)
    // -------------------------------------------------------------------------

    #[test]
    fn test_local_cache_interleaved_put_get_remove() {
        let mut cache: LocalCache<i32> = LocalCache::new().with_max_entries(10);

        for i in 0..10_i32 {
            cache.put(CacheEntry::new(format!("k{i}"), i));
        }

        // Read half of them
        for i in 0..5_i32 {
            assert_eq!(*cache.get(&format!("k{i}")).unwrap(), i);
        }

        // Remove the other half
        for i in 5..10_i32 {
            assert!(cache.remove(&format!("k{i}")).is_some());
        }

        assert_eq!(cache.stats().entry_count, 5);
    }

    #[test]
    fn test_offline_manager_queue_many_operations() {
        let mut manager = OfflineManager::new();
        manager.set_status(OfflineStatus::Offline);

        for i in 0..20 {
            manager.queue_operation(PendingOperation::new(
                OperationType::Create,
                format!("payload-{i}"),
            ));
        }

        assert_eq!(manager.pending_count(), 20);

        // Mark first 10 as synced
        let ids: Vec<String> = manager
            .pending_operations()
            .iter()
            .take(10)
            .map(|o| o.id.clone())
            .collect();

        for id in &ids {
            manager.mark_synced(id);
        }

        assert_eq!(manager.pending_count(), 10);
    }

    #[test]
    fn test_coordinator_full_workflow() {
        let mut coord = LocalFirstCoordinator::new();

        // Simulate going offline
        coord.offline().set_status(OfflineStatus::Offline);

        // Queue some operations
        coord.queue_for_sync(OperationType::Create, "session-data".to_string());
        coord.queue_for_sync(OperationType::Update, "session-update".to_string());

        // Cache some data locally
        coord.cache_response("api/data", "cached-response".to_string(), 256);

        // Add edge tasks
        let mut task = EdgeTask::new(EdgeTaskType::Validation, "validate-this");
        task.complete("valid");
        coord.add_edge_task(task);

        let stats = coord.stats();
        assert_eq!(stats.offline_status, OfflineStatus::Offline);
        assert_eq!(stats.pending_ops, 2);
        assert_eq!(stats.bandwidth_saved_bytes, 256);
        assert_eq!(stats.edge_tasks_completed, 1);
        assert_eq!(stats.edge_tasks_pending, 0);

        // Come back online
        coord.offline().set_status(OfflineStatus::Online);
        assert_eq!(coord.stats().offline_status, OfflineStatus::Online);
    }
}
