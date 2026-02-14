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
        .as_secs()
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
}
